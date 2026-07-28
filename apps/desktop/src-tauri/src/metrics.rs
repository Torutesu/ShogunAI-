//! Live SLO samples for the Full UI's health pane.
//!
//! The spike's `Recorder` is a drain-once ring aimed at a JSONL file for offline analysis — read
//! it and the samples are gone. The health pane needs the opposite: a small window that can be
//! summarised repeatedly while the app runs. So this keeps a bounded ring per metric and computes
//! percentiles on demand.
//!
//! Two rules it inherits rather than reinvents:
//! - Percentiles come from `spike_harness::stats::Percentiles`, which returns `None` for an empty
//!   slice on purpose — "a percentile of nothing is not zero". An unmeasured metric must reach the
//!   window as absent, never as a passing 0ms.
//! - Thresholds come from `spike_harness::slo`, the single source of truth for the SLO table
//!   (spec §4.1). Nothing here restates a number from CLAUDE.md.
//!
//! Samples live in memory only: they describe this run, and persisting latency history would be a
//! schema change for data nobody has asked to keep across restarts.

use std::sync::Mutex;

use spike_harness::stats::Percentiles;

/// Samples kept per metric. At the observed rates (an expand is a user action; cache updates
/// follow focus switches) this is roughly a working session, which is the window the pane means
/// by "right now" — and it bounds memory regardless of uptime.
const WINDOW: usize = 512;

/// One metric's rolling window.
#[derive(Default)]
struct Ring(Vec<f64>);

impl Ring {
    fn push(&mut self, v: f64) {
        if self.0.len() == WINDOW {
            self.0.remove(0);
        }
        self.0.push(v);
    }
}

/// The metrics the health pane reports. One lock per metric would be finer-grained, but these are
/// written a handful of times a minute and read once per window open.
#[derive(Default)]
pub struct SloRegister {
    expand_ms: Mutex<Ring>,
    cache_update_ms: Mutex<Ring>,
    first_token_ms: Mutex<Ring>,
    /// No call site yet — the harness samples CPU out-of-process and local search isn't
    /// instrumented. They stay in `rows()` so the pane lists them as unmeasured rather than
    /// pretending the SLO table is shorter than it is.
    local_search_ms: Mutex<Ring>,
    idle_cpu_pct: Mutex<Ring>,
    /// Answers produced, and how many of them cited at least one source (FR-CF/grounding). A
    /// counter rather than a ring: the pane wants the rate over the run, not a distribution.
    answers: Mutex<(u64, u64)>,
}

/// A metric ready to draw: percentiles plus the target it is judged against.
pub struct SloSample {
    pub name: &'static str,
    pub p50: Option<f64>,
    pub p95: Option<f64>,
    pub target: &'static str,
    pub within_target: bool,
}

impl SloRegister {
    pub fn record_expand_ms(&self, v: f64) {
        push(&self.expand_ms, v);
    }
    pub fn record_cache_update_ms(&self, v: f64) {
        push(&self.cache_update_ms, v);
    }
    pub fn record_first_token_ms(&self, v: f64) {
        push(&self.first_token_ms, v);
    }

    /// Record that an answer was produced, and whether it carried at least one citation.
    pub fn record_answer(&self, grounded: bool) {
        if let Ok(mut a) = self.answers.lock() {
            a.0 += 1;
            if grounded {
                a.1 += 1;
            }
        }
    }

    /// Grounding rate over this run, or `None` before the first answer — a rate over zero answers
    /// is undefined, not 0%.
    pub fn grounding_pct(&self) -> Option<u8> {
        let (total, grounded) = *self.answers.lock().ok()?;
        if total == 0 {
            return None;
        }
        Some(((grounded as f64 / total as f64) * 100.0).round() as u8)
    }

    /// The SLO table as the pane draws it. Rows with no samples carry `None` percentiles and the
    /// window renders them as not-yet-measured.
    pub fn rows(&self) -> Vec<SloSample> {
        vec![
            row("Panel expand", &self.expand_ms, spike_harness::slo::EXPAND_MS, "100ms"),
            row("Cache refresh", &self.cache_update_ms, spike_harness::slo::CACHE_UPDATE_MS, "300ms"),
            row("Local search", &self.local_search_ms, spike_harness::slo::LOCAL_SEARCH_MS, "500ms"),
            row("First token", &self.first_token_ms, 1000.0, "1s"),
            row("Idle CPU", &self.idle_cpu_pct, spike_harness::slo::IDLE_CPU_PCT, "5%"),
        ]
    }
}

fn push(ring: &Mutex<Ring>, v: f64) {
    // A poisoned metrics lock must never take the app down with it — the sample is dropped and
    // the pane shows one fewer data point.
    if let Ok(mut r) = ring.lock() {
        r.push(v);
    }
}

fn row(name: &'static str, ring: &Mutex<Ring>, threshold: f64, target: &'static str) -> SloSample {
    let p = ring.lock().ok().and_then(|r| Percentiles::of(&r.0));
    SloSample {
        name,
        p50: p.map(|p| round1(p.p50)),
        p95: p.map(|p| round1(p.p95)),
        target,
        // Judged on p95, matching how the harness reports a verdict (slo::Verdict::le).
        within_target: p.map_or(true, |p| p.p95 <= threshold),
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmeasured_metrics_report_absent_not_zero() {
        let reg = SloRegister::default();
        let rows = reg.rows();
        assert!(rows.iter().all(|r| r.p50.is_none() && r.p95.is_none()));
        // Absent is not a failure: nothing has been measured, so nothing has missed its target.
        assert!(rows.iter().all(|r| r.within_target));
        assert_eq!(reg.grounding_pct(), None);
    }

    #[test]
    fn percentiles_and_verdict_track_samples() {
        let reg = SloRegister::default();
        for v in [50.0, 60.0, 70.0, 80.0, 400.0] {
            reg.record_expand_ms(v);
        }
        let Some(row) = reg.rows().into_iter().find(|r| r.name == "Panel expand") else {
            panic!("Panel expand row missing");
        };
        assert_eq!(row.p50, Some(70.0));
        // p95 of this window is the 400ms outlier, which blows the 100ms target.
        assert_eq!(row.p95, Some(400.0));
        assert!(!row.within_target);
    }

    #[test]
    fn grounding_is_a_rate_over_answers() {
        let reg = SloRegister::default();
        reg.record_answer(true);
        reg.record_answer(false);
        reg.record_answer(true);
        reg.record_answer(true);
        assert_eq!(reg.grounding_pct(), Some(75));
    }

    #[test]
    fn the_window_is_bounded() {
        let reg = SloRegister::default();
        for i in 0..(WINDOW + 50) {
            reg.record_expand_ms(i as f64);
        }
        let Ok(r) = reg.expand_ms.lock() else { panic!("metrics lock poisoned") };
        assert_eq!(r.0.len(), WINDOW);
        // The oldest samples fell off the front.
        assert_eq!(r.0[0], 50.0);
    }
}
