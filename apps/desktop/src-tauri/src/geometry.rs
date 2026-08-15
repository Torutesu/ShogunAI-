//! Geometry adapter (spec §3.2, §3.4.7).
//!
//! The math lives in `shogun_core::notch::geometry` (Rect/Regions/idle_rect/regions/cg_to_ns,
//! unit-tested on Linux). This adapter reads the raw macOS screen measurements —
//! `NSScreen.frame/visibleFrame/safeAreaInsets` and `auxiliaryTopLeftArea/RightArea`
//! (research item 4: NSRect, empty on non-notch, bottom-left origin) — and feeds them into
//! `shogun_core::notch::geometry::regions(...)`. CGEvent points are normalised with
//! `cg_to_ns(p, primary_height)` at the boundary (T-07).
#![allow(dead_code, unused_imports)]

pub use shogun_core::notch::geometry::{
    cg_to_ns, idle_height, idle_rect, regions, GeometryParams, Point, Rect, Regions,
};

#[cfg(target_os = "macos")]
pub use mac::{read_primary, ScreenGeometry};

#[cfg(target_os = "macos")]
pub mod mac {
    use super::{idle_height, idle_rect, regions, GeometryParams, Rect, Regions};
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    /// The panel-target screen's notch/pseudo geometry, resolved into shogun_core regions,
    /// plus the CG-conversion constant taken from the true primary display.
    pub struct ScreenGeometry {
        pub is_notch: bool,
        pub screen: Rect,
        pub notch_w: f64,
        pub notch_h: f64,
        pub menubar_h: f64,
        /// Visible idle rect (notch height plus content drop on real-notch machines).
        pub idle: Rect,
        /// Exact hardware-notch rectangle used to begin hover.
        pub activation: Rect,
        pub regions: Regions,
        /// Height of `NSScreen.screens[0]` — the primary display that anchors the CG
        /// global coordinate space. This, NOT the panel screen's height, is the
        /// `cg_to_ns` flip constant (review #5: mainScreen follows the key window and
        /// diverges from the primary on multi-display setups).
        pub primary_height: f64,
        /// Number of attached displays (recorded with each expand-latency sample).
        pub display_count: u32,
    }

    /// Read the main screen (must be called on the main thread — pass the `MainThreadMarker`
    /// from Tauri's setup). Returns `None` if there is no main screen.
    /// The notch/pseudo-notch geometry for EVERY attached display.
    ///
    /// The spike resolved only the primary because the panel lived there (spec §3.7.1). That made
    /// the notch on a second monitor inert: the tracker saw the pointer, but hit-tested it against
    /// regions belonging to a screen it wasn't on. Hover has to work wherever you are, so the
    /// adapter now carries one region set per display and picks by pointer.
    ///
    /// `primary_height` stays the PRIMARY's height in every entry — it is the CG↔NS flip constant
    /// for the whole global coordinate space, not a property of the screen being described.
    pub fn read_all(mtm: MainThreadMarker) -> Vec<ScreenGeometry> {
        let screens = NSScreen::screens(mtm);
        let count = screens.len() as u32;
        let primary_height = screens
            .firstObject()
            .map(|s| s.frame().size.height)
            .unwrap_or(0.0);
        screens
            .iter()
            .map(|screen| geometry_for(&screen, primary_height, count))
            .collect()
    }

    /// Resolve one screen's geometry. Split out of `read_primary` so both paths agree by
    /// construction rather than by two copies staying in step.
    fn geometry_for(screen: &NSScreen, primary_height: f64, display_count: u32) -> ScreenGeometry {
        let f = screen.frame();
        let vf = screen.visibleFrame();
        let notch_inset = screen.safeAreaInsets().top;

        let screen_rect = Rect::new(f.origin.x, f.origin.y, f.size.width, f.size.height);
        let menubar_h = (f.origin.y + f.size.height) - (vf.origin.y + vf.size.height);

        let (is_notch, notch_w, notch_h) = if notch_inset > 0.0 {
            let l = screen.auxiliaryTopLeftArea();
            let r = screen.auxiliaryTopRightArea();
            (
                true,
                f.size.width - l.size.width - r.size.width,
                notch_inset,
            )
        } else {
            // Pseudo-notch: 180pt wide, menubar-tall (fallback 24pt), spec §3.2.2.
            (false, 180.0, if menubar_h > 0.0 { menubar_h } else { 24.0 })
        };

        // Real-notch: Idle hit/visual height = silicon cutout + content drop below it.
        let idle_h = idle_height(notch_h, is_notch);
        let idle = idle_rect(screen_rect, notch_w, idle_h);
        let activation = idle_rect(screen_rect, notch_w, notch_h);
        let regs = regions(screen_rect, activation, GeometryParams::default());
        ScreenGeometry {
            is_notch,
            screen: screen_rect,
            notch_w,
            notch_h,
            menubar_h,
            idle,
            activation,
            regions: regs,
            primary_height,
            display_count,
        }
    }

    pub fn read_primary(mtm: MainThreadMarker) -> Option<ScreenGeometry> {
        // Primary display = screens[0] (menubar owner, CG coordinate anchor). The panel
        // itself also targets the primary — the spike's display policy (spec §3.7.1)
        // prefers the internal/primary screen; per-display selection is on-device D-06.
        let screens = NSScreen::screens(mtm);
        let display_count = screens.len() as u32;
        let screen = screens
            .firstObject()
            .or_else(|| NSScreen::mainScreen(mtm))?;
        let f = screen.frame();
        let vf = screen.visibleFrame();
        // These NSScreen accessors are safe fns in objc2-app-kit 0.3.2.
        let notch_inset = screen.safeAreaInsets().top;

        let screen_rect = Rect::new(f.origin.x, f.origin.y, f.size.width, f.size.height);
        let menubar_h = (f.origin.y + f.size.height) - (vf.origin.y + vf.size.height);

        let (is_notch, notch_w, notch_h) = if notch_inset > 0.0 {
            let l = screen.auxiliaryTopLeftArea();
            let r = screen.auxiliaryTopRightArea();
            (
                true,
                f.size.width - l.size.width - r.size.width,
                notch_inset,
            )
        } else {
            // Pseudo-notch: 180pt wide, menubar-tall (fallback 24pt), spec §3.2.2.
            (false, 180.0, if menubar_h > 0.0 { menubar_h } else { 24.0 })
        };

        let idle_h = idle_height(notch_h, is_notch);
        let idle = idle_rect(screen_rect, notch_w, idle_h);
        let activation = idle_rect(screen_rect, notch_w, notch_h);
        let regs = regions(screen_rect, activation, GeometryParams::default());
        Some(ScreenGeometry {
            is_notch,
            screen: screen_rect,
            notch_w,
            notch_h,
            menubar_h,
            idle,
            activation,
            regions: regs,
            primary_height: f.size.height,
            display_count,
        })
    }

    /// Spatial-ready display id for the primary screen (menubar owner). v1 uses index 0.
    pub fn primary_display_id() -> i64 {
        0
    }
}
