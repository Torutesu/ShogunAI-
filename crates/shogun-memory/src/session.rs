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
        row,
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
    conn.query_row(&format!("SELECT {COLS} FROM sessions WHERE id = ?1"), [id], row)
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

/// Sessions whose `started_at` falls in `[from_ts, to_ts]` — the window the Dream Cycle
/// Compression job summarises (Issue #63), the interval analogue of [`crate::thread::active_between`].
/// Inclusive on both ends so a session opened exactly on a window edge is still summarised. Ordered
/// oldest-first for deterministic processing.
pub fn active_between(
    conn: &Connection,
    from_ts: i64,
    to_ts: i64,
) -> Result<Vec<i64>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id FROM sessions WHERE started_at BETWEEN ?1 AND ?2 ORDER BY started_at, id",
    )?;
    let rows = stmt.query_map(params![from_ts, to_ts], |r| r.get::<_, i64>(0))?;
    rows.collect()
}

/// Every event body attached to one session, oldest first — the material the Compression summariser
/// reads (Issue #63). Mirrors [`crate::thread::event_texts`] but keys on `event_log.session_id`.
pub fn event_texts(
    conn: &Connection,
    session_id: i64,
) -> Result<Vec<crate::event_log::EventText>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, content FROM event_log WHERE session_id = ?1 ORDER BY ts, id",
    )?;
    let rows = stmt.query_map(params![session_id], |r| {
        Ok(crate::event_log::EventText { id: r.get(0)?, content: r.get(1)? })
    })?;
    rows.collect()
}

/// Write a session's summary (Issue #63). Like [`crate::thread::set_summary`], the summary is
/// generated content, so it is redacted on write — a summariser could echo a secret that was in the
/// source events. `updated_at` advances so a re-summarised session reads as touched.
pub fn set_summary(
    conn: &Connection,
    session_id: i64,
    summary: &str,
    now_ms: i64,
) -> Result<(), rusqlite::Error> {
    let redacted = crate::redact::redact(summary);
    conn.execute(
        "UPDATE sessions SET summary = ?1, updated_at = ?2 WHERE id = ?3",
        params![redacted.as_ref(), now_ms, session_id],
    )?;
    Ok(())
}

/// Read back a session's summary (`None` when unset or the session is absent) — the Compression
/// job's effect is verified through this, since [`Session`] does not carry `summary`.
pub fn get_summary(conn: &Connection, session_id: i64) -> Result<Option<String>, rusqlite::Error> {
    conn.query_row("SELECT summary FROM sessions WHERE id = ?1", [session_id], |r| r.get(0))
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
}

/// The DISTINCT sessions owning the given events — the query-time consume path (Issue #63): given
/// the events retrieved for a query, find which sessions to pull summaries from. An empty input
/// yields an empty result without ever building an `IN ()`, which is not valid SQL.
pub fn session_ids_for_events(
    conn: &Connection,
    event_ids: &[i64],
) -> Result<Vec<i64>, rusqlite::Error> {
    if event_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; event_ids.len()].join(",");
    // ORDER BY session_id keeps the consume path deterministic (Issue #63): the same query
    // retrieving the same events always visits the owning sessions in a stable order.
    let sql = format!(
        "SELECT DISTINCT session_id FROM event_log \
          WHERE id IN ({placeholders}) AND session_id IS NOT NULL \
          ORDER BY session_id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let params = rusqlite::params_from_iter(event_ids.iter());
    let rows = stmt.query_map(params, |r| r.get::<_, i64>(0))?;
    rows.collect()
}

/// The saved summaries of the given sessions, in one query — the batched analogue of calling
/// [`get_summary`] per id (Issue #63). Returns `(id, thread_key, summary)` for every session in
/// `ids` that HAS a non-null `summary`; sessions without a summary are simply absent. `thread_key`
/// is returned so a caller can dedup a session summary against an already-consumed thread summary
/// of the same conversation. Ordered by `id` for a deterministic consume order. An empty input
/// yields an empty Vec without ever building an `IN ()`, which is not valid SQL.
pub fn summaries_for_sessions(
    conn: &Connection,
    ids: &[i64],
) -> Result<Vec<(i64, String, Option<String>)>, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    // COALESCE the nullable thread_key to '' so a session with no conversation identity still
    // yields a plain String (empty ⇒ never matches a consumed thread_key, so it is kept).
    let sql = format!(
        "SELECT id, COALESCE(thread_key, ''), summary FROM sessions \
          WHERE id IN ({placeholders}) AND summary IS NOT NULL \
          ORDER BY id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let params = rusqlite::params_from_iter(ids.iter());
    let rows = stmt.query_map(params, |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
    })?;
    rows.collect()
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

    fn attach_ev(conn: &Connection, session_id: i64, content: &str, hash: &str, ts: i64) -> i64 {
        let ev = crate::event_log::NewEvent {
            ts,
            source: "capture",
            kind: "text",
            app_bundle_id: Some("us.zoom.xos"),
            window_title: Some("Zoom Meeting"),
            content,
            content_hash: hash,
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        };
        let ev_id = crate::event_log::insert(conn, &ev).unwrap();
        attach_event(conn, session_id, ev_id).unwrap();
        ev_id
    }

    #[test]
    fn active_between_is_inclusive_and_ordered_and_summary_round_trips() {
        let conn = crate::open_in_memory().unwrap();
        let a = open(&conn, &meeting(100)).unwrap();
        let b = open(&conn, &meeting(300)).unwrap();
        let _c = open(&conn, &meeting(500)).unwrap();

        // [100, 300] is inclusive on both ends → sessions a and b, oldest-first.
        let got = active_between(&conn, 100, 300).unwrap();
        assert_eq!(got, vec![a, b], "inclusive on both ends, ordered oldest-first");

        // event_texts returns the session's attached bodies in ts,id order.
        attach_ev(&conn, a, "first session body", "h1", 110);
        attach_ev(&conn, a, "second session body", "h2", 120);
        let texts = event_texts(&conn, a).unwrap();
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0].content, "first session body");
        assert_eq!(texts[1].content, "second session body");

        // set_summary writes it, get_summary reads it back; Session does not expose it.
        assert_eq!(get_summary(&conn, a).unwrap(), None, "unset until written");
        set_summary(&conn, a, "a day summary", 9_999).unwrap();
        assert_eq!(get_summary(&conn, a).unwrap().as_deref(), Some("a day summary"));
        // An absent session yields None, not an error.
        assert_eq!(get_summary(&conn, 99_999).unwrap(), None);
    }

    #[test]
    fn set_summary_redacts_generated_text() {
        let conn = crate::open_in_memory().unwrap();
        let id = open(&conn, &meeting(100)).unwrap();
        set_summary(&conn, id, "leaked sk-ant-abc123def456 key", 1).unwrap();
        let stored = get_summary(&conn, id).unwrap().unwrap();
        assert!(!stored.contains("sk-ant-abc123def456"), "a secret must not survive into the summary");
    }

    #[test]
    fn session_ids_for_events_returns_owning_sessions_and_handles_empty() {
        let conn = crate::open_in_memory().unwrap();
        let a = open(&conn, &meeting(100)).unwrap();
        let b = open(&conn, &meeting(300)).unwrap();
        let ea1 = attach_ev(&conn, a, "a1", "h1", 110);
        let ea2 = attach_ev(&conn, a, "a2", "h2", 120);
        let eb1 = attach_ev(&conn, b, "b1", "h3", 310);
        // An unattached event: present in the log but owned by no session.
        let orphan = crate::event_log::insert(
            &conn,
            &crate::event_log::NewEvent {
                ts: 400,
                source: "capture",
                kind: "text",
                app_bundle_id: None,
                window_title: None,
                content: "orphan",
                content_hash: "h4",
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap();

        // Empty input → empty Vec (no empty IN () built).
        assert!(session_ids_for_events(&conn, &[]).unwrap().is_empty());

        // Two events of a, one of b → the two DISTINCT owning sessions.
        let mut got = session_ids_for_events(&conn, &[ea1, ea2, eb1]).unwrap();
        got.sort_unstable();
        let mut want = vec![a, b];
        want.sort_unstable();
        assert_eq!(got, want, "DISTINCT owning sessions");

        // An orphan event contributes nothing (session_id IS NULL is filtered).
        assert!(session_ids_for_events(&conn, &[orphan]).unwrap().is_empty());

        // Deterministic: DISTINCT owning sessions come back in session_id order regardless of the
        // order the event ids are passed in.
        let ordered = session_ids_for_events(&conn, &[eb1, ea2, ea1]).unwrap();
        assert_eq!(ordered, vec![a, b], "ORDER BY session_id makes the result stable");
    }

    #[test]
    fn summaries_for_sessions_batches_and_returns_thread_key() {
        let conn = crate::open_in_memory().unwrap();
        let a = open(&conn, &meeting(100)).unwrap();
        let b = open(&conn, &meeting(300)).unwrap();
        let _c = open(&conn, &meeting(500)).unwrap();

        // a has a summary and a thread_key; b has a summary but no thread_key; c has no summary.
        set_summary(&conn, a, "a summary", 1_000).unwrap();
        conn.execute("UPDATE sessions SET thread_key = ?1 WHERE id = ?2", params!["mail:t1", a])
            .unwrap();
        set_summary(&conn, b, "b summary", 1_000).unwrap();

        // Empty input → empty Vec (no empty IN () built).
        assert!(summaries_for_sessions(&conn, &[]).unwrap().is_empty());

        // Only sessions WITH a non-null summary come back, ordered by id, each carrying its
        // thread_key ('' when NULL) and its summary.
        let got = summaries_for_sessions(&conn, &[b, a, _c]).unwrap();
        assert_eq!(
            got,
            vec![
                (a, "mail:t1".to_string(), Some("a summary".to_string())),
                (b, String::new(), Some("b summary".to_string())),
            ],
            "batched, ORDER BY id, summary-only, thread_key coalesced to ''"
        );
    }
}
