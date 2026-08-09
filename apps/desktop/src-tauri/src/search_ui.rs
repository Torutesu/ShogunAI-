//! In-panel memory search command (Plan B-6, §6.4 local search / SLO-04). Bridges the Rust core's
//! hybrid search (`Db::search` — lexical FTS + semantic when the local model is present, Warm-first)
//! to the webview's search box.
//!
//! The projection here is deliberately narrow: an excerpt centred on the match (never the whole
//! captured window), the source app, and the timestamp. Window titles are NOT serialized — the
//! panel shows app names only (no usernames/paths leak into the UI), matching the header chip.
//! Data stays Rust-owned (invariant 1); the webview renders what this returns and nothing more.
#![allow(dead_code)]

/// Excerpt budget, in characters. Enough to read the match in context on one or two lines of the
/// panel; the full event stays in the DB.
pub const EXCERPT_CHARS: usize = 160;

/// Result-count ceiling. The panel is for a glance — a long list belongs in the Full UI.
pub const MAX_RESULTS: usize = 20;

/// One search result row for the webview (serde projection of `shogun_memory::search::SearchHit`).
#[derive(serde::Serialize, Clone)]
pub struct SearchHitView {
    /// Event-log id (a stable handle for a future "open the evidence" jump).
    pub event_id: i64,
    /// Event timestamp (unix ms) — the UI renders it as relative time.
    pub ts: i64,
    /// Capture source (e.g. "ax", "gmail", "meeting").
    pub source: String,
    /// App bundle id when the event came from a window capture; empty otherwise.
    pub app: String,
    /// Relevance-centred excerpt (`shogun_memory::search::excerpt`), char-boundary safe.
    pub excerpt: String,
}

#[cfg(target_os = "macos")]
pub mod mac {
    use shogun_core::daemon::Db;

    use super::{SearchHitView, EXCERPT_CHARS, MAX_RESULTS};

    /// Tauri command: hybrid search over the event log for the panel's search box. Empty query →
    /// empty list (the Db guards this too). `limit` is clamped to [`MAX_RESULTS`].
    #[tauri::command]
    pub fn search_memory(query: String, limit: Option<usize>, db: tauri::State<'_, Db>) -> Vec<SearchHitView> {
        let limit = limit.unwrap_or(8).min(MAX_RESULTS);
        db.search(&query, limit)
            .into_iter()
            .map(|hit| SearchHitView {
                event_id: hit.event_id,
                ts: hit.ts,
                source: hit.source,
                app: hit.app_bundle_id.unwrap_or_default(),
                excerpt: shogun_memory::search::excerpt(&hit.content, &query, EXCERPT_CHARS),
            })
            .collect()
    }
}
