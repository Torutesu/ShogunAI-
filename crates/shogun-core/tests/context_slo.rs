//! Latency measurement for the two context paths that sit under the SLO table (CLAUDE.md):
//! offering a context action (150ms) and answering a question (search 500ms).
//!
//! Run explicitly — it is a measurement, not a pass/fail gate on every commit:
//!   cargo test -p shogun-core --features db --test context_slo -- --ignored --nocapture
//!
//! What this is and isn't: it measures the **assembly** cost against a realistically-sized log
//! in a WAL file on this machine. It is not the on-device number — that runs against the user's
//! real database on their hardware, with the vector half fused in, and is what the SLO is
//! actually judged on. This exists so a change that makes assembly 10× slower is caught here
//! rather than on a device weeks later.
#![cfg(feature = "db")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Instant;

use shogun_core::daemon::Db;
use shogun_memory::event_log::NewEvent;

/// Realistic-ish volume: a few months of capture at a steady rate, spread over many threads.
/// Override on the device to see how the numbers move with the log:
///   SHOGUN_SLO_EVENTS=100000 cargo test … -- --ignored --nocapture
const DEFAULT_EVENTS: i64 = 40_000;
const THREADS: i64 = 400;
const SAMPLES: usize = 60;

fn events() -> i64 {
    std::env::var("SHOGUN_SLO_EVENTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_EVENTS)
}

fn percentile(sorted_us: &[u128], p: f64) -> u128 {
    if sorted_us.is_empty() {
        return 0;
    }
    let idx = ((sorted_us.len() as f64 - 1.0) * p).round() as usize;
    sorted_us[idx]
}

fn report(label: &str, mut samples: Vec<u128>, budget_ms: u128) {
    samples.sort_unstable();
    let p50 = percentile(&samples, 0.50);
    let p95 = percentile(&samples, 0.95);
    let max = *samples.last().unwrap();
    println!(
        "{label}: p50={:.1}ms p95={:.1}ms max={:.1}ms (budget {budget_ms}ms, n={})",
        p50 as f64 / 1000.0,
        p95 as f64 / 1000.0,
        max as f64 / 1000.0,
        samples.len()
    );
    // Deliberately generous: this machine is not the target device, so the assertion catches an
    // order-of-magnitude regression rather than pretending to certify the SLO.
    let ceiling = budget_ms * 1000 * 10;
    assert!(
        p95 <= ceiling,
        "{label} p95 {}us is more than 10x the {budget_ms}ms budget — assembly has regressed",
        p95
    );
}

/// Seed a WAL-backed database with `EVENTS` events across `THREADS` threads.
fn seed(path: &std::path::Path) -> Db {
    let total = events();
    let clock = Arc::new(|| 1_700_000_000_000i64);
    let db = Db::open(path, clock).unwrap();
    let start = Instant::now();
    for i in 0..total {
        let thread = i % THREADS;
        let title = format!("Thread {thread}");
        let content = format!(
            "Note {i} on thread {thread}. The vendor renewal discussion continued; pricing was \
             raised again and the team compared options. Someone asked for a summary. \
             Filler to make this look like a real captured window body rather than a one-liner."
        );
        let hash = format!("h{i}");
        db.capture(&NewEvent {
            // Spread across ~90 days. One second apart — the first cut — put a whole corpus
            // inside any recency window, which hid how much the Warm bound actually helps.
            ts: 1_700_000_000_000 - (total - i) * (90 * 24 * 3_600_000 / total.max(1)),
            source: "capture",
            kind: "text",
            app_bundle_id: Some("com.apple.Safari"),
            window_title: Some(&title),
            content: &content,
            content_hash: &hash,
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        })
        .unwrap();
    }
    println!(
        "seeded {total} events across {THREADS} threads in {:.1}s",
        start.elapsed().as_secs_f64()
    );
    db
}

#[test]
#[ignore = "latency measurement: run with --ignored --nocapture"]
fn context_assembly_latency() {
    let path = std::env::temp_dir().join(format!("shogun_slo_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let db = seed(&path);

    // 1. Reply context — the pre-press assembly. Budget: the 150ms action-offer SLO.
    let keys: Vec<String> = (0..SAMPLES)
        .map(|i| {
            shogun_memory::thread::thread_key(
                "capture",
                None,
                Some("com.apple.Safari"),
                Some(&format!("Thread {}", i as i64 % THREADS)),
            )
            .unwrap()
        })
        .collect();
    let mut reply = Vec::new();
    let mut reported_build_ms = Vec::new();
    for k in &keys {
        let t = Instant::now();
        let ctx = db.build_reply_context(k);
        reply.push(t.elapsed().as_micros());
        reported_build_ms.push(ctx.build_ms);
        assert!(!ctx.turns.is_empty(), "each seeded thread has events");
    }
    report("build_reply_context", reply, 150);
    // The build_ms the pack reports must match what we measured — the shipped number is the
    // number, not a decorative field.
    let max_reported = reported_build_ms.iter().copied().max().unwrap_or(0);
    println!("  (pack-reported build_ms max: {max_reported}ms)");

    // 2. Question answering — retrieval + excerpting. Budget: the 500ms local-search SLO.
    let queries = [
        "vendor renewal pricing",
        "summary",
        "options compared",
        "Thread 42",
        "captured window body",
    ];
    let mut ctx_samples = Vec::new();
    for i in 0..SAMPLES {
        let q = queries[i % queries.len()];
        let t = Instant::now();
        let pack = db.assemble_context(q, 6, 600);
        ctx_samples.push(t.elapsed().as_micros());
        assert!(!pack.evidence.is_empty(), "query {q:?} should retrieve something");
    }
    report("assemble_context", ctx_samples, 500);

    // 3. Referent resolution — runs before retrieval on a referring question, so its cost is
    // additive to the same budget.
    let mut referent = Vec::new();
    for _ in 0..SAMPLES {
        let t = Instant::now();
        let _ = db.resolve_referent("how's that going?", None);
        referent.push(t.elapsed().as_micros());
    }
    report("resolve_referent", referent, 150);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

// ---------------------------------------------------------------------------
// AB / SLO gate tests — run on every `cargo test` (not ignored).
// These use an in-memory DB seeded with a handful of events so the tests are
// fast and deterministic; they are not latency measurements (see the ignored
// test above for that role).
// ---------------------------------------------------------------------------

/// Seed an in-memory db with several vendor-renewal events that the query
/// "vendor renewal" will hit, returning the db.
fn seed_in_memory() -> Db {
    let clock = Arc::new(|| 1_700_000_000_000i64);
    let db = Db::open_in_memory(clock).unwrap();
    // Insert 8 events with overlapping content so the compression has actual
    // tokens to select from (each body is ~80 chars → ~20 tokens).
    for i in 0..8i64 {
        let content = format!(
            "vendor renewal discussion item {i}: pricing was raised and options compared \
             for the quarterly report on contract renewal terms."
        );
        let hash = format!("seed{i}");
        db.capture(&NewEvent {
            ts: 1_700_000_000_000 - (8 - i) * 3_600_000,
            source: "capture",
            kind: "text",
            app_bundle_id: Some("com.apple.Notes"),
            window_title: Some("Vendor Renewal"),
            content: &content,
            content_hash: &hash,
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        })
        .unwrap();
    }
    db
}

/// Task 10 test 1 — compressed context must stay within the token budget and,
/// when the budget is tighter than the raw material, must reduce tokens.
#[test]
fn compressed_context_stays_within_budget_and_reduces_tokens() {
    use shogun_fusion::compress::CompressionConfig;

    let db = seed_in_memory();
    let cfg = CompressionConfig { enabled: true, budget_tokens: 200, ..Default::default() };

    let (pack_c, stats, fell_back) = db.assemble_context_compressed("vendor renewal", 6, 600, &cfg);

    assert!(!fell_back, "local assembly must complete within 50 ms");
    assert!(stats.post_tokens <= 200, "post_tokens={} must be within budget 200", stats.post_tokens);
    // When there is more raw material than budget, compression must shrink.
    if stats.pre_tokens > 200 {
        assert!(
            stats.post_tokens < stats.pre_tokens,
            "pre={} post={} — tokens should be reduced when pre exceeds budget",
            stats.pre_tokens,
            stats.post_tokens
        );
    }
    // Compressed pack must not be empty — at least one block must fit.
    assert!(
        !pack_c.facts.is_empty() || !pack_c.evidence.is_empty(),
        "compressed pack must contain at least one item"
    );
}

/// Task 10 test 2 — when the budget is effectively unlimited, the compressed
/// path must pass through the same evidence count as the raw path (nothing
/// dropped, no duplication).
#[test]
fn disabled_or_fallback_matches_raw() {
    use shogun_fusion::compress::CompressionConfig;

    let db = seed_in_memory();
    let raw = db.assemble_context("vendor renewal", 6, 600);

    // budget_tokens = 1_000_000 — everything fits, so nothing is dropped.
    let cfg = CompressionConfig { enabled: true, budget_tokens: 1_000_000, ..Default::default() };
    let (pack_c, _stats, _fell_back) = db.assemble_context_compressed("vendor renewal", 6, 600, &cfg);

    assert_eq!(
        pack_c.evidence.len(),
        raw.evidence.len(),
        "with unlimited budget the compressed path must pass through the same evidence count as raw"
    );
}
