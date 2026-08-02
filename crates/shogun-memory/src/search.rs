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
/// Extract searchable terms from a user's question (same rules as [`fts_query`]).
///
/// Shared by event FTS and meeting-table retrieval so both halves of hybrid search speak the
/// same vocabulary.
pub fn lexical_terms(query: &str) -> Vec<String> {
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
                terms.push(w.iter().collect::<String>());
            }
        } else if !is_stopword(raw) {
            terms.push(raw.to_string());
        }
    }
    terms
}

/// Returns `None` when nothing usable is left, which the caller treats as "no results" rather than
/// running an empty MATCH.
fn fts_query(query: &str) -> Option<String> {
    let terms = lexical_terms(query);
    if terms.is_empty() {
        // A question made only of function words has nothing to retrieve on. Returning None is
        // honest: matching every document via "the" would fill the result budget with noise and
        // push out anything real.
        return None;
    }
    Some(terms.into_iter().map(|t| quote(&t)).collect::<Vec<_>>().join(" OR "))
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

/// Human label for evidence citations (FR-MEM-23). Raw `source` tags stay in the DB.
pub fn evidence_source_label(source: &str) -> String {
    match source {
        "screen_ocr" => "screen text".to_string(),
        "capture" => "window".to_string(),
        "meeting" => "meeting".to_string(),
        "gmail" => "mail".to_string(),
        "gcal" => "calendar".to_string(),
        "slack" => "chat".to_string(),
        "notion" => "doc".to_string(),
        "github" => "code".to_string(),
        "linear" => "issue".to_string(),
        other => other.to_string(),
    }
}

/// True when the question is likely about on-screen content (visual recall path).
pub fn query_asks_about_screen(query: &str) -> bool {
    let q = query.to_ascii_lowercase();
    [
        "on my screen",
        "on screen",
        "what was on",
        "what did i see",
        "shown on",
        "displayed on",
        "looking at",
        "on my display",
    ]
    .iter()
    .any(|p| q.contains(p))
}

/// FTS over one `event_log.source` tag (e.g. `screen_ocr` for visual recall).
pub fn fts_search_source(
    conn: &Connection,
    query: &str,
    source: &str,
    limit: usize,
) -> Result<Vec<i64>, rusqlite::Error> {
    let Some(expr) = fts_query(query) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(
        "SELECT f.rowid FROM event_fts f
         INNER JOIN event_log e ON e.id = f.rowid
         WHERE event_fts MATCH ?1 AND e.source = ?2
         ORDER BY bm25(event_fts) LIMIT ?3",
    )?;
    let ids = stmt
        .query_map(params![expr, source, limit as i64], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// Full-text search over the event log, best-match first, capped at `limit`. Returns event
/// ids ordered by bm25 relevance (SQLite's bm25 is more-negative-is-better, so ascending).
pub fn fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<i64>, rusqlite::Error> {
    fts_search_since(conn, query, None, limit)
}

/// [`fts_search`], optionally restricted to events at or after `since_ts`.
///
/// **Why the bound exists.** `ORDER BY bm25(...)` has to score every matching row before it can
/// take the top `limit`, so the cost tracks how much of the log the query matches, not how much
/// is returned. Measured on an Apple Silicon device, an unbounded search over 40k events reached
/// 506ms against a 500ms budget — already over, and it grows with the log.
///
/// Restricting to a recent window is the 3-tier memory design (Warm is what ordinary search
/// targets), not a shortcut invented for latency: a question is nearly always about recent work.
/// The caller escalates to the full history when a bounded search comes back thin, so nothing
/// older becomes unreachable — see [`crate::search::WARM_WINDOW_MS`].
pub fn fts_search_since(
    conn: &Connection,
    query: &str,
    since_ts: Option<i64>,
    limit: usize,
) -> Result<Vec<i64>, rusqlite::Error> {
    let Some(expr) = fts_query(query) else {
        return Ok(Vec::new());
    };
    match since_ts {
        Some(since) => {
            // Translate the time floor into a docid floor. Joining event_log to filter on `ts`
            // does NOT help: SQLite resolves the MATCH and scores every hit before the join can
            // discard anything, so the bm25 cost is unchanged (measured: no improvement at all).
            // A `rowid >=` constraint is different in kind — FTS5 postings are ordered by docid,
            // so it skips them instead of scoring them.
            //
            // Ids are assigned in insertion order, which tracks time closely but not exactly (a
            // connector backfill can insert an older item later). That makes this bound an
            // approximation, which is fine for a latency bound: anything it misses is recovered
            // by the escalation in `search_warm_first`.
            let min_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM event_log WHERE ts >= ?1 ORDER BY id ASC LIMIT 1",
                    params![since],
                    |r| r.get(0),
                )
                .ok();
            let Some(min_id) = min_id else {
                return Ok(Vec::new()); // nothing in the window at all
            };
            let mut stmt = conn.prepare(
                "SELECT rowid FROM event_fts
                  WHERE event_fts MATCH ?1 AND rowid >= ?2
                  ORDER BY bm25(event_fts) LIMIT ?3",
            )?;
            let ids = stmt
                .query_map(params![expr, min_id, limit as i64], |r| r.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ids)
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT rowid FROM event_fts WHERE event_fts MATCH ?1
                  ORDER BY bm25(event_fts) LIMIT ?2",
            )?;
            let ids = stmt
                .query_map(params![expr, limit as i64], |r| r.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ids)
        }
    }
}

/// The Warm window ordinary search covers (§ 3-tier memory: Hot 24h / Warm 30d / Cold all).
pub const WARM_WINDOW_MS: i64 = 30 * 24 * 60 * 60 * 1000;

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

/// A meeting interval surfaced by query-relevant retrieval over `meeting_recaps` and
/// `transcript_segments` (FR-MT-22 chat grounding). `session_id` is the provenance key.
#[derive(Debug, Clone, PartialEq)]
pub struct MeetingSearchHit {
    pub session_id: i64,
    pub ts: i64,
    pub title: Option<String>,
    pub content: String,
    pub score: f64,
}

/// Lexical search over meeting minutes and transcripts. Query-relevant — not "latest session".
///
/// Scores each meeting interval by how many query terms hit its recap fields and transcript lines.
/// Returns best-first, capped at `limit`. Empty when the query has no usable terms or nothing
/// matches.
pub fn search_meetings(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<MeetingSearchHit>, rusqlite::Error> {
    let terms = lexical_terms(query);
    if terms.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    use std::collections::HashMap;

    struct Candidate {
        ts: i64,
        title: Option<String>,
        summary: Option<String>,
        decisions: Option<String>,
        next_actions: Option<String>,
        transcript: Vec<String>,
        score: f64,
    }

    let mut candidates: HashMap<i64, Candidate> = HashMap::new();

    let mut recap_stmt = conn.prepare(
        "SELECT mr.session_id, s.started_at, s.title, mr.summary, mr.decisions, mr.next_actions
         FROM meeting_recaps mr
         JOIN sessions s ON s.id = mr.session_id
         WHERE s.kind IN ('meeting', 'call')",
    )?;
    let recap_rows = recap_stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
        ))
    })?;
    for row in recap_rows {
        let (sid, ts, title, summary, decisions, next_actions) = row?;
        let blob = format!(
            "{} {} {} {}",
            title.as_deref().unwrap_or(""),
            summary,
            decisions,
            next_actions
        );
        let score = score_terms(&blob, &terms);
        if score <= 0.0 {
            continue;
        }
        candidates.insert(
            sid,
            Candidate {
                ts,
                title,
                summary: Some(summary),
                decisions: Some(decisions),
                next_actions: Some(next_actions),
                transcript: Vec::new(),
                score,
            },
        );
    }

    let mut tx_stmt = conn.prepare(
        "SELECT ts.session_id, s.started_at, s.title, ts.text
         FROM transcript_segments ts
         JOIN sessions s ON s.id = ts.session_id
         WHERE s.kind IN ('meeting', 'call')",
    )?;
    let tx_rows = tx_stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    for row in tx_rows {
        let (sid, ts, title, text) = row?;
        let line_score = score_terms(&text, &terms);
        if line_score <= 0.0 {
            continue;
        }
        candidates
            .entry(sid)
            .and_modify(|c| {
                c.score += line_score * 0.5;
                c.transcript.push(text.clone());
            })
            .or_insert_with(|| Candidate {
                ts,
                title,
                summary: None,
                decisions: None,
                next_actions: None,
                transcript: vec![text],
                score: line_score * 0.5,
            });
    }

    let mut hits: Vec<MeetingSearchHit> = candidates
        .into_iter()
        .map(|(session_id, c)| {
            let content = format_meeting_content(
                c.title.as_deref(),
                c.summary.as_deref(),
                c.decisions.as_deref(),
                c.next_actions.as_deref(),
                &c.transcript,
            );
            MeetingSearchHit {
                session_id,
                ts: c.ts,
                title: c.title,
                content,
                score: c.score,
            }
        })
        .filter(|h| !h.content.is_empty())
        .collect();

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.ts.cmp(&a.ts))
    });
    hits.truncate(limit);
    Ok(hits)
}

fn score_terms(hay: &str, terms: &[String]) -> f64 {
    let lower = hay.to_ascii_lowercase();
    terms
        .iter()
        .filter(|t| lower.contains(&t.to_ascii_lowercase()))
        .count() as f64
}

/// Assemble the searchable text for one meeting interval. Transcript lines are capped so a long
/// call cannot dominate the prompt budget before `excerpt` runs.
fn format_meeting_content(
    title: Option<&str>,
    summary: Option<&str>,
    decisions: Option<&str>,
    next_actions: Option<&str>,
    transcript: &[String],
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        parts.push(format!("Meeting: {t}"));
    }
    if let Some(s) = summary.filter(|s| !s.is_empty()) {
        parts.push(format!("Summary: {s}"));
    }
    if let Some(d) = decisions.filter(|d| !d.is_empty() && *d != "[]") {
        parts.push(format!("Decisions: {d}"));
    }
    if let Some(n) = next_actions.filter(|n| !n.is_empty() && *n != "[]") {
        parts.push(format!("Next actions: {n}"));
    }
    if !transcript.is_empty() {
        let joined = transcript.join(" ");
        const TX_CAP: usize = 4_000;
        if joined.chars().count() > TX_CAP {
            let short: String = joined.chars().take(TX_CAP).collect();
            parts.push(format!("Transcript (excerpt): {short}…"));
        } else {
            parts.push(format!("Transcript: {joined}"));
        }
    }
    parts.join("\n")
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
    search_hybrid_since(conn, query, query_embedding, None, limit)
}

/// [`search_hybrid`] with the lexical half restricted to `since_ts` — see [`fts_search_since`].
pub fn search_hybrid_since(
    conn: &Connection,
    query: &str,
    query_embedding: Option<&[f32]>,
    since_ts: Option<i64>,
    limit: usize,
) -> Result<Vec<SearchHit>, rusqlite::Error> {
    let fts = fts_search_since(conn, query, since_ts, limit)?;
    let vec = match query_embedding {
        Some(emb) => crate::vector::knn(conn, emb, limit)?,
        None => Vec::new(),
    };
    let ranked = reciprocal_rank_fusion(&[&fts, &vec], 60.0);
    let capped: Vec<(i64, f64)> = ranked.into_iter().take(limit).collect();
    hydrate(conn, &capped)
}

/// Search the Warm window first and widen to the whole history only if that comes back thin.
///
/// This is what keeps the recency bound from silently losing answers: a question about something
/// from months ago still gets answered, it just costs the slower path. `now_ms` is passed in
/// rather than read so the behaviour stays deterministic in tests.
pub fn search_warm_first(
    conn: &Connection,
    query: &str,
    query_embedding: Option<&[f32]>,
    now_ms: i64,
    limit: usize,
) -> Result<Vec<SearchHit>, rusqlite::Error> {
    let warm = search_hybrid_since(
        conn,
        query,
        query_embedding,
        Some(now_ms - WARM_WINDOW_MS),
        limit,
    )?;
    // "Thin" means the window plainly did not hold the answer. Half the asked-for results is a
    // deliberately low bar: widening is the expensive path and should be the exception.
    if warm.len() * 2 >= limit || warm.len() >= limit {
        return Ok(warm);
    }
    search_hybrid_since(conn, query, query_embedding, None, limit)
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
    fn search_meetings_finds_recap_by_query_term() {
        use crate::meeting_recaps;
        use crate::session::{open, NewSession};

        let conn = crate::open_in_memory().unwrap();
        let sid = open(
            &conn,
            &NewSession {
                kind: "meeting",
                started_at: 5_000,
                title: Some("Vendor pricing sync"),
                app_bundle_id: Some("us.zoom.xos"),
                calendar_occurrence_id: None,
                confidence: 0.8,
                provenance: "{}",
            },
        )
        .unwrap();
        meeting_recaps::save(
            &conn,
            sid,
            "Discussed renewal pricing and the 12k quote.",
            r#"["Approve the vendor renewal"]"#,
            r#"[{"text":"email procurement","owner":"Alice"}]"#,
            "claude-batch",
            6_000,
        )
        .unwrap();

        let hits = search_meetings(&conn, "vendor pricing", 5).unwrap();
        assert_eq!(hits.len(), 1, "recap matched by query: {hits:?}");
        assert!(hits[0].content.contains("12k"));
        assert_eq!(hits[0].title.as_deref(), Some("Vendor pricing sync"));
    }

    #[test]
    fn search_meetings_prefers_the_relevant_session_not_the_latest() {
        use crate::meeting_recaps;
        use crate::session::{open, NewSession};

        let conn = crate::open_in_memory().unwrap();
        let old = open(
            &conn,
            &NewSession {
                kind: "meeting",
                started_at: 1_000,
                title: Some("Design review"),
                app_bundle_id: None,
                calendar_occurrence_id: None,
                confidence: 0.8,
                provenance: "{}",
            },
        )
        .unwrap();
        let recent = open(
            &conn,
            &NewSession {
                kind: "meeting",
                started_at: 9_000,
                title: Some("Daily standup"),
                app_bundle_id: None,
                calendar_occurrence_id: None,
                confidence: 0.8,
                provenance: "{}",
            },
        )
        .unwrap();
        meeting_recaps::save(&conn, old, "Roadmap and launch timeline for Phoenix.", "[]", "[]", "m", 2_000)
            .unwrap();
        meeting_recaps::save(&conn, recent, "Nothing blocking today.", "[]", "[]", "m", 10_000).unwrap();

        let hits = search_meetings(&conn, "Phoenix launch", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, old, "older but relevant meeting wins over latest");
    }

    #[test]
    fn search_meetings_finds_transcript_when_recap_is_missing() {
        use crate::session::{open, NewSession};
        use crate::transcript_segments::{append, NewSegment, Speaker};

        let conn = crate::open_in_memory().unwrap();
        let sid = open(
            &conn,
            &NewSession {
                kind: "meeting",
                started_at: 3_000,
                title: Some("Budget call"),
                app_bundle_id: None,
                calendar_occurrence_id: None,
                confidence: 0.8,
                provenance: "{}",
            },
        )
        .unwrap();
        append(
            &conn,
            &NewSegment {
                session_id: sid,
                ts: 3_100,
                speaker: Speaker::Other,
                text: "We agreed to cap infrastructure spend at forty thousand.",
                confidence: 0.9,
            },
            3_200,
        )
        .unwrap();

        let hits = search_meetings(&conn, "infrastructure spend", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].content.contains("forty thousand"));
    }

    #[test]
    fn search_meetings_returns_nothing_for_unrelated_queries() {
        use crate::meeting_recaps;
        use crate::session::{open, NewSession};

        let conn = crate::open_in_memory().unwrap();
        let sid = open(
            &conn,
            &NewSession {
                kind: "meeting",
                started_at: 1_000,
                title: Some("Weekly sync"),
                app_bundle_id: None,
                calendar_occurrence_id: None,
                confidence: 0.8,
                provenance: "{}",
            },
        )
        .unwrap();
        meeting_recaps::save(&conn, sid, "Discussed hiring plans.", "[]", "[]", "m", 2_000).unwrap();

        assert!(search_meetings(&conn, "vendor migration", 5).unwrap().is_empty());
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

/// The recency bound must speed things up without making older memory unreachable.
#[cfg(test)]
mod warm_window_tests {
    use super::*;
    use crate::event_log::{insert, NewEvent};

    const DAY: i64 = 24 * 60 * 60 * 1000;

    fn add(conn: &Connection, content: &str, hash: &str, ts: i64) {
        insert(
            conn,
            &NewEvent {
                ts,
                source: "capture",
                kind: "text",
                app_bundle_id: Some("com.apple.Safari"),
                window_title: Some("notes"),
                content,
                content_hash: hash,
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn the_bound_excludes_what_is_older_than_the_window() {
        let conn = crate::open_in_memory().unwrap();
        let now = 100 * DAY;
        // Chronological insertion — how capture and sync actually write.
        add(&conn, "ancient vendor pricing note", "h2", now - 90 * DAY);
        add(&conn, "recent vendor pricing note", "h1", now - DAY);

        let bounded = fts_search_since(&conn, "vendor pricing", Some(now - WARM_WINDOW_MS), 10).unwrap();
        assert_eq!(bounded.len(), 1, "only the in-window row");
        let unbounded = fts_search_since(&conn, "vendor pricing", None, 10).unwrap();
        assert_eq!(unbounded.len(), 2, "the whole history is still reachable");
    }

    /// The docid bound approximates the time bound, and the approximation errs the safe way.
    ///
    /// A backfilled older item gets a higher id than rows that predate it, so it can fall inside
    /// a docid range its timestamp is outside of. That returns an *extra* old row — harmless, it
    /// is ranked and capped like any other. The dangerous direction, silently dropping something
    /// recent, cannot happen: everything newer has a higher id by construction.
    #[test]
    fn an_out_of_order_backfill_may_be_included_but_nothing_recent_is_lost() {
        let conn = crate::open_in_memory().unwrap();
        let now = 100 * DAY;
        add(&conn, "recent vendor pricing note", "h1", now - DAY);
        add(&conn, "backfilled old vendor pricing note", "h2", now - 90 * DAY);

        let bounded = fts_search_since(&conn, "vendor pricing", Some(now - WARM_WINDOW_MS), 10).unwrap();
        assert!(!bounded.is_empty(), "the recent row is never lost");
        let hits = hydrate(&conn, &bounded.iter().map(|id| (*id, 1.0)).collect::<Vec<_>>()).unwrap();
        assert!(
            hits.iter().any(|h| h.content.contains("recent")),
            "the in-window row must be present: {hits:?}"
        );
    }

    /// The bound must not turn "I asked about something old" into "SHOGUN has no idea".
    #[test]
    fn a_question_only_answered_by_old_memory_still_finds_it() {
        let conn = crate::open_in_memory().unwrap();
        let now = 100 * DAY;
        add(&conn, "the vendor migration was cancelled for downtime", "h1", now - 80 * DAY);

        // Warm alone finds nothing, so the search widens rather than answering "nothing found".
        let warm_only =
            search_hybrid_since(&conn, "vendor migration", None, Some(now - WARM_WINDOW_MS), 6).unwrap();
        assert!(warm_only.is_empty(), "precondition: outside the window");

        let escalated = search_warm_first(&conn, "vendor migration", None, now, 6).unwrap();
        assert_eq!(escalated.len(), 1, "escalation reaches the old answer: {escalated:?}");
        assert!(escalated[0].content.contains("cancelled"));
    }

    /// When the window does hold enough, the expensive full-history pass is not run.
    #[test]
    fn a_well_answered_question_stays_on_the_fast_path() {
        let conn = crate::open_in_memory().unwrap();
        let now = 100 * DAY;
        for i in 0..6 {
            add(&conn, "recent vendor pricing note", &format!("h{i}"), now - DAY - i);
        }
        add(&conn, "ancient vendor pricing note", "old", now - 90 * DAY);

        let hits = search_warm_first(&conn, "vendor pricing", None, now, 6).unwrap();
        assert_eq!(hits.len(), 6);
        assert!(
            hits.iter().all(|h| h.ts > now - WARM_WINDOW_MS),
            "the old row must not appear when the window sufficed"
        );
    }

    #[test]
    fn screen_query_heuristic_matches_natural_phrases() {
        assert!(query_asks_about_screen("what was on my screen yesterday"));
        assert!(!query_asks_about_screen("vendor pricing email"));
    }

    #[test]
    fn fts_search_source_scopes_to_one_tag() {
        let conn = crate::open_in_memory().unwrap();
        add(&conn, "quarterly roadmap slide text", "ocr1", 1_000);
        conn.execute(
            "UPDATE event_log SET source = 'screen_ocr' WHERE content_hash = 'ocr1'",
            [],
        )
        .unwrap();
        add(&conn, "quarterly roadmap from accessibility", "cap1", 1_100);

        let ocr_ids = fts_search_source(&conn, "roadmap", "screen_ocr", 5).unwrap();
        assert_eq!(ocr_ids.len(), 1);
        let cap_ids = fts_search_source(&conn, "roadmap", "capture", 5).unwrap();
        assert_eq!(cap_ids.len(), 1);
        assert_ne!(ocr_ids[0], cap_ids[0]);
    }
}
