//! Async embedding job — Warm-layer population (FR-MEM-22).
//!
//! Embedding runs off the write path: an event is durable in `event_log` immediately, and its
//! vector is computed later (tolerance 5 min, FR-MEM-22). Until then the event is found via FTS
//! only. This module owns the "which rows still need embedding" query and the batch step that
//! embeds them and stores the vectors; the runtime schedules the batches (and halves the
//! cadence in Low Power Mode, NFR-RES-04) — that scheduling is the adapter's concern, the batch
//! logic is here and testable.

use rusqlite::{params, Connection};

use crate::embed::{EmbedError, Embedder};

/// Errors from the embed job.
#[derive(Debug, thiserror::Error)]
pub enum EmbedJobError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("embed: {0}")]
    Embed(#[from] EmbedError),
}

/// Event ids (with their content) that have no embedding yet, oldest first, capped at `limit`.
/// A LEFT JOIN against `event_vec` finds the gap.
pub fn pending(conn: &Connection, limit: usize) -> Result<Vec<(i64, String)>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.content
           FROM event_log e
           LEFT JOIN event_vec v ON v.rowid = e.id
          WHERE v.rowid IS NULL
          ORDER BY e.id ASC
          LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit as i64], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Embed and store one batch of pending events (up to `batch`). Returns the number embedded.
/// Zero means nothing was pending. The vectors are written in a single transaction so a batch
/// is all-or-nothing.
pub fn embed_batch(
    conn: &mut Connection,
    embedder: &dyn Embedder,
    batch: usize,
) -> Result<usize, EmbedJobError> {
    let pending = pending(conn, batch)?;
    if pending.is_empty() {
        return Ok(0);
    }
    let texts: Vec<&str> = pending.iter().map(|(_, c)| c.as_str()).collect();
    let vectors = embedder.embed_passages(&texts)?;

    let tx = conn.transaction()?;
    for ((id, _), vec) in pending.iter().zip(vectors.iter()) {
        // Reuse the vector-store upsert semantics inside the batch transaction.
        tx.execute("DELETE FROM event_vec WHERE rowid = ?1", params![id])?;
        let blob: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
        tx.execute("INSERT INTO event_vec (rowid, embedding) VALUES (?1, ?2)", params![id, blob])?;
    }
    tx.commit()?;
    Ok(pending.len())
}

/// Drain all pending events in batches of `batch` until none remain. Returns the total embedded.
/// (The runtime normally calls [`embed_batch`] on a timer; this is for catch-up / tests.)
pub fn embed_all_pending(
    conn: &mut Connection,
    embedder: &dyn Embedder,
    batch: usize,
) -> Result<usize, EmbedJobError> {
    let mut total = 0;
    loop {
        let n = embed_batch(conn, embedder, batch)?;
        if n == 0 {
            break;
        }
        total += n;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::{MockEmbedder, E5_SMALL_DIM};
    use crate::event_log::{insert, NewEvent};

    fn add(conn: &Connection, content: &str, hash: &str) -> i64 {
        insert(
            conn,
            &NewEvent {
                ts: 1,
                source: "capture",
                kind: "text",
                app_bundle_id: None,
                window_title: None,
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
    fn pending_lists_unembedded_events() {
        let mut conn = crate::open_in_memory().unwrap();
        add(&conn, "one", "h1");
        add(&conn, "two", "h2");
        assert_eq!(pending(&conn, 10).unwrap().len(), 2);

        let m = MockEmbedder::new(E5_SMALL_DIM);
        embed_batch(&mut conn, &m, 10).unwrap();
        assert!(pending(&conn, 10).unwrap().is_empty(), "all embedded → nothing pending");
    }

    #[test]
    fn embed_batch_respects_batch_size() {
        let mut conn = crate::open_in_memory().unwrap();
        for i in 0..5 {
            add(&conn, &format!("event {i}"), &format!("h{i}"));
        }
        let m = MockEmbedder::new(E5_SMALL_DIM);
        assert_eq!(embed_batch(&mut conn, &m, 2).unwrap(), 2);
        assert_eq!(crate::vector::count(&conn).unwrap(), 2);
        assert_eq!(pending(&conn, 10).unwrap().len(), 3);
    }

    #[test]
    fn embed_all_drains_then_search_finds_via_vector() {
        let mut conn = crate::open_in_memory().unwrap();
        let id = add(&conn, "the budget review meeting notes", "h1");
        add(&conn, "unrelated chatter about weekend plans", "h2");
        let m = MockEmbedder::new(E5_SMALL_DIM);

        let embedded = embed_all_pending(&mut conn, &m, 8).unwrap();
        assert_eq!(embedded, 2);
        assert_eq!(crate::vector::count(&conn).unwrap(), 2);

        // The now-populated Warm store makes hybrid search return the semantic match.
        use crate::embed::Embedder;
        let q = m.embed_query("budget review").unwrap();
        let hits = crate::search::search_hybrid(&conn, "no_such_fts_term", Some(&q), 5).unwrap();
        assert!(hits.iter().any(|h| h.event_id == id));
    }

    #[test]
    fn embedding_is_off_the_write_path() {
        // An event is durable and FTS-searchable immediately, before any embedding runs.
        let conn = crate::open_in_memory().unwrap();
        add(&conn, "instantly durable content", "h1");
        assert_eq!(crate::vector::count(&conn).unwrap(), 0, "no embedding yet");
        let hits = crate::search::search(&conn, "durable", 5).unwrap();
        assert_eq!(hits.len(), 1, "found via FTS while un-embedded (FR-MEM-22)");
    }
}
