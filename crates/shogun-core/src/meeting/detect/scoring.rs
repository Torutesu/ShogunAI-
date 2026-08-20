//! Raw detection-signal scoring and provenance construction.

use super::{Decision, Signals};

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
        (
            signals.meeting_app_frontmost,
            W_APP,
            "meeting_app_frontmost",
        ),
        (
            signals.meeting_controls_visible,
            W_CONTROLS,
            "meeting_controls_visible",
        ),
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
