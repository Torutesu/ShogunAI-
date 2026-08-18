//! Meeting detection (FR-MT-04): turn observations into an offer, with a confidence and the
//! evidence behind it.
//!
//! Three signals, none trusted alone:
//!
//! | signal | source | alone |
//! |---|---|---|
//! | ① an occurrence is scheduled now | `calendar_occurrences` | strong — but a scheduled meeting is not an attended one |
//! | ② a meeting app is frontmost / the mic is in use | NSWorkspace + bundle id table | medium |
//! | ③ meeting controls are on screen | AX sees Leave/Mute/participants | medium |
//!
//! **② or ③ opens the interval; ① only corroborates.** A calendar entry the user never joined
//! must not produce a session — "there was a meeting on the calendar" is not evidence of
//! attendance, and a product that starts listening because of a diary entry is one that listens
//! when nobody is there.
//!
//! The microphone signal reads *whether the device is in use* and nothing else. **No audio is
//! sampled here.** That boundary is the whole difference between detection and eavesdropping, so
//! it is stated in the type: [`Signals::mic_in_use`] is a `bool`, and this module never sees a
//! sample buffer.
//!
//! Bundle ids and hosts are tiered ([`MeetingHint`]): a **Strong** surface is meeting-only (the
//! Zoom app, a Meet page) and being frontmost is itself evidence; a **Weak** surface is a
//! resident app or portal (Teams, Webex) where frontmost usually means chat, so it only counts
//! as one corroborating vote and needs sustained mic use or another signal before an offer.

/// What the adapter observed at one detection tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Signals {
    /// ② A **Strong** meeting app is frontmost, or the browser is on a **Strong** meeting URL
    /// (see [`bundle_hint`] / [`host_hint`]). Weak surfaces (Teams, Webex, a huddle hint) must
    /// not set this — they reach the detector through [`DetectionCtx`] as single votes.
    pub meeting_app_frontmost: bool,
    /// ② The audio input device is in use. **Truth value only — no samples are read.**
    pub mic_in_use: bool,
    /// ③ Accessibility found meeting controls (Leave / Mute / a participant list).
    pub meeting_controls_visible: bool,
    /// ① A calendar occurrence covers this moment.
    pub occurrence_now: bool,
}

/// The outcome of a tick.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Nothing worth offering.
    Ignore,
    /// Offer to take notes, carrying the confidence and the evidence for it.
    Offer { confidence: f64, provenance: String },
}

/// How strongly a bundle id or host says "a meeting is happening" (FR-MT-04, Plan A).
///
/// `Strong` surfaces are meeting-only — the native Zoom app, a Meet page — so being frontmost is
/// itself evidence and can open an offer alone. `Weak` surfaces are resident chat apps and
/// portals — Teams, Webex — where frontmost usually means someone reading messages; a Weak match
/// is one corroborating vote and never an opener on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingHint {
    Strong,
    Weak,
}

/// Bundle ids of apps that exist to hold meetings (FR-MT-04). Frontmost is evidence by itself.
///
/// A table rather than a heuristic: guessing "is this a meeting app?" from a window title is how
/// a note-taking offer ends up appearing over someone's banking tab.
const STRONG_MEETING_BUNDLES: &[&str] = &["us.zoom.xos"];

/// Bundle ids of resident apps that *can* hold meetings but mostly hold chat (Plan A-2). Teams
/// and Webex stay open all day; frontmost alone must never raise the offer to listen.
const WEAK_MEETING_BUNDLES: &[&str] = &[
    "com.microsoft.teams2",
    "com.microsoft.teams",
    "com.microsoft.teams.work",
    "cisco-systems.spark",
];

/// Hosts that mean "a meeting is open in the browser" (FR-MT-04). Zoom is handled separately:
/// `zoom.us` hosts both meetings and marketing/account pages, so only meeting routes match.
const STRONG_MEETING_HOSTS: &[&str] = &["meet.google.com"];

/// Teams hosts where chat, calendars and meeting pages share the address. A host is not enough:
/// only known join/call routes become a Weak signal, then still need corroboration.
const TEAMS_HOSTS: &[&str] = &[
    "teams.microsoft.com",
    "teams.live.com",
    "teams.cloud.microsoft",
    "teams.microsoft.us",
    "dod.teams.microsoft.us",
];

/// Weak host *suffixes*: Webex parks every tenant on its own subdomain (`acme.webex.com`), and
/// the admin console shares them — suffix-matched like [`is_media_host`], Weak like Teams.
const WEAK_MEETING_HOST_SUFFIXES: &[&str] = &["webex.com"];

/// Slack's bundle id — huddles run inside it, so no bundle table can see them (Plan A-4); the
/// adapter asks [`huddle_hint`] instead when this app is frontmost.
pub const SLACK_BUNDLE_ID: &str = "com.tinyspeck.slackmacgap";

/// How strongly a frontmost bundle id says "meeting", if at all (FR-MT-04, Plan A-0).
pub fn bundle_hint(bundle_id: &str) -> Option<MeetingHint> {
    let bundle_id = normalize_identifier(bundle_id);
    if STRONG_MEETING_BUNDLES.contains(&bundle_id.as_str()) {
        Some(MeetingHint::Strong)
    } else if WEAK_MEETING_BUNDLES.contains(&bundle_id.as_str()) {
        Some(MeetingHint::Weak)
    } else {
        None
    }
}

/// A process/app name fallback when macOS does not expose a bundle id. It is deliberately Weak:
/// names are less authoritative than signed bundle ids, so even Zoom still needs the microphone
/// or another corroborating signal before an offer appears.
pub fn process_hint(process_name: &str) -> Option<MeetingHint> {
    match normalize_identifier(process_name).as_str() {
        "zoom" | "zoom workplace" | "microsoft teams" | "teams" => Some(MeetingHint::Weak),
        _ => None,
    }
}

/// A product-specific title fallback for AX URL gaps. Generic words such as "meeting" are
/// intentionally not signals: titles are user content, and a document called "meeting notes"
/// must never trigger an offer. Like [`process_hint`], titles are Weak evidence only.
pub fn title_hint(title: &str) -> Option<MeetingHint> {
    let title = normalize_identifier(title);
    if ["zoom meeting", "zoom workplace", "google meet", "microsoft teams", "teams meeting"]
        .iter()
        .any(|marker| title.contains(marker))
    {
        Some(MeetingHint::Weak)
    } else {
        None
    }
}

/// How strongly a browser URL says "meeting", if at all (FR-MT-04, Plan A-0).
///
/// Matches on the parsed **host** (via [`host_of`]), never on a substring of the URL — the same
/// protection as [`is_meeting_url`]: `app.zoom.us.evil.test` must not raise the offer to listen.
pub fn host_hint(url: &str) -> Option<MeetingHint> {
    let host = host_of(url)?;
    if STRONG_MEETING_HOSTS.iter().any(|h| host == *h) {
        return Some(MeetingHint::Strong);
    }
    if is_zoom_web_meeting_url(url, &host) {
        return Some(MeetingHint::Strong);
    }
    if is_teams_web_meeting_url(url, &host)
        || host_matches_suffix(&host, WEAK_MEETING_HOST_SUFFIXES)
    {
        return Some(MeetingHint::Weak);
    }
    None
}

/// Zoom Web meeting routes. Zoom serves marketing, account and support pages from the same
/// registrable domain, so host-only matching would cause false offers. `j/<id>` is a join link;
/// `wc/<id>` is the browser client. Regional Zoom hosts (`us02web.zoom.us`, etc.) are accepted
/// only for those routes.
fn is_zoom_web_meeting_url(url: &str, host: &str) -> bool {
    if !host_matches_suffix(host, &["zoom.us"]) {
        return false;
    }
    let Some(path) = path_of(url) else {
        return false;
    };
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    let Some(route) = segments.next() else {
        return false;
    };
    let Some(meeting_or_client) = segments.next() else {
        return false;
    };
    !meeting_or_client.is_empty()
        && (route.eq_ignore_ascii_case("j") || route.eq_ignore_ascii_case("wc"))
}

/// Teams Web has resident chat on the same domains. Match only known meeting-entry routes
/// instead of treating a Teams channel, calendar or admin page as a meeting. These remain Weak
/// evidence because a join page can be open before the user has joined.
fn is_teams_web_meeting_url(url: &str, host: &str) -> bool {
    if !TEAMS_HOSTS.iter().any(|known| host == *known) {
        return false;
    }
    let Some(path) = path_of(url) else {
        return false;
    };
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    match (segments.next(), segments.next()) {
        (Some(first), Some(second))
            if first.eq_ignore_ascii_case("l") && second.eq_ignore_ascii_case("meetup-join") =>
        {
            true
        }
        (Some(first), Some(second)) if first.eq_ignore_ascii_case("meet") && !second.is_empty() => {
            true
        }
        _ => false,
    }
}

/// Exact-or-subdomain host match: `acme.webex.com` matches the suffix `webex.com`;
/// `evilwebex.com` does not. Shared by the media and Webex tables.
fn host_matches_suffix(host: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

fn normalize_identifier(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

/// Whether the bundle id is any known meeting app, Strong or Weak. Thin compatibility wrapper
/// over [`bundle_hint`]; policy code should ask for the tier instead.
pub fn is_meeting_app(bundle_id: &str) -> bool {
    bundle_hint(bundle_id).is_some()
}

/// Host suffixes for passive media playback — never meeting context on their own.
///
/// A sustained microphone on a YouTube tab is usually speaker bleed into an open input device
/// or a PiP window whose URL AX cannot read, not attendance. Mic-only offers are suppressed
/// here; a real Meet/Zoom URL or native meeting app still opens normally.
const MEDIA_HOST_SUFFIXES: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "netflix.com",
    "twitch.tv",
    "spotify.com",
    "vimeo.com",
    "soundcloud.com",
    "music.apple.com",
];

/// Whether a browser URL is any known meeting page, Strong or Weak (FR-MT-04).
///
/// Matches on the parsed **host**, never on a substring of the URL: `meet.google.com.evil.test`
/// and `?redirect=meet.google.com` both contain the host as text, and a `contains` check here
/// would let an arbitrary page raise the offer to listen. Thin compatibility wrapper over
/// [`host_hint`]; policy code should ask for the tier instead.
pub fn is_meeting_url(url: &str) -> bool {
    host_hint(url).is_some()
}

/// Whether a browser URL is known passive media (YouTube, streaming, etc.).
///
/// These pages must not corroborate mic-only detection. [`is_meeting_url`] wins when both could
/// apply — a Meet link is never classified as media.
pub fn is_media_url(url: &str) -> bool {
    let Some(host) = host_of(url) else { return false };
    if is_meeting_url(url) {
        return false;
    }
    is_media_host(&host)
}

fn is_media_host(host: &str) -> bool {
    host_matches_suffix(host, MEDIA_HOST_SUFFIXES)
}

/// Window titles that mean passive media or PiP — not a meeting the user is attending.
///
/// AX often cannot read the URL inside a PiP window, so the title is the only cheap signal.
const SUPPRESSED_TITLE_PATTERNS: &[&str] = &[
    "picture-in-picture",
    "picture in picture",
    "youtube",
    "netflix",
    "twitch",
    "spotify",
    "vimeo",
    "hulu",
    "disney+",
    "prime video",
    "soundcloud",
];

/// Whether a focused window title should block an offer (PiP, streaming tabs, etc.).
pub fn is_suppressed_title(title: &str) -> bool {
    let lower = title.to_ascii_lowercase();
    SUPPRESSED_TITLE_PATTERNS.iter().any(|p| lower.contains(p))
}

/// Adapter-side facts the pure detector needs to apply product policy (FR-MT-04).
#[derive(Debug, Clone, Copy, Default)]
pub struct DetectionCtx<'a> {
    /// Frontmost app is a browser (Chrome, Safari, Arc, …).
    pub is_browser: bool,
    /// Parsed host of the frontmost browser tab, when a URL was read.
    pub page_host: Option<&'a str>,
    /// A **Strong** meeting URL is open (`meet.google.com`, `app.zoom.us`).
    pub has_meet_url: bool,
    /// A **Strong** meeting app (native Zoom) is frontmost.
    pub has_strong_bundle: bool,
    /// A **Weak** meeting surface is frontmost — a Teams/Webex bundle or a Weak host (Plan A-2).
    /// One corroborating vote, never an opener: the adapter must *not* also fold this into
    /// [`Signals::meeting_app_frontmost`], or one observation would count twice.
    pub has_weak_meeting_signal: bool,
    /// Slack's window/AX text looks like a huddle in progress (see [`huddle_hint`], Plan A-4).
    /// One corroborating vote, exactly like a Weak bundle — alone it does nothing, and with
    /// sustained mic use it produces an offer, never an auto-start.
    pub has_huddle_hint: bool,
    /// Focused window title, when AX returned one.
    pub window_title: Option<&'a str>,
}

/// User settings that gate how aggressively mic-only evidence may open an offer.
#[derive(Debug, Clone, Copy, Default)]
pub struct OfferPolicy {
    /// When `false`, sustained mic use alone never opens an interval.
    pub allow_mic_only: bool,
}

/// Count how many independent signals fired (calendar included when present). A Weak meeting
/// surface and a huddle hint are each **one** vote (Plan A-0/A-4): they can corroborate a mic or
/// controls signal into an offer, but two votes are needed and neither is ever an opener.
fn corroborating_count(signals: &Signals, ctx: &DetectionCtx<'_>) -> usize {
    usize::from(signals.mic_in_use)
        + usize::from(signals.meeting_app_frontmost)
        + usize::from(signals.meeting_controls_visible)
        + usize::from(signals.occurrence_now)
        + usize::from(ctx.has_weak_meeting_signal)
        + usize::from(ctx.has_huddle_hint)
}

/// Whether the observation already proves a meeting surface that exists only for meetings
/// (a Strong URL or the native Zoom app). **Strong only** — a Weak surface or a huddle hint
/// must corroborate, never open (Plan A-0).
fn has_strong_opener(ctx: &DetectionCtx<'_>) -> bool {
    ctx.has_meet_url || ctx.has_strong_bundle
}

/// Browser tab with no readable host — cannot prove Meet is open (PiP, AX gaps).
fn browser_lacks_meeting_proof(ctx: &DetectionCtx<'_>) -> bool {
    ctx.is_browser && !ctx.has_meet_url && ctx.page_host.map_or(true, str::is_empty)
}

/// Apply FR-MT-04 policy on top of raw signals, then score.
///
/// Biases toward fewer false positives: mic-only is opt-in, PiP/media titles are dropped,
/// browsers with an empty host cannot corroborate, and non-URL offers need two agreeing signals
/// unless Meet or Zoom is already proven. A Weak surface or huddle hint (Plan A) is one of those
/// two signals — Teams frontmost plus sustained mic offers; Teams frontmost alone never does.
pub fn evaluate_offer(
    signals: &Signals,
    ctx: &DetectionCtx<'_>,
    policy: &OfferPolicy,
) -> Decision {
    if ctx.window_title.is_some_and(is_suppressed_title) {
        return Decision::Ignore;
    }

    let mut effective = *signals;

    if browser_lacks_meeting_proof(ctx) {
        effective.mic_in_use = false;
        effective.meeting_app_frontmost = false;
    }

    // A Weak surface or a huddle hint corroborates the mic, so the mic is no longer "only" —
    // without this the mic-only opt-in gate would zero the very signal Plan A pairs with a Weak
    // vote, and Teams-plus-sustained-mic could never offer under the default policy.
    let mic_is_only_opener = effective.mic_in_use
        && !effective.meeting_app_frontmost
        && !effective.meeting_controls_visible
        && !ctx.has_weak_meeting_signal
        && !ctx.has_huddle_hint;
    if mic_is_only_opener && !policy.allow_mic_only {
        effective.mic_in_use = false;
    }

    let d = decide(&effective);
    let Decision::Offer { confidence, provenance } = d else {
        return d;
    };

    if !has_strong_opener(ctx) && corroborating_count(&effective, ctx) < 2 {
        let mic_only_allowed = policy.allow_mic_only
            && effective.mic_in_use
            && !effective.meeting_app_frontmost
            && !effective.meeting_controls_visible;
        if !mic_only_allowed {
            return Decision::Ignore;
        }
    }

    let mut ctx_evidence: Vec<&str> = Vec::new();
    if ctx.has_weak_meeting_signal {
        ctx_evidence.push("weak_meeting_signal");
    }
    if ctx.has_huddle_hint {
        ctx_evidence.push("huddle_hint");
    }
    let provenance = if ctx_evidence.is_empty() {
        provenance
    } else {
        with_ctx_evidence(&provenance, &ctx_evidence)
    };

    Decision::Offer { confidence, provenance }
}

/// Record ctx-side evidence (Weak surface, huddle hint) in the provenance [`decide`] built, so a
/// wrong Weak offer is explainable from the stored interval like any other (FR-MT-04).
fn with_ctx_evidence(provenance: &str, extra: &[&str]) -> String {
    let mut map = serde_json::from_str::<serde_json::Value>(provenance)
        .ok()
        .and_then(|v| match v {
            serde_json::Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default();
    let signals = map.entry("signals").or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if let serde_json::Value::Array(fired) = signals {
        for name in extra {
            fired.push(serde_json::Value::String((*name).to_string()));
        }
    }
    serde_json::Value::Object(map).to_string()
}

/// The host component of an absolute URL, lowercased and without userinfo or port.
pub fn host_from_url(url: &str) -> Option<String> {
    host_of(url)
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.trim().split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    // `user@host` — the host is what follows the last '@', so `evil.test@meet.google.com` cannot
    // masquerade as the host by sitting in the userinfo.
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    let host = host.trim_end_matches('.');
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// Path component without a query/fragment. URLs without a path return `None` so a bare Zoom or
/// Teams host cannot accidentally become a meeting signal.
fn path_of(url: &str) -> Option<&str> {
    let rest = url.trim().split_once("://")?.1;
    let (_, path_and_more) = rest.split_once('/')?;
    Some(path_and_more.split(['?', '#']).next().unwrap_or_default())
}

/// How much each signal contributes. ② and ③ are the ones that can open an interval; ① only
/// corroborates, so it carries weight but cannot reach the threshold on its own.
const W_MIC: f64 = 0.40;
const W_APP: f64 = 0.30;
const W_CONTROLS: f64 = 0.15;
const W_OCCURRENCE: f64 = 0.10;

/// Combine the signals of one tick into a decision.
///
/// The opener is **sustained microphone use** (`mic_in_use` here means [`MicWatch`] has been
/// answering yes, not that the device opened this instant). That is what "a meeting is
/// happening" actually means: a URL is the same in the lobby, in the call and after everyone has
/// left, and a bundle-id table only knows the apps someone remembered to list. A meeting app or
/// a meeting page in front corroborates and raises the confidence — and either can still open an
/// interval on its own, so a call on a machine whose microphone is not the default input is not
/// invisible.
pub fn decide(signals: &Signals) -> Decision {
    let observed =
        signals.mic_in_use || signals.meeting_app_frontmost || signals.meeting_controls_visible;
    if !observed {
        return Decision::Ignore;
    }

    let mut confidence = 0.0;
    let mut fired: Vec<&str> = Vec::new();
    for (on, weight, name) in [
        (signals.mic_in_use, W_MIC, "mic_sustained"),
        (signals.meeting_app_frontmost, W_APP, "meeting_app_frontmost"),
        (signals.meeting_controls_visible, W_CONTROLS, "meeting_controls_visible"),
        (signals.occurrence_now, W_OCCURRENCE, "occurrence_now"),
    ] {
        if on {
            confidence += weight;
            fired.push(name);
        }
    }

    // The weights sum to 0.95: even total agreement stays short of certainty, because detection
    // is inference and the only honest promotion to 1.0 is the user confirming (FR-MT-17).
    Decision::Offer {
        confidence,
        provenance: serde_json::json!({ "signals": fired }).to_string(),
    }
}


/// How long the microphone must stay in use before it counts when something else already says
/// "meeting" (a Meet URL, Zoom, calendar corroboration). Ten seconds separates "hey Siri" and a
/// voice memo from a call without making the offer feel late.
pub const MIC_SUSTAIN_MS: i64 = 10_000;

/// Mic-only is the weakest opener (FR-MT-04: ② alone is medium, not certain). When nothing else
/// corroborates, wait longer so speaker bleed on a media tab or a stray open input does not
/// surface the offer.
pub const MIC_ONLY_SUSTAIN_MS: i64 = 30_000;

/// Whether sustained microphone use should count as signal ② this tick.
///
/// Suppressed on known media pages unless [`meeting_context`] is already true (Meet URL / Zoom).
/// Shorter sustain when meeting context is present; longer when mic is the only evidence.
pub fn mic_counts_as_signal(sustained_ms: i64, meeting_context: bool, on_media_page: bool) -> bool {
    if sustained_ms == 0 {
        return false;
    }
    if on_media_page && !meeting_context {
        return false;
    }
    let threshold = if meeting_context { MIC_SUSTAIN_MS } else { MIC_ONLY_SUSTAIN_MS };
    sustained_ms >= threshold
}

/// SHOGUN's own bundle id. Its ASR holds the input during a meeting it is already noting, so a
/// holder-attributed signal must never count our own capture as evidence of a *new* meeting.
const SELF_BUNDLE_ID: &str = "com.selectkk.shogun";

/// How many distinct non-meeting apps may be frontmost during one unbroken stretch of microphone
/// use before the coarse signal is written off as stuck.
///
/// Three, because a real call tolerates tabbing away — to notes, to a browser, to the thing being
/// discussed — but a meeting coming into view clears the tally. A signal still "in use" after the
/// user has moved through three unrelated apps with no meeting in sight is not describing this
/// user's meeting; it is describing some daemon holding the device.
pub const MIC_STUCK_DISTINCT_APPS: usize = 3;

/// How long a stretch must have run before the app tally is allowed to condemn it.
///
/// Without a floor, joining a call and immediately opening the agenda, the calendar and a
/// scratchpad would look identical to a stuck daemon. Two minutes costs a genuinely stuck signal
/// almost nothing (it has been true since login) and protects the opening minutes of a real call,
/// which is exactly when people fetch the things they need.
pub const MIC_STUCK_MIN_MS: i64 = 2 * 60 * 1_000;

/// How the platform reported microphone use — and therefore how much the report is worth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicSource<'a> {
    /// The input is attributed to a specific process (macOS 14.4+ CoreAudio process objects).
    /// This is the signal we want: it can answer *who* is talking through the machine.
    Holder { bundle_id: &'a str },
    /// Only "some process on this machine is using an input device" is known (older macOS, or
    /// attribution unavailable). Correct but coarse: an always-on utility that never releases the
    /// microphone makes this true forever, which is what [`MicWatch`]'s stuck check exists for.
    SystemWide,
}

/// One microphone observation, with the context needed to judge whether it means anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MicObservation<'a> {
    /// Whether an input device is running. **Truth value only — no samples are read.**
    pub in_use: bool,
    /// How that truth value was obtained.
    pub source: MicSource<'a>,
    /// The frontmost app at this tick.
    pub frontmost_bundle_id: &'a str,
    /// Whether the frontmost context looks like a meeting (known app, or a meeting URL).
    pub meeting_context: bool,
}

/// Turns "the microphone is open right now" into "a call is happening".
///
/// The single most useful meeting signal is not which page is open — a Meet URL is the same in
/// the lobby, in the call and after everyone has left — it is whether anyone is actually
/// talking through the machine. That is app-agnostic: it catches a call in an app nobody thought
/// to add to a bundle-id table.
///
/// It reads *whether the device is in use* and nothing else. No audio is sampled (FR-MT-04).
#[derive(Debug, Clone, Copy, Default)]
pub struct MicWatch {
    since_ms: Option<i64>,
    /// Distinct non-meeting apps seen in this stretch (hashed, so the watch stays `Copy`).
    seen: [u64; MIC_STUCK_DISTINCT_APPS],
    seen_len: usize,
    /// Set once the coarse signal is written off for this stretch; cleared only by a release or
    /// by a meeting coming into view.
    stuck: bool,
}

/// FNV-1a. Only ever compared against other hashes in the same process, so stability across
/// releases is irrelevant — determinism within a run (and in tests) is what matters.
fn hash_bundle_id(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

impl MicWatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the coarse signal has been written off for the current stretch (diagnostics).
    pub fn is_stuck(&self) -> bool {
        self.stuck
    }

    /// Feed one observation. Returns whether the microphone has been continuously in use for
    /// at least [`MIC_SUSTAIN_MS`] **and** that use is attributable to what the user is doing.
    pub fn observe(&mut self, obs: &MicObservation<'_>, now: i64) -> bool {
        if !obs.in_use {
            *self = Self::new();
            return false;
        }

        match obs.source {
            MicSource::Holder { bundle_id } => {
                // Our own ASR holding the input during a meeting we are already noting is not
                // evidence that a meeting is starting.
                let ours = bundle_id == SELF_BUNDLE_ID;
                let relevant =
                    !ours && (is_meeting_app(bundle_id) || bundle_id == obs.frontmost_bundle_id);
                if !relevant {
                    // Someone else's background use. Not a signal, and not a stretch either —
                    // the clock restarts if the relevant holder starts later.
                    self.since_ms = None;
                    return false;
                }
            }
            MicSource::SystemWide => {
                if obs.meeting_context {
                    // A meeting is in view, so the open device now has an explanation. Forget the
                    // tally *and* the verdict: "stuck" means "no explanation was ever offered",
                    // and one has been. Leaving the meeting re-accumulates it, so a daemon that
                    // really is holding the device is condemned again on the way out.
                    self.seen_len = 0;
                    self.stuck = false;
                } else {
                    self.note_unrelated_app(obs.frontmost_bundle_id, now);
                }
                if self.stuck {
                    return false;
                }
            }
        }

        let since = *self.since_ms.get_or_insert(now);
        // `saturating_sub` so a clock that jumps backwards restarts the wait instead of
        // reporting a meeting that has been running for negative time.
        now.saturating_sub(since) >= MIC_SUSTAIN_MS
    }

    /// Record a distinct non-meeting app, condemning the stretch once the signal has outlived
    /// [`MIC_STUCK_DISTINCT_APPS`] of them *and* [`MIC_STUCK_MIN_MS`].
    fn note_unrelated_app(&mut self, bundle_id: &str, now: i64) {
        let h = hash_bundle_id(bundle_id);
        if !self.seen[..self.seen_len].contains(&h) && self.seen_len < MIC_STUCK_DISTINCT_APPS {
            self.seen[self.seen_len] = h;
            self.seen_len += 1;
        }
        let long_enough =
            self.since_ms.is_some_and(|since| now.saturating_sub(since) >= MIC_STUCK_MIN_MS);
        if self.seen_len >= MIC_STUCK_DISTINCT_APPS && long_enough {
            self.stuck = true;
        }
    }

    /// Continuous in-use duration in milliseconds, or zero when the mic is closed.
    pub fn sustained_ms(&self, now: i64) -> i64 {
        self.since_ms.map(|since| now.saturating_sub(since)).unwrap_or(0)
    }
}

/// What the adapter observes about a meeting that is already running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveSignals {
    /// The meeting app is still frontmost or still running with its window present.
    pub meeting_app_present: bool,
    /// End of the linked calendar occurrence (epoch ms), when there is one.
    pub occurrence_ends_at: Option<i64>,
    /// When audio was last heard above the silence floor (epoch ms).
    pub last_sound_at: i64,
}

/// FR-MT-11: how long past an occurrence's end a meeting is allowed to run on.
pub const OCCURRENCE_GRACE_MS: i64 = 10 * 60 * 1_000;
/// FR-MT-11: silence that ends a meeting.
pub const SILENCE_LIMIT_MS: i64 = 15 * 60 * 1_000;
/// FR-MT-11: after the meeting page leaves the frontmost browser tab, wait this long before
/// wrapping. Covers a quick alt-tab or lobby flicker without keeping the pill alive for hours
/// because Chrome is still open on Gmail.
pub const MEETING_URL_LEFT_GRACE_MS: i64 = 20_000;
/// After the meeting page is gone past grace, the mic must stay closed this long before the
/// session ends. Separates "tab switched away but call still running" (mic open) from hang-up
/// flicker at the device layer.
pub const MIC_QUIET_AFTER_URL_LEFT_MS: i64 = 8_000;

/// Whether the frontmost browser tab still looks like an active meeting (FR-MT-11).
///
/// Used while a session opened on a Meet URL is recording: quitting Chrome is handled by
/// [`LiveSignals::meeting_app_present`]; navigating to mail or closing the Meet tab is this.
pub fn browser_meeting_page_present(page_url: Option<&str>, window_title: Option<&str>) -> bool {
    if page_url.is_some_and(is_meeting_url) {
        return true;
    }
    if let Some(url) = page_url {
        if is_media_url(url) {
            return false;
        }
        if host_of(url).is_some_and(|h| !h.is_empty()) {
            return false;
        }
    }
    if let Some(title) = window_title {
        if title_hint(title).is_some() {
            return true;
        }
        // PiP / AX gaps: an unreadable URL with a media title is still in-call, not "left".
        if is_suppressed_title(title) {
            return true;
        }
    }
    false
}

/// Whether a URL-tracked session has been off the meeting page long enough to wrap (FR-MT-11).
pub fn meeting_url_left_past_grace(lost_since_ms: Option<i64>, now: i64) -> bool {
    lost_since_ms.is_some_and(|since| now.saturating_sub(since) >= MEETING_URL_LEFT_GRACE_MS)
}

/// The word that names a Slack huddle, in the two languages Slack's UI shows it in.
const HUDDLE_WORDS: &[&str] = &["huddle", "ハドル"];

/// Call-control vocabulary that appears in a huddle's UI but not in chat about huddles.
const HUDDLE_CONTROL_VOCAB: &[&str] =
    &["mute", "unmute", "leave", "share screen", "ミュート", "退出", "画面共有"];

fn mentions_huddle(text: &str) -> bool {
    let lower = text.to_lowercase();
    HUDDLE_WORDS.iter().any(|w| lower.contains(w))
}

fn mentions_call_controls(text: &str) -> bool {
    let lower = text.to_lowercase();
    HUDDLE_CONTROL_VOCAB.iter().any(|w| lower.contains(w))
}

/// Whether Slack's window title / AX text looks like a huddle in progress (Plan A-4).
///
/// Slack ships huddles inside the ordinary Slack window under the ordinary bundle id
/// ([`SLACK_BUNDLE_ID`]), so no bundle table can see them. The hint instead reads what
/// Accessibility already exposes: "Huddle" (or the Japanese UI's "ハドル") **co-occurring with
/// call-control vocabulary** (mute / leave / 退出 …). Co-occurrence is the point — "huddle" alone
/// is ordinary chat ("let's huddle tomorrow"), and a hint that fired on message text would put
/// the offer over every conversation that mentions the word.
///
/// Policy lives in [`evaluate_offer`]: the hint alone does nothing; with sustained mic use
/// (≥ [`MIC_SUSTAIN_MS`]) it is a Weak offer, never an auto-start.
pub fn huddle_hint(window_title: Option<&str>, ax_snippets: &[&str]) -> bool {
    let huddle_seen = window_title.is_some_and(mentions_huddle)
        || ax_snippets.iter().any(|s| mentions_huddle(s));
    if !huddle_seen {
        return false;
    }
    window_title.is_some_and(mentions_call_controls)
        || ax_snippets.iter().any(|s| mentions_call_controls(s))
}

/// Plan A-4: after the huddle hint disappears from the title/AX text, wait this long before
/// treating the huddle as left. Same shape and length as [`MEETING_URL_LEFT_GRACE_MS`] — a
/// redraw or a moment on another channel must not wrap a huddle mid-sentence.
pub const HUDDLE_HINT_LOST_GRACE_MS: i64 = 20_000;

/// Whether a huddle session has been without its hint long enough to wrap (Plan A-4).
/// Mirrors [`meeting_url_left_past_grace`].
pub fn huddle_hint_lost_past_grace(lost_since_ms: Option<i64>, now: i64) -> bool {
    lost_since_ms.is_some_and(|since| now.saturating_sub(since) >= HUDDLE_HINT_LOST_GRACE_MS)
}

/// Whether a huddle session should still count as present (Plan A-4). Mirrors
/// [`meet_url_session_present`]: past the hint-loss grace an open microphone still means the call
/// is running (the user alt-tabbed away, and Slack's huddle UI is only visible while Slack is
/// frontmost); once the mic closes, wait [`MIC_QUIET_AFTER_URL_LEFT_MS`] so a hang-up flicker does
/// not end the session mid-word. The silence limit in [`end_condition`] still applies
/// independently, so the huddle ends on whichever fires first.
pub fn huddle_session_present(
    lost_since_ms: Option<i64>,
    now: i64,
    mic_open: bool,
    mic_closed_since_ms: Option<i64>,
) -> bool {
    if !huddle_hint_lost_past_grace(lost_since_ms, now) {
        return true;
    }
    if mic_open {
        return true;
    }
    mic_closed_since_ms
        .map_or(true, |since| now.saturating_sub(since) < MIC_QUIET_AFTER_URL_LEFT_MS)
}

/// Whether a browser Meet session should still count as present (FR-MT-11).
///
/// Past the URL-leave grace the frontmost tab no longer looks like a meeting, but an open
/// microphone almost always means the call is still running on a background tab — the user may be
/// on another Chrome tab, reading mail, or looking at the SHOGUN overlay. Recording continues
/// while the mic is open; once it closes, wait [`MIC_QUIET_AFTER_URL_LEFT_MS`] so a hang-up
/// flicker does not end the session mid-word.
pub fn meet_url_session_present(
    lost_since_ms: Option<i64>,
    now: i64,
    mic_open: bool,
    mic_closed_since_ms: Option<i64>,
) -> bool {
    if !meeting_url_left_past_grace(lost_since_ms, now) {
        return true;
    }
    if mic_open {
        return true;
    }
    mic_closed_since_ms
        .map_or(true, |since| now.saturating_sub(since) < MIC_QUIET_AFTER_URL_LEFT_MS)
}

/// Whether the user has clearly left the call (FR-MT-11). Used to shorten Recap auto-dismiss.
pub fn call_clearly_ended(
    opened_via_meet_url: bool,
    url_lost_since_ms: Option<i64>,
    now: i64,
    mic_open: bool,
    mic_closed_since_ms: Option<i64>,
    zoom_bundle_id: Option<&str>,
    zoom_running: bool,
) -> bool {
    if opened_via_meet_url {
        meeting_url_left_past_grace(url_lost_since_ms, now)
            && !meet_url_session_present(url_lost_since_ms, now, mic_open, mic_closed_since_ms)
    } else if zoom_bundle_id.is_some_and(|b| bundle_hint(b) == Some(MeetingHint::Strong)) {
        // Strong only: quitting Zoom is leaving the call. A Weak bundle (Teams, Webex) stays
        // resident after the call, so its presence proves nothing — fall through to the mic.
        !zoom_running
    } else {
        !mic_open
            && mic_closed_since_ms.is_some_and(|since| {
                now.saturating_sub(since) >= MIC_QUIET_AFTER_URL_LEFT_MS
            })
    }
}

/// Whether a running meeting should end, and why (FR-MT-11).
///
/// This exists so that "it kept recording for six hours because I forgot" cannot happen: the
/// meeting ends on its own from three independent directions, and none of them requires the user
/// to remember anything.
pub fn end_condition(s: &LiveSignals, now: i64) -> Option<super::statemachine::EndReason> {
    use super::statemachine::EndReason;

    // Ordered by how directly each one says "the meeting is over". The app being gone is an
    // observation; silence and an expired slot are inferences from absence, and a meeting the
    // user quit should not be recorded as having died of silence.
    if !s.meeting_app_present {
        return Some(EndReason::AppGone);
    }
    if let Some(ends_at) = s.occurrence_ends_at {
        if now - ends_at > OCCURRENCE_GRACE_MS {
            return Some(EndReason::OccurrenceOver);
        }
    }
    if now - s.last_sound_at > SILENCE_LIMIT_MS {
        return Some(EndReason::Silence);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeting::statemachine::EndReason;

    fn live(now: i64) -> LiveSignals {
        LiveSignals { meeting_app_present: true, occurrence_ends_at: None, last_sound_at: now }
    }

    #[test]
    fn a_meeting_in_progress_keeps_running() {
        let now = 1_000_000;
        assert_eq!(end_condition(&live(now), now), None);
    }

    #[test]
    fn the_meeting_app_disappearing_ends_the_meeting() {
        let now = 1_000_000;
        let s = LiveSignals { meeting_app_present: false, ..live(now) };
        assert_eq!(end_condition(&s, now), Some(EndReason::AppGone));
    }

    #[test]
    fn silence_past_the_limit_ends_the_meeting() {
        let now = 1_000_000;
        let s = LiveSignals { last_sound_at: now - SILENCE_LIMIT_MS - 1, ..live(now) };
        assert_eq!(end_condition(&s, now), Some(EndReason::Silence));
    }

    #[test]
    fn a_quiet_stretch_short_of_the_limit_does_not_end_the_meeting() {
        // Someone listening to a long presentation is not a meeting that has finished.
        let now = 1_000_000;
        let s = LiveSignals { last_sound_at: now - SILENCE_LIMIT_MS + 1, ..live(now) };
        assert_eq!(end_condition(&s, now), None);
    }

    #[test]
    fn a_meeting_running_well_past_its_slot_ends() {
        let now = 1_000_000;
        let s = LiveSignals {
            occurrence_ends_at: Some(now - OCCURRENCE_GRACE_MS - 1),
            ..live(now)
        };
        assert_eq!(end_condition(&s, now), Some(EndReason::OccurrenceOver));
    }

    #[test]
    fn a_meeting_that_merely_runs_over_is_left_alone() {
        // Meetings overrun. Cutting the notes off at the scheduled end would lose exactly the
        // part people stay behind for, so the grace is generous (10 minutes).
        let now = 1_000_000;
        let s = LiveSignals { occurrence_ends_at: Some(now - 60_000), ..live(now) };
        assert_eq!(end_condition(&s, now), None);
    }

    #[test]
    fn the_app_going_away_wins_over_a_slower_condition() {
        // Both true at once: report the one that actually happened first, so Recap and the health
        // metrics do not learn "silence" for a meeting the user simply quit.
        let now = 1_000_000;
        let s = LiveSignals {
            meeting_app_present: false,
            last_sound_at: now - SILENCE_LIMIT_MS - 1,
            occurrence_ends_at: Some(now - OCCURRENCE_GRACE_MS - 1),
        };
        assert_eq!(end_condition(&s, now), Some(EndReason::AppGone));
    }

    fn confidence_of(d: &Decision) -> f64 {
        match d {
            Decision::Offer { confidence, .. } => *confidence,
            Decision::Ignore => panic!("expected an offer, got Ignore"),
        }
    }

    #[test]
    fn a_calendar_entry_alone_does_not_open_an_interval() {
        // The rule that keeps SHOGUN from listening to an empty room: a diary entry is not
        // attendance (FR-MT-04).
        let d = decide(&Signals { occurrence_now: true, ..Default::default() });
        assert_eq!(d, Decision::Ignore);
    }

    #[test]
    fn nothing_observed_is_ignored() {
        assert_eq!(decide(&Signals::default()), Decision::Ignore);
    }

    #[test]
    fn a_frontmost_meeting_app_is_enough_to_offer() {
        let d = decide(&Signals { meeting_app_frontmost: true, ..Default::default() });
        assert!(matches!(d, Decision::Offer { .. }));
    }

    #[test]
    fn visible_meeting_controls_are_enough_to_offer() {
        let d = decide(&Signals { meeting_controls_visible: true, ..Default::default() });
        assert!(matches!(d, Decision::Offer { .. }));
    }

    #[test]
    fn a_scheduled_occurrence_raises_confidence_in_what_was_observed() {
        let observed = Signals { meeting_app_frontmost: true, ..Default::default() };
        let corroborated = Signals { occurrence_now: true, ..observed };

        assert!(
            confidence_of(&decide(&corroborated)) > confidence_of(&decide(&observed)),
            "signal (1) must corroborate (2)/(3), even though it cannot stand alone"
        );
    }

    #[test]
    fn more_agreeing_signals_mean_more_confidence() {
        let one = decide(&Signals { meeting_app_frontmost: true, ..Default::default() });
        let two = decide(&Signals {
            meeting_app_frontmost: true,
            meeting_controls_visible: true,
            ..Default::default()
        });
        assert!(confidence_of(&two) > confidence_of(&one));
    }

    #[test]
    fn confidence_never_reaches_certainty() {
        // Detection is inference. Even with everything agreeing it stays below 1.0, so nothing
        // downstream can treat a detected meeting as a fact (FR-MT-04, FR-ST-02).
        let all = decide(&Signals {
            meeting_app_frontmost: true,
            mic_in_use: true,
            meeting_controls_visible: true,
            occurrence_now: true,
        });
        let c = confidence_of(&all);
        assert!(c < 1.0, "confidence was {c}");
        assert!(c > 0.0);
    }

    #[test]
    fn the_offer_records_which_signals_fired() {
        // Provenance is what makes a wrong detection explainable rather than merely annoying.
        let d = decide(&Signals {
            meeting_app_frontmost: true,
            mic_in_use: true,
            ..Default::default()
        });
        let Decision::Offer { provenance, .. } = d else { panic!("expected an offer") };

        assert!(provenance.contains("meeting_app_frontmost"));
        assert!(provenance.contains("mic_sustained"));
        assert!(!provenance.contains("occurrence_now"), "a signal that did not fire is not evidence");
        serde_json::from_str::<serde_json::Value>(&provenance).expect("provenance must be JSON");
    }

    #[test]
    fn a_sustained_microphone_is_a_meeting_on_its_own() {
        // `decide` still scores mic-only; product policy gates it unless opted in.
        let d = decide(&Signals { mic_in_use: true, ..Default::default() });
        assert!(matches!(d, Decision::Offer { .. }));
    }

    #[test]
    fn mic_only_is_blocked_by_default_policy() {
        let signals = Signals { mic_in_use: true, ..Default::default() };
        let ctx = DetectionCtx::default();
        let policy = OfferPolicy::default();
        assert_eq!(evaluate_offer(&signals, &ctx, &policy), Decision::Ignore);
    }

    #[test]
    fn mic_only_offers_when_opted_in() {
        let signals = Signals { mic_in_use: true, ..Default::default() };
        let ctx = DetectionCtx::default();
        let policy = OfferPolicy { allow_mic_only: true };
        assert!(matches!(evaluate_offer(&signals, &ctx, &policy), Decision::Offer { .. }));
    }

    #[test]
    fn youtube_url_is_not_a_meeting_and_mic_only_blocked() {
        assert!(!is_meeting_url("https://www.youtube.com/watch?v=abc"));
        assert!(is_media_url("https://www.youtube.com/watch?v=abc"));
        let signals = Signals {
            mic_in_use: mic_counts_as_signal(MIC_ONLY_SUSTAIN_MS, false, true),
            ..Default::default()
        };
        let ctx = DetectionCtx {
            is_browser: true,
            page_host: Some("www.youtube.com"),
            window_title: Some("Rick Astley - YouTube"),
            ..Default::default()
        };
        assert_eq!(evaluate_offer(&signals, &ctx, &OfferPolicy::default()), Decision::Ignore);
    }

    #[test]
    fn empty_host_chrome_with_mic_does_not_offer() {
        let signals = Signals {
            mic_in_use: mic_counts_as_signal(MIC_ONLY_SUSTAIN_MS, false, false),
            ..Default::default()
        };
        let ctx = DetectionCtx {
            is_browser: true,
            page_host: Some(""),
            window_title: Some("New tab - Google Chrome"),
            ..Default::default()
        };
        assert_eq!(evaluate_offer(&signals, &ctx, &OfferPolicy::default()), Decision::Ignore);
    }

    #[test]
    fn pip_title_suppresses_even_with_mic() {
        let signals = Signals {
            mic_in_use: mic_counts_as_signal(MIC_ONLY_SUSTAIN_MS, false, false),
            ..Default::default()
        };
        let ctx = DetectionCtx {
            is_browser: true,
            page_host: Some(""),
            window_title: Some("Picture-in-picture"),
            ..Default::default()
        };
        assert_eq!(evaluate_offer(&signals, &ctx, &OfferPolicy::default()), Decision::Ignore);
        assert!(is_suppressed_title("Picture-in-picture"));
    }

    #[test]
    fn meet_url_opens_without_mic() {
        let signals = Signals { meeting_app_frontmost: true, ..Default::default() };
        let ctx = DetectionCtx {
            is_browser: true,
            page_host: Some("meet.google.com"),
            has_meet_url: true,
            ..Default::default()
        };
        assert!(matches!(
            evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
            Decision::Offer { .. }
        ));
    }

    #[test]
    fn zoom_bundle_opens_without_mic() {
        let signals = Signals { meeting_app_frontmost: true, ..Default::default() };
        let ctx = DetectionCtx { has_strong_bundle: true, ..Default::default() };
        assert!(matches!(
            evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
            Decision::Offer { .. }
        ));
    }

    #[test]
    fn controls_alone_need_a_second_signal() {
        let signals = Signals { meeting_controls_visible: true, ..Default::default() };
        assert!(matches!(decide(&signals), Decision::Offer { .. }));
        assert_eq!(
            evaluate_offer(&signals, &DetectionCtx::default(), &OfferPolicy::default()),
            Decision::Ignore
        );
    }

    #[test]
    fn host_from_url_parses_meet() {
        assert_eq!(host_from_url("https://meet.google.com/abc-defg-hij").as_deref(), Some("meet.google.com"));
    }

    #[test]
    fn zoom_is_a_known_meeting_app() {
        assert!(is_meeting_app("us.zoom.xos"));
    }

    #[test]
    fn an_ordinary_app_is_not_a_meeting_app() {
        assert!(!is_meeting_app("com.apple.Safari"));
        assert!(!is_meeting_app(""));
    }

    #[test]
    fn google_meet_urls_are_recognised() {
        assert!(is_meeting_url("https://meet.google.com/abc-defg-hij"));
    }

    #[test]
    fn youtube_is_not_a_meeting_url() {
        assert!(!is_meeting_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(!is_meeting_url("https://youtu.be/dQw4w9WgXcQ"));
        assert!(!is_meeting_url("https://m.youtube.com/watch?v=abc"));
    }

    #[test]
    fn media_urls_are_recognised() {
        assert!(is_media_url("https://www.youtube.com/watch?v=abc"));
        assert!(is_media_url("https://youtu.be/abc"));
        assert!(is_media_url("https://music.youtube.com/watch?v=abc"));
        assert!(is_media_url("https://www.netflix.com/watch/123"));
        assert!(is_media_url("https://www.twitch.tv/somechannel"));
    }

    #[test]
    fn meet_urls_are_not_media() {
        assert!(!is_media_url("https://meet.google.com/abc-defg-hij"));
    }

    #[test]
    fn mic_on_media_never_counts_without_meeting_context() {
        assert!(!mic_counts_as_signal(MIC_ONLY_SUSTAIN_MS, false, true));
        assert!(!mic_counts_as_signal(MIC_ONLY_SUSTAIN_MS + 1_000, false, true));
    }

    #[test]
    fn mic_on_media_counts_with_meeting_context_after_short_sustain() {
        assert!(!mic_counts_as_signal(MIC_SUSTAIN_MS - 1, true, true));
        assert!(mic_counts_as_signal(MIC_SUSTAIN_MS, true, true));
    }

    #[test]
    fn mic_alone_needs_the_longer_sustain() {
        assert!(!mic_counts_as_signal(MIC_SUSTAIN_MS, false, false));
        assert!(mic_counts_as_signal(MIC_ONLY_SUSTAIN_MS, false, false));
    }

    #[test]
    fn mic_with_meeting_context_uses_the_shorter_sustain() {
        assert!(mic_counts_as_signal(MIC_SUSTAIN_MS, true, false));
    }

    #[test]
    fn a_lookalike_host_is_not_a_meeting_url() {
        // Host matching must not be substring matching, or an attacker-controlled or merely
        // unlucky domain turns the microphone offer on.
        assert!(!is_meeting_url("https://meet.google.com.evil.test/x"));
        assert!(!is_meeting_url("https://notmeet.google.com/x"));
        assert!(!is_meeting_url("https://example.test/?u=meet.google.com"));
    }

    #[test]
    fn meeting_surface_matrix_normalizes_supported_variants() {
        for (bundle, expected) in [
            ("us.zoom.xos", Some(MeetingHint::Strong)),
            (" US.ZOOM.XOS ", Some(MeetingHint::Strong)),
            ("com.microsoft.teams2", Some(MeetingHint::Weak)),
            ("COM.MICROSOFT.TEAMS", Some(MeetingHint::Weak)),
            ("com.microsoft.teams.work", Some(MeetingHint::Weak)),
            ("com.apple.Safari", None),
        ] {
            assert_eq!(bundle_hint(bundle), expected, "bundle {bundle}");
        }

        for (url, expected) in [
            ("https://MEET.GOOGLE.COM./abc-defg-hij", Some(MeetingHint::Strong)),
            ("https://zoom.us/j/123456789", Some(MeetingHint::Strong)),
            ("https://us02web.zoom.us/j/123456789", Some(MeetingHint::Strong)),
            ("https://app.zoom.us/wc/123456789/join", Some(MeetingHint::Strong)),
            ("https://teams.microsoft.com/l/meetup-join/19%3ameeting", Some(MeetingHint::Weak)),
            ("https://teams.cloud.microsoft/meet/abc", Some(MeetingHint::Weak)),
            ("https://teams.microsoft.us/meet/abc", Some(MeetingHint::Weak)),
            ("https://zoom.us/pricing", None),
            ("https://app.zoom.us/account", None),
            ("https://us02web.zoom.us/", None),
            ("https://teams.microsoft.com/_#/conversations/General", None),
            ("https://teams.live.com/calendar", None),
            ("https://teams.microsoft.com.evil.test/l/meetup-join/abc", None),
            ("https://zoom.us.evil.test/j/123456789", None),
        ] {
            assert_eq!(host_hint(url), expected, "url {url}");
        }

        for (process, expected) in [
            (" Zoom Workplace ", Some(MeetingHint::Weak)),
            ("MICROSOFT TEAMS", Some(MeetingHint::Weak)),
            ("Meeting Helper", None),
        ] {
            assert_eq!(process_hint(process), expected, "process {process}");
        }

        for (title, expected) in [
            ("Weekly sync | Zoom Meeting", Some(MeetingHint::Weak)),
            ("abc-defg-hij - Google Meet", Some(MeetingHint::Weak)),
            ("Team standup | Microsoft Teams", Some(MeetingHint::Weak)),
            ("Meeting notes - Google Docs", None),
            ("Meet the team - Company wiki", None),
        ] {
            assert_eq!(title_hint(title), expected, "title {title}");
        }
    }

    // ── Plan A: tiered (Strong/Weak) detection ─────────────────────────────────────────────

    #[test]
    fn a_weak_bundle_alone_does_not_offer() {
        // A-0/A-2: a resident chat app in front is someone reading messages, not a meeting.
        let signals = Signals::default();
        let ctx = DetectionCtx {
            has_weak_meeting_signal: true,
            window_title: Some("Chat | Microsoft Teams"),
            ..Default::default()
        };
        assert_eq!(evaluate_offer(&signals, &ctx, &OfferPolicy::default()), Decision::Ignore);
    }

    #[test]
    fn a_weak_bundle_with_sustained_mic_offers() {
        // A-0: the Weak vote plus sustained mic is the two-signal case — and it must clear the
        // default policy's mic-only gate, because the mic is not "only" here.
        let signals = Signals {
            mic_in_use: mic_counts_as_signal(MIC_SUSTAIN_MS, true, false),
            ..Default::default()
        };
        let ctx = DetectionCtx { has_weak_meeting_signal: true, ..Default::default() };
        let d = evaluate_offer(&signals, &ctx, &OfferPolicy::default());
        let Decision::Offer { provenance, .. } = d else { panic!("expected an offer") };
        assert!(provenance.contains("mic_sustained"));
        assert!(provenance.contains("weak_meeting_signal"), "the Weak vote is evidence too");
        serde_json::from_str::<serde_json::Value>(&provenance).expect("provenance must be JSON");
    }

    #[test]
    fn strong_hosts_still_offer_alone() {
        // The A-0 refactor must not weaken what already worked: a Strong URL frontmost is an
        // opener by itself, exactly as before the tiers existed.
        for (url, host) in [
            ("https://meet.google.com/abc-defg-hij", "meet.google.com"),
            ("https://app.zoom.us/wc/1234567890/join", "app.zoom.us"),
        ] {
            assert_eq!(host_hint(url), Some(MeetingHint::Strong), "{url}");
            let signals = Signals { meeting_app_frontmost: true, ..Default::default() };
            let ctx = DetectionCtx {
                is_browser: true,
                page_host: Some(host),
                has_meet_url: true,
                ..Default::default()
            };
            assert!(
                matches!(
                    evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
                    Decision::Offer { .. }
                ),
                "{url} should offer alone"
            );
        }
    }

    #[test]
    fn zoom_web_client_host_offers() {
        // A-1: the Zoom Web client lives on app.zoom.us and is Strong like the native app.
        assert_eq!(host_hint("https://app.zoom.us/wc/1234567890/start"), Some(MeetingHint::Strong));
        assert!(is_meeting_url("https://app.zoom.us/wc/1234567890/start"));
    }

    #[test]
    fn zoom_marketing_site_does_not() {
        // A-1: zoom.us is the marketing site, not the meeting — and no lookalike or subdomain
        // trick may promote itself to app.zoom.us.
        assert_eq!(host_hint("https://zoom.us/pricing"), None);
        assert_eq!(host_hint("https://www.zoom.us/"), None);
        assert_eq!(host_hint("https://app.zoom.us.evil.test/wc"), None);
        assert_eq!(host_hint("https://example.test/?u=app.zoom.us"), None);
    }

    #[test]
    fn teams_frontmost_alone_never_offers() {
        // A-2: Teams is where chat lives all day. Frontmost — even with a calendar occurrence
        // agreeing — is not an observation of attendance and must never open the offer.
        for bundle in ["com.microsoft.teams2", "com.microsoft.teams"] {
            assert_eq!(bundle_hint(bundle), Some(MeetingHint::Weak), "{bundle}");
        }
        let signals = Signals { occurrence_now: true, ..Default::default() };
        let ctx = DetectionCtx { has_weak_meeting_signal: true, ..Default::default() };
        assert_eq!(evaluate_offer(&signals, &ctx, &OfferPolicy::default()), Decision::Ignore);
    }

    #[test]
    fn teams_with_mic_sustained_offers() {
        // A-2: the same Teams window plus a sustained mic is a call.
        let signals = Signals {
            mic_in_use: mic_counts_as_signal(MIC_SUSTAIN_MS, true, false),
            ..Default::default()
        };
        let ctx = DetectionCtx {
            has_weak_meeting_signal: true,
            window_title: Some("Weekly sync | Microsoft Teams"),
            ..Default::default()
        };
        assert!(matches!(
            evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
            Decision::Offer { .. }
        ));
    }

    #[test]
    fn webex_subdomain_matches() {
        // A-2: every Webex tenant is its own subdomain; the suffix match is exact-label, so a
        // lookalike registering evilwebex.com gains nothing.
        assert_eq!(host_hint("https://acme.webex.com/meet/team"), Some(MeetingHint::Weak));
        assert_eq!(host_hint("https://webex.com/"), Some(MeetingHint::Weak));
        assert_eq!(host_hint("https://evilwebex.com/"), None);
        assert_eq!(host_hint("https://acme.webex.com.evil.test/"), None);
        assert_eq!(bundle_hint("Cisco-Systems.Spark"), Some(MeetingHint::Weak));
    }

    #[test]
    fn teams_meeting_ends_on_silence_not_on_app_presence() {
        // A-2: Teams keeps running after everyone hangs up, so AppGone cannot be the end for a
        // Weak-bundle meeting — the silence limit is, and the app's continued presence must not
        // postpone it.
        let now = 1_000_000;
        let call_over_app_still_running = LiveSignals {
            meeting_app_present: true,
            occurrence_ends_at: None,
            last_sound_at: now - SILENCE_LIMIT_MS - 1,
        };
        assert_eq!(end_condition(&call_over_app_still_running, now), Some(EndReason::Silence));

        let still_talking = LiveSignals {
            meeting_app_present: true,
            occurrence_ends_at: None,
            last_sound_at: now,
        };
        assert_eq!(end_condition(&still_talking, now), None);
    }

    #[test]
    fn weak_bundle_quit_does_not_read_as_leaving_the_call() {
        // A-2, the call_clearly_ended side of the same rule: only a Strong bundle's disappearance
        // means "left". A quit Teams with the mic still open is a call routed elsewhere.
        assert!(!call_clearly_ended(
            false,
            None,
            1_000_000,
            true, // mic still open
            None,
            Some("com.microsoft.teams2"),
            false, // app no longer running
        ));
        // Zoom (Strong) quitting still ends it, exactly as before the tiers.
        assert!(call_clearly_ended(false, None, 1_000_000, true, None, Some("us.zoom.xos"), false));
    }

    // ── Plan A-4: Slack huddles ────────────────────────────────────────────────────────────

    #[test]
    fn huddle_title_plus_mic_offers() {
        // The hint needs the huddle word AND call-control vocabulary, then mic-sustained is the
        // second vote. English and Japanese UIs both count.
        assert!(huddle_hint(Some("Huddle • #design"), &["Mute", "Leave huddle"]));
        assert!(huddle_hint(Some("ハドル • #design"), &["ミュート", "退出"]));

        let signals = Signals {
            mic_in_use: mic_counts_as_signal(MIC_SUSTAIN_MS, true, false),
            ..Default::default()
        };
        let ctx = DetectionCtx {
            has_huddle_hint: true,
            window_title: Some("Huddle • #design"),
            ..Default::default()
        };
        let d = evaluate_offer(&signals, &ctx, &OfferPolicy::default());
        let Decision::Offer { provenance, .. } = d else { panic!("expected an offer") };
        assert!(provenance.contains("huddle_hint"), "the hint is evidence and must be recorded");
    }

    #[test]
    fn huddle_title_alone_does_nothing() {
        // One vote is not a meeting: without the mic the hint must produce no offer at all,
        // however clearly the window says "Huddle".
        assert!(huddle_hint(Some("Huddle • #design"), &["Mute"]));
        let signals = Signals::default();
        let ctx = DetectionCtx {
            has_huddle_hint: true,
            window_title: Some("Huddle • #design"),
            ..Default::default()
        };
        assert_eq!(evaluate_offer(&signals, &ctx, &OfferPolicy::default()), Decision::Ignore);
    }

    #[test]
    fn slack_chat_mentioning_huddle_does_not_trigger() {
        // "huddle" in message text is conversation, not a call — the control vocabulary is what
        // separates the huddle UI from chat about huddles.
        assert!(!huddle_hint(
            Some("#general – Slack"),
            &["did you catch the huddle earlier?", "yes — notes are in the doc"],
        ));
        assert!(!huddle_hint(None, &["let's huddle tomorrow morning"]));
        assert!(!huddle_hint(Some("#general – Slack"), &["昨日のハドルどうだった？"]));
        // And control words with no huddle word at all (an ordinary call app's UI) do nothing.
        assert!(!huddle_hint(Some("#general – Slack"), &["Mute", "Leave"]));
    }

    #[test]
    fn huddle_hint_lost_needs_grace_before_wrap() {
        // Mirrors the Meet-tab leave grace: a redraw or a glance at another channel must not
        // wrap the huddle mid-sentence.
        let lost = 1_000_000;
        assert!(!huddle_hint_lost_past_grace(Some(lost), lost + HUDDLE_HINT_LOST_GRACE_MS - 1));
        assert!(huddle_hint_lost_past_grace(Some(lost), lost + HUDDLE_HINT_LOST_GRACE_MS));
    }

    #[test]
    fn huddle_hint_never_lost_has_no_grace_deadline() {
        assert!(!huddle_hint_lost_past_grace(None, 9_999_999));
    }

    #[test]
    fn huddle_past_grace_stays_present_while_mic_open() {
        // Alt-tabbing away from Slack hides the huddle UI, but the open mic proves the call is
        // still running — exactly the Meet-tab rule.
        let lost = 1_000_000;
        let after = lost + HUDDLE_HINT_LOST_GRACE_MS;
        assert!(huddle_session_present(Some(lost), after, true, None));
    }

    #[test]
    fn huddle_past_grace_ends_after_mic_quiet() {
        let lost = 1_000_000;
        let after_grace = lost + HUDDLE_HINT_LOST_GRACE_MS;
        let closed = after_grace;
        assert!(huddle_session_present(Some(lost), closed, false, Some(closed)));
        let quiet = closed + MIC_QUIET_AFTER_URL_LEFT_MS;
        assert!(!huddle_session_present(Some(lost), quiet, false, Some(closed)));
    }

    #[test]
    fn huddle_within_grace_is_present_regardless_of_mic() {
        // A redraw or a glance at another channel must not wrap the huddle mid-sentence.
        let lost = 1_000_000;
        let within = lost + HUDDLE_HINT_LOST_GRACE_MS - 1;
        assert!(huddle_session_present(Some(lost), within, false, Some(lost)));
    }

    #[test]
    fn huddle_hint_never_lost_is_always_present() {
        assert!(huddle_session_present(None, 9_999_999, false, Some(0)));
    }


    #[test]
    fn meet_url_in_browser_counts_as_present() {
        assert!(browser_meeting_page_present(
            Some("https://meet.google.com/abc-defg-hij"),
            Some("Meet – weekly – Google Chrome"),
        ));
    }

    #[test]
    fn gmail_tab_means_meeting_page_left() {
        assert!(!browser_meeting_page_present(
            Some("https://mail.google.com/mail/u/0/"),
            Some("Inbox - Gmail"),
        ));
    }

    #[test]
    fn pip_with_unreadable_url_still_counts_as_present() {
        assert!(browser_meeting_page_present(
            None,
            Some("Picture-in-picture"),
        ));
    }

    #[test]
    fn meeting_url_left_needs_grace_before_wrap() {
        let lost = 1_000_000;
        assert!(!meeting_url_left_past_grace(Some(lost), lost + MEETING_URL_LEFT_GRACE_MS - 1));
        assert!(meeting_url_left_past_grace(Some(lost), lost + MEETING_URL_LEFT_GRACE_MS));
    }

    #[test]
    fn meeting_url_never_lost_has_no_grace_deadline() {
        assert!(!meeting_url_left_past_grace(None, 9_999_999));
    }

    #[test]
    fn meet_url_past_grace_stays_present_while_mic_open() {
        let lost = 1_000_000;
        let after = lost + MEETING_URL_LEFT_GRACE_MS;
        assert!(meet_url_session_present(Some(lost), after, true, None));
    }

    #[test]
    fn meet_url_past_grace_ends_after_mic_quiet() {
        let lost = 1_000_000;
        let after_grace = lost + MEETING_URL_LEFT_GRACE_MS;
        let closed = after_grace;
        assert!(meet_url_session_present(Some(lost), closed, false, Some(closed)));
        let quiet = closed + MIC_QUIET_AFTER_URL_LEFT_MS;
        assert!(!meet_url_session_present(Some(lost), quiet, false, Some(closed)));
    }

    #[test]
    fn tab_switch_with_mic_open_keeps_session() {
        let lost = 1_000_000;
        let later = lost + MEETING_URL_LEFT_GRACE_MS + 60_000;
        assert!(meet_url_session_present(Some(lost), later, true, None));
    }

    /// A coarse (system-wide) observation while `app` is in front.
    fn coarse<'a>(in_use: bool, app: &'a str, meeting_context: bool) -> MicObservation<'a> {
        MicObservation { in_use, source: MicSource::SystemWide, frontmost_bundle_id: app, meeting_context }
    }

    /// The pre-existing shape of these tests: coarse signal, meeting app in front.
    fn in_meeting(in_use: bool) -> MicObservation<'static> {
        coarse(in_use, "us.zoom.xos", true)
    }

    #[test]
    fn a_brief_burst_of_microphone_use_is_not_a_meeting() {
        // Dictation, a voice memo, "hey" into a chat app. Offering to take notes on those is how
        // the panel becomes something the user learns to dismiss without reading.
        let mut w = MicWatch::new();
        assert!(!w.observe(&in_meeting(true), 0));
        assert!(!w.observe(&in_meeting(true), 5_000));
        assert!(!w.observe(&in_meeting(false), 6_000));
        assert!(!w.observe(&in_meeting(true), 7_000), "the clock restarts when the mic closes");
    }

    #[test]
    fn sustained_microphone_use_is_a_meeting() {
        let mut w = MicWatch::new();
        w.observe(&in_meeting(true), 0);
        assert!(!w.observe(&in_meeting(true), MIC_SUSTAIN_MS - 1));
        assert!(w.observe(&in_meeting(true), MIC_SUSTAIN_MS));
    }

    #[test]
    fn the_signal_stays_true_while_the_call_continues() {
        // It has to keep answering yes: the detector asks once a second, and a meeting that
        // "became true and then went quiet" would close the interval mid-call.
        let mut w = MicWatch::new();
        w.observe(&in_meeting(true), 0);
        for t in 0..30 {
            let now = MIC_SUSTAIN_MS + t * 1_000;
            assert!(w.observe(&in_meeting(true), now), "second {t} of the call reported no meeting");
        }
    }

    #[test]
    fn hanging_up_and_calling_again_needs_the_full_sustain_again() {
        let mut w = MicWatch::new();
        w.observe(&in_meeting(true), 0);
        assert!(w.observe(&in_meeting(true), MIC_SUSTAIN_MS));

        w.observe(&in_meeting(false), MIC_SUSTAIN_MS + 1_000);

        assert!(
            !w.observe(&in_meeting(true), MIC_SUSTAIN_MS + 2_000),
            "a new call starts its own clock"
        );
    }

    // ---- the stuck coarse signal (observed on-device 2026-07-31) ------------------------------

    #[test]
    fn an_always_on_holder_stops_reporting_a_meeting_in_every_app() {
        // The bug this check exists for: a voice utility held an input device from login, so the
        // system-wide flag was true in Finder, in the login window and everywhere else — and the
        // watch answered "meeting" for all of them.
        let mut w = MicWatch::new();
        let apps = ["com.apple.finder", "com.google.Chrome", "com.tinyspeck.slackmacgap"];

        // Before the tally can condemn it the signal still reports: it has no reason yet.
        w.observe(&coarse(true, apps[0], false), 0);
        assert!(w.observe(&coarse(true, apps[0], false), MIC_SUSTAIN_MS));

        // The user moves through unrelated apps; past the floor, the signal is written off.
        let t = MIC_STUCK_MIN_MS + MIC_SUSTAIN_MS;
        w.observe(&coarse(true, apps[1], false), t);
        let last = w.observe(&coarse(true, apps[2], false), t + 1_000);

        assert!(w.is_stuck(), "three unrelated apps past the floor is a stuck device");
        assert!(!last, "a stuck signal must not report a meeting");
        assert!(!w.observe(&coarse(true, "com.apple.loginwindow", false), t + 2_000));
    }

    #[test]
    fn multitasking_early_in_a_call_does_not_condemn_the_signal() {
        // Joining a call and immediately opening the agenda, the calendar and a scratchpad is
        // exactly the shape of the stuck pattern — the time floor is what tells them apart.
        let mut w = MicWatch::new();
        w.observe(&coarse(true, "com.hnc.Discord", false), 0);
        w.observe(&coarse(true, "com.apple.Calendar", false), 5_000);
        let reporting = w.observe(&coarse(true, "com.apple.Notes", false), MIC_SUSTAIN_MS + 1);

        assert!(!w.is_stuck(), "three apps inside the floor is a busy call, not a stuck device");
        assert!(reporting, "a real call must keep its opener");
    }

    #[test]
    fn a_meeting_coming_into_view_clears_the_verdict() {
        // "Stuck" means no explanation was ever offered. A meeting in front is an explanation.
        let mut w = MicWatch::new();
        let t = MIC_STUCK_MIN_MS + MIC_SUSTAIN_MS;
        w.observe(&coarse(true, "com.apple.finder", false), 0);
        w.observe(&coarse(true, "com.google.Chrome", false), t);
        w.observe(&coarse(true, "com.tinyspeck.slackmacgap", false), t + 1_000);
        assert!(w.is_stuck());

        let back = w.observe(&coarse(true, "us.zoom.xos", true), t + 2_000);
        assert!(!w.is_stuck(), "an explained device is not a stuck one");
        assert!(back, "the opener returns for a meeting that is actually in front");
    }

    #[test]
    fn releasing_the_device_clears_the_verdict() {
        let mut w = MicWatch::new();
        let t = MIC_STUCK_MIN_MS + MIC_SUSTAIN_MS;
        w.observe(&coarse(true, "a", false), 0);
        w.observe(&coarse(true, "b", false), t);
        w.observe(&coarse(true, "c", false), t + 1_000);
        assert!(w.is_stuck());

        w.observe(&coarse(false, "a", false), t + 2_000);
        assert!(!w.is_stuck(), "a device that can be released was never permanently stuck");
    }

    // ---- attributed use (macOS 14.4+) --------------------------------------------------------

    fn held_by<'a>(holder: &'a str, front: &'a str) -> MicObservation<'a> {
        MicObservation {
            in_use: true,
            source: MicSource::Holder { bundle_id: holder },
            frontmost_bundle_id: front,
            meeting_context: false,
        }
    }

    #[test]
    fn a_background_daemon_holding_the_mic_is_not_a_meeting() {
        let mut w = MicWatch::new();
        w.observe(&held_by("com.voiceos.app", "com.apple.finder"), 0);
        assert!(!w.observe(&held_by("com.voiceos.app", "com.apple.finder"), MIC_SUSTAIN_MS * 10));
    }

    #[test]
    fn a_meeting_app_holding_the_mic_counts_even_from_the_background() {
        // The call is in Zoom while the user takes notes in another app. Requiring the holder to
        // be frontmost would drop the signal exactly when the user is doing the thing SHOGUN is
        // for.
        let mut w = MicWatch::new();
        w.observe(&held_by("us.zoom.xos", "com.apple.Notes"), 0);
        assert!(w.observe(&held_by("us.zoom.xos", "com.apple.Notes"), MIC_SUSTAIN_MS));
    }

    #[test]
    fn the_app_in_front_holding_the_mic_counts_even_if_unlisted() {
        // A huddle in an app nobody put in the bundle table is still a call.
        let mut w = MicWatch::new();
        w.observe(&held_by("com.hnc.Discord", "com.hnc.Discord"), 0);
        assert!(w.observe(&held_by("com.hnc.Discord", "com.hnc.Discord"), MIC_SUSTAIN_MS));
    }

    #[test]
    fn our_own_capture_never_counts_as_a_new_meeting() {
        // SHOGUN's ASR holds the input during a meeting it is already noting. Reading that back
        // as evidence would let the app detect itself.
        let mut w = MicWatch::new();
        w.observe(&held_by(SELF_BUNDLE_ID, SELF_BUNDLE_ID), 0);
        assert!(!w.observe(&held_by(SELF_BUNDLE_ID, SELF_BUNDLE_ID), MIC_SUSTAIN_MS * 10));
    }
}
