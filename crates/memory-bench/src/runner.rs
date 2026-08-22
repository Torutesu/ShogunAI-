//! How we execute.
//!
//! The sequence is fixed and every run does all of it: seed, generate, ingest, warm, measure,
//! summarise, write the artifact. Nothing here knows which backend it is driving, which is what
//! lets a later intervention be measured by exactly this code.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use spike_harness::MonoClock;

use crate::backend::{MemoryBackend, ShogunBackend};
use crate::config::BenchConfig;
use crate::metrics::{LatencySeries, LatencySummary, QualityAccumulator, WriteStats};
use crate::report::{
    BenchReport, Environment, RunMode, StorageReport, BENCHMARK_NAME, REPORT_VERSION,
};
use crate::resources::ResourceTracker;
use crate::rng::Rng;
use crate::workloads;

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
    #[error("generated {workload:?} workload is invalid: {reason}")]
    InvalidGeneratedWorkload { workload: String, reason: String },
    #[error(
        "--db {0:?} already exists — refusing to touch it. The benchmark requires a new disposable \
         scratch directory that it can claim atomically; it creates memory-bench.sqlite and all \
         sidecars inside that directory. Pass a path that does not exist yet, or omit --db for an \
         in-memory run."
    )]
    DbPathExists(String),
    #[error(transparent)]
    Backend(#[from] crate::backend::BackendError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialising report: {0}")]
    Json(#[from] serde_json::Error),
}

/// How often, in events, to take a resource reading during ingest. Frequent enough that peak RSS
/// is a real sampled peak rather than two endpoints, cheap enough not to distort the run.
const RESOURCE_SAMPLE_EVERY: usize = 500;

/// Run the benchmark end to end and return the report.
///
/// `write_report` is separate ([`persist`]) so a test can run a benchmark without touching the
/// filesystem.
pub fn run(config: &BenchConfig) -> Result<BenchReport, RunError> {
    config.validate()?;
    let workload =
        workloads::by_name(&config.workload).ok_or_else(|| RunError::InvalidGeneratedWorkload {
            workload: config.workload.clone(),
            reason: "validated workload could not be resolved".to_string(),
        })?;

    // 1. Seed and generate. The corpus exists in full before the backend is even opened, so
    //    generation cost can never leak into a write-latency measurement.
    let mut rng = Rng::new(config.seed);
    let generated = workload.generate(&mut rng, config.events, config.queries);
    validate_generated_workload(config, &generated)?;

    let clock = MonoClock::new();
    let mut resources = ResourceTracker::new();
    resources.sample(clock.elapsed_ns());

    // 2. Open a fresh store. The caller's path is claimed as a directory in one atomic operation;
    //    SQLite only sees a child path inside that directory. There is no check/open gap in which
    //    another process can place a personal DB where the benchmark is about to migrate and write.
    //    Fresh also means never cleared: a `DELETE FROM` leaves high-water and freelist pages that
    //    would make storage numbers meaningless.
    let in_memory = config.db_path.is_none();
    let mut backend = match &config.db_path {
        Some(p) => ShogunBackend::open(claim_scratch_db(p)?)?,
        None => ShogunBackend::in_memory()?,
    };
    let initial_size = backend.size()?;

    // 3. Ingest, timing each write individually.
    let mut write_latency = LatencySeries::new();
    let mut fact_of: HashMap<i64, String> = HashMap::with_capacity(generated.events.len());
    let mut writes = WriteStats {
        submitted: 0,
        deduplicated: 0,
        wrong_merges: 0,
        failed: 0,
        rows_after: 0,
        unique_facts: generated.unique_facts() as u64,
        duplicate_events: generated.duplicate_events() as u64,
    };

    // Central config validation rejects zero for CLI and direct library callers alike.
    let batch = config.write_batch;
    let mut open_batch = false;
    for (i, ev) in generated.events.iter().enumerate() {
        if i % batch == 0 {
            if open_batch {
                backend.commit_batch()?;
            }
            backend.begin_batch()?;
            open_batch = true;
        }
        let t = Instant::now();
        let outcome = backend.write(ev);
        let elapsed = t.elapsed();
        writes.submitted += 1;
        match outcome {
            Ok(o) => {
                write_latency.record_ns(elapsed.as_nanos() as u64);
                if o.deduplicated {
                    writes.deduplicated += 1;
                    // The workload knows which fact this event carries and the first writer of a
                    // row owns the mapping, so every reported merge is checkable: a merge onto a
                    // row holding a different fact destroyed information, and must be counted
                    // against the backend rather than into its collapse rate (issue #221).
                    if fact_of.get(&o.event_id) != Some(&ev.fact_id) {
                        writes.wrong_merges += 1;
                    }
                }
                // First writer of a row owns the mapping. A correct duplicate resolves to the same
                // row and carries the same fact, so re-inserting would be a no-op; a wrong merge
                // keeps the original owner, which is the fact the row's text actually carries.
                fact_of
                    .entry(o.event_id)
                    .or_insert_with(|| ev.fact_id.clone());
            }
            // A failed write's latency is not a write latency; counting it would let an error path
            // that fails instantly flatter the p95.
            Err(_) => writes.failed += 1,
        }
        if i % RESOURCE_SAMPLE_EVERY == 0 {
            resources.sample(clock.elapsed_ns());
        }
    }
    if open_batch {
        backend.commit_batch()?;
    }
    writes.rows_after = backend.count()?;
    resources.sample(clock.elapsed_ns());

    // 4. Warm up. These queries run against the finished corpus and their latencies are dropped —
    //    the first query after a bulk load pays for page-cache misses the next thousand do not.
    for i in 0..config.warmup {
        if let Some(q) = generated.queries.get(i % generated.queries.len().max(1)) {
            let _ = backend.search(&q.ask, config.k);
        }
    }

    // 5. Measure retrieval.
    let mut query_latency = LatencySeries::new();
    let mut quality = QualityAccumulator::new();
    for (i, q) in generated.queries.iter().enumerate() {
        let t = Instant::now();
        let result = backend.search(&q.ask, config.k);
        let elapsed = t.elapsed();
        match result {
            Ok(ids) => {
                query_latency.record_ns(elapsed.as_nanos() as u64);
                quality.record(&ids, &fact_of, &q.expected, &q.superseded);
            }
            Err(_) => quality.record_failure(),
        }
        if i % RESOURCE_SAMPLE_EVERY == 0 {
            resources.sample(clock.elapsed_ns());
        }
    }

    // 6. Close out.
    resources.sample(clock.elapsed_ns());
    let final_size = backend.size()?;
    let query_summary = LatencySummary::of(&query_latency);
    let quality = quality.summary(config.k);
    let validity = crate::report::MeasurementValidity::assess(&writes, &quality);
    let mode = RunMode {
        // v0.1 measures the lexical half only. The backend passes no query embedding, so RRF fuses
        // one list and hybrid search degenerates to FTS. Wiring the ONNX embedder in (it needs a
        // model file on disk and an off-by-default feature) is the next commit's job; until then
        // this flag is false and the report's SLO check is marked non-authoritative because of it,
        // rather than the run quietly passing itself off as a hybrid measurement.
        semantic: false,
        in_memory,
    };
    let environment = Environment::detect();
    let slo = BenchReport::slo_for(query_summary, mode, environment.profile, validity.valid);

    Ok(BenchReport {
        benchmark: BENCHMARK_NAME,
        version: REPORT_VERSION,
        config: config.clone(),
        // What the generator actually produced. A workload can plan fewer queries than were asked
        // for (`clean` plants at most one per event), and a report that only echoed the request
        // would overstate the run (issue #221).
        generated_events: generated.events.len(),
        generated_queries: generated.queries.len(),
        environment,
        mode,
        backend: backend.name(),
        validity,
        write_amplification: writes.write_amplification(),
        duplicate_collapse_rate: writes.duplicate_collapse_rate(),
        writes,
        write_latency: LatencySummary::of(&write_latency),
        query_latency: query_summary,
        quality,
        resources: resources.summary(),
        storage: StorageReport {
            initial_logical_bytes: initial_size.logical_bytes,
            final_logical_bytes: final_size.logical_bytes,
            final_file_bytes: final_size.file_bytes,
            bytes_per_row: (writes.rows_after > 0)
                .then(|| final_size.logical_bytes as f64 / writes.rows_after as f64),
        },
        slo,
        wall_seconds: clock.elapsed_ns() as f64 / 1e9,
    })
}

fn validate_generated_workload(
    config: &BenchConfig,
    generated: &crate::workload::GeneratedWorkload,
) -> Result<(), RunError> {
    if generated.events.len() != config.events {
        return Err(RunError::InvalidGeneratedWorkload {
            workload: config.workload.clone(),
            reason: format!(
                "requested {} events but generator produced {}",
                config.events,
                generated.events.len()
            ),
        });
    }
    if generated.queries.is_empty() {
        return Err(RunError::InvalidGeneratedWorkload {
            workload: config.workload.clone(),
            reason: "generator produced no queries".to_string(),
        });
    }
    if config.workload == "temporal" {
        let expected_queries = config
            .queries
            .min(crate::workloads::temporal_project_count());
        if generated.queries.len() != expected_queries
            || generated
                .queries
                .iter()
                .any(|query| query.superseded.is_empty())
        {
            return Err(RunError::InvalidGeneratedWorkload {
                workload: config.workload.clone(),
                reason: format!(
                    "expected {expected_queries} queries with superseded history, got {} total",
                    generated.queries.len()
                ),
            });
        }
    }
    Ok(())
}

/// Atomically claim the entire namespace SQLite will use.
///
/// A file-level `exists()` check followed by SQLite open is unsafe: another process can put a DB
/// at the path between those operations. Directory creation has create-new semantics, and keeping
/// DB/WAL/SHM beneath the owned directory avoids a second race across sidecar names.
fn claim_scratch_db(path: &str) -> Result<PathBuf, RunError> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Ok(std::path::Path::new(path).join("memory-bench.sqlite")),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(RunError::DbPathExists(path.to_string()))
        }
        Err(error) => Err(RunError::Io(error)),
    }
}

/// Write the report under `out_dir` and return the file path.
///
/// The filename carries workload, scale and seed so a directory of results is readable without
/// opening any of them, and two different runs never overwrite each other.
pub fn persist(report: &BenchReport, out_dir: &str) -> Result<std::path::PathBuf, RunError> {
    std::fs::create_dir_all(out_dir)?;
    let name = format!(
        "{}-{}e-{}q-seed{}.json",
        report.config.workload, report.config.events, report.config.queries, report.config.seed
    );
    let path = std::path::Path::new(out_dir).join(name);
    std::fs::write(&path, report.to_json()?)?;
    Ok(path)
}
