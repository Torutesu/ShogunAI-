//! The user's own notes during a meeting (FR-MT-10).
//!
//! The expanded panel during a meeting is a place to type, not a transcript to watch. What the
//! user writes is the one part of a meeting record that is unambiguously theirs, so it is stored
//! whole and never rewritten by the model: Recap builds *around* these notes rather than over
//! them.
//!
//! One row per session. Typing is continuous, so writes are upserts rather than appends — the
//! note is a document being edited, not a log.
//!
//! # Two layers
//!
//! After the meeting, the Batch lane writes the fragments up into something readable
//! (FR-MTUX-03). That polished text goes to [`save_enhanced`] — a *separate* table — and the
//! functions here that touch the user's own words ([`save`], [`get`]) never see it. The
//! separation is the feature: `Original` and `Polished` are both always available, and no
//! regeneration can destroy the only copy of what the user actually typed.

use rusqlite::{params, Connection};

/// Write (or overwrite) the note for a session.
///
/// The body is stored as typed. Unlike captured text it is not passed through `redact` — a note
/// is the user writing to themselves in their own words, and silently altering it would break the
/// one part of the record they can trust to be exactly what they wrote.
pub fn save(
    conn: &Connection,
    session_id: i64,
    body: &str,
    now: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO session_notes (session_id, body, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT (session_id) DO UPDATE SET body = ?2, updated_at = ?3",
        params![session_id, body, now],
    )?;
    Ok(())
}

/// The note for a session, if the user wrote one. `None` means they did not type — a normal
/// outcome, not a missing row (FR-MT-10: notes are optional).
pub fn get(conn: &Connection, session_id: i64) -> Result<Option<String>, rusqlite::Error> {
    conn.query_row(
        "SELECT body FROM session_notes WHERE session_id = ?1",
        [session_id],
        |r| r.get(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
}

/// Write (or replace) the polished write-up for a session (FR-MTUX-03).
///
/// Deliberately a different function on a different table from [`save`]: there is no code path
/// by which enhancing a note can write over the note. A caller that wanted to overwrite the
/// user's words would have to call [`save`] explicitly and mean it.
pub fn save_enhanced(
    conn: &Connection,
    session_id: i64,
    body: &str,
    now: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO session_notes_enhanced (session_id, body, generated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT (session_id) DO UPDATE SET body = ?2, generated_at = ?3",
        params![session_id, body, now],
    )?;
    Ok(())
}

/// The polished write-up for a session, if one has been generated. `None` is the normal state
/// until the Batch lane runs — and the permanent state when it is unavailable, which is why the
/// UI must be able to show the original alone.
pub fn get_enhanced(conn: &Connection, session_id: i64) -> Result<Option<String>, rusqlite::Error> {
    conn.query_row(
        "SELECT body FROM session_notes_enhanced WHERE session_id = ?1",
        [session_id],
        |r| r.get(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{open, NewSession};

    fn session(conn: &Connection) -> i64 {
        open(
            conn,
            &NewSession {
                kind: "meeting",
                started_at: 1_000,
                title: Some("Weekly sync"),
                app_bundle_id: Some("us.zoom.xos"),
                calendar_occurrence_id: None,
                confidence: 0.65,
                provenance: "{}",
            },
        )
        .unwrap()
    }

    #[test]
    fn a_session_with_no_note_has_none() {
        // Typing is optional (FR-MT-10) — most people cannot type during a meeting, and the
        // absence of a note is a normal state, not a missing row to be defaulted to "".
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn);
        assert_eq!(get(&conn, id).unwrap(), None);
    }

    #[test]
    fn a_note_is_saved_and_read_back_verbatim() {
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn);

        save(&conn, id, "- pricing decision pending\n- Alice takes the vendor thread", 1_500)
            .unwrap();

        assert_eq!(
            get(&conn, id).unwrap().as_deref(),
            Some("- pricing decision pending\n- Alice takes the vendor thread")
        );
    }

    #[test]
    fn typing_again_replaces_the_note_rather_than_appending() {
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn);

        save(&conn, id, "first", 1_500).unwrap();
        save(&conn, id, "first and second", 1_600).unwrap();

        assert_eq!(get(&conn, id).unwrap().as_deref(), Some("first and second"));
        let n: i64 = conn
            .query_row("SELECT count(*) FROM session_notes WHERE session_id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 1, "a note is one document per session, not an append log");
    }

    #[test]
    fn enhancing_a_note_never_touches_what_the_user_wrote() {
        // The invariant the two-table design exists for (FR-MTUX-03). If this ever fails, the
        // one artefact in the database that is unambiguously the user's has been edited by a
        // model — silently, and with no copy left to return to.
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn);
        save(&conn, id, "- pricing?\n- alice → vendor", 1_500).unwrap();

        save_enhanced(&conn, id, "Pricing is unresolved. Alice owns the vendor thread.", 2_000)
            .unwrap();

        assert_eq!(get(&conn, id).unwrap().as_deref(), Some("- pricing?\n- alice → vendor"));
        assert!(get_enhanced(&conn, id).unwrap().unwrap().starts_with("Pricing is unresolved"));
    }

    #[test]
    fn a_note_with_no_write_up_yet_has_none() {
        // The permanent state when the Batch lane is unavailable: the UI must render the
        // original alone rather than waiting for a polish that is not coming.
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn);
        save(&conn, id, "raw", 1_500).unwrap();
        assert_eq!(get_enhanced(&conn, id).unwrap(), None);
    }

    #[test]
    fn regenerating_the_write_up_replaces_it_rather_than_accumulating() {
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn);
        save_enhanced(&conn, id, "first pass", 2_000).unwrap();
        save_enhanced(&conn, id, "second pass", 3_000).unwrap();

        assert_eq!(get_enhanced(&conn, id).unwrap().as_deref(), Some("second pass"));
        let n: i64 = conn
            .query_row("SELECT count(*) FROM session_notes_enhanced WHERE session_id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn notes_belong_to_their_own_session() {
        let conn = crate::open_in_memory().unwrap();
        let a = session(&conn);
        let b = session(&conn);

        save(&conn, a, "about A", 1_500).unwrap();

        assert_eq!(get(&conn, b).unwrap(), None);
        assert_eq!(get(&conn, a).unwrap().as_deref(), Some("about A"));
    }
}
