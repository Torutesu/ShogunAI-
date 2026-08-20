//! Bundle, process, title, URL, and passive-media matching for meeting detection.

use super::MeetingHint;

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
    if [
        "zoom meeting",
        "zoom workplace",
        "google meet",
        "microsoft teams",
        "teams meeting",
    ]
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
    if !TEAMS_HOSTS.contains(&host) {
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
    suffixes
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
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
    let Some(host) = host_of(url) else {
        return false;
    };
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

/// The host component of an absolute URL, lowercased and without userinfo or port.
pub fn host_from_url(url: &str) -> Option<String> {
    host_of(url)
}

pub(super) fn host_of(url: &str) -> Option<String> {
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
