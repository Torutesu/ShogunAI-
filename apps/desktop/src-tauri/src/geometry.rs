//! Geometry adapter (spec §3.2, §3.4.7).
//!
//! The math lives in `spike_core::geometry` (Rect/Regions/idle_rect/regions/cg_to_ns,
//! unit-tested on Linux). This adapter reads the raw macOS screen measurements —
//! `NSScreen.frame/visibleFrame/safeAreaInsets` and `auxiliaryTopLeftArea/RightArea`
//! (research item 4: NSRect, empty on non-notch, bottom-left origin) — and feeds them into
//! `spike_core::geometry::regions(...)`. CGEvent points are normalised with
//! `cg_to_ns(p, primary_height)` at the boundary (T-07).
#![allow(dead_code, unused_imports)]

pub use spike_core::geometry::{cg_to_ns, idle_rect, regions, GeometryParams, Point, Rect, Regions};

#[cfg(target_os = "macos")]
pub use mac::{read_primary, ScreenGeometry};

#[cfg(target_os = "macos")]
mod mac {
    use super::{idle_rect, regions, GeometryParams, Rect, Regions};
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    /// The primary display's notch/pseudo geometry, resolved into spike_core regions.
    pub struct ScreenGeometry {
        pub is_notch: bool,
        pub screen: Rect,
        pub notch_w: f64,
        pub notch_h: f64,
        pub menubar_h: f64,
        pub regions: Regions,
    }

    /// Read the main screen (must be called on the main thread — pass the `MainThreadMarker`
    /// from Tauri's setup). Returns `None` if there is no main screen.
    pub fn read_primary(mtm: MainThreadMarker) -> Option<ScreenGeometry> {
        let screen = NSScreen::mainScreen(mtm)?;
        let f = screen.frame();
        let vf = screen.visibleFrame();
        // SAFETY: safeAreaInsets / auxiliary*Area are generated as `unsafe fn`; they are
        // valid on any NSScreen and return by value.
        let notch_inset = unsafe { screen.safeAreaInsets() }.top;

        let screen_rect = Rect::new(f.origin.x, f.origin.y, f.size.width, f.size.height);
        let menubar_h = (f.origin.y + f.size.height) - (vf.origin.y + vf.size.height);

        let (is_notch, notch_w, notch_h) = if notch_inset > 0.0 {
            let l = unsafe { screen.auxiliaryTopLeftArea() };
            let r = unsafe { screen.auxiliaryTopRightArea() };
            (true, f.size.width - l.size.width - r.size.width, notch_inset)
        } else {
            // Pseudo-notch: 180pt wide, menubar-tall (fallback 24pt), spec §3.2.2.
            (false, 180.0, if menubar_h > 0.0 { menubar_h } else { 24.0 })
        };

        let idle = idle_rect(screen_rect, notch_w, notch_h);
        let regs = regions(screen_rect, idle, GeometryParams::default());
        Some(ScreenGeometry {
            is_notch,
            screen: screen_rect,
            notch_w,
            notch_h,
            menubar_h,
            regions: regs,
        })
    }
}
