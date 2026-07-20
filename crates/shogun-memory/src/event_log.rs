//! Event-log repository (FR-MEM-10/11, FR-CAP-03).
//!
//! The event log is append-only: the only UPDATE the repository performs is the dedup touch
//! (`last_seen_at` + accumulated `dwell_ms`) when a capture repeats content already recorded
//! (FR-CAP-03). Everything else is INSERT. The near-duplicate *decision* (98% similarity) is a
//! capture-layer concern (WP2.2) that resolves to a `content_hash`; here the hash is the key.

use rusqlite::{params, Connection};

/// A row to append to the event log. Spatial-ready fields are optional and NULL in v1
/// (`window_pose` / `gaze_target` are not written by v1 at all; kept in the schema per
/// FR-MEM-12 so they never need a backward-incompatible add later).
#[derive(Debug, Clone)]
pub struct NewEvent<'a> {
    pub ts: i64,
    pub source: &'a str,
    pub kind: &'a str,
    pub app_bundle_id: Option<&'a str>,
    pub window_title: Option<&'a str>,
    pub content: &'a str,
    pub content_hash: &'a str,
    pub dwell_ms: i64,
    pub display_id: Option<i64>,
    pub window_bounds: Option<&'a str>,
}

/// Append a new event row unconditionally. Returns the new row id. `last_seen_at` starts equal
/// to `ts`.
pub fn insert(conn: &Connection, ev: &NewEvent<'_>) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO event_log
           (ts, source, kind, app_bundle_id, window_title, content, content_hash,
            last_seen_at, dwell_ms, display_id, window_bounds)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?1, ?8, ?9, ?10)",
        params![
            ev.ts,
            ev.source,
            ev.kind,
            ev.app_bundle_id,
            ev.window_title,
            ev.content,
            ev.content_hash,
            ev.dwell_ms,
            ev.display_id,
            ev.window_bounds,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Insert the event, or — if the most recent event from the same source already has this
/// `content_hash` — touch that row instead (FR-CAP-03): advance `last_seen_at` to the new `ts`
/// and add the new `dwell_ms`. Returns `(id, touched)`.
///
/// Matching is scoped to the same `source` so an identical string captured from two different
/// integrations stays as two distinct rows.
pub fn insert_or_touch(conn: &Connection, ev: &NewEvent<'_>) -> Result<(i64, bool), rusqlite::Error> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM event_log WHERE content_hash = ?1 AND source = ?2 ORDER BY id DESC LIMIT 1",
            params![ev.content_hash, ev.source],
            |r| r.get(0),
        )
        .ok();

    if let Some(id) = existing {
        conn.execute(
            "UPDATE event_log SET last_seen_at = ?1, dwell_ms = dwell_ms + ?2 WHERE id = ?3",
            params![ev.ts, ev.dwell_ms, id],
        )?;
        Ok((id, true))
    } else {
        Ok((insert(conn, ev)?, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev<'a>(content: &'a str, hash: &'a str, ts: i64, dwell: i64) -> NewEvent<'a> {
        NewEvent {
            ts,
            source: "capture",
            kind: "text",
            app_bundle_id: Some("com.apple.Safari"),
            window_title: Some("title"),
            content,
            content_hash: hash,
            dwell_ms: dwell,
            display_id: Some(1),
            window_bounds: None,
        }
    }

    #[test]
    fn insert_returns_incrementing_ids() {
        let conn = crate::open_in_memory().unwrap();
        let a = insert(&conn, &ev("a", "ha", 1, 0)).unwrap();
        let b = insert(&conn, &ev("b", "hb", 2, 0)).unwrap();
        assert!(b > a);
    }

    #[test]
    fn touch_accumulates_dwell_without_new_row() {
        let conn = crate::open_in_memory().unwrap();
        let (id1, touched1) = insert_or_touch(&conn, &ev("hello", "h", 100, 50)).unwrap();
        assert!(!touched1);
        let (id2, touched2) = insert_or_touch(&conn, &ev("hello", "h", 200, 30)).unwrap();
        assert!(touched2);
        assert_eq!(id1, id2);

        let count: i64 = conn.query_row("SELECT count(*) FROM event_log", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "touch must not append a row");
        let (last_seen, dwell): (i64, i64) = conn
            .query_row("SELECT last_seen_at, dwell_ms FROM event_log WHERE id = ?1", [id1], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(last_seen, 200);
        assert_eq!(dwell, 80); // 50 + 30
    }

    #[test]
    fn same_hash_different_source_stays_distinct() {
        let conn = crate::open_in_memory().unwrap();
        insert_or_touch(&conn, &ev("x", "h", 1, 0)).unwrap();
        let mut other = ev("x", "h", 2, 0);
        other.source = "gmail";
        let (_, touched) = insert_or_touch(&conn, &other).unwrap();
        assert!(!touched, "a different source must not touch the capture row");
        let count: i64 = conn.query_row("SELECT count(*) FROM event_log", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 2);
    }
}
