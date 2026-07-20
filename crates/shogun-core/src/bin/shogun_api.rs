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
use shogun_mcp::memory_api::TokenRegistry;
use shogun_mcp::server::{bind_local, serve_on, AppState, DEFAULT_PORT};

/// A real wall-clock in unix ms (never panics; 0 before the epoch).
fn wall_clock() -> Clock {
    Arc::new(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    })
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let db_path = std::env::var("SHOGUN_DB_PATH").unwrap_or_else(|_| "./shogun.db".to_string());
    let port = std::env::var("SHOGUN_API_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);

    let clock = wall_clock();
    let db = Db::open(&db_path, clock.clone())
        .map_err(|e| std::io::Error::other(format!("open db {db_path}: {e}")))?;
    let backend = Arc::new(DbBackend::new(db));

    let mut tokens = TokenRegistry::new();
    match std::env::var("SHOGUN_API_TOKEN") {
        Ok(t) if !t.is_empty() => tokens.issue(t),
        _ => eprintln!("warning: SHOGUN_API_TOKEN not set — every tool call will be 401 (only /v1/status is open)"),
    }

    let approvals = Arc::new(Mutex::new(ApprovalQueue::new()));
    let state = AppState::new(Arc::new(tokens), backend, approvals, clock);

    let listener = bind_local(port).await?;
    let addr = listener.local_addr()?;
    println!("shogun-memory-api listening on http://{addr}  (db: {db_path})");
    serve_on(listener, state).await
}
