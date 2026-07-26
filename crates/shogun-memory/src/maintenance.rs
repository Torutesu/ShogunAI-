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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeleteReport {
    pub events: usize,
    pub people: usize,
    pub projects: usize,
    pub commitments: usize,
    pub open_loops: usize,
    pub threads: usize,
    pub sessions: usize,
    pub session_notes: usize,
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
    // Meeting notes are the user's own words — the most personal rows here — and they reference
    // sessions, so they go before them (FR-MT-10).
    let session_notes = tx.execute("DELETE FROM session_notes", [])?;
    // Sessions hold the meeting's title, summary and decisions — user data, and referenced by
    // event_log, so they go after it (FR-SET-07, FR-MT-05).
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
        traceability,
    })
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
}
