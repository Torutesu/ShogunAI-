use rusqlite::{params, Connection};

use super::model::SearchHit;
use super::query::{fts_query, lexical_terms};
use super::ranking::reciprocal_rank_fusion;

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
pub fn fts_search(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<i64>, rusqlite::Error> {
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
pub fn hydrate(
    conn: &Connection,
    ranked: &[(i64, f64)],
) -> Result<Vec<SearchHit>, rusqlite::Error> {
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
pub fn search(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, rusqlite::Error> {
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
        self.score
            .total_cmp(&other.score)
            .then_with(|| other.event_id.cmp(&self.event_id))
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
        return Ok(ColdScan {
            ids: Vec::new(),
            stats,
        });
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
            heap.push(Reverse(ColdHit {
                score: dot * scale as f32,
                event_id,
            }));
            if heap.len() > k {
                heap.pop();
            }
        }
    }

    let mut hits: Vec<ColdHit> = heap.into_iter().map(|r| r.0).collect();
    hits.sort_by(|a, b| b.cmp(a)); // best-first, deterministic (total order incl. id tie-break)
    Ok(ColdScan {
        ids: hits.into_iter().map(|h| h.event_id).collect(),
        stats,
    })
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
    Ok(DeepSearchResult {
        hits: hydrate(conn, &capped)?,
        cold: cold.stats,
    })
}
