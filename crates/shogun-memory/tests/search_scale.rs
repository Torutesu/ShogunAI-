//! Search scaling check (§6.3 acceptance: "search p95 at 100k events"; NFR-SLO-04).
//!
//! Ignored by default so it never slows normal CI; run explicitly with:
//!   cargo test -p shogun-memory --test search_scale -- --ignored --nocapture
//!
//! This is an in-memory scaling sanity check, not the product SLO measurement (that runs
//! on-device against the real WAL file with the vector half fused in). It verifies the FTS
//! half returns correct results and stays fast as the log grows to 100k rows.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Instant;

use shogun_memory::{event_log, search};

#[test]
#[ignore = "scaling bench: run with --ignored"]
fn fts_search_scales_to_100k_events() {
    let conn = shogun_memory::open_in_memory().unwrap();

    const N: i64 = 100_000;
    let insert_start = Instant::now();
    {
        // One transaction for the bulk load so the trigger-maintained FTS index is built once.
        conn.execute_batch("BEGIN").unwrap();
        for i in 0..N {
            // Most rows are filler; sprinkle a rare needle token so the query is selective.
            let content = if i % 5000 == 0 {
                format!("event {i} contains the needle_token marker")
            } else {
                format!("event {i} ordinary background chatter about meetings and code")
            };
            event_log::insert(
                &conn,
                &event_log::NewEvent {
                    ts: i,
                    source: "capture",
                    kind: "text",
                    app_bundle_id: Some("com.apple.Safari"),
                    window_title: Some("t"),
                    content: &content,
                    content_hash: &format!("h{i}"),
                    dwell_ms: 0,
                    display_id: None,
                    window_bounds: None,
                },
            )
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
    }
    let insert_ms = insert_start.elapsed().as_millis();

    // Time a selective search over the full 100k.
    let mut worst = 0u128;
    let runs = 20;
    let mut hits_len = 0;
    for _ in 0..runs {
        let t = Instant::now();
        let hits = search::search(&conn, "needle_token", 50).unwrap();
        worst = worst.max(t.elapsed().as_micros());
        hits_len = hits.len();
    }
    let total: i64 = conn.query_row("SELECT count(*) FROM event_log", [], |r| r.get(0)).unwrap();

    eprintln!(
        "[search_scale] inserted {total} rows in {insert_ms}ms; search worst-of-{runs} = {:.2}ms; hits={hits_len}",
        worst as f64 / 1000.0
    );

    assert_eq!(total, N);
    assert_eq!(hits_len, (N / 5000) as usize, "needle should match every 5000th row");
    // Generous ceiling for an in-memory FTS query (the on-device SLO is 500ms p95 including
    // hydration + the vector half); a regression that blows past this is worth catching.
    assert!(worst < 500_000, "search worst-case {}us exceeded 500ms", worst);
}
