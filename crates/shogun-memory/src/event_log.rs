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
///
/// The `thread_key` is **derived** rather than passed in, so every writer is grouped into threads
/// without having to remember to do it; a screen capture has nothing but its app and window title
/// to group on anyway. A source that knows its own conversation id calls
/// [`insert_with_thread`] instead.
pub fn insert(conn: &Connection, ev: &NewEvent<'_>) -> Result<i64, rusqlite::Error> {
    insert_with_thread(conn, ev, None)
}

/// [`insert`], for a source that knows its own conversation id (an AI session id, a Gmail thread
/// id, an issue URL). That id is what the thread is really keyed on; without it the derivation
/// falls back to app + window title, which splits one conversation whenever the title varies —
/// an AI session's user and assistant turns would land in different threads, for instance.
pub fn insert_with_thread(
    conn: &Connection,
    ev: &NewEvent<'_>,
    native_thread_id: Option<&str>,
) -> Result<i64, rusqlite::Error> {
    let thread_key =
        crate::thread::thread_key(ev.source, native_thread_id, ev.app_bundle_id, ev.window_title);
    // Mask credentials before the row exists. Doing it here — rather than at each call site —
    // means no writer can forget, and the database never holds an unmasked copy to leak later.
    let content = crate::redact::redact(ev.content);
    conn.execute(
        "INSERT INTO event_log
           (ts, source, kind, app_bundle_id, window_title, content, content_hash,
            last_seen_at, dwell_ms, display_id, window_bounds, thread_key)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?1, ?8, ?9, ?10, ?11)",
        params![
            ev.ts,
            ev.source,
            ev.kind,
            ev.app_bundle_id,
            ev.window_title,
            content.as_ref(),
            ev.content_hash,
            ev.dwell_ms,
            ev.display_id,
            ev.window_bounds,
            thread_key,
        ],
    )?;
    let id = conn.last_insert_rowid();
    // Keep the thread index in step with the log. Failing to index must not fail the write —
    // the event is the durable record; a thread row can be rebuilt from it.
    if let Some(key) = thread_key.as_deref() {
        let _ = crate::thread::upsert_from_event(conn, key, ev.source, ev.window_title, ev.ts);
    }
    Ok(id)
}

/// Insert the event, or — if the most recent event from the same source already has this
/// `content_hash` — touch that row instead (FR-CAP-03): advance `last_seen_at` to the new `ts`
/// and add the new `dwell_ms`. Returns `(id, touched)`.
///
/// Matching is scoped to the same `source` so an identical string captured from two different
/// integrations stays as two distinct rows.
pub fn insert_or_touch(conn: &Connection, ev: &NewEvent<'_>) -> Result<(i64, bool), rusqlite::Error> {
    insert_or_touch_with_thread(conn, ev, None)
}

/// [`insert_or_touch`], carrying the source's own conversation id — see [`insert_with_thread`].
pub fn insert_or_touch_with_thread(
    conn: &Connection,
    ev: &NewEvent<'_>,
    native_thread_id: Option<&str>,
) -> Result<(i64, bool), rusqlite::Error> {
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
        Ok((insert_with_thread(conn, ev, native_thread_id)?, false))
    }
}

/// The most recent capture bodies `(content_hash, content)`, newest first, for the near-duplicate
/// collapse (FR-CAP-03). Scoped to `source = 'capture'` (only re-read window bodies collapse; user
/// notes and integration events are distinct). `limit` bounds the comparison cost.
pub fn recent_capture_bodies(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<(String, String)>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT content_hash, content FROM event_log
         WHERE source = 'capture' ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// One event's id and content, for the Dream Cycle consolidation pass (which classifies a day's
/// events). Content is included because consolidation reads it; callers that only need metadata
/// use the state reads instead.
#[derive(Debug, Clone, PartialEq)]
pub struct EventText {
    pub id: i64,
    pub content: String,
}

/// List events whose `ts` is in `[from_ts, to_ts)`, oldest first — the day's window a Dream Cycle
/// consolidation job consumes (FR-DC-03). The half-open range matches the `job_runs` input range so
/// re-running a job over the same window is deterministic.
pub fn events_in_range(
    conn: &Connection,
    from_ts: i64,
    to_ts: i64,
) -> Result<Vec<EventText>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, content FROM event_log WHERE ts >= ?1 AND ts < ?2 ORDER BY ts, id",
    )?;
    let rows = stmt.query_map(params![from_ts, to_ts], |r| {
        Ok(EventText { id: r.get(0)?, content: r.get(1)? })
    })?;
    rows.collect()
}

/// Count events in `[from_ts, to_ts)` — the size of a Dream Cycle input window (FR-DC-06), without
/// materializing the rows.
pub fn count_in_range(conn: &Connection, from_ts: i64, to_ts: i64) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT count(*) FROM event_log WHERE ts >= ?1 AND ts < ?2",
        params![from_ts, to_ts],
        |r| r.get(0),
    )
}

/// How many distinct hours in `[from_ts, to_ts)` produced at least one event.
///
/// This is the Coverage numerator (spec §D2): "18h / 24h captured" means eighteen of the last
/// twenty-four hours have something in them, not that eighteen hours of wall time were recorded.
/// Counting distinct hour buckets is what makes an idle lunch break read as a gap rather than
/// being averaged away by a busy morning.
///
/// Read-only: no schema change, just a grouping over the existing `ts` index.
pub fn hours_covered(conn: &Connection, from_ts: i64, to_ts: i64) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT count(DISTINCT ts / 3600000) FROM event_log WHERE ts >= ?1 AND ts < ?2",
        params![from_ts, to_ts],
        |r| r.get(0),
    )
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

    #[test]
    fn recent_capture_bodies_is_newest_first_and_capture_scoped() {
        let conn = crate::open_in_memory().unwrap();
        insert(&conn, &ev("first", "h1", 1, 0)).unwrap();
        insert(&conn, &ev("second", "h2", 2, 0)).unwrap();
        // a non-capture event must be excluded
        let mut note = ev("a note", "h3", 3, 0);
        note.source = "user";
        insert(&conn, &note).unwrap();

        let got = recent_capture_bodies(&conn, 8).unwrap();
        assert_eq!(got.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>(), vec!["second", "first"]);
        assert_eq!(got[0].0, "h2");
    }

    #[test]
    fn events_in_range_is_half_open_and_ordered() {
        let conn = crate::open_in_memory().unwrap();
        insert(&conn, &ev("a", "ha", 10, 0)).unwrap();
        insert(&conn, &ev("b", "hb", 20, 0)).unwrap();
        insert(&conn, &ev("c", "hc", 30, 0)).unwrap();
        // [10, 30): includes 10 and 20, excludes 30
        let got = events_in_range(&conn, 10, 30).unwrap();
        assert_eq!(got.iter().map(|e| e.content.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn hours_covered_counts_buckets_not_events() {
        const H: i64 = 3_600_000;
        let conn = crate::open_in_memory().unwrap();
        // A busy hour must not count for more than one, and an empty hour must stay a gap:
        // three events in hour 0, none in hour 1, one in hour 2 => 2 of 3 hours covered.
        insert(&conn, &ev("a", "ha", 1, 0)).unwrap();
        insert(&conn, &ev("b", "hb", 2, 0)).unwrap();
        insert(&conn, &ev("c", "hc", 3, 0)).unwrap();
        insert(&conn, &ev("d", "hd", 2 * H + 5, 0)).unwrap();

        assert_eq!(hours_covered(&conn, 0, 3 * H).unwrap(), 2);
        // Half-open, like every other range query here.
        assert_eq!(hours_covered(&conn, 0, 2 * H).unwrap(), 1);
        assert_eq!(hours_covered(&conn, 10 * H, 12 * H).unwrap(), 0);
    }
}
