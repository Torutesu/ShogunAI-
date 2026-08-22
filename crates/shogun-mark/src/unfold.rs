//! The arrival, as a function of time.
//!
//! A second copy of the `shogun-unfold-*` keyframes in
//! `apps/desktop/src/styles/logo-motion.css`, because AppKit draws the Dock and menu-bar icons and
//! never sees that stylesheet. Same durations, same delays, same easing, same overshoot — a Dock
//! icon that folded to a different rhythm than the window it launches would read as two products.
//!
//! One deliberate difference: the CSS container animation also rises by a flat `7px`, which is 27%
//! of a 26px mark and 2% of a 338px one. A fixed pixel rise has no meaning at icon scale, so only
//! the scale and the fade are carried over.

use crate::geometry::Facet;

/// How long the whole arrival takes. The container animation is the longest of the four.
pub const DURATION_MS: f32 = 760.0;

/// `cubic-bezier(0.22, 1, 0.36, 1)` — the ease every part of the fold settles on.
const EASE: [f32; 4] = [0.22, 1.0, 0.36, 1.0];

/// Solve the CSS timing function: given progress along x, the eased progress along y.
fn ease(x: f32) -> f32 {
    let (x1, y1, x2, y2) = (EASE[0], EASE[1], EASE[2], EASE[3]);
    let x = x.clamp(0.0, 1.0);
    // Bisection rather than Newton: it cannot diverge, and 24 halvings put us well inside the
    // precision a rasterised pixel can show.
    let bez = |a: f32, b: f32, t: f32| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * a + 3.0 * mt * t * t * b + t * t * t
    };
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        if bez(x1, x2, mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez(y1, y2, (lo + hi) * 0.5)
}

/// One `@keyframes` stop: the offset, the fold scale, and the facet's own opacity.
struct Stop {
    at: f32,
    scale: f32,
    alpha: Option<f32>,
}

/// A facet's timing and keyframes, straight off the stylesheet.
struct Track {
    delay_ms: f32,
    duration_ms: f32,
    stops: [Stop; 4],
}

fn track(facet: Facet) -> Track {
    // Each facet carries a little past its crease at 55% and settles back — that overshoot is what
    // keeps the fold reading as taut paper rather than a tween. The facet also fades up as it turns
    // flat to the viewer, which is what a face turning edge-on actually does.
    match facet {
        Facet::Peak => Track {
            delay_ms: 0.0,
            duration_ms: 480.0,
            stops: [
                Stop { at: 0.0, scale: 0.04, alpha: Some(0.30) },
                Stop { at: 0.55, scale: 1.04, alpha: Some(1.0) },
                Stop { at: 0.78, scale: 0.99, alpha: None },
                Stop { at: 1.0, scale: 1.0, alpha: None },
            ],
        },
        Facet::Wing => Track {
            delay_ms: 110.0,
            duration_ms: 540.0,
            stops: [
                Stop { at: 0.0, scale: 0.04, alpha: Some(0.24) },
                Stop { at: 0.55, scale: 1.04, alpha: Some(1.0) },
                Stop { at: 0.78, scale: 0.99, alpha: None },
                Stop { at: 1.0, scale: 1.0, alpha: None },
            ],
        },
        Facet::Blade => Track {
            delay_ms: 280.0,
            duration_ms: 440.0,
            stops: [
                Stop { at: 0.0, scale: 0.04, alpha: Some(0.20) },
                Stop { at: 0.55, scale: 1.06, alpha: Some(1.0) },
                Stop { at: 0.78, scale: 0.985, alpha: None },
                Stop { at: 1.0, scale: 1.0, alpha: None },
            ],
        },
    }
}

/// Where a facet is at `ms` into the arrival: how far open, and how solid.
///
/// Before its delay elapses the facet sits at its first keyframe — `animation-fill-mode: backwards`
/// in the stylesheet, and the reason the wings do not flash into view before their turn.
pub fn facet_at(facet: Facet, ms: f32) -> (f32, f32) {
    let t = track(facet);
    let p = ((ms - t.delay_ms) / t.duration_ms).clamp(0.0, 1.0);

    let mut scale = t.stops[0].scale;
    let mut alpha = t.stops[0].alpha.unwrap_or(1.0);
    for pair in t.stops.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if p < a.at {
            break;
        }
        // CSS applies the timing function across each keyframe interval, not once across the whole
        // animation, so the ease is re-applied per pair.
        let local = if p >= b.at {
            1.0
        } else {
            ease((p - a.at) / (b.at - a.at))
        };
        scale = a.scale + (b.scale - a.scale) * local;
        // A keyframe with no opacity of its own holds the last one that had it.
        if let Some(to) = b.alpha {
            let from = a.alpha.unwrap_or(alpha);
            alpha = from + (to - from) * local;
        }
    }
    (scale, alpha.clamp(0.0, 1.0))
}

/// The whole mark's arrival at `ms`: how big it is, and how visible.
///
/// `shogun-mark-in` in the stylesheet — 0.9 to 1 over the full duration, opaque by 40%.
pub fn mark_at(ms: f32) -> (f32, f32) {
    let p = (ms / DURATION_MS).clamp(0.0, 1.0);
    let scale = 0.9 + 0.1 * ease(p);
    let opacity = (p / 0.4).clamp(0.0, 1.0);
    (scale, opacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ease_runs_from_nothing_to_everything() {
        assert!(ease(0.0).abs() < 1e-4);
        assert!((ease(1.0) - 1.0).abs() < 1e-4);
        // Expo-out: most of the distance is covered early.
        assert!(ease(0.25) > 0.6, "got {}", ease(0.25));
        // And it never goes backwards.
        let mut last = -1.0;
        for i in 0..=100 {
            let y = ease(i as f32 / 100.0);
            assert!(y >= last - 1e-6, "ease dipped at {i}");
            last = y;
        }
    }

    #[test]
    fn every_facet_starts_shut_and_ends_open() {
        for facet in Facet::ALL {
            let (scale, alpha) = facet_at(facet, 0.0);
            assert!(scale < 0.05, "{facet:?} started open at {scale}");
            assert!(alpha < 0.35, "{facet:?} started solid at {alpha}");

            let (scale, alpha) = facet_at(facet, DURATION_MS);
            assert!((scale - 1.0).abs() < 1e-3, "{facet:?} ended at {scale}");
            assert!((alpha - 1.0).abs() < 1e-3, "{facet:?} ended at {alpha}");
        }
    }

    #[test]
    fn a_facet_waits_its_turn() {
        // The stagger is the whole point: centre first, then the wings, then the blades.
        let (peak, _) = facet_at(Facet::Peak, 110.0);
        let (wing, _) = facet_at(Facet::Wing, 110.0);
        let (blade, _) = facet_at(Facet::Blade, 110.0);
        assert!(peak > wing, "peak {peak} should lead the wing {wing}");
        assert!(wing >= blade, "wing {wing} should lead the blade {blade}");
        // The blade has not started at all yet — it is held at its first keyframe.
        assert!((blade - 0.04).abs() < 1e-3, "blade moved early: {blade}");
    }

    #[test]
    fn the_fold_carries_past_its_crease_before_settling() {
        // Somewhere in the middle every facet is wider than it ends up.
        for facet in Facet::ALL {
            let peak_scale = (0..=76)
                .map(|i| facet_at(facet, i as f32 * 10.0).0)
                .fold(0.0_f32, f32::max);
            assert!(peak_scale > 1.02, "{facet:?} never overshot: {peak_scale}");
        }
    }

    #[test]
    fn the_mark_settles_at_its_own_size() {
        let (scale, opacity) = mark_at(0.0);
        assert!((scale - 0.9).abs() < 1e-3 && opacity.abs() < 1e-4);
        let (scale, opacity) = mark_at(DURATION_MS);
        assert!((scale - 1.0).abs() < 1e-3 && (opacity - 1.0).abs() < 1e-4);
        // Opaque well before the fold finishes.
        assert!((mark_at(DURATION_MS * 0.4).1 - 1.0).abs() < 1e-3);
    }

    #[test]
    fn time_past_the_end_is_just_the_end() {
        for facet in Facet::ALL {
            assert_eq!(facet_at(facet, DURATION_MS), facet_at(facet, DURATION_MS * 4.0));
        }
        assert_eq!(mark_at(DURATION_MS), mark_at(9_999.0));
    }
}
