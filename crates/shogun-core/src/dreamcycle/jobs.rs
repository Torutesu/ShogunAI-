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

/// Build one Batch item per event: `custom_id` is the event id (so results map back), `purpose`
/// tags the lane for traceability, `chunk` is the text to classify.
pub fn build_batch_items(events: &[shogun_memory::event_log::EventText]) -> Vec<crate::llm::anthropic::BatchItem> {
    events
        .iter()
        .map(|e| crate::llm::anthropic::BatchItem {
            custom_id: e.id.to_string(),
            purpose: "consolidation".to_string(),
            chunk: e.content.clone(),
        })
        .collect()
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
    fn build_batch_items_maps_id_and_content() {
        let events = vec![
            EventText { id: 7, content: "hello".into() },
            EventText { id: 9, content: "world".into() },
        ];
        let items = build_batch_items(&events);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].custom_id, "7");
        assert_eq!(items[0].purpose, "consolidation");
        assert_eq!(items[1].chunk, "world");
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
