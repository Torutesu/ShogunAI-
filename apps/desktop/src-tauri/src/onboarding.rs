//! First-run onboarding (issue #6), superseding the single-permission Accessibility guide
//! (issue #46) while keeping its window/watcher assets.
//!
//! The flow is six steps (welcome → reads → permission → plan → connect → ready), rendered in the
//! dedicated onboarding webview (`onboarding.html`). Everything factual the flow shows is answered
//! by Rust, not decided in the webview (invariant 1):
//! - `onboarding_state` / `set_onboarding_state` — Rust-owned progress, persisted to
//!   `app_data/onboarding.json`. The trial is stamped ONCE, at the first write that flips
//!   `completed` to true, and never restarts (see [`state::apply`]).
//! - `permission_status` — NON-prompting Accessibility, Microphone, and Screen Recording checks.
//! - explicit request commands — prompts or opens the exact System Settings pane only after a
//!   click. Background checks never prompt.
//! - a silent watcher emits `permissions-changed` on every edge while the window is open, so all
//!   three permission lanes update immediately.
//! - `onboarding_event` — funnel measurement through the PostHog adapter (#91), behind its
//!   `opt_out` gate, names allowlisted in Rust; step ids only, never content (invariant 3).
//!
//! Plan choice recorded here is an INTENT only: it decides whether the flow asks for a BYOK key.
//! Real entitlement enforcement does not exist yet anywhere in the codebase — it must land in the
//! Rust core with billing (CLAUDE.md: plan gating is core-side), tracked as a follow-up.
//! Design record: docs/fixes/2026-07-30-onboarding-rebuild-design.md.

/// The pure onboarding state machine. Cross-platform on purpose: the trial-stamp and migration
/// rules are the part that must never regress, so their tests run on Linux CI too.
pub mod state {
    use serde::{Deserialize, Serialize};

    /// The six steps, in order. Kept in lockstep with `StepId` in
    /// `apps/desktop/src/onboarding/ipc.ts` — that file is the contract's single list.
    pub const STEPS: [&str; 6] = ["welcome", "reads", "permission", "plan", "connect", "ready"];

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum OnboardingStep {
        Intro,
        #[default]
        Welcome,
        Reads,
        Permission,
        Accessibility,
        Microphone,
        ScreenRecording,
        RightOption,
        ScribeDemo,
        DictationDemo,
        Privacy,
        Plan,
        Connect,
        Gate,
        Ready,
    }

    impl OnboardingStep {
        #[cfg(test)]
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::Intro => "intro",
                Self::Welcome => "welcome",
                Self::Reads => "reads",
                Self::Permission => "permission",
                Self::Accessibility => "accessibility",
                Self::Microphone => "microphone",
                Self::ScreenRecording => "screen_recording",
                Self::RightOption => "right_option",
                Self::ScribeDemo => "scribe_demo",
                Self::DictationDemo => "dictation_demo",
                Self::Privacy => "privacy",
                Self::Plan => "plan",
                Self::Connect => "connect",
                Self::Gate => "gate",
                Self::Ready => "ready",
            }
        }

        fn from_v1(value: &str) -> Option<Self> {
            match value {
                "welcome" => Some(Self::Welcome),
                "reads" => Some(Self::Reads),
                "permission" => Some(Self::Permission),
                "plan" => Some(Self::Plan),
                "connect" => Some(Self::Connect),
                "ready" => Some(Self::Ready),
                _ => None,
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum RestartReason {
        ScreenRecording,
    }

    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    pub struct RestartPending {
        pub reason: RestartReason,
        pub bundle_id: String,
        pub step: OnboardingStep,
    }

    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    pub struct OnboardingState {
        /// True once the user finished (or explicitly skipped to the end).
        #[serde(default)]
        pub completed: bool,
        /// Furthest step reached, so a quit mid-flow resumes there.
        #[serde(default)]
        pub step: OnboardingStep,
        /// Compare-and-set revision. Every successful persisted mutation increments exactly once.
        #[serde(default)]
        pub revision: u64,
        #[serde(default)]
        pub intro_complete: bool,
        #[serde(default)]
        pub music_muted: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub restart_pending: Option<RestartPending>,
        /// Which plan the user said they wanted ("standard" | "pro"). Billing is a separate flow;
        /// this only records the intent (plan gating itself lives in the Rust core, not here —
        /// and does not exist yet: follow-up, see module docs).
        #[serde(default)]
        pub plan: Option<String>,
        /// Unix seconds when the 7-day trial started. Per issue #6 the trial begins at onboarding
        /// COMPLETION, not first launch — stamped the first time `completed` becomes true and
        /// never moved again. Re-running onboarding from Settings sets `completed = false` but
        /// must not restart the clock. Local-only, not a secret, so no Keychain.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub trial_started_at: Option<i64>,
        /// True when the user explicitly continued past the Accessibility step without granting
        /// it. This is distinct from completing onboarding after granting permission: only the
        /// latter should reopen a repair prompt if macOS later revokes trust.
        #[serde(default)]
        pub accessibility_skipped: bool,
        /// A response-only hint for the webview. It is calculated from live permission status;
        /// the alias reads state written by the earlier Accessibility-only repair flow.
        #[serde(default, alias = "accessibility_repair")]
        pub permissions_repair: bool,
    }

    /// Fold a whole-record write from the flow into the next persisted state. Pure so the
    /// trial-start stamp is testable without a real clock — `now_unix` is injected.
    ///
    /// The flow has exactly one writer and always sends the whole record, so there is no partial
    /// update to reconcile; the only derived field is `trial_started_at`, which the caller never
    /// sends.
    pub fn apply(
        prev: &OnboardingState,
        step: OnboardingStep,
        plan: Option<String>,
        completed: bool,
        now_unix: i64,
    ) -> OnboardingState {
        // Once the trial has started it never restarts (reopening from Settings sends
        // completed=false, and losing the stamp would hand a fresh 7 days); otherwise the first
        // write that completes onboarding stamps it.
        let trial_started_at =
            prev.trial_started_at
                .or(if completed { Some(now_unix) } else { None });
        OnboardingState {
            completed,
            step,
            revision: prev.revision.saturating_add(1),
            intro_complete: prev.intro_complete,
            music_muted: prev.music_muted,
            restart_pending: prev.restart_pending.clone(),
            plan,
            trial_started_at,
            accessibility_skipped: prev.accessibility_skipped,
            permissions_repair: false,
        }
    }

    /// Whether a completed setup needs its focused permissions repair card. The legacy skip bit is
    /// retained for migrated installs that explicitly deferred the old Accessibility-only guide.
    pub fn needs_permissions_repair(
        accessibility: bool,
        microphone: bool,
        screen_recording: bool,
        state: &OnboardingState,
    ) -> bool {
        state.completed
            && ((!accessibility && !state.accessibility_skipped)
                || !microphone
                || !screen_recording)
    }

    /// On-disk format, versioned like `shortcuts.json` so a future default change can migrate once.
    #[derive(Serialize, Deserialize, Default)]
    pub struct OnboardingFile {
        #[serde(default)]
        pub version: u32,
        #[serde(default)]
        pub state: OnboardingState,
    }

    pub const ONBOARDING_VERSION: u32 = 2;

    #[derive(Deserialize, Default)]
    struct OnboardingFileV1 {
        #[serde(default)]
        version: u32,
        #[serde(default)]
        state: OnboardingStateV1,
    }

    #[derive(Deserialize, Default)]
    struct OnboardingStateV1 {
        #[serde(default)]
        completed: bool,
        #[serde(default = "v1_first_step")]
        step: String,
        #[serde(default)]
        plan: Option<String>,
        #[serde(default)]
        trial_started_at: Option<i64>,
        #[serde(default)]
        accessibility_skipped: bool,
        #[serde(default, alias = "accessibility_repair")]
        permissions_repair: bool,
    }

    fn v1_first_step() -> String {
        "welcome".to_owned()
    }

    /// The issue-#46 guide's disposition file, which lived at the same path. Its two flags meant
    /// "reached the AX-granted success screen" / "chose later".
    #[derive(Deserialize, Default)]
    struct LegacyDisposition {
        #[serde(default)]
        completed: bool,
        #[serde(default)]
        skipped: bool,
    }

    /// Parse persisted state, migrating the legacy #46 disposition in place.
    ///
    /// Migration rule (recorded in the design doc §4.1): a device that completed OR deliberately
    /// skipped the old AX guide is already using the app — re-trapping it in the full flow would
    /// punish existing users, so both map to `completed = true`. `trial_started_at` is NOT
    /// fabricated; if a later write completes onboarding again it stamps then.
    ///
    /// Anything unreadable is first-run (`Default`) — this is a guide, not a data-integrity
    /// surface.
    pub fn parse(text: &str) -> OnboardingState {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
            if let Some(version) = value.get("version") {
                let supported = version.as_u64().is_some_and(|version| {
                    version == 1 || version == u64::from(ONBOARDING_VERSION)
                });
                if !supported {
                    return OnboardingState::default();
                }
            }
        }
        if let Ok(file) = serde_json::from_str::<OnboardingFile>(text) {
            // The legacy file has no `version` field, which deserializes as 0 with a default
            // (first-run!) state — so version 0 falls through to the legacy branch instead of
            // silently discarding the old disposition.
            if file.version == ONBOARDING_VERSION {
                return file.state;
            }
        }
        if let Ok(file) = serde_json::from_str::<OnboardingFileV1>(text) {
            if file.version == 1 {
                let Some(step) = OnboardingStep::from_v1(&file.state.step) else {
                    return OnboardingState::default();
                };
                return OnboardingState {
                    completed: file.state.completed,
                    step,
                    revision: 0,
                    intro_complete: false,
                    music_muted: false,
                    restart_pending: None,
                    plan: file.state.plan,
                    trial_started_at: file.state.trial_started_at,
                    accessibility_skipped: file.state.accessibility_skipped,
                    permissions_repair: file.state.permissions_repair,
                };
            }
        }
        let legacy = serde_json::from_str::<LegacyDisposition>(text).unwrap_or_default();
        if legacy.completed || legacy.skipped {
            return OnboardingState {
                completed: true,
                step: OnboardingStep::Ready,
                revision: 0,
                intro_complete: false,
                music_muted: false,
                restart_pending: None,
                plan: None,
                trial_started_at: None,
                accessibility_skipped: legacy.skipped,
                permissions_repair: false,
            };
        }
        OnboardingState::default()
    }

    pub fn migration_needed(text: &str) -> bool {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
            return false;
        };
        match value.get("version").and_then(serde_json::Value::as_u64) {
            Some(1) => serde_json::from_value::<OnboardingFileV1>(value)
                .ok()
                .and_then(|file| OnboardingStep::from_v1(&file.state.step))
                .is_some(),
            Some(_) => false,
            None => serde_json::from_value::<LegacyDisposition>(value)
                .is_ok_and(|legacy| legacy.completed || legacy.skipped),
        }
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    mod tests {
        use super::*;

        #[test]
        fn v1_steps_migrate_without_losing_legacy_fields() {
            for step in STEPS {
                let json = format!(
                    r#"{{"version":1,"state":{{"completed":true,"step":"{step}","plan":"pro","trial_started_at":42,"accessibility_skipped":true,"permissions_repair":true}}}}"#
                );
                let state = parse(&json);
                assert_eq!(state.step.as_str(), step);
                assert!(state.completed);
                assert_eq!(state.plan.as_deref(), Some("pro"));
                assert_eq!(state.trial_started_at, Some(42));
                assert!(state.accessibility_skipped);
                assert!(state.permissions_repair);
            }
        }

        #[test]
        fn v1_records_receive_v2_defaults_and_roundtrip() {
            let state = parse(r#"{"version":1,"state":{"step":"plan"}}"#);
            assert_eq!(state.revision, 0);
            assert!(!state.intro_complete);
            assert!(!state.music_muted);
            assert_eq!(state.restart_pending, None);

            let file = OnboardingFile {
                version: ONBOARDING_VERSION,
                state: state.clone(),
            };
            let json = serde_json::to_string(&file).unwrap();
            assert_eq!(parse(&json), state);
        }

        #[test]
        fn unknown_external_step_fails_safe() {
            assert!(serde_json::from_str::<OnboardingStep>(r#""surprise""#).is_err());
            let state = parse(r#"{"version":2,"state":{"step":"surprise"}}"#);
            assert_eq!(state, OnboardingState::default());
        }

        #[test]
        fn future_file_version_is_rejected_without_downgrade() {
            for json in [
                r#"{"version":3,"state":{"step":"ready","completed":true}}"#,
                r#"{"version":3,"completed":true}"#,
                r#"{"version":"3","completed":true}"#,
            ] {
                assert_eq!(parse(json), OnboardingState::default());
            }
            assert!(!migration_needed(
                r#"{"version":3,"state":{"step":"ready","completed":true}}"#
            ));
        }

        #[test]
        fn restart_marker_roundtrip_preserves_exact_step_and_identity() {
            let state = OnboardingState {
                step: OnboardingStep::ScreenRecording,
                restart_pending: Some(RestartPending {
                    reason: RestartReason::ScreenRecording,
                    bundle_id: "com.test.app".to_owned(),
                    step: OnboardingStep::ScreenRecording,
                }),
                ..OnboardingState::default()
            };
            let json = serde_json::to_string(&OnboardingFile {
                version: ONBOARDING_VERSION,
                state,
            })
            .expect("serialize restart marker");
            let state = parse(&json);
            assert_eq!(
                state.restart_pending.as_ref().map(|marker| marker.step),
                Some(OnboardingStep::ScreenRecording)
            );
            assert_eq!(
                state
                    .restart_pending
                    .as_ref()
                    .map(|marker| marker.bundle_id.as_str()),
                Some("com.test.app")
            );
        }

        fn done(trial: Option<i64>) -> OnboardingState {
            OnboardingState {
                completed: true,
                step: OnboardingStep::Ready,
                plan: None,
                trial_started_at: trial,
                accessibility_skipped: false,
                permissions_repair: false,
                ..OnboardingState::default()
            }
        }

        #[test]
        fn stamps_trial_at_first_completion() {
            let prev = OnboardingState::default();
            let next = apply(&prev, OnboardingStep::Ready, Some("pro".into()), true, 1000);
            assert_eq!(next.trial_started_at, Some(1000));
        }

        #[test]
        fn no_trial_before_completion() {
            let prev = OnboardingState::default();
            let next = apply(&prev, OnboardingStep::Plan, None, false, 1000);
            assert_eq!(next.trial_started_at, None);
        }

        #[test]
        fn completion_is_idempotent() {
            // A second write with completed=true must not re-stamp a later time.
            let next = apply(&done(Some(1000)), OnboardingStep::Ready, None, true, 2000);
            assert_eq!(next.trial_started_at, Some(1000));
        }

        #[test]
        fn reopening_from_settings_keeps_the_trial() {
            // Settings re-runs onboarding by setting completed=false; the clock must not restart.
            let next = apply(
                &done(Some(1000)),
                OnboardingStep::Welcome,
                None,
                false,
                2000,
            );
            assert_eq!(next.trial_started_at, Some(1000));
        }

        #[test]
        fn every_successful_apply_increments_revision_once() {
            let next = apply(
                &OnboardingState::default(),
                OnboardingStep::Welcome,
                None,
                false,
                0,
            );
            assert_eq!(next.revision, 1);
        }

        #[test]
        fn parse_reads_the_versioned_format() {
            let json = r#"{"version":1,"state":{"completed":false,"step":"plan","plan":"pro"}}"#;
            let s = parse(json);
            assert!(!s.completed);
            assert_eq!(s.step, OnboardingStep::Plan);
            assert_eq!(s.plan.as_deref(), Some("pro"));
        }

        #[test]
        fn parse_migrates_the_legacy_ax_guide_disposition() {
            // #46 wrote {completed, skipped} at the same path. Either flag means the device is
            // already past first run — don't re-trap it, and don't fabricate a trial stamp.
            for legacy in [r#"{"completed":true}"#, r#"{"skipped":true}"#] {
                let s = parse(legacy);
                assert!(s.completed, "{legacy} must migrate to completed");
                assert_eq!(s.trial_started_at, None, "no fabricated trial stamp");
            }
        }

        #[test]
        fn legacy_skip_stays_out_of_accessibility_repair() {
            let state = parse(r#"{"skipped":true}"#);
            assert!(state.accessibility_skipped);
            assert!(!needs_permissions_repair(false, true, true, &state));
            assert!(needs_permissions_repair(false, false, true, &state));
            assert!(needs_permissions_repair(false, true, false, &state));
        }

        #[test]
        fn completed_setup_repairs_lost_accessibility_trust() {
            let state = done(Some(42));
            assert!(needs_permissions_repair(false, true, true, &state));
            assert!(!needs_permissions_repair(true, true, true, &state));
        }

        #[test]
        fn parse_treats_garbage_and_untouched_legacy_as_first_run() {
            for text in [
                "",
                "not json",
                r#"{"completed":false,"skipped":false}"#,
                "{}",
            ] {
                assert_eq!(parse(text), OnboardingState::default(), "{text:?}");
            }
        }

        #[test]
        fn roundtrip_through_the_versioned_file() {
            let s = done(Some(42));
            let file = OnboardingFile {
                version: ONBOARDING_VERSION,
                state: s.clone(),
            };
            let json = serde_json::to_string(&file).unwrap();
            assert_eq!(parse(&json), s);
        }
    }
}

#[cfg(target_os = "macos")]
pub mod mac {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    #[cfg(test)]
    use std::sync::atomic::AtomicU64;
    use std::sync::Mutex;

    use tauri::{AppHandle, Manager};

    use super::state::{
        self, OnboardingFile, OnboardingState, OnboardingStep, RestartPending, RestartReason,
        ONBOARDING_VERSION,
    };
    use crate::permissions::PermissionSnapshot;

    /// Stable label for interactive onboarding. Cinematic surfaces use generation-scoped labels.
    pub const ONBOARDING_LABEL: &str = "onboarding";

    fn permission_gate_blocks(
        previous_step: OnboardingStep,
        next_step: OnboardingStep,
        completed: bool,
        all_effective: bool,
    ) -> bool {
        !all_effective
            && (completed
                || (previous_step == OnboardingStep::Permission
                    && next_step != OnboardingStep::Permission))
    }

    struct StateOwner {
        current: OnboardingState,
        path: Option<PathBuf>,
        write_blocked: bool,
    }

    /// One serialized owner holds validation, mutation, persistence, and managed state update.
    pub struct Store(Mutex<StateOwner>);

    impl Store {
        pub fn load(app: &AppHandle) -> Self {
            let path = config_path(app);
            let write_blocked = path.as_deref().is_some_and(unsupported_version_at_path);
            let current = path
                .as_deref()
                .map(load_and_migrate_path)
                .unwrap_or_default();
            Self(Mutex::new(StateOwner {
                current,
                path,
                write_blocked,
            }))
        }

        pub(crate) fn snapshot(&self) -> Result<OnboardingState, String> {
            self.0
                .lock()
                .map(|owner| owner.current.clone())
                .map_err(|_| "onboarding state unavailable".to_owned())
        }

        pub(crate) fn mark_intro_complete(
            &self,
            expected_revision: u64,
        ) -> Result<OnboardingState, String> {
            let mut owner = self
                .0
                .lock()
                .map_err(|_| "onboarding state unavailable".to_owned())?;
            if owner.current.intro_complete {
                return Ok(owner.current.clone());
            }
            let mut next = owner.current.clone();
            next.revision = expected_revision
                .checked_add(1)
                .ok_or_else(|| "onboarding revision exhausted".to_owned())?;
            next.intro_complete = true;
            owner.persist(expected_revision, next)
        }
    }

    fn config_path(app: &AppHandle) -> Option<PathBuf> {
        app.path()
            .app_data_dir()
            .ok()
            .map(|d| d.join("onboarding.json"))
    }

    /// Load persisted state, migrating the legacy #46 disposition and defaulting to first-run when
    /// the file is absent or unreadable (see [`state::parse`]).
    pub fn load(app: &AppHandle) -> OnboardingState {
        let Some(p) = config_path(app) else {
            return OnboardingState::default();
        };
        load_and_migrate_path(&p)
    }

    fn load_and_migrate_path(path: &Path) -> OnboardingState {
        let Ok(text) = std::fs::read_to_string(path) else {
            return OnboardingState::default();
        };
        let parsed = state::parse(&text);
        if state::migration_needed(&text) {
            if let Err(error) = atomic_save(path, &parsed) {
                eprintln!("[onboarding] v2 migration save failed: {error}");
            }
        }
        parsed
    }

    fn unsupported_version_at_path(path: &Path) -> bool {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| value.get("version").cloned())
            .is_some_and(|version| match version.as_u64() {
                Some(1) => false,
                Some(version) if version == u64::from(ONBOARDING_VERSION) => false,
                _ => true,
            })
    }

    #[cfg(test)]
    static TEMP_NONCE: AtomicU64 = AtomicU64::new(1);

    fn serialized(s: &OnboardingState) -> Result<Vec<u8>, String> {
        let file = OnboardingFile {
            version: ONBOARDING_VERSION,
            state: s.clone(),
        };
        serde_json::to_vec_pretty(&file).map_err(|error| error.to_string())
    }

    fn random_temp_suffix() -> Result<String, String> {
        use std::fmt::Write as _;

        let mut bytes = [0_u8; 16];
        getrandom::getrandom(&mut bytes)
            .map_err(|error| format!("onboarding temp randomness failed: {error}"))?;
        let mut suffix = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(&mut suffix, "{byte:02x}")
                .map_err(|_| "onboarding temp name formatting failed".to_owned())?;
        }
        Ok(suffix)
    }

    fn atomic_save_with(
        path: &Path,
        state: &OnboardingState,
        mut next_suffix: impl FnMut() -> Result<String, String>,
        rename: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
    ) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or_else(|| "onboarding path has no parent".to_owned())?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("onboarding directory create failed: {error}"))?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "onboarding path has no file name".to_owned())?;
        let bytes = serialized(state)?;
        let mut opened = None;
        for _ in 0..8 {
            let suffix = next_suffix()?;
            let temp = parent.join(format!(".{name}.{suffix}.tmp"));
            match OpenOptions::new().write(true).create_new(true).open(&temp) {
                Ok(file) => {
                    opened = Some((temp, file));
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!("onboarding temp create failed: {error}"));
                }
            }
        }
        let Some((temp, mut file)) = opened else {
            return Err("onboarding temp create failed after collision retries".to_owned());
        };
        let result = (|| {
            file.write_all(&bytes)
                .map_err(|error| format!("onboarding temp write failed: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("onboarding temp sync failed: {error}"))?;
            rename(&temp, path)
                .map_err(|error| format!("onboarding atomic rename failed: {error}"))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result
    }

    fn atomic_save_with_rename(
        path: &Path,
        state: &OnboardingState,
        rename: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
    ) -> Result<(), String> {
        atomic_save_with(path, state, random_temp_suffix, rename)
    }

    fn atomic_save(path: &Path, state: &OnboardingState) -> Result<(), String> {
        atomic_save_with_rename(path, state, |from, to| std::fs::rename(from, to))
    }

    impl StateOwner {
        fn persist_with(
            &mut self,
            expected_revision: u64,
            next: OnboardingState,
            rename: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
        ) -> Result<OnboardingState, String> {
            if self.write_blocked {
                return Err("onboarding state was written by a newer app version".to_owned());
            }
            if self.current.revision != expected_revision {
                return Err(format!(
                    "stale onboarding revision: expected {expected_revision}, current {}",
                    self.current.revision
                ));
            }
            let next_revision = expected_revision
                .checked_add(1)
                .ok_or_else(|| "onboarding revision exhausted".to_owned())?;
            if next.revision != next_revision {
                return Err("onboarding mutation must increment revision exactly once".to_owned());
            }
            let path = self
                .path
                .as_deref()
                .ok_or_else(|| "no app data dir".to_owned())?;
            atomic_save_with_rename(path, &next, rename)?;
            self.current = next.clone();
            Ok(next)
        }

        fn persist(
            &mut self,
            expected_revision: u64,
            next: OnboardingState,
        ) -> Result<OnboardingState, String> {
            self.persist_with(expected_revision, next, |from, to| {
                std::fs::rename(from, to)
            })
        }
    }

    fn now_unix() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn validate_packaged_executable(path: &Path) -> Result<(), String> {
        let macos = path
            .parent()
            .ok_or_else(|| "restart requires a packaged macOS app".to_owned())?;
        let contents = macos
            .parent()
            .ok_or_else(|| "restart requires a packaged macOS app".to_owned())?;
        let app_bundle = contents
            .parent()
            .ok_or_else(|| "restart requires a packaged macOS app".to_owned())?;
        let valid = macos.file_name().is_some_and(|name| name == "MacOS")
            && contents.file_name().is_some_and(|name| name == "Contents")
            && app_bundle
                .extension()
                .is_some_and(|extension| extension == "app")
            && path.file_name().is_some();
        if valid {
            Ok(())
        } else {
            Err("restart requires a packaged macOS app".to_owned())
        }
    }

    pub(crate) struct RuntimeBundleIdentity {
        pub(crate) executable: PathBuf,
        pub(crate) app_bundle: PathBuf,
    }

    pub(crate) fn runtime_bundle_identity(
        app: &AppHandle,
    ) -> Result<RuntimeBundleIdentity, String> {
        use objc2_foundation::NSBundle;

        let executable = tauri::process::current_binary(&app.env())
            .map_err(|error| format!("restart executable unavailable: {error}"))?
            .canonicalize()
            .map_err(|error| format!("restart executable identity unavailable: {error}"))?;
        validate_packaged_executable(&executable)?;
        if !executable.is_file() {
            return Err("restart executable is not a file".to_owned());
        }
        let bundle = NSBundle::mainBundle();
        let actual_bundle_id = bundle
            .bundleIdentifier()
            .map(|value| value.to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "restart bundle identifier unavailable".to_owned())?;
        if actual_bundle_id != app.config().identifier {
            return Err("restart bundle identifier does not match this build".to_owned());
        }
        let bundle_executable = bundle
            .executableURL()
            .and_then(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()))
            .ok_or_else(|| "restart bundle executable unavailable".to_owned())?
            .canonicalize()
            .map_err(|error| format!("restart bundle executable identity unavailable: {error}"))?;
        if bundle_executable != executable {
            return Err("restart executable does not match the running app bundle".to_owned());
        }
        let app_bundle = bundle
            .bundleURL()
            .path()
            .map(|path| PathBuf::from(path.to_string()))
            .ok_or_else(|| "restart bundle path unavailable".to_owned())?
            .canonicalize()
            .map_err(|error| format!("restart bundle identity unavailable: {error}"))?;
        let executable_bundle = executable
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or_else(|| "restart requires a packaged macOS app".to_owned())?;
        if app_bundle != executable_bundle || !app_bundle.is_dir() {
            return Err("restart bundle does not contain the running executable".to_owned());
        }
        Ok(RuntimeBundleIdentity {
            executable,
            app_bundle,
        })
    }

    fn runtime_bundle_executable(
        app: &AppHandle,
        expected_bundle_id: &str,
    ) -> Result<PathBuf, String> {
        if expected_bundle_id != app.config().identifier {
            return Err("restart bundle identifier does not match this build".to_owned());
        }
        Ok(runtime_bundle_identity(app)?.executable)
    }

    fn restart_marker(
        current: &OnboardingState,
        expected_revision: u64,
        step: OnboardingStep,
        bundle_id: &str,
        executable: &Path,
    ) -> Result<OnboardingState, String> {
        if current.revision != expected_revision {
            return Err(format!(
                "stale onboarding revision: expected {expected_revision}, current {}",
                current.revision
            ));
        }
        if step != OnboardingStep::ScreenRecording || current.step != step {
            return Err("restart is only available on the active Screen Recording step".to_owned());
        }
        if bundle_id.trim().is_empty() {
            return Err("restart requires a bundle identifier".to_owned());
        }
        validate_packaged_executable(executable)?;
        let mut next = current.clone();
        next.revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| "onboarding revision exhausted".to_owned())?;
        next.restart_pending = Some(RestartPending {
            reason: RestartReason::ScreenRecording,
            bundle_id: bundle_id.to_owned(),
            step,
        });
        Ok(next)
    }

    fn consume_restart_marker(
        current: &OnboardingState,
        bundle_id: &str,
        screen_recording: bool,
    ) -> Option<OnboardingState> {
        let marker = current.restart_pending.as_ref()?;
        let matches = marker.reason == RestartReason::ScreenRecording
            && marker.bundle_id == bundle_id
            && marker.step == current.step
            && screen_recording;
        if !matches {
            return None;
        }
        let mut next = current.clone();
        next.revision = current.revision.checked_add(1)?;
        next.restart_pending = None;
        Some(next)
    }

    fn launch_then_exit_with(
        executable: &Path,
        arguments: &[std::ffi::OsString],
        spawn: impl FnOnce(&Path, &[std::ffi::OsString]) -> std::io::Result<()>,
        exit: impl FnOnce(),
    ) -> Result<(), String> {
        spawn(executable, arguments).map_err(|error| format!("restart launch failed: {error}"))?;
        exit();
        Ok(())
    }

    fn clear_failed_restart_marker(
        owner: &mut StateOwner,
        marker_revision: u64,
    ) -> Result<(), String> {
        if owner.current.revision != marker_revision || owner.current.restart_pending.is_none() {
            return Ok(());
        }
        let mut next = owner.current.clone();
        next.revision = marker_revision
            .checked_add(1)
            .ok_or_else(|| "onboarding revision exhausted".to_owned())?;
        next.restart_pending = None;
        owner.persist(marker_revision, next)?;
        Ok(())
    }

    /// Live status for all required permissions. This command never prompts and is safe to poll.
    #[tauri::command]
    pub fn permission_status(app: AppHandle) -> PermissionSnapshot {
        crate::permissions::mac::status(&app)
    }

    #[tauri::command]
    pub fn permission_listener_ready(app: AppHandle) -> PermissionSnapshot {
        crate::permissions::mac::listener_ready(&app)
    }

    /// Current onboarding state for the flow (invariant 1: Rust owns it). Reads the managed copy
    /// when available; falls back to disk so a call racing setup still answers honestly.
    #[tauri::command]
    pub fn onboarding_state(app: AppHandle) -> OnboardingState {
        let permissions = crate::permissions::mac::status(&app);
        let mut state = match app.try_state::<Store>() {
            Some(store) => store
                .0
                .lock()
                .map(|owner| owner.current.clone())
                .unwrap_or_default(),
            None => load(&app),
        };
        let needs_repair = state::needs_permissions_repair(
            permissions.accessibility,
            permissions.microphone,
            permissions.screen_recording,
            &state,
        );
        state.permissions_repair = needs_repair;
        if needs_repair {
            state.step = OnboardingStep::Permission;
        }
        state
    }

    /// Called only after the relaunched frontend has rendered the exact saved step. Reading state
    /// alone cannot consume the marker because that would lose recovery context before the user
    /// sees it. Fresh native permission truth remains the final gate.
    #[tauri::command]
    pub fn acknowledge_onboarding_restart(
        expected_revision: u64,
        step: OnboardingStep,
        app: AppHandle,
        store: tauri::State<'_, Store>,
    ) -> Result<OnboardingState, String> {
        let permissions = crate::permissions::mac::status(&app);
        let mut owner = store
            .0
            .lock()
            .map_err(|_| "onboarding state unavailable".to_owned())?;
        if owner.current.revision != expected_revision {
            return Err(format!(
                "stale onboarding revision: expected {expected_revision}, current {}",
                owner.current.revision
            ));
        }
        if owner.current.step != step {
            return Err("restart acknowledgement does not match the rendered step".to_owned());
        }
        let next = consume_restart_marker(
            &owner.current,
            &app.config().identifier,
            permissions.screen_recording,
        )
        .ok_or_else(|| "restart still requires matching Screen Recording access".to_owned())?;
        owner.persist(expected_revision, next)
    }

    /// Persist exact resume context, fence active text/audio delivery, then ask Tauri to relaunch
    /// the signed app bundle. Development binaries are intentionally rejected: macOS permission
    /// identity is bundle/code-signature scoped and restarting a loose executable would test the
    /// wrong identity.
    #[tauri::command]
    pub fn restart_onboarding(
        expected_revision: u64,
        step: OnboardingStep,
        app: AppHandle,
        store: tauri::State<'_, Store>,
    ) -> Result<(), String> {
        let executable = runtime_bundle_executable(&app, &app.config().identifier)?;
        {
            let owner = store
                .0
                .lock()
                .map_err(|_| "onboarding state unavailable".to_owned())?;
            let _ = restart_marker(
                &owner.current,
                expected_revision,
                step,
                &app.config().identifier,
                &executable,
            )?;
        }

        crate::scribe::mac::cancel_active_for_restart(&app)?;
        crate::voice_session::mac::cancel_for_restart(&app)?;
        crate::permission_drag::cleanup(&app, false);

        let marker_revision = {
            let mut owner = store
                .0
                .lock()
                .map_err(|_| "onboarding state unavailable".to_owned())?;
            let next = restart_marker(
                &owner.current,
                expected_revision,
                step,
                &app.config().identifier,
                &executable,
            )?;
            owner.persist(expected_revision, next)?.revision
        };
        crate::onboarding_windows::mac::cleanup(&app);
        let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
        let launch = launch_then_exit_with(
            &executable,
            &arguments,
            |path, args| {
                std::process::Command::new(path)
                    .args(args)
                    .spawn()
                    .map(|_| ())
            },
            || app.exit(0),
        );
        if let Err(launch_error) = launch {
            let rollback = store
                .0
                .lock()
                .map_err(|_| "onboarding state unavailable after restart failure".to_owned())
                .and_then(|mut owner| clear_failed_restart_marker(&mut owner, marker_revision));
            return match rollback {
                Ok(()) => Err(launch_error),
                Err(rollback_error) => Err(format!("{launch_error}; {rollback_error}")),
            };
        }
        Ok(())
    }

    /// Whole-record write — the flow has one writer, so a partial update would let a resumed
    /// session disagree with itself. Folds in the derived `trial_started_at` (see
    /// [`state::apply`]), persists, updates the managed copy, and — on the write that completes
    /// the flow — closes the onboarding window and fires the completion funnel event.
    ///
    /// Agent-side symmetry (invariant 6): `device.onboarding.get` exists on the MCP/REST/CLI
    /// surface; serving this live value there is a shared-store follow-up (design doc §4.4).
    #[tauri::command]
    pub fn set_onboarding_state(
        expected_revision: u64,
        step: OnboardingStep,
        plan: Option<String>,
        completed: bool,
        app: AppHandle,
        store: tauri::State<'_, Store>,
    ) -> Result<OnboardingState, String> {
        let mut owner = store
            .0
            .lock()
            .map_err(|_| "onboarding state unavailable".to_owned())?;
        if owner.current.revision != expected_revision {
            return Err(format!(
                "stale onboarding revision: expected {expected_revision}, current {}",
                owner.current.revision
            ));
        }
        let permissions = crate::permissions::mac::status(&app);
        if permission_gate_blocks(
            owner.current.step,
            step,
            completed,
            permissions.all_effective,
        ) {
            return Err(
                "Accessibility, Microphone, and Screen Recording are required to continue"
                    .to_owned(),
            );
        }
        let prev = owner.current.clone();
        let mut next = state::apply(&prev, step, plan, completed, now_unix());
        if permissions.all_effective {
            // A later successful grant supersedes an earlier "not now" choice, so a future
            // re-sign/revoke can surface the repair card again.
            next.accessibility_skipped = false;
        }
        let next = owner.persist(expected_revision, next)?;
        drop(owner);
        if prev.step != next.step {
            crate::permission_drag::cleanup(&app, true);
        }
        let newly_completed = next.completed && !prev.completed;
        if newly_completed {
            eprintln!("[onboarding] completed (plan intent: {:?})", next.plan);
            // The one sound SHOGUN makes about itself, once in the life of an install (#49 §6.2).
            crate::sound::mac::play(shogun_core::sound::Cue::OnboardingComplete);
            // Funnel: completion, plan intent only — never content (#91 gate applies).
            if let Some(analytics) = app.try_state::<crate::analytics::Analytics>() {
                let mut p = shogun_core::analytics::Props::new();
                p.insert(
                    "plan_intent".into(),
                    serde_json::Value::from(next.plan.as_deref().unwrap_or("undecided")),
                );
                analytics.capture("onboarding_completed", p);
            }
        }
        // Close on ANY completing write, not only the first — a re-run (Settings, or the
        // SHOGUN_FORCE_ONBOARDING QA hatch on an already-completed machine) must still be able to
        // leave through the same door.
        if next.completed {
            crate::permission_drag::cleanup(&app, true);
            crate::onboarding_windows::mac::cleanup(&app);
        }
        Ok(next)
    }

    /// Open System Settings at Privacy › Accessibility. First calls the *prompting* trust check:
    /// its side effect is to register SHOGUN in the Accessibility list, so when the pane opens
    /// there is already a SHOGUN row with a toggle to flip (otherwise the list can be empty and
    /// the step instructions have nothing to point at). Fired once, from the button — never from
    /// the poll (which uses the silent check).
    #[tauri::command]
    pub fn open_accessibility_settings(app: AppHandle) -> Result<(), String> {
        crate::permission_drag::perform_permission_action(
            &app,
            crate::onboarding_windows::ExternalPermissionKind::Accessibility,
            || crate::permissions::mac::request_accessibility(&app),
        )
    }

    /// Ask for microphone access through AVFoundation. macOS only prompts while the state is not
    /// determined; denied/restricted states go straight to the exact repair pane.
    #[tauri::command]
    pub fn request_microphone_permission(app: AppHandle) -> Result<(), String> {
        crate::permission_drag::perform_permission_action(
            &app,
            crate::onboarding_windows::ExternalPermissionKind::Microphone,
            || crate::permissions::mac::request_microphone(&app),
        )
    }

    /// Request Screen Recording through CoreGraphics. A prior denial cannot prompt again, so the
    /// exact manual repair pane opens when the request does not grant access.
    #[tauri::command]
    pub fn request_screen_recording_permission(app: AppHandle) -> Result<(), String> {
        crate::permission_drag::perform_permission_action(
            &app,
            crate::onboarding_windows::ExternalPermissionKind::ScreenRecording,
            || crate::permissions::mac::request_screen_recording(&app),
        )
    }

    /// A funnel event from the flow. The webview passes a short name; Rust maps it onto an
    /// allowlisted PostHog event (so arbitrary webview strings never become event names) and the
    /// #91 adapter's `opt_out` gate decides whether anything is sent. Step ids only, no content
    /// (invariant 3). The local log line stays — it is the on-device funnel for dev builds.
    #[tauri::command]
    pub fn onboarding_event(name: String, app: AppHandle) {
        eprintln!("[onboarding] event={name}");
        let (event, step): (&str, Option<&str>) = match name.as_str() {
            "shown" => ("onboarding_shown", None),
            s if state::STEPS.contains(&s) => ("onboarding_step_viewed", Some(s)),
            "accessibility_settings_opened" => ("onboarding_accessibility_settings_opened", None),
            "microphone_requested" => ("onboarding_microphone_requested", None),
            "screen_recording_requested" => ("onboarding_screen_recording_requested", None),
            "permission_app_drag_started" => ("onboarding_permission_app_drag_started", None),
            "all_permissions_granted" => ("onboarding_all_permissions_granted", None),
            other => {
                eprintln!("[onboarding] event ignored (not allowlisted): {other}");
                return;
            }
        };
        if let Some(analytics) = app.try_state::<crate::analytics::Analytics>() {
            let mut p = shogun_core::analytics::Props::new();
            if let Some(s) = step {
                p.insert("step".into(), serde_json::Value::from(s));
            }
            analytics.capture(event, p);
        }
    }

    /// Poll all required permissions while onboarding is open and emit a snapshot on every edge.
    /// Every status API is non-prompting. Idempotent while one watcher is live.
    pub fn start_watcher(app: AppHandle) -> Option<u64> {
        crate::permissions::mac::start(app)
    }

    /// Start or focus the native generation-owned onboarding window session.
    pub fn build_onboarding_window(app: &AppHandle) {
        if let Err(error) = crate::onboarding_windows::mac::start(app) {
            eprintln!("[onboarding] window session failed: {error}");
        }
    }

    /// Whether to open onboarding. First-run resumes normally. A completed setup that loses any
    /// required permission opens the focused repair card; legacy explicit deferrals remain
    /// respected.
    pub fn should_show_onboarding(app: &AppHandle) -> bool {
        // Escape hatch for QA/preview: force the flow even on a completed machine, so the screens
        // can be reviewed without wiping app data. Harmless in production (nobody sets it).
        if std::env::var("SHOGUN_FORCE_ONBOARDING").is_ok() {
            return true;
        }
        let state = load(app);
        let permissions = crate::permissions::mac::status(app);
        !state.completed
            || state::needs_permissions_repair(
                permissions.accessibility,
                permissions.microphone,
                permissions.screen_recording,
                &state,
            )
    }

    #[cfg(test)]
    mod tests {
        use super::{
            atomic_save_with, clear_failed_restart_marker, consume_restart_marker,
            launch_then_exit_with, load_and_migrate_path, permission_gate_blocks, restart_marker,
            unsupported_version_at_path, validate_packaged_executable, StateOwner, Store,
        };
        use crate::onboarding::state::{self, OnboardingState, OnboardingStep};

        fn test_path(name: &str) -> std::path::PathBuf {
            let nonce = super::TEMP_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            std::env::temp_dir()
                .join(format!(
                    "shogun-onboarding-test-{}-{nonce}",
                    std::process::id()
                ))
                .join(name)
        }

        #[test]
        fn stale_revision_is_rejected_and_success_increments_once() {
            let path = test_path("onboarding.json");
            let mut owner = StateOwner {
                current: OnboardingState::default(),
                path: Some(path.clone()),
                write_blocked: false,
            };
            let next = state::apply(&owner.current, OnboardingStep::Welcome, None, false, 0);
            let saved = owner.persist(0, next).expect("first save");
            assert_eq!(saved.revision, 1);

            let stale = state::apply(
                &OnboardingState::default(),
                OnboardingStep::Reads,
                None,
                false,
                0,
            );
            assert!(owner.persist(0, stale).is_err());
            assert_eq!(owner.current.revision, 1);
            let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
        }

        #[test]
        fn intro_completion_uses_store_revision_and_persists_once() {
            let path = test_path("onboarding.json");
            let store = Store(std::sync::Mutex::new(StateOwner {
                current: OnboardingState::default(),
                path: Some(path.clone()),
                write_blocked: false,
            }));
            let saved = store.mark_intro_complete(0).expect("intro save");
            assert!(saved.intro_complete);
            assert_eq!(saved.revision, 1);
            let same = store.mark_intro_complete(1).expect("idempotent intro save");
            assert_eq!(same.revision, 1);
            let loaded = load_and_migrate_path(&path);
            assert!(loaded.intro_complete);
            let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
        }

        #[test]
        fn failed_atomic_rename_preserves_destination_and_managed_state() {
            let path = test_path("onboarding.json");
            std::fs::create_dir_all(path.parent().expect("parent")).expect("create test dir");
            std::fs::write(&path, b"last-good").expect("seed destination");
            let original = OnboardingState::default();
            let mut owner = StateOwner {
                current: original.clone(),
                path: Some(path.clone()),
                write_blocked: false,
            };
            let next = state::apply(&original, OnboardingStep::Reads, None, false, 0);
            let result = owner.persist_with(0, next, |_from, _to| {
                Err(std::io::Error::other("injected rename failure"))
            });
            assert!(result.is_err());
            assert_eq!(owner.current, original);
            assert_eq!(
                std::fs::read(&path).expect("read destination"),
                b"last-good"
            );
            let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
        }

        #[test]
        fn store_load_durably_migrates_v1_to_v2() {
            let path = test_path("onboarding.json");
            std::fs::create_dir_all(path.parent().expect("parent")).expect("create test dir");
            std::fs::write(
                &path,
                r#"{"version":1,"state":{"step":"plan","plan":"pro","trial_started_at":42}}"#,
            )
            .expect("seed v1");
            let state = load_and_migrate_path(&path);
            assert_eq!(state.step, OnboardingStep::Plan);
            let written: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).expect("read migrated file"))
                    .expect("parse migrated file");
            assert_eq!(written["version"], 2);
            let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
        }

        #[test]
        fn store_load_rejects_future_version_without_rewriting_it() {
            let path = test_path("onboarding.json");
            std::fs::create_dir_all(path.parent().expect("parent")).expect("create test dir");
            let future = r#"{"version":3,"state":{"step":"ready","completed":true,"future":42}}"#;
            std::fs::write(&path, future).expect("seed future file");
            assert_eq!(load_and_migrate_path(&path), OnboardingState::default());
            assert_eq!(
                std::fs::read_to_string(&path).expect("future remains"),
                future
            );
            assert!(unsupported_version_at_path(&path));
            let mut owner = StateOwner {
                current: OnboardingState::default(),
                path: Some(path.clone()),
                write_blocked: true,
            };
            let next = state::apply(&owner.current, OnboardingStep::Welcome, None, false, 0);
            assert!(owner.persist(0, next).is_err());
            assert_eq!(
                std::fs::read_to_string(&path).expect("future still remains"),
                future
            );
            let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
        }

        #[test]
        fn malformed_versioned_state_is_read_only() {
            for version in [r#""3""#, "null", "-1"] {
                let path = test_path("onboarding.json");
                std::fs::create_dir_all(path.parent().expect("parent")).expect("create test dir");
                let record = format!(r#"{{"version":{version},"state":{{"step":"ready"}}}}"#);
                std::fs::write(&path, &record).expect("seed malformed version");
                assert!(unsupported_version_at_path(&path));
                assert_eq!(std::fs::read_to_string(&path).expect("unchanged"), record);
                let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
            }
        }

        #[test]
        fn atomic_save_retries_stale_temp_name_collision() {
            let path = test_path("onboarding.json");
            let parent = path.parent().expect("parent");
            std::fs::create_dir_all(parent).expect("create test dir");
            let stale = parent.join(".onboarding.json.collision.tmp");
            std::fs::write(&stale, b"stale").expect("seed collision");
            let mut suffixes = ["collision", "fresh"].into_iter();
            atomic_save_with(
                &path,
                &OnboardingState::default(),
                || Ok(suffixes.next().expect("retry suffix").to_owned()),
                |from, to| std::fs::rename(from, to),
            )
            .expect("save after collision");
            assert_eq!(std::fs::read(stale).expect("stale remains"), b"stale");
            assert!(path.exists());
            let _ = std::fs::remove_dir_all(parent);
        }

        #[test]
        fn permission_step_cannot_be_bypassed() {
            assert!(permission_gate_blocks(
                OnboardingStep::Permission,
                OnboardingStep::Plan,
                false,
                false
            ));
            assert!(permission_gate_blocks(
                OnboardingStep::Ready,
                OnboardingStep::Ready,
                true,
                false
            ));
            assert!(!permission_gate_blocks(
                OnboardingStep::Permission,
                OnboardingStep::Plan,
                false,
                true
            ));
            assert!(!permission_gate_blocks(
                OnboardingStep::Welcome,
                OnboardingStep::Reads,
                false,
                false
            ));
        }

        #[test]
        fn restart_requires_current_screen_step_and_revision() {
            let current = OnboardingState {
                step: OnboardingStep::ScreenRecording,
                revision: 7,
                ..OnboardingState::default()
            };
            let executable =
                std::path::Path::new("/Applications/ShogunAI.app/Contents/MacOS/ShogunAI");
            assert!(
                restart_marker(&current, 6, current.step, "com.shogun.ai", executable).is_err()
            );
            assert!(restart_marker(
                &current,
                7,
                OnboardingStep::Microphone,
                "com.shogun.ai",
                executable
            )
            .is_err());
        }

        #[test]
        fn restart_marker_preserves_exact_identity_and_step() {
            let current = OnboardingState {
                step: OnboardingStep::ScreenRecording,
                revision: 2,
                ..OnboardingState::default()
            };
            let next = restart_marker(
                &current,
                2,
                OnboardingStep::ScreenRecording,
                "com.shogun.ai",
                std::path::Path::new("/Applications/ShogunAI.app/Contents/MacOS/ShogunAI"),
            )
            .expect("valid restart marker");
            let marker = next.restart_pending.expect("marker");
            assert_eq!(next.revision, 3);
            assert_eq!(marker.bundle_id, "com.shogun.ai");
            assert_eq!(marker.step, OnboardingStep::ScreenRecording);
        }

        #[test]
        fn restart_rejects_loose_executable() {
            assert!(validate_packaged_executable(std::path::Path::new("/tmp/shogun")).is_err());
            assert!(validate_packaged_executable(std::path::Path::new(
                "/Applications/ShogunAI.app/Contents/MacOS/ShogunAI"
            ))
            .is_ok());
        }

        #[test]
        fn restart_marker_consumes_only_after_matching_grant() {
            let current = restart_marker(
                &OnboardingState {
                    step: OnboardingStep::ScreenRecording,
                    revision: 0,
                    ..OnboardingState::default()
                },
                0,
                OnboardingStep::ScreenRecording,
                "com.shogun.ai",
                std::path::Path::new("/Applications/ShogunAI.app/Contents/MacOS/ShogunAI"),
            )
            .expect("marker");
            assert!(consume_restart_marker(&current, "wrong.bundle", true).is_none());
            assert!(consume_restart_marker(&current, "com.shogun.ai", false).is_none());
            let consumed = consume_restart_marker(&current, "com.shogun.ai", true)
                .expect("matching marker consumed");
            assert!(consumed.restart_pending.is_none());
            assert_eq!(consumed.revision, 2);
        }

        #[test]
        fn restart_launch_failure_does_not_exit_current_process() {
            let exited = std::cell::Cell::new(false);
            let result = launch_then_exit_with(
                std::path::Path::new("/Applications/ShogunAI.app/Contents/MacOS/ShogunAI"),
                &[],
                |_path, _args| Err(std::io::Error::other("injected spawn failure")),
                || exited.set(true),
            );
            assert!(result.is_err());
            assert!(!exited.get());
        }

        #[test]
        fn restart_exits_only_after_successful_spawn() {
            let spawned = std::cell::Cell::new(false);
            let exited = std::cell::Cell::new(false);
            launch_then_exit_with(
                std::path::Path::new("/Applications/ShogunAI.app/Contents/MacOS/ShogunAI"),
                &[std::ffi::OsString::from("--test")],
                |_path, args| {
                    assert_eq!(args, [std::ffi::OsString::from("--test")]);
                    spawned.set(true);
                    Ok(())
                },
                || {
                    assert!(spawned.get());
                    exited.set(true);
                },
            )
            .expect("launch succeeds");
            assert!(exited.get());
        }

        #[test]
        fn failed_restart_clears_persisted_marker_and_keeps_store_usable() {
            let path = test_path("onboarding.json");
            let current = restart_marker(
                &OnboardingState {
                    step: OnboardingStep::ScreenRecording,
                    revision: 0,
                    ..OnboardingState::default()
                },
                0,
                OnboardingStep::ScreenRecording,
                "com.shogun.ai",
                std::path::Path::new("/Applications/ShogunAI.app/Contents/MacOS/ShogunAI"),
            )
            .expect("marker");
            let mut owner = StateOwner {
                current: OnboardingState {
                    step: OnboardingStep::ScreenRecording,
                    ..OnboardingState::default()
                },
                path: Some(path.clone()),
                write_blocked: false,
            };
            owner.persist(0, current).expect("persist marker");
            clear_failed_restart_marker(&mut owner, 1).expect("clear marker");
            assert_eq!(owner.current.revision, 2);
            assert!(owner.current.restart_pending.is_none());
            let disk = load_and_migrate_path(&path);
            assert_eq!(disk, owner.current);
            let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
        }
    }
}
