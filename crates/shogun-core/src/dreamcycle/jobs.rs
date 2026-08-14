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
//! local DB effect; Compression and MorningBrief route their prose through the same injected
//! [`Summarizer`] seam (extractive by default, Batch on-device — `generated` records which one
//! wrote the persisted brief). The Degraded sequence (StateUpdate + ConfidenceRecalc) therefore
//! needs no classifier at all — matching FR-DC-01 (a catch-up run does no Batch work).

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

/// Turns a thread's day of event texts into a one-line summary written to `threads.summary`. Like
/// [`Classifier`], this is the invariant-5 boundary: only the on-device **Batch/Select-KK**
/// summariser may touch a model, and it is injected — never referenced here. `None` means "nothing
/// worth summarising" and the caller leaves the summary unwritten.
pub trait Summarizer {
    fn summarize(&self, events: &[shogun_memory::event_log::EventText]) -> Option<String>;

    /// Whether this summariser produces model-generated prose (the Batch/Select-KK lane). The
    /// default is `false`: the extractive fallback is honest degradation, and a Morning Brief
    /// persisted through it is marked `generated = 0` (FR-MB-04) — the same pattern as
    /// Consolidation running on local rules. The on-device Batch summariser overrides this.
    fn is_generative(&self) -> bool {
        false
    }
}

/// The always-available, network-free summariser (the Linux-test default): pull each event's lead
/// sentence, join them, and cap the whole thing. Extractive and deterministic — no model, no clock,
/// no RNG — so the runner stays Linux-testable end to end. The on-device build swaps in a Batch
/// abstractive summariser without changing this file (a separate PR).
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalExtractiveSummarizer;

/// Max characters an extractive summary may carry. A summary is always shorter than its input: this
/// caps the join, and each event contributes only its lead sentence, so the output cannot exceed
/// the source.
const EXTRACTIVE_SUMMARY_CHARS: usize = 280;

impl Summarizer for LocalExtractiveSummarizer {
    fn summarize(&self, events: &[shogun_memory::event_log::EventText]) -> Option<String> {
        if events.is_empty() {
            return None;
        }
        let mut out = String::new();
        for e in events {
            // The lead sentence, up to the first terminator (Latin or CJK).
            let lead = e
                .content
                .split(['.', '!', '?', '。', '！', '？'])
                .find(|s| !s.trim().is_empty())
                .unwrap_or("")
                .trim();
            if lead.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push_str(" / ");
            }
            out.push_str(lead);
            if out.chars().count() >= EXTRACTIVE_SUMMARY_CHARS {
                break;
            }
        }
        if out.is_empty() {
            return None;
        }
        // Cut on a char boundary; never split a codepoint.
        Some(out.chars().take(EXTRACTIVE_SUMMARY_CHARS).collect())
    }
}

/// Half-life for nightly confidence decay (FR-ST-21). 30 days: a state row not re-evidenced for a
/// month loses half its confidence, so stale inferences fade instead of lingering as fact.
pub const CONFIDENCE_HALF_LIFE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// The production runner: holds the shared DB handle and the injected classifier. `now_ms` is
/// captured once at construction so every job in a cycle recomputes against the same instant
/// (idempotent re-runs, FR-DC-04).
pub struct DbDreamRunner<'a, C: Classifier, S: Summarizer> {
    db: &'a Db,
    classifier: &'a C,
    summarizer: &'a S,
    now_ms: i64,
}

impl<'a, C: Classifier, S: Summarizer> DbDreamRunner<'a, C, S> {
    pub fn new(db: &'a Db, classifier: &'a C, summarizer: &'a S, now_ms: i64) -> Self {
        Self { db, classifier, summarizer, now_ms }
    }

    /// Compression (Full only): summarise each thread active in the window and fill
    /// `threads.summary` (Issue #63). The summariser is the injected seam (local-extractive or the
    /// on-device Batch one). A thread the summariser has nothing to say about (`None`) is skipped —
    /// this must never fail the sequence, so every step is fallible-but-recoverable.
    fn run_compression<Sum: Summarizer>(&self, summarizer: &Sum, from_ts: i64, to_ts: i64) -> Result<(), String> {
        for t in self.db.active_threads_between(from_ts, to_ts) {
            let events = self.db.thread_event_texts(&t.thread_key);
            if let Some(summary) = summarizer.summarize(&events) {
                self.db.set_thread_summary(&t.thread_key, &summary);
            }
        }
        // Sessions carry the same day-summary treatment, symmetric to threads (Issue #63): a session
        // active in the window is summarised through the same injected seam and its `summary` filled.
        // A session the summariser has nothing to say about (`None`) is skipped, never failed.
        for sid in self.db.active_sessions_between(from_ts, to_ts) {
            let events = self.db.session_event_texts(sid);
            if let Some(summary) = summarizer.summarize(&events) {
                self.db.set_session_summary(sid, &summary);
            }
        }
        Ok(())
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

    /// LessonDistillation (Full only, Plan D-4): turn unprocessed approval feedback into
    /// lessons, then run the lesson lifecycle. Local rules only in v1 (designs §5.3 honest
    /// degradation) — no Batch call, so the job runs identically on Linux and on-device.
    ///
    /// The processed watermark is `lesson_distill_meta.last_processed_feedback_id` (V17): the
    /// job reads feedback strictly above it and advances it only after every upsert landed.
    /// Idempotent across a crash-resume (FR-DC-04): a re-run before the watermark moved re-reads
    /// the same window, and `upsert_lesson` dedupes already-linked evidence, so nothing double
    /// counts; after the watermark moved, old feedback is never re-consumed, so decay is not
    /// refreshed by stale evidence. Errors carry no feedback text (CLAUDE.md privacy rule).
    fn lesson_distillation(&self) -> Result<(), String> {
        let watermark = self.db.lesson_distill_watermark();
        let feedback = self.db.feedback_after(watermark);
        let candidates = shogun_memory::lessons::distill(&feedback);
        if !candidates.is_empty() {
            for candidate in &candidates {
                if self.db.upsert_lesson(candidate, self.now_ms).is_none() {
                    return Err("lesson upsert failed".to_string());
                }
            }
            // Advance only after every candidate landed, so a crash re-runs the whole window.
            // A night that distilled nothing leaves the watermark put: below-threshold signals
            // (two same-direction edits) keep accumulating until a later night completes the
            // pattern, instead of being silently consumed. No upsert happens on those nights,
            // so re-reading the window cannot refresh any lesson's decay clock.
            let max_id = feedback.iter().map(|f| f.id).max().unwrap_or(watermark);
            if !self.db.set_lesson_distill_watermark(max_id) {
                return Err("lesson watermark write failed".to_string());
            }
        }
        // Lifecycle last, new evidence included: decay, contradiction, floor, cap (§5.3).
        self.db
            .decay_lessons(self.now_ms)
            .map(|_outcome| ())
            .ok_or_else(|| "lesson lifecycle failed".to_string())
    }

    /// MorningBrief (Full only, Plan C-1): assemble the day's brief from state tables plus the
    /// window's meeting recaps, and persist it to `briefs` keyed by the local date — so the
    /// morning display is a read (immediate, offline-stable) instead of a live degraded assembly.
    ///
    /// The summariser seam decides honesty, same as Compression: a Batch-backed summariser yields
    /// generated prose and `generated = 1`; the local extractive default persists `generated = 0`
    /// (FR-MB-04 honest degradation). The upsert keys on the day, so a crash-resume re-run over
    /// the same plan day is idempotent (FR-DC-04), and the FR-MB-06 `updated` mark falls out of
    /// the stored payload-digest comparison.
    fn morning_brief(&self, from_ts: i64, to_ts: i64) -> Result<(), String> {
        use shogun_fusion::brief::{assemble_brief, CalendarLine, WHAT_HAPPENED_MAX};

        const DAY_MS: i64 = 24 * 60 * 60 * 1000;
        let date = crate::daemon::local_date_string(self.now_ms);
        let day = crate::daemon::local_day_bounds(self.now_ms);

        // Calendar-equivalent "Today" lines (design §4.1): commitments due this local day stand in
        // for calendar rows until the real Calendar connector lands (B-4) and replaces them.
        let due = self.db.commitments_due(self.now_ms);
        let calendar: Vec<CalendarLine> = due
            .iter()
            .filter_map(|c| {
                c.due_at_ms
                    .filter(|&t| t >= day.today_start_ms && t < day.today_start_ms + DAY_MS)
                    .map(|t| CalendarLine { start_ms: t, title: c.description.clone(), updated: false })
            })
            .collect();

        // "What happened": the window's meeting recaps first (minutes already written by the Recap
        // lane), then the day's sessions/threads through the summariser seam. Compression ran
        // earlier in the sequence, so a thread's stored summary is reused before re-summarising.
        // The assembler caps the section again (FR-MB-01: ≤5 lines).
        let mut what_happened: Vec<String> = Vec::new();
        for sid in self.db.active_sessions_between(from_ts, to_ts) {
            if what_happened.len() >= WHAT_HAPPENED_MAX {
                break;
            }
            if let Some(recap) = self.db.meeting_recap_full(sid) {
                if !recap.summary.is_empty() {
                    what_happened.push(recap.summary);
                    continue;
                }
            }
            if let Some(s) = self.db.session_summary(sid).or_else(|| self.summarizer.summarize(&self.db.session_event_texts(sid))) {
                what_happened.push(s);
            }
        }
        for t in self.db.active_threads_between(from_ts, to_ts) {
            if what_happened.len() >= WHAT_HAPPENED_MAX {
                break;
            }
            if let Some(s) = self
                .db
                .thread_summary(&t.thread_key)
                .or_else(|| self.summarizer.summarize(&self.db.thread_event_texts(&t.thread_key)))
            {
                what_happened.push(s);
            }
        }
        if what_happened.is_empty() {
            if let Some(s) = self.summarizer.summarize(&self.db.events_in_range(from_ts, to_ts)) {
                what_happened.push(s);
            }
        }

        // Suggested actions are Fusion's runtime concern (the panel ranks them against the live
        // screen); the nightly brief persists none rather than freezing stale ones overnight.
        let brief = assemble_brief(calendar, &due, &self.db.open_loops(), what_happened, Vec::new());
        let payload = payload_from_brief(&date, &brief);
        let json = serde_json::to_string(&payload).map_err(|e| format!("brief serialize: {e}"))?;
        self.db
            .save_brief(&date, &json, self.summarizer.is_generative())
            .map(|_updated| ())
            .ok_or_else(|| "brief write failed".to_string())
    }
}

/// Convert the assembled fusion brief into the persisted payload shape. The stored type lives in
/// `shogun_memory::briefs` (storage must not depend on shogun-fusion), so the conversion happens
/// here, at the layer that can see both.
fn payload_from_brief(
    date: &str,
    brief: &shogun_fusion::brief::MorningBrief,
) -> shogun_memory::briefs::BriefPayload {
    use shogun_memory::briefs::{BriefActionLine, BriefLine, BriefPayload, BriefScheduleLine};
    let line = |i: &shogun_fusion::brief::BriefItem| BriefLine {
        text: i.text.clone(),
        provenance_event_id: i.provenance_event_id,
        possibly: i.possibly,
    };
    BriefPayload {
        date: date.to_string(),
        today: brief
            .today
            .iter()
            .map(|c| BriefScheduleLine { start_ms: c.start_ms, title: c.title.clone(), updated: c.updated })
            .collect(),
        commitments_due: brief.commitments_due.iter().map(line).collect(),
        open_loops: brief.open_loops.iter().map(line).collect(),
        what_happened: brief.what_happened.clone(),
        suggested_actions: brief
            .suggested_actions
            .iter()
            .map(|a| BriefActionLine { label: a.rationale.clone(), level: format!("{:?}", a.level) })
            .collect(),
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
///
/// Takes [`BatchEventText`](shogun_memory::event_log::BatchEventText), not `EventText`, on
/// purpose: that type is only produced by `events_in_range_partitioned`'s source-filtered `cloud`
/// half, so an unfiltered window (which would carry meeting transcripts — A-2,
/// `docs/meeting-text-on-the-search-spine.md`) cannot be compiled into a relay submission.
pub fn build_batch_items(events: &[shogun_memory::event_log::BatchEventText]) -> Vec<crate::llm::anthropic::BatchItem> {
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
/// `DreamJobRunner` never has to bridge async. Generic over the
/// [`BatchLane`](crate::llm::anthropic::BatchLane) — the direct Anthropic client (dev) and the
/// relay client (shipping) both fit — so it is Linux-testable with a mock transport (no network).
/// `sleep` is the injected inter-poll delay (FR-DC-05).
pub async fn classify_via_batch<B, F, Fut>(
    client: &B,
    events: &[shogun_memory::event_log::BatchEventText],
    max_polls: u32,
    sleep: F,
) -> Result<Vec<(i64, Vec<Candidate>)>, crate::llm::LlmError>
where
    B: crate::llm::anthropic::BatchLane,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    if events.is_empty() {
        return Ok(Vec::new());
    }
    let items = build_batch_items(events);
    let results =
        crate::llm::anthropic::run_batch_to_completion(client, &items, max_polls, sleep).await?;
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

impl<C: Classifier, S: Summarizer> DreamJobRunner for DbDreamRunner<'_, C, S> {
    fn run(&self, kind: JobKind, from_ts: i64, to_ts: i64) -> Result<(), String> {
        match kind {
            JobKind::Consolidation => self.consolidate(from_ts, to_ts),
            // Compression summarises the window's active threads through the injected summariser
            // (local-extractive by default, Batch on-device). It must never block the sequence:
            // threads with nothing to summarise are skipped, not failed.
            JobKind::Compression => self.run_compression(self.summarizer, from_ts, to_ts),
            JobKind::StateUpdate => self.state_update(),
            JobKind::ConfidenceRecalc => self.confidence_recalc(),
            JobKind::ColdDemotion => self.cold_demotion(),
            // MorningBrief persists the day's brief to `briefs` (Plan C-1) so the morning display
            // is a read; `Db::local_morning_brief` remains the fallback when no row exists.
            JobKind::MorningBrief => self.morning_brief(from_ts, to_ts),
            // LessonDistillation consumes the feedback watermark, not the cycle window: feedback
            // recorded between cycles must never fall through a window seam (Plan D-4).
            JobKind::LessonDistillation => self.lesson_distillation(),
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
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        let report = run_cycle(&db, &runner, "cycle-1", CycleKind::Full, now - 86_400_000, now);
        assert!(report.is_complete(), "full cycle should complete: {report:?}");

        // consolidation persisted the low-confidence candidates
        let commitments = db.commitments_due(now);
        assert_eq!(commitments.len(), 1);
        assert!(commitments[0].confidence <= shogun_memory::extract::LOCAL_RULE_MAX_CONFIDENCE);

        // the final job persisted the day's brief (Plan C-1)
        let date = crate::daemon::local_date_string(now);
        assert!(db.brief_for(&date).is_some(), "a full cycle must leave a briefs row");
    }

    #[test]
    fn consolidation_is_idempotent_across_reruns() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        db.capture(&make_ev(now - 1000, "I'll send the report.", "h1")).unwrap();

        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
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
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
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
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        let report = run_cycle(&db, &runner, "deg-1", CycleKind::Degraded, 0, now);
        assert!(report.is_complete());
    }

    #[test]
    fn build_batch_items_maps_id_and_wraps_content_in_the_prompt() {
        use shogun_memory::event_log::BatchEventText;
        let events = vec![
            BatchEventText { id: 7, content: "hello".into() },
            BatchEventText { id: 9, content: "world".into() },
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
        let events = vec![shogun_memory::event_log::BatchEventText {
            id: 42,
            content: "I promised the deck".into(),
        }];
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

    #[test]
    fn local_extractive_summarizer_produces_nonempty_summary_shorter_than_input() {
        let events = vec![
            EventText { id: 1, content: "決めた: 金曜に出す。残りは月曜。".into() },
            EventText { id: 2, content: "Next up is the invoice review. Then the deck.".into() },
        ];
        let input_len: usize = events.iter().map(|e| e.content.chars().count()).sum();
        let s = LocalExtractiveSummarizer.summarize(&events).unwrap();
        assert!(!s.is_empty());
        // Extractive: the output never exceeds the source (lead sentences only, then capped).
        assert!(s.chars().count() <= input_len, "summary must be no longer than its input");
    }

    #[test]
    fn local_extractive_summarizer_empty_input_is_none() {
        assert!(LocalExtractiveSummarizer.summarize(&[]).is_none());
    }

    #[test]
    fn local_extractive_summarizer_is_deterministic() {
        let events = vec![EventText { id: 1, content: "same in. same out.".into() }];
        assert_eq!(
            LocalExtractiveSummarizer.summarize(&events),
            LocalExtractiveSummarizer.summarize(&events)
        );
    }

    #[test]
    fn compression_fills_thread_summary() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        // A titled capture derives a thread_key, so the thread exists and is "active" at `now`.
        db.capture(&make_ev_titled(now - 1000, "I'll send the report. Waiting on legal.", "h1", "Q3 pricing"))
            .unwrap();
        let thread_key = db.active_threads_between(0, now).first().map(|t| t.thread_key.clone());
        let thread_key = thread_key.expect("a titled capture must create a thread");
        assert_eq!(db.thread_summary(&thread_key), None, "summary is unset before Compression");

        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        runner.run(JobKind::Compression, 0, now + 1).unwrap();

        let summary = db.thread_summary(&thread_key).expect("Compression must fill the summary");
        assert!(!summary.is_empty());
        assert!(summary.contains("send the report"), "summary carries the thread's lead sentence");
    }

    #[test]
    fn compression_fills_session_summary() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        // A meeting opened at `now` is "active" in a window covering it.
        let sid = db.open_meeting(Some("Weekly sync"), Some("us.zoom.xos"), 0.6, "{}").unwrap();
        // An event captured during it, attached to the session.
        let (ev_id, _) = db.capture(&make_ev(now - 1000, "We decided to ship Friday. Legal to review.", "s1")).unwrap();
        assert!(db.attach_event_to_meeting(sid, ev_id), "the event must attach");
        assert_eq!(db.session_summary(sid), None, "summary is unset before Compression");

        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        runner.run(JobKind::Compression, 0, now + 1).unwrap();

        let summary = db.session_summary(sid).expect("Compression must fill the session summary");
        assert!(!summary.is_empty());
        assert!(summary.contains("ship Friday"), "summary carries the session's lead sentence");
    }

    // ------------------------------------------------------------------ MorningBrief (Plan C-1)

    /// A high-confidence overdue commitment — solid brief material (Low is excluded, FR-MB-05).
    fn overdue_commitment(db: &Db, desc: &'static str, now: i64) {
        let e = db.capture(&make_ev(1, desc, desc)).unwrap().0;
        db.insert_commitment(
            &shogun_memory::state::NewCommitment {
                direction: shogun_memory::state::CommitmentDirection::Mine,
                counterparty_id: None,
                description: desc,
                due_at: Some(now - 5000),
                status: shogun_memory::state::CommitmentStatus::Open,
                project_id: None,
                confidence: 0.9,
                now: 1,
            },
            &[shogun_memory::state::Provenance::new(e)],
        )
        .unwrap();
    }

    #[test]
    fn morning_brief_persists_a_row_marked_not_generated_with_the_local_summarizer() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        overdue_commitment(&db, "send the overdue deck", now);

        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        runner.run(JobKind::MorningBrief, now - 86_400_000, now).unwrap();

        let date = crate::daemon::local_date_string(now);
        let row = db.brief_for(&date).expect("the job must write a briefs row");
        assert!(!row.generated, "the extractive summariser is honest degradation: generated=0");
        assert!(!row.updated, "the day's first brief is not an update");

        let payload: shogun_memory::briefs::BriefPayload =
            serde_json::from_str(&row.payload).expect("payload is valid BriefPayload JSON");
        assert_eq!(payload.date, date);
        assert!(
            payload.commitments_due.iter().any(|l| l.text == "send the overdue deck"),
            "the overdue commitment reaches the persisted brief"
        );
        assert!(payload.suggested_actions.is_empty(), "no stale actions are frozen overnight");
    }

    #[test]
    fn morning_brief_rerun_over_the_same_day_is_idempotent() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        overdue_commitment(&db, "send the report", now);

        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        // crash-resume: the job re-runs over the same plan day (FR-DC-04)
        runner.run(JobKind::MorningBrief, now - 86_400_000, now).unwrap();
        runner.run(JobKind::MorningBrief, now - 86_400_000, now).unwrap();

        let row = db.brief_for(&crate::daemon::local_date_string(now)).unwrap();
        assert!(!row.updated, "an unchanged re-run must not manufacture an Updated mark");
    }

    #[test]
    fn morning_brief_regeneration_after_state_changed_is_marked_updated() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        overdue_commitment(&db, "first thing", now);

        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        runner.run(JobKind::MorningBrief, now - 86_400_000, now).unwrap();

        // new state lands, the brief is regenerated for the same day → content differs
        overdue_commitment(&db, "second thing", now);
        runner.run(JobKind::MorningBrief, now - 86_400_000, now).unwrap();

        let row = db.brief_for(&crate::daemon::local_date_string(now)).unwrap();
        assert!(row.updated, "changed content is the FR-MB-06 Updated case");
    }

    #[test]
    fn morning_brief_marks_generated_when_the_summarizer_is_batch_backed() {
        // A stand-in for the on-device Batch summariser: generative prose, is_generative()=true.
        struct Generative;
        impl Summarizer for Generative {
            fn summarize(&self, _: &[EventText]) -> Option<String> {
                Some("A generated recap of the day.".into())
            }
            fn is_generative(&self) -> bool {
                true
            }
        }

        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        db.capture(&make_ev(now - 1000, "the day's material", "h1")).unwrap();

        let clf = LocalRuleClassifier;
        let sum = Generative;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        runner.run(JobKind::MorningBrief, now - 86_400_000, now).unwrap();

        let row = db.brief_for(&crate::daemon::local_date_string(now)).unwrap();
        assert!(row.generated, "a Batch-backed summariser marks the brief generated");
        let payload: shogun_memory::briefs::BriefPayload = serde_json::from_str(&row.payload).unwrap();
        assert_eq!(payload.what_happened, vec!["A generated recap of the day.".to_string()]);
    }

    #[test]
    fn morning_brief_carries_the_windows_meeting_recap() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        let sid = db.open_meeting(Some("Weekly sync"), Some("us.zoom.xos"), 0.6, "{}").unwrap();
        let (ev_id, _) = db.capture(&make_ev(now - 1000, "meeting talk", "m1")).unwrap();
        assert!(db.attach_event_to_meeting(sid, ev_id));
        db.save_meeting_recap(sid, "Decided to ship Friday.", "[]", "[]", "batch-model");

        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        runner.run(JobKind::MorningBrief, 0, now + 1).unwrap();

        let row = db.brief_for(&crate::daemon::local_date_string(now)).unwrap();
        let payload: shogun_memory::briefs::BriefPayload = serde_json::from_str(&row.payload).unwrap();
        assert!(
            payload.what_happened.iter().any(|l| l.contains("ship Friday")),
            "the recap summary feeds What happened: {:?}",
            payload.what_happened
        );
    }

    // ------------------------------------------------------------------ LessonDistillation (Plan D-4)

    /// A body long enough that stripping one closing line is nowhere near a 30% cut.
    const DRAFT_BODY: &str = "Hi team,\nHere is the current status of the migration work.\nEverything is on track for the Friday checkpoint and the remaining items are listed in the tracker.";

    /// Record `n` approval-time edits that all strip the same signature line (the (a) rule).
    fn record_signature_edits(db: &Db, n: usize, base_ts: i64) -> Vec<i64> {
        use shogun_memory::lessons::{FeedbackKind, LessonScope, NewFeedback};
        (0..n)
            .map(|i| {
                let before = format!("{DRAFT_BODY}\nExtra note number {i}.\nBest, Taro");
                let after = format!("{DRAFT_BODY}\nExtra note number {i}.");
                db.record_feedback(
                    FeedbackKind::EditBeforeApprove,
                    LessonScope::App,
                    &NewFeedback {
                        ts_ms: base_ts + i as i64,
                        action_kind: Some("draft_reply"),
                        scope_ref: Some("com.apple.Mail"),
                        before_text: Some(&before),
                        after_text: Some(&after),
                        ..Default::default()
                    },
                )
                .unwrap()
            })
            .collect()
    }

    fn lesson_state(db: &Db) -> Vec<(i64, f64, i64, bool)> {
        db.lessons_all().into_iter().map(|l| (l.id, l.confidence, l.evidence_count, l.active)).collect()
    }

    #[test]
    fn lesson_distillation_distills_a_pattern_and_advances_the_watermark() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        let ids = record_signature_edits(&db, 3, now - 5000);

        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        runner.run(JobKind::LessonDistillation, now - 86_400_000, now).unwrap();

        let lessons = db.lessons_all();
        assert_eq!(lessons.len(), 1, "three same-direction edits distill one lesson");
        assert!(lessons[0].active);
        assert_eq!(lessons[0].evidence_count, 3);
        assert!(lessons[0].instruction.contains("Best, Taro"));
        assert_eq!(db.lesson_distill_watermark(), *ids.iter().max().unwrap());
    }

    #[test]
    fn below_threshold_feedback_is_not_consumed_until_the_pattern_completes() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        record_signature_edits(&db, 2, now - 5000);

        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        runner.run(JobKind::LessonDistillation, 0, now).unwrap();
        assert!(db.lessons_all().is_empty(), "two edits are not a pattern");
        assert_eq!(db.lesson_distill_watermark(), 0, "unfired signals stay unconsumed");

        // A later night's third edit completes the pattern across the accumulated window.
        let third = record_signature_edits(&db, 1, now - 100);
        runner.run(JobKind::LessonDistillation, 0, now).unwrap();
        let lessons = db.lessons_all();
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].evidence_count, 3, "the two earlier edits count as evidence");
        assert_eq!(db.lesson_distill_watermark(), third[0]);
    }

    #[test]
    fn lesson_distillation_rerun_does_not_double_evidence() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        record_signature_edits(&db, 3, now - 5000);

        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);

        // Crash-resume case 1: upserts landed but the watermark write was lost — replay the
        // job's first half by hand (watermark still 0), then run the real job over the same
        // unprocessed window.
        for candidate in shogun_memory::lessons::distill(&db.feedback_after(0)) {
            db.upsert_lesson(&candidate, now).unwrap();
        }
        let after_first = lesson_state(&db);
        runner.run(JobKind::LessonDistillation, 0, now).unwrap();
        assert_eq!(lesson_state(&db), after_first, "re-reading the window must not double evidence");

        // Crash-resume case 2: the whole job ran but the ledger `Done` was lost — a full re-run
        // sees no unprocessed feedback and changes nothing.
        runner.run(JobKind::LessonDistillation, 0, now).unwrap();
        assert_eq!(lesson_state(&db), after_first, "an idle re-run must change nothing");
    }

    #[test]
    fn lesson_distillation_runs_the_lifecycle_so_stale_lessons_sleep() {
        use shogun_memory::lessons::{DEACTIVATION_FLOOR, LESSON_HALF_LIFE_MS};
        let born = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(born);
        record_signature_edits(&db, 3, born - 5000);
        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        DbDreamRunner::new(&db, &clf, &sum, born).run(JobKind::LessonDistillation, 0, born).unwrap();
        assert!(db.lessons_all()[0].active);

        // Five silent half-lives later, the nightly job's lifecycle pass puts it to sleep.
        let later = born + 5 * LESSON_HALF_LIFE_MS;
        DbDreamRunner::new(&db, &clf, &sum, later).run(JobKind::LessonDistillation, 0, later).unwrap();
        let l = &db.lessons_all()[0];
        assert!(l.confidence < DEACTIVATION_FLOOR, "confidence decayed: {}", l.confidence);
        assert!(!l.active, "a long-unevidenced lesson sleeps");
    }

    #[test]
    fn full_cycle_with_no_feedback_completes_and_touches_no_lessons() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        db.capture(&make_ev(now - 1000, "I'll send the deck.", "h1")).unwrap();
        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        let report = run_cycle(&db, &runner, "cycle-l5", CycleKind::Full, now - 86_400_000, now);
        assert!(report.is_complete(), "{report:?}");
        assert!(report.completed.contains(&JobKind::LessonDistillation));
        assert!(db.lessons_all().is_empty());
        assert_eq!(db.lesson_distill_watermark(), 0);
    }

    #[test]
    fn compression_does_not_panic_on_an_empty_window() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let db = db_at(now);
        let clf = LocalRuleClassifier;
        let sum = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &clf, &sum, now);
        // No threads in the window: must complete cleanly, never block the sequence.
        runner.run(JobKind::Compression, 0, now).unwrap();
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

    fn make_ev_titled<'a>(
        ts: i64,
        content: &'a str,
        hash: &'a str,
        title: &'a str,
    ) -> shogun_memory::event_log::NewEvent<'a> {
        shogun_memory::event_log::NewEvent {
            ts,
            source: "capture",
            kind: "text",
            app_bundle_id: Some("com.apple.Safari"),
            window_title: Some(title),
            content,
            content_hash: hash,
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        }
    }
}
