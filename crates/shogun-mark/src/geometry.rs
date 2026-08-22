//! The mark's own vertices, and the fold applied to them.
//!
//! This is the same artwork `apps/desktop/src/Logo.tsx` draws, and the same fold
//! `apps/desktop/src/styles/logo-motion.css` animates — one brand, one mark. It exists twice
//! because the Dock icon and the menu-bar icon are drawn by AppKit, which never sees our CSS.
//! `tests/matches_the_webview.rs` reads the TypeScript and fails if the two copies drift.

/// The artwork's own coordinate space. Wider than it is tall.
pub const ART_W: f32 = 957.0;
pub const ART_H: f32 = 614.0;

/// Left half of the mark. The right half is this mirrored about `ART_W / 2`, so the two sides
/// cannot drift apart, and the source artwork's own 3px asymmetry is resolved in favour of true
/// symmetry — exactly as the webview does it.
pub const KABUTO: [&[[f32; 2]]; 3] = [
    &[[296.0, 254.0], [469.0, 0.0], [469.0, 525.0]], // centre peak
    &[[0.0, 101.0], [276.0, 264.0], [446.0, 524.0], [176.0, 390.0]], // wing
    &[[62.0, 613.0], [171.0, 413.0], [331.0, 493.0]], // blade
];

/// Which fold a facet is. The three creases are measured off the paths above; the fold turns each
/// facet about its own, which flat on is a scale perpendicular to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facet {
    /// Hinges on the centre line (x = 469), so the fold is a plain horizontal scale.
    Peak,
    /// Hinges on (276,264)-(446,524): 56.82° from horizontal, so the scale runs along -33.18°.
    Wing,
    /// Hinges on (171,413)-(331,493): 26.57° from horizontal, so the scale runs along -63.43°.
    Blade,
}

impl Facet {
    pub const ALL: [Facet; 3] = [Facet::Peak, Facet::Wing, Facet::Blade];

    fn index(self) -> usize {
        match self {
            Facet::Peak => 0,
            Facet::Wing => 1,
            Facet::Blade => 2,
        }
    }

    pub fn polygon(self) -> &'static [[f32; 2]] {
        KABUTO[self.index()]
    }

    /// The angle the fold's scale runs along, in radians. Zero for the peak, whose crease is
    /// vertical and whose scale is therefore already horizontal.
    fn fold_angle(self) -> f32 {
        match self {
            Facet::Peak => 0.0,
            Facet::Wing => -33.18_f32.to_radians(),
            Facet::Blade => -63.43_f32.to_radians(),
        }
    }

    /// A point on the facet's crease, as a fraction of its own bounding box — the same
    /// `transform-origin` percentages logo-motion.css uses, and for the same reason: a fill-box
    /// origin mirrors with the facet, so the right half needs no separate numbers.
    fn origin_fraction(self) -> [f32; 2] {
        match self {
            Facet::Peak => [1.0, 0.5],
            Facet::Wing => [0.8094, 0.6927],
            Facet::Blade => [0.7026, 0.20],
        }
    }

    fn origin(self) -> [f32; 2] {
        let (min, max) = bounds(self.polygon());
        let f = self.origin_fraction();
        [
            min[0] + (max[0] - min[0]) * f[0],
            min[1] + (max[1] - min[1]) * f[1],
        ]
    }
}

fn bounds(poly: &[[f32; 2]]) -> ([f32; 2], [f32; 2]) {
    let mut min = [f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN];
    for p in poly {
        min[0] = min[0].min(p[0]);
        min[1] = min[1].min(p[1]);
        max[0] = max[0].max(p[0]);
        max[1] = max[1].max(p[1]);
    }
    (min, max)
}

/// One facet of the mark, folded to `scale` about its crease and drawn at `alpha`.
#[derive(Clone, Debug)]
pub struct FoldedFacet {
    pub points: Vec<[f32; 2]>,
    pub alpha: f32,
}

/// The facet, turned `scale` of the way open about its crease.
///
/// `rotate(a) scaleX(k) rotate(-a)` about a point on the crease — a scale taken in a rotated
/// frame, which is what a facet turning about a diagonal crease looks like seen flat on. Points on
/// the crease itself do not move, whatever `scale` is.
pub fn fold_facet(facet: Facet, scale: f32) -> Vec<[f32; 2]> {
    let o = facet.origin();
    let a = facet.fold_angle();
    let (sin, cos) = a.sin_cos();
    facet
        .polygon()
        .iter()
        .map(|p| {
            // Into the crease's frame, scale across it, and back out.
            let dx = p[0] - o[0];
            let dy = p[1] - o[1];
            let u = dx * cos + dy * sin;
            let v = -dx * sin + dy * cos;
            let u = u * scale;
            [o[0] + u * cos - v * sin, o[1] + u * sin + v * cos]
        })
        .collect()
}

/// `points` mirrored about the artwork's vertical centre line, which is how the right half of the
/// mark is drawn.
pub fn mirror(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    points.iter().map(|p| [ART_W - p[0], p[1]]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fully_open_facet_is_the_artwork_itself() {
        for facet in Facet::ALL {
            let open = fold_facet(facet, 1.0);
            for (got, want) in open.iter().zip(facet.polygon()) {
                assert!((got[0] - want[0]).abs() < 1e-3, "{facet:?} x: {got:?} {want:?}");
                assert!((got[1] - want[1]).abs() < 1e-3, "{facet:?} y: {got:?} {want:?}");
            }
        }
    }

    #[test]
    fn a_closed_facet_collapses_onto_its_crease() {
        // At scale 0 every vertex lands on the crease line, so the facet has no area left.
        for facet in Facet::ALL {
            let shut = fold_facet(facet, 0.0);
            let (min, max) = bounds(&shut);
            let across = ((max[0] - min[0]).powi(2) + (max[1] - min[1]).powi(2)).sqrt();
            let (omin, omax) = bounds(facet.polygon());
            let open = ((omax[0] - omin[0]).powi(2) + (omax[1] - omin[1]).powi(2)).sqrt();
            // Collapsed to a line: still as long as the crease, but with no width.
            assert!(across < open, "{facet:?} did not collapse");
            assert!(area(&shut).abs() < 1.0, "{facet:?} kept area {}", area(&shut));
        }
    }

    #[test]
    fn the_peak_holds_the_centre_line_whatever_the_fold() {
        // Its crease IS the centre line; if that moved, the two halves would part company.
        for scale in [0.0, 0.3, 1.0, 1.04] {
            let p = fold_facet(Facet::Peak, scale);
            assert!((p[1][0] - 469.0).abs() < 1e-3, "apex left x=469 at {scale}");
            assert!((p[2][0] - 469.0).abs() < 1e-3, "base left x=469 at {scale}");
        }
    }

    #[test]
    fn folding_never_turns_a_facet_inside_out() {
        // A negative area means the winding flipped, which the rasterizer would punch a hole for.
        for facet in Facet::ALL {
            for step in 1..=20 {
                let scale = step as f32 / 20.0;
                assert!(area(&fold_facet(facet, scale)) > 0.0, "{facet:?} at {scale}");
            }
        }
    }

    #[test]
    fn the_mirror_is_a_mirror() {
        let there = fold_facet(Facet::Wing, 1.0);
        let back = mirror(&mirror(&there));
        for (a, b) in there.iter().zip(back.iter()) {
            assert!((a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3);
        }
    }

    fn area(poly: &[[f32; 2]]) -> f32 {
        let mut sum = 0.0;
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            sum += a[0] * b[1] - b[0] * a[1];
        }
        sum / 2.0
    }
}
