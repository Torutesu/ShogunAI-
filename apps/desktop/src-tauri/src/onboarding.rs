//! First-run onboarding (issue #6), superseding the single-permission Accessibility guide
//! (issue #46) while keeping its window/watcher assets.
//!
//! The flow is six steps (welcome → reads → permission → plan → connect → ready), rendered in the
//! dedicated onboarding webview (`onboarding.html`). Everything factual the flow shows is answered
//! by Rust, not decided in the webview (invariant 1):
//! - `onboarding_state` / `set_onboarding_state` — Rust-owned progress, persisted to
//!   `app_data/onboarding.json`. The trial is stamped ONCE, at the first write that flips
//!   `completed` to true, and never restarts (see [`state::apply`]).
//! - `accessibility_status` — the NON-prompting trust check the permission step polls (a prompting
//!   check would reopen the system dialog on every poll).
//! - `open_accessibility_settings` — the one-shot prompting check + System Settings deep link,
//!   fired only from the button.
//! - a silent watcher emits `accessibility-changed` on every edge while the window is open, so the
//!   permission card flips to green the instant the toggle goes on.
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
    }

    impl Default for OnboardingState {
        fn default() -> Self {
            Self { completed: false, step: first_step(), plan: None, trial_started_at: None }
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
            prev.trial_started_at.or(if completed { Some(now_unix) } else { None });
        let step = if STEPS.contains(&step.as_str()) { step } else { first_step() };
        OnboardingState { completed, step, plan, trial_started_at }
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
        fn parse_treats_garbage_and_untouched_legacy_as_first_run() {
            for text in ["", "not json", r#"{"completed":false,"skipped":false}"#, "{}"] {
                assert_eq!(parse(text), OnboardingState::default(), "{text:?}");
            }
        }

        #[test]
        fn roundtrip_through_the_versioned_file() {
            let s = done(Some(42));
            let file = OnboardingFile { version: ONBOARDING_VERSION, state: s.clone() };
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

    /// In-memory copy of the persisted state, managed so reads answer without touching disk on
    /// every panel launch.
    pub struct Store(pub Mutex<OnboardingState>);

    fn config_path(app: &AppHandle) -> Option<PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("onboarding.json"))
    }

    /// Load persisted state, migrating the legacy #46 disposition and defaulting to first-run when
    /// the file is absent or unreadable (see [`state::parse`]).
    pub fn load(app: &AppHandle) -> OnboardingState {
        let Some(p) = config_path(app) else { return OnboardingState::default() };
        let Ok(text) = std::fs::read_to_string(p) else { return OnboardingState::default() };
        state::parse(&text)
    }

    fn save(app: &AppHandle, s: &OnboardingState) -> Result<(), String> {
        let p = config_path(app).ok_or_else(|| "no app data dir".to_string())?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let file = OnboardingFile { version: ONBOARDING_VERSION, state: s.clone() };
        let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        std::fs::write(&p, json).map_err(|e| format!("onboarding save failed: {e}"))
    }

    fn now_unix() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// Live Accessibility trust, without the system prompt. The permission step polls this as a
    /// fallback to the pushed `accessibility-changed` event.
    #[tauri::command]
    pub fn accessibility_status() -> bool {
        axcache::ax_trusted_silent()
    }

    /// Current onboarding state for the flow (invariant 1: Rust owns it). Reads the managed copy
    /// when available; falls back to disk so a call racing setup still answers honestly.
    #[tauri::command]
    pub fn onboarding_state(app: AppHandle) -> OnboardingState {
        match app.try_state::<Store>() {
            Some(store) => store.0.lock().map(|g| g.clone()).unwrap_or_default(),
            None => load(&app),
        }
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
        let next = state::apply(&prev, step, plan, completed, now_unix());
        save(&app, &next)?;
        let newly_completed = next.completed && !prev.completed;
        if let Ok(mut g) = store.0.lock() {
            *g = next.clone();
        }
        if newly_completed {
            eprintln!("[onboarding] completed (plan intent: {:?})", next.plan);
            // The one sound SHOGUN makes about itself, once in the life of an install (#49 §6.2).
            crate::sound::mac::play(&app, shogun_core::sound::Cue::OnboardingComplete);
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
        std::process::Command::new("open")
            .arg(AX_SETTINGS_URL)
            .status()
            .map_err(|e| format!("open failed: {e}"))?;
        eprintln!("[onboarding] opened System Settings › Accessibility");
        Ok(())
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
            "ax_settings_opened" => ("onboarding_ax_settings_opened", None),
            "ax_granted" => ("onboarding_ax_granted", None),
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

    /// Poll AX trust while the onboarding window is open and emit `accessibility-changed` (a bool)
    /// on every edge. Stops the moment the window is gone. Uses the SILENT check, so this
    /// background loop can never put up the system prompt. Idempotent — a second call is a no-op
    /// while one watcher is live.
    pub fn start_watcher(app: AppHandle) {
        static RUNNING: AtomicBool = AtomicBool::new(false);
        if RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }
        std::thread::spawn(move || {
            let mut last = axcache::ax_trusted_silent();
            // Emit once up front so a window that opens already-granted (re-permission after an
            // update) renders its granted state without waiting for an edge.
            let _ = app.emit("accessibility-changed", last);
            loop {
                if app.get_webview_window(ONBOARDING_LABEL).is_none() {
                    break;
                }
                let now = axcache::ax_trusted_silent();
                if now != last {
                    eprintln!("[onboarding] accessibility {last} -> {now}");
                    let _ = app.emit("accessibility-changed", now);
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
        .title("SHOGUN")
        .inner_size(720.0, 640.0)
        .min_inner_size(640.0, 560.0)
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
        const LEVEL: isize = 3; // NSFloatingWindowLevel — matches the notch overlay (OVERLAY_LEVEL).

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

    /// Whether the first-run flow should appear at launch: simply "not completed yet". Unlike the
    /// #46 guide this is NOT gated on Accessibility trust — the flow is about more than the one
    /// permission, and its permission step renders a green already-granted card when trust exists.
    /// A quit mid-flow resumes at the persisted step. Legacy #46 completed/skipped devices migrate
    /// to completed and are never re-trapped (see `state::parse`).
    pub fn should_show_onboarding(app: &AppHandle) -> bool {
        // Escape hatch for QA/preview: force the flow even on a completed machine, so the screens
        // can be reviewed without wiping app data. Harmless in production (nobody sets it).
        if std::env::var("SHOGUN_FORCE_ONBOARDING").is_ok() {
            return true;
        }
        !load(app).completed
    }
}
