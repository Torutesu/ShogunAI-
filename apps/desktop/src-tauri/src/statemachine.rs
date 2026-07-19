//! Notch state machine — the single owner of UI state (spec §3.3).
//!
//! State and timers live here, in Rust, never in the webview (CLAUDE.md: data centre of
//! gravity is the Rust core). `hover` feeds this over a channel; this notifies `ipc`.
//! Transitions T1..T6 and their timer values are defined in spec §3.3 / Appendix A and
//! must come from a config struct, not hardcoded magic numbers (dev-instructions §5.6).
#![allow(dead_code)]

/// The five states of spec §3.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    HoverIntent,
    Expanded,
    Collapsing,
}

impl State {
    /// Lowercase tag sent to the webview `state` event (spec §3.11.2).
    pub fn tag(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::HoverIntent => "hoverintent",
            State::Expanded => "expanded",
            State::Collapsing => "collapsing",
        }
    }
}

/// Tunable timers/thresholds (spec Appendix A). Centralised so the Go/No-Go retry loop
/// (spec §6.4) can vary them without code changes.
#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub dwell_ms: u64,
    pub dwell_fast_ms: u64,
    pub fast_enter_pt_s: f64,
    pub expand_anim_ms: u64,
    pub collapse_anim_ms: u64,
    pub collapse_timeout_ms: u64,
    pub exit_grace_ms: u64,
}

impl Default for Params {
    fn default() -> Self {
        // Spec Appendix A baseline values.
        Self {
            dwell_ms: 100,
            dwell_fast_ms: 250,
            fast_enter_pt_s: 1200.0,
            expand_anim_ms: 120,
            collapse_anim_ms: 160,
            collapse_timeout_ms: 400,
            exit_grace_ms: 300,
        }
    }
}

// on-device (T-08): transition engine driving `ipc` + `panel.set_ignores_mouse_events`,
// with every transition recorded to the harness as `event.state_transition`.
