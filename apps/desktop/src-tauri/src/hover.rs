//! Hover adapter (spec §3.4). Event-driven; polling forbidden.
//!
//! The judgement lives in `spike_core::hover::HoverTracker` (early-reject, 16ms coalesce,
//! velocity/fast-dwell, menu/drag suppression — unit-tested on Linux). on-device (T-07)
//! this module runs a listen-only CGEventTap (`kCGEventMouseMoved`, research item 2),
//! normalises each point to NS, calls `HoverTracker::on_move/on_button_*`, and forwards the
//! emitted `HoverSignal`s to `statemachine` over a channel. No allocation/log I/O in the
//! tap callback (Q3 CPU budget). Requires Accessibility permission (research item 3).
#![allow(dead_code, unused_imports)]

pub use spike_core::hover::{HoverParams, HoverSignal, HoverTracker};
