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

    fn first_step() -> String {
        "welcome".into()
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct OnboardingState {
        /// True once the user finished (or explicitly skipped to the end).
        #[serde(default)]
        pub completed: bool,
        /// Furthest step reached, so a quit mid-flow resumes there.
        #[serde(default = "first_step")]
        pub step: String,
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

    impl Default for OnboardingState {
        fn default() -> Self {
            Self {
                completed: false,
                step: first_step(),
                plan: None,
                trial_started_at: None,
                accessibility_skipped: false,
                permissions_repair: false,
            }
        }
    }

    /// Fold a whole-record write from the flow into the next persisted state. Pure so the
    /// trial-start stamp is testable without a real clock — `now_unix` is injected.
    ///
    /// The flow has exactly one writer and always sends the whole record, so there is no partial
    /// update to reconcile; the only derived field is `trial_started_at`, which the caller never
    /// sends.
    pub fn apply(
        prev: &OnboardingState,
        step: String,
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
        let step = if STEPS.contains(&step.as_str()) {
            step
        } else {
            first_step()
        };
        OnboardingState {
            completed,
            step,
            plan,
            trial_started_at,
            accessibility_skipped: prev.accessibility_skipped,
            permissions_repair: false,
        }
    }

    /// Whether a completed setup needs its focused permissions repair card. The legacy skip bit is
    /// retained for migrated installs that explicitly deferred the old Accessibility-only guide.
    pub fn needs_permissions_repair(all_granted: bool, state: &OnboardingState) -> bool {
        state.completed && !all_granted && !state.accessibility_skipped
    }

    /// On-disk format, versioned like `shortcuts.json` so a future default change can migrate once.
    #[derive(Serialize, Deserialize, Default)]
    pub struct OnboardingFile {
        #[serde(default)]
        pub version: u32,
        #[serde(default)]
        pub state: OnboardingState,
    }

    pub const ONBOARDING_VERSION: u32 = 1;

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
        if let Ok(file) = serde_json::from_str::<OnboardingFile>(text) {
            // The legacy file has no `version` field, which deserializes as 0 with a default
            // (first-run!) state — so version 0 falls through to the legacy branch instead of
            // silently discarding the old disposition.
            if file.version >= 1 {
                return file.state;
            }
        }
        let legacy = serde_json::from_str::<LegacyDisposition>(text).unwrap_or_default();
        if legacy.completed || legacy.skipped {
            return OnboardingState {
                completed: true,
                step: "ready".into(),
                plan: None,
                trial_started_at: None,
                accessibility_skipped: legacy.skipped,
                permissions_repair: false,
            };
        }
        OnboardingState::default()
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    mod tests {
        use super::*;

        fn done(trial: Option<i64>) -> OnboardingState {
            OnboardingState {
                completed: true,
                step: "ready".into(),
                plan: None,
                trial_started_at: trial,
                accessibility_skipped: false,
                permissions_repair: false,
            }
        }

        #[test]
        fn stamps_trial_at_first_completion() {
            let prev = OnboardingState::default();
            let next = apply(&prev, "ready".into(), Some("pro".into()), true, 1000);
            assert_eq!(next.trial_started_at, Some(1000));
        }

        #[test]
        fn no_trial_before_completion() {
            let prev = OnboardingState::default();
            let next = apply(&prev, "plan".into(), None, false, 1000);
            assert_eq!(next.trial_started_at, None);
        }

        #[test]
        fn completion_is_idempotent() {
            // A second write with completed=true must not re-stamp a later time.
            let next = apply(&done(Some(1000)), "ready".into(), None, true, 2000);
            assert_eq!(next.trial_started_at, Some(1000));
        }

        #[test]
        fn reopening_from_settings_keeps_the_trial() {
            // Settings re-runs onboarding by setting completed=false; the clock must not restart.
            let next = apply(&done(Some(1000)), "welcome".into(), None, false, 2000);
            assert_eq!(next.trial_started_at, Some(1000));
        }

        #[test]
        fn unknown_step_falls_back_to_welcome() {
            let next = apply(&OnboardingState::default(), "bogus".into(), None, false, 0);
            assert_eq!(next.step, "welcome");
        }

        #[test]
        fn parse_reads_the_versioned_format() {
            let json = r#"{"version":1,"state":{"completed":false,"step":"plan","plan":"pro"}}"#;
            let s = parse(json);
            assert!(!s.completed);
            assert_eq!(s.step, "plan");
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
            assert!(!needs_permissions_repair(false, &state));
        }

        #[test]
        fn completed_setup_repairs_lost_accessibility_trust() {
            let state = done(Some(42));
            assert!(needs_permissions_repair(false, &state));
            assert!(!needs_permissions_repair(true, &state));
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
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    use tauri::{AppHandle, Emitter, Manager};

    use super::state::{self, OnboardingFile, OnboardingState, ONBOARDING_VERSION};
    use crate::axcache;

    /// Window label for the onboarding webview. Shared by the builder, the watcher (to detect the
    /// window closing) and the completion write (to close it).
    pub const ONBOARDING_LABEL: &str = "onboarding";

    /// The exact System Settings deep link for Privacy › Accessibility. The scheme is stable across
    /// macOS 14/15; if Apple ever renames the pane, `open` still lands the user in Settings.
    const AX_SETTINGS_URL: &str =
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
    const MICROPHONE_SETTINGS_URL: &str =
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";
    const SCREEN_RECORDING_SETTINGS_URL: &str =
        "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

    /// One honest view of every capability onboarding requires. Status checks are side-effect
    /// free; only the separate request commands may prompt or open System Settings.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
    pub struct PermissionSnapshot {
        accessibility: bool,
        microphone: bool,
        screen_recording: bool,
        all_granted: bool,
    }

    impl PermissionSnapshot {
        fn new(accessibility: bool, microphone: bool, screen_recording: bool) -> Self {
            Self {
                accessibility,
                microphone,
                screen_recording,
                all_granted: accessibility && microphone && screen_recording,
            }
        }
    }

    fn permission_gate_blocks(
        previous_step: &str,
        next_step: &str,
        completed: bool,
        all_granted: bool,
    ) -> bool {
        !all_granted && (completed || (previous_step == "permission" && next_step != "permission"))
    }

    /// In-memory copy of the persisted state, managed so reads answer without touching disk on
    /// every panel launch.
    pub struct Store(pub Mutex<OnboardingState>);

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
        let Ok(text) = std::fs::read_to_string(p) else {
            return OnboardingState::default();
        };
        state::parse(&text)
    }

    fn save(app: &AppHandle, s: &OnboardingState) -> Result<(), String> {
        let p = config_path(app).ok_or_else(|| "no app data dir".to_string())?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let file = OnboardingFile {
            version: ONBOARDING_VERSION,
            state: s.clone(),
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        std::fs::write(&p, json).map_err(|e| format!("onboarding save failed: {e}"))
    }

    fn now_unix() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn microphone_authorized() -> bool {
        use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

        // SAFETY: AVMediaTypeAudio exists on every supported macOS release and is the only media
        // type passed to this API.
        let Some(media_type) = (unsafe { AVMediaTypeAudio }) else {
            return false;
        };
        (unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) })
            == AVAuthorizationStatus::Authorized
    }

    fn permission_snapshot() -> PermissionSnapshot {
        use objc2_core_graphics::CGPreflightScreenCaptureAccess;

        PermissionSnapshot::new(
            axcache::ax_trusted_silent(),
            microphone_authorized(),
            CGPreflightScreenCaptureAccess(),
        )
    }

    /// Live status for all required permissions. This command never prompts and is safe to poll.
    #[tauri::command]
    pub fn permission_status() -> PermissionSnapshot {
        permission_snapshot()
    }

    /// Current onboarding state for the flow (invariant 1: Rust owns it). Reads the managed copy
    /// when available; falls back to disk so a call racing setup still answers honestly.
    #[tauri::command]
    pub fn onboarding_state(app: AppHandle) -> OnboardingState {
        let mut state = match app.try_state::<Store>() {
            Some(store) => store.0.lock().map(|g| g.clone()).unwrap_or_default(),
            None => load(&app),
        };
        if state::needs_permissions_repair(permission_snapshot().all_granted, &state) {
            state.step = "permission".into();
            state.permissions_repair = true;
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
        step: String,
        plan: Option<String>,
        completed: bool,
        app: AppHandle,
        store: tauri::State<'_, Store>,
    ) -> Result<(), String> {
        let prev = store.0.lock().map(|g| g.clone()).unwrap_or_default();
        let permissions = permission_snapshot();
        if permission_gate_blocks(&prev.step, &step, completed, permissions.all_granted) {
            return Err(
                "Accessibility, Microphone, and Screen Recording are required to continue"
                    .to_owned(),
            );
        }
        let mut next = state::apply(&prev, step.clone(), plan, completed, now_unix());
        if permissions.all_granted {
            // A later successful grant supersedes an earlier "not now" choice, so a future
            // re-sign/revoke can surface the repair card again.
            next.accessibility_skipped = false;
        }
        save(&app, &next)?;
        let newly_completed = next.completed && !prev.completed;
        if let Ok(mut g) = store.0.lock() {
            *g = next.clone();
        }
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
        Ok(())
    }

    /// Open System Settings at Privacy › Accessibility. First calls the *prompting* trust check:
    /// its side effect is to register SHOGUN in the Accessibility list, so when the pane opens
    /// there is already a SHOGUN row with a toggle to flip (otherwise the list can be empty and
    /// the step instructions have nothing to point at). Fired once, from the button — never from
    /// the poll (which uses the silent check).
    #[tauri::command]
    pub fn open_accessibility_settings() -> Result<(), String> {
        // Register SHOGUN in the AX list (get rule; may also surface the OS alert — harmless here,
        // the user is explicitly asking to grant).
        let _ = axcache::ax_trusted();
        open_privacy_settings(AX_SETTINGS_URL, "Accessibility")
    }

    fn open_privacy_settings(url: &str, label: &str) -> Result<(), String> {
        let status = std::process::Command::new("open")
            .arg(url)
            .status()
            .map_err(|error| format!("open failed: {error}"))?;
        if !status.success() {
            return Err(format!("System Settings exited with {status}"));
        }
        eprintln!("[onboarding] opened System Settings > {label}");
        Ok(())
    }

    /// Ask for microphone access through AVFoundation. macOS only prompts while the state is not
    /// determined; denied/restricted states go straight to the exact repair pane.
    #[tauri::command]
    pub fn request_microphone_permission() -> Result<(), String> {
        use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

        // SAFETY: see `microphone_authorized`.
        let media_type = (unsafe { AVMediaTypeAudio })
            .ok_or_else(|| "AVFoundation audio media type unavailable".to_owned())?;
        let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };
        match status {
            AVAuthorizationStatus::Authorized => Ok(()),
            AVAuthorizationStatus::NotDetermined => {
                let completion = block2::RcBlock::new(|granted: objc2::runtime::Bool| {
                    eprintln!(
                        "[onboarding] microphone permission granted={}",
                        granted.as_bool()
                    );
                });
                // SAFETY: the audio media type and completion block match AVFoundation's API;
                // AVFoundation copies the block before invoking it on an arbitrary queue.
                unsafe {
                    AVCaptureDevice::requestAccessForMediaType_completionHandler(
                        media_type,
                        &completion,
                    );
                }
                Ok(())
            }
            AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => {
                open_privacy_settings(MICROPHONE_SETTINGS_URL, "Microphone")
            }
            _ => Err("unknown microphone authorization state".to_owned()),
        }
    }

    /// Request Screen Recording through CoreGraphics. A prior denial cannot prompt again, so the
    /// exact manual repair pane opens when the request does not grant access.
    #[tauri::command]
    pub fn request_screen_recording_permission() -> Result<(), String> {
        use objc2_core_graphics::{CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};

        if CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess() {
            return Ok(());
        }
        open_privacy_settings(SCREEN_RECORDING_SETTINGS_URL, "Screen Recording")
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
    pub fn start_watcher(app: AppHandle) {
        static RUNNING: AtomicBool = AtomicBool::new(false);
        if RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }
        std::thread::spawn(move || {
            let mut last = permission_snapshot();
            // Emit once up front so a window that opens during a repair renders without waiting.
            let _ = app.emit("permissions-changed", last);
            loop {
                if app.get_webview_window(ONBOARDING_LABEL).is_none() {
                    break;
                }
                let now = permission_snapshot();
                if now != last {
                    eprintln!(
                        "[onboarding] permissions accessibility={} microphone={} screen_recording={}",
                        now.accessibility, now.microphone, now.screen_recording
                    );
                    let _ = app.emit("permissions-changed", now);
                    last = now;
                }
                std::thread::sleep(Duration::from_millis(800));
            }
            RUNNING.store(false, Ordering::SeqCst);
        });
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
                start_watcher(app.clone());
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
        !state.completed
            || state::needs_permissions_repair(permission_snapshot().all_granted, &state)
    }

    #[cfg(test)]
    mod tests {
        use super::{permission_gate_blocks, PermissionSnapshot};

        #[test]
        fn snapshot_requires_every_permission() {
            assert!(PermissionSnapshot::new(true, true, true).all_granted);
            assert!(!PermissionSnapshot::new(true, false, true).all_granted);
        }

        #[test]
        fn permission_step_cannot_be_bypassed() {
            assert!(permission_gate_blocks("permission", "plan", false, false));
            assert!(permission_gate_blocks("ready", "ready", true, false));
            assert!(!permission_gate_blocks("permission", "plan", false, true));
            assert!(!permission_gate_blocks("welcome", "reads", false, false));
        }
    }
}
