//! `shogun-mcp` — the MCP server face (§6.11), a stdio JSON-RPC loop. An AI client (spawned by the
//! user) speaks the Model Context Protocol over stdin/stdout; each request runs against the same
//! DB-backed Memory API backend the REST/CLI faces use (invariant 6).
//!
//! Config via env:
//! - `SHOGUN_DB_PATH` (default `./shogun.db`)
//! - `SHOGUN_MEMORY_API_SETTINGS` (optional override for `memory_api.json`)
//! - `SHOGUN_L3_APPROVALS` (optional override for `l3_approvals.json` — shared with desktop Approvals)
//! - `SHOGUN_API_TOKEN` — **required when any Memory API tokens have been issued** (Settings →
//!   Issue). If no tokens exist yet, process-trust allows the call when Memory API is enabled
//!   (dev DX). Fail closed when Memory API is disabled.
//!
//! Soft Pro gate: `enabled` in `memory_api.json` is the product gate until Stripe WP5.1. Trial is
//! Pro-equivalent — do not block on a "trial" string alone.
//!
//! Run (as an MCP server the client launches):
//! ```text
//! SHOGUN_DB_PATH=./shogun.db SHOGUN_API_TOKEN=… cargo run -p shogun-core --features db --bin shogun-mcp
//! ```

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use shogun_core::daemon::Db;
use shogun_core::db_backend::DbBackend;
use shogun_mcp::mcp::{serve, McpServer};
use shogun_mcp::memory_api::{AuthResult, TokenRegistry};
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
                Err(error) => Err(format!("read Memory API token blob: {error}")),
            },
            |bytes| {
                shogun_integrations::keychain_store::set_generic_secret(
                    TOKENS_KEYCHAIN_ACCOUNT,
                    bytes,
                )
                .map_err(|e| format!("rewrite Memory API token blob: {e}"))
            },
        )?;
        for token in blob.tokens {
            tokens.issue_verifier(&token.verifier)?;
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = &mut tokens;
    }
    if let Ok(token) = std::env::var("SHOGUN_API_TOKEN") {
        if !token.is_empty() {
            tokens.issue(token);
        }
    }
    Ok(tokens)
}

/// Fail closed on disabled Memory API. If tokens exist, require `SHOGUN_API_TOKEN` to match one.
fn gate_or_exit(db_path: &str) {
    let path = memory_api_settings::resolve_settings_path(db_path);
    if let Err(msg) = memory_api_settings::require_enabled(&path) {
        eprintln!("{msg}");
        std::process::exit(1);
    }
    let tokens = match load_token_registry() {
        Ok(tokens) => tokens,
        Err(message) => {
            eprintln!("Memory API token loader failed: {message}");
            std::process::exit(1);
        }
    };
    if tokens.is_empty() {
        // Dev DX: no tokens issued yet — process trust when enabled.
        return;
    }
    match std::env::var("SHOGUN_API_TOKEN") {
        Ok(t) if matches!(tokens.authenticate(Some(&t)), AuthResult::Granted) => {}
        Ok(_) => {
            eprintln!(
                "SHOGUN_API_TOKEN does not match any issued Memory API token. Re-issue in Settings or fix the env."
            );
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!(
                "SHOGUN_API_TOKEN is required because Memory API tokens have been issued. Set it in your MCP client env."
            );
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
    let approvals_path = shogun_mcp::approval_store::resolve_store_path(&db_path);
    let server = McpServer::new(backend, now_ms).with_approvals_path(approvals_path);

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(&server, stdin.lock(), stdout.lock())
}
