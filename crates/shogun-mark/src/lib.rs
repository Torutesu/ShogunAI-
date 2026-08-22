//! The ShogunAI mark, drawn without a webview.
//!
//! The app's windows fold the mark in CSS. The Dock icon and the menu-bar icon are drawn by
//! AppKit, which never sees that stylesheet, so the same fold has to exist here too. This crate
//! holds the artwork's vertices, the fold's timing, and a rasteriser — and nothing platform
//! specific, so Linux CI covers all three.
//!
//! The output is coverage, not pixels: an alpha map the caller paints in whatever the surface
//! calls for. The Dock wants brand blue on the icon's plate; the menu bar wants a template
//! silhouette macOS tints itself. Both are the same fold.

pub mod geometry;
pub mod raster;
pub mod unfold;

pub use geometry::{Facet, ART_H, ART_W};
pub use unfold::DURATION_MS;

/// Where the artwork sits inside the output box.
///
/// The mark is centred, at `width_fraction` of the box's width. The shipped Dock icon puts it at
/// 338/512 of the plate; the menu-bar icon at 38/44. Getting this from the artwork rather than
/// eyeballing it is what makes the animation's last frame land on the icon that ships.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub width_fraction: f32,
}

impl Placement {
    pub const fn new(width_fraction: f32) -> Self {
        Self { width_fraction }
    }

    /// Artwork units to pixels, for a box of `width` x `height`.
    fn transform(&self, width: u32, height: u32) -> (f32, f32, f32) {
        let scale = width as f32 * self.width_fraction / ART_W;
        let dx = (width as f32 - ART_W * scale) / 2.0;
        let dy = (height as f32 - ART_H * scale) / 2.0;
        (scale, dx, dy)
    }
}

/// The mark's alpha at `ms` into its arrival, one byte a pixel, row-major.
///
/// Facets are combined by taking the greater coverage rather than adding it: mid-fold they carry
/// past their creases and briefly overlap, and summing there would print a seam brighter than the
/// paper around it.
pub fn unfold_alpha(ms: f32, width: u32, height: u32, placement: Placement) -> Vec<u8> {
    let (mark_scale, mark_opacity) = unfold::mark_at(ms);
    let (scale, dx, dy) = placement.transform(width, height);

    // The whole mark scales about its own centre, on top of each facet's fold.
    let cx = ART_W / 2.0;
    let cy = ART_H / 2.0;
    let place = |p: [f32; 2]| -> [f32; 2] {
        [
            dx + (cx + (p[0] - cx) * mark_scale) * scale,
            dy + (cy + (p[1] - cy) * mark_scale) * scale,
        ]
    };

    let mut alpha = vec![0.0_f32; (width as usize) * (height as usize)];
    for facet in Facet::ALL {
        let (fold, facet_opacity) = unfold::facet_at(facet, ms);
        let opacity = facet_opacity * mark_opacity;
        if opacity <= 0.0 {
            continue;
        }
        let folded = geometry::fold_facet(facet, fold);
        let left: Vec<[f32; 2]> = folded.iter().map(|p| place(*p)).collect();
        let right: Vec<[f32; 2]> = geometry::mirror(&folded).iter().map(|p| place(*p)).collect();
        for half in [left, right] {
            for (dst, cov) in alpha
                .iter_mut()
                .zip(raster::coverage(&half, width, height))
            {
                *dst = dst.max(cov * opacity);
            }
        }
    }

    alpha
        .into_iter()
        .map(|a| (a.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect()
}

/// The mark at rest — the artwork itself, no fold, fully solid.
pub fn still_alpha(width: u32, height: u32, placement: Placement) -> Vec<u8> {
    unfold_alpha(DURATION_MS, width, height, placement)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How much ink a frame lays down, as a fraction of the box.
    fn ink(alpha: &[u8]) -> f32 {
        alpha.iter().map(|a| *a as f32 / 255.0).sum::<f32>() / alpha.len() as f32
    }

    const DOCK: Placement = Placement::new(338.0 / 512.0);

    #[test]
    fn the_frame_is_the_size_it_was_asked_for() {
        assert_eq!(unfold_alpha(0.0, 64, 64, DOCK).len(), 64 * 64);
        assert_eq!(unfold_alpha(100.0, 44, 22, DOCK).len(), 44 * 22);
    }

    #[test]
    fn the_arrival_fills_out_over_time() {
        // Not strictly monotonic — facets overshoot and settle — but the trend has to be up, and
        // the first frame has to be empty or the fold has nothing to show.
        let start = ink(&unfold_alpha(0.0, 128, 128, DOCK));
        let third = ink(&unfold_alpha(DURATION_MS / 3.0, 128, 128, DOCK));
        let end = ink(&unfold_alpha(DURATION_MS, 128, 128, DOCK));
        assert!(start < 0.005, "the fold started already drawn: {start}");
        assert!(third > start * 4.0, "nothing happened by a third: {third}");
        assert!(end > third, "the fold went backwards: {third} then {end}");
    }

    #[test]
    fn the_last_frame_is_the_mark_itself() {
        let settled = unfold_alpha(DURATION_MS, 256, 256, DOCK);
        let still = still_alpha(256, 256, DOCK);
        assert_eq!(settled, still);
        // And it is real ink, not a ghost: somewhere in there the paper is fully opaque.
        assert_eq!(settled.iter().copied().max(), Some(255));
    }

    #[test]
    fn the_mark_lands_where_the_shipped_icon_puts_it() {
        // Measured off icons/icon-512.png: the mark spans x 87..424 of 512, centred.
        let alpha = still_alpha(512, 512, DOCK);
        let (mut x0, mut x1) = (512_usize, 0_usize);
        let (mut y0, mut y1) = (512_usize, 0_usize);
        for y in 0..512 {
            for x in 0..512 {
                if alpha[y * 512 + x] > 8 {
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                    y1 = y1.max(y);
                }
            }
        }
        assert!((x0 as i32 - 87).abs() <= 2, "left edge at {x0}, shipped icon has 87");
        assert!((x1 as i32 - 424).abs() <= 2, "right edge at {x1}, shipped icon has 424");
        assert!((y0 as i32 - 147).abs() <= 2, "top edge at {y0}, shipped icon has 147");
        assert!((y1 as i32 - 363).abs() <= 2, "bottom edge at {y1}, shipped icon has 363");
    }

    #[test]
    fn the_mark_is_symmetric_about_its_centre_line() {
        let (w, h) = (128_usize, 128_usize);
        for ms in [0.0, 200.0, 400.0, DURATION_MS] {
            let a = unfold_alpha(ms, w as u32, h as u32, DOCK);
            for y in 0..h {
                for x in 0..w / 2 {
                    let l = a[y * w + x] as i32;
                    let r = a[y * w + (w - 1 - x)] as i32;
                    assert!((l - r).abs() <= 2, "asymmetric at {ms}ms ({x},{y}): {l} vs {r}");
                }
            }
        }
    }

    #[test]
    fn nothing_spills_outside_the_box() {
        // The overshoot pushes the wings past the artwork's own width; the frame must still hold.
        for ms in [0.0, 150.0, 300.0, 450.0, 600.0, DURATION_MS] {
            let a = unfold_alpha(ms, 64, 64, Placement::new(0.99));
            assert_eq!(a.len(), 64 * 64);
        }
    }

    #[test]
    fn a_menu_bar_sized_frame_still_reads() {
        // 22pt at 2x, the size the tray actually draws. If the fold vanishes here it was pointless.
        let tray = Placement::new(38.0 / 44.0);
        let ink_end = ink(&still_alpha(44, 44, tray));
        assert!(ink_end > 0.10, "the mark barely showed at tray size: {ink_end}");
    }
}
