//! Warm-layer vector store + KNN search (FR-MEM-01/03) over the sqlite-vec `event_vec` table.
//!
//! sqlite-vec does an exhaustive (brute-force) scan, which is exactly why vector search targets
//! the Warm set only (FR-MEM-03). Embeddings are 384-dim (e5-small) f32 vectors stored as
//! little-endian byte blobs, keyed by `rowid = event_log.id`. This module owns the extension
//! registration, the store/delete, and the KNN query that produces the ranked id list the
//! search layer fuses with FTS (via RRF).

use std::sync::Once;

use rusqlite::{params, Connection};

static REGISTER: Once = Once::new();

/// Register the sqlite-vec extension for all subsequently-opened connections (idempotent,
/// process-global). Must run before any connection that uses `vec0` — [`crate::open`] and
/// [`crate::open_in_memory`] call it first.
pub fn register_extension() {
    REGISTER.call_once(|| {
        // SAFETY: the documented sqlite-vec registration — `sqlite3_vec_init` is a valid
        // extension entry point; the transmute reinterprets it as the fn-pointer type
        // `sqlite3_auto_extension` expects (inferred from the argument position). One-shot
        // behind `Once`, before any connection opens.
        #[allow(clippy::missing_transmute_annotations)]
        unsafe {
            let init = std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ());
            rusqlite::ffi::sqlite3_auto_extension(Some(init));
        }
    });
}

/// Pack an f32 vector into the little-endian byte blob sqlite-vec expects.
fn to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Store (or replace) the embedding for `event_id`. The vector length must match the table's
/// declared dimension (384) or sqlite-vec rejects it.
pub fn upsert(conn: &Connection, event_id: i64, embedding: &[f32]) -> Result<(), rusqlite::Error> {
    // vec0 is a virtual table; DELETE-then-INSERT keeps it simple and idempotent.
    conn.execute("DELETE FROM event_vec WHERE rowid = ?1", params![event_id])?;
    conn.execute(
        "INSERT INTO event_vec (rowid, embedding) VALUES (?1, ?2)",
        params![event_id, to_blob(embedding)],
    )?;
    Ok(())
}

/// Remove an event's embedding (e.g. when the event moves to Cold).
pub fn delete(conn: &Connection, event_id: i64) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM event_vec WHERE rowid = ?1", params![event_id])?;
    Ok(())
}

/// Number of stored embeddings.
pub fn count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT count(*) FROM event_vec", [], |r| r.get(0))
}

/// K-nearest-neighbour search: returns up to `k` `event_id`s ordered by ascending distance
/// (nearest first). This is the Warm-layer vector list the search layer fuses with FTS.
pub fn knn(conn: &Connection, query: &[f32], k: usize) -> Result<Vec<i64>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT rowid FROM event_vec WHERE embedding MATCH ?1 AND k = ?2 ORDER BY distance",
    )?;
    let ids = stmt
        .query_map(params![to_blob(query), k as i64], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::{Embedder, MockEmbedder, E5_SMALL_DIM};
    use crate::event_log::{insert, NewEvent};

    fn add_event(conn: &Connection, content: &str, hash: &str) -> i64 {
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
    fn store_and_count() {
        let conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let id = add_event(&conn, "hello world", "h1");
        let vec = m.embed_passages(&["hello world"]).unwrap()[0].clone();
        upsert(&conn, id, &vec).unwrap();
        assert_eq!(count(&conn).unwrap(), 1);
        delete(&conn, id).unwrap();
        assert_eq!(count(&conn).unwrap(), 0);
    }

    #[test]
    fn knn_ranks_semantically_closer_events_first() {
        let conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);

        let close_id = add_event(&conn, "the quarterly budget review meeting", "h1");
        let far_id = add_event(&conn, "lunch plans for saturday afternoon", "h2");
        for (id, text) in [(close_id, "the quarterly budget review meeting"), (far_id, "lunch plans for saturday afternoon")] {
            let v = m.embed_passages(&[text]).unwrap()[0].clone();
            upsert(&conn, id, &v).unwrap();
        }

        let q = m.embed_query("quarterly budget review").unwrap();
        let hits = knn(&conn, &q, 2).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0], close_id, "the budget event must rank first");
    }

    #[test]
    fn upsert_replaces_not_duplicates() {
        let conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let id = add_event(&conn, "x", "h1");
        let v1 = m.embed_passages(&["first"]).unwrap()[0].clone();
        let v2 = m.embed_passages(&["second"]).unwrap()[0].clone();
        upsert(&conn, id, &v1).unwrap();
        upsert(&conn, id, &v2).unwrap();
        assert_eq!(count(&conn).unwrap(), 1);
    }
}
