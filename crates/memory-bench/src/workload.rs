//! What we run: a deterministic corpus of events plus the queries that interrogate it.
//!
//! A workload is generated entirely from a seed and is independent of any backend, so the same
//! corpus can be replayed against the current memory layer and against a future intervention
//! (selective update, consolidation, retention policy) and the two runs stay comparable.
//! Interventions change [`crate::backend`]; they must never change this module, or the
//! comparison stops meaning anything.

use crate::rng::Rng;

/// One thing SHOGUN "saw", in the shape [`shogun_memory::event_log::NewEvent`] wants.
#[derive(Debug, Clone)]
pub struct BenchEvent {
    pub ts: i64,
    pub source: String,
    pub kind: String,
    pub app_bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub content: String,
    pub dwell_ms: i64,
    /// The underlying *fact* this event carries.
    ///
    /// Several events can share one `fact_id` — that is what a duplicate is. Distinguishing the
    /// fact from the row it is written to is what makes write amplification measurable at all:
    /// rows are what the database holds, facts are what the user actually told it.
    pub fact_id: String,
}

/// A question, and which facts answer it.
#[derive(Debug, Clone)]
pub struct BenchQuery {
    pub ask: String,
    /// Facts that are a correct answer *now*. Recall and MRR are computed against these.
    pub expected: Vec<String>,
    /// Facts that were a correct answer at some earlier point and have since been superseded.
    ///
    /// Retrieving one of these is not a miss in the recall sense — the row is genuinely in the
    /// log and genuinely matches the words — but it is a wrong answer to a present-tense
    /// question. Counting them separately gives a stale-retrieval rate without needing any
    /// semantic contradiction detection, which is deliberately out of scope for v0.1.
    pub superseded: Vec<String>,
}

/// A generated corpus plus its query set.
#[derive(Debug, Clone)]
pub struct GeneratedWorkload {
    pub name: &'static str,
    pub events: Vec<BenchEvent>,
    pub queries: Vec<BenchQuery>,
}

impl GeneratedWorkload {
    /// Distinct `fact_id`s across the corpus. The denominator of write amplification.
    pub fn unique_facts(&self) -> usize {
        let mut ids: Vec<&str> = self.events.iter().map(|e| e.fact_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    }

    /// Events that repeat a `fact_id` already carried by an earlier event — the number of writes
    /// a perfect deduplicator would collapse.
    pub fn duplicate_events(&self) -> usize {
        self.events.len() - self.unique_facts()
    }
}

/// Generates a corpus. One implementation per question we want to ask the memory layer.
pub trait Workload {
    /// Stable identifier, recorded in the report and used by `--workload`.
    fn name(&self) -> &'static str;

    /// Build the corpus. Must be a pure function of `(seed, events, queries)`: the determinism
    /// test in this crate replays it and compares.
    fn generate(&self, rng: &mut Rng, events: usize, queries: usize) -> GeneratedWorkload;
}
