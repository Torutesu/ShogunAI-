//! Cold-layer repository (FR-MEM-04): Warm→Cold demotion and the int8 archive.
//!
//! Demotion is what keeps the sqlite-vec brute-force scan bounded (FR-MEM-03): once an event ages
//! past the Warm window (30 days), its f32 Warm embedding is re-quantized to int8, archived in
//! `cold_embeddings`, and removed from `event_vec`. The event and its FTS coverage stay — only the
//! Warm *vector* leaves. Everything runs in one transaction so a crash can't half-move a row.
//!
//! The Dream Cycle's ColdDemotion job (M3) calls [`demote_older_than`] nightly.

use rusqlite::{params, Connection};

use crate::quantize::{pack_i8, quantize_i8, unpack_i8};

/// The Warm window in milliseconds (FR-MEM-04: Warm = 30 days). Events older than `now - WARM_WINDOW_MS`
/// are demotion candidates.
pub const WARM_WINDOW_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Period-partition span in milliseconds (30 days). Coarse buckets let the archive be pruned or
/// loaded by period without a calendar library; `partition_of` is deterministic.
pub const PARTITION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// The period-partition bucket an event timestamp falls into.
pub fn partition_of(ts_ms: i64) -> i64 {
    ts_ms.div_euclid(PARTITION_MS)
}

/// Archive one event's embedding into the Cold tier and drop it from Warm, atomically. The f32
/// vector is re-quantized to int8. Idempotent: re-demoting replaces the Cold row. Returns `true` if
/// a Warm embedding was present and moved, `false` if there was nothing to demote.
pub fn demote(conn: &mut Connection, event_id: i64, ts_ms: i64) -> Result<bool, rusqlite::Error> {
    let embedding = match crate::vector::get(conn, event_id)? {
        Some(v) if !v.is_empty() => v,
        _ => return Ok(false),
    };
    let (codes, scale) = quantize_i8(&embedding);
    let dim = codes.len() as i64;
    let blob = pack_i8(&codes);
    let partition = partition_of(ts_ms);

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT OR REPLACE INTO cold_embeddings (event_id, partition, scale, dim, codes)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![event_id, partition, scale as f64, dim, blob],
    )?;
    tx.execute("DELETE FROM event_vec WHERE rowid = ?1", params![event_id])?;
    tx.commit()?;
    Ok(true)
}

/// Demote every Warm embedding whose event is older than `cutoff_ms` (typically `now - WARM_WINDOW_MS`).
/// Returns the number of embeddings moved. Only events that both have a Warm embedding and an
/// `event_log` timestamp before the cutoff are affected.
pub fn demote_older_than(conn: &mut Connection, cutoff_ms: i64) -> Result<usize, rusqlite::Error> {
    // Collect candidates first (id + ts) so we don't hold a statement open across the mutations.
    let candidates: Vec<(i64, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT e.id, e.ts FROM event_log e
             JOIN event_vec v ON v.rowid = e.id
             WHERE e.ts < ?1
             ORDER BY e.id",
        )?;
        let rows = stmt.query_map(params![cutoff_ms], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let mut moved = 0usize;
    for (id, ts) in candidates {
        if demote(conn, id, ts)? {
            moved += 1;
        }
    }
    Ok(moved)
}

/// Number of archived Cold embeddings.
pub fn count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT count(*) FROM cold_embeddings", [], |r| r.get(0))
}

/// A Cold embedding read back (int8 codes reconstructed to f32 via its scale). Cold is not the
/// routine search target (FR-MEM-03); this is for period loads / diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct ColdEmbedding {
    pub event_id: i64,
    pub partition: i64,
    pub vector: Vec<f32>,
}

/// Load all Cold embeddings in a partition, dequantized back to f32.
pub fn load_partition(conn: &Connection, partition: i64) -> Result<Vec<ColdEmbedding>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT event_id, partition, scale, codes FROM cold_embeddings WHERE partition = ?1 ORDER BY event_id",
    )?;
    let rows = stmt.query_map(params![partition], |r| {
        let event_id: i64 = r.get(0)?;
        let partition: i64 = r.get(1)?;
        let scale: f64 = r.get(2)?;
        let codes_blob: Vec<u8> = r.get(3)?;
        let codes = unpack_i8(&codes_blob);
        let vector = crate::quantize::dequantize_i8(&codes, scale as f32);
        Ok(ColdEmbedding { event_id, partition, vector })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::{cosine_similarity, Embedder, MockEmbedder, E5_SMALL_DIM};
    use crate::event_log::{insert, NewEvent};

    fn add_event(conn: &Connection, ts: i64, content: &str, hash: &str) -> i64 {
        insert(
            conn,
            &NewEvent {
                ts,
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
    fn demote_moves_warm_to_cold_and_preserves_direction() {
        let mut conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let id = add_event(&conn, 1000, "the quarterly review", "h1");
        let v = m.embed_passages(&["the quarterly review"]).unwrap()[0].clone();
        crate::vector::upsert(&conn, id, &v).unwrap();
        assert_eq!(crate::vector::count(&conn).unwrap(), 1);

        assert!(demote(&mut conn, id, 1000).unwrap());
        // gone from Warm, present in Cold
        assert_eq!(crate::vector::count(&conn).unwrap(), 0);
        assert_eq!(count(&conn).unwrap(), 1);

        // the archived vector still resembles the original (int8 round-trip)
        let cold = load_partition(&conn, partition_of(1000)).unwrap();
        assert_eq!(cold.len(), 1);
        assert_eq!(cold[0].event_id, id);
        assert!(cosine_similarity(&v, &cold[0].vector) >= 0.999);
    }

    #[test]
    fn demote_older_than_only_moves_aged_events() {
        let mut conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let now = 100 * PARTITION_MS;
        let old_id = add_event(&conn, now - WARM_WINDOW_MS - 1, "old note", "h_old");
        let fresh_id = add_event(&conn, now - 1000, "fresh note", "h_fresh");
        for (id, text) in [(old_id, "old note"), (fresh_id, "fresh note")] {
            let v = m.embed_passages(&[text]).unwrap()[0].clone();
            crate::vector::upsert(&conn, id, &v).unwrap();
        }

        let moved = demote_older_than(&mut conn, now - WARM_WINDOW_MS).unwrap();
        assert_eq!(moved, 1, "only the aged event demotes");
        assert_eq!(crate::vector::count(&conn).unwrap(), 1, "the fresh embedding stays Warm");
        assert_eq!(count(&conn).unwrap(), 1);
    }

    #[test]
    fn demoted_event_is_no_longer_a_warm_knn_hit() {
        let mut conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let id = add_event(&conn, 1000, "budget planning", "h1");
        let v = m.embed_passages(&["budget planning"]).unwrap()[0].clone();
        crate::vector::upsert(&conn, id, &v).unwrap();
        demote(&mut conn, id, 1000).unwrap();
        // Warm KNN now returns nothing (it left the Warm scan set, FR-MEM-03)
        let hits = crate::vector::knn(&conn, &v, 5).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn demote_with_no_warm_embedding_is_a_noop() {
        let mut conn = crate::open_in_memory().unwrap();
        let id = add_event(&conn, 1000, "no embedding", "h1");
        assert!(!demote(&mut conn, id, 1000).unwrap());
        assert_eq!(count(&conn).unwrap(), 0);
    }

    #[test]
    fn partition_bucketing_is_deterministic_and_period_aligned() {
        assert_eq!(partition_of(0), 0);
        assert_eq!(partition_of(PARTITION_MS - 1), 0);
        assert_eq!(partition_of(PARTITION_MS), 1);
        assert_eq!(partition_of(2 * PARTITION_MS + 5), 2);
    }

    #[test]
    fn deleting_an_event_cascades_into_cold() {
        let mut conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let id = add_event(&conn, 1000, "x", "h1");
        let v = m.embed_passages(&["x"]).unwrap()[0].clone();
        crate::vector::upsert(&conn, id, &v).unwrap();
        demote(&mut conn, id, 1000).unwrap();
        assert_eq!(count(&conn).unwrap(), 1);
        conn.execute("DELETE FROM event_log WHERE id = ?1", [id]).unwrap();
        assert_eq!(count(&conn).unwrap(), 0, "cold_embeddings cascades on event delete");
    }
}
