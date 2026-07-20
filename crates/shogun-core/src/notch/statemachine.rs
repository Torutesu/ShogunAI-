//! Notch state machine (spec §3.3) — the single owner of UI state.
//!
//! Deterministic and time-injected: it consumes [`Input`]s (hover signals, timer
//! expiries, key/force events) and returns [`Effect`]s (state transitions, timer
//! start/cancel, mouse-passthrough toggles, the Q2 expand-commit marker). It owns no
//! real timers or threads — the macOS adapter schedules timers and feeds the expiries
//! back in. This is what makes every transition T1..T6 unit-testable off-device.

/// Timer kinds the adapter must schedule when asked (spec §3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timer {
    /// HoverIntent dwell (100ms, or 250ms fast).
    Dwell,
    /// Expanded exit grace (300ms).
    Grace,
    /// Collapse animation timeout (400ms; webview normally reports AnimDone at ~160ms).
    CollapseAnim,
}

/// The four UI states (spec §3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    HoverIntent,
    Expanded,
    Collapsing,
}

impl State {
    pub fn tag(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::HoverIntent => "hoverintent",
            State::Expanded => "expanded",
            State::Collapsing => "collapsing",
        }
    }
}

/// Inputs into the machine. Hover signals arrive from [`crate::notch::hover`]; timer expiries
/// and key/force events arrive from the macOS adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// Entered R_enter under T1 conditions. `fast` selects the longer dwell.
    HoverEnter { fast: bool },
    /// Left R_stay (T3).
    HoverExitStay,
    /// Dwell timer fired (T2).
    DwellExpired,
    /// A mouse button went down during HoverIntent (T1 note — treat as menubar intent).
    ButtonDown,
    /// Mouse left R_exp — arm the grace timer.
    ExpExit,
    /// Mouse re-entered R_exp — disarm the grace timer.
    ExpReenter,
    /// Grace timer fired (T4a).
    GraceExpired,
    /// Re-entered R_enter during Collapsing (T5).
    ReenterEnter,
    /// Esc while the panel is key (T4b).
    Esc,
    /// Click in the transparent margin (T4c).
    OutsideClick,
    /// Collapse animation reported done by the webview (T6).
    AnimDone,
    /// Collapse animation timeout — webview hang suspicion (T6 fallback).
    AnimTimeout,
    /// Display change / sleep — force collapse (T4d).
    ForceCollapse,
}

/// Side effects the adapter must apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Transition(State),
    StartTimer { timer: Timer, ms: u64 },
    CancelTimer(Timer),
    /// Toggle `ignoresMouseEvents` (spec §3.1.3): true in Idle/HoverIntent/Collapsing.
    SetIgnoresMouse(bool),
    /// The Q2 `t0` marker: dwell expiry = expand commit (spec §4.2.1).
    MarkExpandCommit,
}

/// Tunable timers (spec Appendix A).
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub dwell_ms: u64,
    pub dwell_fast_ms: u64,
    pub exit_grace_ms: u64,
    pub collapse_timeout_ms: u64,
}

impl Default for Params {
    fn default() -> Self {
        Self { dwell_ms: 100, dwell_fast_ms: 250, exit_grace_ms: 300, collapse_timeout_ms: 400 }
    }
}

/// The state machine.
#[derive(Debug)]
pub struct StateMachine {
    state: State,
    params: Params,
}

impl StateMachine {
    pub fn new(params: Params) -> Self {
        Self { state: State::Idle, params }
    }

    pub fn state(&self) -> State {
        self.state
    }

    fn go(&mut self, next: State, effects: &mut Vec<Effect>) {
        self.state = next;
        effects.push(Effect::Transition(next));
    }

    /// Apply one input, returning the effects to enact. Unhandled (input, state) pairs
    /// are no-ops (empty vec) — the machine never panics on an unexpected event.
    pub fn step(&mut self, input: Input) -> Vec<Effect> {
        use Input::*;
        use State::*;
        let mut fx = Vec::new();
        match (self.state, input) {
            // --- Idle ---
            (Idle, HoverEnter { fast }) => {
                let ms = if fast { self.params.dwell_fast_ms } else { self.params.dwell_ms };
                self.go(HoverIntent, &mut fx);
                fx.push(Effect::StartTimer { timer: Timer::Dwell, ms });
            }

            // --- HoverIntent ---
            (HoverIntent, DwellExpired) => {
                self.go(Expanded, &mut fx);
                fx.push(Effect::MarkExpandCommit);
                fx.push(Effect::SetIgnoresMouse(false));
            }
            (HoverIntent, HoverExitStay | ButtonDown | ForceCollapse) => {
                fx.push(Effect::CancelTimer(Timer::Dwell));
                self.go(Idle, &mut fx);
            }

            // --- Expanded ---
            (Expanded, ExpExit) => {
                fx.push(Effect::StartTimer { timer: Timer::Grace, ms: self.params.exit_grace_ms });
            }
            (Expanded, ExpReenter) => {
                fx.push(Effect::CancelTimer(Timer::Grace));
            }
            (Expanded, GraceExpired | Esc | OutsideClick | ForceCollapse) => {
                fx.push(Effect::CancelTimer(Timer::Grace));
                self.go(Collapsing, &mut fx);
                fx.push(Effect::SetIgnoresMouse(true));
                fx.push(Effect::StartTimer { timer: Timer::CollapseAnim, ms: self.params.collapse_timeout_ms });
            }

            // --- Collapsing ---
            (Collapsing, ReenterEnter) => {
                fx.push(Effect::CancelTimer(Timer::CollapseAnim));
                self.go(Expanded, &mut fx);
                fx.push(Effect::SetIgnoresMouse(false));
            }
            (Collapsing, AnimDone) => {
                fx.push(Effect::CancelTimer(Timer::CollapseAnim));
                self.go(Idle, &mut fx);
            }
            (Collapsing, AnimTimeout) => {
                self.go(Idle, &mut fx);
            }

            // Everything else is a deliberate no-op.
            _ => {}
        }
        fx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sm() -> StateMachine {
        StateMachine::new(Params::default())
    }

    #[test]
    fn happy_path_idle_to_expanded() {
        let mut m = sm();
        let fx = m.step(Input::HoverEnter { fast: false });
        assert_eq!(m.state(), State::HoverIntent);
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::Dwell, ms: 100 }));

        let fx = m.step(Input::DwellExpired);
        assert_eq!(m.state(), State::Expanded);
        assert!(fx.contains(&Effect::MarkExpandCommit));
        assert!(fx.contains(&Effect::SetIgnoresMouse(false)));
    }

    #[test]
    fn fast_entry_uses_longer_dwell() {
        let mut m = sm();
        let fx = m.step(Input::HoverEnter { fast: true });
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::Dwell, ms: 250 }));
    }

    #[test]
    fn exit_stay_during_hover_cancels_dwell() {
        let mut m = sm();
        m.step(Input::HoverEnter { fast: false });
        let fx = m.step(Input::HoverExitStay);
        assert_eq!(m.state(), State::Idle);
        assert!(fx.contains(&Effect::CancelTimer(Timer::Dwell)));
    }

    #[test]
    fn button_down_during_hover_returns_to_idle() {
        let mut m = sm();
        m.step(Input::HoverEnter { fast: false });
        m.step(Input::ButtonDown);
        assert_eq!(m.state(), State::Idle);
    }

    #[test]
    fn grace_reenter_keeps_expanded() {
        let mut m = sm();
        m.step(Input::HoverEnter { fast: false });
        m.step(Input::DwellExpired);
        let fx = m.step(Input::ExpExit);
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::Grace, ms: 300 }));
        assert_eq!(m.state(), State::Expanded);
        let fx = m.step(Input::ExpReenter);
        assert!(fx.contains(&Effect::CancelTimer(Timer::Grace)));
        assert_eq!(m.state(), State::Expanded);
    }

    #[test]
    fn grace_expiry_collapses_and_restores_passthrough() {
        let mut m = sm();
        m.step(Input::HoverEnter { fast: false });
        m.step(Input::DwellExpired);
        m.step(Input::ExpExit);
        let fx = m.step(Input::GraceExpired);
        assert_eq!(m.state(), State::Collapsing);
        assert!(fx.contains(&Effect::SetIgnoresMouse(true)));
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::CollapseAnim, ms: 400 }));
    }

    #[test]
    fn reenter_during_collapsing_revives_expanded() {
        let mut m = sm();
        m.step(Input::HoverEnter { fast: false });
        m.step(Input::DwellExpired);
        m.step(Input::ExpExit);
        m.step(Input::GraceExpired);
        let fx = m.step(Input::ReenterEnter);
        assert_eq!(m.state(), State::Expanded);
        assert!(fx.contains(&Effect::CancelTimer(Timer::CollapseAnim)));
        assert!(fx.contains(&Effect::SetIgnoresMouse(false)));
    }

    #[test]
    fn anim_done_returns_to_idle() {
        let mut m = sm();
        m.step(Input::HoverEnter { fast: false });
        m.step(Input::DwellExpired);
        m.step(Input::Esc);
        assert_eq!(m.state(), State::Collapsing);
        m.step(Input::AnimDone);
        assert_eq!(m.state(), State::Idle);
    }

    #[test]
    fn anim_timeout_forces_idle() {
        let mut m = sm();
        m.step(Input::HoverEnter { fast: false });
        m.step(Input::DwellExpired);
        m.step(Input::OutsideClick);
        m.step(Input::AnimTimeout);
        assert_eq!(m.state(), State::Idle);
    }

    #[test]
    fn force_collapse_from_expanded() {
        let mut m = sm();
        m.step(Input::HoverEnter { fast: false });
        m.step(Input::DwellExpired);
        let fx = m.step(Input::ForceCollapse);
        assert_eq!(m.state(), State::Collapsing);
        assert!(fx.contains(&Effect::SetIgnoresMouse(true)));
    }

    #[test]
    fn unexpected_input_is_noop() {
        let mut m = sm();
        // DwellExpired in Idle should do nothing.
        assert!(m.step(Input::DwellExpired).is_empty());
        assert_eq!(m.state(), State::Idle);
    }
}
