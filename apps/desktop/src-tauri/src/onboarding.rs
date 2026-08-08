//! First-run Accessibility permission onboarding (Issue #46) — macOS side.
//!
//! SHOGUN is useless without Accessibility trust: it reads on-screen *text* through the
//! Accessibility API (invariant 2), and the ⌥-tap / notch-hover paths install CGEventTaps that the
//! OS silently refuses until the app is trusted (see `hover.rs`). Today a fresh install lands the
//! user in a running-but-inert app with no explanation — the离脱 point Issue #46 targets.
//!
//! This module owns the Rust half of the branded guide screen:
//! - `accessibility_status` / `onboarding_get` expose the current TCC state to the webview.
//! - `open_accessibility_settings` opens the exact System Settings pane AND registers SHOGUN in the
//!   Accessibility list (via the prompting check) so the user has a toggle to flip.
//! - a **silent** watcher polls trust while the window is open and emits `accessibility-changed` on
//!   the false→true edge, so the screen flips to its success state the instant the toggle goes on —
//!   no "restart the app" dead end.
//! - `onboarding_finish` records the outcome (completed / skipped) under the app-data dir so the
//!   guide does not reappear on every launch, and closes the window.
//!
//! Measurement stays on device (invariant 3): the funnel events are structured `eprintln!` lines,
//! never network, and never carry captured text.
//!
//! The window is a plain centered window, NOT the notch NSPanel — the panel is a 640×300
//! nonactivating overlay reparented into native AppKit (`adopt_native_panel`), the wrong surface
//! for a focused full-screen decision. Onboarding is its own webview entry (`onboarding.html`).

#[cfg(target_os = "macos")]
pub mod mac {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use serde::{Deserialize, Serialize};
    use tauri::{AppHandle, Emitter, Manager};

    use crate::axcache;

    /// Window label for the onboarding webview. Shared by the builder, the watcher (to detect the
    /// window closing) and `onboarding_finish` (to close it).
    pub const ONBOARDING_LABEL: &str = "onboarding";

    /// The exact System Settings deep link for Privacy › Accessibility. The scheme is stable across
    /// macOS 14/15; if Apple ever renames the pane, `open` still lands the user in Settings.
    const AX_SETTINGS_URL: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

    /// Persisted first-run disposition. The *outcome* only — never captured content, never the
    /// user's text. Absent file = never onboarded.
    #[derive(Clone, Copy, Default, Serialize, Deserialize)]
    struct Disposition {
        /// The user reached the granted/success state at least once.
        #[serde(default)]
        completed: bool,
        /// The user deliberately chose "later" while permission was still missing.
        #[serde(default)]
        skipped: bool,
    }

    /// Status the webview reads on load and after each action. `granted` is the live TCC state;
    /// `completed`/`skipped` are the persisted disposition.
    #[derive(Serialize, Clone)]
    pub struct OnboardingStatus {
        granted: bool,
        completed: bool,
        skipped: bool,
    }

    fn state_path(app: &AppHandle) -> Option<PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("onboarding.json"))
    }

    fn load_disposition(app: &AppHandle) -> Disposition {
        let Some(path) = state_path(app) else { return Disposition::default() };
        // A missing or unreadable file is simply "never onboarded" — never a hard error: this is a
        // guide screen, not a data-integrity surface.
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_disposition(app: &AppHandle, d: Disposition) {
        let Some(path) = state_path(app) else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string(&d) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    eprintln!("[onboarding] could not persist disposition: {e}");
                }
            }
            Err(e) => eprintln!("[onboarding] could not serialize disposition: {e}"),
        }
    }

    /// Live Accessibility trust, without the system prompt. The webview polls this as a fallback to
    /// the pushed `accessibility-changed` event.
    #[tauri::command]
    pub fn accessibility_status() -> bool {
        axcache::ax_trusted_silent()
    }

    /// Full status for the webview on load: live trust + persisted disposition.
    #[tauri::command]
    pub fn onboarding_get(app: AppHandle) -> OnboardingStatus {
        let d = load_disposition(&app);
        OnboardingStatus {
            granted: axcache::ax_trusted_silent(),
            completed: d.completed,
            skipped: d.skipped,
        }
    }

    /// Open System Settings at Privacy › Accessibility. First calls the *prompting* trust check:
    /// its side effect is to register SHOGUN in the Accessibility list, so when the pane opens there
    /// is already a SHOGUN row with a toggle to flip (otherwise the list can be empty and the step
    /// instructions have nothing to point at).
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

    /// Record the outcome and close the window. `action` is "completed" (granted) or "skipped"
    /// (chose later). Anything else is ignored so a typo can't corrupt the flag.
    #[tauri::command]
    pub fn onboarding_finish(app: AppHandle, action: String) {
        let mut d = load_disposition(&app);
        match action.as_str() {
            "completed" => d.completed = true,
            "skipped" => d.skipped = true,
            other => {
                eprintln!("[onboarding] finish ignored unknown action={other}");
                return;
            }
        }
        save_disposition(&app, d);
        eprintln!("[onboarding] finish action={action}");
        if let Some(win) = app.get_webview_window(ONBOARDING_LABEL) {
            let _ = win.close();
        }
    }

    /// A local funnel event (no content). Same spirit as the SLO recorder: measurement never leaves
    /// the device (invariant 3). A structured log line is enough to reconstruct the funnel from a
    /// dev/internal build; a real analytics sink is a follow-up.
    #[tauri::command]
    pub fn onboarding_event(name: String) {
        eprintln!("[onboarding] event={name}");
    }

    /// Poll AX trust while the onboarding window is open and emit `accessibility-changed` (a bool)
    /// on every edge. Stops the moment the window is gone. Uses the SILENT check, so this background
    /// loop can never put up the system prompt. Idempotent — a second call is a no-op while one
    /// watcher is live.
    pub fn start_watcher(app: AppHandle) {
        static RUNNING: AtomicBool = AtomicBool::new(false);
        if RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }
        std::thread::spawn(move || {
            let mut last = axcache::ax_trusted_silent();
            // Emit once up front so a window that opens already-granted (re-permission after an
            // update) renders its success state without waiting for an edge.
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

    /// Whether the first-run guide should appear at launch: Accessibility is missing AND the user
    /// has neither completed nor deliberately skipped it. Once granted, `completed` is irrelevant —
    /// a trusted process never sees the guide.
    pub fn should_show_onboarding(app: &AppHandle) -> bool {
        // Escape hatch for QA/preview: force the guide even on an already-trusted machine, so the
        // screen can be reviewed without revoking real Accessibility permission. Harmless in
        // production (nobody sets it) and never bypasses the persisted disposition below when unset.
        if std::env::var("SHOGUN_FORCE_ONBOARDING").is_ok() {
            return true;
        }
        if axcache::ax_trusted_silent() {
            return false;
        }
        let d = load_disposition(app);
        !d.completed && !d.skipped
    }
}
