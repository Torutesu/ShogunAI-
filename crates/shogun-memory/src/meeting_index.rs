//! Putting a finished meeting on the searchable spine (FR-MT-14, FR-MTUX-02).
//!
//! Transcripts and notes live in their own tables, structured the way the Recap needs them:
//! per-segment, speaker-attributed, time-ordered. That shape is right for reading a meeting back
//! and wrong for finding one. Everything else in SHOGUN is found through `event_log` — the FTS
//! index is a shadow of it, hybrid search walks it, Fusion and extraction read it — so content
//! that never reaches `event_log` is content the user cannot search for.
//!
//! FR-MT-14 puts it as a requirement: the transcript must land where "既存の検索・抽出・Fusion
//! がそのまま効く". This module is what makes that true. It is the same move the connector lane
//! makes (Gmail is normalised into the log while Gmail keeps its own shape) — one spine, many
//! sources, no per-source search path.
//!
//! **What being on the spine does NOT mean** (the A-2 decision, 2026-08-14,
//! `docs/meeting-text-on-the-search-spine.md`): rows with [`SOURCE`] never reach the Batch lane.
//! `event_log::events_in_range_for_batch` excludes them by construction, so search, Fusion,
//! the context pack and *local* extraction all see meetings — and the nightly cloud
//! classification does not. The Deepgram consent covers live transcription only; shipping the
//! finished transcript to the relay every night would be a second, undisclosed egress
//! (invariant 3). `scripts/check-batch-source-filter.py` guards the exclusion in CI.
//!
//! **When**: at the end of a session (Wrapping → Recap), because that is when the transcript is
//! final — and again whenever the note changes afterwards, because a note is often flushed
//! (blur / debounce) moments *after* auto-wrap closed the session.
//!
//! **Changed text replaces its row.** [`index_session`] first drops any previously indexed row of
//! the same kind whose content hash no longer matches (and its vector/cold shadows), then
//! inserts. So re-running with identical text is a dedup touch, an edited note replaces the
//! note's row, and a future re-transcription (WS9) replaces the transcript's — the stale version
//! never lingers in search beside the current one.

use rusqlite::Connection;

use crate::event_log::{insert_or_touch, NewEvent};

/// The `source` value meeting-derived rows carry. Distinct from `capture`, so a search result can
/// say "this was said in a meeting" rather than "this was on your screen" (FR-MEM-23) — and so
/// the Batch lane's source filter has a name to exclude.
pub const SOURCE: &str = "meeting";

/// What [`index_session`] wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Indexed {
    /// The event row carrying the transcript, if the session had one.
    pub transcript_event_id: Option<i64>,
    /// The event row carrying the user's own note, if they wrote one.
    pub note_event_id: Option<i64>,
}

impl Indexed {
    pub fn is_empty(&self) -> bool {
        self.transcript_event_id.is_none() && self.note_event_id.is_none()
    }
}

/// Render a session's transcript as one searchable body.
///
/// Speakers are prefixed when known and omitted when not — `Unknown` is stored as NULL precisely
/// because we do not guess (FR-MT-15), and inventing "Someone:" here would launder that NULL into
/// an assertion.
fn transcript_body(
    conn: &Connection,
    session_id: i64,
) -> Result<Option<String>, rusqlite::Error> {
    let segments = crate::transcript_segments::for_session(conn, session_id)?;
    if segments.is_empty() {
        return Ok(None);
    }
    let mut body = String::new();
    for (_ts, speaker, text, _confidence) in segments {
        if !body.is_empty() {
            body.push('\n');
        }
        match speaker.as_deref() {
            Some("me") => body.push_str("Me: "),
            Some("other") => body.push_str("Them: "),
            _ => {}
        }
        body.push_str(&text);
    }
    Ok(Some(body))
}

/// Drop this session's indexed rows of `kind` whose content hash is not `keep_hash` — the stale
/// versions an edit leaves behind. Vector and cold-embedding shadows are keyed on the event id and
/// have no delete trigger, so they go explicitly; the FTS mirror follows via the AD trigger.
fn drop_stale(
    conn: &Connection,
    session_id: i64,
    kind: &str,
    keep_hash: Option<&str>,
) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id FROM event_log
         WHERE session_id = ?1 AND source = ?2 AND kind = ?3
           AND (?4 IS NULL OR content_hash != ?4)",
    )?;
    let stale: Vec<i64> = stmt
        .query_map(rusqlite::params![session_id, SOURCE, kind, keep_hash], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for id in stale {
        conn.execute("DELETE FROM event_vec WHERE rowid = ?1", [id])?;
        conn.execute("DELETE FROM cold_embeddings WHERE event_id = ?1", [id])?;
        conn.execute("DELETE FROM event_log WHERE id = ?1", [id])?;
    }
    Ok(())
}

/// Make a finished meeting searchable. Returns which rows were written.
///
/// A session with neither a transcript nor a note indexes to nothing — a meeting where the user
/// took no notes and transcription was off leaves no text, and inventing a placeholder row would
/// put an empty result in every future search. A body that *disappeared* (a note cleared to
/// empty) takes its indexed row with it.
pub fn index_session(conn: &Connection, session_id: i64) -> Result<Indexed, rusqlite::Error> {
    let Some(session) = crate::session::get(conn, session_id)? else {
        return Ok(Indexed::default());
    };

    let mut out = Indexed::default();
    let title = session.title.as_deref();
    let app = session.app_bundle_id.as_deref();

    // Two rows, not one: the transcript is what was said and the note is what the user chose to
    // write. Merging them would make a search hit unable to say which it found — and the note is
    // the row that must never read as machine-generated.
    for (kind, body) in [
        ("transcript", transcript_body(conn, session_id)?),
        ("note", crate::session_notes::get(conn, session_id)?),
    ] {
        let Some(body) = body.filter(|b| !b.trim().is_empty()) else {
            drop_stale(conn, session_id, kind, None)?;
            continue;
        };
        let hash = crate::event_log::content_hash(&body);
        // An edit replaces its row: the old version must not stay findable beside the new one.
        drop_stale(conn, session_id, kind, Some(&hash))?;
        let (event_id, _is_new) = insert_or_touch(
            conn,
            &NewEvent {
                // The meeting's own time, not the moment of indexing: a search for what was said
                // last Tuesday must find it under last Tuesday.
                ts: session.started_at,
                source: SOURCE,
                kind,
                app_bundle_id: app,
                window_title: title,
                content: &body,
                content_hash: &hash,
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )?;
        crate::session::attach_event(conn, session_id, event_id)?;
        match kind {
            "transcript" => out.transcript_event_id = Some(event_id),
            _ => out.note_event_id = Some(event_id),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{close, open, NewSession};
    use crate::transcript_segments::{append, NewSegment, Speaker};

    fn session(conn: &Connection, title: &str) -> i64 {
        let id = open(
            conn,
            &NewSession {
                kind: "meeting",
                started_at: 1_000,
                title: Some(title),
                app_bundle_id: Some("us.zoom.xos"),
                calendar_occurrence_id: None,
                confidence: 0.7,
                provenance: "{}",
            },
        )
        .unwrap();
        close(conn, id, 5_000).unwrap();
        id
    }

    fn say(conn: &Connection, session_id: i64, speaker: Speaker, text: &str, ts: i64) {
        append(conn, &NewSegment { session_id, ts, speaker, text, confidence: 0.9 }, ts).unwrap();
    }

    #[test]
    fn a_transcript_becomes_findable_by_the_ordinary_search() {
        // The whole point of FR-MT-14: no meeting-specific search path.
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Weekly sync");
        say(&conn, id, Speaker::Other, "the vendor renewal was settled at 12k", 1_100);

        let indexed = index_session(&conn, id).unwrap();
        assert!(indexed.transcript_event_id.is_some());

        let hits = crate::search::search(&conn, "vendor renewal", 10).unwrap();
        assert_eq!(hits.len(), 1, "the meeting must be reachable from ordinary search");
        assert_eq!(hits[0].source, SOURCE, "and be attributable to a meeting (FR-MEM-23)");
    }

    #[test]
    fn a_note_is_indexed_separately_from_the_transcript() {
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "1:1");
        say(&conn, id, Speaker::Me, "spoken words", 1_100);
        crate::session_notes::save(&conn, id, "typed words", 2_000).unwrap();

        let indexed = index_session(&conn, id).unwrap();
        assert!(indexed.transcript_event_id.is_some());
        assert!(indexed.note_event_id.is_some());
        assert_ne!(indexed.transcript_event_id, indexed.note_event_id);
    }

    #[test]
    fn indexed_rows_belong_to_their_session() {
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Weekly sync");
        say(&conn, id, Speaker::Me, "hello", 1_100);
        index_session(&conn, id).unwrap();

        // `session_id` is what the library view and the compression job join on.
        let texts = crate::session::event_texts(&conn, id).unwrap();
        assert_eq!(texts.len(), 1);
        assert!(texts[0].content.contains("hello"));
    }

    #[test]
    fn the_event_carries_the_meeting_time_not_the_indexing_time() {
        // Searching for "what was said last Tuesday" must find it under last Tuesday.
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Weekly sync");
        say(&conn, id, Speaker::Me, "hello", 1_100);
        let indexed = index_session(&conn, id).unwrap();

        let ts: i64 = conn
            .query_row(
                "SELECT ts FROM event_log WHERE id = ?1",
                [indexed.transcript_event_id.unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ts, 1_000, "the session's start, not the sweep that indexed it");
    }

    #[test]
    fn re_indexing_the_same_meeting_does_not_duplicate_it() {
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Weekly sync");
        say(&conn, id, Speaker::Me, "hello", 1_100);

        let first = index_session(&conn, id).unwrap();
        let second = index_session(&conn, id).unwrap();

        assert_eq!(first.transcript_event_id, second.transcript_event_id);
        let n: i64 = conn
            .query_row("SELECT count(*) FROM event_log WHERE source = ?1", [SOURCE], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "a regenerated Recap must not double the meeting in search");
    }

    #[test]
    fn a_silent_meeting_with_no_notes_indexes_to_nothing() {
        // Notes are optional and transcription can be off. An empty placeholder row would put a
        // blank result into every future search.
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Quiet meeting");
        assert!(index_session(&conn, id).unwrap().is_empty());
        let n: i64 =
            conn.query_row("SELECT count(*) FROM event_log", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn a_whitespace_only_note_is_not_indexed() {
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Meeting");
        crate::session_notes::save(&conn, id, "   \n  ", 2_000).unwrap();
        assert!(index_session(&conn, id).unwrap().note_event_id.is_none());
    }

    #[test]
    fn an_unknown_speaker_is_not_given_a_name() {
        // FR-MT-15: NULL means we do not know. Rendering "Someone:" would launder that into a
        // claim, and the claim would then be searchable as if it were evidence.
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Meeting");
        say(&conn, id, Speaker::Unknown, "an unattributed line", 1_100);
        index_session(&conn, id).unwrap();

        let body: String = conn
            .query_row("SELECT content FROM event_log WHERE source = ?1", [SOURCE], |r| r.get(0))
            .unwrap();
        assert_eq!(body, "an unattributed line");
    }

    #[test]
    fn known_speakers_are_labelled() {
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Meeting");
        say(&conn, id, Speaker::Me, "my line", 1_100);
        say(&conn, id, Speaker::Other, "their line", 1_200);
        index_session(&conn, id).unwrap();

        let body: String = conn
            .query_row("SELECT content FROM event_log WHERE source = ?1", [SOURCE], |r| r.get(0))
            .unwrap();
        assert_eq!(body, "Me: my line\nThem: their line");
    }

    #[test]
    fn a_missing_session_indexes_to_nothing_rather_than_failing() {
        let conn = crate::open_in_memory().unwrap();
        assert!(index_session(&conn, 9_999).unwrap().is_empty());
    }

    #[test]
    fn an_edited_note_replaces_its_indexed_row() {
        // The late-flush path: the note is saved (and re-saved) after the session closed. The old
        // version must stop being findable — two search hits for one note, one of them stale, is
        // worse than either alone.
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Meeting");
        crate::session_notes::save(&conn, id, "first draft of the note", 2_000).unwrap();
        let first = index_session(&conn, id).unwrap();

        crate::session_notes::save(&conn, id, "final version of the note", 3_000).unwrap();
        let second = index_session(&conn, id).unwrap();
        // Row *identity* is not the property (SQLite may reuse the freed rowid) — content is.
        assert!(first.note_event_id.is_some() && second.note_event_id.is_some());

        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM event_log WHERE source = ?1 AND kind = 'note'",
                [SOURCE],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "the stale note version must be gone");
        assert!(crate::search::search(&conn, "first draft", 10).unwrap().is_empty());
        assert_eq!(crate::search::search(&conn, "final version", 10).unwrap().len(), 1);
    }

    #[test]
    fn a_note_cleared_to_empty_takes_its_indexed_row_with_it() {
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Meeting");
        crate::session_notes::save(&conn, id, "temporary thought", 2_000).unwrap();
        index_session(&conn, id).unwrap();

        crate::session_notes::save(&conn, id, "", 3_000).unwrap();
        let after = index_session(&conn, id).unwrap();
        assert!(after.note_event_id.is_none());
        assert!(crate::search::search(&conn, "temporary thought", 10).unwrap().is_empty());
    }

    #[test]
    fn replacing_a_row_does_not_touch_the_other_kind() {
        // The transcript must survive a note edit: drop_stale is kind-scoped.
        let conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Meeting");
        say(&conn, id, Speaker::Me, "the spoken record", 1_100);
        crate::session_notes::save(&conn, id, "note v1", 2_000).unwrap();
        let first = index_session(&conn, id).unwrap();

        crate::session_notes::save(&conn, id, "note v2", 3_000).unwrap();
        let second = index_session(&conn, id).unwrap();

        assert_eq!(first.transcript_event_id, second.transcript_event_id);
        assert_eq!(crate::search::search(&conn, "spoken record", 10).unwrap().len(), 1);
    }
}
