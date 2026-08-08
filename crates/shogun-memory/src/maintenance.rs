//! Local data export and full deletion (FR-SET-07). The user owns their memory: they can export it
//! all as JSON and delete it all. Both operate entirely on-device — export is a local file, never a
//! network send (invariant 3), and deletion wipes user data while keeping the schema so the app
//! keeps working.

use rusqlite::Connection;
use serde_json::{json, Value};

/// Export all user data as a JSON string (FR-SET-07). Includes the event log and the four state
/// tables with their content — this is the user's own local export, so full content is included
/// (it never leaves the device by this path). Ordering is stable (by id) for reproducibility.
pub fn export_json(conn: &Connection) -> Result<String, rusqlite::Error> {
    let events = rows(conn, "SELECT id, ts, source, kind, app_bundle_id, window_title, content, dwell_ms FROM event_log ORDER BY id", |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "ts": r.get::<_, i64>(1)?,
            "source": r.get::<_, String>(2)?,
            "kind": r.get::<_, String>(3)?,
            "app_bundle_id": r.get::<_, Option<String>>(4)?,
            "window_title": r.get::<_, Option<String>>(5)?,
            "content": r.get::<_, String>(6)?,
            "dwell_ms": r.get::<_, i64>(7)?,
        }))
    })?;
    let people = rows(conn, "SELECT id, display_name, confidence FROM people ORDER BY id", |r| {
        Ok(json!({ "id": r.get::<_, i64>(0)?, "display_name": r.get::<_, String>(1)?, "confidence": r.get::<_, f64>(2)? }))
    })?;
    let projects = rows(conn, "SELECT id, name, status, confidence FROM projects ORDER BY id", |r| {
        Ok(json!({ "id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "status": r.get::<_, String>(2)?, "confidence": r.get::<_, f64>(3)? }))
    })?;
    let commitments = rows(conn, "SELECT id, description, due_at, status, confidence FROM commitments ORDER BY id", |r| {
        Ok(json!({ "id": r.get::<_, i64>(0)?, "description": r.get::<_, String>(1)?, "due_at": r.get::<_, Option<i64>>(2)?, "status": r.get::<_, String>(3)?, "confidence": r.get::<_, f64>(4)? }))
    })?;
    let open_loops = rows(conn, "SELECT id, kind, description, staleness_days, status, confidence FROM open_loops ORDER BY id", |r| {
        Ok(json!({ "id": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?, "description": r.get::<_, String>(2)?, "staleness_days": r.get::<_, i64>(3)?, "status": r.get::<_, String>(4)?, "confidence": r.get::<_, f64>(5)? }))
    })?;

    let doc = json!({
        "schema_version": crate::schema_version(conn)?,
        "event_log": events,
        "people": people,
        "projects": projects,
        "commitments": commitments,
        "open_loops": open_loops,
    });
    Ok(doc.to_string())
}

/// Run a query and collect each row into a JSON value.
fn rows(
    conn: &Connection,
    sql: &str,
    map: impl Fn(&rusqlite::Row<'_>) -> rusqlite::Result<Value>,
) -> Result<Vec<Value>, rusqlite::Error> {
    let mut stmt = conn.prepare(sql)?;
    let out = stmt.query_map([], |r| map(r))?;
    out.collect()
}

/// How many rows deletion removed, per table.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct DeleteReport {
    pub events: usize,
    pub people: usize,
    pub projects: usize,
    pub commitments: usize,
    pub open_loops: usize,
    pub threads: usize,
    pub sessions: usize,
    pub session_notes: usize,
    pub screen_frames: usize,
    pub traceability: usize,
}

/// Delete **all** user data (FR-SET-07), keeping the schema. Runs in a single transaction so a
/// failure leaves the database untouched. Child rows (provenance, commitments, open loops) go
/// before their parents to satisfy foreign keys; embeddings, the FTS mirror (via triggers), the
/// traceability log, and the Dream Cycle ledger are cleared too.
pub fn delete_all(conn: &mut Connection) -> Result<DeleteReport, rusqlite::Error> {
    let tx = conn.transaction()?;
    // children first (FK order): provenance → commitments/open_loops → threads → people/projects.
    // `threads` holds titles, summaries and participants, so it is user data and must go too —
    // and it references projects, so it goes before them.
    tx.execute("DELETE FROM state_provenance", [])?;
    let commitments = tx.execute("DELETE FROM commitments", [])?;
    let open_loops = tx.execute("DELETE FROM open_loops", [])?;
    let threads = tx.execute("DELETE FROM threads", [])?;
    let people = tx.execute("DELETE FROM people", [])?;
    let projects = tx.execute("DELETE FROM projects", [])?;
    // embeddings (Warm vec0 + Cold int8) then the event log (its AD trigger clears event_fts)
    tx.execute("DELETE FROM event_vec", [])?;
    tx.execute("DELETE FROM cold_embeddings", [])?;
    let events = tx.execute("DELETE FROM event_log", [])?;
    // Everything that references sessions with no ON DELETE CASCADE must go before the sessions
    // themselves, or with foreign_keys=ON the delete FK-fails and rolls back. Meeting notes
    // (V8, the user's own words — the most personal rows here, FR-MT-10), transcript segments
    // (V9), and meeting recaps (V10) all reference sessions.
    let session_notes = tx.execute("DELETE FROM session_notes", [])?;
    tx.execute("DELETE FROM transcript_segments", [])?;
    tx.execute("DELETE FROM meeting_recaps", [])?;
    let screen_frames = tx.execute("DELETE FROM screen_frames", [])?;
    // Sessions hold the meeting's title, summary and decisions — user data. event_log also
    // references sessions, and it was already cleared above, so sessions can go now (FR-SET-07,
    // FR-MT-05).
    let sessions = tx.execute("DELETE FROM sessions", [])?;
    let traceability = tx.execute("DELETE FROM traceability_log", [])?;
    tx.execute("DELETE FROM job_runs", [])?;
    tx.commit()?;

    Ok(DeleteReport {
        events,
        people,
        projects,
        commitments,
        open_loops,
        threads,
        sessions,
        session_notes,
        screen_frames,
        traceability,
    })
}

/// Delete every user row whose occurrence time is at or after `cutoff_ts` (unix ms), and any state
/// row that loses ALL of its evidence as a result (design decision ③). Runs in a single
/// transaction. Derived summary text on a surviving state row may still reflect a deleted event
/// until the next Dream Cycle re-derivation — this is documented, not silently hidden.
pub fn delete_since(conn: &mut Connection, cutoff_ts: i64) -> Result<DeleteReport, rusqlite::Error> {
    let tx = conn.transaction()?;

    // Provenance rows that point at events we are about to delete go first (FK: they reference
    // event_log). This is what can orphan a state row.
    tx.execute(
        "DELETE FROM state_provenance WHERE event_id IN (SELECT id FROM event_log WHERE ts >= ?1)",
        [cutoff_ts],
    )?;

    // Vectors + cold embeddings for the doomed events (keyed on event id).
    tx.execute(
        "DELETE FROM event_vec WHERE rowid IN (SELECT id FROM event_log WHERE ts >= ?1)",
        [cutoff_ts],
    )?;
    tx.execute(
        "DELETE FROM cold_embeddings WHERE event_id IN (SELECT id FROM event_log WHERE ts >= ?1)",
        [cutoff_ts],
    )?;

    // Visual-recall frames in the window — and any frame referencing a doomed event (V12:
    // screen_frames.event_id → event_log with no cascade) — must go before the events, or the
    // event delete FK-fails and rolls the whole deletion back.
    let screen_frames = tx.execute(
        "DELETE FROM screen_frames WHERE created_at_ms >= ?1 OR event_id IN (SELECT id FROM event_log WHERE ts >= ?1)",
        [cutoff_ts],
    )?;

    // The events themselves (AD trigger clears event_fts).
    let events = tx.execute("DELETE FROM event_log WHERE ts >= ?1", [cutoff_ts])?;

    // Meeting sessions started in the window, and everything that references them, must all go
    // before `DELETE FROM sessions` — with foreign_keys=ON a lingering child is a hard FK error
    // that rolls the whole deletion back. Children of sessions with NO ON DELETE CASCADE:
    // session_notes (V8), transcript_segments (V9), meeting_recaps (V10).
    let session_notes = tx.execute(
        "DELETE FROM session_notes WHERE session_id IN (SELECT id FROM sessions WHERE started_at >= ?1)",
        [cutoff_ts],
    )?;
    tx.execute(
        "DELETE FROM transcript_segments WHERE session_id IN (SELECT id FROM sessions WHERE started_at >= ?1)",
        [cutoff_ts],
    )?;
    tx.execute(
        "DELETE FROM meeting_recaps WHERE session_id IN (SELECT id FROM sessions WHERE started_at >= ?1)",
        [cutoff_ts],
    )?;
    // event_log.session_id (V7) also references sessions with no cascade. A surviving event
    // (ts < cutoff) could still point at a session started in the window; clear that dangling link
    // so the session delete below cannot FK-fail. The event itself is kept.
    tx.execute(
        "UPDATE event_log SET session_id = NULL WHERE session_id IN (SELECT id FROM sessions WHERE started_at >= ?1)",
        [cutoff_ts],
    )?;
    let sessions = tx.execute("DELETE FROM sessions WHERE started_at >= ?1", [cutoff_ts])?;

    // Traceability rows for sends in the window.
    let traceability = tx.execute("DELETE FROM traceability_log WHERE ts >= ?1", [cutoff_ts])?;

    // Orphan sweep: any state row with no surviving provenance is removed (children first).
    let commitments = tx.execute(orphan_sql("commitments"), [])?;
    let open_loops = tx.execute(orphan_sql("open_loops"), [])?;
    let people = tx.execute(orphan_sql("people"), [])?;
    let projects = tx.execute(orphan_sql("projects"), [])?;

    tx.commit()?;

    Ok(DeleteReport {
        events,
        people,
        projects,
        commitments,
        open_loops,
        // threads is a derived cache (titles/summaries/salience) rebuilt by the Dream Cycle from the
        // surviving event log; time-range deletion leaves it to be re-derived rather than time-slicing it.
        // (Not skipped for lack of a timestamp — it has first/last_activity_at.)
        threads: 0,
        sessions,
        session_notes,
        screen_frames,
        traceability,
    })
}

/// DELETE for one state table's rows that have no remaining provenance row.
fn orphan_sql(table: &'static str) -> &'static str {
    match table {
        "commitments" => "DELETE FROM commitments WHERE id NOT IN (SELECT state_id FROM state_provenance WHERE state_table='commitments')",
        "open_loops" => "DELETE FROM open_loops WHERE id NOT IN (SELECT state_id FROM state_provenance WHERE state_table='open_loops')",
        "people" => "DELETE FROM people WHERE id NOT IN (SELECT state_id FROM state_provenance WHERE state_table='people')",
        "projects" => "DELETE FROM projects WHERE id NOT IN (SELECT state_id FROM state_provenance WHERE state_table='projects')",
        _ => unreachable!("unknown state table"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_log::{insert as insert_event, NewEvent};
    use crate::state::{insert_person, CommitmentDirection, CommitmentStatus, NewCommitment, NewPerson, Provenance};

    fn seed(conn: &mut Connection) {
        let e = insert_event(
            conn,
            &NewEvent {
                ts: 1,
                source: "capture",
                kind: "text",
                app_bundle_id: Some("com.apple.Mail"),
                window_title: Some("Inbox"),
                content: "Alice asked for the quarterly report",
                content_hash: "h1",
                dwell_ms: 5,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap();
        let alice = insert_person(conn, &NewPerson { display_name: "Alice", confidence: 0.9, now: 1, ..Default::default() }, &[Provenance::new(e)]).unwrap();
        insert_commitment(
            conn,
            &NewCommitment {
                direction: CommitmentDirection::Mine,
                counterparty_id: Some(alice),
                description: "send the report",
                due_at: Some(100),
                status: CommitmentStatus::Open,
                project_id: None,
                confidence: 0.8,
                now: 1,
            },
            &[Provenance::new(e)],
        )
        .unwrap();
    }

    // shadow the crate import path for the test helper
    use crate::state::insert_commitment;

    #[test]
    fn export_includes_events_and_state_with_content() {
        let mut conn = crate::open_in_memory().unwrap();
        seed(&mut conn);
        let json = export_json(&conn).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["schema_version"], crate::LATEST_SCHEMA_VERSION);
        assert_eq!(v["event_log"].as_array().unwrap().len(), 1);
        assert_eq!(v["event_log"][0]["content"], "Alice asked for the quarterly report");
        assert_eq!(v["people"][0]["display_name"], "Alice");
        assert_eq!(v["commitments"][0]["description"], "send the report");
    }

    #[test]
    fn delete_all_wipes_user_data_but_keeps_schema() {
        let mut conn = crate::open_in_memory().unwrap();
        seed(&mut conn);
        let report = delete_all(&mut conn).unwrap();
        assert_eq!(report.events, 1);
        assert_eq!(report.people, 1);
        assert_eq!(report.commitments, 1);

        // every table is empty...
        for table in ["event_log", "people", "projects", "commitments", "open_loops", "threads", "state_provenance", "traceability_log"] {
            let n: i64 = conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{table} should be empty after delete_all");
        }
        // ...but the schema (and version) survives, so the app keeps working
        assert_eq!(crate::schema_version(&conn).unwrap(), Some(crate::LATEST_SCHEMA_VERSION));
        // a fresh insert still works
        seed(&mut conn);
        let n: i64 = conn.query_row("SELECT count(*) FROM people", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn delete_all_wipes_meeting_sessions() {
        // A session carries the meeting's title, summary and decisions — user data by any
        // reading. "Delete everything" that leaves the record of who met about what would be a
        // privacy failure, not a missed table (FR-SET-07).
        let mut conn = crate::open_in_memory().unwrap();
        let id = crate::session::open(
            &conn,
            &crate::session::NewSession {
                kind: "meeting",
                started_at: 1_000,
                title: Some("Weekly sync"),
                app_bundle_id: Some("us.zoom.xos"),
                calendar_occurrence_id: None,
                confidence: 0.6,
                provenance: "{}",
            },
        )
        .unwrap();
        crate::session::close(&conn, id, 2_000).unwrap();

        delete_all(&mut conn).unwrap();

        let n: i64 =
            conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "sessions must not survive delete_all");
    }

    #[test]
    fn delete_all_wipes_the_notes_the_user_typed_in_meetings() {
        // A meeting note is the user writing in their own words — the most personal row in the
        // database. It must not survive "delete everything", and because it references sessions,
        // forgetting it also breaks the delete outright under foreign_keys=ON (FR-SET-07).
        let mut conn = crate::open_in_memory().unwrap();
        let id = crate::session::open(
            &conn,
            &crate::session::NewSession {
                kind: "meeting",
                started_at: 1_000,
                title: Some("1:1"),
                app_bundle_id: Some("us.zoom.xos"),
                calendar_occurrence_id: None,
                confidence: 0.6,
                provenance: "{}",
            },
        )
        .unwrap();
        crate::session_notes::save(&conn, id, "salary conversation", 1_200).unwrap();

        delete_all(&mut conn).unwrap();

        let n: i64 =
            conn.query_row("SELECT count(*) FROM session_notes", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "meeting notes must not survive delete_all");
    }

    #[test]
    fn export_after_delete_is_empty_collections() {
        let mut conn = crate::open_in_memory().unwrap();
        seed(&mut conn);
        delete_all(&mut conn).unwrap();
        let v: Value = serde_json::from_str(&export_json(&conn).unwrap()).unwrap();
        assert!(v["event_log"].as_array().unwrap().is_empty());
        assert!(v["people"].as_array().unwrap().is_empty());
    }

    #[test]
    fn delete_since_removes_recent_events_and_keeps_older_ones() {
        let mut conn = crate::open_in_memory().unwrap();
        let old = insert_event(&conn, &NewEvent { ts: 1_000, source: "capture", kind: "text",
            app_bundle_id: None, window_title: None, content: "old note", content_hash: "old",
            dwell_ms: 0, display_id: None, window_bounds: None }).unwrap();
        let recent = insert_event(&conn, &NewEvent { ts: 9_000, source: "capture", kind: "text",
            app_bundle_id: None, window_title: None, content: "recent note", content_hash: "new",
            dwell_ms: 0, display_id: None, window_bounds: None }).unwrap();

        let report = delete_since(&mut conn, 5_000).unwrap();
        assert_eq!(report.events, 1, "only the ts>=5000 event is deleted");

        let remaining: Vec<i64> = {
            let mut stmt = conn.prepare("SELECT id FROM event_log ORDER BY id").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
        };
        assert_eq!(remaining, vec![old], "old survives, recent gone");
        let _ = recent;
    }

    #[test]
    fn delete_since_drops_orphaned_state_but_keeps_still_supported_state() {
        let mut conn = crate::open_in_memory().unwrap();
        // A person supported by BOTH an old and a recent event survives; a commitment supported
        // ONLY by the recent event is orphaned and removed.
        let old = insert_event(&conn, &NewEvent { ts: 1_000, source: "capture", kind: "text",
            app_bundle_id: None, window_title: None, content: "met Alice", content_hash: "e-old",
            dwell_ms: 0, display_id: None, window_bounds: None }).unwrap();
        let recent = insert_event(&conn, &NewEvent { ts: 9_000, source: "capture", kind: "text",
            app_bundle_id: None, window_title: None, content: "Alice asked X", content_hash: "e-new",
            dwell_ms: 0, display_id: None, window_bounds: None }).unwrap();
        let alice = insert_person(&mut conn, &NewPerson { display_name: "Alice", confidence: 0.9, now: 1, ..Default::default() },
            &[Provenance::new(old), Provenance::new(recent)]).unwrap();
        insert_commitment(&mut conn, &NewCommitment { direction: CommitmentDirection::Mine,
            counterparty_id: Some(alice), description: "do X", due_at: None,
            status: CommitmentStatus::Open, project_id: None, confidence: 0.8, now: 1 },
            &[Provenance::new(recent)]).unwrap();

        delete_since(&mut conn, 5_000).unwrap();

        let people: i64 = conn.query_row("SELECT count(*) FROM people", [], |r| r.get(0)).unwrap();
        let commitments: i64 = conn.query_row("SELECT count(*) FROM commitments", [], |r| r.get(0)).unwrap();
        assert_eq!(people, 1, "Alice keeps her old evidence, survives");
        assert_eq!(commitments, 0, "commitment lost all evidence, removed");
        // provenance pointing at the deleted event is gone; the old one remains.
        let prov: i64 = conn.query_row("SELECT count(*) FROM state_provenance", [], |r| r.get(0)).unwrap();
        assert_eq!(prov, 1, "only the old-event provenance row survives");
    }

    #[test]
    fn delete_since_deletes_session_children_before_the_session_so_it_does_not_fk_fail() {
        // With foreign_keys=ON, a transcript segment or recap left behind when its session is
        // deleted is a hard FK error that rolls the whole deletion back. Any user who recorded a
        // meeting would then be unable to delete their recent data at all — the whole point.
        let mut conn = crate::open_in_memory().unwrap();
        let sid = crate::session::open(
            &conn,
            &crate::session::NewSession {
                kind: "meeting",
                started_at: 9_000,
                title: Some("Recorded sync"),
                app_bundle_id: Some("us.zoom.xos"),
                calendar_occurrence_id: None,
                confidence: 0.6,
                provenance: "{}",
            },
        )
        .unwrap();
        crate::transcript_segments::append(
            &conn,
            &crate::transcript_segments::NewSegment {
                session_id: sid,
                ts: 9_100,
                speaker: crate::transcript_segments::Speaker::Me,
                text: "hello team",
                confidence: 0.9,
            },
            9_100,
        )
        .unwrap();
        crate::meeting_recaps::save(&conn, sid, "we agreed X", "[]", "[]", "test-model", 9_200)
            .unwrap();

        let report = delete_since(&mut conn, 5_000);
        assert!(report.is_ok(), "delete must not FK-fail with a recorded meeting: {report:?}");

        for table in ["sessions", "transcript_segments", "meeting_recaps"] {
            let n: i64 = conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{table} must be empty after delete_since");
        }
    }

    #[test]
    fn delete_since_deletes_the_event_exactly_at_the_cutoff() {
        // The docstring promises "at or after" (>=). An event whose ts equals the cutoff MUST go,
        // so a future refactor to `>` is caught here.
        let mut conn = crate::open_in_memory().unwrap();
        insert_event(&conn, &NewEvent { ts: 5_000, source: "capture", kind: "text",
            app_bundle_id: None, window_title: None, content: "boundary note", content_hash: "b",
            dwell_ms: 0, display_id: None, window_bounds: None }).unwrap();
        let report = delete_since(&mut conn, 5_000).unwrap();
        assert_eq!(report.events, 1, "ts == cutoff must be deleted (>=, not >)");
        let n: i64 = conn.query_row("SELECT count(*) FROM event_log", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn delete_since_keeps_the_schema_and_fts_in_sync() {
        let mut conn = crate::open_in_memory().unwrap();
        insert_event(&conn, &NewEvent { ts: 9_000, source: "capture", kind: "text",
            app_bundle_id: None, window_title: Some("Inbox"), content: "secret meeting", content_hash: "h",
            dwell_ms: 0, display_id: None, window_bounds: None }).unwrap();
        delete_since(&mut conn, 5_000).unwrap();
        // FTS mirror must not still return the deleted row.
        let hits: i64 = conn.query_row(
            "SELECT count(*) FROM event_fts WHERE event_fts MATCH 'secret'", [], |r| r.get(0)).unwrap();
        assert_eq!(hits, 0, "AD trigger cleared the FTS row");
    }
}
