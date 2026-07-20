//! Hover judgement (spec §3.4). Converts raw mouse samples into hit-region signals.
//!
//! This is the pure decision core that the macOS CGEventTap adapter drives. It performs
//! early-reject (top-band gate), 16ms coalescing, velocity estimation (fly-by dwell), and
//! menu/drag suppression, then emits [`HoverSignal`]s for the state machine. No OS calls,
//! no allocation-per-event beyond the returned signal vec.

use crate::notch::geometry::{Point, Regions};
use std::collections::VecDeque;

/// Signals emitted toward the state machine (spec §3.3 inputs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoverSignal {
    /// Entered R_enter under all T1 conditions. `fast` extends the dwell (spec §3.4.4).
    EnterEnter { fast: bool },
    /// Left R_stay (ends HoverIntent).
    ExitStay,
    /// Left R_exp (starts the Expanded grace timer).
    ExitExp,
    /// Re-entered R_exp (cancels the grace timer).
    ReenterExp,
    /// Crossed into the top 40pt band — the Q4 false-positive-rate denominator.
    TopBandEntry,
}

/// Tunable hover parameters (spec Appendix A).
#[derive(Clone, Copy, Debug)]
pub struct HoverParams {
    pub coalesce_ms: u64,
    pub fast_enter_pt_s: f64,
    pub velocity_window_ms: u64,
    pub menu_suppress_ms: u64,
}

impl Default for HoverParams {
    fn default() -> Self {
        Self { coalesce_ms: 16, fast_enter_pt_s: 1200.0, velocity_window_ms: 48, menu_suppress_ms: 300 }
    }
}

/// Stateful hover tracker. One instance per display where the panel can appear.
#[derive(Debug)]
pub struct HoverTracker {
    params: HoverParams,
    regions: Regions,
    /// Menubar band `[menubar_min_y, screen top]`, used only for menu suppression.
    menubar_min_y: f64,
    last_decision_ms: Option<u64>,
    history: VecDeque<(Point, u64)>,
    inside_enter: bool,
    inside_stay: bool,
    inside_exp: bool,
    in_top_band: bool,
    /// Some(t): a menubar mouse-down is suppressing hover until this time.
    suppressed_until_ms: Option<u64>,
    /// True while a menubar-down is held (cleared on up, which schedules the release grace).
    menu_down: bool,
}

impl HoverTracker {
    pub fn new(regions: Regions, menubar_min_y: f64, params: HoverParams) -> Self {
        Self {
            params,
            regions,
            menubar_min_y,
            last_decision_ms: None,
            history: VecDeque::with_capacity(8),
            inside_enter: false,
            inside_stay: false,
            inside_exp: false,
            in_top_band: false,
            suppressed_until_ms: None,
            menu_down: false,
        }
    }

    /// Update regions after a display change (spec §3.7.2).
    pub fn set_regions(&mut self, regions: Regions, menubar_min_y: f64) {
        self.regions = regions;
        self.menubar_min_y = menubar_min_y;
    }

    fn suppressed_at(&self, t_ms: u64) -> bool {
        self.menu_down || self.suppressed_until_ms.is_some_and(|until| t_ms < until)
    }

    fn velocity_pt_s(&self, now: u64) -> f64 {
        // Oldest sample within the velocity window vs the newest; distance / time.
        let cutoff = now.saturating_sub(self.params.velocity_window_ms);
        let recent: Vec<&(Point, u64)> = self.history.iter().filter(|(_, t)| *t >= cutoff).collect();
        match (recent.first(), recent.last()) {
            (Some((p0, t0)), Some((p1, t1))) if t1 > t0 => {
                let dx = p1.x - p0.x;
                let dy = p1.y - p0.y;
                let dist = (dx * dx + dy * dy).sqrt();
                dist / ((t1 - t0) as f64 / 1000.0)
            }
            _ => 0.0,
        }
    }

    /// Feed a mouse-move sample. `buttons` is the pressed-button count (drag suppression).
    pub fn on_move(&mut self, p: Point, t_ms: u64, buttons: u32) -> Vec<HoverSignal> {
        let mut out = Vec::new();

        // Early reject: below the top band, nothing near the notch can be true.
        if p.y < self.regions.top_band_min_y {
            if self.in_top_band {
                self.in_top_band = false;
            }
            out.extend(self.leave_all());
            self.history.clear();
            self.last_decision_ms = Some(t_ms);
            return out;
        }

        // Entered the top band → Q4 denominator.
        if !self.in_top_band {
            self.in_top_band = true;
            out.push(HoverSignal::TopBandEntry);
        }

        // Keep velocity history regardless of coalescing.
        self.history.push_back((p, t_ms));
        while self.history.front().is_some_and(|(_, t)| t_ms.saturating_sub(*t) > self.params.velocity_window_ms) {
            self.history.pop_front();
        }

        // Coalesce: skip region decisions if within the coalesce window.
        if let Some(last) = self.last_decision_ms {
            if t_ms.saturating_sub(last) < self.params.coalesce_ms {
                return out;
            }
        }
        self.last_decision_ms = Some(t_ms);

        let now_enter = self.regions.r_enter.contains(p);
        let now_stay = self.regions.r_stay.contains(p);
        let now_exp = self.regions.r_exp.contains(p);

        // T1 gate: enter R_enter, buttons up, not suppressed.
        if now_enter && !self.inside_enter && buttons == 0 && !self.suppressed_at(t_ms) {
            let fast = self.velocity_pt_s(t_ms) > self.params.fast_enter_pt_s;
            out.push(HoverSignal::EnterEnter { fast });
        }
        if self.inside_stay && !now_stay {
            out.push(HoverSignal::ExitStay);
        }
        if self.inside_exp && !now_exp {
            out.push(HoverSignal::ExitExp);
        } else if !self.inside_exp && now_exp {
            out.push(HoverSignal::ReenterExp);
        }

        self.inside_enter = now_enter;
        self.inside_stay = now_stay;
        self.inside_exp = now_exp;
        out
    }

    fn leave_all(&mut self) -> Vec<HoverSignal> {
        let mut out = Vec::new();
        if self.inside_stay {
            out.push(HoverSignal::ExitStay);
        }
        if self.inside_exp {
            out.push(HoverSignal::ExitExp);
        }
        self.inside_enter = false;
        self.inside_stay = false;
        self.inside_exp = false;
        out
    }

    /// Mouse button down. Sets menu suppression if it lands in the menubar band outside
    /// R_enter (spec §3.4.5). Returns no hover signals (the state machine handles ButtonDown).
    pub fn on_button_down(&mut self, p: Point, _t_ms: u64) {
        let in_menubar = p.y >= self.menubar_min_y;
        if in_menubar && !self.regions.r_enter.contains(p) {
            self.menu_down = true;
        }
    }

    /// Mouse button up. Starts the 300ms release grace for menu suppression.
    pub fn on_button_up(&mut self, t_ms: u64) {
        if self.menu_down {
            self.menu_down = false;
            self.suppressed_until_ms = Some(t_ms + self.params.menu_suppress_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notch::geometry::{idle_rect, regions, GeometryParams, Rect};

    fn setup() -> (HoverTracker, Regions, f64) {
        let screen = Rect::new(0.0, 0.0, 1512.0, 982.0);
        let idle = idle_rect(screen, 200.0, 32.0);
        let regs = regions(screen, idle, GeometryParams::default());
        let menubar_min_y = screen.max_y() - 24.0;
        (HoverTracker::new(regs, menubar_min_y, HoverParams::default()), regs, menubar_min_y)
    }

    fn center_of(r: Rect) -> Point {
        Point::new(r.mid_x(), r.y + r.h / 2.0)
    }

    #[test]
    fn below_top_band_is_early_rejected() {
        let (mut h, _, _) = setup();
        let out = h.on_move(Point::new(756.0, 100.0), 0, 0);
        assert!(out.is_empty());
    }

    #[test]
    fn entering_r_enter_emits_enter_and_top_band() {
        let (mut h, regs, _) = setup();
        let out = h.on_move(center_of(regs.r_enter), 100, 0);
        assert!(out.contains(&HoverSignal::TopBandEntry));
        assert!(out.iter().any(|s| matches!(s, HoverSignal::EnterEnter { fast: false })));
    }

    #[test]
    fn enter_is_emitted_once_not_repeatedly() {
        let (mut h, regs, _) = setup();
        let c = center_of(regs.r_enter);
        h.on_move(c, 100, 0);
        let again = h.on_move(Point::new(c.x + 1.0, c.y), 200, 0);
        assert!(!again.iter().any(|s| matches!(s, HoverSignal::EnterEnter { .. })));
    }

    #[test]
    fn coalesced_sample_skips_decision() {
        let (mut h, regs, _) = setup();
        // First sample below band to set last_decision, then two quick samples.
        h.on_move(Point::new(756.0, 900.0), 100, 0); // in band, above? 900<942 so below band-min(942). Actually establish baseline.
        let c = center_of(regs.r_enter);
        h.on_move(c, 1000, 0); // decision → enter
        let quick = h.on_move(Point::new(c.x + 5.0, c.y), 1005, 0); // within 16ms → skipped
        assert!(!quick.iter().any(|s| matches!(s, HoverSignal::ExitStay | HoverSignal::EnterEnter { .. })));
    }

    #[test]
    fn buttons_down_suppresses_enter() {
        let (mut h, regs, _) = setup();
        let out = h.on_move(center_of(regs.r_enter), 100, 1); // dragging
        assert!(!out.iter().any(|s| matches!(s, HoverSignal::EnterEnter { .. })));
    }

    #[test]
    fn menubar_click_suppresses_then_releases() {
        let (mut h, regs, menubar_min_y) = setup();
        // Click in menubar band, away from the notch.
        h.on_button_down(Point::new(200.0, menubar_min_y + 2.0), 100);
        h.on_button_up(200);
        // Within 300ms of the up: entering R_enter must NOT emit EnterEnter.
        let suppressed = h.on_move(center_of(regs.r_enter), 300, 0);
        assert!(!suppressed.iter().any(|s| matches!(s, HoverSignal::EnterEnter { .. })));
        // Move out and back after the grace: now it emits.
        h.on_move(Point::new(756.0, 100.0), 400, 0); // leave (below band)
        let allowed = h.on_move(center_of(regs.r_enter), 900, 0);
        assert!(allowed.iter().any(|s| matches!(s, HoverSignal::EnterEnter { .. })));
    }

    #[test]
    fn fast_entry_flags_fast() {
        let (mut h, regs, _) = setup();
        let c = center_of(regs.r_enter);
        // Both samples stay inside the top band (same y) so history isn't cleared; the
        // first is outside R_enter horizontally, the second at centre 20ms later (past the
        // coalesce window). 150pt in 20ms = 7500pt/s > 1200 → fast.
        h.on_move(Point::new(c.x - 150.0, c.y), 0, 0);
        let out = h.on_move(c, 20, 0);
        assert!(out.iter().any(|s| matches!(s, HoverSignal::EnterEnter { fast: true })));
    }
}
