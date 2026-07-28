//! The three ways to say no (FR-MT-02), and the default that makes them meaningful (FR-MT-01).
//!
//! | tier | reached from | effect |
//! |---|---|---|
//! | (a) whole feature off | Settings → Meeting notes → Off | nothing is detected, nothing is listened to, the microphone is never touched |
//! | (b) not for this app | the Offered panel's secondary action / the exclusion list | that meeting app never offers again |
//! | (c) not this meeting | "Not now" / "Stop" | this one only; settings unchanged |
//!
//! The default is **off** (FR-MT-01). A feature that listens is the one kind that must never be
//! found already enabled — so enabling is a decision the user makes once, in onboarding, and an
//! update must not make it for them.
//!
//! Deciding whether to offer is a pure function of these settings plus what was observed, so the
//! rule is testable on its own and cannot drift between the panel, the daemon and the settings
//! screen.

use std::collections::BTreeSet;

/// Which on-device ASR model to use. `Small` (bundled default) or `Turbo` (large-v3-turbo,
/// opt-in high accuracy, fetched on first use). Defaults to Small (§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AsrModel {
    #[default]
    Small,
    Turbo,
}

/// Meeting-notes settings as persisted.
///
/// Every field defaults, so a file written before this feature existed — or a half-written one —
/// reads as the shipped default rather than failing to parse. The default being *off* is what
/// makes that safe: failing to read settings can never turn listening on (FR-MT-01).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Tier (a). `false` is the shipped default (FR-MT-01).
    pub enabled: bool,
    /// Tier (b): bundle ids (and meeting hosts) that never offer.
    pub excluded_apps: BTreeSet<String>,
    /// Tier (b), the calendar half: occurrence external ids that never offer — the recurring 1:1
    /// a user never wants noted (FR-MT-02).
    pub excluded_occurrences: BTreeSet<String>,
    /// Which on-device ASR model transcribes the meeting. Defaults to Small (§5).
    #[serde(default)]
    pub asr_model: AsrModel,
}

// Written out rather than derived, though it is derivable. `#[derive(Default)]` would leave the
// most important property of this type — that it ships off — resting on the reader knowing that
// `bool` defaults to `false`. This default is a promise to the user (FR-MT-01), so it is stated.
#[allow(clippy::derivable_impls)]
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            excluded_apps: BTreeSet::new(),
            excluded_occurrences: BTreeSet::new(),
            asr_model: AsrModel::Small,
        }
    }
}

/// What the detector saw, reduced to the identifiers the settings are expressed in.
#[derive(Debug, Clone, Copy, Default)]
pub struct OfferContext<'a> {
    pub app_bundle_id: Option<&'a str>,
    pub occurrence_external_id: Option<&'a str>,
}

impl Settings {
    /// Whether an offer may be shown at all. Consulted before detection acts on anything.
    ///
    /// Tier (a) is checked first and answers on its own: with the feature off, no exclusion list,
    /// calendar or observation can produce an offer. Tier (c) is not represented here — declining
    /// one meeting is deliberately not persisted (FR-MT-02c).
    pub fn may_offer(&self, ctx: &OfferContext<'_>) -> bool {
        if !self.enabled {
            return false;
        }
        if ctx.app_bundle_id.is_some_and(|id| self.excluded_apps.contains(id)) {
            return false;
        }
        if ctx
            .occurrence_external_id
            .is_some_and(|id| self.excluded_occurrences.contains(id))
        {
            return false;
        }
        true
    }

    /// Tier (b) from the Offered panel: never offer for this app again.
    pub fn exclude_app(&mut self, bundle_id: &str) {
        self.excluded_apps.insert(bundle_id.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zoom() -> OfferContext<'static> {
        OfferContext { app_bundle_id: Some("us.zoom.xos"), occurrence_external_id: None }
    }

    #[test]
    fn the_feature_ships_switched_off() {
        // The promise of FR-MT-01, asserted rather than trusted: a build that flips this default
        // fails here instead of in a user's meeting.
        assert!(!Settings::default().enabled);
    }

    #[test]
    fn nothing_is_offered_while_the_feature_is_off() {
        let s = Settings::default();
        assert!(!s.may_offer(&zoom()));
    }

    #[test]
    fn an_enabled_feature_offers_for_an_ordinary_meeting() {
        let s = Settings { enabled: true, ..Default::default() };
        assert!(s.may_offer(&zoom()));
    }

    #[test]
    fn an_excluded_app_never_offers() {
        let mut s = Settings { enabled: true, ..Default::default() };
        s.exclude_app("us.zoom.xos");
        assert!(!s.may_offer(&zoom()));
    }

    #[test]
    fn excluding_one_app_leaves_the_others_alone() {
        let mut s = Settings { enabled: true, ..Default::default() };
        s.exclude_app("us.zoom.xos");

        let other = OfferContext { app_bundle_id: Some("com.google.Chrome"), ..Default::default() };
        assert!(s.may_offer(&other));
    }

    #[test]
    fn an_excluded_recurring_meeting_never_offers() {
        // The 1:1 a user does not want noted stays un-noted every week, without them having to
        // decline it every week (FR-MT-02).
        let mut s = Settings { enabled: true, ..Default::default() };
        s.excluded_occurrences.insert("evt-1on1".into());

        let ctx = OfferContext {
            app_bundle_id: Some("us.zoom.xos"),
            occurrence_external_id: Some("evt-1on1"),
        };
        assert!(!s.may_offer(&ctx));
    }

    #[test]
    fn the_off_switch_outranks_every_other_setting() {
        // Tier (a) is absolute: an empty exclusion list must not make an off feature offer.
        let s = Settings { enabled: false, ..Default::default() };
        assert!(!s.may_offer(&OfferContext::default()));
        assert!(!s.may_offer(&zoom()));
    }

    #[test]
    fn excluding_the_same_app_twice_is_harmless() {
        let mut s = Settings { enabled: true, ..Default::default() };
        s.exclude_app("us.zoom.xos");
        s.exclude_app("us.zoom.xos");
        assert_eq!(s.excluded_apps.len(), 1);
    }

    #[test]
    fn settings_survive_a_save_and_load_round_trip() {
        let mut s = Settings { enabled: true, ..Default::default() };
        s.exclude_app("us.zoom.xos");
        s.excluded_occurrences.insert("evt-1on1".into());

        let restored: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();

        assert_eq!(restored, s);
    }

    #[test]
    fn a_settings_file_that_predates_this_feature_reads_as_off() {
        // The upgrade path. An install from before meeting notes existed has no such key, and the
        // one behaviour that would be indefensible is for the update itself to switch listening
        // on (FR-MT-01). Absent means off, and it is asserted rather than assumed.
        let restored: Settings = serde_json::from_str("{}").unwrap();

        assert!(!restored.enabled);
        assert!(restored.excluded_apps.is_empty());
    }

    #[test]
    fn a_corrupt_or_partial_file_does_not_silently_enable_listening() {
        // A half-written file (power loss mid-save) must fail closed. Anything that cannot be
        // read as settings falls back to the default, which is off.
        let restored: Settings =
            serde_json::from_str(r#"{"excluded_apps":["us.zoom.xos"]}"#).unwrap();

        assert!(!restored.enabled);
        assert_eq!(restored.excluded_apps.len(), 1);
    }

    #[test]
    fn asr_model_defaults_to_small() {
        assert_eq!(AsrModel::default(), AsrModel::Small);
    }

    #[test]
    fn asr_model_round_trips_json() {
        let json = serde_json::to_string(&AsrModel::Turbo).unwrap();
        assert_eq!(json, "\"turbo\"");
        let back: AsrModel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AsrModel::Turbo);
    }
}
