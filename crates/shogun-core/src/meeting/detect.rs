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

/// What the adapter observed at one detection tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Signals {
    /// ② A known meeting app is frontmost (see [`is_meeting_app`]), or the browser is on a
    /// meeting URL (see [`is_meeting_url`]).
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

/// Bundle ids of the meeting apps v1 detects (FR-MT-04). Zoom only for the native app; Meet is a
/// browser URL, handled by [`is_meeting_url`].
///
/// A table rather than a heuristic: guessing "is this a meeting app?" from a window title is how
/// a note-taking offer ends up appearing over someone's banking tab.
const MEETING_BUNDLE_IDS: &[&str] = &["us.zoom.xos"];

pub fn is_meeting_app(bundle_id: &str) -> bool {
    MEETING_BUNDLE_IDS.contains(&bundle_id)
}

/// Hosts that mean "a meeting is open in the browser" (FR-MT-04). Google Meet in v1.
const MEETING_HOSTS: &[&str] = &["meet.google.com"];

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

/// Whether a browser URL is a meeting (FR-MT-04).
///
/// Matches on the parsed **host**, never on a substring of the URL: `meet.google.com.evil.test`
/// and `?redirect=meet.google.com` both contain the host as text, and a `contains` check here
/// would let an arbitrary page raise the offer to listen.
pub fn is_meeting_url(url: &str) -> bool {
    let Some(host) = host_of(url) else { return false };
    MEETING_HOSTS.iter().any(|h| host == *h)
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
    MEDIA_HOST_SUFFIXES
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
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
    /// A known meeting URL is open (`meet.google.com`, …).
    pub has_meet_url: bool,
    /// Native Zoom (`us.zoom.xos`) is frontmost.
    pub has_zoom_bundle: bool,
    /// Focused window title, when AX returned one.
    pub window_title: Option<&'a str>,
}

/// User settings that gate how aggressively mic-only evidence may open an offer.
#[derive(Debug, Clone, Copy, Default)]
pub struct OfferPolicy {
    /// When `false`, sustained mic use alone never opens an interval.
    pub allow_mic_only: bool,
}

/// Count how many independent signals fired (calendar included when present).
fn corroborating_count(signals: &Signals) -> usize {
    usize::from(signals.mic_in_use)
        + usize::from(signals.meeting_app_frontmost)
        + usize::from(signals.meeting_controls_visible)
        + usize::from(signals.occurrence_now)
}

/// Whether the observation already proves a v1 meeting app (Meet URL or native Zoom).
fn has_strong_opener(ctx: &DetectionCtx<'_>) -> bool {
    ctx.has_meet_url || ctx.has_zoom_bundle
}

/// Browser tab with no readable host — cannot prove Meet is open (PiP, AX gaps).
fn browser_lacks_meeting_proof(ctx: &DetectionCtx<'_>) -> bool {
    ctx.is_browser && !ctx.has_meet_url && ctx.page_host.is_none_or(str::is_empty)
}

/// Apply FR-MT-04 policy on top of raw signals, then score.
///
/// Biases toward fewer false positives: mic-only is opt-in, PiP/media titles are dropped,
/// browsers with an empty host cannot corroborate, and non-URL offers need two agreeing signals
/// unless Meet or Zoom is already proven.
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

    let mic_is_only_opener = effective.mic_in_use
        && !effective.meeting_app_frontmost
        && !effective.meeting_controls_visible;
    if mic_is_only_opener && !policy.allow_mic_only {
        effective.mic_in_use = false;
    }

    let d = decide(&effective);
    let Decision::Offer { confidence, provenance } = d else {
        return d;
    };

    if !has_strong_opener(ctx) && corroborating_count(&effective) < 2 {
        let mic_only_allowed = policy.allow_mic_only
            && effective.mic_in_use
            && !effective.meeting_app_frontmost
            && !effective.meeting_controls_visible;
        if !mic_only_allowed {
            return Decision::Ignore;
        }
    }

    Decision::Offer { confidence, provenance }
}

/// The host component of an absolute URL, lowercased and without userinfo or port.
pub fn host_from_url(url: &str) -> Option<String> {
    host_of(url)
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    // `user@host` — the host is what follows the last '@', so `evil.test@meet.google.com` cannot
    // masquerade as the host by sitting in the userinfo.
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
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
}

impl MicWatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one observation. Returns whether the microphone has been continuously in use for
    /// at least [`MIC_SUSTAIN_MS`] — used by the recording watchdog, not the offer threshold.
    pub fn observe(&mut self, in_use: bool, now: i64) -> bool {
        if !in_use {
            self.since_ms = None;
            return false;
        }
        let since = *self.since_ms.get_or_insert(now);
        // `saturating_sub` so a clock that jumps backwards restarts the wait instead of
        // reporting a meeting that has been running for negative time.
        now.saturating_sub(since) >= MIC_SUSTAIN_MS
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
        let lower = title.to_ascii_lowercase();
        if lower.contains("meet") || lower.contains("zoom") {
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
        .is_none_or(|since| now.saturating_sub(since) < MIC_QUIET_AFTER_URL_LEFT_MS)
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
    } else if zoom_bundle_id.is_some_and(is_meeting_app) {
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
        let ctx = DetectionCtx { has_zoom_bundle: true, ..Default::default() };
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

    #[test]
    fn a_brief_burst_of_microphone_use_is_not_a_meeting() {
        // Dictation, a voice memo, "hey" into a chat app. Offering to take notes on those is how
        // the panel becomes something the user learns to dismiss without reading.
        let mut w = MicWatch::new();
        assert!(!w.observe(true, 0));
        assert!(!w.observe(true, 5_000));
        assert!(!w.observe(false, 6_000));
        assert!(!w.observe(true, 7_000), "the clock restarts when the mic closes");
    }

    #[test]
    fn sustained_microphone_use_is_a_meeting() {
        let mut w = MicWatch::new();
        w.observe(true, 0);
        assert!(!w.observe(true, MIC_SUSTAIN_MS - 1));
        assert!(w.observe(true, MIC_SUSTAIN_MS));
    }

    #[test]
    fn the_signal_stays_true_while_the_call_continues() {
        // It has to keep answering yes: the detector asks once a second, and a meeting that
        // "became true and then went quiet" would close the interval mid-call.
        let mut w = MicWatch::new();
        w.observe(true, 0);
        for t in 0..30 {
            let now = MIC_SUSTAIN_MS + t * 1_000;
            assert!(w.observe(true, now), "second {t} of the call reported no meeting");
        }
    }

    #[test]
    fn hanging_up_and_calling_again_needs_the_full_sustain_again() {
        let mut w = MicWatch::new();
        w.observe(true, 0);
        assert!(w.observe(true, MIC_SUSTAIN_MS));

        w.observe(false, MIC_SUSTAIN_MS + 1_000);

        assert!(!w.observe(true, MIC_SUSTAIN_MS + 2_000), "a new call starts its own clock");
    }
}
