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
//! `event_log::events_in_range_partitioned` excludes them by construction, so search, Fusion,
//! the context pack and *local* extraction all see meetings — and the nightly cloud
//! classification does not. The Deepgram consent covers live transcription only; shipping the
//! finished transcript to the relay every night would be a second, undisclosed egress
//! (invariant 3). `scripts/check-batch-source-filter.py` guards the exclusion in CI.
//!
//! **When**: at the end of a session (Wrapping → Recap), because that is when the transcript is
//! final — and again whenever the note changes afterwards, because a note is often flushed
//! (blur / debounce) moments *after* auto-wrap closed the session.
//!
//! **Changed text replaces its row — in place.** [`index_session`] updates an existing row of the
//! same kind through `event_log::update_content_and_hash` rather than delete-and-reinsert:
//! nightly extraction links `state_provenance` rows to meeting events (a commitment made in a
//! call cites the transcript), and deleting the event would either trip the FK or orphan the
//! evidence behind a state row. An update keeps that provenance honestly valid — the evidence is
//! still "this meeting", now in its edited form — while the stale embedding is dropped so the
//! embed job re-embeds the new text. Only a body that *disappeared* (a note cleared to empty)
//! deletes its row, and then the provenance rows go with it: evidence that no longer exists must
//! not keep vouching for state.

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

/// This session's already-indexed rows of `kind`: `(event_id, content_hash)`, oldest first.
fn indexed_rows(
    conn: &Connection,
    session_id: i64,
    kind: &str,
) -> Result<Vec<(i64, String)>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, content_hash FROM event_log
         WHERE session_id = ?1 AND source = ?2 AND kind = ?3
         ORDER BY id",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![session_id, SOURCE, kind], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    rows.collect()
}

/// Delete one indexed row for good: the provenance rows citing it first (`state_provenance`
/// carries an FK to `event_log`, and evidence that no longer exists must not keep vouching for
/// state), then the embedding shadows (keyed on the event id, no delete trigger), then the row
/// (the FTS mirror follows via the AD trigger).
fn remove_row(conn: &Connection, event_id: i64) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM state_provenance WHERE event_id = ?1", [event_id])?;
    conn.execute("DELETE FROM event_vec WHERE rowid = ?1", [event_id])?;
    conn.execute("DELETE FROM cold_embeddings WHERE event_id = ?1", [event_id])?;
    conn.execute("DELETE FROM event_log WHERE id = ?1", [event_id])?;
    Ok(())
}

/// Rewrite an indexed row with edited text. In place, not delete-and-reinsert: nightly extraction
/// links `state_provenance` to meeting events, and dropping the row would either trip that FK or
/// orphan the evidence behind a state row — while the update keeps it honestly valid (the
/// evidence is still this meeting, in its edited form). The stale embedding is removed so the
/// embed job re-embeds the new text; the FTS mirror follows via the AU trigger.
fn rewrite_row(
    conn: &Connection,
    event_id: i64,
    body: &str,
    hash: &str,
) -> Result<(), rusqlite::Error> {
    crate::event_log::update_content_and_hash(conn, event_id, body, hash)?;
    conn.execute("DELETE FROM event_vec WHERE rowid = ?1", [event_id])?;
    conn.execute("DELETE FROM cold_embeddings WHERE event_id = ?1", [event_id])?;
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
        let existing = indexed_rows(conn, session_id, kind)?;
        let Some(body) = body.filter(|b| !b.trim().is_empty()) else {
            for (id, _) in existing {
                remove_row(conn, id)?;
            }
            continue;
        };
        let hash = crate::event_log::content_hash(&body);

        // Converge on exactly one row per kind: the first existing row is kept (rewritten if the
        // text changed); any extras are defensive cleanup — index_session itself never creates a
        // second one.
        let mut kept = None;
        for (id, existing_hash) in existing {
            if kept.is_none() {
                if existing_hash != hash {
                    rewrite_row(conn, id, &body, &hash)?;
                }
                kept = Some(id);
            } else {
                remove_row(conn, id)?;
            }
        }

        let event_id = match kept {
            Some(id) => id,
            None => {
                let (id, _is_new) = insert_or_touch(
                    conn,
                    &NewEvent {
                        // The meeting's own time, not the moment of indexing: a search for what
                        // was said last Tuesday must find it under last Tuesday.
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
                crate::session::attach_event(conn, session_id, id)?;
                id
            }
        };
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
        // The edit rewrites the SAME row (provenance stability), so the id must not change.
        assert_eq!(first.note_event_id, second.note_event_id);

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
    fn an_edit_survives_extraction_having_cited_the_event() {
        // The FK case: the nightly cycle extracted a commitment from the transcript and wrote a
        // state_provenance row citing the meeting event. A later re-index (edited note flushes
        // re-run index_session for every kind) must neither fail on the FK nor orphan the
        // evidence — the rewrite keeps the row, so the citation stays valid.
        use crate::state::{CommitmentDirection, CommitmentStatus, NewCommitment, Provenance};
        let mut conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Weekly sync");
        say(&conn, id, Speaker::Me, "I will send the budget", 1_100);
        crate::session_notes::save(&conn, id, "note v1", 2_000).unwrap();
        let first = index_session(&conn, id).unwrap();
        let transcript_event = first.transcript_event_id.unwrap();

        // extraction cites the transcript event
        crate::state::insert_commitment(
            &mut conn,
            &NewCommitment {
                direction: CommitmentDirection::Mine,
                counterparty_id: None,
                description: "send the budget",
                due_at: None,
                status: CommitmentStatus::Open,
                project_id: None,
                confidence: 0.4,
                now: 2_500,
            },
            &[Provenance::new(transcript_event)],
        )
        .unwrap();

        // the late note edit re-runs the whole index — including the transcript's converge pass
        crate::session_notes::save(&conn, id, "note v2", 3_000).unwrap();
        let second = index_session(&conn, id).expect("re-index must not trip the provenance FK");
        assert_eq!(second.transcript_event_id, Some(transcript_event));
        assert_eq!(crate::search::search(&conn, "note v2", 10).unwrap().len(), 1);

        // the citation is intact
        let cites: i64 = conn
            .query_row(
                "SELECT count(*) FROM state_provenance WHERE event_id = ?1",
                [transcript_event],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cites, 1, "provenance must survive a re-index");
    }

    #[test]
    fn a_cleared_note_with_a_citation_removes_the_citation_too() {
        // Deleting the note deletes its evidence; provenance rows citing it must go rather than
        // dangle (FK) — the state row stays, but it no longer claims this note as its source.
        use crate::state::{NewOpenLoop, OpenLoopKind, Provenance};
        let mut conn = crate::open_in_memory().unwrap();
        let id = session(&conn, "Meeting");
        crate::session_notes::save(&conn, id, "chase the vendor", 2_000).unwrap();
        let first = index_session(&conn, id).unwrap();
        let note_event = first.note_event_id.unwrap();
        crate::state::insert_open_loop(
            &mut conn,
            &NewOpenLoop {
                kind: OpenLoopKind::FollowUp,
                description: "chase the vendor",
                counterparty_id: None,
                project_id: None,
                opened_at: 2_100,
                confidence: 0.4,
                now: 2_100,
            },
            &[Provenance::new(note_event)],
        )
        .unwrap();

        crate::session_notes::save(&conn, id, "", 3_000).unwrap();
        index_session(&conn, id).expect("clearing a cited note must not trip the FK");
        assert!(crate::search::search(&conn, "chase the vendor", 10).unwrap().is_empty());
        let cites: i64 = conn
            .query_row("SELECT count(*) FROM state_provenance WHERE event_id = ?1", [note_event], |r| r.get(0))
            .unwrap();
        assert_eq!(cites, 0);
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
