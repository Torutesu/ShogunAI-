//! State maintenance passes for the Dream Cycle (FR-ST-21, FR-DC-03). Pure DB effects, no model
//! call — these run in both the Full and Degraded sequences so state does not rot when a full cycle
//! is missed. All updates are idempotent given the same `now` (they recompute an absolute value from
//! stored evidence times, never accumulate).

use rusqlite::{params, Connection};

/// One day in milliseconds (open-loop staleness is measured in whole days).
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Recompute overdue status and open-loop staleness from `now_ms` (FR-ST-21):
/// - an `open` commitment whose `due_at` is in the past becomes `overdue`;
/// - every `open` open loop's `staleness_days` is set to whole days since `opened_at`.
///
/// Returns `(commitments_flagged_overdue, open_loops_touched)`. Idempotent: re-running with the
/// same `now` flags nothing new and rewrites the same staleness values.
pub fn recompute_overdue_and_staleness(
    conn: &mut Connection,
    now_ms: i64,
) -> Result<(usize, usize), rusqlite::Error> {
    let tx = conn.transaction()?;
    let overdue = tx.execute(
        "UPDATE commitments SET status = 'overdue', updated_at = ?1
         WHERE status = 'open' AND due_at IS NOT NULL AND due_at < ?1",
        params![now_ms],
    )?;
    let loops = tx.execute(
        "UPDATE open_loops
         SET staleness_days = MAX(0, (?1 - opened_at) / ?2), updated_at = ?1
         WHERE status = 'open'",
        params![now_ms, DAY_MS],
    )?;
    tx.commit()?;
    Ok((overdue, loops))
}

/// The four state tables carry a `confidence` and a `last_evidence_at`; decay lowers confidence for
/// rows not re-evidenced recently.
const CONFIDENCE_TABLES: &[&str] = &["people", "projects", "commitments", "open_loops"];

/// Age-decay confidence for every state row (FR-ST-21). Each row's confidence is multiplied by
/// `0.5^(elapsed / half_life_ms)`, where `elapsed = now - last_evidence_at` — i.e. one half-life of
/// silence halves the confidence. New evidence refreshes `last_evidence_at` elsewhere, so a
/// re-evidenced row barely decays. Rows with `last_evidence_at >= now` (future/edge) are left as-is.
///
/// Computed in Rust (not SQL `pow`, which the bundled SQLite may lack) and clamped to `[0, 1]` so
/// the schema CHECK always holds. Returns the number of rows whose confidence actually changed.
pub fn decay_confidence(
    conn: &mut Connection,
    now_ms: i64,
    half_life_ms: i64,
) -> Result<usize, rusqlite::Error> {
    if half_life_ms <= 0 {
        return Ok(0);
    }
    let half_life = half_life_ms as f64;
    let mut changed = 0usize;
    let tx = conn.transaction()?;
    for table in CONFIDENCE_TABLES {
        // (id, confidence, last_evidence_at) for rows that could decay
        let rows: Vec<(i64, f64, i64)> = {
            let sql = format!(
                "SELECT id, confidence, last_evidence_at FROM {table} WHERE last_evidence_at < ?1"
            );
            let mut stmt = tx.prepare(&sql)?;
            let mapped = stmt.query_map(params![now_ms], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        for (id, confidence, last_evidence_at) in rows {
            let elapsed = (now_ms - last_evidence_at) as f64;
            let factor = 0.5_f64.powf(elapsed / half_life);
            let decayed = (confidence * factor).clamp(0.0, 1.0);
            // only write when it meaningfully moves, to keep the changed-count honest
            if (decayed - confidence).abs() > 1e-9 {
                let sql = format!("UPDATE {table} SET confidence = ?1, updated_at = ?2 WHERE id = ?3");
                tx.execute(&sql, params![decayed, now_ms, id])?;
                changed += 1;
            }
        }
    }
    tx.commit()?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_log::{insert as insert_event, NewEvent};
    use crate::state::{
        insert_commitment, insert_open_loop, insert_person, CommitmentDirection, CommitmentStatus,
        NewCommitment, NewOpenLoop, NewPerson, OpenLoopKind, Provenance,
    };

    fn seed_event(conn: &Connection) -> i64 {
        insert_event(
            conn,
            &NewEvent {
                ts: 1,
                source: "capture",
                kind: "text",
                app_bundle_id: None,
                window_title: None,
                content: "evidence",
                content_hash: "h1",
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn past_due_open_commitment_becomes_overdue() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn);
        insert_commitment(
            &mut conn,
            &NewCommitment {
                direction: CommitmentDirection::Mine,
                counterparty_id: None,
                description: "ship it",
                due_at: Some(1_000),
                status: CommitmentStatus::Open,
                project_id: None,
                confidence: 0.9,
                now: 1,
            },
            &[Provenance::new(e)],
        )
        .unwrap();
        let (overdue, _) = recompute_overdue_and_staleness(&mut conn, 2_000).unwrap();
        assert_eq!(overdue, 1);
        let status: String =
            conn.query_row("SELECT status FROM commitments", [], |r| r.get(0)).unwrap();
        assert_eq!(status, "overdue");
        // idempotent: a second pass flags nothing new
        let (again, _) = recompute_overdue_and_staleness(&mut conn, 2_000).unwrap();
        assert_eq!(again, 0);
    }

    #[test]
    fn open_loop_staleness_tracks_elapsed_days() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn);
        insert_open_loop(
            &mut conn,
            &NewOpenLoop {
                kind: OpenLoopKind::ReplyNeeded,
                description: "reply",
                counterparty_id: None,
                project_id: None,
                opened_at: 0,
                confidence: 0.6,
                now: 0,
            },
            &[Provenance::new(e)],
        )
        .unwrap();
        recompute_overdue_and_staleness(&mut conn, 3 * DAY_MS + 500).unwrap();
        let days: i64 =
            conn.query_row("SELECT staleness_days FROM open_loops", [], |r| r.get(0)).unwrap();
        assert_eq!(days, 3);
    }

    #[test]
    fn confidence_halves_after_one_half_life() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn);
        insert_person(
            &mut conn,
            &NewPerson { display_name: "Ann", confidence: 0.8, now: 0, ..Default::default() },
            &[Provenance::new(e)],
        )
        .unwrap();
        let half_life = 10 * DAY_MS;
        let changed = decay_confidence(&mut conn, half_life, half_life).unwrap();
        assert_eq!(changed, 1);
        let c: f64 = conn.query_row("SELECT confidence FROM people", [], |r| r.get(0)).unwrap();
        assert!((c - 0.4).abs() < 1e-6, "confidence should halve: {c}");
        // stays within the schema CHECK range
        assert!((0.0..=1.0).contains(&c));
    }

    #[test]
    fn freshly_evidenced_rows_barely_decay() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn);
        insert_person(
            &mut conn,
            &NewPerson { display_name: "Now", confidence: 0.9, now: 1_000, ..Default::default() },
            &[Provenance::new(e)],
        )
        .unwrap();
        // now == last_evidence_at → elapsed 0, no decay (row excluded by the < now filter)
        let changed = decay_confidence(&mut conn, 1_000, 10 * DAY_MS).unwrap();
        assert_eq!(changed, 0);
        let c: f64 = conn.query_row("SELECT confidence FROM people", [], |r| r.get(0)).unwrap();
        assert!((c - 0.9).abs() < 1e-9);
    }
}
