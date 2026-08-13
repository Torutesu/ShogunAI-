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
pub(crate) fn fts_query(query: &str) -> Option<String> {
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

/// Exact local calendar boundaries supplied by the OS-facing caller.
///
/// Two boundaries are required because a local day can be 23 or 25 hours across DST changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDayBounds {
    pub yesterday_start_ms: i64,
    pub today_start_ms: i64,
}

fn query_has_word(query: &str, word: &str) -> bool {
    query
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| part == word)
}

/// Local day window implied by the question (`today`, `yesterday`, …). Returns `(from_ms, to_ms)`.
pub fn query_time_window(
    query: &str,
    now_ms: i64,
    local_days: LocalDayBounds,
) -> Option<(i64, i64)> {
    let q = query.to_ascii_lowercase();
    if q.contains("yesterday") {
        return Some((local_days.yesterday_start_ms, local_days.today_start_ms));
    }
    if q.contains("today") || q.contains("this morning") || q.contains("earlier today") {
        return Some((local_days.today_start_ms, now_ms));
    }
    None
}

/// True when the agent should pull stored screen frames (visual recall path).
pub fn query_wants_visual_recall(
    query: &str,
    now_ms: i64,
    local_days: LocalDayBounds,
) -> bool {
    if query_asks_about_screen(query) {
        return true;
    }
    query_time_window(query, now_ms, local_days).is_some_and(|_| {
        let q = query.to_ascii_lowercase();
        ["screen", "window", "see", "look", "show", "display", "app"]
            .iter()
            .any(|word| query_has_word(&q, word))
    })
}

/// Default time window for visual-recall frame search.
pub fn visual_recall_window(
    query: &str,
    now_ms: i64,
    local_days: LocalDayBounds,
) -> (i64, i64) {
    if let Some(win) = query_time_window(query, now_ms, local_days) {
        return win;
    }
    (now_ms - crate::screen_frames::RETENTION_MS, now_ms)
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

/// The Cold cutoff: events older than `now - COLD_CUTOFF_MS` have had their Warm f32 embedding
/// demoted to the int8 Cold archive ([`crate::cold`]), so a semantic hit on them can only come
/// from the Cold partition scan. Identical to [`WARM_WINDOW_MS`] by design — demotion and the
/// search boundary must agree or a band of events becomes semantically unreachable.
pub const COLD_CUTOFF_MS: i64 = WARM_WINDOW_MS;

/// Default cap on how many Cold partitions one query scans, newest first (design §2.1, E-09).
///
/// A partition is one [`crate::cold::PARTITION_MS`] period (30 days), so 6 partitions ≈ 7 months
/// of history counting the Warm window. The scan is a brute-force int8 dot product over every row
/// in each partition — bounding the partition count is what keeps the deep path's cost linear in
/// *recent* archive size instead of total history. Callers wanting the full archive pass a larger
/// cap explicitly (CLI/REST/MCP `depth: all` plumbing); the Warm default path never runs this scan.
pub const DEFAULT_MAX_COLD_PARTITIONS: usize = 6;

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

/// How deep the semantic half of hybrid search reaches (design §2.1, E-09).
///
/// `WarmOnly` is the default and matches every pre-existing call path: the vector list comes from
/// the Warm sqlite-vec KNN alone, and the Cold archive is never opened. `All` additionally runs
/// the Cold int8 partition scan ([`search_cold_partitions`]) and fuses its hits as a third RRF
/// source. FTS is unaffected either way — event text stays in `event_log`/FTS after demotion, so
/// the lexical half already spans Cold and must not be re-run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchDepth {
    /// Semantic search over the Warm window only (the NFR-SLO-04 fast path).
    #[default]
    WarmOnly,
    /// Also scan the Cold int8 archive, up to the configured partition cap.
    All,
}

/// Options for [`search_hybrid_with_options`]. `Default` reproduces [`search_hybrid`] exactly
/// (unbounded FTS, Warm-only semantics, no Cold scan).
#[derive(Debug, Clone, Copy)]
pub struct SearchOptions {
    /// Lexical floor, as in [`fts_search_since`]. `None` = unbounded.
    pub since_ts: Option<i64>,
    /// Semantic reach. See [`SearchDepth`].
    pub depth: SearchDepth,
    /// Cap on Cold partitions scanned (newest first) when the Cold scan runs.
    /// See [`DEFAULT_MAX_COLD_PARTITIONS`].
    pub max_cold_partitions: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            since_ts: None,
            depth: SearchDepth::WarmOnly,
            max_cold_partitions: DEFAULT_MAX_COLD_PARTITIONS,
        }
    }
}

/// What one Cold scan actually did — the honest-measurement counterpart of the partition cap.
/// `partitions_visited == 0` is the proof a Warm-only query never opened the archive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ColdScanStats {
    /// Non-empty partitions the scan iterated (bounded by the cap).
    pub partitions_visited: usize,
    /// Cold rows whose int8 dot product was computed.
    pub rows_scanned: usize,
}

/// Ranked outcome of a Cold partition scan: event ids best-first, plus scan stats.
#[derive(Debug, Clone, Default)]
pub struct ColdScan {
    /// Event ids ordered by descending dequantized dot product (ties: ascending id).
    pub ids: Vec<i64>,
    pub stats: ColdScanStats,
}

/// One Cold candidate held in the top-k heap. Ordering is total and deterministic:
/// higher score wins; equal scores prefer the smaller event id (matching the RRF tie-break).
struct ColdHit {
    score: f32,
    event_id: i64,
}

impl Ord for ColdHit {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // total_cmp gives a total order over f32 (scores are finite here: bounded int8 codes,
        // finite scale and query components). Larger event_id compares *less* so that among
        // equal scores the smaller id survives eviction and ranks first.
        self.score.total_cmp(&other.score).then_with(|| other.event_id.cmp(&self.event_id))
    }
}

impl PartialOrd for ColdHit {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ColdHit {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for ColdHit {}

/// Semantic search over the Cold int8 archive (design §2.1, E-09): a bounded brute-force scan of
/// the period partitions intersecting `range = (from_ms, to_ms)`, newest partition first, at most
/// `max_partitions` of them, keeping a global top-`k` by dot product.
///
/// Each row is scored without dequantizing into an allocation: the stored int8 codes are read as
/// a borrowed blob and dotted against the f32 query on the fly (`code as f32 * q`), then corrected
/// by the row's per-vector scale. Warm embeddings are L2-normalized before quantization, so this
/// dot product ranks like cosine similarity. Rows whose dimension does not match the query (e.g.
/// archived under an older embedding model) are skipped rather than mis-scored.
///
/// This is the *deep* path only — never called from the Warm default path (FR-MEM-03: routine
/// vector search stays on Warm). Cost is `rows_in_visited_partitions × dim` multiplies; the
/// partition cap keeps that bounded regardless of total history size.
pub fn search_cold_partitions(
    conn: &Connection,
    query_vec: &[f32],
    range: (i64, i64),
    max_partitions: usize,
    k: usize,
) -> Result<ColdScan, rusqlite::Error> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let (from_ms, to_ms) = range;
    let mut stats = ColdScanStats::default();
    if query_vec.is_empty() || k == 0 || max_partitions == 0 || from_ms > to_ms {
        return Ok(ColdScan { ids: Vec::new(), stats });
    }

    // Only partitions that actually hold rows count against the cap — an empty month costs nothing
    // and must not shadow older populated ones.
    let partitions: Vec<i64> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT partition FROM cold_embeddings
             WHERE partition BETWEEN ?1 AND ?2
             ORDER BY partition DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            params![
                crate::cold::partition_of(from_ms),
                crate::cold::partition_of(to_ms),
                max_partitions as i64
            ],
            |r| r.get::<_, i64>(0),
        )?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    // Min-heap of size ≤ k: the weakest of the current top-k sits on top and is evicted first.
    let mut heap: BinaryHeap<Reverse<ColdHit>> = BinaryHeap::with_capacity(k + 1);
    let mut stmt =
        conn.prepare("SELECT event_id, scale, codes FROM cold_embeddings WHERE partition = ?1")?;
    for p in partitions {
        stats.partitions_visited += 1;
        let mut rows = stmt.query(params![p])?;
        while let Some(row) = rows.next()? {
            let event_id: i64 = row.get(0)?;
            let scale: f64 = row.get(1)?;
            let codes: &[u8] = row.get_ref(2)?.as_blob()?;
            stats.rows_scanned += 1;
            if codes.len() != query_vec.len() {
                continue; // archived under a different embedding dim — cannot be scored
            }
            let mut dot = 0.0f32;
            for (b, q) in codes.iter().zip(query_vec) {
                dot += (*b as i8) as f32 * q;
            }
            heap.push(Reverse(ColdHit { score: dot * scale as f32, event_id }));
            if heap.len() > k {
                heap.pop();
            }
        }
    }

    let mut hits: Vec<ColdHit> = heap.into_iter().map(|r| r.0).collect();
    hits.sort_by(|a, b| b.cmp(a)); // best-first, deterministic (total order incl. id tie-break)
    Ok(ColdScan { ids: hits.into_iter().map(|h| h.event_id).collect(), stats })
}

/// Hybrid result plus the Cold-scan measurement (sub-SLO accounting per design §2.1: the deep
/// path is measured separately from the Warm 500ms budget, never silently folded into it).
#[derive(Debug, Clone)]
pub struct DeepSearchResult {
    pub hits: Vec<SearchHit>,
    /// All-zero when the Cold archive was not opened.
    pub cold: ColdScanStats,
}

/// [`search_hybrid_since`] with explicit reach (design §2.1, E-09). The Cold int8 archive is
/// scanned and fused as a **third RRF source** when either
///
/// (a) `opts.depth == SearchDepth::All`, or
/// (b) the caller's explicit time range reaches past the Cold cutoff
///     (`opts.since_ts` set and older than `now_ms - COLD_CUTOFF_MS`) — asking for a window that
///     old *is* asking for Cold, and answering from Warm alone would be silently wrong.
///
/// Default options ([`SearchOptions::default`]) reproduce [`search_hybrid`] exactly: Warm-only,
/// archive untouched (`result.cold == ColdScanStats::default()`). FTS is never duplicated — the
/// lexical half already covers Cold text (demotion removes only the Warm vector), so the Cold
/// scan contributes semantics only. `now_ms` is passed in, not read, for deterministic tests.
pub fn search_hybrid_with_options(
    conn: &Connection,
    query: &str,
    query_embedding: Option<&[f32]>,
    now_ms: i64,
    opts: &SearchOptions,
    limit: usize,
) -> Result<DeepSearchResult, rusqlite::Error> {
    let fts = fts_search_since(conn, query, opts.since_ts, limit)?;
    let vec = match query_embedding {
        Some(emb) => crate::vector::knn(conn, emb, limit)?,
        None => Vec::new(),
    };
    let reach_cold = opts.depth == SearchDepth::All
        || opts.since_ts.is_some_and(|s| s < now_ms - COLD_CUTOFF_MS);
    let cold = match (reach_cold, query_embedding) {
        (true, Some(emb)) => search_cold_partitions(
            conn,
            emb,
            (opts.since_ts.unwrap_or(i64::MIN), now_ms),
            opts.max_cold_partitions,
            limit,
        )?,
        _ => ColdScan::default(),
    };
    let ranked = reciprocal_rank_fusion(&[&fts, &vec, &cold.ids], 60.0);
    let capped: Vec<(i64, f64)> = ranked.into_iter().take(limit).collect();
    Ok(DeepSearchResult { hits: hydrate(conn, &capped)?, cold: cold.stats })
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
        let days = LocalDayBounds {
            yesterday_start_ms: 0,
            today_start_ms: 86_400_000,
        };
        assert!(query_asks_about_screen("what was on my screen yesterday"));
        assert!(!query_asks_about_screen("vendor pricing email"));
        assert!(query_wants_visual_recall("what was on my screen yesterday", 0, days));
        assert!(query_wants_visual_recall(
            "what did I see on screen today",
            86_400_000,
            days
        ));
        assert!(!query_wants_visual_recall("what happened today", 86_400_000, days));
        let now = 86_400_000 * 2;
        let Some((from, to)) = query_time_window("yesterday", now, days) else {
            panic!("expected window");
        };
        assert_eq!(from, 0);
        assert_eq!(to, 86_400_000);
    }

    #[test]
    fn query_time_window_uses_exact_local_midnights() {
        // 2020-01-02 01:00 UTC = 2020-01-02 10:00 JST (+9h)
        let now = 1_577_894_400_000_i64;
        let days = LocalDayBounds {
            yesterday_start_ms: 1_577_804_400_000,
            today_start_ms: 1_577_890_800_000,
        };
        let Some((from, to)) = query_time_window("today", now, days) else {
            panic!("expected window");
        };
        assert_eq!(from, days.today_start_ms);
        assert_eq!(to, now);
        let Some((from, to)) = query_time_window("yesterday", now, days) else {
            panic!("expected window");
        };
        assert_eq!((from, to), (days.yesterday_start_ms, days.today_start_ms));
    }

    #[test]
    fn yesterday_window_can_span_a_dst_transition() {
        let days = LocalDayBounds {
            yesterday_start_ms: 1_000,
            today_start_ms: 1_000 + 23 * 60 * 60 * 1_000,
        };
        assert_eq!(
            query_time_window("yesterday", days.today_start_ms + 1, days),
            Some((days.yesterday_start_ms, days.today_start_ms))
        );
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

/// Cold-tier semantic search (design §2.1, E-09): the archive must be reachable on request and
/// untouchable by default.
#[cfg(test)]
mod cold_search_tests {
    use super::*;
    use crate::cold::{self, PARTITION_MS};
    use crate::embed::{Embedder, MockEmbedder, E5_SMALL_DIM};
    use crate::event_log::{insert, NewEvent};

    const DAY: i64 = 24 * 60 * 60 * 1000;

    fn add(conn: &Connection, content: &str, hash: &str, ts: i64) -> i64 {
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
        .unwrap()
    }

    fn embed(conn: &Connection, m: &MockEmbedder, id: i64, text: &str) {
        let v = m.embed_passages(&[text]).unwrap()[0].clone();
        crate::vector::upsert(conn, id, &v).unwrap();
    }

    /// One event well past the cutoff, embedded and demoted through the real demotion path, plus
    /// a fresh Warm event. The old content shares no token with the query "standup", so lexical
    /// search alone cannot surface it — only its embedding can.
    fn seeded_across_cutoff() -> (Connection, i64, i64) {
        let mut conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let now = 100 * PARTITION_MS;

        let old_text = "the budget review meeting is on friday";
        let old_id = add(&conn, old_text, "h_old", now - 80 * DAY);
        embed(&conn, &m, old_id, old_text);

        let fresh_text = "lunch plans for saturday afternoon";
        let fresh_id = add(&conn, fresh_text, "h_fresh", now - DAY);
        embed(&conn, &m, fresh_id, fresh_text);

        let moved = cold::demote_older_than(&mut conn, now - COLD_CUTOFF_MS).unwrap();
        assert_eq!(moved, 1, "precondition: the old embedding is demoted to Cold");
        assert_eq!(cold::count(&conn).unwrap(), 1);
        (conn, now, old_id)
    }

    #[test]
    fn warm_only_never_touches_cold() {
        let (conn, now, old_id) = seeded_across_cutoff();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let q = m.embed_query("review meeting standup").unwrap();

        // Default options (WarmOnly, no explicit floor) — the archive stays closed.
        let res = search_hybrid_with_options(
            &conn,
            "standup",
            Some(&q),
            now,
            &SearchOptions::default(),
            10,
        )
        .unwrap();
        assert_eq!(res.cold, ColdScanStats::default(), "no partition opened, no row scanned");
        assert!(
            res.hits.iter().all(|h| h.event_id != old_id),
            "the demoted event must not surface on the Warm path: {:?}",
            res.hits
        );

        // Same with an explicit floor inside the Warm window.
        let opts = SearchOptions { since_ts: Some(now - 7 * DAY), ..Default::default() };
        let res = search_hybrid_with_options(&conn, "standup", Some(&q), now, &opts, 10).unwrap();
        assert_eq!(res.cold, ColdScanStats::default());

        // And default options reproduce the pre-existing entry point exactly.
        let legacy = search_hybrid(&conn, "standup", Some(&q), 10).unwrap();
        let via_opts =
            search_hybrid_with_options(&conn, "standup", Some(&q), now, &SearchOptions::default(), 10)
                .unwrap();
        assert_eq!(legacy, via_opts.hits, "default options must not change existing behavior");
    }

    #[test]
    fn depth_all_finds_an_old_semantic_match_lexical_search_misses() {
        let (conn, now, old_id) = seeded_across_cutoff();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let q = m.embed_query("review meeting standup").unwrap();

        // Lexical alone: "standup" appears nowhere, so even unbounded FTS returns nothing.
        assert!(fts_search(&conn, "standup", 10).unwrap().is_empty(), "precondition: FTS miss");

        let opts = SearchOptions { depth: SearchDepth::All, ..Default::default() };
        let res = search_hybrid_with_options(&conn, "standup", Some(&q), now, &opts, 10).unwrap();
        assert!(res.cold.partitions_visited >= 1, "the archive was actually opened");
        assert!(res.cold.rows_scanned >= 1);
        assert!(
            res.hits.iter().any(|h| h.event_id == old_id),
            "the >30-day-old semantic match must surface via the Cold RRF source: {:?}",
            res.hits
        );
    }

    #[test]
    fn an_explicit_range_past_the_cutoff_reaches_cold_without_depth_all() {
        let (conn, now, old_id) = seeded_across_cutoff();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let q = m.embed_query("review meeting standup").unwrap();

        // depth stays WarmOnly, but the asked-for window plainly includes Cold territory.
        let opts = SearchOptions { since_ts: Some(now - 90 * DAY), ..Default::default() };
        let res = search_hybrid_with_options(&conn, "standup", Some(&q), now, &opts, 10).unwrap();
        assert!(res.cold.partitions_visited >= 1, "an explicitly old range implies Cold");
        assert!(res.hits.iter().any(|h| h.event_id == old_id));
    }

    #[test]
    fn partition_cap_limits_scanning_newest_first() {
        let mut conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        // Five populated partitions, one event each, at periods 10..14.
        let mut ids = Vec::new();
        for i in 0..5i64 {
            let ts = (10 + i) * PARTITION_MS + 5;
            let text = format!("archived note number {i}");
            let id = add(&conn, &text, &format!("h{i}"), ts);
            embed(&conn, &m, id, &text);
            assert!(cold::demote(&mut conn, id, ts).unwrap());
            ids.push(id);
        }
        let q = m.embed_query("archived note").unwrap();
        let range = (0, 20 * PARTITION_MS);

        let started = std::time::Instant::now();
        let capped = search_cold_partitions(&conn, &q, range, 2, 10).unwrap();
        println!(
            "cold scan: {} partitions / {} rows in {:?}",
            capped.stats.partitions_visited,
            capped.stats.rows_scanned,
            started.elapsed()
        );
        assert_eq!(capped.stats.partitions_visited, 2, "cap bounds the scan");
        assert_eq!(capped.stats.rows_scanned, 2);
        // Newest partitions win the cap slots: only the two most recent events are reachable.
        assert_eq!(capped.ids.len(), 2);
        assert!(capped.ids.contains(&ids[4]) && capped.ids.contains(&ids[3]), "{:?}", capped.ids);

        // Uncapped (default cap ≥ 5): every populated partition is visited.
        let full =
            search_cold_partitions(&conn, &q, range, DEFAULT_MAX_COLD_PARTITIONS, 10).unwrap();
        assert_eq!(full.stats.partitions_visited, 5);
        assert_eq!(full.stats.rows_scanned, 5);
        assert_eq!(full.ids.len(), 5);

        // The time range prunes partitions before the cap does.
        let narrow =
            search_cold_partitions(&conn, &q, (13 * PARTITION_MS, 20 * PARTITION_MS), 6, 10)
                .unwrap();
        assert_eq!(narrow.stats.partitions_visited, 2, "only periods 13 and 14 intersect");
    }

    #[test]
    fn cold_scan_ranks_by_similarity_and_breaks_ties_deterministically() {
        let mut conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let ts = 10 * PARTITION_MS + 5;

        // Two identical contents (identical vectors → identical scores) and one distant decoy.
        let a = add(&conn, "vendor contract renewal", "ha", ts);
        let b = add(&conn, "vendor contract renewal", "hb", ts + 1);
        let decoy = add(&conn, "weekend hiking photos", "hc", ts + 2);
        for (id, text) in
            [(a, "vendor contract renewal"), (b, "vendor contract renewal"), (decoy, "weekend hiking photos")]
        {
            embed(&conn, &m, id, text);
            assert!(cold::demote(&mut conn, id, ts).unwrap());
        }
        let q = m.embed_query("vendor contract renewal").unwrap();
        let range = (0, 20 * PARTITION_MS);

        let scan = search_cold_partitions(&conn, &q, range, 6, 3).unwrap();
        assert_eq!(scan.ids, vec![a, b, decoy], "score order, ties by ascending id");

        // k=1 under a tie must deterministically keep the smaller id.
        let top1 = search_cold_partitions(&conn, &q, range, 6, 1).unwrap();
        assert_eq!(top1.ids, vec![a]);
    }

    #[test]
    fn rrf_merge_with_cold_source_is_stable_and_deterministic() {
        let (conn, now, _old_id) = seeded_across_cutoff();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        // A query that exercises all three sources: "friday" hits FTS (old event text is still
        // indexed), the embedding hits Warm KNN and the Cold scan.
        let q = m.embed_query("budget review friday").unwrap();
        let opts = SearchOptions { depth: SearchDepth::All, ..Default::default() };

        let first =
            search_hybrid_with_options(&conn, "budget friday", Some(&q), now, &opts, 10).unwrap();
        let second =
            search_hybrid_with_options(&conn, "budget friday", Some(&q), now, &opts, 10).unwrap();
        assert!(!first.hits.is_empty());
        assert_eq!(first.hits, second.hits, "same inputs, same fused ranking");
        assert_eq!(first.cold, second.cold);
        // Scores are strictly ordered best-first (RRF output is sorted and hydration preserves it).
        assert!(first.hits.windows(2).all(|w| w[0].score >= w[1].score));
    }

    #[test]
    fn cold_scan_degenerate_inputs_return_empty() {
        let conn = crate::open_in_memory().unwrap();
        let q = vec![0.5f32; E5_SMALL_DIM];
        // Empty archive, zero cap, zero k, inverted range — all empty, none error.
        assert!(search_cold_partitions(&conn, &q, (0, i64::MAX), 6, 10).unwrap().ids.is_empty());
        assert!(search_cold_partitions(&conn, &q, (0, 100), 0, 10).unwrap().ids.is_empty());
        assert!(search_cold_partitions(&conn, &q, (0, 100), 6, 0).unwrap().ids.is_empty());
        assert!(search_cold_partitions(&conn, &q, (100, 0), 6, 10).unwrap().ids.is_empty());
        assert!(search_cold_partitions(&conn, &[], (0, 100), 6, 10).unwrap().ids.is_empty());
    }
}

