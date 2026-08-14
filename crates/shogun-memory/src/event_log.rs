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

/// The canonical content hash (xxhash64, hex) — the dedup key every writer must agree on.
///
/// It lives here rather than in each writer because it *is* the dedup contract: two callers
/// hashing the same text differently would not collide, and the near-duplicate collapse
/// (FR-CAP-03) would silently stop collapsing. One function, one definition.
pub fn content_hash(text: &str) -> String {
    use std::hash::Hasher;
    let mut h = twox_hash::XxHash64::with_seed(0);
    h.write(text.as_bytes());
    format!("{:016x}", h.finish())
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
    //
    // Deliberately NOT one transaction, against CLAUDE.md's default "DB書き込みはWAL＋トランザクション":
    // a transaction here would have to roll the EVENT back when only the derived index failed,
    // which is the one outcome this path must never produce. `threads` is a Dream-Cycle-rebuildable
    // recency cache, so a crash between the two statements costs a stale index until the next
    // re-derivation — and costs no captured data at all.
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
        // A touch is the user returning to this conversation, so its thread's recency must move
        // too — recency drives referent resolution ("that thing"), and the windows a user
        // revisits most are exactly the ones that would otherwise go stale. Best-effort like the
        // insert path's thread upsert: an index failure never fails the durable write.
        // `event_count` stays put — no new event exists.
        let _ = conn.execute(
            "UPDATE threads SET last_activity_at = max(last_activity_at, ?1), updated_at = ?1
              WHERE thread_key = (SELECT thread_key FROM event_log WHERE id = ?2)",
            params![ev.ts, id],
        );
        Ok((id, true))
    } else {
        Ok((insert_with_thread(conn, ev, native_thread_id)?, false))
    }
}

/// Replace event text after a Vision re-scan (visual recall). Updates `content_hash` for FTS.
pub fn update_content_and_hash(
    conn: &Connection,
    event_id: i64,
    content: &str,
    content_hash: &str,
) -> Result<bool, rusqlite::Error> {
    let content = crate::redact::redact(content);
    let n = conn.execute(
        "UPDATE event_log SET content = ?1, content_hash = ?2 WHERE id = ?3",
        params![content.as_ref(), content_hash, event_id],
    )?;
    Ok(n > 0)
}

/// The most recent `(content_hash, content)` pairs for near-dup collapse (FR-CAP-03), scoped to
/// one `source` so OCR re-reads do not collapse against AX captures and vice versa.
pub fn recent_source_bodies(
    conn: &Connection,
    source: &str,
    limit: usize,
) -> Result<Vec<(String, String)>, rusqlite::Error> {
    // Scoped to one app: a ≥98%-similar body seen in a *different* app must stay a separate
    // event, or the touch reuses the other app's row and the new capture's app/window attribution
    // is silently lost (`IS` so a NULL bundle id still matches only other NULL rows).
    let mut stmt = conn.prepare(
        "SELECT content_hash, content FROM event_log
         WHERE source = ?1 ORDER BY id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![source, limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// [`recent_source_bodies`] for `source = 'capture'`, additionally scoped to ONE app: a
/// ≥98%-similar body in a different app is a different capture, and collapsing onto it would
/// silently reassign the new capture to the other app's row. `IS` (not `=`) so a NULL bundle id
/// matches only NULL.
pub fn recent_capture_bodies(
    conn: &Connection,
    app_bundle_id: Option<&str>,
    limit: usize,
) -> Result<Vec<(String, String)>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT content_hash, content FROM event_log
         WHERE source = 'capture' AND app_bundle_id IS ?1 ORDER BY id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![app_bundle_id, limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// Metadata + short excerpt for recent events from one `source` (settings / Full UI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentEventPreview {
    pub id: i64,
    pub ts: i64,
    pub app_bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub excerpt: String,
    pub content_len: usize,
    pub dwell_ms: i64,
    pub display_id: Option<i64>,
}

/// Newest-first previews for a source. Excerpt is capped; full body is never returned.
pub fn recent_previews_by_source(
    conn: &Connection,
    source: &str,
    limit: usize,
    excerpt_chars: usize,
) -> Result<Vec<RecentEventPreview>, rusqlite::Error> {
    let cap = excerpt_chars.max(1) as i64;
    let mut stmt = conn.prepare(
        "SELECT id, ts, app_bundle_id, window_title,
                substr(content, 1, ?3), length(content), dwell_ms, display_id
         FROM event_log WHERE source = ?1 ORDER BY id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![source, limit as i64, cap], |r| {
        Ok(RecentEventPreview {
            id: r.get(0)?,
            ts: r.get(1)?,
            app_bundle_id: r.get(2)?,
            window_title: r.get(3)?,
            excerpt: r.get::<_, String>(4)?.trim().to_string(),
            content_len: r.get::<_, i64>(5)? as usize,
            dwell_ms: r.get(6)?,
            display_id: r.get(7)?,
        })
    })?;
    rows.collect()
}

/// Count events from `source` in `[from_ts, to_ts)`.
pub fn count_source_in_range(
    conn: &Connection,
    source: &str,
    from_ts: i64,
    to_ts: i64,
) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT count(*) FROM event_log WHERE source = ?1 AND ts >= ?2 AND ts < ?3",
        params![source, from_ts, to_ts],
        |r| r.get(0),
    )
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
///
/// **Local reads only.** Anything that leaves the device goes through
/// [`events_in_range_partitioned`], whose `cloud` half is source-filtered — this function returns
/// every source, including the ones that must never reach the Batch lane.
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

/// Sources whose text stays on the device: search, Fusion and *local* extraction may read them,
/// the Batch lane may not (A-2 decision, `docs/meeting-text-on-the-search-spine.md`).
///
/// `meeting` is here because its consent story is different from capture's. The Deepgram opt-in
/// covers live transcription (process-only); it does not cover shipping the finished transcript
/// to the relay every night for classification. A source added later with the same shape — text
/// whose disclosure named a narrower use than "nightly cloud classification" — belongs on this
/// list too.
pub const BATCH_EXCLUDED_SOURCES: &[&str] = &["meeting"];

/// An event text cleared for the Batch lane. The type is the proof: only
/// [`events_in_range_partitioned`] constructs it (enforced by
/// `scripts/check-batch-source-filter.py`), so `build_batch_items` demanding `&[BatchEventText]`
/// means an unfiltered window *cannot compile* its way to the relay.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchEventText {
    pub id: i64,
    pub content: String,
}

/// One window, split by where its text is allowed to go.
#[derive(Debug, Clone, Default)]
pub struct PartitionedEvents {
    /// May be sent to the Batch/Select-KK lane for classification.
    pub cloud: Vec<BatchEventText>,
    /// Device-only sources ([`BATCH_EXCLUDED_SOURCES`]): still classified, but by the local
    /// rule extractor — never a model call.
    pub local_only: Vec<EventText>,
}

/// The Dream Cycle's window read: everything in `[from_ts, to_ts)`, partitioned into what the
/// Batch lane may see and what stays local.
///
/// One query and a total split — every event lands in exactly one half, so an excluded source is
/// visibly routed to the local classifier rather than silently dropped from the night's work.
pub fn events_in_range_partitioned(
    conn: &Connection,
    from_ts: i64,
    to_ts: i64,
) -> Result<PartitionedEvents, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, content, source FROM event_log WHERE ts >= ?1 AND ts < ?2 ORDER BY ts, id",
    )?;
    let mut rows = stmt.query(params![from_ts, to_ts])?;
    let mut out = PartitionedEvents::default();
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let content: String = row.get(1)?;
        let source: String = row.get(2)?;
        if BATCH_EXCLUDED_SOURCES.contains(&source.as_str()) {
            out.local_only.push(EventText { id, content });
        } else {
            out.cloud.push(BatchEventText { id, content });
        }
    }
    Ok(out)
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
    fn the_partition_routes_meeting_text_away_from_the_cloud_half() {
        // The A-2 invariant in one test: a window with capture and meeting text splits totally,
        // and the meeting rows land in local_only — never in the half the relay sees.
        let conn = crate::open_in_memory().unwrap();
        insert(&conn, &ev("on screen", "h1", 100, 0)).unwrap();
        insert(
            &conn,
            &NewEvent { source: "meeting", ..ev("Me: said in a call", "h2", 200, 0) },
        )
        .unwrap();
        insert(&conn, &ev("more screen", "h3", 300, 0)).unwrap();

        let w = events_in_range_partitioned(&conn, 0, 1_000).unwrap();
        assert_eq!(w.cloud.len(), 2);
        assert_eq!(w.local_only.len(), 1);
        assert!(w.cloud.iter().all(|e| !e.content.contains("said in a call")));
        assert_eq!(w.local_only[0].content, "Me: said in a call");
        // total: nothing silently dropped
        assert_eq!(w.cloud.len() + w.local_only.len(), events_in_range(&conn, 0, 1_000).unwrap().len());
    }

    #[test]
    fn an_empty_exclusion_free_window_is_all_cloud() {
        let conn = crate::open_in_memory().unwrap();
        insert(&conn, &ev("a", "ha", 1, 0)).unwrap();
        let w = events_in_range_partitioned(&conn, 0, 10).unwrap();
        assert_eq!(w.cloud.len(), 1);
        assert!(w.local_only.is_empty());
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

        let got = recent_capture_bodies(&conn, Some("com.apple.Safari"), 8).unwrap();
        assert_eq!(got.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>(), vec!["second", "first"]);
        assert_eq!(got[0].0, "h2");
    }

    #[test]
    fn recent_capture_bodies_is_scoped_to_one_app() {
        // The near-dup collapse must only compare against the SAME app's recent bodies: a
        // ≥98%-similar body in a different app is a different capture, and collapsing onto it
        // would silently reassign the new capture to the other app's row.
        let conn = crate::open_in_memory().unwrap();
        insert(&conn, &ev("shared body", "h1", 1, 0)).unwrap();
        let mut other_app = ev("other app body", "h2", 2, 0);
        other_app.app_bundle_id = Some("com.apple.Mail");
        insert(&conn, &other_app).unwrap();

        let safari = recent_capture_bodies(&conn, Some("com.apple.Safari"), 8).unwrap();
        assert_eq!(safari.len(), 1);
        assert_eq!(safari[0].1, "shared body");
        let mail = recent_capture_bodies(&conn, Some("com.apple.Mail"), 8).unwrap();
        assert_eq!(mail.len(), 1);
        assert_eq!(mail[0].1, "other app body");
        assert!(recent_capture_bodies(&conn, None, 8).unwrap().is_empty(), "NULL matches only NULL");
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
