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
        // Idle entry is exactly the hardware notch. Hysteresis applies only after entry.
        Self {
            enter_lr: 0.0,
            enter_bottom: 0.0,
            stay_hysteresis: 2.0,
            exp_margin: 16.0,
            expanded_w: 400.0,
            expanded_h: 180.0,
            top_band: 40.0,
        }
    }
}

/// Visible Idle content row below the hardware cutout (boring.notch drop). Welded black still
/// fills `notch_h`; labels/icons live in this extra strip so they are not under silicon.
pub const IDLE_CONTENT_DROP: f64 = 44.0;

/// Idle hit/visual height: hardware `notch_h` plus the visible content drop on real-notch
/// machines. Pseudo-notch already sits in the visible menubar band — no extra drop.
pub fn idle_height(notch_h: f64, is_notch: bool) -> f64 {
    if is_notch {
        notch_h + IDLE_CONTENT_DROP
    } else {
        notch_h
    }
}

/// The Idle "silhouette" rect: the real notch (plus content drop), or the 180×menubar
/// pseudo-notch, anchored top-centre on `screen` (spec §3.2.1, §3.2.2).
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

/// Rebuild regions with a live panel size (open/resize). Keeps Idle enter/stay rings; replaces
/// `r_exp` with the actual panel frame + grace margin so leave-grace covers the Combined region.
pub fn regions_with_panel(screen: Rect, idle: Rect, panel_w: f64, panel_h: f64, p: GeometryParams) -> Regions {
    let r_enter = idle.expand(p.enter_lr, p.enter_lr, p.enter_bottom, TOP_EDGE_OVERSHOOT);
    let r_stay = r_enter.inset_all(p.stay_hysteresis);
    let panel = Rect::new(
        screen.mid_x() - panel_w / 2.0,
        screen.max_y() - panel_h,
        panel_w,
        panel_h,
    );
    let r_exp = panel.inset_all(p.exp_margin);
    // Idle early-reject floor: Idle chin (+ enter bottom). Points inside `r_exp` below this
    // floor are still tracked (see HoverTracker). Width is enforced in the CGEventTap adapter.
    Regions {
        r_enter,
        r_stay,
        r_exp,
        top_band_min_y: screen.max_y() - p.top_band.max(idle.h + p.enter_bottom),
    }
}

/// Build the three regions from the Idle rect and the screen (spec §3.4.2).
pub fn regions(screen: Rect, idle: Rect, p: GeometryParams) -> Regions {
    regions_with_panel(screen, idle, p.expanded_w, p.expanded_h, p)
}

/// Where the user parks SHOGUN's panel — its "castle" (issue #20). The Notch is the default,
/// top-centre resting place; the other five let the panel live at a screen edge or corner.
/// The wire form (JSON on disk + the IPC boundary) is the `snake_case` `key`/`from_key` string, so
/// this stays a plain enum with no serde dependency in the core crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CastlePosition {
    /// Top-centre, hanging from the notch. The default resting place (spec §3.2.1).
    #[default]
    Notch,
    /// Left screen edge, vertically centred.
    LeftEdge,
    /// Right screen edge, vertically centred.
    RightEdge,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom-centre.
    BottomCenter,
    /// Bottom-right corner.
    BottomRight,
}

impl CastlePosition {
    /// The stable wire key (matches the `serde` `snake_case` form) used by the JSON store and the
    /// UI ↔ Rust IPC. Kept explicit so the boundary never depends on serde quoting.
    pub fn key(self) -> &'static str {
        match self {
            CastlePosition::Notch => "notch",
            CastlePosition::LeftEdge => "left_edge",
            CastlePosition::RightEdge => "right_edge",
            CastlePosition::BottomLeft => "bottom_left",
            CastlePosition::BottomCenter => "bottom_center",
            CastlePosition::BottomRight => "bottom_right",
        }
    }

    /// Parse a wire key back into a position. Unknown keys yield `None` so callers can fall back to
    /// the default rather than adopt a bogus placement.
    pub fn from_key(s: &str) -> Option<Self> {
        Some(match s {
            "notch" => CastlePosition::Notch,
            "left_edge" => CastlePosition::LeftEdge,
            "right_edge" => CastlePosition::RightEdge,
            "bottom_left" => CastlePosition::BottomLeft,
            "bottom_center" => CastlePosition::BottomCenter,
            "bottom_right" => CastlePosition::BottomRight,
            _ => return None,
        })
    }

    /// Compact `u8` encoding for the lock-free `AtomicU8` the shell reads from the placement fns.
    /// Paired with `from_u8`; the value is an internal detail, never persisted or sent.
    pub fn to_u8(self) -> u8 {
        match self {
            CastlePosition::Notch => 0,
            CastlePosition::LeftEdge => 1,
            CastlePosition::RightEdge => 2,
            CastlePosition::BottomLeft => 3,
            CastlePosition::BottomCenter => 4,
            CastlePosition::BottomRight => 5,
        }
    }

    /// Inverse of `to_u8`; any out-of-range byte falls back to the default (`Notch`).
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => CastlePosition::LeftEdge,
            2 => CastlePosition::RightEdge,
            3 => CastlePosition::BottomLeft,
            4 => CastlePosition::BottomCenter,
            5 => CastlePosition::BottomRight,
            _ => CastlePosition::Notch,
        }
    }
}

/// Which frame to dock against for `pos`.
///
/// **Notch** uses the full `screen` frame so the panel top edge meets the physical display top
/// (welded under/behind the hardware notch — boring.notch). Edge/corner castles use `visible`
/// (menu bar + Dock already excluded) so they stay in the usable work area.
pub fn castle_dock_frame(screen: Rect, visible: Rect, pos: CastlePosition) -> Rect {
    match pos {
        CastlePosition::Notch => screen,
        _ => visible,
    }
}

/// Bottom-left NS origin for a `w`×`h` panel resting at `pos` inside `dock` — typically from
/// [`castle_dock_frame`]. The anchored axis sits flush to the edge; the free axis is centred.
/// The result is clamped so the panel stays inside `dock`.
pub fn castle_origin(dock: Rect, w: f64, h: f64, pos: CastlePosition) -> Point {
    use CastlePosition::*;
    let left = dock.x;
    let centre_x = dock.x + (dock.w - w) / 2.0;
    let right = dock.max_x() - w;
    let top = dock.max_y() - h;
    let middle_y = dock.y + (dock.h - h) / 2.0;
    let bottom = dock.y;
    let (x, y) = match pos {
        Notch => (centre_x, top),
        LeftEdge => (left, middle_y),
        RightEdge => (right, middle_y),
        BottomLeft => (left, bottom),
        BottomCenter => (centre_x, bottom),
        BottomRight => (right, bottom),
    };
    // Keep the panel inside `dock` even when it is wider/taller than the free space (clamp
    // collapses to the edge). `max` guards a degenerate frame smaller than the panel.
    let x = x.clamp(dock.x, (dock.max_x() - w).max(dock.x));
    let y = y.clamp(dock.y, (dock.max_y() - h).max(dock.y));
    Point::new(x, y)
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
    fn r_enter_matches_notch_except_unreachable_top_overshoot() {
        let s = internal_screen();
        let idle = idle_rect(s, 200.0, 32.0);
        let r = regions(s, idle, GeometryParams::default());
        assert_eq!(r.r_enter.x, idle.x);
        assert_eq!(r.r_enter.max_x(), idle.max_x());
        assert_eq!(r.r_enter.y, idle.y);
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

    // Full screen vs visible (menu bar excluded): 1512×982 screen, 1512×950 visible.
    fn screen() -> Rect {
        Rect::new(0.0, 0.0, 1512.0, 982.0)
    }
    fn visible() -> Rect {
        Rect::new(0.0, 0.0, 1512.0, 950.0)
    }

    #[test]
    fn castle_key_roundtrips_every_variant() {
        for p in [
            CastlePosition::Notch,
            CastlePosition::LeftEdge,
            CastlePosition::RightEdge,
            CastlePosition::BottomLeft,
            CastlePosition::BottomCenter,
            CastlePosition::BottomRight,
        ] {
            assert_eq!(CastlePosition::from_key(p.key()), Some(p));
            assert_eq!(CastlePosition::from_u8(p.to_u8()), p);
        }
        assert_eq!(CastlePosition::from_key("nonsense"), None);
        assert_eq!(CastlePosition::from_u8(200), CastlePosition::Notch); // unknown → default
        assert_eq!(CastlePosition::default(), CastlePosition::Notch);
    }

    #[test]
    fn notch_docks_to_full_screen_top_not_visible_frame() {
        let s = screen();
        let v = visible();
        assert_eq!(castle_dock_frame(s, v, CastlePosition::Notch), s);
        assert_eq!(castle_dock_frame(s, v, CastlePosition::BottomCenter), v);
        let (w, h) = (400.0, 180.0);
        let dock = castle_dock_frame(s, v, CastlePosition::Notch);
        let o = castle_origin(dock, w, h, CastlePosition::Notch);
        // Flush to physical screen top (behind/under hardware notch), horizontally centred.
        assert_eq!(o.y, s.max_y() - h);
        assert_eq!(o.x, s.x + (s.w - w) / 2.0);
        assert!(o.y + h > v.max_y()); // overlaps menu-bar band — welded, not below it
    }

    #[test]
    fn edges_flush_to_their_side_and_centre_the_free_axis() {
        let v = visible();
        let (w, h) = (400.0, 180.0);
        let left = castle_origin(v, w, h, CastlePosition::LeftEdge);
        assert_eq!(left.x, v.x); // flush left
        assert_eq!(left.y, v.y + (v.h - h) / 2.0); // vertically centred
        let right = castle_origin(v, w, h, CastlePosition::RightEdge);
        assert_eq!(right.x, v.max_x() - w); // flush right
        assert_eq!(right.y, v.y + (v.h - h) / 2.0);
    }

    #[test]
    fn bottom_row_flush_to_the_bottom() {
        let v = visible();
        let (w, h) = (400.0, 180.0);
        let bl = castle_origin(v, w, h, CastlePosition::BottomLeft);
        assert_eq!((bl.x, bl.y), (v.x, v.y)); // bottom-left corner
        let bc = castle_origin(v, w, h, CastlePosition::BottomCenter);
        assert_eq!((bc.x, bc.y), (v.x + (v.w - w) / 2.0, v.y));
        let br = castle_origin(v, w, h, CastlePosition::BottomRight);
        assert_eq!((br.x, br.y), (v.max_x() - w, v.y)); // bottom-right corner
    }

    #[test]
    fn castle_origin_clamps_a_panel_larger_than_the_free_space() {
        // An oversized panel can't hang off the edge — it collapses to the visible-frame origin.
        let v = visible();
        let o = castle_origin(v, v.w + 200.0, v.h + 200.0, CastlePosition::BottomRight);
        assert_eq!((o.x, o.y), (v.x, v.y));
    }

    #[test]
    fn switching_size_at_bottom_centre_keeps_the_bottom_edge_and_centre() {
        // Collapse (pill) → expand (panel) at the same anchor: the bottom edge stays put and the
        // panel grows upward, mirroring the notch's grow-downward behaviour.
        let v = visible();
        let pill = castle_origin(v, 260.0, 44.0, CastlePosition::BottomCenter);
        let open = castle_origin(v, 400.0, 180.0, CastlePosition::BottomCenter);
        assert_eq!(pill.y, v.y);
        assert_eq!(open.y, v.y); // bottom edge held across the size change
        assert_eq!(pill.x + 260.0 / 2.0, open.x + 400.0 / 2.0); // centre held
    }
}
