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
    // The rest of what the app holds about the user: threads (titles + summaries), meeting
    // sessions, the user's own meeting notes, transcripts, recaps, the provenance links that let
    // them verify any state claim, and the traceability log (digest-only by construction). An
    // export that omitted these would not be "export it all" (FR-SET-07).
    let threads = rows(conn, "SELECT id, thread_key, title, summary, participants, first_activity_at, last_activity_at, event_count FROM threads ORDER BY id", |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?, "thread_key": r.get::<_, String>(1)?,
            "title": r.get::<_, Option<String>>(2)?, "summary": r.get::<_, Option<String>>(3)?,
            "participants": r.get::<_, Option<String>>(4)?,
            "first_activity_at": r.get::<_, i64>(5)?, "last_activity_at": r.get::<_, i64>(6)?,
            "event_count": r.get::<_, i64>(7)?,
        }))
    })?;
    let sessions = rows(conn, "SELECT id, kind, started_at, ended_at, title, participants, summary, decisions FROM sessions ORDER BY id", |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?,
            "started_at": r.get::<_, i64>(2)?, "ended_at": r.get::<_, Option<i64>>(3)?,
            "title": r.get::<_, Option<String>>(4)?, "participants": r.get::<_, Option<String>>(5)?,
            "summary": r.get::<_, Option<String>>(6)?, "decisions": r.get::<_, Option<String>>(7)?,
        }))
    })?;
    let session_notes = rows(conn, "SELECT session_id, body, updated_at FROM session_notes ORDER BY session_id", |r| {
        Ok(json!({ "session_id": r.get::<_, i64>(0)?, "body": r.get::<_, String>(1)?, "updated_at": r.get::<_, i64>(2)? }))
    })?;
    let transcript_segments = rows(conn, "SELECT id, session_id, ts, speaker, text, origin, confidence FROM transcript_segments ORDER BY id", |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?, "session_id": r.get::<_, i64>(1)?, "ts": r.get::<_, i64>(2)?,
            "speaker": r.get::<_, Option<String>>(3)?, "text": r.get::<_, String>(4)?,
            "origin": r.get::<_, String>(5)?, "confidence": r.get::<_, Option<f64>>(6)?,
        }))
    })?;
    let meeting_recaps = rows(conn, "SELECT session_id, summary, decisions, next_actions, model, created_at FROM meeting_recaps ORDER BY session_id", |r| {
        Ok(json!({
            "session_id": r.get::<_, i64>(0)?, "summary": r.get::<_, Option<String>>(1)?,
            "decisions": r.get::<_, Option<String>>(2)?, "next_actions": r.get::<_, Option<String>>(3)?,
            "model": r.get::<_, Option<String>>(4)?, "created_at": r.get::<_, i64>(5)?,
        }))
    })?;
    let state_provenance = rows(conn, "SELECT state_table, state_id, event_id, weight FROM state_provenance ORDER BY state_table, state_id, event_id", |r| {
        Ok(json!({
            "state_table": r.get::<_, String>(0)?, "state_id": r.get::<_, i64>(1)?,
            "event_id": r.get::<_, i64>(2)?, "weight": r.get::<_, f64>(3)?,
        }))
    })?;
    let traceability = rows(conn, "SELECT id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party FROM traceability_log ORDER BY id", |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?, "ts": r.get::<_, i64>(1)?, "route": r.get::<_, String>(2)?,
            "purpose": r.get::<_, String>(3)?, "destination": r.get::<_, String>(4)?,
            "chunk_bytes": r.get::<_, i64>(5)?, "chunk_xxh64": r.get::<_, String>(6)?,
            "third_party": r.get::<_, i64>(7)? != 0,
        }))
    })?;

    // Briefs (V15) and distilled lessons (V16) are the user's data too. Raw feedback_events /
    // lesson_provenance stay out by design — V16 marks them "local DB only; never exported".
    let briefs = rows(conn, "SELECT date, payload, generated, built_at FROM briefs ORDER BY date", |r| {
        Ok(json!({
            "date": r.get::<_, String>(0)?, "payload": r.get::<_, String>(1)?,
            "generated": r.get::<_, i64>(2)? != 0, "built_at": r.get::<_, i64>(3)?,
        }))
    })?;
    let lessons = rows(conn, "SELECT id, kind, scope, scope_ref, instruction, confidence, evidence_count, active, created_at, updated_at, last_evidence_at FROM lessons ORDER BY id", |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?,
            "scope": r.get::<_, String>(2)?, "scope_ref": r.get::<_, Option<String>>(3)?,
            "instruction": r.get::<_, String>(4)?, "confidence": r.get::<_, f64>(5)?,
            "evidence_count": r.get::<_, i64>(6)?, "active": r.get::<_, i64>(7)? != 0,
            "created_at": r.get::<_, i64>(8)?, "updated_at": r.get::<_, i64>(9)?,
            "last_evidence_at": r.get::<_, i64>(10)?,
        }))
    })?;

    let doc = json!({
        "schema_version": crate::schema_version(conn)?,
        "event_log": events,
        "people": people,
        "projects": projects,
        "commitments": commitments,
        "open_loops": open_loops,
        "threads": threads,
        "sessions": sessions,
        "session_notes": session_notes,
        "transcript_segments": transcript_segments,
        "meeting_recaps": meeting_recaps,
        "state_provenance": state_provenance,
        "traceability_log": traceability,
        "briefs": briefs,
        "lessons": lessons,
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
    pub transcript_segments: usize,
    pub meeting_recaps: usize,
    pub screen_frames: usize,
    pub traceability: usize,
    pub briefs: usize,
    pub feedback_events: usize,
    pub lessons: usize,
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
    // Visual-recall frames reference event_log (V12, no ON DELETE), so they must go before the
    // event log — with foreign_keys=ON a lingering frame FK-fails the event delete and rolls the
    // whole "delete everything" back for anyone who ever enabled Visual recall (FR-SET-07).
    let screen_frames = tx.execute("DELETE FROM screen_frames", [])?;
    // embeddings (Warm vec0 + Cold int8) then the event log (its AD trigger clears event_fts)
    tx.execute("DELETE FROM event_vec", [])?;
    tx.execute("DELETE FROM cold_embeddings", [])?;
    let events = tx.execute("DELETE FROM event_log", [])?;
    // Everything that references sessions with no ON DELETE CASCADE must go before the sessions
    // themselves, or with foreign_keys=ON the delete FK-fails and rolls back. Meeting notes
    // (V8, the user's own words — the most personal rows here, FR-MT-10), transcript segments
    // (V9), and meeting recaps (V10) all reference sessions.
    let session_notes = tx.execute("DELETE FROM session_notes", [])?;
    // Transcripts (what was said, FR-MT-13) and recaps (what the model concluded) both hold
    // `NOT NULL REFERENCES sessions(id)` with no ON DELETE clause, so under foreign_keys=ON they
    // must go before sessions — forgetting either aborts the whole transaction and "delete
    // everything" deletes nothing (FR-SET-07).
    let transcript_segments = tx.execute("DELETE FROM transcript_segments", [])?;
    let meeting_recaps = tx.execute("DELETE FROM meeting_recaps", [])?;
    // Sessions hold the meeting's title, summary and decisions — user data. event_log also
    // references sessions, and it was already cleared above, so sessions can go now (FR-SET-07,
    // FR-MT-05).
    let sessions = tx.execute("DELETE FROM sessions", [])?;
    let traceability = tx.execute("DELETE FROM traceability_log", [])?;
    // L5 learning data (V16) and persisted briefs (V15). feedback_events.before_text/after_text
    // hold the user's actual proposed and approved message bodies — the most personal rows here.
    // Provenance goes before both of its parents (FK order).
    tx.execute("DELETE FROM lesson_provenance", [])?;
    let lessons = tx.execute("DELETE FROM lessons", [])?;
    let feedback_events = tx.execute("DELETE FROM feedback_events", [])?;
    // feedback_events.id carries no AUTOINCREMENT, so ids restart at 1 after the wipe. The
    // distill watermark is monotonic (MAX on advance) and would sit above every future id,
    // silently ending lesson learning forever — rewind it with the data it indexed.
    tx.execute(
        "UPDATE lesson_distill_meta SET last_processed_feedback_id = 0 WHERE id = 1",
        [],
    )?;
    let briefs = tx.execute("DELETE FROM briefs", [])?;
    tx.execute("DELETE FROM job_runs", [])?;
    // Query-hash metrics carry no content, but they are still records of the user's activity.
    tx.execute("DELETE FROM compression_metrics", [])?;
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
        transcript_segments,
        meeting_recaps,
        screen_frames,
        traceability,
        briefs,
        feedback_events,
        lessons,
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
    let transcript_segments = tx.execute(
        "DELETE FROM transcript_segments WHERE session_id IN (SELECT id FROM sessions WHERE started_at >= ?1)",
        [cutoff_ts],
    )?;
    let meeting_recaps = tx.execute(
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

    // L5 learning signals in the window (V16): provenance rows pointing at doomed feedback first
    // (FK), then the feedback rows; a lesson that loses ALL of its evidence goes with them — the
    // same orphan rule as the state tables. Briefs are keyed by build time (V15).
    tx.execute(
        "DELETE FROM lesson_provenance WHERE feedback_event_id IN (SELECT id FROM feedback_events WHERE ts_ms >= ?1)",
        [cutoff_ts],
    )?;
    let feedback_events = tx.execute("DELETE FROM feedback_events WHERE ts_ms >= ?1", [cutoff_ts])?;
    // The deleted rows are the newest (highest ids), and ids are reused after deletion (no
    // AUTOINCREMENT). Clamp the monotonic distill watermark down to the highest surviving id so
    // re-issued ids are not skipped as already-processed.
    tx.execute(
        "UPDATE lesson_distill_meta
             SET last_processed_feedback_id = MIN(
                 last_processed_feedback_id,
                 COALESCE((SELECT MAX(id) FROM feedback_events), 0))
           WHERE id = 1",
        [],
    )?;
    let lessons = tx.execute(
        "DELETE FROM lessons WHERE id NOT IN (SELECT lesson_id FROM lesson_provenance)",
        [],
    )?;
    let briefs = tx.execute("DELETE FROM briefs WHERE built_at >= ?1", [cutoff_ts])?;

    // Orphan sweep: any state row with no surviving provenance is removed (children first).
    let commitments = sweep_orphans(&tx, "commitments")?;
    let open_loops = sweep_orphans(&tx, "open_loops")?;
    // A SURVIVING commitment/open_loop/thread can still reference a person/project that just
    // lost its last provenance (their evidence lives in different events). Null those links
    // before the parent sweep: with foreign_keys=ON and no ON DELETE clause, the sweep would
    // otherwise FK-fail and roll back the entire deletion.
    tx.execute(
        "UPDATE commitments SET counterparty_id = NULL WHERE counterparty_id IN
             (SELECT id FROM people WHERE id NOT IN
                 (SELECT state_id FROM state_provenance WHERE state_table='people'))",
        [],
    )?;
    tx.execute(
        "UPDATE open_loops SET counterparty_id = NULL WHERE counterparty_id IN
             (SELECT id FROM people WHERE id NOT IN
                 (SELECT state_id FROM state_provenance WHERE state_table='people'))",
        [],
    )?;
    for table in ["commitments", "open_loops", "threads"] {
        tx.execute(
            &format!(
                "UPDATE {table} SET project_id = NULL WHERE project_id IN
                     (SELECT id FROM projects WHERE id NOT IN
                         (SELECT state_id FROM state_provenance WHERE state_table='projects'))"
            ),
            [],
        )?;
    }
    let people = sweep_orphans(&tx, "people")?;
    let projects = sweep_orphans(&tx, "projects")?;

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
        transcript_segments,
        meeting_recaps,
        screen_frames,
        traceability,
        briefs,
        feedback_events,
        lessons,
    })
}

/// DELETE for one state table's rows that have no remaining provenance row. `None` for a table
/// this sweep does not know — a fifth state table added without touching here should be skipped,
/// not panic the maintenance thread mid-transaction (the caller treats it as "nothing to sweep").
fn orphan_sql(table: &str) -> Option<&'static str> {
    Some(match table {
        "commitments" => "DELETE FROM commitments WHERE id NOT IN (SELECT state_id FROM state_provenance WHERE state_table='commitments')",
        "open_loops" => "DELETE FROM open_loops WHERE id NOT IN (SELECT state_id FROM state_provenance WHERE state_table='open_loops')",
        "people" => "DELETE FROM people WHERE id NOT IN (SELECT state_id FROM state_provenance WHERE state_table='people')",
        "projects" => "DELETE FROM projects WHERE id NOT IN (SELECT state_id FROM state_provenance WHERE state_table='projects')",
        _ => return None,
    })
}

/// Run the orphan sweep for `table`, or 0 when the table is unknown to [`orphan_sql`].
fn sweep_orphans(tx: &rusqlite::Transaction<'_>, table: &str) -> Result<usize, rusqlite::Error> {
    match orphan_sql(table) {
        Some(sql) => tx.execute(sql, []),
        None => Ok(0),
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
    fn delete_all_survives_a_meeting_with_transcript_and_recap() {
        // The regression that motivated this test: transcript_segments and meeting_recaps both
        // hold NOT NULL FKs to sessions with no ON DELETE. Deleting sessions first aborted the
        // transaction, so "delete everything" deleted nothing at all for anyone who had ever
        // held a meeting.
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
        crate::transcript_segments::append(
            &conn,
            &crate::transcript_segments::NewSegment {
                session_id: id,
                ts: 1_100,
                speaker: crate::transcript_segments::Speaker::Unknown,
                text: "we agreed on the renewal",
                confidence: 0.9,
            },
            1_100,
        )
        .unwrap();
        crate::meeting_recaps::save(&conn, id, "renewal agreed", "[]", "[]", "test", 1_300).unwrap();
        crate::session::close(&conn, id, 2_000).unwrap();

        let report = delete_all(&mut conn).expect("delete_all must not trip the session FKs");
        assert_eq!(report.sessions, 1);
        assert_eq!(report.transcript_segments, 1);
        assert_eq!(report.meeting_recaps, 1);
        for table in ["sessions", "transcript_segments", "meeting_recaps", "session_notes"] {
            let n: i64 = conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{table} should be empty after delete_all");
        }
    }

    #[test]
    fn delete_all_survives_a_visual_recall_frame() {
        // screen_frames.event_id → event_log with no ON DELETE (V12). Deleting event_log first
        // FK-failed and rolled the whole transaction back — "delete everything" deleted nothing
        // for anyone who had ever enabled Visual recall.
        let mut conn = crate::open_in_memory().unwrap();
        seed(&mut conn);
        let event_id: i64 = conn.query_row("SELECT id FROM event_log LIMIT 1", [], |r| r.get(0)).unwrap();
        crate::screen_frames::insert(
            &conn,
            &crate::screen_frames::NewFrame {
                created_at_ms: 2,
                event_id,
                app_bundle_id: Some("com.apple.Mail"),
                window_title: Some("Inbox"),
                display_id: None,
                width: 100,
                height: 100,
                jpeg: b"\xff\xd8fake",
            },
        )
        .unwrap();

        let report = delete_all(&mut conn).expect("delete_all must not trip the frame FK");
        assert_eq!(report.screen_frames, 1);
        for table in ["screen_frames", "event_log"] {
            let n: i64 = conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{table} should be empty after delete_all");
        }
    }

    #[test]
    fn delete_all_rewinds_the_lesson_distill_watermark() {
        // feedback ids restart after a wipe (no AUTOINCREMENT); a surviving watermark would sit
        // above every future id and lesson learning would silently never run again.
        let mut conn = crate::open_in_memory().unwrap();
        crate::lessons::set_distill_watermark(&conn, 100).unwrap();
        delete_all(&mut conn).unwrap();
        assert_eq!(crate::lessons::distill_watermark(&conn).unwrap(), 0);
    }

    #[test]
    fn delete_since_survives_a_surviving_commitment_referencing_an_orphaned_person() {
        // Alice's only evidence is a RECENT event; the commitment's evidence is an OLD event and
        // it references Alice. delete_since(recent) orphans Alice while the commitment survives —
        // the people sweep used to FK-fail on that link and roll back the entire deletion.
        let mut conn = crate::open_in_memory().unwrap();
        let old_event = insert_event(
            &conn,
            &NewEvent {
                ts: 1,
                source: "capture",
                kind: "text",
                app_bundle_id: None,
                window_title: None,
                content: "old evidence",
                content_hash: "old1",
                dwell_ms: 5,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap();
        let recent_event = insert_event(
            &conn,
            &NewEvent {
                ts: 1_000,
                source: "capture",
                kind: "text",
                app_bundle_id: None,
                window_title: None,
                content: "recent evidence",
                content_hash: "new1",
                dwell_ms: 5,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap();
        let alice = insert_person(
            &mut conn,
            &NewPerson { display_name: "Alice", confidence: 0.9, now: 1_000, ..Default::default() },
            &[Provenance::new(recent_event)],
        )
        .unwrap();
        insert_commitment(
            &mut conn,
            &NewCommitment {
                direction: CommitmentDirection::Mine,
                counterparty_id: Some(alice),
                description: "send Alice the report",
                due_at: None,
                status: CommitmentStatus::Open,
                project_id: None,
                confidence: 0.8,
                now: 1,
            },
            &[Provenance::new(old_event)],
        )
        .unwrap();

        let report = delete_since(&mut conn, 500).expect("delete_since must not trip the people FK");
        assert_eq!(report.events, 1, "only the recent event goes");
        assert_eq!(report.people, 1, "Alice lost all her evidence");
        // The surviving commitment is kept, with its dangling link cleared.
        let (n, counterparty): (i64, Option<i64>) = conn
            .query_row("SELECT count(*), counterparty_id FROM commitments", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(counterparty, None);
    }

    #[test]
    fn delete_since_clamps_the_lesson_distill_watermark_to_surviving_ids() {
        use crate::lessons::{record_feedback, FeedbackKind, LessonScope, NewFeedback};
        let mut conn = crate::open_in_memory().unwrap();
        let old = record_feedback(
            &conn,
            FeedbackKind::Reject,
            LessonScope::Global,
            &NewFeedback { ts_ms: 10, ..Default::default() },
        )
        .unwrap();
        let newest = record_feedback(
            &conn,
            FeedbackKind::Reject,
            LessonScope::Global,
            &NewFeedback { ts_ms: 1_000, ..Default::default() },
        )
        .unwrap();
        crate::lessons::set_distill_watermark(&conn, newest).unwrap();

        delete_since(&mut conn, 500).unwrap();

        // The newest (highest-id) feedback died; its id will be reused. The watermark must fall
        // back to the highest surviving id so the reissued id is not skipped.
        assert_eq!(crate::lessons::distill_watermark(&conn).unwrap(), old);
    }

    #[test]
    fn export_covers_meetings_notes_transcripts_and_provenance() {
        let mut conn = crate::open_in_memory().unwrap();
        seed(&mut conn);
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
        crate::session_notes::save(&conn, id, "my own words", 1_200).unwrap();
        crate::transcript_segments::append(
            &conn,
            &crate::transcript_segments::NewSegment {
                session_id: id,
                ts: 1_100,
                speaker: crate::transcript_segments::Speaker::Unknown,
                text: "spoken words",
                confidence: 0.9,
            },
            1_100,
        )
        .unwrap();

        let v: Value = serde_json::from_str(&export_json(&conn).unwrap()).unwrap();
        assert_eq!(v["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(v["session_notes"][0]["body"], "my own words");
        assert_eq!(v["transcript_segments"][0]["text"], "spoken words");
        assert!(!v["state_provenance"].as_array().unwrap().is_empty(), "provenance links must export");
        assert!(v["traceability_log"].as_array().unwrap().is_empty());
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
