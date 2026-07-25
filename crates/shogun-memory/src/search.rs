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

/// Longest sequence of terms sent to FTS. A pathological paste should not turn into a thousand-
/// clause MATCH; the leading terms carry the question anyway.
const MAX_FTS_TERMS: usize = 24;

/// Build the FTS5 MATCH expression for a user's question.
///
/// Quoting matters twice over. Unquoted, a question containing `-`, `*`, `OR` or `NEAR` is parsed
/// as FTS operators and errors or silently means something else. But quoting the *whole* question
/// makes it a single phrase, which only matches text containing those words contiguously — and a
/// question almost never appears verbatim in the answer ("vendor renewal pricing" does not occur
/// in "the vendor renewal discussion continued; pricing was raised"). That returned nothing for
/// essentially every multi-word question.
///
/// So each term is quoted separately and combined with OR: any term can match, and bm25 ranks a
/// document that matches more — and rarer — terms higher, which is the behaviour a question wants.
///
/// The index uses a trigram tokenizer, so a term shorter than three characters cannot match
/// anything and is dropped. For CJK, which has no spaces to split on, a long run is expanded into
/// its overlapping trigrams — that is how a trigram index is queried for those scripts, and it is
/// the same OR-of-terms shape.
///
/// Returns `None` when nothing usable is left, which the caller treats as "no results" rather than
/// running an empty MATCH.
fn fts_query(query: &str) -> Option<String> {
    let mut terms: Vec<String> = Vec::new();
    for raw in query.split(|c: char| !c.is_alphanumeric()) {
        if terms.len() >= MAX_FTS_TERMS {
            break;
        }
        let chars: Vec<char> = raw.chars().collect();
        if chars.len() < 3 {
            continue;
        }
        if chars.iter().any(|c| is_cjk(*c)) {
            // No word boundaries to rely on: match on the run's trigrams.
            for w in chars.windows(3) {
                if terms.len() >= MAX_FTS_TERMS {
                    break;
                }
                terms.push(quote(&w.iter().collect::<String>()));
            }
        } else if is_stopword(raw) {
            // Keep going — a stopword contributes no signal but its presence still means the
            // question had words in it.
        } else {
            terms.push(quote(raw));
        }
    }
    if terms.is_empty() {
        // A question made only of function words has nothing to retrieve on. Returning None is
        // honest: matching every document via "the" would fill the result budget with noise and
        // push out anything real.
        return None;
    }
    Some(terms.join(" OR "))
}

/// English function words, which match nearly every document and so only crowd out real hits.
///
/// bm25 already scores them near zero, but scoring is not the problem — the result *limit* is:
/// a handful of slots filled by documents that merely contain "the" are slots a real match
/// cannot have. Kept deliberately small and closed-class; anything with topical meaning stays.
fn is_stopword(term: &str) -> bool {
    const STOPWORDS: &[&str] = &[
        "the", "and", "for", "are", "was", "were", "you", "your", "our", "their", "his", "her",
        "its", "that", "this", "these", "those", "with", "from", "into", "about", "what", "when",
        "where", "which", "who", "whom", "how", "why", "did", "does", "done", "have", "has", "had",
        "can", "could", "would", "should", "will", "shall", "may", "might", "not", "but", "any",
        "all", "some", "there", "here", "then", "than", "them", "they", "she", "him", "get", "got",
    ];
    let lower = term.to_ascii_lowercase();
    STOPWORDS.contains(&lower.as_str())
}

fn quote(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x309F | 0x30A0..=0x30FF | 0x4E00..=0x9FFF | 0xFF66..=0xFF9F
    )
}

/// Full-text search over the event log, best-match first, capped at `limit`. Returns event
/// ids ordered by bm25 relevance (SQLite's bm25 is more-negative-is-better, so ascending).
pub fn fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<i64>, rusqlite::Error> {
    let Some(expr) = fts_query(query) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(
        "SELECT rowid FROM event_fts WHERE event_fts MATCH ?1 ORDER BY bm25(event_fts) LIMIT ?2",
    )?;
    let ids = stmt
        .query_map(params![expr, limit as i64], |r| r.get::<_, i64>(0))?
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

/// The retrieval bug this file exists to prevent a repeat of: a question is not a phrase.
#[cfg(test)]
mod fts_query_tests {
    use super::*;
    use crate::event_log::{insert, NewEvent};

    fn seeded() -> Connection {
        let conn = crate::open_in_memory().unwrap();
        insert(
            &conn,
            &NewEvent {
                ts: 1,
                source: "capture",
                kind: "text",
                app_bundle_id: Some("com.apple.Safari"),
                window_title: Some("notes"),
                content: "The vendor renewal discussion continued; pricing was raised again.",
                content_hash: "h1",
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap();
        conn
    }

    #[test]
    fn a_multi_word_question_matches_without_being_contiguous() {
        // These words all appear, but never as this phrase — the whole-query-quoted version
        // returned nothing here, which silently emptied retrieval for most real questions.
        let conn = seeded();
        assert_eq!(fts_search(&conn, "vendor renewal pricing", 10).unwrap().len(), 1);
        assert_eq!(fts_search(&conn, "what did we decide about pricing?", 10).unwrap().len(), 1);
    }

    #[test]
    fn fts_operators_in_a_question_are_treated_as_text() {
        // Unquoted, these would parse as FTS5 syntax and error or silently mean something else.
        let conn = seeded();
        for q in ["pricing OR vendor", "vendor NEAR renewal", "pricing - vendor", "vendor*"] {
            assert!(fts_search(&conn, q, 10).is_ok(), "must not be a syntax error: {q}");
        }
        // A quote in the question must not break out of the quoting.
        assert!(fts_search(&conn, "he said \"pricing\" twice", 10).is_ok());
    }

    #[test]
    fn terms_too_short_for_the_trigram_index_are_dropped() {
        // "is"/"it" cannot match a trigram index; only the usable term survives.
        assert_eq!(fts_query("is it pricing"), Some("\"pricing\"".to_string()));
        // Function words carry no signal and would match nearly every document.
        assert_eq!(fts_query("what was the pricing"), Some("\"pricing\"".to_string()));
        // Nothing usable at all → no query, rather than an empty MATCH.
        assert_eq!(fts_query("is it"), None);
        assert_eq!(fts_query("what was that"), None, "all function words → nothing to retrieve on");
        assert_eq!(fts_query("   "), None);
    }

    #[test]
    fn cjk_runs_are_expanded_into_trigrams() {
        // No spaces to split on, so the run becomes its overlapping trigrams.
        assert_eq!(fts_query("資料の期限"), Some("\"資料の\" OR \"料の期\" OR \"の期限\"".to_string()));
    }

    #[test]
    fn a_pathological_query_is_capped() {
        let huge = (0..500).map(|i| format!("term{i}")).collect::<Vec<_>>().join(" ");
        let expr = fts_query(&huge).unwrap();
        assert_eq!(expr.matches(" OR ").count(), MAX_FTS_TERMS - 1, "term count is bounded");
    }
}
