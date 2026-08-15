//! NotchEngine — the integrated behavioural core (spec §3.3 + §3.4 wiring).
//!
//! This is the pure, testable heart of the adapter loop: it owns the [`HoverTracker`] and
//! [`StateMachine`], routes raw mouse samples and timer expiries through them, and emits
//! concrete [`EngineOutput`]s the macOS layer applies (schedule/cancel real timers, toggle
//! `ignoresMouseEvents`, push the webview `state` event, mark the Q2 expand commit, bump the
//! Q4 top-band counter). Keeping the routing here — including the "re-enter during Collapsing
//! becomes T5" rule and stale-timer suppression — means the integration is unit-tested off
//! device; the macOS adapter only sources events and applies outputs.

use crate::notch::geometry::{cg_to_ns, GeometryParams, Point, Rect, Regions};
use crate::notch::hover::{HoverParams, HoverSignal, HoverTracker};
use crate::notch::statemachine::{Effect, Input, Params, State, StateMachine, Timer};
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
    /// Click on the preview/panel (from the webview) → open the full panel.
    Click,
    /// Global hotkey (⌘⇧Space) → open the full panel directly.
    Hotkey,
    /// An interaction inside Expanded (from the webview) — resets the idle timeout.
    Interaction,
    /// "Open Full UI" chosen from Expanded.
    OpenFullUi,
    /// The webview reported the collapse animation done.
    AnimDone,
    /// Esc while the panel is key.
    Esc,
    /// Click in the transparent margin of the panel.
    OutsideClick,
    /// Focused app entered fullscreen (FR-NU-08).
    EnterFullscreen,
    /// Focused app left fullscreen.
    ExitFullscreen,
    /// Force collapse (display change / sleep).
    ForceCollapse,
}

/// Concrete actions the macOS adapter must apply.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EngineOutput {
    /// Push this state to the webview (`state` event) — the only UI signal.
    WebviewState(State),
    /// Toggle `ignoresMouseEvents` on the panel.
    SetIgnoresMouse(bool),
    /// Schedule a one-shot timer that fires `TimerFired(timer)` after `ms`.
    ScheduleTimer { timer: Timer, ms: u64 },
    /// Cancel a pending timer (the adapter must actually stop the underlying timer).
    CancelTimer(Timer),
    /// `t0` for the preview-open latency (Idle→Hover) — the Phase 0 Q2 measurement.
    PreviewCommit,
    /// `t0` for the full-expand latency (→Expanded) — NFR-SLO-01.
    ExpandCommit,
    /// Open the separate Full UI window.
    OpenFullUi,
    /// Q4 denominator: the pointer entered the top band.
    TopBandEntry,
    /// CGEventTap early-reject zone: height from display top, width centred on display
    /// (Idle = notch silhouette + pad; open = live panel + grace).
    HoverBand { height: f64, width: f64 },
}

/// The integrated engine. One per display where the panel appears.
pub struct NotchEngine {
    hover: HoverTracker,
    sm: StateMachine,
    /// Primary-display height for CG→NS normalisation (spec §3.4.7).
    primary_height: f64,
    /// Menubar band floor (NS y) — kept so `set_panel_hit_size` can re-apply regions.
    menubar_min_y: f64,
    /// Screen + Idle rects for rebuilding `r_exp` on open/resize.
    screen: Rect,
    idle: Rect,
    /// Last live panel size (from `set_panel_hit_size`); floors open hit region.
    last_panel_w: f64,
    last_panel_h: f64,
    /// Timers the adapter currently has scheduled — used to drop stale fires after a cancel.
    active_timers: HashSet<TimerKey>,
}

// HashSet key (Timer isn't Hash upstream; map to a small enum here).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum TimerKey {
    Dwell,
    HoverExit,
    ExpandedIdle,
    CollapseAnim,
}

impl From<Timer> for TimerKey {
    fn from(t: Timer) -> Self {
        match t {
            Timer::Dwell => TimerKey::Dwell,
            Timer::HoverExit => TimerKey::HoverExit,
            Timer::ExpandedIdle => TimerKey::ExpandedIdle,
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
        screen: Rect,
        idle: Rect,
    ) -> Self {
        Self {
            hover: HoverTracker::new(regions, menubar_min_y, hover_params),
            sm: StateMachine::new(sm_params),
            primary_height,
            menubar_min_y,
            screen,
            idle,
            last_panel_w: GeometryParams::default().expanded_w,
            last_panel_h: GeometryParams::default().expanded_h,
            active_timers: HashSet::new(),
        }
    }

    pub fn state(&self) -> State {
        self.sm.state()
    }

    /// Update regions after a display change (spec §3.7.2).
    pub fn set_regions(
        &mut self,
        regions: Regions,
        menubar_min_y: f64,
        primary_height: f64,
        screen: Rect,
        idle: Rect,
    ) {
        self.hover.set_regions(regions, menubar_min_y);
        self.menubar_min_y = menubar_min_y;
        self.primary_height = primary_height;
        self.screen = screen;
        self.idle = idle;
    }

    /// Replace `r_exp` with the live NSPanel size (open / resize). Leave-grace covers full panel.
    ///
    /// The visual frame may shrink (welded hide) below [`Self::idle`]; hit regions and the
    /// CGEventTap band must still cover the full Idle silhouette so notch hover keeps working.
    pub fn set_panel_hit_size(&mut self, panel_w: f64, panel_h: f64) {
        use crate::notch::geometry::regions_with_panel;
        let w = panel_w.max(1.0);
        let h = panel_h.max(1.0);
        self.last_panel_w = w;
        self.last_panel_h = h;
        let hit_w = w.max(self.idle.w);
        let hit_h = h.max(self.idle.h);
        let regs = regions_with_panel(self.screen, self.idle, hit_w, hit_h, GeometryParams::default());
        self.hover.set_regions(regs, self.menubar_min_y);
    }

    /// CGEventTap early-reject zone for a live panel size. Floors to the Idle silhouette when
    /// welded hide shrinks the visual frame — hover still finds the notch, but the band stays
    /// notch-wide (not a full menu-bar strip). Open panels grow the band to panel + grace.
    pub fn hover_band_cg_for_panel(&self, panel_w: f64, panel_h: f64) -> (f64, f64) {
        let p = GeometryParams::default();
        let idle_h = self.idle.h + p.enter_bottom;
        let idle_w = self.idle.w + 2.0 * p.enter_lr;
        // Height is the open signal — welded hide is 180×32 while idle silhouette is ~179×76.
        // Comparing width alone (180 > 179) wrongly inflated the band to panel+grace in Idle.
        let open = panel_h > self.idle.h;
        let h = if open {
            panel_h + p.exp_margin
        } else {
            idle_h
        };
        let w = if open {
            panel_w + 2.0 * p.exp_margin
        } else {
            idle_w
        };
        (h, w)
    }

    fn idle_hover_band(&self) -> (f64, f64) {
        let p = GeometryParams::default();
        (self.idle.h + p.enter_bottom, self.idle.w + 2.0 * p.enter_lr)
    }

    /// Grow `r_exp` to at least the last/open panel floor as soon as Hover/Expanded starts,
    /// so the cursor can enter the body before `set_panel_size` IPC arrives.
    fn ensure_open_hit_region(&mut self) {
        let p = GeometryParams::default();
        // Product chat defaults (560×360) exceed spike 400×180 — floor covers both.
        let w = self.last_panel_w.max(p.expanded_w).max(560.0);
        let h = self.last_panel_h.max(p.expanded_h).max(360.0);
        self.set_panel_hit_size(w, h);
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
                        Timer::HoverExit => Input::HoverExitExpired,
                        Timer::ExpandedIdle => Input::ExpandedIdleExpired,
                        Timer::CollapseAnim => Input::AnimTimeout,
                    };
                    self.apply_sm(sm_input, &mut out);
                }
            }
            EngineInput::Click => self.apply_sm(Input::Click, &mut out),
            EngineInput::Hotkey => self.apply_sm(Input::Hotkey, &mut out),
            EngineInput::Interaction => self.apply_sm(Input::Interaction, &mut out),
            EngineInput::OpenFullUi => self.apply_sm(Input::OpenFullUi, &mut out),
            EngineInput::AnimDone => self.apply_sm(Input::AnimDone, &mut out),
            EngineInput::Esc => self.apply_sm(Input::Esc, &mut out),
            EngineInput::OutsideClick => self.apply_sm(Input::OutsideClick, &mut out),
            EngineInput::EnterFullscreen => self.apply_sm(Input::EnterFullscreen, &mut out),
            EngineInput::ExitFullscreen => self.apply_sm(Input::ExitFullscreen, &mut out),
            EngineInput::ForceCollapse => self.apply_sm(Input::ForceCollapse, &mut out),
        }
        out
    }

    /// Route a hover signal to the state machine, interpreting the region boundaries per the
    /// current state (spec §6.1.1): the dwell (HoverIntent) is cancelled on leaving the inner
    /// R_stay, while the preview (Hover) uses the larger R_exp boundary for its leave-grace so
    /// small movements don't collapse it.
    fn route_hover(&mut self, sig: HoverSignal, out: &mut Vec<EngineOutput>) {
        let st = self.sm.state();
        match sig {
            HoverSignal::TopBandEntry => out.push(EngineOutput::TopBandEntry),
            HoverSignal::EnterEnter { fast } => {
                // Re-entering R_enter during Collapsing revives the preview (T5); while the
                // preview is up it just cancels any pending leave-grace; otherwise it's the
                // Idle→dwell trigger.
                let input = match st {
                    State::Collapsing => Input::ReenterEnter,
                    State::Hover => Input::HoverReenter,
                    _ => Input::HoverEnter { fast },
                };
                self.apply_sm(input, out);
            }
            // Leaving the inner region cancels a pending dwell; ignored once the preview is up.
            HoverSignal::ExitStay => {
                if st == State::HoverIntent {
                    self.apply_sm(Input::HoverExitStay, out);
                }
            }
            // Leaving / re-entering the outer region arms / disarms the preview leave-grace.
            HoverSignal::ExitExp => {
                if st == State::Hover {
                    self.apply_sm(Input::HoverExitStay, out);
                }
            }
            HoverSignal::ReenterExp => {
                if st == State::Hover {
                    self.apply_sm(Input::HoverReenter, out);
                }
            }
        }
    }

    fn apply_sm(&mut self, input: Input, out: &mut Vec<EngineOutput>) {
        for effect in self.sm.step(input) {
            match effect {
                Effect::Transition(s) => {
                    out.push(EngineOutput::WebviewState(s));
                    if matches!(s, State::Hover | State::Expanded) {
                        self.ensure_open_hit_region();
                        let p = GeometryParams::default();
                        out.push(EngineOutput::HoverBand {
                            height: self.last_panel_h + p.exp_margin,
                            width: self.last_panel_w + 2.0 * p.exp_margin,
                        });
                    } else if matches!(s, State::Idle | State::Hidden) {
                        // Idle: notch silhouette only (±enter pad) — not a full-width top strip.
                        let (h, w) = self.idle_hover_band();
                        out.push(EngineOutput::HoverBand { height: h, width: w });
                    }
                }
                Effect::SetIgnoresMouse(b) => out.push(EngineOutput::SetIgnoresMouse(b)),
                Effect::MarkPreviewCommit => out.push(EngineOutput::PreviewCommit),
                Effect::MarkExpandCommit => out.push(EngineOutput::ExpandCommit),
                Effect::OpenFullUi => out.push(EngineOutput::OpenFullUi),
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
    use crate::notch::geometry::{idle_rect, regions, GeometryParams, Rect};

    fn engine() -> (NotchEngine, Regions, f64) {
        // Primary display 1512×982, top-left origin at CG y=0 == NS y=982.
        let screen = Rect::new(0.0, 0.0, 1512.0, 982.0);
        let idle = idle_rect(screen, 200.0, 32.0);
        let regs = regions(screen, idle, GeometryParams::default());
        let menubar_min_y = screen.max_y() - 24.0;
        (
            NotchEngine::new(
                regs,
                menubar_min_y,
                982.0,
                HoverParams::default(),
                Params::default(),
                screen,
                idle,
            ),
            regs,
            982.0,
        )
    }

    /// Convert an NS point to the CG coordinates the engine expects as input.
    fn cg(ns_x: f64, ns_y: f64, primary_h: f64) -> (f64, f64) {
        (ns_x, primary_h - ns_y)
    }

    /// Move the pointer into R_enter (top band + dwell arm).
    fn enter_notch(e: &mut NotchEngine, regs: &Regions, h: f64, t_ms: u64) -> Vec<EngineOutput> {
        let cx = regs.r_enter.mid_x();
        let cy = regs.r_enter.y + regs.r_enter.h / 2.0;
        let (gx, gy) = cg(cx, cy, h);
        e.on_input(EngineInput::MouseCg { x: gx, y: gy, t_ms, buttons: 0 })
    }

    #[test]
    fn mouse_into_notch_schedules_dwell_then_opens_preview() {
        let (mut e, regs, h) = engine();
        let out = enter_notch(&mut e, &regs, h, 100);
        assert!(out.contains(&EngineOutput::TopBandEntry));
        assert!(out.contains(&EngineOutput::WebviewState(State::HoverIntent)));
        assert!(out.iter().any(|o| matches!(o, EngineOutput::ScheduleTimer { timer: Timer::Dwell, .. })));

        // Dwell fires → Hover(preview) + PreviewCommit + passthrough off (NOT ExpandCommit).
        let out = e.on_input(EngineInput::TimerFired(Timer::Dwell));
        assert!(out.contains(&EngineOutput::PreviewCommit));
        assert!(out.contains(&EngineOutput::WebviewState(State::Hover)));
        assert!(out.contains(&EngineOutput::SetIgnoresMouse(false)));
        assert!(!out.contains(&EngineOutput::ExpandCommit));
        assert_eq!(e.state(), State::Hover);
    }

    #[test]
    fn click_promotes_preview_to_expanded() {
        let (mut e, regs, h) = engine();
        enter_notch(&mut e, &regs, h, 100);
        e.on_input(EngineInput::TimerFired(Timer::Dwell)); // Hover
        let out = e.on_input(EngineInput::Click);
        assert!(out.contains(&EngineOutput::ExpandCommit));
        assert!(out.contains(&EngineOutput::WebviewState(State::Expanded)));
        assert!(out.iter().any(|o| matches!(o, EngineOutput::ScheduleTimer { timer: Timer::ExpandedIdle, .. })));
        assert_eq!(e.state(), State::Expanded);
    }

    #[test]
    fn hotkey_opens_expanded_directly_from_idle() {
        let (mut e, _regs, _h) = engine();
        let out = e.on_input(EngineInput::Hotkey);
        assert!(out.contains(&EngineOutput::ExpandCommit));
        assert!(out.contains(&EngineOutput::WebviewState(State::Expanded)));
        assert!(out.contains(&EngineOutput::SetIgnoresMouse(false)));
        assert_eq!(e.state(), State::Expanded);
    }

    #[test]
    fn stale_hover_exit_fire_after_cancel_is_dropped() {
        let (mut e, regs, h) = engine();
        enter_notch(&mut e, &regs, h, 100);
        e.on_input(EngineInput::TimerFired(Timer::Dwell)); // Hover(preview)

        // Leave R_exp entirely (below the open hit floor) → HoverExit grace scheduled.
        let cx = regs.r_enter.mid_x();
        // After Hover opens, r_exp grows to the product floor (~360pt); leave well below it.
        let (bx, by) = cg(cx, 100.0, h);
        let out = e.on_input(EngineInput::MouseCg { x: bx, y: by, t_ms: 200, buttons: 0 });
        assert!(out.iter().any(|o| matches!(o, EngineOutput::ScheduleTimer { timer: Timer::HoverExit, .. })));

        // Re-enter R_exp → grace cancelled.
        let out = enter_notch(&mut e, &regs, h, 250);
        assert!(out.contains(&EngineOutput::CancelTimer(Timer::HoverExit)));

        // A stale HoverExit fire now must be ignored (still Hover).
        let out = e.on_input(EngineInput::TimerFired(Timer::HoverExit));
        assert!(out.is_empty());
        assert_eq!(e.state(), State::Hover);
    }

    #[test]
    fn reenter_during_collapsing_revives_preview() {
        let (mut e, regs, h) = engine();
        enter_notch(&mut e, &regs, h, 100);
        e.on_input(EngineInput::TimerFired(Timer::Dwell)); // Hover
        // Leave R_exp + grace expiry → Collapsing.
        let cx = regs.r_enter.mid_x();
        let (bx, by) = cg(cx, 100.0, h);
        e.on_input(EngineInput::MouseCg { x: bx, y: by, t_ms: 200, buttons: 0 });
        e.on_input(EngineInput::TimerFired(Timer::HoverExit));
        assert_eq!(e.state(), State::Collapsing);

        // Re-enter R_enter → EnterEnter routed as ReenterEnter → Hover(preview).
        let out = enter_notch(&mut e, &regs, h, 300);
        assert!(out.contains(&EngineOutput::WebviewState(State::Hover)));
        assert_eq!(e.state(), State::Hover);
    }

    #[test]
    fn fullscreen_hides_and_restores() {
        let (mut e, _regs, _h) = engine();
        e.on_input(EngineInput::Hotkey); // Expanded
        let out = e.on_input(EngineInput::EnterFullscreen);
        assert!(out.contains(&EngineOutput::WebviewState(State::Hidden)));
        assert_eq!(e.state(), State::Hidden);
        let out = e.on_input(EngineInput::ExitFullscreen);
        assert!(out.contains(&EngineOutput::WebviewState(State::Idle)));
    }

    #[test]
    fn open_full_ui_emits_output() {
        let (mut e, _regs, _h) = engine();
        e.on_input(EngineInput::Hotkey); // Expanded
        let out = e.on_input(EngineInput::OpenFullUi);
        assert!(out.contains(&EngineOutput::OpenFullUi));
        assert_eq!(e.state(), State::Collapsing);
    }

    #[test]
    fn welded_hide_floors_hover_band_to_hardware_notch() {
        let screen = Rect::new(0.0, 0.0, 1512.0, 982.0);
        let idle_h = 32.0;
        let idle = idle_rect(screen, 180.0, idle_h);
        let regs = regions(screen, idle, GeometryParams::default());
        let menubar_min_y = screen.max_y() - 24.0;
        let mut e = NotchEngine::new(
            regs,
            menubar_min_y,
            982.0,
            HoverParams::default(),
            Params::default(),
            screen,
            idle,
        );
        e.set_panel_hit_size(180.0, 32.0);
        let p = GeometryParams::default();
        let (band_h, band_w) = e.hover_band_cg_for_panel(180.0, 32.0);
        assert!(band_h >= idle_h + p.enter_bottom);
        // Width stays notch-sized — must not become a full menu-bar catch strip.
        assert!(band_w <= idle.w + 2.0 * p.enter_lr + 1.0);
        // 180×32 welded hide: 180 > 179 must not trigger open-band width (212pt).
        assert!((band_w - (idle.w + 2.0 * p.enter_lr)).abs() < 1.0);

        // A point below the hardware notch must not begin hover intent.
        let cx = idle.mid_x();
        let below_notch_y = idle.y - 1.0;
        let (gx, gy) = cg(cx, below_notch_y, 982.0);
        let out = e.on_input(EngineInput::MouseCg { x: gx, y: gy, t_ms: 100, buttons: 0 });
        assert!(!out.iter().any(|o| matches!(o, EngineOutput::WebviewState(State::HoverIntent))));
    }
}
