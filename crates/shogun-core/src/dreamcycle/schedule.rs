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
use super::jobs::{
    classify_via_batch, Classifier, DbDreamRunner, LocalExtractiveSummarizer, LocalRuleClassifier,
    PrecomputedClassifier,
};
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

// ------------------------------------------------------------------------- the nightly window
// The window is local wall-clock (FR-DC-01: default 02:00–06:00 local). The adapter can read the
// clock and the zone offset but must not own the calendar math, so all of it is here and tested.

/// Default nightly window (FR-DC-01), local hours: `[START, END)`.
pub const DEFAULT_WINDOW_START_HOUR: u32 = 2;
pub const DEFAULT_WINDOW_END_HOUR: u32 = 6;

/// Local wall-clock date and hour, derived from a UTC instant plus the zone's offset. The adapter
/// supplies both (macOS `localtime_r` gives them together); everything downstream is pure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    /// `YYYYMMDD`, local.
    pub yyyymmdd: u32,
    /// Local hour, 0..=23.
    pub hour: u32,
}

/// Local date/hour for a UTC instant. `gmt_offset_secs` is seconds east of UTC (`-25200` for
/// UTC-7); the adapter reads it from the OS, so DST is already folded in.
pub fn local_time(unix_secs: i64, gmt_offset_secs: i32) -> LocalTime {
    let local = unix_secs + gmt_offset_secs as i64;
    LocalTime {
        yyyymmdd: yyyymmdd_from_days(local.div_euclid(86_400)),
        hour: (local.rem_euclid(86_400) / 3_600) as u32,
    }
}

/// Where the local hour sits relative to the nightly window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowPosition {
    /// Inside the window: a satisfied gate runs the full cycle.
    pub within: bool,
    /// The window passed without a full run: the next idle runs the degraded catch-up.
    pub elapsed: bool,
}

/// Position of `hour` relative to `[start, end)`, handling a window that wraps midnight (a user who
/// sets 22:00–04:00). Before the window, both flags are false — there is nothing to do yet.
pub fn window_position(hour: u32, start_hour: u32, end_hour: u32) -> WindowPosition {
    if start_hour <= end_hour {
        WindowPosition { within: hour >= start_hour && hour < end_hour, elapsed: hour >= end_hour }
    } else {
        // Wrapping: the window is [start, 24) ∪ [0, end). "Elapsed" is the daytime gap between them.
        let within = hour >= start_hour || hour < end_hour;
        WindowPosition { within, elapsed: !within && hour >= end_hour }
    }
}

/// The cycle id (`YYYYMMDD`) for the *night* this instant belongs to — the ledger key that makes
/// "a full cycle already ran tonight" answerable (FR-DC-01, FR-DC-04).
///
/// With a window that wraps midnight, 01:00 belongs to the night that started the previous evening,
/// so it must not get a fresh id — otherwise the cycle would restart halfway through.
pub fn cycle_id(unix_secs: i64, gmt_offset_secs: i32, start_hour: u32, end_hour: u32) -> String {
    let local = local_time(unix_secs, gmt_offset_secs);
    let belongs_to_previous_day = start_hour > end_hour && local.hour < end_hour;
    let at = if belongs_to_previous_day { unix_secs - 86_400 } else { unix_secs };
    format!("{:08}", local_time(at, gmt_offset_secs).yyyymmdd)
}

/// `YYYYMMDD` for a count of days since the epoch (civil-from-days, Howard Hinnant) — no date
/// dependency, and correct across leap years and century rules.
fn yyyymmdd_from_days(days: i64) -> u32 {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y.max(0) as u32) * 10_000 + (m as u32) * 100 + d as u32
}

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
        // The Compression summariser seam: the Linux/on-device-sync path uses the network-free
        // local-extractive default (the Batch abstractive summariser is a separate on-device PR).
        let summarizer = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(self.db, self.classifier, &summarizer, now_ms);
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
/// async. Generic over the [`BatchLane`](crate::llm::anthropic::BatchLane) — the direct Anthropic
/// client (dev) and the relay client (shipping, docs/batch-relay-design.md) both fit — so it is
/// Linux-testable with a mock transport (no network).
pub async fn run_batch_cycle<B, F, Fut>(
    db: &Db,
    batch_client: &B,
    conditions: &RunConditions,
    cycle_id: &str,
    now_ms: i64,
    max_polls: u32,
    sleep: F,
) -> Result<GatedRun, crate::llm::LlmError>
where
    B: crate::llm::anthropic::BatchLane,
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
            let summarizer = LocalExtractiveSummarizer;
            let runner = DbDreamRunner::new(db, &pc, &summarizer, now_ms);
            let report = run_cycle(db, &runner, cycle_id, CycleKind::Full, from_ts, to_ts);
            Ok(GatedRun::Ran { cycle: CycleKind::Full, report })
        }
        RunDecision::Degraded => {
            // No Batch work in a catch-up (FR-DC-01); the classifier is never consulted by the
            // degraded sequence, so the local-rule one is a safe unused placeholder.
            let classifier = LocalRuleClassifier;
            let summarizer = LocalExtractiveSummarizer;
            let runner = DbDreamRunner::new(db, &classifier, &summarizer, now_ms);
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
    fn local_time_shifts_by_the_zone_offset() {
        // 2026-07-25T01:30:00Z
        let utc = 1_784_943_000;
        assert_eq!(local_time(utc, 0), LocalTime { yyyymmdd: 20_260_725, hour: 1 });
        // UTC+9: same instant is 10:30 on the 25th
        assert_eq!(local_time(utc, 9 * 3600), LocalTime { yyyymmdd: 20_260_725, hour: 10 });
        // UTC-7: still the 24th, 18:30 — the date must roll back, not just the hour
        assert_eq!(local_time(utc, -7 * 3600), LocalTime { yyyymmdd: 20_260_724, hour: 18 });
    }

    #[test]
    fn civil_dates_survive_leap_days_and_century_rules() {
        // 2024-02-29 (leap), 2000-02-29 (400-year rule), 1900-03-01 (100-year rule: no Feb 29)
        assert_eq!(local_time(1_709_208_000, 0).yyyymmdd, 20_240_229);
        assert_eq!(local_time(951_825_600, 0).yyyymmdd, 20_000_229);
        assert_eq!(local_time(-2_203_848_000, 0).yyyymmdd, 19_000_301);
    }

    #[test]
    fn the_default_window_is_two_to_six_local() {
        let at = |h| window_position(h, DEFAULT_WINDOW_START_HOUR, DEFAULT_WINDOW_END_HOUR);
        // before: nothing to do yet
        assert_eq!(at(1), WindowPosition { within: false, elapsed: false });
        // inside: end hour is exclusive
        assert!(at(2).within && at(5).within);
        assert!(!at(6).within);
        // after: the catch-up case
        assert_eq!(at(6), WindowPosition { within: false, elapsed: true });
        assert!(at(23).elapsed);
    }

    #[test]
    fn a_window_that_wraps_midnight_stays_one_window() {
        // 22:00–04:00
        let at = |h| window_position(h, 22, 4);
        assert!(at(22).within && at(23).within && at(0).within && at(3).within);
        assert!(!at(4).within, "end hour is exclusive on the far side of midnight too");
        // the daytime gap is the elapsed case; the pre-window evening is not
        assert!(at(4).elapsed && at(12).elapsed);
        assert!(!at(0).elapsed, "inside the window is never also elapsed");
    }

    #[test]
    fn a_wrapping_window_keeps_one_cycle_id_across_midnight() {
        // 23:30 local and 01:30 local the next morning are the same night (window 22:00–04:00),
        // so they must share a ledger key — otherwise the cycle restarts halfway through.
        let evening = 1_784_935_800; // 2026-07-24T23:30:00Z
        let small_hours = 1_784_943_200; // 2026-07-25T01:33:20Z
        assert_eq!(cycle_id(evening, 0, 22, 4), "20260724");
        assert_eq!(cycle_id(small_hours, 0, 22, 4), "20260724");
        // with the default (non-wrapping) window, the small hours are simply that day's night
        assert_eq!(
            cycle_id(small_hours, 0, DEFAULT_WINDOW_START_HOUR, DEFAULT_WINDOW_END_HOUR),
            "20260725"
        );
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
