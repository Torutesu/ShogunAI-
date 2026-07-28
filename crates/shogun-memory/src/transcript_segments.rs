//! The meeting transcript, stored as text only (FR-MT-13).
//!
//! Invariant 2: the audio itself is never persisted. This module is the *only* writer of the
//! transcript, and it writes text that has already been through `redact` — a spoken credential
//! ("my password is …") is as sensitive on the write path as a typed one, so the same masking
//! that protects captured screen text protects the transcript.

use rusqlite::{params, Connection};

/// Who spoke, decided by the capture source rather than by inference: microphone input is `Me`,
/// the system tap is `Other`. `Unknown` is stored as NULL — we never guess a speaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    Me,
    Other,
    Unknown,
}

impl Speaker {
    fn as_str(self) -> Option<&'static str> {
        match self {
            Speaker::Me => Some("me"),
            Speaker::Other => Some("other"),
            Speaker::Unknown => None,
        }
    }
}

/// One transcribed line, ready to persist.
#[derive(Debug, Clone, PartialEq)]
pub struct NewSegment<'a> {
    pub session_id: i64,
    pub ts: i64,
    pub speaker: Speaker,
    pub text: &'a str,
    pub confidence: f64,
}

/// Append one transcript line. `origin` is fixed to `'asr'` here; the caption path is a future
/// caller. Returns the new row id.
pub fn append(conn: &Connection, seg: &NewSegment, now: i64) -> Result<i64, rusqlite::Error> {
    let redacted = crate::redact::redact(seg.text);
    conn.execute(
        "INSERT INTO transcript_segments
           (session_id, ts, speaker, text, origin, confidence, created_at)
         VALUES (?1, ?2, ?3, ?4, 'asr', ?5, ?6)",
        params![
            seg.session_id,
            seg.ts,
            seg.speaker.as_str(),
            redacted.as_ref(),
            seg.confidence,
            now,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// All lines for a session in time order. Recap (MT4) reads through this.
#[allow(clippy::type_complexity)]
pub fn for_session(
    conn: &Connection,
    session_id: i64,
) -> Result<Vec<(i64, Option<String>, String, f64)>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT ts, speaker, text, confidence FROM transcript_segments
         WHERE session_id = ?1 ORDER BY ts, id",
    )?;
    let rows = stmt.query_map([session_id], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })?;
    rows.collect()
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
    fn segments_are_read_back_in_time_order() {
        let conn = crate::open_in_memory().unwrap();
        let sid = session(&conn);
        append(&conn, &NewSegment { session_id: sid, ts: 2_000, speaker: Speaker::Other, text: "second", confidence: 0.9 }, 9).unwrap();
        append(&conn, &NewSegment { session_id: sid, ts: 1_000, speaker: Speaker::Me, text: "first", confidence: 0.8 }, 9).unwrap();
        let got = for_session(&conn, sid).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].2, "first");
        assert_eq!(got[1].2, "second");
    }

    #[test]
    fn unknown_speaker_is_stored_as_null() {
        let conn = crate::open_in_memory().unwrap();
        let sid = session(&conn);
        append(&conn, &NewSegment { session_id: sid, ts: 1_000, speaker: Speaker::Unknown, text: "hi", confidence: 0.5 }, 9).unwrap();
        let got = for_session(&conn, sid).unwrap();
        assert_eq!(got[0].1, None);
    }

    #[test]
    fn spoken_secrets_are_redacted_before_write() {
        let conn = crate::open_in_memory().unwrap();
        let sid = session(&conn);
        append(&conn, &NewSegment { session_id: sid, ts: 1_000, speaker: Speaker::Me, text: "the key is sk-ant-abc123def456", confidence: 0.9 }, 9).unwrap();
        let got = for_session(&conn, sid).unwrap();
        assert!(!got[0].2.contains("sk-ant-abc123def456"), "raw secret leaked into transcript");
    }
}
