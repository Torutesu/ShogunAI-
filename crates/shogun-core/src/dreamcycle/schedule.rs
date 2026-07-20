//! Dream Cycle scheduling glue (WP3.4, §6.7, feature `db`). The *when* and *over-what* of a nightly
//! run — the pure decisions that sit between the macOS trigger (a timer + wall-clock read, on-device)
//! and [`run::decide_and_run`](super::run).
//!
//! The macOS scheduler wakes on a timer, reads the wall clock / idle / power state into
//! [`RunConditions`](super::gate::RunConditions), then calls [`DreamScheduler::tick`]. This module
//! owns the input-window math (what event range a cycle consumes) and the cycle-id derivation, so
//! that logic stays Linux-testable; the adapter contributes only the platform reads it alone can do.

use crate::daemon::Db;

use super::gate::RunConditions;
use super::jobs::{Classifier, DbDreamRunner};
use super::run::{decide_and_run, GatedRun};

/// The event-time window `[from_ts, to_ts)` a cycle should consume: from the high-water mark of the
/// last completed consolidation up to `now`. On the very first run (no prior consolidation) it falls
/// back to `now - default_lookback_ms` so an install with history still gets a bounded first window.
pub fn input_range(last_consolidated_to: Option<i64>, now_ms: i64, default_lookback_ms: i64) -> (i64, i64) {
    let from = last_consolidated_to.unwrap_or_else(|| now_ms - default_lookback_ms);
    // never invert: a clock skew (last > now) collapses to an empty range at `now`.
    (from.min(now_ms), now_ms)
}

/// Default first-run lookback if no cycle has ever completed: one day.
pub const DEFAULT_LOOKBACK_MS: i64 = 24 * 60 * 60 * 1000;

/// Drives one scheduled evaluation against the real DB. Holds the shared handle and the injected
/// classifier (invariant 5: only a Batch/Select-KK classifier may touch a model; the Linux build
/// injects the local-rule one).
pub struct DreamScheduler<'a, C: Classifier> {
    db: &'a Db,
    classifier: &'a C,
}

impl<'a, C: Classifier> DreamScheduler<'a, C> {
    pub fn new(db: &'a Db, classifier: &'a C) -> Self {
        Self { db, classifier }
    }

    /// Evaluate the run conditions and, if the gate allows, run (or resume) the cycle for `cycle_id`
    /// over the input window derived from the ledger. `now_ms` is the adapter's wall clock. Returns
    /// the gated outcome (Skipped with a reason, or Ran with a report).
    pub fn tick(&self, conditions: &RunConditions, cycle_id: &str, now_ms: i64) -> GatedRun {
        let (from_ts, to_ts) =
            input_range(self.db.last_consolidated_to(), now_ms, DEFAULT_LOOKBACK_MS);
        let runner = DbDreamRunner::new(self.db, self.classifier, now_ms);
        decide_and_run(self.db, &runner, conditions, cycle_id, from_ts, to_ts)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::dreamcycle::gate::{RunConditions, IDLE_THRESHOLD_MS};
    use crate::dreamcycle::jobs::LocalRuleClassifier;
    use crate::dreamcycle::run::GatedRun;
    use std::sync::Arc;

    fn db_at(now: i64) -> Db {
        Db::open_in_memory(Arc::new(move || now)).unwrap()
    }

    #[test]
    fn input_range_uses_the_high_water_mark() {
        assert_eq!(input_range(Some(500), 1_000, 9_999), (500, 1_000));
    }

    #[test]
    fn input_range_falls_back_to_lookback_on_first_run() {
        assert_eq!(input_range(None, 1_000, 400), (600, 1_000));
    }

    #[test]
    fn input_range_never_inverts_under_clock_skew() {
        // last consolidated is (impossibly) in the future → clamp to an empty [now, now) range
        assert_eq!(input_range(Some(2_000), 1_000, 400), (1_000, 1_000));
    }

    #[test]
    fn tick_skips_when_user_is_active() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        let clf = LocalRuleClassifier;
        let sched = DreamScheduler::new(&db, &clf);
        let active = RunConditions {
            within_window: true,
            window_elapsed: false,
            idle_ms: 0,
            screen_locked: false,
            power_connected: true,
            battery_pct: 100,
            full_run_done_today: false,
        };
        assert!(matches!(sched.tick(&active, "c1", now), GatedRun::Skipped(_)));
    }

    #[test]
    fn tick_runs_a_full_cycle_over_the_derived_window() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        // a capture inside the last-day window carries an actionable sentence
        db.capture(&super_ev(now - 1000, "I'll send the report.", "h1")).unwrap();
        let clf = LocalRuleClassifier;
        let sched = DreamScheduler::new(&db, &clf);
        let idle = RunConditions {
            within_window: true,
            window_elapsed: false,
            idle_ms: IDLE_THRESHOLD_MS,
            screen_locked: false,
            power_connected: true,
            battery_pct: 100,
            full_run_done_today: false,
        };
        match sched.tick(&idle, "c1", now) {
            GatedRun::Ran { report, .. } => {
                assert!(report.is_complete());
                // consolidation classified the window → a commitment candidate exists
                assert_eq!(db.commitments_due(now).len(), 1);
            }
            other => panic!("expected a run, got {other:?}"),
        }
    }

    fn super_ev<'a>(ts: i64, content: &'a str, hash: &'a str) -> shogun_memory::event_log::NewEvent<'a> {
        shogun_memory::event_log::NewEvent {
            ts,
            source: "capture",
            kind: "text",
            app_bundle_id: None,
            window_title: None,
            content,
            content_hash: hash,
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        }
    }
}
