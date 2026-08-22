//! End-to-end checks for the memory benchmark.
//!
//! These run in normal CI — unlike `shogun-memory`'s `search_scale` and `retrieval_eval`, which are
//! `#[ignore]`d because they are slow. Everything here is sized so the whole file finishes in
//! seconds: the question it answers is "does the harness work", not "how fast is memory".
//!
//! The determinism tests are the important ones. If a seed stops reproducing its corpus, every
//! stored baseline silently stops being comparable to every new run, and nothing else in this
//! crate would notice.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use memory_bench::config::BenchConfig;
use memory_bench::rng::Rng;
// `Workload` is deliberately not imported: `by_name` hands back a `Box<dyn Workload>`, and
// methods on a trait object resolve without the trait in scope. The workspace denies `unused`.
use memory_bench::workload::GeneratedWorkload;
use memory_bench::workloads;

fn generate(name: &str, seed: u64, events: usize, queries: usize) -> GeneratedWorkload {
    let w = workloads::by_name(name).expect("known workload");
    let mut rng = Rng::new(seed);
    w.generate(&mut rng, events, queries)
}

/// Compare two corpora field by field. `BenchEvent` intentionally does not derive `PartialEq`
/// (it is a data carrier, not a value), so equality is spelled out here.
fn same_corpus(a: &GeneratedWorkload, b: &GeneratedWorkload) -> bool {
    a.events.len() == b.events.len()
        && a.events.iter().zip(b.events.iter()).all(|(x, y)| {
            x.ts == y.ts
                && x.content == y.content
                && x.fact_id == y.fact_id
                && x.source == y.source
                && x.window_title == y.window_title
                && x.app_bundle_id == y.app_bundle_id
        })
        && a.queries.len() == b.queries.len()
        && a.queries.iter().zip(b.queries.iter()).all(|(x, y)| {
            x.ask == y.ask && x.expected == y.expected && x.superseded == y.superseded
        })
}

#[test]
fn every_workload_is_reproducible_from_its_seed() {
    for name in workloads::ALL {
        let a = generate(name, 42, 500, 20);
        let b = generate(name, 42, 500, 20);
        assert!(
            same_corpus(&a, &b),
            "{name} was not reproducible from seed 42"
        );
    }
}

#[test]
fn a_different_seed_gives_a_different_corpus() {
    for name in workloads::ALL {
        let a = generate(name, 42, 500, 20);
        let b = generate(name, 43, 500, 20);
        assert!(!same_corpus(&a, &b), "{name} ignored its seed");
    }
}

#[test]
fn requested_event_count_is_produced_exactly() {
    for name in workloads::ALL {
        for events in [120usize, 500, 2_000] {
            let w = generate(name, 42, events, 12);
            assert_eq!(w.events.len(), events, "{name} at {events} events");
        }
    }
}

#[test]
fn the_clean_corpus_contains_no_duplicate_facts() {
    let w = generate("clean", 42, 1_000, 50);
    assert_eq!(
        w.duplicate_events(),
        0,
        "clean is the reference point; it must be duplicate-free"
    );
    assert_eq!(w.unique_facts(), 1_000);
    assert_eq!(w.queries.len(), 50);
}

#[test]
fn every_clean_query_has_a_distinct_answer_present_in_the_corpus() {
    let w = generate("clean", 42, 2_000, 60);
    let facts: std::collections::HashSet<&str> =
        w.events.iter().map(|e| e.fact_id.as_str()).collect();
    let mut answers = std::collections::HashSet::new();
    for q in &w.queries {
        for e in &q.expected {
            assert!(
                facts.contains(e.as_str()),
                "query {:?} expects absent fact {e}",
                q.ask
            );
            assert!(
                answers.insert(e.clone()),
                "two queries share the answer {e}"
            );
        }
    }
}

#[test]
fn the_duplicate_corpus_actually_repeats_facts() {
    let w = generate("duplicate", 42, 1_000, 30);
    let dups = w.duplicate_events();
    assert!(dups > 0, "duplicate workload produced no repeats");
    // ~30% by construction; the band is wide because the exact/near split is randomised.
    let share = dups as f64 / w.events.len() as f64;
    assert!(
        (0.20..=0.40).contains(&share),
        "duplicate share {share:.3} outside the design range"
    );
}

#[test]
fn the_temporal_corpus_supersedes_earlier_facts() {
    let w = generate("temporal", 42, 2_000, 8);
    assert!(!w.queries.is_empty(), "temporal produced no queries");
    for q in &w.queries {
        assert!(
            !q.superseded.is_empty(),
            "temporal query {:?} has nothing superseded",
            q.ask
        );
        for s in &q.superseded {
            assert!(
                !q.expected.contains(s),
                "a fact cannot be both current and superseded"
            );
        }
    }
}

#[test]
fn minimum_valid_temporal_corpus_keeps_history_for_every_query() {
    for queries in 1..=memory_bench::workloads::temporal_project_count() {
        let events = memory_bench::workloads::minimum_temporal_events(queries);
        let workload = generate("temporal", 42, events, queries);
        assert_eq!(workload.events.len(), events, "{queries} queries");
        assert_eq!(workload.queries.len(), queries, "{queries} queries");
        assert!(
            workload
                .queries
                .iter()
                .all(|query| !query.superseded.is_empty()),
            "{queries} queries at minimum {events} events lost temporal history"
        );
    }
}

/// The full pipeline, at a size that finishes fast. This is what the CI job runs.
#[test]
fn benchmark_runs_end_to_end_and_reports_every_required_field() {
    let cfg = BenchConfig::smoke();
    let report = memory_bench::run(&cfg).expect("benchmark should complete");

    assert_eq!(report.writes.submitted, cfg.events as u64);
    assert_eq!(
        report.writes.failed, 0,
        "no write should fail on a clean corpus"
    );
    assert!(report.writes.rows_after > 0);

    let writes = report.write_latency.expect("write latency measured");
    assert_eq!(writes.n, cfg.events);
    assert!(
        writes.p95_ms >= writes.p50_ms,
        "percentiles must be ordered"
    );
    assert!(writes.max_ms >= writes.p99_ms);

    let queries = report.query_latency.expect("query latency measured");
    assert_eq!(queries.n, cfg.queries);

    assert_eq!(report.quality.queries, cfg.queries);
    assert!(report.quality.recall_at_5.is_some());
    assert!(report.quality.mrr.is_some());

    // A clean corpus has no repeats, so these must be absent rather than zero.
    assert!(report.duplicate_collapse_rate.is_none());
    assert!(
        report.quality.stale_rate.is_none(),
        "clean corpus has nothing to go stale"
    );

    assert!(report.storage.final_logical_bytes > report.storage.initial_logical_bytes);
    assert!(
        report.slo.is_some(),
        "a run with queries can always state the search SLO"
    );
    assert!(!report.to_json().expect("serialises").is_empty());
}

/// The clean corpus is the reference point: one row per fact, nothing collapsed.
#[test]
fn a_clean_corpus_writes_one_row_per_fact() {
    let cfg = BenchConfig {
        events: 1_000,
        queries: 20,
        ..BenchConfig::smoke()
    };
    let report = memory_bench::run(&cfg).expect("run");
    assert_eq!(
        report.writes.deduplicated, 0,
        "nothing in a clean corpus should collapse"
    );
    assert_eq!(report.writes.rows_after, cfg.events as i64);
    assert_eq!(report.write_amplification, Some(1.0));
}

/// The measurement the duplicate workload exists to take: what the `content_hash` contract catches.
#[test]
fn the_duplicate_corpus_exercises_the_dedup_path() {
    let cfg = BenchConfig {
        workload: "duplicate".to_string(),
        events: 2_000,
        queries: 30,
        ..BenchConfig::smoke()
    };
    let report = memory_bench::run(&cfg).expect("run");
    assert_eq!(report.writes.submitted, 2_000);
    assert!(
        report.writes.deduplicated > 0,
        "exact repeats should hit the content_hash match"
    );
    assert!(
        report.writes.rows_after < 2_000,
        "collapsed writes must mean fewer rows than writes"
    );
    let collapse = report
        .duplicate_collapse_rate
        .expect("workload had repeats");
    assert!((0.0..=1.0).contains(&collapse));
    // Near-duplicates hash differently and survive as separate rows, so the collapse is partial.
    // This is the baseline a later commit has to improve on, asserted loosely because the exact
    // value is a measurement, not a contract.
    assert!(
        collapse < 1.0,
        "near-duplicates are not expected to collapse at the hash level"
    );
}

/// The staleness baseline. No assertion on the rate itself — that is the number being measured.
#[test]
fn the_temporal_corpus_produces_a_staleness_measurement() {
    let cfg = BenchConfig {
        workload: "temporal".to_string(),
        events: 3_000,
        queries: 10,
        ..BenchConfig::smoke()
    };
    let report = memory_bench::run(&cfg).expect("run");
    assert!(
        report.quality.temporal_queries > 0,
        "temporal queries should be recognised as such"
    );
    let rate = report
        .quality
        .stale_rate
        .expect("temporal workload can express staleness");
    assert!((0.0..=1.0).contains(&rate));
}

#[test]
fn a_report_can_be_written_and_read_back() {
    let cfg = BenchConfig {
        events: 300,
        queries: 10,
        ..BenchConfig::smoke()
    };
    let report = memory_bench::run(&cfg).expect("run");

    let dir = std::env::temp_dir().join(format!("memory-bench-test-{}", std::process::id()));
    let path = memory_bench::runner::persist(&report, &dir.to_string_lossy()).expect("persist");
    assert!(path.exists());

    let text = std::fs::read_to_string(&path).expect("read back");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(parsed["benchmark"], "shogun-memory-bench");
    assert_eq!(parsed["config"]["seed"], 42);
    assert_eq!(parsed["config"]["events"], 300);
    // The fields that make a result reproducible six weeks later.
    assert!(parsed.get("environment").is_some());
    assert!(parsed["environment"].get("git_commit").is_some());
    assert!(parsed["environment"].get("profile").is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn repeated_report_persistence_never_overwrites_the_first_artifact() {
    let cfg = BenchConfig {
        events: 300,
        queries: 20,
        ..BenchConfig::smoke()
    };
    let report = memory_bench::run(&cfg).expect("run");
    let dir = std::env::temp_dir().join(format!(
        "memory-bench-no-overwrite-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let first = memory_bench::runner::persist(&report, &dir.to_string_lossy()).expect("first");
    let first_bytes = std::fs::read(&first).expect("read first");
    let second = memory_bench::runner::persist(&report, &dir.to_string_lossy()).expect("second");

    assert_ne!(first, second, "repeated runs need distinct artifact paths");
    assert_eq!(
        std::fs::read(&first).expect("read first again"),
        first_bytes,
        "saving a second run must not change the first artifact"
    );
    assert!(
        second
            .file_stem()
            .is_some_and(|name| name.to_string_lossy().ends_with("-run2")),
        "second artifact should have a readable collision suffix: {second:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unknown_workload_is_an_error_not_a_default() {
    let cfg = BenchConfig {
        workload: "nope".to_string(),
        ..BenchConfig::smoke()
    };
    assert!(memory_bench::run(&cfg).is_err());
}

#[test]
fn direct_library_calls_reject_invalid_resource_bounds_without_panicking() {
    let cfg = BenchConfig {
        write_batch: 0,
        ..BenchConfig::smoke()
    };
    let error = memory_bench::run(&cfg).expect_err("zero batch must be rejected");
    assert!(error.to_string().contains("--write-batch"), "{error}");
}

#[test]
fn temporal_library_calls_require_real_superseded_history() {
    let cfg = BenchConfig {
        workload: "temporal".to_string(),
        events: 1,
        queries: 1,
        ..BenchConfig::smoke()
    };
    let error = memory_bench::run(&cfg).expect_err("one event cannot measure supersession");
    assert!(error.to_string().contains("at least 3 events"), "{error}");
}

#[test]
fn an_existing_db_is_refused_not_reused() {
    // Issue #221 (Severe): the benchmark migrates the schema and writes synthetic events into
    // whatever file it is handed, so an existing database must never be accepted — and must be
    // byte-identical afterwards to prove nothing touched it.
    let dir = std::env::temp_dir().join(format!("memory-bench-db-guard-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let db = dir.join("existing.db");
    std::fs::write(&db, b"this is somebody's file").expect("seed file");

    let cfg = BenchConfig {
        db_path: Some(db.to_string_lossy().into_owned()),
        ..BenchConfig::smoke()
    };
    let err = memory_bench::run(&cfg).expect_err("existing db must be refused");
    let msg = err.to_string();
    assert!(msg.contains("already exists"), "error must say why: {msg}");
    assert!(
        msg.contains("disposable"),
        "error must state the contract: {msg}"
    );
    assert_eq!(
        std::fs::read(&db).expect("still readable"),
        b"this is somebody's file",
        "refusal must leave the file untouched"
    );

    // Existing directories are caller-owned too. The marker proves refusal never enters or clears
    // them; DB/WAL/SHM only ever live below a directory this run created atomically.
    let db2 = dir.join("somebody-elses-run");
    std::fs::create_dir(&db2).expect("seed existing directory");
    let marker = db2.join("personal-memory-do-not-touch");
    std::fs::write(&marker, b"owned").expect("seed marker");
    let cfg = BenchConfig {
        db_path: Some(db2.to_string_lossy().into_owned()),
        ..BenchConfig::smoke()
    };
    let err = memory_bench::run(&cfg).expect_err("existing directory must be refused");
    assert!(err.to_string().contains("already exists"), "{err}");
    assert_eq!(std::fs::read(&marker).expect("marker survives"), b"owned");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_fresh_db_path_still_works_and_reports_no_wrong_merges() {
    let dir = std::env::temp_dir().join(format!("memory-bench-db-fresh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let db = dir.join("fresh.db");

    let cfg = BenchConfig {
        db_path: Some(db.to_string_lossy().into_owned()),
        events: 500,
        queries: 10,
        ..BenchConfig::smoke()
    };
    let report = memory_bench::run(&cfg).expect("fresh path runs");
    // The honest backend merges only byte-identical repeats, so the wrong-merge counter — the
    // issue #221 guard against rewarding lossy merging — must sit at exactly zero here.
    assert_eq!(report.writes.wrong_merges, 0);
    assert!(
        db.is_dir(),
        "--db claims a private directory, not a caller DB file"
    );
    assert!(db.join("memory-bench.sqlite").is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&db)
                .expect("scratch metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700,
            "scratch directory must not expose benchmark data to other users"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn two_runs_cannot_claim_the_same_db_directory() {
    let dir =
        std::env::temp_dir().join(format!("memory-bench-db-exclusive-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let db = dir.join("exclusive-run");
    let cfg = BenchConfig {
        events: 30,
        queries: 1,
        write_batch: 10,
        db_path: Some(db.to_string_lossy().into_owned()),
        ..BenchConfig::smoke()
    };
    let left = cfg.clone();
    let right = cfg;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let first_barrier = barrier.clone();
    let first = std::thread::spawn(move || {
        first_barrier.wait();
        memory_bench::run(&left)
    });
    let second = std::thread::spawn(move || {
        barrier.wait();
        memory_bench::run(&right)
    });
    let outcomes = [
        first.join().expect("first thread"),
        second.join().expect("second thread"),
    ];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| result
                .as_ref()
                .is_err_and(|error| error.to_string().contains("already exists")))
            .count(),
        1
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_report_records_planned_work_and_no_absolute_paths() {
    // Issue #221: asking for more queries than the workload can plant must be visible in the
    // report, and committed reports must not carry machine paths.
    let dir = std::env::temp_dir().join(format!("memory-bench-report-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let db = dir.join("paths.db");

    let cfg = BenchConfig {
        events: 3,
        queries: 500,
        db_path: Some(db.to_string_lossy().into_owned()),
        out_dir: Some(dir.to_string_lossy().into_owned()),
        ..BenchConfig::smoke()
    };
    let report = memory_bench::run(&cfg).expect("run");
    assert_eq!(
        report.generated_queries, 3,
        "clean plants at most one query per event"
    );
    assert_eq!(report.quality.queries, 3);

    let json: serde_json::Value =
        serde_json::from_str(&report.to_json().expect("json")).expect("parse");
    assert_eq!(
        json["config"]["db_path"], "paths.db",
        "file name only, never the path"
    );
    assert!(
        !report
            .to_json()
            .expect("json")
            .contains(&dir.to_string_lossy().into_owned()),
        "no absolute path may appear anywhere in the report"
    );

    let rendered = memory_bench::report::render_summary(&report);
    assert!(
        rendered.contains("500 queries requested, workload produced 3"),
        "console must state the divergence: {rendered}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
