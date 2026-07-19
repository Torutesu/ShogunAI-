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

    /// The panel-target screen's notch/pseudo geometry, resolved into spike_core regions,
    /// plus the CG-conversion constant taken from the true primary display.
    pub struct ScreenGeometry {
        pub is_notch: bool,
        pub screen: Rect,
        pub notch_w: f64,
        pub notch_h: f64,
        pub menubar_h: f64,
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
    pub fn read_primary(mtm: MainThreadMarker) -> Option<ScreenGeometry> {
        // Primary display = screens[0] (menubar owner, CG coordinate anchor). The panel
        // itself also targets the primary — the spike's display policy (spec §3.7.1)
        // prefers the internal/primary screen; per-display selection is on-device D-06.
        let screens = NSScreen::screens(mtm);
        let display_count = screens.len() as u32;
        let screen = screens.firstObject().or_else(|| NSScreen::mainScreen(mtm))?;
        let f = screen.frame();
        let vf = screen.visibleFrame();
        // These NSScreen accessors are safe fns in objc2-app-kit 0.3.2.
        let notch_inset = screen.safeAreaInsets().top;

        let screen_rect = Rect::new(f.origin.x, f.origin.y, f.size.width, f.size.height);
        let menubar_h = (f.origin.y + f.size.height) - (vf.origin.y + vf.size.height);

        let (is_notch, notch_w, notch_h) = if notch_inset > 0.0 {
            let l = screen.auxiliaryTopLeftArea();
            let r = screen.auxiliaryTopRightArea();
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
            primary_height: f.size.height,
            display_count,
        })
    }
}
