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
use super::plan::{CycleKind, JobKind};
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
/// With a window that wraps midnight, every hour before the window's start belongs to the night
/// that started the previous evening: 01:00 is that night still in progress (a fresh id there
/// would restart the cycle halfway through), and 12:00 is the elapsed daytime gap *after* it — a
/// degraded catch-up fired there must record its ledger rows under the elapsed night's id, never
/// under the id tonight's full run will derive. Sharing tonight's id would let the catch-up's
/// StateUpdate/ConfidenceRecalc rows pre-mark tonight's jobs done ([`remaining`](super::plan)
/// skips by job kind alone), so the freshly consolidated state would get no overdue/decay pass.
pub fn cycle_id(unix_secs: i64, gmt_offset_secs: i32, start_hour: u32, end_hour: u32) -> String {
    let local = local_time(unix_secs, gmt_offset_secs);
    let belongs_to_previous_day = start_hour > end_hour && local.hour < start_hour;
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
            // The one model call — only for a Full run, only when the ledger says Consolidation
            // still has to run for this cycle, only over this window, and only over the window's
            // CLOUD half. The ledger check is the crash-resume egress guard: a resumed cycle
            // whose Consolidation is already `Done` skips the job, so its classifier is never
            // consulted — paying the Batch lane then would send the window's captured text to
            // the cloud only to discard the results. Meeting text (`local_only`) never reaches
            // the relay: its consent covers live transcription, not nightly classification
            // (A-2). It still gets classified — by the same local rules the degraded cycle runs
            // — so a commitment made in a call lands in state either way, at local confidence
            // instead of batch.
            let needs_consolidation =
                db.resume(cycle_id, CycleKind::Full).contains(&JobKind::Consolidation);
            let classified = if needs_consolidation {
                let window = db.events_in_range_partitioned(from_ts, to_ts);
                let cloud =
                    classify_via_batch(batch_client, &window.cloud, max_polls, sleep).await?;
                let local = LocalRuleClassifier.classify(&window.local_only);
                cloud.into_iter().chain(local).collect()
            } else {
                Vec::new()
            };
            let pc = PrecomputedClassifier::new(classified);
            let summarizer = LocalExtractiveSummarizer;
            // Charm (issue #10) stays unset here on purpose: the extractive summariser cannot
            // write the line (trait default None). The Batch abstractive summariser PR adds
            // `.with_charm(...)` from the parsed Shougun.md alongside its Summarizer impl.
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
    fn a_daytime_catch_up_derives_the_elapsed_nights_id_not_tonights() {
        // Window 22:00–04:00. Noon sits in the elapsed gap after the night that started the
        // previous evening; tonight's 22:30 full run needs a fresh ledger key. If both derived
        // the same id, the catch-up's StateUpdate/ConfidenceRecalc rows would pre-mark tonight's
        // jobs done (`remaining` skips by job kind alone).
        let noon = 1_784_980_800; // 2026-07-25T12:00:00Z
        let tonight = 1_785_018_600; // 2026-07-25T22:30:00Z
        assert_eq!(cycle_id(noon, 0, 22, 4), "20260724", "the gap belongs to the elapsed night");
        assert_eq!(cycle_id(tonight, 0, 22, 4), "20260725", "tonight starts a new cycle");
    }

    #[test]
    fn a_full_cycle_after_a_daytime_catch_up_still_runs_every_job() {
        use crate::dreamcycle::plan::FULL_SEQUENCE;

        // Wrapping window 22:00–04:00: a degraded catch-up at noon and tonight's full run must
        // land in different ledgers, so the full run still gets its own StateUpdate and
        // ConfidenceRecalc pass over the freshly consolidated state.
        let noon_secs: i64 = 1_784_980_800; // 2026-07-25T12:00:00Z
        let night_secs: i64 = 1_785_018_600; // 2026-07-25T22:30:00Z
        let deg_id = cycle_id(noon_secs, 0, 22, 4);
        let full_id = cycle_id(night_secs, 0, 22, 4);

        let now = night_secs * 1000;
        let db = db_at(now);
        db.capture(&super_ev(noon_secs * 1000 - 1000, "I'll send the report.", "h1")).unwrap();
        let clf = LocalRuleClassifier;
        let sched = DreamScheduler::new(&db, &clf);

        // Noon: window elapsed without a full run → degraded catch-up under its own id.
        let elapsed = RunConditions {
            within_window: false,
            window_elapsed: true,
            idle_ms: IDLE_THRESHOLD_MS,
            screen_locked: false,
            power_connected: true,
            battery_pct: 100,
            full_run_done_today: false,
        };
        match sched.tick(&elapsed, &deg_id, noon_secs * 1000) {
            GatedRun::Ran { cycle: crate::dreamcycle::plan::CycleKind::Degraded, report } => {
                assert!(report.is_complete());
            }
            other => panic!("expected a degraded run, got {other:?}"),
        }

        // Tonight: the full sequence must run in its entirety — the catch-up's ledger rows must
        // not satisfy any of tonight's jobs.
        let idle = RunConditions {
            within_window: true,
            window_elapsed: false,
            idle_ms: IDLE_THRESHOLD_MS,
            screen_locked: false,
            power_connected: true,
            battery_pct: 100,
            full_run_done_today: false,
        };
        match sched.tick(&idle, &full_id, now) {
            GatedRun::Ran { cycle: crate::dreamcycle::plan::CycleKind::Full, report } => {
                assert_eq!(
                    report.completed,
                    FULL_SEQUENCE.to_vec(),
                    "tonight's full cycle must run every job, including the state pass"
                );
            }
            other => panic!("expected a full run, got {other:?}"),
        }
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

    #[tokio::test]
    async fn a_resumed_cycle_with_consolidation_done_never_calls_the_batch_lane() {
        // Crash-resume egress guard: tonight's cycle already completed Consolidation, then the
        // process died. On resume the ledger skips Consolidation and the classifier is never
        // consulted — so the Batch lane must not be paid (and the window's captured text must
        // not cross the wire) just to throw the results away.
        use crate::dreamcycle::plan::{JobKind, JobState};
        use crate::llm::anthropic::{AnthropicBatchClient, AnthropicConfig};
        use crate::llm::traceability::RecordingSink;
        use crate::llm::transport::MockTransport;
        use crate::llm::{Secret, SelectKkKey};

        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        // captured text inside the resumed window — the text an unguarded resume would re-send
        db.capture(&super_ev(now - 100, "the deck discussion", "h1")).unwrap();
        // the previous run of THIS cycle finished Consolidation before the crash
        assert!(db.record_job("c1", JobKind::Consolidation, JobState::Done, 0, now - 500));

        // no responses queued — any request against the lane fails the run
        let transport = std::sync::Arc::new(MockTransport::new([]));
        let client = AnthropicBatchClient::new(
            SharedTransport(transport.clone()),
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
        match out {
            GatedRun::Ran { cycle: CycleKind::Full, report } => {
                assert!(report.is_complete(), "the resumed remainder must finish: {report:?}");
                assert!(
                    !report.completed.contains(&JobKind::Consolidation),
                    "Consolidation was already done and must not re-run"
                );
            }
            other => panic!("expected a resumed full run, got {other:?}"),
        }
        assert!(
            transport.sent().is_empty(),
            "no request may reach the Batch lane when Consolidation is already done"
        );
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

    /// A [`MockTransport`](crate::llm::transport::MockTransport) the test keeps a handle to after
    /// the client takes ownership, so the requests the lane actually sent can be inspected.
    struct SharedTransport(std::sync::Arc<crate::llm::transport::MockTransport>);
    impl crate::llm::transport::HttpTransport for SharedTransport {
        fn send(
            &self,
            req: crate::llm::transport::HttpRequest,
        ) -> impl std::future::Future<
            Output = Result<crate::llm::transport::HttpResponse, crate::llm::transport::TransportError>,
        > + Send {
            self.0.send(req)
        }
    }

    #[tokio::test]
    async fn a_full_run_never_sends_meeting_text_to_the_batch_lane() {
        // The A-2 regression (docs/meeting-text-on-the-search-spine.md): a window holding both a
        // screen capture and an indexed meeting transcript classifies BOTH — but only the capture
        // crosses the wire. This drives the real submit path and reads back what the transport
        // was actually given, so a future rewiring of the window read cannot quietly widen the
        // egress.
        use crate::llm::anthropic::{AnthropicBatchClient, AnthropicConfig};
        use crate::llm::traceability::RecordingSink;
        use crate::llm::transport::{HttpResponse, MockTransport};
        use crate::llm::{Secret, SelectKkKey};

        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        let (capture_id, _) = db.capture(&super_ev(now - 1000, "the deck discussion", "h1")).unwrap();
        // A meeting on the spine, exactly as index_session writes it. "I will send the budget"
        // trips the same local commitment rules inline capture uses.
        db.capture(&shogun_memory::event_log::NewEvent {
            source: "meeting",
            kind: "transcript",
            ..super_ev(now - 900, "Me: I will send the budget tomorrow", "h2")
        })
        .unwrap();

        let transport = std::sync::Arc::new(MockTransport::new([
            HttpResponse { status: 200, body: r#"{"id":"b","processing_status":"ended"}"#.into() },
            HttpResponse {
                status: 200,
                body: format!(
                    r#"{{"custom_id":"{capture_id}","result":{{"type":"succeeded","message":{{"content":[{{"type":"text","text":"{{\"commitments\":[{{\"direction\":\"mine\",\"description\":\"send the deck\"}}],\"open_loops\":[]}}"}}]}}}}}}"#
                ),
            },
        ]));
        let client = AnthropicBatchClient::new(
            SharedTransport(transport.clone()),
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

        // The wire carried the capture and NOT the meeting — checked on the raw request bodies.
        let sent = transport.sent();
        assert!(!sent.is_empty(), "the batch submit must have gone out");
        for req in &sent {
            let body = req.body.as_deref().unwrap_or("");
            assert!(
                !body.contains("send the budget"),
                "meeting text reached the batch lane: {}",
                req.url
            );
        }
        assert!(
            sent.iter().any(|r| r.body.as_deref().unwrap_or("").contains("deck discussion")),
            "the capture half must still be classified in the cloud"
        );

        // …and the meeting commitment still landed in state, through the LOCAL rules.
        let commitments = db.commitments_due(now + 7 * 24 * 60 * 60 * 1000);
        let descriptions: Vec<&str> =
            commitments.iter().map(|c| c.description.as_str()).collect();
        assert!(
            descriptions.iter().any(|d| d.contains("send the budget")),
            "the meeting's commitment must still be extracted locally: {descriptions:?}"
        );
    }
}
