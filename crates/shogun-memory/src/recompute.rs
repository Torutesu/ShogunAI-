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

/// Confidence a corroborated row may reach, and no more.
///
/// The local rules cap a single-sighting candidate at [`crate::extract::LOCAL_RULE_MAX_CONFIDENCE`]
/// (0.4, i.e. Low — excluded from generations entirely). Repeated *independent* evidence is real
/// signal that it was not a one-off misread, so it should count for something. But nothing here has
/// verified what the sentence means: the promotion therefore tops out just under High, so a
/// corroborated row can be offered as "possibly …" and can never be stated as fact. Only the model
/// pass (Batch classification) may lift a row into High.
const CORROBORATION_CEILING: f64 = 0.75;

/// Shapes how fast corroboration accumulates. Larger = slower. Tuned so a second sighting is a
/// clear step up and a tenth is a marginal one.
const CORROBORATION_SHAPE: f64 = 2.0;

/// Raise confidence for state rows backed by several independent events (FR-ST-21).
///
/// Without this, every locally-extracted commitment stays below the Low/Medium boundary forever —
/// the second stage that was meant to promote them is the model pass, and until that runs the user
/// sees nothing at all from their own captured work. Corroboration is the part of that judgement
/// that can be made locally and honestly: the same promise seen in four separate events is not a
/// parsing accident.
///
/// Only ever raises, never lowers (decay is [`decay_confidence`]'s job), and is idempotent — the
/// target is computed from the evidence count, not accumulated.
///
/// Returns the number of rows raised.
pub fn corroborate(conn: &mut Connection) -> Result<usize, rusqlite::Error> {
    let mut raised = 0usize;
    let tx = conn.transaction()?;
    for table in CONFIDENCE_TABLES {
        // Distinct events behind each row, via the provenance join.
        let sql = format!(
            "SELECT s.id, s.confidence, count(DISTINCT p.event_id)
               FROM {table} s
               JOIN state_provenance p
                 ON p.state_table = '{table}' AND p.state_id = s.id
              GROUP BY s.id
             HAVING count(DISTINCT p.event_id) > 1"
        );
        let mut stmt = tx.prepare(&sql)?;
        let rows: Vec<(i64, f64, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (id, confidence, evidence) in rows {
            let target = corroborated_confidence(evidence as f64);
            if target > confidence {
                tx.execute(
                    &format!("UPDATE {table} SET confidence = ?1 WHERE id = ?2"),
                    params![target, id],
                )?;
                raised += 1;
            }
        }
    }
    tx.commit()?;
    Ok(raised)
}

/// The confidence `evidence_count` independent sightings justify, with diminishing returns and a
/// hard ceiling below High. Pure, so the curve is testable on its own.
pub fn corroborated_confidence(evidence_count: f64) -> f64 {
    if evidence_count <= 1.0 {
        return 0.0; // a single sighting is not corroboration
    }
    // Asymptotic rather than linear: each additional sighting adds less than the one before, and
    // the ceiling is approached but never reached. More evidence should always help a little and
    // never harden into certainty — that judgement needs the model pass.
    let extra = evidence_count - 1.0;
    let scale = extra / (extra + CORROBORATION_SHAPE);
    0.5 + scale * (CORROBORATION_CEILING - 0.5)
}

#[cfg(test)]
mod corroboration_tests {
    use super::*;
    use crate::state::{insert_commitment, CommitmentDirection, CommitmentStatus, NewCommitment, Provenance};

    #[test]
    fn one_sighting_is_not_corroboration() {
        assert_eq!(corroborated_confidence(1.0), 0.0);
        assert_eq!(corroborated_confidence(0.0), 0.0);
    }

    #[test]
    fn more_independent_evidence_means_more_confidence_with_diminishing_returns() {
        let two = corroborated_confidence(2.0);
        let three = corroborated_confidence(3.0);
        let four = corroborated_confidence(4.0);
        assert!(two < three && three < four, "{two} {three} {four}");
        assert!(four - three < three - two, "returns must diminish");
    }

    /// The invariant this whole mechanism rests on: corroboration alone can make a row *offerable*
    /// ("possibly …") but never *assertable*. Only a model pass may reach High.
    #[test]
    fn corroboration_can_never_reach_the_high_band() {
        for n in [2.0, 4.0, 10.0, 1000.0] {
            let c = corroborated_confidence(n);
            assert!(c < 0.8, "{n} sightings reached High ({c}) — that must need a model");
            assert!(c >= 0.5, "corroborated rows must at least be offerable: {c}");
        }
    }

    fn seed(conn: &mut Connection, evidence: usize) -> i64 {
        let events: Vec<i64> = (0..evidence)
            .map(|i| {
                crate::event_log::insert(
                    conn,
                    &crate::event_log::NewEvent {
                        ts: 1 + i as i64,
                        source: "capture",
                        kind: "text",
                        app_bundle_id: Some("com.apple.Mail"),
                        window_title: Some("t"),
                        content: &format!("evidence {i}"),
                        content_hash: &format!("h{i}"),
                        dwell_ms: 0,
                        display_id: None,
                        window_bounds: None,
                    },
                )
                .unwrap()
            })
            .collect();
        let prov: Vec<Provenance> = events.iter().map(|e| Provenance::new(*e)).collect();
        insert_commitment(
            conn,
            &NewCommitment {
                direction: CommitmentDirection::Mine,
                counterparty_id: None,
                description: "send the deck",
                due_at: None,
                status: CommitmentStatus::Open,
                project_id: None,
                confidence: 0.35, // what a local rule assigns — Low, so currently invisible
                now: 1,
            },
            &prov,
        )
        .unwrap()
    }

    #[test]
    fn a_repeatedly_evidenced_commitment_becomes_visible() {
        let mut conn = crate::open_in_memory().unwrap();
        let id = seed(&mut conn, 3);
        assert_eq!(corroborate(&mut conn).unwrap(), 1);
        let c: f64 = conn
            .query_row("SELECT confidence FROM commitments WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert!(c >= 0.5, "must clear the Low band it was stuck below: {c}");
        assert!(c < 0.8, "but not become assertable: {c}");
    }

    #[test]
    fn a_single_sighting_is_left_alone() {
        let mut conn = crate::open_in_memory().unwrap();
        let id = seed(&mut conn, 1);
        assert_eq!(corroborate(&mut conn).unwrap(), 0);
        let c: f64 = conn
            .query_row("SELECT confidence FROM commitments WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert!((c - 0.35).abs() < 1e-9, "unchanged: {c}");
    }

    #[test]
    fn running_twice_changes_nothing_and_never_lowers() {
        let mut conn = crate::open_in_memory().unwrap();
        let id = seed(&mut conn, 4);
        corroborate(&mut conn).unwrap();
        let first: f64 = conn
            .query_row("SELECT confidence FROM commitments WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(corroborate(&mut conn).unwrap(), 0, "idempotent");
        let second: f64 = conn
            .query_row("SELECT confidence FROM commitments WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(first, second);

        // A row already above the corroboration ceiling (model-verified) must not be pulled down.
        conn.execute("UPDATE commitments SET confidence = 0.95 WHERE id = ?1", [id]).unwrap();
        corroborate(&mut conn).unwrap();
        let after: f64 = conn
            .query_row("SELECT confidence FROM commitments WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert!((after - 0.95).abs() < 1e-9, "corroboration must never lower: {after}");
    }
}
