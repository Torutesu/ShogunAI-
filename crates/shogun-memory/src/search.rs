//! Local hybrid search (FR-MEM-20, NFR-SLO-04).
//!
//! The product ranks results by fusing an FTS5 full-text list with a Warm-layer vector list
//! (Reciprocal Rank Fusion). The vector half arrives with embeddings (WP2.5); this module
//! ships the FTS half and the fusion core now, structured so the vector list plugs into the
//! same [`reciprocal_rank_fusion`] call without changing the shape.
//!
//! Every hit carries its source attribution (FR-MEM-23) so the UI can distinguish
//! capture-derived rows from integration-derived ones.

use rusqlite::{params, Connection};

/// A search result row, hydrated with the fields the UI needs (FR-MEM-23).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub event_id: i64,
    pub ts: i64,
    pub source: String,
    pub app_bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub content: String,
    /// Fusion score (higher = more relevant).
    pub score: f64,
}

/// Reciprocal Rank Fusion over several ranked id lists (each best-first, 1-based rank). The
/// score of an id is `Σ_lists 1/(k + rank)`; `k` damps the influence of low ranks (60 is the
/// canonical default). Ids are returned sorted by descending score; ties break by smaller id
/// for determinism. Pure — no DB, fully unit-tested.
pub fn reciprocal_rank_fusion(lists: &[&[i64]], k: f64) -> Vec<(i64, f64)> {
    use std::collections::HashMap;
    let mut scores: HashMap<i64, f64> = HashMap::new();
    for list in lists {
        for (i, &id) in list.iter().enumerate() {
            let rank = (i + 1) as f64;
            *scores.entry(id).or_insert(0.0) += 1.0 / (k + rank);
        }
    }
    let mut out: Vec<(i64, f64)> = scores.into_iter().collect();
    // Descending score; deterministic tie-break by ascending id.
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    out
}

/// Quote a user query as a single FTS5 string token (phrase), escaping embedded double quotes.
/// This keeps arbitrary user input from being parsed as FTS5 operators.
fn fts_quote(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

/// Full-text search over the event log, best-match first, capped at `limit`. Returns event
/// ids ordered by bm25 relevance (SQLite's bm25 is more-negative-is-better, so ascending).
pub fn fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<i64>, rusqlite::Error> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT rowid FROM event_fts WHERE event_fts MATCH ?1 ORDER BY bm25(event_fts) LIMIT ?2",
    )?;
    let ids = stmt
        .query_map(params![fts_quote(trimmed), limit as i64], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// Hydrate ranked `(id, score)` pairs into full [`SearchHit`]s, preserving the ranked order.
/// Ids no longer present in the event log (e.g. moved to Cold) are skipped.
pub fn hydrate(conn: &Connection, ranked: &[(i64, f64)]) -> Result<Vec<SearchHit>, rusqlite::Error> {
    let mut hits = Vec::with_capacity(ranked.len());
    let mut stmt = conn.prepare(
        "SELECT ts, source, app_bundle_id, window_title, content FROM event_log WHERE id = ?1",
    )?;
    for &(id, score) in ranked {
        let row = stmt
            .query_row(params![id], |r| {
                Ok(SearchHit {
                    event_id: id,
                    ts: r.get(0)?,
                    source: r.get(1)?,
                    app_bundle_id: r.get(2)?,
                    window_title: r.get(3)?,
                    content: r.get(4)?,
                    score,
                })
            })
            .ok();
        if let Some(hit) = row {
            hits.push(hit);
        }
    }
    Ok(hits)
}

/// Hybrid search entry point (FR-MEM-20). Today it fuses the FTS list with itself (a no-op
/// fusion that preserves FTS order); when the Warm vector list lands (WP2.5) it becomes the
/// second list into the same RRF call.
pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<SearchHit>, rusqlite::Error> {
    let fts = fts_search(conn, query, limit)?;
    let ranked = reciprocal_rank_fusion(&[&fts], 60.0);
    let capped: Vec<(i64, f64)> = ranked.into_iter().take(limit).collect();
    hydrate(conn, &capped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_log::{insert, NewEvent};

    fn add(conn: &Connection, content: &str, source: &str, hash: &str) -> i64 {
        insert(
            conn,
            &NewEvent {
                ts: 1,
                source,
                kind: "text",
                app_bundle_id: Some("com.apple.Safari"),
                window_title: Some("t"),
                content,
                content_hash: hash,
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn rrf_ranks_items_appearing_high_in_multiple_lists_first() {
        // id 7 is rank 1 in one list and rank 2 in the other → should win.
        let a = [7, 3, 9];
        let b = [5, 7, 1];
        let fused = reciprocal_rank_fusion(&[&a, &b], 60.0);
        assert_eq!(fused[0].0, 7);
    }

    #[test]
    fn rrf_is_deterministic_on_ties() {
        let a = [1, 2];
        let b = [2, 1];
        // 1 and 2 have identical fused scores; tie-break picks the smaller id first.
        let fused = reciprocal_rank_fusion(&[&a, &b], 60.0);
        assert_eq!(fused.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn rrf_empty_is_empty() {
        assert!(reciprocal_rank_fusion(&[], 60.0).is_empty());
        assert!(reciprocal_rank_fusion(&[&[]], 60.0).is_empty());
    }

    #[test]
    fn fts_search_finds_and_orders() {
        let conn = crate::open_in_memory().unwrap();
        add(&conn, "the annual budget spreadsheet", "capture", "h1");
        add(&conn, "unrelated lunch plans", "capture", "h2");
        let ids = fts_search(&conn, "budget", 10).unwrap();
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn empty_query_returns_nothing() {
        let conn = crate::open_in_memory().unwrap();
        add(&conn, "anything", "capture", "h1");
        assert!(fts_search(&conn, "   ", 10).unwrap().is_empty());
        assert!(search(&conn, "", 10).unwrap().is_empty());
    }

    #[test]
    fn query_with_quotes_does_not_break() {
        let conn = crate::open_in_memory().unwrap();
        add(&conn, "he said \"ship it\" today", "capture", "h1");
        // A query containing a double quote must not be parsed as an FTS operator / must not error.
        let hits = search(&conn, "ship", 10).unwrap();
        assert_eq!(hits.len(), 1);
        let none = search(&conn, "\"", 10);
        assert!(none.is_ok());
    }

    #[test]
    fn search_hydrates_with_source_attribution() {
        let conn = crate::open_in_memory().unwrap();
        add(&conn, "quarterly review notes", "gmail", "h1");
        let hits = search(&conn, "quarterly", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "gmail"); // FR-MEM-23 attribution
        assert!(hits[0].content.contains("quarterly"));
    }
}
