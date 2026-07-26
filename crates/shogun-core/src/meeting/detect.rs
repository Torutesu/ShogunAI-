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

/// Whether a browser URL is a meeting (FR-MT-04).
///
/// Matches on the parsed **host**, never on a substring of the URL: `meet.google.com.evil.test`
/// and `?redirect=meet.google.com` both contain the host as text, and a `contains` check here
/// would let an arbitrary page raise the offer to listen.
pub fn is_meeting_url(url: &str) -> bool {
    let Some(host) = host_of(url) else { return false };
    MEETING_HOSTS.iter().any(|h| host == *h)
}

/// The host component of an absolute URL, lowercased and without userinfo or port.
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
const W_APP: f64 = 0.35;
const W_CONTROLS: f64 = 0.30;
const W_MIC: f64 = 0.15;
const W_OCCURRENCE: f64 = 0.15;

/// Combine the signals of one tick into a decision.
pub fn decide(signals: &Signals) -> Decision {
    // ② or ③ — an observation that the user is *in* a meeting. Without one of these there is
    // nothing to offer, however full the calendar is.
    let observed = signals.meeting_app_frontmost || signals.meeting_controls_visible;
    if !observed {
        return Decision::Ignore;
    }

    let mut confidence = 0.0;
    let mut fired: Vec<&str> = Vec::new();
    for (on, weight, name) in [
        (signals.meeting_app_frontmost, W_APP, "meeting_app_frontmost"),
        (signals.meeting_controls_visible, W_CONTROLS, "meeting_controls_visible"),
        (signals.mic_in_use, W_MIC, "mic_in_use"),
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(provenance.contains("mic_in_use"));
        assert!(!provenance.contains("occurrence_now"), "a signal that did not fire is not evidence");
        serde_json::from_str::<serde_json::Value>(&provenance).expect("provenance must be JSON");
    }

    #[test]
    fn the_microphone_alone_is_not_a_meeting() {
        // Dictation, a voice memo, a phone call in the browser: the mic being open says someone
        // is speaking, not that a meeting is happening. Offering on this alone would make the
        // panel appear whenever the user talks to their computer.
        let d = decide(&Signals { mic_in_use: true, ..Default::default() });
        assert_eq!(d, Decision::Ignore);
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
    fn a_lookalike_host_is_not_a_meeting_url() {
        // Host matching must not be substring matching, or an attacker-controlled or merely
        // unlucky domain turns the microphone offer on.
        assert!(!is_meeting_url("https://meet.google.com.evil.test/x"));
        assert!(!is_meeting_url("https://notmeet.google.com/x"));
        assert!(!is_meeting_url("https://example.test/?u=meet.google.com"));
    }
}
