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

    use super::state::{self, OnboardingFile, OnboardingState, OnboardingStep, ONBOARDING_VERSION};
    use crate::permissions::PermissionSnapshot;

    /// Window label for the onboarding webview. Shared by the builder, the watcher (to detect the
    /// window closing) and the completion write (to close it).
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
    }

    /// One serialized owner holds validation, mutation, persistence, and managed state update.
    pub struct Store(Mutex<StateOwner>);

    impl Store {
        pub fn load(app: &AppHandle) -> Self {
            let path = config_path(app);
            let current = path
                .as_deref()
                .map(load_and_migrate_path)
                .unwrap_or_default();
            Self(Mutex::new(StateOwner { current, path }))
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
        let mut state = match app.try_state::<Store>() {
            Some(store) => store
                .0
                .lock()
                .map(|owner| owner.current.clone())
                .unwrap_or_default(),
            None => load(&app),
        };
        let permissions = crate::permissions::mac::status(&app);
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
            if let Some(win) = app.get_webview_window(ONBOARDING_LABEL) {
                let _ = win.close();
            }
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
        crate::permissions::mac::request_accessibility(&app)
    }

    /// Ask for microphone access through AVFoundation. macOS only prompts while the state is not
    /// determined; denied/restricted states go straight to the exact repair pane.
    #[tauri::command]
    pub fn request_microphone_permission(app: AppHandle) -> Result<(), String> {
        crate::permissions::mac::request_microphone(&app)
    }

    /// Request Screen Recording through CoreGraphics. A prior denial cannot prompt again, so the
    /// exact manual repair pane opens when the request does not grant access.
    #[tauri::command]
    pub fn request_screen_recording_permission(app: AppHandle) -> Result<(), String> {
        crate::permissions::mac::request_screen_recording(&app)
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

    /// Build the onboarding window (a plain centered window) and start the permission watcher.
    /// Idempotent: if the window already exists it is just focused.
    pub fn build_onboarding_window(app: &AppHandle) {
        if let Some(win) = app.get_webview_window(ONBOARDING_LABEL) {
            let _ = win.set_focus();
            return;
        }
        let builder = tauri::WebviewWindowBuilder::new(
            app,
            ONBOARDING_LABEL,
            tauri::WebviewUrl::App("onboarding.html".into()),
        )
        .title("ShogunAI")
        .inner_size(760.0, 700.0)
        .min_inner_size(680.0, 620.0)
        .resizable(false)
        .center()
        // SHOGUN runs as an Accessory app (prohibited activation, no Dock icon) so the notch panel
        // can float over other Spaces. A plain window from such an app builds but never comes
        // forward — it sits behind whatever is focused and the user never sees the guide (observed
        // on device). Floating level + an explicit show/focus put it in front without promoting the
        // whole app to a Regular activation policy (which would flash a Dock icon).
        .always_on_top(true)
        .focused(true);
        match builder.build() {
            Ok(win) => {
                eprintln!("[onboarding] window built");
                let _ = win.show();
                let _ = win.set_focus();
                float_over_all_spaces(&win);
                crate::permission_drag::install_monitor(app);
                if let Some(generation) = start_watcher(app.clone()) {
                    let stop_app = app.clone();
                    win.on_window_event(move |event| {
                        if matches!(event, tauri::WindowEvent::Destroyed) {
                            crate::permissions::mac::stop(&stop_app, generation);
                        }
                    });
                }
            }
            Err(e) => eprintln!("[onboarding] window build failed: {e}"),
        }
    }

    /// Make the onboarding window join every Space and float over full-screen apps — the same
    /// NSWindow recipe the notch panel uses (`float_on_all_spaces` in lib.rs), applied to the plain
    /// window directly with NO window-class swap (a swap blanks the wry webview on device). Without
    /// it the guide is invisible while a full-screen app (e.g. a full-screen video call) is
    /// frontmost — exactly when a first-run user most needs to see it (observed on device). Main
    /// thread (setup).
    fn float_over_all_spaces(win: &tauri::WebviewWindow) {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;

        // canJoinAllSpaces (1<<0) | fullScreenAuxiliary (1<<8) = 257. fullScreenAuxiliary is the
        // bit that lets a non-full-screen window appear over another app's full-screen Space.
        const BEHAVIOR: usize = (1 << 0) | (1 << 8);
        const LEVEL: isize = crate::OVERLAY_LEVEL; // mainMenu+3 — notch residency

        let ptr = match win.ns_window() {
            Ok(p) if !p.is_null() => p as *mut AnyObject,
            Ok(_) => {
                eprintln!("[onboarding] ns_window null — cannot float over all spaces");
                return;
            }
            Err(e) => {
                eprintln!("[onboarding] ns_window unavailable: {e}");
                return;
            }
        };

        // SAFETY: `ptr` is the live NSWindow owned by Tauri, messaged synchronously on the main
        // thread. Each setter takes a scalar (NSUInteger / NSInteger / BOOL) and returns void.
        unsafe {
            let _: () = msg_send![ptr, setCollectionBehavior: BEHAVIOR];
            let _: () = msg_send![ptr, setLevel: LEVEL];
            // Accessory app: without this the window is ordered out the moment the app deactivates
            // (a full-screen app staying frontmost), which is the whole bug.
            let _: () = msg_send![ptr, setHidesOnDeactivate: false];
            // Accessory apps don't auto-show their windows; force it visible even while inactive.
            let _: () = msg_send![ptr, orderFrontRegardless];
        }
        eprintln!("[onboarding] floating over all spaces (behavior={BEHAVIOR} level={LEVEL})");
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
        use super::{atomic_save_with, load_and_migrate_path, permission_gate_blocks, StateOwner};
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
        fn failed_atomic_rename_preserves_destination_and_managed_state() {
            let path = test_path("onboarding.json");
            std::fs::create_dir_all(path.parent().expect("parent")).expect("create test dir");
            std::fs::write(&path, b"last-good").expect("seed destination");
            let original = OnboardingState::default();
            let mut owner = StateOwner {
                current: original.clone(),
                path: Some(path.clone()),
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
            let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
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
    }
}
