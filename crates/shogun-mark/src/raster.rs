//! A polygon rasteriser, sized to exactly this job.
//!
//! The mark is six flat facets with straight edges and no curves, so a scanline fill with
//! nonzero winding covers it completely — no path flattening, no curve subdivision, and no new
//! dependency in a tree that already carries enough of them.
//!
//! Anti-aliasing is supersampled down the page and exact across it: each pixel row is walked as
//! `SUB` sub-rows, and within a sub-row a span contributes its true fractional overlap to the
//! pixels at each end. That reads clean at 16px in the menu bar, which is the size that decides
//! whether this was worth doing.

/// Sub-rows per pixel row. Four is where the stair-stepping on the mark's shallowest edge — the
/// blade, at 26.57° — stops being visible at menu-bar size.
const SUB: usize = 4;

/// Coverage of one polygon over a `width` x `height` grid, 0.0 to 1.0 per pixel.
pub fn coverage(points: &[[f32; 2]], width: u32, height: u32) -> Vec<f32> {
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0.0_f32; w * h];
    if points.len() < 3 || w == 0 || h == 0 {
        return out;
    }

    // Only the rows the polygon actually touches.
    let (mut top, mut bottom) = (f32::MAX, f32::MIN);
    for p in points {
        top = top.min(p[1]);
        bottom = bottom.max(p[1]);
    }
    let first = (top.floor().max(0.0)) as usize;
    let last = ((bottom.ceil()) as isize).clamp(0, h as isize) as usize;

    let mut crossings: Vec<(f32, i32)> = Vec::with_capacity(points.len());
    let weight = 1.0 / SUB as f32;

    for row in first..last.min(h) {
        let out_row = &mut out[row * w..(row + 1) * w];
        for sub in 0..SUB {
            let y = row as f32 + (sub as f32 + 0.5) / SUB as f32;

            // Where this sub-row crosses each edge, and which way the edge was going.
            crossings.clear();
            for i in 0..points.len() {
                let a = points[i];
                let b = points[(i + 1) % points.len()];
                if (a[1] <= y) == (b[1] <= y) {
                    continue; // horizontal to this row, or entirely on one side of it
                }
                let t = (y - a[1]) / (b[1] - a[1]);
                crossings.push((a[0] + (b[0] - a[0]) * t, if b[1] > a[1] { 1 } else { -1 }));
            }
            if crossings.len() < 2 {
                continue;
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));

            // Nonzero winding: inside wherever the running direction count is not zero. Adjacent
            // facets that share an edge therefore join rather than cancelling, which even-odd
            // would not do.
            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                if winding != 0 {
                    add_span(out_row, pair[0].0, pair[1].0, weight);
                }
            }
        }
    }
    out
}

/// Add `weight` to every pixel between `x0` and `x1`, giving the two partly-covered pixels at the
/// ends only the fraction they actually contain.
fn add_span(row: &mut [f32], x0: f32, x1: f32, weight: f32) {
    let w = row.len() as f32;
    let x0 = x0.max(0.0);
    let x1 = x1.min(w);
    if x1 <= x0 {
        return;
    }
    let first = x0.floor() as usize;
    let last = (x1.ceil() as usize).min(row.len());
    if first >= row.len() {
        return;
    }
    if last - first == 1 {
        // Both ends inside one pixel.
        row[first] += (x1 - x0) * weight;
        return;
    }
    for (i, cell) in row.iter_mut().enumerate().take(last).skip(first) {
        let left = (i as f32).max(x0);
        let right = ((i + 1) as f32).min(x1);
        *cell += (right - left).max(0.0) * weight;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(cov: &[f32]) -> f32 {
        cov.iter().sum()
    }

    #[test]
    fn a_square_covers_exactly_its_own_area() {
        let cov = coverage(&[[2.0, 2.0], [10.0, 2.0], [10.0, 8.0], [2.0, 8.0]], 16, 16);
        assert!((total(&cov) - 48.0).abs() < 0.05, "got {}", total(&cov));
        assert!((cov[4 * 16 + 5] - 1.0).abs() < 1e-3, "inside should be solid");
        assert!(cov[0] < 1e-6, "outside should be empty");
    }

    #[test]
    fn a_half_covered_pixel_comes_back_half() {
        // A 1x1 square offset by half a pixel in each direction lands a quarter in four pixels.
        let cov = coverage(&[[1.5, 1.5], [2.5, 1.5], [2.5, 2.5], [1.5, 2.5]], 4, 4);
        for (x, y) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
            assert!((cov[y * 4 + x] - 0.25).abs() < 0.02, "({x},{y}) = {}", cov[y * 4 + x]);
        }
    }

    #[test]
    fn a_triangle_covers_half_its_box() {
        let cov = coverage(&[[0.0, 0.0], [20.0, 0.0], [0.0, 20.0]], 20, 20);
        assert!((total(&cov) - 200.0).abs() < 1.0, "got {}", total(&cov));
    }

    #[test]
    fn winding_direction_does_not_change_the_result() {
        let cw = coverage(&[[1.0, 1.0], [9.0, 1.0], [9.0, 9.0], [1.0, 9.0]], 10, 10);
        let ccw = coverage(&[[1.0, 9.0], [9.0, 9.0], [9.0, 1.0], [1.0, 1.0]], 10, 10);
        assert!((total(&cw) - total(&ccw)).abs() < 1e-3);
    }

    #[test]
    fn nothing_escapes_the_grid() {
        // A polygon mostly off-canvas must not index outside the buffer or wrap around it.
        let cov = coverage(&[[-50.0, -50.0], [8.0, -50.0], [8.0, 8.0], [-50.0, 8.0]], 10, 10);
        assert!((total(&cov) - 64.0).abs() < 0.2, "got {}", total(&cov));
        assert!(cov[9 * 10 + 9] < 1e-6, "far corner should be untouched");
    }

    #[test]
    fn coverage_never_exceeds_a_whole_pixel() {
        let cov = coverage(&[[0.0, 0.0], [40.0, 0.0], [40.0, 40.0], [0.0, 40.0]], 32, 32);
        for c in cov {
            assert!(c <= 1.0 + 1e-4, "pixel over-covered: {c}");
        }
    }

    #[test]
    fn a_degenerate_polygon_draws_nothing() {
        assert_eq!(total(&coverage(&[[1.0, 1.0], [2.0, 2.0]], 8, 8)), 0.0);
        assert_eq!(total(&coverage(&[], 8, 8)), 0.0);
        assert!(coverage(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], 0, 0).is_empty());
    }
}
