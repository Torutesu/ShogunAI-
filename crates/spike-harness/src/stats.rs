//! Aggregation: percentiles (nearest-rank) and the CPU moving average (spec §4.3, §4.2.3).
//!
//! Sample counts are at most a few thousand, so exact all-samples percentiles are used;
//! no streaming approximation is needed.

use serde::Serialize;

/// p50/p95/p99/max plus n, computed by the nearest-rank method.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Percentiles {
    pub n: usize,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
}

impl Percentiles {
    /// Compute over `values`. Returns `None` for an empty slice (a percentile of
    /// nothing is not zero — the report must show "no data", not a passing 0ms).
    pub fn of(values: &[f64]) -> Option<Percentiles> {
        if values.is_empty() {
            return None;
        }
        let mut v: Vec<f64> = values.to_vec();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Some(Percentiles {
            n: v.len(),
            p50: nearest_rank(&v, 0.50),
            p95: nearest_rank(&v, 0.95),
            p99: nearest_rank(&v, 0.99),
            max: v[v.len() - 1],
        })
    }
}

/// Nearest-rank percentile. `p` in [0,1]. `sorted` must be ascending and non-empty.
fn nearest_rank(sorted: &[f64], p: f64) -> f64 {
    debug_assert!(!sorted.is_empty());
    let p = p.clamp(0.0, 1.0);
    // rank = ceil(p * n), 1-indexed, clamped to [1, n].
    let n = sorted.len();
    let rank = (p * n as f64).ceil().max(1.0) as usize;
    sorted[rank.min(n) - 1]
}

/// Fixed-window moving average over a stream of samples (spec §4.2.3:
/// 5s sampling, 12-sample = 1-minute window).
#[derive(Debug, Clone)]
pub struct MovingAverage {
    window: usize,
    buf: std::collections::VecDeque<f64>,
    sum: f64,
}

impl MovingAverage {
    pub fn new(window: usize) -> Self {
        assert!(window > 0, "window must be positive");
        Self {
            window,
            buf: std::collections::VecDeque::with_capacity(window),
            sum: 0.0,
        }
    }

    /// Push a sample, return the current average once the window is full.
    /// Returns `None` while the window is still filling (no premature averages).
    pub fn push(&mut self, sample: f64) -> Option<f64> {
        self.buf.push_back(sample);
        self.sum += sample;
        if self.buf.len() > self.window {
            if let Some(old) = self.buf.pop_front() {
                self.sum -= old;
            }
        }
        if self.buf.len() == self.window {
            Some(self.sum / self.window as f64)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_none_not_zero() {
        assert!(Percentiles::of(&[]).is_none());
    }

    #[test]
    fn nearest_rank_known_values() {
        let vals: Vec<f64> = (1..=100).map(|x| x as f64).collect();
        let p = Percentiles::of(&vals).expect("pct");
        assert_eq!(p.n, 100);
        assert_eq!(p.p50, 50.0); // ceil(0.50*100)=50 → index 50
        assert_eq!(p.p95, 95.0);
        assert_eq!(p.p99, 99.0);
        assert_eq!(p.max, 100.0);
    }

    #[test]
    fn single_value() {
        let p = Percentiles::of(&[42.0]).expect("pct");
        assert_eq!(p.p50, 42.0);
        assert_eq!(p.p95, 42.0);
        assert_eq!(p.max, 42.0);
    }

    #[test]
    fn moving_average_fills_then_slides() {
        let mut ma = MovingAverage::new(3);
        assert_eq!(ma.push(3.0), None);
        assert_eq!(ma.push(6.0), None);
        assert_eq!(ma.push(9.0), Some(6.0)); // (3+6+9)/3
        assert_eq!(ma.push(12.0), Some(9.0)); // (6+9+12)/3
    }
}
