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

/// A relevance-centred excerpt of `content`, at most `max_chars` characters.
///
/// A single captured window can be thousands of characters, so handing the model its first N
/// wastes the budget on whatever happened to be at the top of the window — usually chrome, not
/// the part that matched. This centres the window on the earliest occurrence of a query token
/// instead, and falls back to the head when nothing matches (an FTS trigram hit can land on a
/// substring no whole token covers).
///
/// Char-boundary safe: it slices by `char`, never by byte, so multi-byte text is never cut in
/// half. Matching is ASCII-case-insensitive — that covers the English path (the accuracy
/// priority) and is a no-op for scripts without case, so no text is mishandled either way.
pub fn excerpt(content: &str, query: &str, max_chars: usize) -> String {
    let chars: Vec<char> = content.chars().collect();
    if max_chars == 0 {
        return String::new();
    }
    if chars.len() <= max_chars {
        return content.trim().to_string();
    }
    // Earliest position among the query's tokens; short tokens are skipped as too noisy.
    let hit = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 2)
        .filter_map(|t| find_ci(&chars, t))
        .min();

    // Keep a little of the run-up so the match reads in context rather than starting mid-thought.
    let lead = max_chars / 3;
    let mut start = hit.unwrap_or(0).saturating_sub(lead);
    let mut end = start + max_chars;
    if end > chars.len() {
        end = chars.len();
        start = end.saturating_sub(max_chars);
    }
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(chars[start..end].iter().collect::<String>().trim());
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// Char-index of `needle` in `hay`, ASCII-case-insensitively. Works in char space so the index
/// it returns can be used to slice `hay` directly.
fn find_ci(hay: &[char], needle: &str) -> Option<usize> {
    let n: Vec<char> = needle.chars().map(|c| c.to_ascii_lowercase()).collect();
    if n.is_empty() || n.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - n.len())
        .find(|&i| hay[i..i + n.len()].iter().map(|c| c.to_ascii_lowercase()).eq(n.iter().copied()))
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

/// FTS-only search — the lexical half alone. Kept for callers without an embedder.
pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<SearchHit>, rusqlite::Error> {
    search_hybrid(conn, query, None, limit)
}

/// Hybrid search (FR-MEM-20): fuse the FTS lexical list with the Warm-layer vector list via RRF.
/// The caller supplies the query embedding (so shogun-memory stays decoupled from the model);
/// pass `None` to run FTS-only. Both lists rank the same event ids, so the vector half simply
/// becomes a second input to the same fusion. Un-embedded events (no `event_vec` row yet,
/// FR-MEM-22) still appear via the FTS list.
pub fn search_hybrid(
    conn: &Connection,
    query: &str,
    query_embedding: Option<&[f32]>,
    limit: usize,
) -> Result<Vec<SearchHit>, rusqlite::Error> {
    let fts = fts_search(conn, query, limit)?;
    let vec = match query_embedding {
        Some(emb) => crate::vector::knn(conn, emb, limit)?,
        None => Vec::new(),
    };
    let ranked = reciprocal_rank_fusion(&[&fts, &vec], 60.0);
    let capped: Vec<(i64, f64)> = ranked.into_iter().take(limit).collect();
    hydrate(conn, &capped)
}

#[cfg(test)]
mod excerpt_tests {
    use super::excerpt;

    #[test]
    fn short_content_is_returned_whole() {
        assert_eq!(excerpt("  send Alice the deck  ", "deck", 100), "send Alice the deck");
    }

    #[test]
    fn window_centres_on_the_match_not_the_head() {
        // The interesting sentence is buried at the end of a long window capture.
        let content = format!("{}NEEDLE pricing decision{}", "chrome ".repeat(200), " x".repeat(200));
        let got = excerpt(&content, "needle", 80);
        assert!(got.contains("NEEDLE pricing decision"), "match must survive the cut: {got}");
        assert!(got.chars().count() <= 82, "budget respected (plus ellipses): {got}");
    }

    #[test]
    fn falls_back_to_the_head_when_nothing_matches() {
        let content = "alpha ".repeat(100);
        let got = excerpt(&content, "zzz", 40);
        assert!(got.starts_with("alpha"), "no match → head window: {got}");
        assert!(got.ends_with('…'));
    }

    #[test]
    fn never_splits_a_multi_byte_char() {
        // Pure multi-byte content: slicing by byte here would panic or produce invalid UTF-8.
        let content = "あ".repeat(500);
        let got = excerpt(&content, "あ", 50);
        assert!(got.chars().all(|c| c == 'あ' || c == '…'));
        assert!(got.chars().filter(|&c| c == 'あ').count() <= 50);
    }

    #[test]
    fn matching_is_case_insensitive_for_the_english_path() {
        let content = format!("{}Quarterly Deck review{}", "z ".repeat(200), " y".repeat(200));
        let got = excerpt(&content, "QUARTERLY", 60);
        assert!(got.contains("Quarterly Deck"), "case-insensitive hit expected: {got}");
    }

    #[test]
    fn zero_budget_yields_nothing() {
        assert_eq!(excerpt("anything at all", "any", 0), "");
    }
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

    #[test]
    fn hybrid_search_finds_semantic_match_fts_would_miss() {
        use crate::embed::{Embedder, MockEmbedder, E5_SMALL_DIM};
        let conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);

        // A doc that shares tokens with the query but NOT the exact FTS term.
        let id = add(&conn, "the budget review meeting is on friday", "gmail", "h1");
        let v = m.embed_passages(&["the budget review meeting is on friday"]).unwrap()[0].clone();
        crate::vector::upsert(&conn, id, &v).unwrap();

        // Query term "standup" isn't in the doc (FTS finds nothing), but the embedding overlaps
        // on "review"/"meeting" so the vector list surfaces it — hybrid fusion returns it.
        let q = m.embed_query("review meeting standup").unwrap();
        let fts_only = search(&conn, "standup", 10).unwrap();
        assert!(fts_only.is_empty(), "FTS alone should miss it");
        let hybrid = search_hybrid(&conn, "standup", Some(&q), 10).unwrap();
        assert_eq!(hybrid.len(), 1, "the vector half should surface the semantic match");
        assert_eq!(hybrid[0].event_id, id);
    }
}
