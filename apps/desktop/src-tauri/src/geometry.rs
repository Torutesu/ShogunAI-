//! Notch / pseudo-notch detection and hit-region math (spec §3.2, §3.4.2, §3.4.7).
//!
//! on-device (T-06): read `NSScreen.safeAreaInsets.top` and `auxiliaryTopLeftArea` /
//! `auxiliaryTopRightArea` (research item 4: `NSRect?`, nil when no notch, bottom-left
//! origin) to size the notch; derive R_enter/R_stay/R_exp. All coordinates are normalised
//! to NS (bottom-left) at the module boundary so CGEventTap's top-left coords can't leak
//! a sign bug into Q4 (spec §3.4.7) — normalisation gets unit tests on-device.
#![allow(dead_code)]
