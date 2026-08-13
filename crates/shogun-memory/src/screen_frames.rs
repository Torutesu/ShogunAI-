//! Short-lived JPEG frames from visual-recall OCR (issue #106, 2026-08-02).
//!
//! Frames live in the encrypted memory DB as BLOBs, linked to their `screen_ocr` event row.
//! A background purge drops anything older than [`RETENTION_MS`] (72 h rolling).

use rusqlite::{params, Connection};
use rusqlite::OptionalExtension;

/// Rolling retention for OCR capture frames (72 hours).
pub const RETENTION_MS: i64 = 72 * 60 * 60 * 1000;

/// OCR text shorter than this triggers `needs_rescan` on recall hits.
pub const THIN_OCR_CHARS: usize = 100;

/// Row to insert after a fresh OCR event is persisted.
#[derive(Debug, Clone)]
pub struct NewFrame<'a> {
    pub created_at_ms: i64,
    pub event_id: i64,
    pub app_bundle_id: Option<&'a str>,
    pub window_title: Option<&'a str>,
    pub display_id: Option<i64>,
    pub width: u32,
    pub height: u32,
    pub jpeg: &'a [u8],
}

/// Metadata for list/search surfaces — no pixel payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameSummary {
    pub id: i64,
    pub created_at_ms: i64,
    pub event_id: i64,
    pub app_bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub display_id: Option<i64>,
    pub width: u32,
    pub height: u32,
    pub jpeg_bytes: usize,
    pub ocr_text: String,
    /// Linked event source: `screen_ocr` (auto) or `user_screenshot` (explicit capture).
    pub source: String,
}

/// Full frame row including JPEG bytes (for agent re-scan / future vision input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameRecord {
    pub summary: FrameSummary,
    pub jpeg: Vec<u8>,
}

/// Recall hit for context assembly — metadata + OCR excerpt, no bytes inline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameRecallHit {
    pub frame_id: i64,
    pub event_id: i64,
    pub ts: i64,
    pub app_bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub width: u32,
    pub height: u32,
    pub ocr_excerpt: String,
    /// True when stored OCR text is thin; caller should re-scan the JPEG (Vision path).
    pub needs_rescan: bool,
    /// Linked event source (`screen_ocr` or `user_screenshot`).
    pub source: String,
}

fn map_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<FrameSummary> {
    Ok(FrameSummary {
        id: row.get(0)?,
        created_at_ms: row.get(1)?,
        event_id: row.get(2)?,
        app_bundle_id: row.get(3)?,
        window_title: row.get(4)?,
        display_id: row.get(5)?,
        width: u32::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
        height: u32::try_from(row.get::<_, i64>(7)?).unwrap_or(0),
        jpeg_bytes: usize::try_from(row.get::<_, i64>(8)?).unwrap_or(0),
        ocr_text: row.get(9)?,
        source: row.get(10)?,
    })
}

const SUMMARY_SELECT: &str = "SELECT f.id, f.created_at_ms, f.event_id, f.app_bundle_id, f.window_title,
    f.display_id, f.width, f.height, length(f.bytes), coalesce(e.content, ''), coalesce(e.source, 'screen_ocr')";

/// Insert one compressed frame. Returns the new row id.
pub fn insert(conn: &Connection, frame: &NewFrame<'_>) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO screen_frames
           (created_at_ms, event_id, app_bundle_id, window_title, display_id, width, height, mime, bytes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'image/jpeg', ?8)",
        params![
            frame.created_at_ms,
            frame.event_id,
            frame.app_bundle_id,
            frame.window_title,
            frame.display_id,
            i64::from(frame.width),
            i64::from(frame.height),
            frame.jpeg,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Lookup by frame row id.
pub fn get_summary_by_id(
    conn: &Connection,
    id: i64,
) -> Result<Option<FrameSummary>, rusqlite::Error> {
    conn.query_row(
        &format!(
            "{SUMMARY_SELECT}
             FROM screen_frames f
             LEFT JOIN event_log e ON e.id = f.event_id
             WHERE f.id = ?1"
        ),
        [id],
        map_summary,
    )
    .optional()
}

/// Lookup by frame row id, including JPEG bytes.
pub fn get_by_id(conn: &Connection, id: i64) -> Result<Option<FrameRecord>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!(
        "{SUMMARY_SELECT}, f.bytes
         FROM screen_frames f
         LEFT JOIN event_log e ON e.id = f.event_id
         WHERE f.id = ?1"
    ))?;
    let mut rows = stmt.query([id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let summary = map_summary(row)?;
    let jpeg: Vec<u8> = row.get(11)?;
    Ok(Some(FrameRecord { summary, jpeg }))
}

/// Lookup by linked event id.
pub fn get_by_event_id(conn: &Connection, event_id: i64) -> Result<Option<FrameRecord>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!(
        "{SUMMARY_SELECT}, f.bytes
         FROM screen_frames f
         LEFT JOIN event_log e ON e.id = f.event_id
         WHERE f.event_id = ?1
         ORDER BY f.id DESC LIMIT 1"
    ))?;
    let mut rows = stmt.query([event_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let summary = map_summary(row)?;
    let jpeg: Vec<u8> = row.get(11)?;
    Ok(Some(FrameRecord { summary, jpeg }))
}

/// List frames in a time window, newest first.
pub fn list_in_range(
    conn: &Connection,
    from_ms: i64,
    to_ms: i64,
    limit: usize,
) -> Result<Vec<FrameSummary>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!(
        "{SUMMARY_SELECT}
         FROM screen_frames f
         LEFT JOIN event_log e ON e.id = f.event_id
         WHERE f.created_at_ms >= ?1 AND f.created_at_ms <= ?2
         ORDER BY f.created_at_ms DESC
         LIMIT ?3"
    ))?;
    let rows = stmt.query_map(params![from_ms, to_ms, limit as i64], map_summary)?;
    rows.collect()
}

/// Frame id for a `screen_ocr` event, if any.
pub fn frame_id_for_event(conn: &Connection, event_id: i64) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row(
        "SELECT id FROM screen_frames WHERE event_id = ?1 ORDER BY id DESC LIMIT 1",
        [event_id],
        |r| r.get(0),
    )
    .optional()
}

/// Latest frame id per event (one query — avoids N+1 in evidence assembly).
pub fn frame_ids_for_events(
    conn: &Connection,
    event_ids: &[i64],
) -> Result<std::collections::HashMap<i64, i64>, rusqlite::Error> {
    if event_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = std::iter::repeat("?")
        .take(event_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT event_id, id FROM screen_frames WHERE event_id IN ({placeholders}) ORDER BY id DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(event_ids.iter()))?;
    let mut out = std::collections::HashMap::new();
    while let Some(row) = rows.next()? {
        let event_id: i64 = row.get(0)?;
        let frame_id: i64 = row.get(1)?;
        out.entry(event_id).or_insert(frame_id);
    }
    Ok(out)
}

/// Every frame's id, age and stored size — the input [`crate::retention::Policy::sweep`] decides
/// over. Metadata only: `length(bytes)` never loads the JPEG.
pub fn retention_items(conn: &Connection) -> Result<Vec<crate::retention::Item>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, created_at_ms, length(bytes) FROM screen_frames ORDER BY created_at_ms, id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(crate::retention::Item { id: r.get(0)?, created_at: r.get(1)?, bytes: r.get(2)? })
    })?;
    rows.collect()
}

/// Delete the frames a sweep selected, in ONE transaction. Returns rows removed.
///
/// **Frames only — the linked events stay.** Expiry retires the *image*, not the memory: the OCR
/// text and its provenance live in `event_log` and are the part SHOGUN is allowed to keep. This
/// is the opposite of [`delete_by_id`], where the user asked for a specific capture to be gone
/// and the record of it should go too.
///
/// One transaction so a crash mid-sweep leaves the cache consistent rather than half-expired.
pub fn delete_ids(conn: &mut Connection, ids: &[i64]) -> Result<usize, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let tx = conn.transaction()?;
    let mut removed = 0;
    for &id in ids {
        removed += tx.execute("DELETE FROM screen_frames WHERE id = ?1", [id])?;
    }
    tx.commit()?;
    Ok(removed)
}

/// Delete auto-capture frames only (`screen_ocr` events). User-initiated shots are kept.
pub fn purge_auto_only(conn: &Connection) -> Result<usize, rusqlite::Error> {
    conn.execute(
        "DELETE FROM screen_frames WHERE event_id IN (
            SELECT id FROM event_log WHERE source = 'screen_ocr'
         )",
        [],
    )
}

/// Delete one frame and its now-orphaned visual-recall event atomically.
pub fn delete_by_id(conn: &mut Connection, id: i64) -> Result<bool, rusqlite::Error> {
    let tx = conn.transaction()?;
    let event_id: Option<i64> = tx
        .query_row("SELECT event_id FROM screen_frames WHERE id = ?1", [id], |r| r.get(0))
        .optional()?;
    let Some(event_id) = event_id else {
        return Ok(false);
    };
    let removed = tx.execute("DELETE FROM screen_frames WHERE id = ?1", [id])?;
    if removed == 0 {
        return Ok(false);
    }
    tx.execute(
        "DELETE FROM event_log
         WHERE id = ?1
           AND source IN ('screen_ocr', 'user_screenshot')
           AND NOT EXISTS (SELECT 1 FROM screen_frames WHERE event_id = ?1)",
        [event_id],
    )?;
    tx.commit()?;
    Ok(true)
}

/// Search frames for visual-recall questions: FTS on linked OCR text, scoped to a time window.
pub fn search_for_recall(
    conn: &Connection,
    query: &str,
    from_ms: i64,
    to_ms: i64,
    limit: usize,
    excerpt_chars: usize,
) -> Result<Vec<FrameRecallHit>, rusqlite::Error> {
    let mut out = Vec::new();
    if let Some(expr) = crate::search::fts_query(query) {
        let mut stmt = conn.prepare(&format!(
            "{SUMMARY_SELECT}
             FROM screen_frames f
             INNER JOIN event_log e ON e.id = f.event_id
             INNER JOIN event_fts fts ON fts.rowid = e.id
             WHERE f.created_at_ms >= ?1 AND f.created_at_ms <= ?2
               AND event_fts MATCH ?3 AND e.source IN ('screen_ocr', 'user_screenshot')
             ORDER BY bm25(event_fts), f.created_at_ms DESC
             LIMIT ?4"
        ))?;
        let rows = stmt.query_map(params![from_ms, to_ms, expr, limit as i64], map_summary)?;
        for row in rows {
            out.push(recall_hit_from_summary(&row?, excerpt_chars));
        }
    }
    if out.len() < limit {
        let seen: std::collections::HashSet<i64> = out.iter().map(|h| h.frame_id).collect();
        let room = limit.saturating_sub(out.len());
        for s in list_in_range(conn, from_ms, to_ms, room + seen.len())? {
            if seen.contains(&s.id) {
                continue;
            }
            out.push(recall_hit_from_summary(&s, excerpt_chars));
            if out.len() >= limit {
                break;
            }
        }
    }
    Ok(out)
}

fn recall_hit_from_summary(s: &FrameSummary, excerpt_chars: usize) -> FrameRecallHit {
    let needs_rescan = s.ocr_text.trim().len() < THIN_OCR_CHARS;
    FrameRecallHit {
        frame_id: s.id,
        event_id: s.event_id,
        ts: s.created_at_ms,
        app_bundle_id: s.app_bundle_id.clone(),
        window_title: s.window_title.clone(),
        width: s.width,
        height: s.height,
        ocr_excerpt: crate::search::excerpt(&s.ocr_text, "", excerpt_chars),
        needs_rescan,
        source: s.source.clone(),
    }
}

/// Aggregate stats for settings / status surfaces (no pixel payload).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub count: i64,
    pub oldest_ms: Option<i64>,
    pub total_bytes: i64,
}

pub fn stats(conn: &Connection) -> Result<FrameStats, rusqlite::Error> {
    conn.query_row(
        "SELECT count(*), min(created_at_ms), coalesce(sum(length(bytes)), 0) FROM screen_frames",
        [],
        |r| {
            Ok(FrameStats {
                count: r.get(0)?,
                oldest_ms: r.get(1)?,
                total_bytes: r.get(2)?,
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_log::{insert as insert_event, NewEvent};

    fn seed_event(conn: &Connection, ts: i64, content: &str) -> i64 {
        insert_event(
            conn,
            &NewEvent {
                ts,
                source: "screen_ocr",
                kind: "text",
                app_bundle_id: Some("com.apple.Safari"),
                window_title: Some("Inbox"),
                content,
                content_hash: &format!("h{ts}"),
                dwell_ms: 1,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap()
    }

    fn seed_frame(conn: &Connection, ts: i64, event_id: i64, jpeg: &[u8]) -> i64 {
        insert(
            conn,
            &NewFrame {
                created_at_ms: ts,
                event_id,
                app_bundle_id: Some("com.apple.Safari"),
                window_title: Some("Inbox"),
                display_id: Some(1),
                width: 100,
                height: 50,
                jpeg,
            },
        )
        .unwrap()
    }

    #[test]
    fn insert_and_stats() {
        let conn = crate::open_in_memory().unwrap();
        let event_id = seed_event(&conn, 1_000, "hello");
        seed_frame(&conn, 1_000, event_id, &[0xFF, 0xD8, 0xFF, 0x00]);
        let s = stats(&conn).unwrap();
        assert_eq!(s.count, 1);
        assert_eq!(s.oldest_ms, Some(1_000));
        assert_eq!(s.total_bytes, 4);
    }

    #[test]
    fn a_sweep_expires_by_age_then_evicts_by_bytes() {
        use crate::retention::Policy;
        let mut conn = crate::open_in_memory().unwrap();
        // Four frames of 4 bytes each. The oldest is past a 1_000 ms window; the ceiling of 8
        // bytes then still leaves the survivors 4 over.
        for (ts, body) in [(100i64, b"aaaa"), (5_000, b"bbbb"), (5_100, b"cccc"), (5_200, b"dddd")] {
            let e = seed_event(&conn, ts, "x");
            seed_frame(&conn, ts, e, body);
        }
        let items = retention_items(&conn).unwrap();
        assert_eq!(items.len(), 4);
        assert!(items.iter().all(|i| i.bytes == 4), "size comes from length(bytes)");

        let sweep = Policy { retain_ms: 1_000, max_bytes: 8 }.sweep(&items, 5_200);
        assert_eq!(sweep.expired.len(), 1, "the 100 ms frame is past the window");
        assert_eq!(sweep.over_budget.len(), 1, "12 surviving bytes against an 8-byte ceiling");

        let removed = delete_ids(&mut conn, &sweep.all()).unwrap();
        assert_eq!(removed, 2);
        let s = stats(&conn).unwrap();
        assert_eq!(s.count, 2);
        assert_eq!(s.total_bytes, 8, "back under the ceiling");
        assert_eq!(s.oldest_ms, Some(5_100), "eviction took the oldest survivor");
        // The OCR events all survive. Expiry retires the image, not the memory it produced —
        // the text and its provenance are what SHOGUN keeps (V12's rollback note says the same).
        let events: i64 = conn
            .query_row("SELECT count(*) FROM event_log WHERE source = 'screen_ocr'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(events, 4, "expiring a frame must not delete the text it yielded");
    }

    #[test]
    fn deleting_no_ids_touches_nothing() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn, 100, "x");
        seed_frame(&conn, 100, e, b"jpeg");
        assert_eq!(delete_ids(&mut conn, &[]).unwrap(), 0);
        assert_eq!(stats(&conn).unwrap().count, 1);
    }

    #[test]
    fn get_by_event_and_search() {
        let conn = crate::open_in_memory().unwrap();
        let event_id = seed_event(&conn, 5_000, "quarterly roadmap slide");
        let frame_id = seed_frame(&conn, 5_000, event_id, b"jpeg");
        let rec = get_by_event_id(&conn, event_id).unwrap().expect("frame");
        assert_eq!(rec.summary.id, frame_id);
        assert_eq!(rec.jpeg, b"jpeg");
        let hits = search_for_recall(&conn, "roadmap", 0, 10_000, 5, 80).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].frame_id, frame_id);
        assert_eq!(frame_id_for_event(&conn, event_id).unwrap(), Some(frame_id));
    }

    #[test]
    fn list_in_range_respects_bounds() {
        let conn = crate::open_in_memory().unwrap();
        let e1 = seed_event(&conn, 1_000, "a");
        let e2 = seed_event(&conn, 5_000, "b");
        seed_frame(&conn, 1_000, e1, b"a");
        seed_frame(&conn, 5_000, e2, b"b");
        let listed = list_in_range(&conn, 2_000, 6_000, 10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].event_id, e2);
    }

    #[test]
    fn delete_removes_orphan_event_in_same_transaction() {
        let mut conn = crate::open_in_memory().unwrap();
        let event_id = seed_event(&conn, 1_000, "private screen text");
        let first = seed_frame(&conn, 1_000, event_id, b"a");
        let second = seed_frame(&conn, 1_001, event_id, b"b");

        assert!(delete_by_id(&mut conn, first).unwrap());
        let event_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM event_log WHERE id = ?1)",
                [event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(event_exists);

        assert!(delete_by_id(&mut conn, second).unwrap());
        let event_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM event_log WHERE id = ?1)",
                [event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!event_exists);
    }
}
