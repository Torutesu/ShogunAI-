//! State-tables repository (§6.4, FR-ST-01/02).
//!
//! Every state row is inserted together with at least one provenance event, atomically. The
//! guard [`insert_with_provenance`] rejects an empty provenance list (FR-ST-02: "no code path
//! inserts a state row with empty provenance") — this is the repository-layer half of that
//! invariant; the schema keeps provenance in a separate `state_provenance` table so it can be
//! many-to-many. Typed inserts (e.g. [`insert_person`]) are thin wrappers over the guard.

use rusqlite::{params, Connection, Transaction};

use crate::MemoryError;

/// The four state tables (FR-ST-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateTable {
    People,
    Projects,
    Commitments,
    OpenLoops,
}

impl StateTable {
    pub fn as_str(self) -> &'static str {
        match self {
            StateTable::People => "people",
            StateTable::Projects => "projects",
            StateTable::Commitments => "commitments",
            StateTable::OpenLoops => "open_loops",
        }
    }
}

/// One provenance link: the event that evidences a state row, with a weight (FR-ST-02).
#[derive(Debug, Clone, Copy)]
pub struct Provenance {
    pub event_id: i64,
    pub weight: f64,
}

impl Provenance {
    pub fn new(event_id: i64) -> Self {
        Self { event_id, weight: 1.0 }
    }
}

/// Insert a state row and its provenance atomically. `insert_row` performs the table-specific
/// INSERT inside the transaction and returns the new row id; this guard then writes one
/// `state_provenance` row per link. An empty `provenance` is rejected before any write
/// (FR-ST-02), so a provenance-less state row can never be committed.
pub fn insert_with_provenance<F>(
    conn: &mut Connection,
    table: StateTable,
    provenance: &[Provenance],
    insert_row: F,
) -> Result<i64, MemoryError>
where
    F: FnOnce(&Transaction<'_>) -> Result<i64, rusqlite::Error>,
{
    if provenance.is_empty() {
        return Err(MemoryError::EmptyProvenance);
    }
    let tx = conn.transaction()?;
    let id = insert_row(&tx)?;
    for p in provenance {
        tx.execute(
            "INSERT INTO state_provenance (state_table, state_id, event_id, weight)
             VALUES (?1, ?2, ?3, ?4)",
            params![table.as_str(), id, p.event_id, p.weight],
        )?;
    }
    tx.commit()?;
    Ok(id)
}

/// A new person (§6.4.2). JSON fields are pre-serialized strings (the caller owns the shape).
#[derive(Debug, Clone, Default)]
pub struct NewPerson<'a> {
    pub display_name: &'a str,
    pub aliases_json: Option<&'a str>,
    pub emails_json: Option<&'a str>,
    pub handles_json: Option<&'a str>,
    pub relationship_summary: Option<&'a str>,
    pub confidence: f64,
    pub now: i64,
}

/// Insert a person with provenance (FR-ST-02). `confidence` must be in 0.0..=1.0 (the schema
/// CHECK enforces it; an out-of-range value returns an error and rolls back).
pub fn insert_person(
    conn: &mut Connection,
    person: &NewPerson<'_>,
    provenance: &[Provenance],
) -> Result<i64, MemoryError> {
    insert_with_provenance(conn, StateTable::People, provenance, |tx| {
        tx.execute(
            "INSERT INTO people
               (display_name, aliases, emails, handles, relationship_summary,
                confidence, created_at, updated_at, last_evidence_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?7)",
            params![
                person.display_name,
                person.aliases_json,
                person.emails_json,
                person.handles_json,
                person.relationship_summary,
                person.confidence,
                person.now,
            ],
        )?;
        Ok(tx.last_insert_rowid())
    })
}

/// Count provenance links for a state row (test / diagnostics helper).
pub fn provenance_count(conn: &Connection, table: StateTable, state_id: i64) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT count(*) FROM state_provenance WHERE state_table = ?1 AND state_id = ?2",
        params![table.as_str(), state_id],
        |r| r.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_log::{insert as insert_event, NewEvent};

    fn seed_event(conn: &Connection, hash: &str) -> i64 {
        insert_event(
            conn,
            &NewEvent {
                ts: 1,
                source: "capture",
                kind: "text",
                app_bundle_id: None,
                window_title: None,
                content: "evidence",
                content_hash: hash,
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap()
    }

    fn person(name: &str, confidence: f64) -> NewPerson<'_> {
        NewPerson { display_name: name, confidence, now: 100, ..Default::default() }
    }

    #[test]
    fn insert_person_with_provenance_succeeds() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn, "h1");
        let id = insert_person(&mut conn, &person("Alice", 0.9), &[Provenance::new(e)]).unwrap();
        assert_eq!(provenance_count(&conn, StateTable::People, id).unwrap(), 1);
    }

    #[test]
    fn empty_provenance_is_rejected_and_nothing_written() {
        let mut conn = crate::open_in_memory().unwrap();
        let err = insert_person(&mut conn, &person("Bob", 0.9), &[]);
        assert!(matches!(err, Err(MemoryError::EmptyProvenance)));
        let count: i64 = conn.query_row("SELECT count(*) FROM people", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0, "no person row may exist after a rejected insert");
    }

    #[test]
    fn multiple_provenance_events_all_recorded() {
        let mut conn = crate::open_in_memory().unwrap();
        let e1 = seed_event(&conn, "h1");
        let e2 = seed_event(&conn, "h2");
        let id = insert_person(
            &mut conn,
            &person("Carol", 0.8),
            &[Provenance::new(e1), Provenance { event_id: e2, weight: 0.5 }],
        )
        .unwrap();
        assert_eq!(provenance_count(&conn, StateTable::People, id).unwrap(), 2);
    }

    #[test]
    fn bad_confidence_rolls_back_the_whole_insert() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn, "h1");
        // confidence 2.0 violates the schema CHECK — the transaction must roll back, leaving
        // neither the person row nor any provenance.
        let res = insert_person(&mut conn, &person("Dave", 2.0), &[Provenance::new(e)]);
        assert!(res.is_err());
        let people: i64 = conn.query_row("SELECT count(*) FROM people", [], |r| r.get(0)).unwrap();
        let prov: i64 = conn.query_row("SELECT count(*) FROM state_provenance", [], |r| r.get(0)).unwrap();
        assert_eq!(people, 0);
        assert_eq!(prov, 0);
    }

    #[test]
    fn provenance_fk_requires_a_real_event() {
        let mut conn = crate::open_in_memory().unwrap();
        // event_id 999 does not exist — the FK on state_provenance.event_id must reject it and
        // roll the person insert back.
        let res = insert_person(&mut conn, &person("Eve", 0.9), &[Provenance::new(999)]);
        assert!(res.is_err());
        let people: i64 = conn.query_row("SELECT count(*) FROM people", [], |r| r.get(0)).unwrap();
        assert_eq!(people, 0);
    }
}
