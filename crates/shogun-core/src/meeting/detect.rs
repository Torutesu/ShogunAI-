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

mod matching;
mod rules;
mod scoring;

pub use matching::{
    bundle_hint, host_from_url, host_hint, is_media_url, is_meeting_app, is_meeting_url,
    is_suppressed_title, process_hint, title_hint, SLACK_BUNDLE_ID,
};
pub use rules::{
    browser_meeting_page_present, call_clearly_ended, end_condition, evaluate_offer, huddle_hint,
    huddle_hint_lost_past_grace, huddle_session_present, meet_url_session_present,
    meeting_url_left_past_grace, mic_counts_as_signal, DetectionCtx, LiveSignals, MicObservation,
    MicSource, MicWatch, OfferPolicy, HUDDLE_HINT_LOST_GRACE_MS, MEETING_URL_LEFT_GRACE_MS,
    MIC_ONLY_SUSTAIN_MS, MIC_QUIET_AFTER_URL_LEFT_MS, MIC_STUCK_DISTINCT_APPS, MIC_STUCK_MIN_MS,
    MIC_SUSTAIN_MS, OCCURRENCE_GRACE_MS, SILENCE_LIMIT_MS,
};
pub use scoring::decide;

#[cfg(test)]
#[path = "detect/tests.rs"]
mod tests;
