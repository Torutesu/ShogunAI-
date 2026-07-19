//! End-to-end integration of the spike's behavioural core: a realistic mouse trajectory
//! is pushed through `HoverTracker`, its signals are routed into `StateMachine` (the same
//! mapping the macOS adapter performs), and the resulting state path is asserted. This is
//! the off-device stand-in for the S-12 expand flow — it exercises the two modules together
//! rather than in isolation.

use spike_core::geometry::{idle_rect, regions, GeometryParams, Point, Rect};
use spike_core::hover::{HoverParams, HoverSignal, HoverTracker};
use spike_core::statemachine::{Effect, Input, Params, State, StateMachine};

/// The adapter's HoverSignal → Input routing (spec §3.11 boundary). Timer expiries
/// (DwellExpired/GraceExpired/AnimDone) are injected separately, as on-device.
fn route(sig: HoverSignal, sm: &mut StateMachine) -> Vec<Effect> {
    match sig {
        HoverSignal::EnterEnter { fast } => sm.step(Input::HoverEnter { fast }),
        HoverSignal::ExitStay => sm.step(Input::HoverExitStay),
        HoverSignal::ExitExp => sm.step(Input::ExpExit),
        HoverSignal::ReenterExp => sm.step(Input::ExpReenter),
        HoverSignal::TopBandEntry => vec![], // denominator counter only
    }
}

fn setup() -> (HoverTracker, StateMachine, spike_core::geometry::Regions) {
    let screen = Rect::new(0.0, 0.0, 1512.0, 982.0);
    let idle = idle_rect(screen, 200.0, 32.0);
    let regs = regions(screen, idle, GeometryParams::default());
    let menubar_min_y = screen.max_y() - 24.0;
    (
        HoverTracker::new(regs, menubar_min_y, HoverParams::default()),
        StateMachine::new(Params::default()),
        regs,
    )
}

fn feed(h: &mut HoverTracker, sm: &mut StateMachine, p: Point, t: u64) -> Vec<Effect> {
    h.on_move(p, t, 0).into_iter().flat_map(|sig| route(sig, sm)).collect()
}

#[test]
fn full_expand_then_collapse_cycle() {
    let (mut h, mut sm, regs) = setup();
    let center = Point::new(regs.r_enter.mid_x(), regs.r_enter.y + regs.r_enter.h / 2.0);
    let below = Point::new(center.x, regs.top_band_min_y - 50.0);

    // 1) Mouse well below the notch — no state change.
    feed(&mut h, &mut sm, below, 0);
    assert_eq!(sm.state(), State::Idle);

    // 2) Move onto the notch (slowly): HoverEnter → HoverIntent, dwell timer armed.
    feed(&mut h, &mut sm, center, 500);
    assert_eq!(sm.state(), State::HoverIntent);

    // 3) Dwell fires (adapter timer) → Expanded, with the Q2 t0 marker exactly once.
    let fx = sm.step(Input::DwellExpired);
    assert_eq!(sm.state(), State::Expanded);
    assert_eq!(fx.iter().filter(|e| matches!(e, Effect::MarkExpandCommit)).count(), 1);
    assert!(fx.contains(&Effect::SetIgnoresMouse(false)));

    // 4) Move out past R_exp → grace armed; grace fires → Collapsing.
    feed(&mut h, &mut sm, below, 1000);
    assert_eq!(sm.state(), State::Expanded); // still expanded during grace
    let fx = sm.step(Input::GraceExpired);
    assert_eq!(sm.state(), State::Collapsing);
    assert!(fx.contains(&Effect::SetIgnoresMouse(true)));

    // 5) Collapse animation completes → Idle.
    sm.step(Input::AnimDone);
    assert_eq!(sm.state(), State::Idle);
}

#[test]
fn horizontal_flyby_does_not_expand() {
    // Sweeping across the menubar (not stopping on the notch) must not reach Expanded:
    // the dwell timer would be cancelled by the exit before it fires. Here we model that
    // the adapter never delivers DwellExpired because ExitStay arrives first.
    let (mut h, mut sm, regs) = setup();
    let left = Point::new(regs.r_stay.x - 60.0, regs.r_enter.y + 2.0);
    let center = Point::new(regs.r_enter.mid_x(), regs.r_enter.y + regs.r_enter.h / 2.0);
    let right = Point::new(regs.r_stay.max_x() + 60.0, regs.r_enter.y + 2.0);

    feed(&mut h, &mut sm, left, 0);
    feed(&mut h, &mut sm, center, 20); // enters → HoverIntent (dwell armed, 100ms)
    assert_eq!(sm.state(), State::HoverIntent);
    // Before 100ms elapses, the mouse has already left R_stay:
    feed(&mut h, &mut sm, right, 60);
    assert_eq!(sm.state(), State::Idle); // ExitStay cancelled the dwell
}
