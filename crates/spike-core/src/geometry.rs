//! Notch/pseudo-notch geometry and hit-region math (spec §3.2, §3.4.2, §3.4.7).
//!
//! All rectangles use NS coordinates: origin bottom-left, `(x, y)` is the bottom-left
//! corner, so `max_y = y + h` is the top edge. macOS supplies the raw screen/notch
//! measurements; every derivation and hit-test lives here where it is unit-tested.

/// A point in NS coordinates (bottom-left origin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A rectangle in NS coordinates. `(x, y)` is the bottom-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn max_x(&self) -> f64 {
        self.x + self.w
    }
    pub fn max_y(&self) -> f64 {
        self.y + self.h
    }
    pub fn mid_x(&self) -> f64 {
        self.x + self.w / 2.0
    }

    /// Inclusive-min, exclusive-max containment (`[x, x+w) × [y, y+h)`).
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.max_x() && p.y >= self.y && p.y < self.max_y()
    }

    /// Grow each edge independently (used for the asymmetric R_enter expansion).
    pub fn expand(&self, left: f64, right: f64, bottom: f64, top: f64) -> Rect {
        Rect::new(self.x - left, self.y - bottom, self.w + left + right, self.h + bottom + top)
    }

    /// Grow all four edges by `d` (hysteresis / grace rings).
    pub fn inset_all(&self, d: f64) -> Rect {
        self.expand(d, d, d, d)
    }
}

/// The three hit-regions the state machine reasons about (spec §3.4.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Regions {
    /// Entry region — crossing in can start HoverIntent.
    pub r_enter: Rect,
    /// Stay region — R_enter grown by the hysteresis margin; leaving it ends HoverIntent.
    pub r_stay: Rect,
    /// Expanded region — the visible panel grown by the grace margin.
    pub r_exp: Rect,
    /// The top band (`max_y - band`) for the early-reject fast path (spec §3.4.1).
    pub top_band_min_y: f64,
}

/// Fixed geometry parameters (spec Appendix A). Overridable for the Q4 retry loop.
#[derive(Clone, Copy, Debug)]
pub struct GeometryParams {
    pub enter_lr: f64,
    pub enter_bottom: f64,
    pub stay_hysteresis: f64,
    pub exp_margin: f64,
    pub expanded_w: f64,
    pub expanded_h: f64,
    pub top_band: f64,
}

impl Default for GeometryParams {
    fn default() -> Self {
        Self {
            enter_lr: 8.0,
            enter_bottom: 4.0,
            stay_hysteresis: 4.0,
            exp_margin: 16.0,
            expanded_w: 400.0,
            expanded_h: 180.0,
            top_band: 40.0,
        }
    }
}

/// The Idle "silhouette" rect: the real notch, or the 180×menubar pseudo-notch,
/// anchored top-centre on `screen` (spec §3.2.1, §3.2.2).
pub fn idle_rect(screen: Rect, notch_w: f64, notch_h: f64) -> Rect {
    Rect::new(screen.mid_x() - notch_w / 2.0, screen.max_y() - notch_h, notch_w, notch_h)
}

/// Overshoot added above the screen's top edge for hit regions anchored there.
/// `Rect::contains` is half-open (max-exclusive); a cursor pinned against the top of the
/// display sits at exactly `ns.y == screen.max_y()` (CG pins at y=0), which a flush-top
/// rect would exclude — the primary "flick to the notch" gesture would never enter
/// R_enter. Extending 1pt beyond the screen is unreachable by any other pointer position,
/// so it only admits the pinned case.
pub const TOP_EDGE_OVERSHOOT: f64 = 1.0;

/// Build the three regions from the Idle rect and the screen (spec §3.4.2).
pub fn regions(screen: Rect, idle: Rect, p: GeometryParams) -> Regions {
    let r_enter = idle.expand(p.enter_lr, p.enter_lr, p.enter_bottom, TOP_EDGE_OVERSHOOT);
    let r_stay = r_enter.inset_all(p.stay_hysteresis);
    let expanded =
        Rect::new(screen.mid_x() - p.expanded_w / 2.0, screen.max_y() - p.expanded_h, p.expanded_w, p.expanded_h);
    let r_exp = expanded.inset_all(p.exp_margin);
    Regions { r_enter, r_stay, r_exp, top_band_min_y: screen.max_y() - p.top_band }
}

/// Convert a CGEvent point (top-left origin, y-down, referenced to the primary display)
/// to NS coordinates (spec §3.4.7). `primary_height` is `NSScreen.screens[0].frame.height`.
///
/// The conversion is an involution, so it also maps NS→CG. Multi-display global flips use
/// the primary display's height as the reference; verify the reference on-device.
pub fn cg_to_ns(p: Point, primary_height: f64) -> Point {
    Point::new(p.x, primary_height - p.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn internal_screen() -> Rect {
        // 14" MBP logical resolution, notch machine.
        Rect::new(0.0, 0.0, 1512.0, 982.0)
    }

    #[test]
    fn idle_rect_is_top_centre_anchored() {
        let s = internal_screen();
        let idle = idle_rect(s, 200.0, 32.0);
        assert_eq!(idle.max_y(), s.max_y()); // top edge flush with screen top
        assert_eq!(idle.mid_x(), s.mid_x()); // horizontally centred
        assert_eq!(idle.w, 200.0);
        assert_eq!(idle.h, 32.0);
    }

    #[test]
    fn r_enter_expands_sides_bottom_and_top_overshoot() {
        let s = internal_screen();
        let idle = idle_rect(s, 200.0, 32.0);
        let r = regions(s, idle, GeometryParams::default());
        // left/right +8, bottom +4, top +TOP_EDGE_OVERSHOOT (pinned-cursor admission).
        assert_eq!(r.r_enter.x, idle.x - 8.0);
        assert_eq!(r.r_enter.max_x(), idle.max_x() + 8.0);
        assert_eq!(r.r_enter.y, idle.y - 4.0);
        assert_eq!(r.r_enter.max_y(), idle.max_y() + TOP_EDGE_OVERSHOOT);
    }

    #[test]
    fn pinned_cursor_at_top_edge_is_inside_r_enter() {
        // CG pins the cursor at y=0 against the top of the display; cg_to_ns maps that to
        // exactly ns.y == screen.max_y(). The half-open contains() would exclude a flush
        // rect — the overshoot must admit it.
        let s = internal_screen();
        let idle = idle_rect(s, 200.0, 32.0);
        let r = regions(s, idle, GeometryParams::default());
        let pinned = cg_to_ns(Point::new(s.mid_x(), 0.0), s.max_y());
        assert_eq!(pinned.y, s.max_y());
        assert!(r.r_enter.contains(pinned));
        assert!(r.r_stay.contains(pinned));
        assert!(r.r_exp.contains(pinned));
    }

    #[test]
    fn r_stay_contains_r_enter() {
        let s = internal_screen();
        let idle = idle_rect(s, 200.0, 32.0);
        let r = regions(s, idle, GeometryParams::default());
        // A point just outside R_enter's left edge is still inside R_stay.
        let just_outside = Point::new(r.r_enter.x - 2.0, idle.mid_x().min(idle.max_y() - 1.0));
        let p = Point::new(just_outside.x, r.r_enter.y + 1.0);
        assert!(!r.r_enter.contains(p));
        assert!(r.r_stay.contains(p));
    }

    #[test]
    fn top_band_is_forty_below_top() {
        let s = internal_screen();
        let r = regions(s, idle_rect(s, 200.0, 32.0), GeometryParams::default());
        assert_eq!(r.top_band_min_y, s.max_y() - 40.0);
    }

    #[test]
    fn cg_ns_conversion_is_involution() {
        for h in [900.0, 982.0, 1329.0] {
            let cg = Point::new(756.0, 5.0); // near the top in CG (y-down)
            let ns = cg_to_ns(cg, h);
            assert_eq!(ns.y, h - 5.0); // near the top in NS (y-up)
            assert_eq!(cg_to_ns(ns, h), cg); // round trip
        }
    }

    #[test]
    fn contains_is_half_open() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(r.contains(Point::new(0.0, 0.0))); // min inclusive
        assert!(!r.contains(Point::new(10.0, 5.0))); // max exclusive
        assert!(!r.contains(Point::new(5.0, 10.0)));
    }
}
