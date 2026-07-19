//! Display changes, sleep/wake, health check, self-heal (spec §3.7–§3.9) and frontmost-app
//! tracking that feeds the context cache.
//!
//! on-device (T-12): debounce `didChangeScreenParametersNotification` (500ms), re-pick the
//! target screen (internal-first, spec §3.7.1), re-detect notch, reposition the panel, run
//! the health check, and record `event.display_change` / `event.panel_recovered`. Force-
//! collapse on `willSleepNotification`; health-check 1000ms after wake. Pseudo-notch
//! fullscreen visibility follows spec §3.8. Fullscreen-space detection has no public API
//! (research item 12) — fall back to menubar-visibility change, note SPI option in findings.
#![allow(dead_code, unused_imports)]

#[cfg(target_os = "macos")]
pub use mac::{frontmost_app, frontmost_pid, FrontApp};

#[cfg(target_os = "macos")]
mod mac {
    use objc2_app_kit::NSWorkspace;

    /// Frontmost-app identity for the context cache (spec §3.10.1): pid + bundle id +
    /// localized name. Bundle id / name are best-effort (empty when the app exposes none).
    pub struct FrontApp {
        pub pid: i32,
        pub bundle_id: String,
        pub name: String,
    }

    /// PID of the frontmost application, if any. This is the focus signal that drives the
    /// context-cache walk (spec §3.10.1); on-device it is complemented by the
    /// `didActivateApplication` notification for event-driven updates.
    pub fn frontmost_pid() -> Option<i32> {
        // These NSWorkspace accessors are safe fns in objc2-app-kit 0.3.2.
        let ws = NSWorkspace::sharedWorkspace();
        let app = ws.frontmostApplication()?;
        Some(app.processIdentifier())
    }

    /// Frontmost app with its bundle id and localized name — the focus-watcher's input.
    pub fn frontmost_app() -> Option<FrontApp> {
        let ws = NSWorkspace::sharedWorkspace();
        let app = ws.frontmostApplication()?;
        // NSString → Rust String via Display (lossy UTF-8); default to empty when absent.
        let bundle_id = app.bundleIdentifier().map(|s| s.to_string()).unwrap_or_default();
        let name = app.localizedName().map(|s| s.to_string()).unwrap_or_default();
        Some(FrontApp { pid: app.processIdentifier(), bundle_id, name })
    }
}
