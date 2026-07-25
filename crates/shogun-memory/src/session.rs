//! Sessions: the *interval* the event log cannot express (FR-MT-05).
//!
//! `event_log` records points — a capture, a mail, a chat line. A meeting is an interval, and
//! "what was decided in that half hour" has nowhere to live in a log of points. `sessions` is one
//! layer above the log: a half-open span `[started_at, ended_at)` that events attach to via
//! `event_log.session_id`.
//!
//! Detection is inference, so a session carries `confidence` + `provenance` under the same rule as
//! every state row (FR-MT-04): a detected meeting is never stated as fact.
//!
//! `kind` is deliberately wider than `meeting` — `focus` gives "what was I doing for the last
//! thirty minutes" the same container, so meetings are the first application of the interval
//! rather than a special case in the schema.

use rusqlite::{params, Connection};

/// A session to open. `ended_at` is not part of this: an interval is opened without knowing when
/// it closes, which is the whole reason detection needs [`close`].
#[derive(Debug, Clone)]
pub struct NewSession<'a> {
    /// 'meeting' | 'call' | 'focus' — see the CHECK in V7.
    pub kind: &'a str,
    pub started_at: i64,
    /// Calendar title when the session is tied to an occurrence, else the window title that
    /// triggered detection. NULL when neither is known — not guessed.
    pub title: Option<&'a str>,
    pub app_bundle_id: Option<&'a str>,
    /// Set when detection signal ① (a calendar occurrence) agrees with ②/③ (FR-MT-04).
    pub calendar_occurrence_id: Option<i64>,
    pub confidence: f64,
    /// JSON describing which detection signals fired. The evidence for the confidence above.
    pub provenance: &'a str,
}

/// An open or closed interval as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: i64,
    pub kind: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub title: Option<String>,
    pub app_bundle_id: Option<String>,
    pub calendar_occurrence_id: Option<i64>,
    pub confidence: f64,
}

const COLS: &str = "id, kind, started_at, ended_at, title, app_bundle_id, \
                    calendar_occurrence_id, confidence";

fn row(r: &rusqlite::Row<'_>) -> Result<Session, rusqlite::Error> {
    Ok(Session {
        id: r.get(0)?,
        kind: r.get(1)?,
        started_at: r.get(2)?,
        ended_at: r.get(3)?,
        title: r.get(4)?,
        app_bundle_id: r.get(5)?,
        calendar_occurrence_id: r.get(6)?,
        confidence: r.get(7)?,
    })
}

/// Open an interval. Returns the new row id.
pub fn open(conn: &Connection, s: &NewSession<'_>) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO sessions
           (kind, started_at, title, app_bundle_id, calendar_occurrence_id, confidence,
            provenance, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?2, ?2)",
        params![
            s.kind,
            s.started_at,
            s.title,
            s.app_bundle_id,
            s.calendar_occurrence_id,
            s.confidence,
            s.provenance,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// The currently open session, if any — the one row with `ended_at IS NULL`.
pub fn active(conn: &Connection) -> Result<Option<Session>, rusqlite::Error> {
    conn.query_row(
        &format!("SELECT {COLS} FROM sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1"),
        [],
        |r| row(r),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
}

/// Close an interval at `ended_at`.
///
/// Idempotent by construction: `WHERE ended_at IS NULL` means the *first* close wins. Auto-wrap
/// (FR-MT-11) and the user's Stop can fire on the same meeting — the app quits just as the user
/// reaches for the button — and an interval that has already finished must not be extended by
/// whichever signal arrives second.
pub fn close(conn: &Connection, id: i64, ended_at: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE sessions SET ended_at = ?1, updated_at = ?1 WHERE id = ?2 AND ended_at IS NULL",
        params![ended_at, id],
    )?;
    Ok(())
}

/// Read one session by id, open or closed.
pub fn get(conn: &Connection, id: i64) -> Result<Option<Session>, rusqlite::Error> {
    conn.query_row(&format!("SELECT {COLS} FROM sessions WHERE id = ?1"), [id], |r| row(r))
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
}

/// Attach an already-recorded event to an interval.
///
/// The event is written first and attached second, rather than the log taking a session id at
/// insert time: capture must not have to know whether a meeting is running, and an event that
/// fails to attach is still a durable event.
pub fn attach_event(
    conn: &Connection,
    session_id: i64,
    event_id: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE event_log SET session_id = ?1 WHERE id = ?2",
        params![session_id, event_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meeting(started_at: i64) -> NewSession<'static> {
        NewSession {
            kind: "meeting",
            started_at,
            title: Some("Weekly sync"),
            app_bundle_id: Some("us.zoom.xos"),
            calendar_occurrence_id: None,
            confidence: 0.6,
            provenance: r#"{"signals":["app_foreground","mic_in_use"]}"#,
        }
    }

    #[test]
    fn opening_a_session_makes_it_the_active_one() {
        let conn = crate::open_in_memory().unwrap();
        let id = open(&conn, &meeting(1_000)).unwrap();

        let got = active(&conn).unwrap().expect("an opened session must be active");
        assert_eq!(got.id, id);
        assert_eq!(got.started_at, 1_000);
        assert_eq!(got.ended_at, None, "an open interval has no end yet");
    }

    #[test]
    fn closing_a_session_leaves_no_active_one() {
        let conn = crate::open_in_memory().unwrap();
        let id = open(&conn, &meeting(1_000)).unwrap();

        close(&conn, id, 5_000).unwrap();

        assert_eq!(active(&conn).unwrap(), None);
    }

    #[test]
    fn closing_records_the_end_of_the_interval() {
        let conn = crate::open_in_memory().unwrap();
        let id = open(&conn, &meeting(1_000)).unwrap();

        close(&conn, id, 5_000).unwrap();

        let got = get(&conn, id).unwrap().expect("a closed session is still readable");
        assert_eq!(got.ended_at, Some(5_000));
        assert_eq!(got.started_at, 1_000, "closing must not move the start");
    }

    #[test]
    fn closing_an_already_closed_session_does_not_move_its_end() {
        // Auto-wrap (FR-MT-11) and a user's Stop can race: the meeting app quits at the same
        // moment the user hits Stop. The first end is the true one — the second must not extend
        // an interval that has already finished.
        let conn = crate::open_in_memory().unwrap();
        let id = open(&conn, &meeting(1_000)).unwrap();

        close(&conn, id, 5_000).unwrap();
        close(&conn, id, 9_000).unwrap();

        assert_eq!(get(&conn, id).unwrap().unwrap().ended_at, Some(5_000));
    }

    #[test]
    fn events_recorded_during_a_session_are_attached_to_it() {
        let conn = crate::open_in_memory().unwrap();
        let id = open(&conn, &meeting(1_000)).unwrap();
        let ev = crate::event_log::NewEvent {
            ts: 1_500,
            source: "capture",
            kind: "text",
            app_bundle_id: Some("us.zoom.xos"),
            window_title: Some("Zoom Meeting"),
            content: "shared the roadmap",
            content_hash: "h1",
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        };
        let ev_id = crate::event_log::insert(&conn, &ev).unwrap();

        attach_event(&conn, id, ev_id).unwrap();

        let got: Option<i64> = conn
            .query_row("SELECT session_id FROM event_log WHERE id = ?1", [ev_id], |r| r.get(0))
            .unwrap();
        assert_eq!(got, Some(id));
    }
}
