//! End-to-end integration of the behavioural core: a realistic mouse trajectory is pushed
//! through the whole `NotchEngine` (HoverTracker → state-aware routing → StateMachine), the
//! same path the macOS adapter drives, and the resulting state sequence is asserted. This is
//! the off-device stand-in for the expand flow — it exercises the modules together under the
//! two-level open model (spec §6.1.1) rather than in isolation.

use shogun_core::notch::engine::{EngineInput, EngineOutput, NotchEngine};
use shogun_core::notch::geometry::{idle_rect, regions, GeometryParams, Rect, Regions};
use shogun_core::notch::hover::HoverParams;
use shogun_core::notch::statemachine::{Params, State, Timer};

fn engine() -> (NotchEngine, Regions, f64) {
    let screen = Rect::new(0.0, 0.0, 1512.0, 982.0);
    let idle = idle_rect(screen, 200.0, 32.0);
    let regs = regions(screen, idle, GeometryParams::default());
    let menubar_min_y = screen.max_y() - 24.0;
    let primary_h = 982.0;
    (
        NotchEngine::new(
            regs,
            menubar_min_y,
            primary_h,
            HoverParams::default(),
            Params::default(),
            screen,
            idle,
        ),
        regs,
        primary_h,
    )
}

/// NS→CG (top-left) conversion the adapter feeds the engine.
fn cg(ns_x: f64, ns_y: f64, primary_h: f64) -> (f64, f64) {
    (ns_x, primary_h - ns_y)
}

fn move_to(e: &mut NotchEngine, ns_x: f64, ns_y: f64, primary_h: f64, t_ms: u64) -> Vec<EngineOutput> {
    let (x, y) = cg(ns_x, ns_y, primary_h);
    e.on_input(EngineInput::MouseCg { x, y, t_ms, buttons: 0 })
}

#[test]
fn full_preview_expand_then_collapse_cycle() {
    let (mut e, regs, h) = engine();
    let cx = regs.r_enter.mid_x();
    let cy = regs.r_enter.y + regs.r_enter.h / 2.0;
    let below_y = 100.0; // well below open r_exp floor (~360pt)

    // 1) Below the notch — Idle.
    move_to(&mut e, cx, below_y, h, 0);
    assert_eq!(e.state(), State::Idle);

    // 2) Onto the notch → HoverIntent (dwell armed).
    move_to(&mut e, cx, cy, h, 500);
    assert_eq!(e.state(), State::HoverIntent);

    // 3) Dwell fires → Hover(preview) with the preview commit exactly once.
    let out = e.on_input(EngineInput::TimerFired(Timer::Dwell));
    assert_eq!(e.state(), State::Hover);
    assert_eq!(out.iter().filter(|o| matches!(o, EngineOutput::PreviewCommit)).count(), 1);
    assert!(out.contains(&EngineOutput::SetIgnoresMouse(false)));

    // 4) Click promotes the preview to the full panel (SLO-01 commit).
    let out = e.on_input(EngineInput::Click);
    assert_eq!(e.state(), State::Expanded);
    assert!(out.contains(&EngineOutput::ExpandCommit));

    // 5) Esc → Collapsing; anim done → Idle.
    let out = e.on_input(EngineInput::Esc);
    assert_eq!(e.state(), State::Collapsing);
    assert!(out.contains(&EngineOutput::SetIgnoresMouse(true)));
    e.on_input(EngineInput::AnimDone);
    assert_eq!(e.state(), State::Idle);
}

#[test]
fn preview_then_leave_collapses_without_click() {
    let (mut e, regs, h) = engine();
    let cx = regs.r_enter.mid_x();
    let cy = regs.r_enter.y + regs.r_enter.h / 2.0;
    let below_y = 100.0; // well below open r_exp floor (~360pt)

    move_to(&mut e, cx, cy, h, 500);
    e.on_input(EngineInput::TimerFired(Timer::Dwell));
    assert_eq!(e.state(), State::Hover);

    // Leave the outer region → HoverExit grace; grace fires → Collapsing.
    move_to(&mut e, cx, below_y, h, 1000);
    assert_eq!(e.state(), State::Hover); // still previewing during the grace
    e.on_input(EngineInput::TimerFired(Timer::HoverExit));
    assert_eq!(e.state(), State::Collapsing);
}

#[test]
fn horizontal_flyby_does_not_open_preview() {
    // Sweeping across the menubar (not stopping on the notch) must not reach Hover: the
    // dwell is cancelled by the exit before it fires. We model that the adapter never
    // delivers DwellExpired because ExitStay arrives first.
    let (mut e, regs, h) = engine();
    let left = (regs.r_stay.x - 60.0, regs.r_enter.y + 2.0);
    let center = (regs.r_enter.mid_x(), regs.r_enter.y + regs.r_enter.h / 2.0);
    let right = (regs.r_stay.max_x() + 60.0, regs.r_enter.y + 2.0);

    move_to(&mut e, left.0, left.1, h, 0);
    move_to(&mut e, center.0, center.1, h, 20); // enters → HoverIntent
    assert_eq!(e.state(), State::HoverIntent);
    // Before the dwell elapses, the mouse has already left R_stay:
    move_to(&mut e, right.0, right.1, h, 60);
    assert_eq!(e.state(), State::Idle); // ExitStay cancelled the dwell
}
