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

/// Hard caps keep accidental or agent-generated inputs from exhausting the machine before the
/// benchmark can produce a useful result. The event cap is ten times the required 100k SLO scale;
/// each generated event owns several strings and is also indexed by runner-side maps, so allowing
/// ten million events would permit multi-gigabyte allocations before SQLite opened.
pub const MAX_EVENTS: usize = 1_000_000;
pub const MAX_QUERIES: usize = 1_000_000;
pub const MAX_K: usize = 1_000;
pub const MAX_WARMUP: usize = 100_000;

/// Smallest sample set for which nearest-rank p95 has at least one observation above the cutoff.
pub const MIN_PERCENTILE_SAMPLES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("unknown workload {0:?} (valid: {})", crate::workloads::ALL.join(", "))]
    UnknownWorkload(String),
    #[error("{field} must be between {min} and {max}, got {actual}")]
    OutOfRange {
        field: &'static str,
        min: usize,
        max: usize,
        actual: usize,
    },
    #[error(
        "temporal workload needs at least {minimum} events for {queries} requested queries, got \
         {events}; every measured query must contain a current fact and superseded history"
    )]
    TemporalHistory {
        events: usize,
        queries: usize,
        minimum: usize,
    },
    #[error("{field} must not be empty")]
    EmptyPath { field: &'static str },
}

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
    /// Where the benchmark creates its private SQLite scratch directory. `None` runs in memory —
    /// faster, but its write latency is not the product's write latency, and the report marks it.
    /// The path must not exist: `runner::run` claims the whole directory atomically, then places
    /// `memory-bench.sqlite` and its WAL/SHM inside it. The caller's path is never opened as a DB.
    ///
    /// Serialized as the file name only. Absolute paths carry usernames and machine layout, and a
    /// committed baseline is public (issue #221); the path's identity adds nothing to
    /// reproducibility, which is defined by the runtime knobs + mode + commit + profile.
    #[serde(serialize_with = "file_name_only")]
    pub db_path: Option<String>,
    /// Directory the JSON report is written to. `None` prints the summary and writes nothing.
    /// Serialized as the final path component only, for the same reason as `db_path`.
    #[serde(serialize_with = "file_name_only")]
    pub out_dir: Option<String>,
    /// How many events are written inside one transaction.
    ///
    /// The product does not batch — a capture is one write. But a 100k-event corpus committed one
    /// transaction at a time is dominated by fsync and takes long enough that nobody runs it, so
    /// the bulk load is batched and this is recorded, because it means `write.p95` here is a
    /// *lower bound* on the per-capture latency, not an estimate of it.
    pub write_batch: usize,
}

/// Serialize a path as its final component. The report needs to say *that* a file-backed run
/// happened (and `mode.in_memory` already does); where the file lived on one contributor's disk
/// is machine metadata that has no place in a committed artifact.
fn file_name_only<S: serde::Serializer>(v: &Option<String>, ser: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(p) => {
            let name = std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.clone());
            ser.serialize_some(&name)
        }
        None => ser.serialize_none(),
    }
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
    /// Validate every entry point, including direct library callers.
    ///
    /// CLI parsing is only syntax. Resource bounds and workload invariants belong here because
    /// `memory_bench::run` is public and must be no easier to crash than the binary (issue #221).
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !crate::workloads::ALL.contains(&self.workload.as_str()) {
            return Err(ConfigError::UnknownWorkload(self.workload.clone()));
        }

        let bounded = |field: &'static str,
                       actual: usize,
                       min: usize,
                       max: usize|
         -> Result<(), ConfigError> {
            if actual < min || actual > max {
                return Err(ConfigError::OutOfRange {
                    field,
                    min,
                    max,
                    actual,
                });
            }
            Ok(())
        };
        bounded("--events", self.events, 1, MAX_EVENTS)?;
        bounded("--queries", self.queries, 1, MAX_QUERIES)?;
        bounded("--k", self.k, 1, MAX_K)?;
        bounded("--warmup", self.warmup, 0, MAX_WARMUP)?;
        bounded("--write-batch", self.write_batch, 1, MAX_EVENTS)?;

        for (field, path) in [("--db", &self.db_path), ("--out", &self.out_dir)] {
            if path.as_deref().is_some_and(str::is_empty) {
                return Err(ConfigError::EmptyPath { field });
            }
        }

        if self.workload == "temporal" {
            let minimum = crate::workloads::minimum_temporal_events(self.queries);
            if self.events < minimum {
                return Err(ConfigError::TemporalHistory {
                    events: self.events,
                    queries: self.queries,
                    minimum,
                });
            }
        }
        Ok(())
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_every_resource_value_that_can_exhaust_or_crash_run() {
        let invalid = [
            BenchConfig {
                events: MAX_EVENTS + 1,
                ..BenchConfig::smoke()
            },
            BenchConfig {
                queries: MAX_QUERIES + 1,
                ..BenchConfig::smoke()
            },
            BenchConfig {
                k: MAX_K + 1,
                ..BenchConfig::smoke()
            },
            BenchConfig {
                warmup: MAX_WARMUP + 1,
                ..BenchConfig::smoke()
            },
            BenchConfig {
                write_batch: 0,
                ..BenchConfig::smoke()
            },
        ];
        for config in invalid {
            assert!(config.validate().is_err(), "accepted {config:?}");
        }
    }

    #[test]
    fn temporal_validation_accepts_exactly_enough_history() {
        let queries = crate::workloads::temporal_project_count();
        let config = BenchConfig {
            workload: "temporal".to_string(),
            events: crate::workloads::minimum_temporal_events(queries),
            queries,
            ..BenchConfig::smoke()
        };
        assert!(config.validate().is_ok());
    }
}
