//! `shogun-api` — the runnable Memory API server (§6.11). Composes the daemon's DB, the DB-backed
//! Memory API backend, the shared approval queue, and the localhost REST listener into one process.
//!
//! It is the composition root: every piece was unit-tested in its own crate; here they are wired.
//! Run it with:
//!
//! ```text
//! SHOGUN_API_TOKEN=dev-token cargo run -p shogun-core --features daemon-server --bin shogun-api
//! curl -s 127.0.0.1:7464/v1/status
//! curl -s -H "Authorization: Bearer dev-token" 127.0.0.1:7464/v1/state/commitments
//! ```
//!
//! Config via env: `SHOGUN_DB_PATH` (default `./shogun.db`), `SHOGUN_API_TOKEN` (issues one client
//! token; without it every call is 401), `SHOGUN_API_PORT` (default 7464, ephemeral fallback if busy).

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use shogun_agents::approval::ApprovalQueue;
use shogun_core::daemon::{Clock, Db};
use shogun_core::db_backend::DbBackend;
use shogun_core::metrics::{
    render_snapshots_json_with_lessons_harness_and_sanitizer, SanitizerCounters, SloRegistry,
};
use shogun_mcp::memory_api::TokenRegistry;
use shogun_mcp::server::{bind_local, serve_on, AppState, MetricsSource, DEFAULT_PORT};

/// The live SLO metrics source served at `GET /v1/metrics` (NFR-SLO-00). Wraps the shared
/// registry the runtime records into; here it starts empty, so every SLO reads as unmeasured until
/// the notch runtime populates it (silence ≠ success, spec §4.5). The D-6 lesson counters
/// (active lessons, feedback in the last 7 days) come from the DB; an unreadable DB renders
/// `lessons.measured:false` in the same convention. H1 harness counters (compression + tool loop)
/// ride next to them: unmeasured until an assemble or loop has run; never prompt text.
struct RegistryMetrics {
    registry: Arc<Mutex<SloRegistry>>,
    db: Db,
}

impl MetricsSource for RegistryMetrics {
    fn snapshot_json(&self) -> String {
        let lessons = self.db.lesson_counters();
        let harness = self.db.harness_counters();
        let snap = shogun_memory::sanitize::snapshot();
        let sanitizer = SanitizerCounters {
            events_stripped: snap.events_stripped,
            chars_removed: snap.chars_removed,
        };
        self.registry
            .lock()
            .map(|r| {
                render_snapshots_json_with_lessons_harness_and_sanitizer(
                    &r.snapshot_all(),
                    lessons,
                    harness,
                    sanitizer,
                )
            })
            .unwrap_or_else(|_| {
                r#"{"metrics":[],"lessons":{"measured":false},"harness":{"measured":false},"sanitizer":{"events_stripped":0,"chars_removed":0}}"#
                    .to_string()
            })
    }
}

/// Build the metrics source for the API process (empty SLO registry + DB-backed lesson counters).
fn metrics_source(db: Db) -> Arc<dyn MetricsSource> {
    Arc::new(RegistryMetrics { registry: Arc::new(Mutex::new(SloRegistry::new())), db })
}

/// A real wall-clock in unix ms (never panics; 0 before the epoch).
fn wall_clock() -> Clock {
    Arc::new(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    })
}

fn visual_recall_settings_path(db_path: &str) -> Option<std::path::PathBuf> {
    std::env::var("SHOGUN_VISUAL_RECALL_SETTINGS")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::path::Path::new(db_path)
                .parent()
                .map(|p| p.join("visual_recall.json"))
        })
}

fn db_backend(db: Db) -> DbBackend {
    let mut backend = DbBackend::new(db);
    let db_path = std::env::var("SHOGUN_DB_PATH").unwrap_or_else(|_| "./shogun.db".to_string());
    if let Some(path) = visual_recall_settings_path(&db_path) {
        backend = backend.with_visual_recall_settings_path(path);
    }
    backend
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let db_path = std::env::var("SHOGUN_DB_PATH").unwrap_or_else(|_| "./shogun.db".to_string());
    let port = std::env::var("SHOGUN_API_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);

    let clock = wall_clock();
    let db = Db::open_at_path(&db_path, clock.clone())
        .map_err(|e| std::io::Error::other(format!("open db {db_path}: {e}")))?;
    let metrics = metrics_source(db.clone());
    let backend = Arc::new(db_backend(db));

    let mut tokens = TokenRegistry::new();
    match std::env::var("SHOGUN_API_TOKEN") {
        Ok(t) if !t.is_empty() => tokens.issue(t),
        _ => eprintln!("warning: SHOGUN_API_TOKEN not set — every tool call will be 401 (only /v1/status is open)"),
    }

    let approvals = Arc::new(Mutex::new(ApprovalQueue::new()));
    // Plan gate (issue #97): the trial stamp comes from the desktop app's onboarding.json
    // (SHOGUN_ONBOARDING_JSON overrides the path); billing is the pre-Stripe stub. Re-resolved on
    // every request so a trial expiring while the server runs locks the next call.
    let plan_source = shogun_mcp::plan_source::FilePlanSource::from_env();
    let plan_clock = clock.clone();
    let state = AppState::new(Arc::new(tokens), backend, approvals, clock)
        .with_metrics(metrics)
        .with_entitlements(Arc::new(move || {
            plan_source.resolve(u64::try_from((plan_clock)()).unwrap_or(0))
        }));

    let listener = bind_local(port).await?;
    let addr = listener.local_addr()?;
    println!("shogun-memory-api listening on http://{addr}  (db: {db_path})");
    serve_on(listener, state).await
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    /// Boot the real server on a background thread (own runtime) and return its port. Uses an
    /// in-memory DB seeded with nothing; the token `dev` is issued.
    fn boot_server() -> u16 {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async move {
                let db = Db::open_in_memory(wall_clock()).unwrap();
                let metrics = metrics_source(db.clone());
                let backend = Arc::new(db_backend(db));
                let mut tokens = TokenRegistry::new();
                tokens.issue("dev");
                let approvals = Arc::new(Mutex::new(ApprovalQueue::new()));
                let state =
                    AppState::new(Arc::new(tokens), backend, approvals, wall_clock()).with_metrics(metrics);
                let listener = bind_local(0).await.unwrap();
                let port = listener.local_addr().unwrap().port();
                tx.send(port).unwrap();
                let _ = serve_on(listener, state).await;
            });
        });
        let port = rx.recv().unwrap();
        // give the listener a moment to start accepting
        std::thread::sleep(Duration::from_millis(300));
        port
    }

    #[test]
    fn cli_client_drives_the_real_server_end_to_end() {
        use shogun_cli::http::request;
        let port = boot_server();

        // status is open
        let r = request(port, "GET", "/v1/status", None, None).unwrap();
        assert_eq!(r.status, 200);
        assert!(r.body.contains("shogun-memory-api"));

        // a tool call without a token is 401 (FR-API-03)
        let r = request(port, "GET", "/v1/state/people", None, None).unwrap();
        assert_eq!(r.status, 401);

        // write a note, then search it back — the full write→persist→read loop over the socket
        let r = request(port, "POST", "/v1/memory/notes", Some("dev"), Some("call Bob about the roadmap")).unwrap();
        assert_eq!(r.status, 202);
        assert!(r.body.contains("\"id\":"));

        let r = request(port, "GET", "/v1/memory/search?q=roadmap", Some("dev"), None).unwrap();
        assert_eq!(r.status, 200);
        assert!(r.body.contains("call Bob about the roadmap"), "search body: {}", r.body);

        // the in-product SLO snapshot is served, open like status (NFR-SLO-00); empty registry ⇒
        // every SLO reads unmeasured, never a false green (spec §4.5).
        let r = request(port, "GET", "/v1/metrics", None, None).unwrap();
        assert_eq!(r.status, 200);
        assert!(r.body.contains("\"metrics\":"), "metrics body: {}", r.body);
        assert!(r.body.contains("NFR-SLO-01"), "metrics body: {}", r.body);
        assert!(r.body.contains("\"measured\":false"), "unmeasured SLOs must not read as pass: {}", r.body);
        // the D-6 lesson counters ride on the same surface: a live DB reports real (zero) counts
        assert!(
            r.body.contains(r#""lessons":{"active_lessons":0,"feedback_events_last_7d":0,"measured":true}"#),
            "lesson counters missing: {}",
            r.body
        );
        assert!(
            r.body.contains(r#""harness":{"measured":false}"#),
            "unmeasured harness must not read as zeros: {}",
            r.body
        );
        assert!(
            r.body.contains(r#""sanitizer":{"events_stripped":"#),
            "sanitizer counts missing: {}",
            r.body
        );

        // An external send never runs (FR-API-04, invariant 4). In THIS binary it is also never
        // accepted: standalone shogun-api has no confirm UI, so an enqueued L3 proposal would sit
        // until it expired and the caller would have been told "pending" for something that could
        // not be approved. Absent surface ⇒ 501, said out loud.
        let r = request(
            port,
            "POST",
            "/v1/actions/execute",
            Some("dev"),
            Some(r#"{"kind":"send_email","to":"a@b.com","subject":"s","body":"b"}"#),
        )
        .unwrap();
        assert_eq!(r.status, 501);
        assert!(r.body.contains("no_approval_surface"), "body: {}", r.body);
        assert!(!r.body.contains("\"pending\":true"), "must not read as accepted: {}", r.body);
    }
}
