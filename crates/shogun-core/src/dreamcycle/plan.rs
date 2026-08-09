//! Dream Cycle job plan (FR-DC-03 sequence, FR-DC-04 idempotency & crash-resume). Pure: the job
//! ordering and the resume logic are computed from `job_runs` records, so a mid-cycle kill resumes
//! by *skipping the jobs already marked done* — never re-running them.
//!
//! The actual job effects (Batch-API consolidation, Warm→Cold demotion, …) are I/O the daemon runs
//! behind the [`DreamJob`] seam; this module only decides *which* jobs remain and *in what order*.

/// The seven Dream Cycle jobs, in execution order (FR-DC-03 + Plan D-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobKind {
    /// 1. Extract state-table update candidates from today's event log.
    Consolidation,
    /// 2. Summarise/compress the day's events for the Hot layer + search.
    Compression,
    /// 3. Apply candidates: name-match and conflict-detect against existing records.
    StateUpdate,
    /// 4. Recompute confidence + overdue/staleness for all state records (FR-ST-21).
    ConfidenceRecalc,
    /// 5. Demote Warm rows past 30 days to the int8 Cold partitions (FR-MEM-04).
    ColdDemotion,
    /// 6. Generate the Morning Brief (§6.8).
    MorningBrief,
    /// 7. Distill unprocessed approval feedback into lessons + run the lesson lifecycle
    ///    (Plan D-4, designs §5.3). Local rules only — no Batch call in v1.
    LessonDistillation,
}

/// The full nightly sequence (FR-DC-03).
pub const FULL_SEQUENCE: &[JobKind] = &[
    JobKind::Consolidation,
    JobKind::Compression,
    JobKind::StateUpdate,
    JobKind::ConfidenceRecalc,
    JobKind::ColdDemotion,
    JobKind::MorningBrief,
    JobKind::LessonDistillation,
];

/// The degraded (catch-up) sequence: local state maintenance only, **no Batch API** (FR-DC-01).
/// It applies already-known candidates and recomputes overdue/staleness so state does not rot when
/// a full cycle is missed; the Batch-API-dependent jobs wait for the next night.
pub const DEGRADED_SEQUENCE: &[JobKind] = &[JobKind::StateUpdate, JobKind::ConfidenceRecalc];

/// The persisted state of one job in `job_runs` (FR-DC-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Pending,
    Running,
    Done,
    Failed,
}

/// A `job_runs` record: which job, over which input range, and its state (FR-DC-04). The input
/// range makes double-application detectable — the same (kind, range) must be idempotent upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobRun {
    pub kind: JobKind,
    pub state: JobState,
    /// Inclusive start of the event-time range this job consumed (unix ms).
    pub input_from_ts: i64,
    /// Exclusive end of the range (unix ms).
    pub input_to_ts: i64,
}

/// Which sequence a cycle runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleKind {
    Full,
    Degraded,
}

impl CycleKind {
    pub fn sequence(self) -> &'static [JobKind] {
        match self {
            CycleKind::Full => FULL_SEQUENCE,
            CycleKind::Degraded => DEGRADED_SEQUENCE,
        }
    }
}

/// Compute the jobs still to run for `cycle`, given the `job_runs` already recorded for it
/// (FR-DC-04 resume). A job is skipped iff a record for its kind is `Done`; everything else
/// (Pending / Running / Failed / absent) is (re)scheduled, in sequence order. Because a killed
/// `Running` job is rescheduled and the effect is upsert-idempotent, resuming after a crash cannot
/// corrupt state.
pub fn remaining(cycle: CycleKind, runs: &[JobRun]) -> Vec<JobKind> {
    cycle
        .sequence()
        .iter()
        .copied()
        .filter(|k| !is_done(*k, runs))
        .collect()
}

/// The next job to run, or `None` when the cycle is complete.
pub fn next_job(cycle: CycleKind, runs: &[JobRun]) -> Option<JobKind> {
    cycle.sequence().iter().copied().find(|k| !is_done(*k, runs))
}

/// Whether the whole cycle is complete (every job in the sequence is `Done`).
pub fn is_complete(cycle: CycleKind, runs: &[JobRun]) -> bool {
    next_job(cycle, runs).is_none()
}

fn is_done(kind: JobKind, runs: &[JobRun]) -> bool {
    runs.iter().any(|r| r.kind == kind && r.state == JobState::Done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(kind: JobKind) -> JobRun {
        JobRun { kind, state: JobState::Done, input_from_ts: 0, input_to_ts: 100 }
    }

    #[test]
    fn full_sequence_is_the_seven_jobs_in_order() {
        assert_eq!(
            FULL_SEQUENCE,
            &[
                JobKind::Consolidation,
                JobKind::Compression,
                JobKind::StateUpdate,
                JobKind::ConfidenceRecalc,
                JobKind::ColdDemotion,
                JobKind::MorningBrief,
                JobKind::LessonDistillation,
            ]
        );
    }

    #[test]
    fn fresh_cycle_runs_everything() {
        assert_eq!(remaining(CycleKind::Full, &[]), FULL_SEQUENCE.to_vec());
        assert_eq!(next_job(CycleKind::Full, &[]), Some(JobKind::Consolidation));
        assert!(!is_complete(CycleKind::Full, &[]));
    }

    #[test]
    fn resume_skips_completed_prefix() {
        // Killed after the first two jobs → resume at StateUpdate.
        let runs = [done(JobKind::Consolidation), done(JobKind::Compression)];
        assert_eq!(next_job(CycleKind::Full, &runs), Some(JobKind::StateUpdate));
        assert_eq!(
            remaining(CycleKind::Full, &runs),
            vec![
                JobKind::StateUpdate,
                JobKind::ConfidenceRecalc,
                JobKind::ColdDemotion,
                JobKind::MorningBrief,
                JobKind::LessonDistillation
            ]
        );
    }

    #[test]
    fn failed_or_running_jobs_are_rescheduled_not_skipped() {
        let runs = [
            done(JobKind::Consolidation),
            JobRun { kind: JobKind::Compression, state: JobState::Failed, input_from_ts: 0, input_to_ts: 100 },
            JobRun { kind: JobKind::StateUpdate, state: JobState::Running, input_from_ts: 0, input_to_ts: 100 },
        ];
        // Compression (failed) is next — Running/Failed are not "done".
        assert_eq!(next_job(CycleKind::Full, &runs), Some(JobKind::Compression));
    }

    #[test]
    fn out_of_order_done_records_still_skip_correctly() {
        // Even if only a later job is marked done (unusual), only that one is skipped.
        let runs = [done(JobKind::ColdDemotion)];
        let rem = remaining(CycleKind::Full, &runs);
        assert!(!rem.contains(&JobKind::ColdDemotion));
        assert_eq!(rem.first(), Some(&JobKind::Consolidation));
        assert_eq!(rem.len(), FULL_SEQUENCE.len() - 1);
    }

    #[test]
    fn completed_cycle_reports_complete_and_no_next() {
        let runs: Vec<JobRun> = FULL_SEQUENCE.iter().map(|k| done(*k)).collect();
        assert!(is_complete(CycleKind::Full, &runs));
        assert_eq!(next_job(CycleKind::Full, &runs), None);
        assert!(remaining(CycleKind::Full, &runs).is_empty());
    }

    #[test]
    fn degraded_cycle_is_state_maintenance_only_no_batch_jobs() {
        // No Consolidation / Compression / ColdDemotion / MorningBrief / LessonDistillation
        // (Batch-API, heavy, or non-essential for a catch-up run).
        assert_eq!(DEGRADED_SEQUENCE, &[JobKind::StateUpdate, JobKind::ConfidenceRecalc]);
        let rem = remaining(CycleKind::Degraded, &[]);
        assert!(!rem.contains(&JobKind::Consolidation));
        assert!(!rem.contains(&JobKind::MorningBrief));
        assert!(!rem.contains(&JobKind::LessonDistillation));
    }
}
