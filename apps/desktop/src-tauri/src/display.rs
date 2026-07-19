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
pub use mac::frontmost_pid;

#[cfg(target_os = "macos")]
mod mac {
    use objc2_app_kit::NSWorkspace;

    /// PID of the frontmost application, if any. This is the focus signal that drives the
    /// context-cache walk (spec §3.10.1); on-device it is complemented by the
    /// `didActivateApplication` notification for event-driven updates.
    pub fn frontmost_pid() -> Option<i32> {
        // SAFETY: sharedWorkspace / frontmostApplication / processIdentifier are generated
        // as unsafe fns; valid to call on the main thread at any time.
        unsafe {
            let ws = NSWorkspace::sharedWorkspace();
            let app = ws.frontmostApplication()?;
            Some(app.processIdentifier())
        }
    }
}
