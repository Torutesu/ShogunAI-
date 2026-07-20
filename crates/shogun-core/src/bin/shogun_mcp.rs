//! `shogun-mcp` — the MCP server face (§6.11), a stdio JSON-RPC loop. An AI client (spawned by the
//! user) speaks the Model Context Protocol over stdin/stdout; each request runs against the same
//! DB-backed Memory API backend the REST/CLI faces use (invariant 6).
//!
//! Config via env: `SHOGUN_DB_PATH` (default `./shogun.db`). No token — over stdio the client is a
//! local subprocess the user launched (process trust); the REST/HTTP face is where the token gate
//! lives (FR-API-03). Levels still apply: an external send routes to the approval queue.
//!
//! Run (as an MCP server the client launches):
//! ```text
//! SHOGUN_DB_PATH=./shogun.db cargo run -p shogun-core --features db --bin shogun-mcp
//! ```

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use shogun_core::daemon::Db;
use shogun_core::db_backend::DbBackend;
use shogun_mcp::mcp::{serve, McpServer};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn main() -> std::io::Result<()> {
    let db_path = std::env::var("SHOGUN_DB_PATH").unwrap_or_else(|_| "./shogun.db".to_string());
    let clock: shogun_core::daemon::Clock = Arc::new(now_ms);
    let db = Db::open(&db_path, clock)
        .map_err(|e| std::io::Error::other(format!("open db {db_path}: {e}")))?;
    let server = McpServer::new(DbBackend::new(db), now_ms);

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(&server, stdin.lock(), stdout.lock())
}
