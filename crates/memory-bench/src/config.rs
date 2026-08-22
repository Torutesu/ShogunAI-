//! What a run is.
//!
//! Everything that can change a result lives in this struct and is serialized into the report.
//! A number without the configuration that produced it is not a measurement, and the specific
//! failure this guards against is comparing a baseline captured at one scale against an
//! intervention captured at another and calling the difference an improvement.

use serde::Serialize;

/// Default corpus size. 100k is not a round number picked for effect: it is the scale
/// `docs/phase1-implementation-plan.md` M2 states the search SLO at
/// ("検索SLO（10万イベントp95≤500ms）") and the scale WP2.6 requires the bench to run at.
pub const DEFAULT_EVENTS: usize = 100_000;

/// Default query count. Large enough that p95 has ~25 samples above it and p99 has ~5, which is
/// the floor at which those percentiles say anything at all.
pub const DEFAULT_QUERIES: usize = 500;

/// Results returned per query. Matches the `k` the quality floors in
/// `shogun-memory/tests/retrieval_eval.rs` are stated at, so recall@5 here and recall@5 there are
/// the same measurement.
pub const DEFAULT_K: usize = 10;

/// Queries run before measurement starts, to warm the page cache and the prepared-statement path.
/// Their latencies are discarded.
pub const DEFAULT_WARMUP: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct BenchConfig {
    /// Which corpus to generate. See [`crate::workloads::ALL`].
    pub workload: String,
    /// The seed. Every random choice in the workload derives from this one number, so a run is
    /// reproducible from `(workload, seed, events, queries)` alone.
    pub seed: u64,
    pub events: usize,
    pub queries: usize,
    /// Results requested per query.
    pub k: usize,
    pub warmup: usize,
    /// Where the SQLite file goes. `None` runs in memory — faster, but its write latency is not
    /// the product's write latency, and the report marks it.
    pub db_path: Option<String>,
    /// Directory the JSON report is written to. `None` prints the summary and writes nothing.
    pub out_dir: Option<String>,
    /// How many events are written inside one transaction.
    ///
    /// The product does not batch — a capture is one write. But a 100k-event corpus committed one
    /// transaction at a time is dominated by fsync and takes long enough that nobody runs it, so
    /// the bulk load is batched and this is recorded, because it means `write.p95` here is a
    /// *lower bound* on the per-capture latency, not an estimate of it.
    pub write_batch: usize,
}

/// Default batch size for the bulk load. 1 measures the real per-write cost.
pub const DEFAULT_WRITE_BATCH: usize = 1_000;

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            workload: "clean".to_string(),
            seed: 42,
            events: DEFAULT_EVENTS,
            queries: DEFAULT_QUERIES,
            k: DEFAULT_K,
            warmup: DEFAULT_WARMUP,
            db_path: None,
            out_dir: None,
            write_batch: DEFAULT_WRITE_BATCH,
        }
    }
}

impl BenchConfig {
    /// A small, fast configuration for the CI smoke run: proves the pipeline works end to end
    /// without spending CI minutes on numbers nobody will quote.
    pub fn smoke() -> Self {
        Self {
            workload: "clean".to_string(),
            seed: 42,
            events: 2_000,
            queries: 50,
            k: DEFAULT_K,
            warmup: 5,
            db_path: None,
            out_dir: None,
            write_batch: DEFAULT_WRITE_BATCH,
        }
    }
}
