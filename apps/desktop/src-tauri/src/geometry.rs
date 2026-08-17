//! Geometry adapter (spec §3.2, §3.4.7).
//!
//! The math lives in `shogun_core::notch::geometry` (Rect/Regions/idle_rect/regions/cg_to_ns,
//! unit-tested on Linux). This adapter reads the raw macOS screen measurements —
//! `NSScreen.frame/visibleFrame/safeAreaInsets` and `auxiliaryTopLeftArea/RightArea`
//! (research item 4: NSRect, empty on non-notch, bottom-left origin) — and feeds them into
//! `shogun_core::notch::geometry::regions(...)`. CGEvent points are normalised with
//! each display's paired CoreGraphics/AppKit bounds at the boundary (T-07).
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
    use objc2_core_graphics::CGDisplayBounds;
    use objc2_foundation::{NSNumber, NSString};

    /// The panel-target screen's notch/pseudo geometry, resolved into shogun_core regions,
    /// plus the CG-conversion constant taken from the true primary display.
    pub struct ScreenGeometry {
        /// Stable CoreGraphics display identity from the documented NSScreenNumber descriptor.
        pub display_id: u32,
        /// Physical display bounds in CoreGraphics's top-left/y-down coordinates.
        pub cg_screen: Rect,
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
    pub fn read_all(mtm: MainThreadMarker) -> Vec<ScreenGeometry> {
        let screens = NSScreen::screens(mtm);
        let count = screens.len() as u32;
        screens
            .iter()
            .filter_map(|screen| geometry_for(&screen, count))
            .collect()
    }

    /// Resolve one screen's geometry. Split out of `read_primary` so both paths agree by
    /// construction rather than by two copies staying in step.
    fn geometry_for(screen: &NSScreen, display_count: u32) -> Option<ScreenGeometry> {
        let f = screen.frame();
        let vf = screen.visibleFrame();
        let notch_inset = screen.safeAreaInsets().top;

        let screen_rect = Rect::new(f.origin.x, f.origin.y, f.size.width, f.size.height);
        let screen_number_key = NSString::from_str("NSScreenNumber");
        let display_id = screen
            .deviceDescription()
            .objectForKey(&screen_number_key)
            .and_then(|value| value.downcast::<NSNumber>().ok())
            .map(|number| number.as_u32())?;
        let cg = CGDisplayBounds(display_id);
        let cg_screen = Rect::new(cg.origin.x, cg.origin.y, cg.size.width, cg.size.height);
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
        Some(ScreenGeometry {
            display_id,
            cg_screen,
            is_notch,
            screen: screen_rect,
            notch_w,
            notch_h,
            menubar_h,
            idle,
            activation,
            regions: regs,
            display_count,
        })
    }

    pub fn read_primary(mtm: MainThreadMarker) -> Option<ScreenGeometry> {
        // Primary display = screens[0] (menubar owner, CG coordinate anchor). The panel
        // itself also targets the primary — the spike's display policy (spec §3.7.1)
        // prefers the internal/primary screen; per-display selection is on-device D-06.
        let screens = NSScreen::screens(mtm);
        let display_count = screens.len() as u32;
        screens
            .firstObject()
            .and_then(|screen| geometry_for(&screen, display_count))
            .or_else(|| {
                NSScreen::mainScreen(mtm).and_then(|screen| geometry_for(&screen, display_count))
            })
    }

    /// Spatial-ready display id for the primary screen (menubar owner). v1 uses index 0.
    pub fn primary_display_id() -> i64 {
        0
    }
}
