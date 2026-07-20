//! Concrete Dream Cycle job effects (WP3.4, §6.7, feature `db`). This is the production
//! [`DreamJobRunner`] the nightly loop drives — [`run::run_cycle`](super::run) calls `run(kind, …)`
//! for each job in sequence.
//!
//! The one model-dependent step (Consolidation: turning a day's events into state candidates) goes
//! through a [`Classifier`] seam. That seam is the invariant-5 boundary: the **Batch/Select-KK**
//! classifier is the only thing that may touch a model, and it is injected — never referenced here.
//! The default [`LocalRuleClassifier`] runs the same heuristics as inline capture
//! ([`shogun_memory::extract`]) with **no network**, so the whole runner is Linux-testable
//! end-to-end; the on-device build swaps in a Batch classifier without changing this file.
//!
//! Every other job (Compression, StateUpdate, ConfidenceRecalc, ColdDemotion, MorningBrief) is a
//! pure local DB effect. The Degraded sequence (StateUpdate + ConfidenceRecalc) therefore needs no
//! classifier at all — matching FR-DC-01 (a catch-up run does no Batch work).

use shogun_memory::extract::Candidate;

use crate::daemon::Db;

use super::plan::JobKind;
use super::run::DreamJobRunner;

/// Turns a day's captured event texts into state-table candidates. Implementors: the on-device
/// **Batch/Select-KK** classifier (the only model-touching one, invariant 5) and the local-rule
/// default below. Returns, per input event id, the candidates extracted from it.
pub trait Classifier {
    fn classify(&self, events: &[shogun_memory::event_log::EventText]) -> Vec<(i64, Vec<Candidate>)>;
}

/// The always-available, network-free classifier: the same heuristic rules inline capture uses
/// ([`shogun_memory::extract::extract`]). Produces low-confidence candidates only — the Batch
/// classifier is what raises confidence (WP2.7 second stage).
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalRuleClassifier;

impl Classifier for LocalRuleClassifier {
    fn classify(&self, events: &[shogun_memory::event_log::EventText]) -> Vec<(i64, Vec<Candidate>)> {
        events
            .iter()
            .map(|e| (e.id, shogun_memory::extract::extract(&e.content)))
            .filter(|(_, cands)| !cands.is_empty())
            .collect()
    }
}

/// Half-life for nightly confidence decay (FR-ST-21). 30 days: a state row not re-evidenced for a
/// month loses half its confidence, so stale inferences fade instead of lingering as fact.
pub const CONFIDENCE_HALF_LIFE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// The production runner: holds the shared DB handle and the injected classifier. `now_ms` is
/// captured once at construction so every job in a cycle recomputes against the same instant
/// (idempotent re-runs, FR-DC-04).
pub struct DbDreamRunner<'a, C: Classifier> {
    db: &'a Db,
    classifier: &'a C,
    now_ms: i64,
}

impl<'a, C: Classifier> DbDreamRunner<'a, C> {
    pub fn new(db: &'a Db, classifier: &'a C, now_ms: i64) -> Self {
        Self { db, classifier, now_ms }
    }

    /// Consolidation (Full only): classify the window's events and persist *new* candidates,
    /// deduping by description against existing state so a crash-resume re-run adds nothing twice.
    fn consolidate(&self, from_ts: i64, to_ts: i64) -> Result<(), String> {
        let events = self.db.events_in_range(from_ts, to_ts);
        let seen = self.db.existing_state_descriptions();
        let mut already = seen;
        for (event_id, cands) in self.classifier.classify(&events) {
            let fresh: Vec<Candidate> = cands
                .into_iter()
                .filter(|c| already.insert(description_of(c)))
                .collect();
            if !fresh.is_empty() {
                self.db.persist_candidates(event_id, &fresh);
            }
        }
        Ok(())
    }

    /// StateUpdate (Full + Degraded): recompute overdue + staleness from `now` (FR-ST-21).
    fn state_update(&self) -> Result<(), String> {
        self.db.recompute_overdue_and_staleness(self.now_ms);
        Ok(())
    }

    /// ConfidenceRecalc (Full + Degraded): age-decay confidence (FR-ST-21).
    fn confidence_recalc(&self) -> Result<(), String> {
        self.db.decay_confidence(self.now_ms, CONFIDENCE_HALF_LIFE_MS);
        Ok(())
    }

    /// ColdDemotion (Full only): demote Warm embeddings older than the 30-day window (FR-MEM-04).
    fn cold_demotion(&self) -> Result<(), String> {
        self.db.demote_cold(self.now_ms - shogun_memory::cold::WARM_WINDOW_MS);
        Ok(())
    }
}

// ------------------------------------------------------------------ Batch classifier (pure parts)
// The on-device Consolidation stage classifies via the Batch/Select-KK lane (invariant 5). The two
// pure, network-free halves live here and are Linux-tested; the only untestable glue is the async
// `AnthropicBatchClient::run` call between them (feature `net`, needs a real key → on-device):
//
//     let items   = build_batch_items(&events);
//     let results = batch_client.run(&items, ...).await?;   // on-device only
//     let cands   = parse_batch_classification(&results);
//
// so a Batch `Classifier` impl is a thin wrapper around these, not new logic.

/// Confidence a Batch-classified candidate carries. Above the local-rule cap (0.4) and the Medium
/// threshold (0.5) — a model classification is more trustworthy than a heuristic — but below the
/// High band (≥0.7) reserved for user-confirmed / repeatedly-evidenced state (FR-ST-20/21).
pub const BATCH_CONFIDENCE: f64 = 0.6;

/// The classification prompt wrapped around one event's captured text. Instructs the model to
/// return exactly the JSON contract [`parse_batch_classification`] reads — no prose. Sending
/// processed chunks (the prompt + this event's text) to the Batch lane is the only egress here
/// (invariant 3: traceability is recorded by `AnthropicBatchClient::submit`).
pub fn consolidation_prompt(content: &str) -> String {
    format!(
        "You extract commitments and open loops from a snippet of a user's captured screen text.\n\
         Return ONLY a JSON object (no prose, no code fence) of this exact shape:\n\
         {{\"commitments\":[{{\"direction\":\"mine|theirs\",\"description\":\"...\"}}],\
         \"open_loops\":[{{\"kind\":\"reply_needed|waiting_on_them|review_pending|decision_pending|follow_up|other\",\"description\":\"...\"}}]}}\n\
         A commitment is an explicit promise: direction \"mine\" if the user promised, \"theirs\" if \
         someone promised the user. An open loop is something awaiting action. If there is nothing \
         actionable, return empty arrays.\n\
         Text:\n{content}"
    )
}

/// Build one Batch item per event: `custom_id` is the event id (so results map back), `purpose`
/// tags the lane for traceability, `chunk` is the classification prompt over the event's text.
pub fn build_batch_items(events: &[shogun_memory::event_log::EventText]) -> Vec<crate::llm::anthropic::BatchItem> {
    events
        .iter()
        .map(|e| crate::llm::anthropic::BatchItem {
            custom_id: e.id.to_string(),
            purpose: "consolidation".to_string(),
            chunk: consolidation_prompt(&e.content),
        })
        .collect()
}

/// Classify a window of events through the **Batch/Select-KK** lane (invariant 5) end-to-end:
/// build the prompts, run the batch to completion (submit → poll → results), and parse the model's
/// JSON into per-event candidates at [`BATCH_CONFIDENCE`]. Async — the on-device scheduler awaits
/// this *before* the sync cycle and feeds the result to a [`PrecomputedClassifier`], so the sync
/// `DreamJobRunner` never has to bridge async. Generic over the transport, so it is Linux-testable
/// with a mock (no network). `sleep` is the injected inter-poll delay (FR-DC-05).
pub async fn classify_via_batch<T, S, F, Fut>(
    client: &crate::llm::anthropic::AnthropicBatchClient<T, S>,
    events: &[shogun_memory::event_log::EventText],
    max_polls: u32,
    sleep: F,
) -> Result<Vec<(i64, Vec<Candidate>)>, crate::llm::LlmError>
where
    T: crate::llm::transport::HttpTransport,
    S: crate::llm::traceability::TraceabilitySink,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    if events.is_empty() {
        return Ok(Vec::new());
    }
    let items = build_batch_items(events);
    let results = client.run(&items, max_polls, sleep).await?;
    Ok(parse_batch_classification(&results))
}

/// A [`Classifier`] that returns a *precomputed* classification (built by [`classify_via_batch`] in
/// an async context) keyed by event id. This is the bridge that keeps the sync cycle sync: the
/// async Batch call happens first, its result is wrapped here, and Consolidation reads it like any
/// classifier — no runtime `block_on` inside the sync job.
pub struct PrecomputedClassifier {
    by_event: std::collections::HashMap<i64, Vec<Candidate>>,
}

impl PrecomputedClassifier {
    pub fn new(classified: Vec<(i64, Vec<Candidate>)>) -> Self {
        Self { by_event: classified.into_iter().collect() }
    }
}

impl Classifier for PrecomputedClassifier {
    fn classify(&self, events: &[shogun_memory::event_log::EventText]) -> Vec<(i64, Vec<Candidate>)> {
        events
            .iter()
            .filter_map(|e| self.by_event.get(&e.id).map(|c| (e.id, c.clone())))
            .collect()
    }
}

/// Parse Batch results into per-event candidates. Each succeeded result's text is expected to be a
/// JSON object `{ "commitments": [{direction, description}], "open_loops": [{kind, description}] }`;
/// unknown directions/kinds and malformed lines are skipped (never panic on model output). Emitted
/// at [`BATCH_CONFIDENCE`].
pub fn parse_batch_classification(
    results: &[crate::llm::anthropic::BatchResult],
) -> Vec<(i64, Vec<Candidate>)> {
    let mut out = Vec::new();
    for r in results {
        let (Some(id), Some(text)) = (r.custom_id.parse::<i64>().ok(), r.text.as_deref()) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { continue };
        let mut cands = Vec::new();
        if let Some(arr) = v.get("commitments").and_then(|c| c.as_array()) {
            for c in arr {
                let desc = c.get("description").and_then(|d| d.as_str()).unwrap_or_default();
                if desc.is_empty() {
                    continue;
                }
                let direction = match c.get("direction").and_then(|d| d.as_str()) {
                    Some("theirs") => shogun_memory::state::CommitmentDirection::Theirs,
                    _ => shogun_memory::state::CommitmentDirection::Mine,
                };
                cands.push(Candidate::Commitment {
                    direction,
                    description: desc.to_string(),
                    confidence: BATCH_CONFIDENCE,
                });
            }
        }
        if let Some(arr) = v.get("open_loops").and_then(|l| l.as_array()) {
            for l in arr {
                let desc = l.get("description").and_then(|d| d.as_str()).unwrap_or_default();
                if desc.is_empty() {
                    continue;
                }
                let Some(kind) = open_loop_kind(l.get("kind").and_then(|k| k.as_str())) else {
                    continue;
                };
                cands.push(Candidate::OpenLoop { kind, description: desc.to_string(), confidence: BATCH_CONFIDENCE });
            }
        }
        if !cands.is_empty() {
            out.push((id, cands));
        }
    }
    out
}

/// Map a wire kind string to an [`OpenLoopKind`]; `None` for an unknown value (skipped).
fn open_loop_kind(s: Option<&str>) -> Option<shogun_memory::state::OpenLoopKind> {
    use shogun_memory::state::OpenLoopKind::*;
    Some(match s? {
        "reply_needed" => ReplyNeeded,
        "waiting_on_them" => WaitingOnThem,
        "review_pending" => ReviewPending,
        "decision_pending" => DecisionPending,
        "follow_up" => FollowUp,
        "other" => Other,
        _ => return None,
    })
}

/// The description text a candidate carries (dedup key).
fn description_of(c: &Candidate) -> String {
    match c {
        Candidate::Commitment { description, .. } | Candidate::OpenLoop { description, .. } => {
            description.clone()
        }
    }
}

impl<C: Classifier> DreamJobRunner for DbDreamRunner<'_, C> {
    fn run(&self, kind: JobKind, from_ts: i64, to_ts: i64) -> Result<(), String> {
        match kind {
            JobKind::Consolidation => self.consolidate(from_ts, to_ts),
            // Compression's real effect (LLM day-summary) rides the Batch lane on-device; the
            // Linux build has no local summariser, so it is a structural no-op that still records
            // as done (it must never block the sequence).
            JobKind::Compression => Ok(()),
            JobKind::StateUpdate => self.state_update(),
            JobKind::ConfidenceRecalc => self.confidence_recalc(),
            JobKind::ColdDemotion => self.cold_demotion(),
            // MorningBrief is generated on demand from live state (Db::local_morning_brief); the job
            // slot exists for sequencing/telemetry and has no separate persisted effect here.
            JobKind::MorningBrief => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::dreamcycle::plan::CycleKind;
    use crate::dreamcycle::run::run_cycle;
    use shogun_memory::event_log::EventText;
    use std::sync::Arc;

    fn db_at(now: i64) -> Db {
        Db::open_in_memory(Arc::new(move || now)).unwrap()
    }

    #[test]
    fn full_cycle_consolidates_and_maintains_state() {
        let now = 100 * 24 * 60 * 60 * 1000; // 100 days in
        let db = db_at(now);
        // a captured promise inside the window
        let (id, _t) = db.capture(&make_ev(now - 1000, "I'll send the deck. Waiting on legal.", "h1")).unwrap();
        assert!(id > 0);

        let clf = LocalRuleClassifier;
        let runner = DbDreamRunner::new(&db, &clf, now);
        let report = run_cycle(&db, &runner, "cycle-1", CycleKind::Full, now - 86_400_000, now);
        assert!(report.is_complete(), "full cycle should complete: {report:?}");

        // consolidation persisted the low-confidence candidates
        let commitments = db.commitments_due(now);
        assert_eq!(commitments.len(), 1);
        assert!(commitments[0].confidence <= shogun_memory::extract::LOCAL_RULE_MAX_CONFIDENCE);
    }

    #[test]
    fn consolidation_is_idempotent_across_reruns() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        db.capture(&make_ev(now - 1000, "I'll send the report.", "h1")).unwrap();

        let clf = LocalRuleClassifier;
        let runner = DbDreamRunner::new(&db, &clf, now);
        // run consolidation twice over the same window
        runner.run(JobKind::Consolidation, now - 86_400_000, now).unwrap();
        runner.run(JobKind::Consolidation, now - 86_400_000, now).unwrap();
        // dedup by description → still exactly one commitment
        assert_eq!(db.commitments_due(now).len(), 1, "re-run must not duplicate the candidate");
    }

    #[test]
    fn state_update_flags_overdue() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        let e = db.capture(&make_ev(1, "evidence", "h1")).unwrap().0;
        db.insert_commitment(
            &shogun_memory::state::NewCommitment {
                direction: shogun_memory::state::CommitmentDirection::Mine,
                counterparty_id: None,
                description: "overdue thing",
                due_at: Some(now - 5000),
                status: shogun_memory::state::CommitmentStatus::Open,
                project_id: None,
                confidence: 0.9,
                now: 1,
            },
            &[shogun_memory::state::Provenance::new(e)],
        )
        .unwrap();
        let clf = LocalRuleClassifier;
        let runner = DbDreamRunner::new(&db, &clf, now);
        runner.run(JobKind::StateUpdate, 0, now).unwrap();
        assert!(db.commitments_due(now)[0].overdue, "past-due open commitment must be overdue");
    }

    #[test]
    fn degraded_cycle_runs_without_touching_the_classifier() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        // a classifier that panics if called — proves Degraded never consolidates
        struct Boom;
        impl Classifier for Boom {
            fn classify(&self, _: &[EventText]) -> Vec<(i64, Vec<Candidate>)> {
                panic!("classifier must not run in a degraded cycle");
            }
        }
        let clf = Boom;
        let runner = DbDreamRunner::new(&db, &clf, now);
        let report = run_cycle(&db, &runner, "deg-1", CycleKind::Degraded, 0, now);
        assert!(report.is_complete());
    }

    #[test]
    fn build_batch_items_maps_id_and_wraps_content_in_the_prompt() {
        let events = vec![
            EventText { id: 7, content: "hello".into() },
            EventText { id: 9, content: "world".into() },
        ];
        let items = build_batch_items(&events);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].custom_id, "7");
        assert_eq!(items[0].purpose, "consolidation");
        // chunk is the classification prompt wrapping the event text
        assert!(items[1].chunk.contains("world"));
        assert!(items[1].chunk.contains("commitments"), "prompt asks for the JSON contract");
    }

    #[test]
    fn consolidation_prompt_names_the_contract_fields() {
        let p = consolidation_prompt("some text");
        for needle in ["commitments", "open_loops", "direction", "mine", "theirs", "some text"] {
            assert!(p.contains(needle), "prompt missing {needle}");
        }
    }

    #[tokio::test]
    async fn classify_via_batch_runs_the_lane_and_parses_candidates() {
        use crate::llm::anthropic::{AnthropicBatchClient, AnthropicConfig};
        use crate::llm::transport::{HttpResponse, MockTransport};
        use crate::llm::{SelectKkKey, Secret};

        // submit(ended) → results(JSONL with the classification JSON for event 42)
        let transport = MockTransport::new([
            HttpResponse { status: 200, body: r#"{"id":"b","processing_status":"ended"}"#.into() },
            HttpResponse {
                status: 200,
                body: r#"{"custom_id":"42","result":{"type":"succeeded","message":{"content":[{"type":"text","text":"{\"commitments\":[{\"direction\":\"mine\",\"description\":\"send the deck\"}],\"open_loops\":[]}"}]}}}"#.into(),
            },
        ]);
        let client = AnthropicBatchClient::new(
            transport,
            crate::llm::traceability::RecordingSink::new(),
            SelectKkKey::new(Secret::new("kk-123456")),
            AnthropicConfig::new("claude-x"),
        );
        let events = vec![EventText { id: 42, content: "I promised the deck".into() }];
        let classified = classify_via_batch(&client, &events, 3, || async {}).await.unwrap();
        assert_eq!(classified.len(), 1);
        let (id, cands) = &classified[0];
        assert_eq!(*id, 42);
        assert!(matches!(
            &cands[0],
            Candidate::Commitment { direction: shogun_memory::state::CommitmentDirection::Mine, .. }
        ));
        assert_eq!(cands[0].confidence(), BATCH_CONFIDENCE);
    }

    #[tokio::test]
    async fn classify_via_batch_empty_events_makes_no_call() {
        use crate::llm::anthropic::{AnthropicBatchClient, AnthropicConfig};
        use crate::llm::transport::MockTransport;
        use crate::llm::{SelectKkKey, Secret};
        // no responses queued — if it tried to call, it would panic/err; empty input must skip.
        let client = AnthropicBatchClient::new(
            MockTransport::new([]),
            crate::llm::traceability::RecordingSink::new(),
            SelectKkKey::new(Secret::new("kk-123456")),
            AnthropicConfig::new("claude-x"),
        );
        let out = classify_via_batch(&client, &[], 3, || async {}).await.unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn precomputed_classifier_returns_by_event_id() {
        let classified = vec![(
            7i64,
            vec![Candidate::Commitment {
                direction: shogun_memory::state::CommitmentDirection::Mine,
                description: "x".into(),
                confidence: BATCH_CONFIDENCE,
            }],
        )];
        let pc = PrecomputedClassifier::new(classified);
        // an event present in the precomputed map yields its candidates; an absent one yields nothing
        let present = pc.classify(&[EventText { id: 7, content: "ignored".into() }]);
        assert_eq!(present.len(), 1);
        let absent = pc.classify(&[EventText { id: 99, content: "ignored".into() }]);
        assert!(absent.is_empty());
    }

    #[test]
    fn parse_batch_classification_reads_json_at_medium_confidence() {
        use crate::llm::anthropic::BatchResult;
        let results = vec![BatchResult {
            custom_id: "42".into(),
            text: Some(
                r#"{"commitments":[{"direction":"theirs","description":"Bob will send the doc"}],
                    "open_loops":[{"kind":"waiting_on_them","description":"waiting on legal"}]}"#
                    .into(),
            ),
            error: None,
        }];
        let parsed = parse_batch_classification(&results);
        assert_eq!(parsed.len(), 1);
        let (id, cands) = &parsed[0];
        assert_eq!(*id, 42);
        assert_eq!(cands.len(), 2);
        for c in cands {
            assert_eq!(c.confidence(), BATCH_CONFIDENCE);
            assert!(c.confidence() > shogun_memory::extract::LOCAL_RULE_MAX_CONFIDENCE);
        }
        assert!(matches!(
            &cands[0],
            Candidate::Commitment { direction: shogun_memory::state::CommitmentDirection::Theirs, .. }
        ));
    }

    #[test]
    fn parse_batch_classification_skips_malformed_and_unknown() {
        use crate::llm::anthropic::BatchResult;
        let results = vec![
            BatchResult { custom_id: "1".into(), text: Some("not json".into()), error: None },
            BatchResult { custom_id: "notanid".into(), text: Some("{}".into()), error: None },
            BatchResult {
                custom_id: "2".into(),
                text: Some(r#"{"open_loops":[{"kind":"bogus","description":"x"}]}"#.into()),
                error: None,
            },
        ];
        // none yield candidates: bad json, bad id, unknown kind
        assert!(parse_batch_classification(&results).is_empty());
    }

    fn make_ev<'a>(ts: i64, content: &'a str, hash: &'a str) -> shogun_memory::event_log::NewEvent<'a> {
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
