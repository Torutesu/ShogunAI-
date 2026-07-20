//! The Dream Cycle execution loop (WP3.4, §6.7). Feature `db`. Wires the pure pieces —
//! [`gate::decide`], the resumable [`plan`], and the persisted job ledger — into the actual nightly
//! run, driving job effects through a [`DreamJobRunner`] seam (Batch-API consolidation, Warm→Cold
//! demotion, Brief generation live behind it, so the loop is Linux-testable without a network).
//!
//! Crash-resume + idempotency (FR-DC-04): the loop starts from [`Db::resume`], so a cycle killed
//! mid-run continues by skipping the jobs already `done`. Each job is marked `running` before and
//! `done`/`failed` after. On a job failure the loop **stops and leaves the cycle resumable** — the
//! failed job (and everything after it) re-runs next time, and because effects are upsert-idempotent
//! that cannot corrupt state (FR-DC-05: local features are unaffected regardless).

use crate::daemon::Db;

use super::gate::{decide, RunConditions, RunDecision};
use super::plan::{CycleKind, JobKind, JobState};

/// Runs one Dream Cycle job's effect. The real implementation calls the Batch API / moves layers;
/// tests inject a double. Returns an error string (no captured text) on failure.
pub trait DreamJobRunner {
    fn run(&self, kind: JobKind, input_from_ts: i64, input_to_ts: i64) -> Result<(), String>;
}

/// What a cycle run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleReport {
    /// Jobs that completed this run, in order.
    pub completed: Vec<JobKind>,
    /// The job that failed (if any) and its error — the loop stopped here, leaving it resumable.
    pub failed: Option<(JobKind, String)>,
}

impl CycleReport {
    /// True if the whole remaining sequence completed without a failure.
    pub fn is_complete(&self) -> bool {
        self.failed.is_none()
    }
}

/// Run (or resume) a cycle: execute the still-to-do jobs in order, persisting each transition.
/// Stops at the first failure, leaving the cycle resumable.
pub fn run_cycle<R: DreamJobRunner>(
    db: &Db,
    runner: &R,
    cycle_id: &str,
    cycle: CycleKind,
    input_from_ts: i64,
    input_to_ts: i64,
) -> CycleReport {
    let mut completed = Vec::new();
    for kind in db.resume(cycle_id, cycle) {
        db.record_job(cycle_id, kind, JobState::Running, input_from_ts, input_to_ts);
        match runner.run(kind, input_from_ts, input_to_ts) {
            Ok(()) => {
                db.record_job(cycle_id, kind, JobState::Done, input_from_ts, input_to_ts);
                completed.push(kind);
            }
            Err(e) => {
                db.record_job(cycle_id, kind, JobState::Failed, input_from_ts, input_to_ts);
                return CycleReport { completed, failed: Some((kind, e)) };
            }
        }
    }
    CycleReport { completed, failed: None }
}

/// The outcome of a gated cycle attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatedRun {
    /// The gate declined; carries the decision (Skip reason).
    Skipped(RunDecision),
    /// The gate allowed a run; carries its report and which sequence ran (Full or Degraded).
    Ran { cycle: CycleKind, report: CycleReport },
}

/// Check the run conditions (FR-DC-01) and, if allowed, run the appropriate cycle. A `Full`
/// decision runs the full sequence; a `Degraded` (missed-window catch-up) runs the state-only one.
pub fn decide_and_run<R: DreamJobRunner>(
    db: &Db,
    runner: &R,
    conditions: &RunConditions,
    cycle_id: &str,
    input_from_ts: i64,
    input_to_ts: i64,
) -> GatedRun {
    match decide(conditions) {
        RunDecision::Full => GatedRun::Ran {
            cycle: CycleKind::Full,
            report: run_cycle(db, runner, cycle_id, CycleKind::Full, input_from_ts, input_to_ts),
        },
        RunDecision::Degraded => GatedRun::Ran {
            cycle: CycleKind::Degraded,
            report: run_cycle(db, runner, cycle_id, CycleKind::Degraded, input_from_ts, input_to_ts),
        },
        skip => GatedRun::Skipped(skip),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dreamcycle::gate::{RunConditions, SkipReason, IDLE_THRESHOLD_MS};
    use std::cell::RefCell;
    use std::sync::Arc;

    fn db() -> Db {
        Db::open_in_memory(Arc::new(|| 1)).unwrap()
    }

    /// A runner that succeeds, but fails on a designated job kind (once).
    struct Runner {
        calls: RefCell<Vec<JobKind>>,
        fail_on: Option<JobKind>,
    }
    impl Runner {
        fn new(fail_on: Option<JobKind>) -> Self {
            Self { calls: RefCell::new(Vec::new()), fail_on }
        }
    }
    impl DreamJobRunner for Runner {
        fn run(&self, kind: JobKind, _from: i64, _to: i64) -> Result<(), String> {
            self.calls.borrow_mut().push(kind);
            if self.fail_on == Some(kind) {
                Err("batch api unavailable".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn full_cycle_runs_all_six_in_order() {
        let db = db();
        let runner = Runner::new(None);
        let report = run_cycle(&db, &runner, "c1", CycleKind::Full, 0, 100);
        assert!(report.is_complete());
        assert_eq!(report.completed, super::super::plan::FULL_SEQUENCE.to_vec());
        // nothing left to resume
        assert!(db.resume("c1", CycleKind::Full).is_empty());
    }

    #[test]
    fn failure_stops_and_leaves_the_cycle_resumable() {
        let db = db();
        // fail on the 3rd job (StateUpdate)
        let runner = Runner::new(Some(JobKind::StateUpdate));
        let report = run_cycle(&db, &runner, "c2", CycleKind::Full, 0, 100);
        assert_eq!(report.failed, Some((JobKind::StateUpdate, "batch api unavailable".into())));
        assert_eq!(report.completed, vec![JobKind::Consolidation, JobKind::Compression]);

        // resume: the failed job is first again, the two done ones are skipped
        let todo = db.resume("c2", CycleKind::Full);
        assert_eq!(todo.first(), Some(&JobKind::StateUpdate));
        assert!(!todo.contains(&JobKind::Consolidation));

        // a second run (now healthy) finishes the rest
        let runner2 = Runner::new(None);
        let report2 = run_cycle(&db, &runner2, "c2", CycleKind::Full, 0, 100);
        assert!(report2.is_complete());
        assert_eq!(report2.completed.first(), Some(&JobKind::StateUpdate));
        // the retry did not re-run the already-done jobs
        assert!(!runner2.calls.borrow().contains(&JobKind::Consolidation));
    }

    #[test]
    fn gate_skip_does_not_run_anything() {
        let db = db();
        let runner = Runner::new(None);
        // active user → skip
        let cond = RunConditions {
            within_window: true,
            window_elapsed: false,
            idle_ms: 0,
            screen_locked: false,
            power_connected: true,
            battery_pct: 100,
            full_run_done_today: false,
        };
        let out = decide_and_run(&db, &runner, &cond, "c3", 0, 100);
        assert_eq!(out, GatedRun::Skipped(RunDecision::Skip(SkipReason::NotIdle)));
        assert!(runner.calls.borrow().is_empty());
    }

    #[test]
    fn degraded_catch_up_runs_only_state_jobs() {
        let db = db();
        let runner = Runner::new(None);
        // missed window but idle+powered → degraded
        let cond = RunConditions {
            within_window: false,
            window_elapsed: true,
            idle_ms: IDLE_THRESHOLD_MS,
            screen_locked: false,
            power_connected: true,
            battery_pct: 100,
            full_run_done_today: false,
        };
        let out = decide_and_run(&db, &runner, &cond, "c4", 0, 100);
        match out {
            GatedRun::Ran { cycle, report } => {
                assert_eq!(cycle, CycleKind::Degraded);
                assert!(report.is_complete());
                assert_eq!(report.completed, super::super::plan::DEGRADED_SEQUENCE.to_vec());
            }
            other => panic!("expected a degraded run, got {other:?}"),
        }
        // no Batch-API job (Consolidation/Compression/…) ran
        assert!(!runner.calls.borrow().contains(&JobKind::Consolidation));
    }
}
