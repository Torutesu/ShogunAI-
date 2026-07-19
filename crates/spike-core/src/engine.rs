//! NotchEngine — the integrated behavioural core (spec §3.3 + §3.4 wiring).
//!
//! This is the pure, testable heart of the adapter loop: it owns the [`HoverTracker`] and
//! [`StateMachine`], routes raw mouse samples and timer expiries through them, and emits
//! concrete [`EngineOutput`]s the macOS layer applies (schedule/cancel real timers, toggle
//! `ignoresMouseEvents`, push the webview `state` event, mark the Q2 expand commit, bump the
//! Q4 top-band counter). Keeping the routing here — including the "re-enter during Collapsing
//! becomes T5" rule and stale-timer suppression — means the integration is unit-tested off
//! device; the macOS adapter only sources events and applies outputs.

use crate::geometry::{cg_to_ns, Point, Regions};
use crate::hover::{HoverParams, HoverSignal, HoverTracker};
use crate::statemachine::{Effect, Input, Params, State, StateMachine, Timer};
use std::collections::HashSet;

/// Events fed into the engine by the macOS adapter.
#[derive(Clone, Copy, Debug)]
pub enum EngineInput {
    /// A raw mouse-move sample in CGEvent (top-left) coordinates.
    MouseCg { x: f64, y: f64, t_ms: u64, buttons: u32 },
    /// Mouse button down in CGEvent coordinates (menu-suppression / drag).
    ButtonDownCg { x: f64, y: f64, t_ms: u64 },
    /// Mouse button up.
    ButtonUp { t_ms: u64 },
    /// A scheduled timer fired.
    TimerFired(Timer),
    /// The webview reported the collapse animation done (T6).
    AnimDone,
    /// Esc while the panel is key (T4b).
    Esc,
    /// Force collapse (display change / sleep, T4d).
    ForceCollapse,
}

/// Concrete actions the macOS adapter must apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineOutput {
    /// Push this state to the webview (`state` event) — the only UI signal.
    WebviewState(State),
    /// Toggle `ignoresMouseEvents` on the panel.
    SetIgnoresMouse(bool),
    /// Schedule a one-shot timer that fires `TimerFired(timer)` after `ms`.
    ScheduleTimer { timer: Timer, ms: u64 },
    /// Cancel a pending timer (the adapter must actually stop the underlying timer).
    CancelTimer(Timer),
    /// Q2 `t0`: record the expand-commit instant to the harness.
    ExpandCommit,
    /// Q4 denominator: the pointer entered the top band.
    TopBandEntry,
}

/// The integrated engine. One per display where the panel appears.
pub struct NotchEngine {
    hover: HoverTracker,
    sm: StateMachine,
    /// Primary-display height for CG→NS normalisation (spec §3.4.7).
    primary_height: f64,
    /// Timers the adapter currently has scheduled — used to drop stale fires after a cancel.
    active_timers: HashSet<TimerKey>,
}

// HashSet key (Timer isn't Hash upstream; map to a small enum here).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum TimerKey {
    Dwell,
    Grace,
    CollapseAnim,
}

impl From<Timer> for TimerKey {
    fn from(t: Timer) -> Self {
        match t {
            Timer::Dwell => TimerKey::Dwell,
            Timer::Grace => TimerKey::Grace,
            Timer::CollapseAnim => TimerKey::CollapseAnim,
        }
    }
}

impl NotchEngine {
    pub fn new(
        regions: Regions,
        menubar_min_y: f64,
        primary_height: f64,
        hover_params: HoverParams,
        sm_params: Params,
    ) -> Self {
        Self {
            hover: HoverTracker::new(regions, menubar_min_y, hover_params),
            sm: StateMachine::new(sm_params),
            primary_height,
            active_timers: HashSet::new(),
        }
    }

    pub fn state(&self) -> State {
        self.sm.state()
    }

    /// Update regions after a display change (spec §3.7.2).
    pub fn set_regions(&mut self, regions: Regions, menubar_min_y: f64, primary_height: f64) {
        self.hover.set_regions(regions, menubar_min_y);
        self.primary_height = primary_height;
    }

    /// Feed one input; returns the outputs to apply, in order.
    pub fn on_input(&mut self, input: EngineInput) -> Vec<EngineOutput> {
        let mut out = Vec::new();
        match input {
            EngineInput::MouseCg { x, y, t_ms, buttons } => {
                let ns = cg_to_ns(Point::new(x, y), self.primary_height);
                for sig in self.hover.on_move(ns, t_ms, buttons) {
                    self.route_hover(sig, &mut out);
                }
            }
            EngineInput::ButtonDownCg { x, y, t_ms } => {
                let ns = cg_to_ns(Point::new(x, y), self.primary_height);
                self.hover.on_button_down(ns, t_ms);
                // A button press during HoverIntent cancels it (T1 note / menubar intent).
                self.apply_sm(Input::ButtonDown, &mut out);
            }
            EngineInput::ButtonUp { t_ms } => {
                self.hover.on_button_up(t_ms);
            }
            EngineInput::TimerFired(t) => {
                // Drop stale fires for timers cancelled/rescheduled since (spec robustness).
                if self.active_timers.remove(&TimerKey::from(t)) {
                    let sm_input = match t {
                        Timer::Dwell => Input::DwellExpired,
                        Timer::Grace => Input::GraceExpired,
                        Timer::CollapseAnim => Input::AnimTimeout,
                    };
                    self.apply_sm(sm_input, &mut out);
                }
            }
            EngineInput::AnimDone => self.apply_sm(Input::AnimDone, &mut out),
            EngineInput::Esc => self.apply_sm(Input::Esc, &mut out),
            EngineInput::ForceCollapse => self.apply_sm(Input::ForceCollapse, &mut out),
        }
        out
    }

    fn route_hover(&mut self, sig: HoverSignal, out: &mut Vec<EngineOutput>) {
        match sig {
            HoverSignal::TopBandEntry => out.push(EngineOutput::TopBandEntry),
            HoverSignal::EnterEnter { fast } => {
                // Re-entering R_enter during Collapsing revives Expanded (T5); otherwise T1.
                let input = if self.sm.state() == State::Collapsing {
                    Input::ReenterEnter
                } else {
                    Input::HoverEnter { fast }
                };
                self.apply_sm(input, out);
            }
            HoverSignal::ExitStay => self.apply_sm(Input::HoverExitStay, out),
            HoverSignal::ExitExp => self.apply_sm(Input::ExpExit, out),
            HoverSignal::ReenterExp => self.apply_sm(Input::ExpReenter, out),
        }
    }

    fn apply_sm(&mut self, input: Input, out: &mut Vec<EngineOutput>) {
        for effect in self.sm.step(input) {
            match effect {
                Effect::Transition(s) => out.push(EngineOutput::WebviewState(s)),
                Effect::SetIgnoresMouse(b) => out.push(EngineOutput::SetIgnoresMouse(b)),
                Effect::MarkExpandCommit => out.push(EngineOutput::ExpandCommit),
                Effect::StartTimer { timer, ms } => {
                    self.active_timers.insert(TimerKey::from(timer));
                    out.push(EngineOutput::ScheduleTimer { timer, ms });
                }
                Effect::CancelTimer(timer) => {
                    self.active_timers.remove(&TimerKey::from(timer));
                    out.push(EngineOutput::CancelTimer(timer));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{idle_rect, regions, GeometryParams, Rect};

    fn engine() -> (NotchEngine, Regions, f64) {
        // Primary display 1512×982, top-left origin at CG y=0 == NS y=982.
        let screen = Rect::new(0.0, 0.0, 1512.0, 982.0);
        let idle = idle_rect(screen, 200.0, 32.0);
        let regs = regions(screen, idle, GeometryParams::default());
        let menubar_min_y = screen.max_y() - 24.0;
        (
            NotchEngine::new(regs, menubar_min_y, 982.0, HoverParams::default(), Params::default()),
            regs,
            982.0,
        )
    }

    /// Convert an NS point to the CG coordinates the engine expects as input.
    fn cg(ns_x: f64, ns_y: f64, primary_h: f64) -> (f64, f64) {
        (ns_x, primary_h - ns_y)
    }

    #[test]
    fn mouse_into_notch_schedules_dwell_then_expands() {
        let (mut e, regs, h) = engine();
        let cx = regs.r_enter.mid_x();
        let cy = regs.r_enter.y + regs.r_enter.h / 2.0;
        let (gx, gy) = cg(cx, cy, h);

        let out = e.on_input(EngineInput::MouseCg { x: gx, y: gy, t_ms: 100, buttons: 0 });
        assert!(out.contains(&EngineOutput::TopBandEntry));
        assert!(out.contains(&EngineOutput::WebviewState(State::HoverIntent)));
        assert!(out.iter().any(|o| matches!(o, EngineOutput::ScheduleTimer { timer: Timer::Dwell, .. })));

        // Dwell fires → Expanded + ExpandCommit + passthrough off.
        let out = e.on_input(EngineInput::TimerFired(Timer::Dwell));
        assert!(out.contains(&EngineOutput::ExpandCommit));
        assert!(out.contains(&EngineOutput::WebviewState(State::Expanded)));
        assert!(out.contains(&EngineOutput::SetIgnoresMouse(false)));
        assert_eq!(e.state(), State::Expanded);
    }

    #[test]
    fn stale_grace_fire_after_cancel_is_dropped() {
        let (mut e, regs, h) = engine();
        let cx = regs.r_enter.mid_x();
        let cy = regs.r_enter.y + regs.r_enter.h / 2.0;
        let (gx, gy) = cg(cx, cy, h);
        e.on_input(EngineInput::MouseCg { x: gx, y: gy, t_ms: 100, buttons: 0 });
        e.on_input(EngineInput::TimerFired(Timer::Dwell)); // Expanded

        // Leave R_exp → grace scheduled.
        let (bx, by) = cg(cx, regs.top_band_min_y - 50.0, h); // below band → exits
        let out = e.on_input(EngineInput::MouseCg { x: bx, y: by, t_ms: 200, buttons: 0 });
        assert!(out.iter().any(|o| matches!(o, EngineOutput::ScheduleTimer { timer: Timer::Grace, .. })));

        // Re-enter R_exp → grace cancelled.
        let out = e.on_input(EngineInput::MouseCg { x: gx, y: gy, t_ms: 250, buttons: 0 });
        assert!(out.contains(&EngineOutput::CancelTimer(Timer::Grace)));

        // A stale Grace fire now must be ignored (still Expanded).
        let out = e.on_input(EngineInput::TimerFired(Timer::Grace));
        assert!(out.is_empty());
        assert_eq!(e.state(), State::Expanded);
    }

    #[test]
    fn reenter_during_collapsing_revives_expanded() {
        let (mut e, regs, h) = engine();
        let cx = regs.r_enter.mid_x();
        let cy = regs.r_enter.y + regs.r_enter.h / 2.0;
        let (gx, gy) = cg(cx, cy, h);
        e.on_input(EngineInput::MouseCg { x: gx, y: gy, t_ms: 100, buttons: 0 });
        e.on_input(EngineInput::TimerFired(Timer::Dwell));
        // Exit + grace expiry → Collapsing.
        let (bx, by) = cg(cx, regs.top_band_min_y - 50.0, h);
        e.on_input(EngineInput::MouseCg { x: bx, y: by, t_ms: 200, buttons: 0 });
        e.on_input(EngineInput::TimerFired(Timer::Grace));
        assert_eq!(e.state(), State::Collapsing);

        // Re-enter R_enter → EnterEnter routed as ReenterEnter → Expanded.
        let out = e.on_input(EngineInput::MouseCg { x: gx, y: gy, t_ms: 300, buttons: 0 });
        assert!(out.contains(&EngineOutput::WebviewState(State::Expanded)));
        assert_eq!(e.state(), State::Expanded);
    }
}
