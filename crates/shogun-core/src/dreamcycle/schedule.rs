//! Dream Cycle scheduling glue (WP3.4, §6.7, feature `db`). The *when* and *over-what* of a nightly
//! run — the pure decisions that sit between the macOS trigger (a timer + wall-clock read, on-device)
//! and [`run::decide_and_run`](super::run).
//!
//! The macOS scheduler wakes on a timer, reads the wall clock / idle / power state into
//! [`RunConditions`](super::gate::RunConditions), then calls [`DreamScheduler::tick`]. This module
//! owns the input-window math (what event range a cycle consumes) and the cycle-id derivation, so
//! that logic stays Linux-testable; the adapter contributes only the platform reads it alone can do.

use crate::daemon::Db;

use super::gate::{decide, RunConditions, RunDecision};
use super::jobs::{classify_via_batch, Classifier, DbDreamRunner, LocalRuleClassifier, PrecomputedClassifier};
use super::plan::CycleKind;
use super::run::{decide_and_run, run_cycle, GatedRun};

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

/// Run one **Batch-backed** cycle end-to-end (the on-device model path). This is the async wrapper
/// the macOS scheduler awaits: it gates on `conditions`, and only for a **Full** decision does it
/// pay for the Batch/Select-KK lane — read the window, classify via [`classify_via_batch`], wrap the
/// result in a [`PrecomputedClassifier`], then drive the sync cycle. A **Degraded** catch-up runs
/// the state-only sequence with no model call (FR-DC-01); a **Skip** does nothing.
///
/// Keeping the model call here (async) and the cycle sync means the `DreamJobRunner` never bridges
/// async. Generic over the transport, so it is Linux-testable with a mock (no network).
pub async fn run_batch_cycle<T, S, F, Fut>(
    db: &Db,
    batch_client: &crate::llm::anthropic::AnthropicBatchClient<T, S>,
    conditions: &RunConditions,
    cycle_id: &str,
    now_ms: i64,
    max_polls: u32,
    sleep: F,
) -> Result<GatedRun, crate::llm::LlmError>
where
    T: crate::llm::transport::HttpTransport,
    S: crate::llm::traceability::TraceabilitySink,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let (from_ts, to_ts) = input_range(db.last_consolidated_to(), now_ms, DEFAULT_LOOKBACK_MS);
    match decide(conditions) {
        RunDecision::Full => {
            // The one model call — only for a Full run, only over this window.
            let events = db.events_in_range(from_ts, to_ts);
            let classified = classify_via_batch(batch_client, &events, max_polls, sleep).await?;
            let pc = PrecomputedClassifier::new(classified);
            let runner = DbDreamRunner::new(db, &pc, now_ms);
            let report = run_cycle(db, &runner, cycle_id, CycleKind::Full, from_ts, to_ts);
            Ok(GatedRun::Ran { cycle: CycleKind::Full, report })
        }
        RunDecision::Degraded => {
            // No Batch work in a catch-up (FR-DC-01); the classifier is never consulted by the
            // degraded sequence, so the local-rule one is a safe unused placeholder.
            let classifier = LocalRuleClassifier;
            let runner = DbDreamRunner::new(db, &classifier, now_ms);
            let report = run_cycle(db, &runner, cycle_id, CycleKind::Degraded, from_ts, to_ts);
            Ok(GatedRun::Ran { cycle: CycleKind::Degraded, report })
        }
        skip => Ok(GatedRun::Skipped(skip)),
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

    #[tokio::test]
    async fn run_batch_cycle_full_persists_model_candidates() {
        use crate::llm::anthropic::{AnthropicBatchClient, AnthropicConfig};
        use crate::llm::traceability::RecordingSink;
        use crate::llm::transport::{HttpResponse, MockTransport};
        use crate::llm::{SelectKkKey, Secret};

        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        // an event in the window; the model (mocked) classifies it as a commitment
        db.capture(&super_ev(now - 1000, "the deck discussion", "h1")).unwrap();

        let transport = MockTransport::new([
            HttpResponse { status: 200, body: r#"{"id":"b","processing_status":"ended"}"#.into() },
            HttpResponse {
                status: 200,
                body: r#"{"custom_id":"1","result":{"type":"succeeded","message":{"content":[{"type":"text","text":"{\"commitments\":[{\"direction\":\"mine\",\"description\":\"send the deck\"}],\"open_loops\":[]}"}]}}}"#.into(),
            },
        ]);
        let client = AnthropicBatchClient::new(
            transport,
            RecordingSink::new(),
            SelectKkKey::new(Secret::new("kk-123456")),
            AnthropicConfig::new("claude-x"),
        );
        let idle = RunConditions {
            within_window: true,
            window_elapsed: false,
            idle_ms: IDLE_THRESHOLD_MS,
            screen_locked: false,
            power_connected: true,
            battery_pct: 100,
            full_run_done_today: false,
        };
        let out = run_batch_cycle(&db, &client, &idle, "c1", now, 3, || async {}).await.unwrap();
        assert!(matches!(out, GatedRun::Ran { cycle: CycleKind::Full, .. }));
        // the model's classification landed as a Medium-confidence commitment
        let commitments = db.commitments_due(now);
        assert_eq!(commitments.len(), 1);
        assert!((commitments[0].confidence - crate::dreamcycle::jobs::BATCH_CONFIDENCE).abs() < 1e-9);
    }

    #[tokio::test]
    async fn run_batch_cycle_skips_without_calling_the_model() {
        use crate::llm::anthropic::{AnthropicBatchClient, AnthropicConfig};
        use crate::llm::traceability::RecordingSink;
        use crate::llm::transport::MockTransport;
        use crate::llm::{SelectKkKey, Secret};

        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        // no responses queued — a Skip must not call the Batch lane
        let client = AnthropicBatchClient::new(
            MockTransport::new([]),
            RecordingSink::new(),
            SelectKkKey::new(Secret::new("kk-123456")),
            AnthropicConfig::new("claude-x"),
        );
        let active = RunConditions {
            within_window: true,
            window_elapsed: false,
            idle_ms: 0,
            screen_locked: false,
            power_connected: true,
            battery_pct: 100,
            full_run_done_today: false,
        };
        let out = run_batch_cycle(&db, &client, &active, "c1", now, 3, || async {}).await.unwrap();
        assert!(matches!(out, GatedRun::Skipped(_)));
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
