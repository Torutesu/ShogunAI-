//! The persisted nightly Morning Brief repository (Plan C-1, §6.8 / FR-MB-01..06).
//!
//! One row per local calendar day (`date` is the primary key): the Dream Cycle's MorningBrief job
//! assembles the brief at night and upserts it here, so the morning display is a read — immediate
//! and offline-stable — rather than a live assembly. Like `meeting_recaps`, this is a document,
//! not a log: regenerating a day's brief replaces the row.
//!
//! The brief's *content* type lives here too ([`BriefPayload`]): the assembler (shogun-fusion,
//! driven from shogun-core) serializes into it and the desktop layer deserializes out of it, but
//! storage itself only ever sees the JSON string — the same already-serialized-columns pattern as
//! `meeting_recaps`.
//!
//! `prev_digest` carries the digest of the payload the current row replaced. That makes the
//! FR-MB-06 "Updated" mark derivable without a second table: a brief is `updated` when its payload
//! digest differs from `prev_digest`. An upsert with an unchanged payload touches neither the
//! payload nor `prev_digest`, so re-running the nightly job (crash-resume, FR-DC-04) is idempotent.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// One calendar-equivalent line in the brief's "Today" section (FR-MB-01). Until the real
/// Calendar connector lands (Plan B-4), these come from detected meetings and today-due
/// commitments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BriefScheduleLine {
    pub start_ms: i64,
    pub title: String,
    /// FR-MB-06: changed after the brief was generated (shown with an "Updated" mark).
    pub updated: bool,
}

/// One confidence-gated brief line (commitments due / open loops). `possibly` is the
/// medium-confidence hedge (FR-MB-05); provenance travels with every item (FR-ST-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BriefLine {
    pub text: String,
    pub provenance_event_id: i64,
    pub possibly: bool,
}

/// One suggested action attached to the brief (FR-MB-01: ≤3, each permission-level-tagged).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BriefActionLine {
    pub label: String,
    /// The action's permission level as a string ("L1" / "L2" / "L3").
    pub level: String,
}

/// The persisted brief content — the JSON shape stored in `briefs.payload`. Mirrors the fusion
/// assembler's `MorningBrief` sections; this crate must not depend on shogun-fusion, so the shape
/// is declared here and the core layer converts into it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefPayload {
    /// The local calendar day this brief is for ('YYYY-MM-DD'), same value as the row key.
    pub date: String,
    /// "Today" calendar-equivalent lines, time-ordered.
    pub today: Vec<BriefScheduleLine>,
    /// "Commitments due": overdue first, then soonest.
    pub commitments_due: Vec<BriefLine>,
    /// "Open loops": top-N by staleness.
    pub open_loops: Vec<BriefLine>,
    /// "What happened" summary lines (≤5). Extractive unless the Batch lane wrote prose.
    pub what_happened: Vec<String>,
    /// Suggested actions (≤3), permission-level-tagged.
    pub suggested_actions: Vec<BriefActionLine>,
}

/// A `briefs` row as stored, plus the derived `updated` flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredBrief {
    pub date: String,
    /// The BriefPayload JSON, verbatim.
    pub payload: String,
    /// Whether generated prose was attached (false = extractive honest degradation, FR-MB-04).
    pub generated: bool,
    pub built_at: i64,
    /// Digest of the payload this row replaced (`None` until a regeneration changes it).
    pub prev_digest: Option<String>,
    /// FR-MB-06: the brief's content changed after it was first built for this day.
    pub updated: bool,
}

/// Stable content digest for change detection (the FR-MB-06 `updated` mark): FNV-1a 64-bit over
/// the payload's UTF-8 bytes, 16 lower-hex chars. Implemented inline — this crate carries no hash
/// dependency, and the digest is a change marker, never a security boundary. Deliberately the same
/// digest-not-content posture as `traceability`: only a digest of the payload's predecessor
/// persists, and the payload itself is local-only data anyway.
pub fn digest(payload: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    let mut h = OFFSET;
    for b in payload.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(PRIME);
    }
    format!("{h:016x}")
}

fn derive_updated(prev_digest: Option<&str>, payload_digest: &str) -> bool {
    prev_digest.is_some_and(|p| p != payload_digest)
}

/// Insert or replace the brief for `date` (FR-DC-04 idempotent upsert). Returns the resulting
/// `updated` flag.
///
/// - First write of the day: row inserted, `prev_digest` NULL → `updated = false`.
/// - Re-run with the *same* payload (crash-resume): payload and `prev_digest` untouched — only
///   `generated`/`built_at` refresh — so `updated` keeps its value (false unless an earlier
///   regeneration changed the content).
/// - Regeneration with a *different* payload: `prev_digest` becomes the replaced payload's
///   digest → `updated = true` (FR-MB-06).
pub fn upsert_brief(
    conn: &Connection,
    date: &str,
    payload_json: &str,
    generated: bool,
    built_at: i64,
) -> Result<bool, rusqlite::Error> {
    let new_digest = digest(payload_json);
    let existing: Option<(String, Option<String>)> = conn
        .query_row("SELECT payload, prev_digest FROM briefs WHERE date = ?1", [date], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    match existing {
        None => {
            conn.execute(
                "INSERT INTO briefs (date, payload, generated, built_at, prev_digest)
                 VALUES (?1, ?2, ?3, ?4, NULL)",
                params![date, payload_json, i64::from(generated), built_at],
            )?;
            Ok(false)
        }
        Some((old_payload, old_prev)) => {
            if digest(&old_payload) == new_digest {
                // Same content: a re-run must not manufacture an "Updated" mark or lose one.
                conn.execute(
                    "UPDATE briefs SET generated = ?2, built_at = ?3 WHERE date = ?1",
                    params![date, i64::from(generated), built_at],
                )?;
                Ok(derive_updated(old_prev.as_deref(), &new_digest))
            } else {
                conn.execute(
                    "UPDATE briefs SET payload = ?2, generated = ?3, built_at = ?4, prev_digest = ?5
                     WHERE date = ?1",
                    params![date, payload_json, i64::from(generated), built_at, digest(&old_payload)],
                )?;
                Ok(true)
            }
        }
    }
}

/// The persisted brief for `date`, with the derived `updated` flag. `None` means the nightly job
/// has not written one — a normal state (the caller falls back to the degraded live assembly).
pub fn get_brief(conn: &Connection, date: &str) -> Result<Option<StoredBrief>, rusqlite::Error> {
    conn.query_row(
        "SELECT date, payload, generated, built_at, prev_digest FROM briefs WHERE date = ?1",
        [date],
        |r| {
            let payload: String = r.get(1)?;
            let prev_digest: Option<String> = r.get(4)?;
            let updated = derive_updated(prev_digest.as_deref(), &digest(&payload));
            Ok(StoredBrief {
                date: r.get(0)?,
                payload,
                generated: r.get::<_, i64>(2)? != 0,
                built_at: r.get(3)?,
                prev_digest,
                updated,
            })
        },
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(marker: &str) -> String {
        // A minimal valid BriefPayload JSON; `marker` varies the content.
        serde_json::to_string(&BriefPayload {
            date: "2026-08-09".into(),
            today: vec![],
            commitments_due: vec![BriefLine {
                text: marker.into(),
                provenance_event_id: 1,
                possibly: false,
            }],
            open_loops: vec![],
            what_happened: vec![],
            suggested_actions: vec![],
        })
        .unwrap()
    }

    #[test]
    fn a_day_with_no_brief_reads_none() {
        let conn = crate::open_in_memory().unwrap();
        assert_eq!(get_brief(&conn, "2026-08-09").unwrap(), None);
    }

    #[test]
    fn first_write_is_not_marked_updated() {
        let conn = crate::open_in_memory().unwrap();
        let updated = upsert_brief(&conn, "2026-08-09", &payload("a"), false, 100).unwrap();
        assert!(!updated, "the day's first brief has nothing to differ from");

        let row = get_brief(&conn, "2026-08-09").unwrap().unwrap();
        assert_eq!(row.payload, payload("a"));
        assert!(!row.generated);
        assert_eq!(row.built_at, 100);
        assert_eq!(row.prev_digest, None);
        assert!(!row.updated);
    }

    #[test]
    fn rewriting_the_same_payload_is_idempotent_and_stays_not_updated() {
        let conn = crate::open_in_memory().unwrap();
        upsert_brief(&conn, "2026-08-09", &payload("a"), false, 100).unwrap();
        let updated = upsert_brief(&conn, "2026-08-09", &payload("a"), false, 200).unwrap();
        assert!(!updated, "an unchanged payload must not manufacture an Updated mark");

        let row = get_brief(&conn, "2026-08-09").unwrap().unwrap();
        assert!(!row.updated);
        assert_eq!(row.built_at, 200, "the re-run's timestamp still lands");
        let n: i64 =
            conn.query_row("SELECT count(*) FROM briefs", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "one row per day, not an append log");
    }

    #[test]
    fn a_changed_payload_flips_updated_and_records_the_replaced_digest() {
        let conn = crate::open_in_memory().unwrap();
        upsert_brief(&conn, "2026-08-09", &payload("a"), false, 100).unwrap();
        let updated = upsert_brief(&conn, "2026-08-09", &payload("b"), false, 200).unwrap();
        assert!(updated, "changed content is the FR-MB-06 Updated case");

        let row = get_brief(&conn, "2026-08-09").unwrap().unwrap();
        assert!(row.updated);
        assert_eq!(row.payload, payload("b"));
        assert_eq!(row.prev_digest.as_deref(), Some(digest(&payload("a")).as_str()));
    }

    #[test]
    fn a_rerun_after_a_change_keeps_the_updated_mark() {
        let conn = crate::open_in_memory().unwrap();
        upsert_brief(&conn, "2026-08-09", &payload("a"), false, 100).unwrap();
        upsert_brief(&conn, "2026-08-09", &payload("b"), false, 200).unwrap();
        // crash-resume re-runs the job with the same (changed) payload
        let updated = upsert_brief(&conn, "2026-08-09", &payload("b"), false, 300).unwrap();
        assert!(updated, "an idempotent re-run must not erase the day's Updated mark");
        assert!(get_brief(&conn, "2026-08-09").unwrap().unwrap().updated);
    }

    #[test]
    fn briefs_are_keyed_per_day() {
        let conn = crate::open_in_memory().unwrap();
        upsert_brief(&conn, "2026-08-08", &payload("yesterday"), true, 100).unwrap();
        upsert_brief(&conn, "2026-08-09", &payload("today"), false, 200).unwrap();

        let yesterday = get_brief(&conn, "2026-08-08").unwrap().unwrap();
        let today = get_brief(&conn, "2026-08-09").unwrap().unwrap();
        assert!(yesterday.generated);
        assert!(!today.generated);
        assert_ne!(yesterday.payload, today.payload);
        assert!(!today.updated, "a new day starts unmarked even when yesterday's differs");
    }

    #[test]
    fn digest_is_deterministic_hex_and_content_sensitive() {
        assert_eq!(digest("x"), digest("x"));
        assert_ne!(digest("x"), digest("y"));
        let d = digest("payload");
        assert_eq!(d.len(), 16);
        assert!(d.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn payload_round_trips_through_json() {
        let p = BriefPayload {
            date: "2026-08-09".into(),
            today: vec![BriefScheduleLine { start_ms: 9, title: "standup".into(), updated: false }],
            commitments_due: vec![BriefLine { text: "send the deck".into(), provenance_event_id: 4, possibly: true }],
            open_loops: vec![],
            what_happened: vec!["shipped the report".into()],
            suggested_actions: vec![BriefActionLine { label: "prep the standup".into(), level: "L2".into() }],
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: BriefPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }
}
