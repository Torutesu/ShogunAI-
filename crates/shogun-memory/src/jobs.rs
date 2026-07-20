//! Dream Cycle job-run ledger repository (FR-DC-04). Stores one row per `(cycle_id, kind)` and
//! upserts its state across retries, so a killed cycle resumes by skipping the jobs already `done`.
//!
//! The repository is deliberately string-typed for `kind` / `state`: shogun-memory owns the table
//! but not the Dream Cycle's job vocabulary (that's shogun-core's `dreamcycle::plan`). The core
//! daemon maps its enums to/from these strings, so storage stays free of an upward dependency. The
//! `state` value is still constrained by a CHECK.

use rusqlite::{params, Connection};

/// A job-run row as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRunRow {
    pub cycle_id: String,
    pub kind: String,
    pub state: String,
    pub input_from_ts: i64,
    pub input_to_ts: i64,
}

/// Insert or update the row for `(cycle_id, kind)` (FR-DC-04 upsert). Idempotent: recording the
/// same job twice leaves one row, its state/range refreshed.
pub fn upsert(
    conn: &Connection,
    cycle_id: &str,
    kind: &str,
    state: &str,
    input_from_ts: i64,
    input_to_ts: i64,
    now: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO job_runs (cycle_id, kind, state, input_from_ts, input_to_ts, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (cycle_id, kind) DO UPDATE SET
           state = excluded.state,
           input_from_ts = excluded.input_from_ts,
           input_to_ts = excluded.input_to_ts,
           updated_at = excluded.updated_at",
        params![cycle_id, kind, state, input_from_ts, input_to_ts, now],
    )?;
    Ok(())
}

/// All job rows recorded for a cycle (order unspecified; the caller orders by the plan sequence).
pub fn list_by_cycle(conn: &Connection, cycle_id: &str) -> Result<Vec<JobRunRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT cycle_id, kind, state, input_from_ts, input_to_ts FROM job_runs WHERE cycle_id = ?1",
    )?;
    let rows = stmt.query_map([cycle_id], |r| {
        Ok(JobRunRow {
            cycle_id: r.get(0)?,
            kind: r.get(1)?,
            state: r.get(2)?,
            input_from_ts: r.get(3)?,
            input_to_ts: r.get(4)?,
        })
    })?;
    rows.collect()
}

/// The end (`input_to_ts`) of the most recent **completed** consolidation, i.e. the high-water mark
/// of events already consolidated (FR-DC-04). The next cycle consumes `[this, now)`, so no event is
/// classified twice and none is skipped. `None` if no consolidation has ever completed (first run).
/// Scoped to the `consolidation` job because it is the one that reads the event window.
pub fn last_consolidated_to(conn: &Connection) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row(
        "SELECT MAX(input_to_ts) FROM job_runs WHERE kind = 'consolidation' AND state = 'done'",
        [],
        |r| r.get::<_, Option<i64>>(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_consolidated_to_is_the_high_water_mark() {
        let conn = crate::open_in_memory().unwrap();
        assert_eq!(last_consolidated_to(&conn).unwrap(), None, "no completed cycle yet");
        upsert(&conn, "c1", "consolidation", "done", 0, 100, 1).unwrap();
        upsert(&conn, "c2", "consolidation", "done", 100, 250, 2).unwrap();
        // a running (not done) later cycle must not count
        upsert(&conn, "c3", "consolidation", "running", 250, 400, 3).unwrap();
        assert_eq!(last_consolidated_to(&conn).unwrap(), Some(250));
    }

    #[test]
    fn upsert_is_idempotent_on_cycle_and_kind() {
        let conn = crate::open_in_memory().unwrap();
        upsert(&conn, "20260720", "consolidation", "running", 0, 100, 1).unwrap();
        upsert(&conn, "20260720", "consolidation", "done", 0, 100, 2).unwrap();
        let rows = list_by_cycle(&conn, "20260720").unwrap();
        assert_eq!(rows.len(), 1, "same (cycle,kind) stays one row");
        assert_eq!(rows[0].state, "done", "state advanced to the latest");
    }

    #[test]
    fn list_scopes_to_the_cycle() {
        let conn = crate::open_in_memory().unwrap();
        upsert(&conn, "night-a", "consolidation", "done", 0, 1, 1).unwrap();
        upsert(&conn, "night-a", "compression", "failed", 0, 1, 1).unwrap();
        upsert(&conn, "night-b", "consolidation", "done", 0, 1, 1).unwrap();
        assert_eq!(list_by_cycle(&conn, "night-a").unwrap().len(), 2);
        assert_eq!(list_by_cycle(&conn, "night-b").unwrap().len(), 1);
    }

    #[test]
    fn bad_state_is_rejected_by_check() {
        let conn = crate::open_in_memory().unwrap();
        assert!(upsert(&conn, "c", "consolidation", "sideways", 0, 1, 1).is_err());
    }
}
