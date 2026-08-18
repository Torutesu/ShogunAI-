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
use shogun_mcp::memory_api::TokenRegistry;
use shogun_mcp::memory_api_settings::{self, TOKENS_KEYCHAIN_ACCOUNT};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
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

fn load_token_registry() -> Result<TokenRegistry, String> {
    let mut tokens = TokenRegistry::new();
    #[cfg(target_os = "macos")]
    {
        let blob = memory_api_settings::load_token_blob_with_migration(
            || match shogun_integrations::keychain_store::get_generic_secret(
                TOKENS_KEYCHAIN_ACCOUNT,
            ) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(error) if error.code() == -25300 => Ok(None),
                Err(_) => Err("could not read Memory API tokens from Keychain".to_string()),
            },
            |bytes| {
                shogun_integrations::keychain_store::set_generic_secret(
                    TOKENS_KEYCHAIN_ACCOUNT,
                    bytes,
                )
                .map_err(|_| "could not migrate Memory API token verifiers".to_string())
            },
        )?;
        for token in blob.tokens {
            tokens.issue_verifier(&token.verifier)?;
        }
    }
    #[cfg(not(target_os = "macos"))]
    if let Ok(token) = std::env::var("SHOGUN_API_TOKEN") {
        if !token.is_empty() {
            tokens.issue(token);
        }
    }
    Ok(tokens)
}

fn gate_or_exit(db_path: &str) {
    if let Err(message) =
        memory_api_settings::require_enabled(&memory_api_settings::resolve_settings_path(db_path))
    {
        eprintln!("{message}");
        std::process::exit(1);
    }
    match load_token_registry() {
        Ok(tokens)
            if tokens.authenticate_process(std::env::var("SHOGUN_API_TOKEN").ok().as_deref()) => {}
        Ok(_) => {
            eprintln!("SHOGUN_API_TOKEN must match an issued Memory API token.");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("could not load Memory API tokens securely");
            std::process::exit(1);
        }
    }
}

fn main() -> std::io::Result<()> {
    let db_path = std::env::var("SHOGUN_DB_PATH").unwrap_or_else(|_| "./shogun.db".to_string());
    gate_or_exit(&db_path);
    let clock: shogun_core::daemon::Clock = Arc::new(now_ms);
    let db = Db::open_at_path(&db_path, clock)
        .map_err(|e| std::io::Error::other(format!("open db {db_path}: {e}")))?;
    let mut backend = DbBackend::new(db);
    if let Some(path) = visual_recall_settings_path(&db_path) {
        backend = backend.with_visual_recall_settings_path(path);
    }
    backend =
        backend.with_memory_api_settings_path(memory_api_settings::resolve_settings_path(&db_path));
    // Plan gate (issue #97): trial stamp from the desktop app's onboarding.json
    // (SHOGUN_ONBOARDING_JSON overrides the path); billing is the pre-Stripe stub. Consulted on
    // every tools/call, so trial expiry takes effect mid-session.
    let plan_source = shogun_mcp::plan_source::FilePlanSource::from_env();
    // The process-wide L3 approval queue (B-3 / E-08): created once at the composition root and
    // injected — the MCP face never owns a private queue.
    let approvals = Arc::new(std::sync::Mutex::new(
        shogun_agents::approval::ApprovalQueue::new(),
    ));
    let server = McpServer::new(backend, approvals, now_ms, move || {
        plan_source.resolve(u64::try_from(now_ms()).unwrap_or(0))
    });

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(&server, stdin.lock(), stdout.lock())
}
