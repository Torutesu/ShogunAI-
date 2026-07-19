//! Display changes, sleep/wake, health check, self-heal (spec §3.7–§3.9).
//!
//! on-device (T-12): debounce `didChangeScreenParametersNotification` (500ms), re-pick the
//! target screen (internal-first, spec §3.7.1), re-detect notch, reposition the panel, run
//! the health check, and record `event.display_change` / `event.panel_recovered`. Force-
//! collapse on `willSleepNotification`; health-check 1000ms after wake. Pseudo-notch
//! fullscreen visibility follows spec §3.8. Fullscreen-space detection has no public API
//! (research item 12) — fall back to menubar-visibility change, note SPI option in findings.
#![allow(dead_code)]
