//! Notch UI behavioural core (spec §3.3 + §3.4 wiring, carried from the Phase 0 spike).
//!
//! - [`geometry`] — screen/notch measurements → hit regions (R_enter/R_stay/R_exp), plus the
//!   CGEvent→NSScreen coordinate conversion.
//! - [`hover`] — raw mouse samples → hover signals (early-reject, coalescing, speed/fast-dwell,
//!   menu/drag suppression).
//! - [`statemachine`] — the deterministic T1..T6 state machine (time-injected, no real timers).
//! - [`engine`] — routes hover signals and timer expiries through the tracker + machine and
//!   emits the concrete effects the macOS adapter applies.
//! - [`optiontap`] — the ⌥ double-tap trigger (FR-NU-10): modifier events → "the user meant it",
//!   which is mostly a machine for refusing to fire.

pub mod engine;
pub mod geometry;
pub mod hover;
pub mod optiontap;
pub mod statemachine;
