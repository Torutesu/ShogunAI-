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

/// Project status (§6.4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectStatus {
    Active,
    Waiting,
    Paused,
    Done,
}

impl ProjectStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectStatus::Active => "active",
            ProjectStatus::Waiting => "waiting",
            ProjectStatus::Paused => "paused",
            ProjectStatus::Done => "done",
        }
    }
}

/// A new project (§6.4.3).
#[derive(Debug, Clone)]
pub struct NewProject<'a> {
    pub name: &'a str,
    pub status: ProjectStatus,
    pub summary: Option<&'a str>,
    pub participants_json: Option<&'a str>,
    pub sources_json: Option<&'a str>,
    pub confidence: f64,
    pub now: i64,
}

/// Insert a project with provenance (FR-ST-02).
pub fn insert_project(
    conn: &mut Connection,
    project: &NewProject<'_>,
    provenance: &[Provenance],
) -> Result<i64, MemoryError> {
    insert_with_provenance(conn, StateTable::Projects, provenance, |tx| {
        tx.execute(
            "INSERT INTO projects
               (name, status, summary, participants, sources,
                confidence, created_at, updated_at, last_evidence_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?7)",
            params![
                project.name,
                project.status.as_str(),
                project.summary,
                project.participants_json,
                project.sources_json,
                project.confidence,
                project.now,
            ],
        )?;
        Ok(tx.last_insert_rowid())
    })
}

/// Commitment direction and status (§6.4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitmentDirection {
    Mine,
    Theirs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitmentStatus {
    Open,
    Done,
    Overdue,
    Cancelled,
}

impl CommitmentDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            CommitmentDirection::Mine => "mine",
            CommitmentDirection::Theirs => "theirs",
        }
    }
}

impl CommitmentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            CommitmentStatus::Open => "open",
            CommitmentStatus::Done => "done",
            CommitmentStatus::Overdue => "overdue",
            CommitmentStatus::Cancelled => "cancelled",
        }
    }
}

/// A new commitment (§6.4.4). Only for explicit-promise evidence (FR-ST-11).
#[derive(Debug, Clone)]
pub struct NewCommitment<'a> {
    pub direction: CommitmentDirection,
    pub counterparty_id: Option<i64>,
    pub description: &'a str,
    pub due_at: Option<i64>,
    pub status: CommitmentStatus,
    pub project_id: Option<i64>,
    pub confidence: f64,
    pub now: i64,
}

/// Insert a commitment with provenance (FR-ST-02).
pub fn insert_commitment(
    conn: &mut Connection,
    c: &NewCommitment<'_>,
    provenance: &[Provenance],
) -> Result<i64, MemoryError> {
    insert_with_provenance(conn, StateTable::Commitments, provenance, |tx| {
        tx.execute(
            "INSERT INTO commitments
               (direction, counterparty_id, description, due_at, status, project_id,
                confidence, created_at, updated_at, last_evidence_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?8)",
            params![
                c.direction.as_str(),
                c.counterparty_id,
                c.description,
                c.due_at,
                c.status.as_str(),
                c.project_id,
                c.confidence,
                c.now,
            ],
        )?;
        Ok(tx.last_insert_rowid())
    })
}

/// Open-loop kind and status (§6.4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenLoopKind {
    ReplyNeeded,
    WaitingOnThem,
    ReviewPending,
    DecisionPending,
    FollowUp,
    Other,
}

impl OpenLoopKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OpenLoopKind::ReplyNeeded => "reply_needed",
            OpenLoopKind::WaitingOnThem => "waiting_on_them",
            OpenLoopKind::ReviewPending => "review_pending",
            OpenLoopKind::DecisionPending => "decision_pending",
            OpenLoopKind::FollowUp => "follow_up",
            OpenLoopKind::Other => "other",
        }
    }
}

/// A new open loop (§6.4.5). `open` status; close is a later state change with its own
/// provenance (FR-ST-13).
#[derive(Debug, Clone)]
pub struct NewOpenLoop<'a> {
    pub kind: OpenLoopKind,
    pub description: &'a str,
    pub counterparty_id: Option<i64>,
    pub project_id: Option<i64>,
    pub opened_at: i64,
    pub confidence: f64,
    pub now: i64,
}

/// Insert an open loop with provenance (FR-ST-02), status `open`.
pub fn insert_open_loop(
    conn: &mut Connection,
    l: &NewOpenLoop<'_>,
    provenance: &[Provenance],
) -> Result<i64, MemoryError> {
    insert_with_provenance(conn, StateTable::OpenLoops, provenance, |tx| {
        tx.execute(
            "INSERT INTO open_loops
               (kind, description, counterparty_id, project_id, opened_at, staleness_days,
                status, confidence, created_at, updated_at, last_evidence_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 'open', ?6, ?7, ?7, ?7)",
            params![
                l.kind.as_str(),
                l.description,
                l.counterparty_id,
                l.project_id,
                l.opened_at,
                l.confidence,
                l.now,
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

    #[test]
    fn insert_project_with_provenance() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn, "h1");
        let p = NewProject {
            name: "Roadmap",
            status: ProjectStatus::Active,
            summary: Some("Q3 planning"),
            participants_json: None,
            sources_json: None,
            confidence: 0.9,
            now: 100,
        };
        let id = insert_project(&mut conn, &p, &[Provenance::new(e)]).unwrap();
        assert_eq!(provenance_count(&conn, StateTable::Projects, id).unwrap(), 1);
        let status: String = conn.query_row("SELECT status FROM projects WHERE id=?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(status, "active");
    }

    #[test]
    fn insert_commitment_links_counterparty_and_project() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn, "h1");
        let person_id = insert_person(&mut conn, &person("Frank", 0.9), &[Provenance::new(e)]).unwrap();
        let e2 = seed_event(&conn, "h2");
        let proj = NewProject {
            name: "P",
            status: ProjectStatus::Active,
            summary: None,
            participants_json: None,
            sources_json: None,
            confidence: 0.9,
            now: 100,
        };
        let project_id = insert_project(&mut conn, &proj, &[Provenance::new(e2)]).unwrap();
        let e3 = seed_event(&conn, "h3");
        let c = NewCommitment {
            direction: CommitmentDirection::Mine,
            counterparty_id: Some(person_id),
            description: "Send the report by Friday",
            due_at: Some(1_700_000_000_000),
            status: CommitmentStatus::Open,
            project_id: Some(project_id),
            confidence: 0.85,
            now: 100,
        };
        let id = insert_commitment(&mut conn, &c, &[Provenance::new(e3)]).unwrap();
        assert_eq!(provenance_count(&conn, StateTable::Commitments, id).unwrap(), 1);
    }

    #[test]
    fn commitment_with_dangling_counterparty_is_rejected() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn, "h1");
        let c = NewCommitment {
            direction: CommitmentDirection::Theirs,
            counterparty_id: Some(999), // no such person
            description: "x",
            due_at: None,
            status: CommitmentStatus::Open,
            project_id: None,
            confidence: 0.8,
            now: 100,
        };
        let res = insert_commitment(&mut conn, &c, &[Provenance::new(e)]);
        assert!(res.is_err(), "FK to a missing person must reject the commitment");
        let n: i64 = conn.query_row("SELECT count(*) FROM commitments", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn insert_open_loop_defaults_to_open() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn, "h1");
        let l = NewOpenLoop {
            kind: OpenLoopKind::ReplyNeeded,
            description: "Reply to Alice",
            counterparty_id: None,
            project_id: None,
            opened_at: 50,
            confidence: 0.7,
            now: 100,
        };
        let id = insert_open_loop(&mut conn, &l, &[Provenance::new(e)]).unwrap();
        let (status, kind): (String, String) = conn
            .query_row("SELECT status, kind FROM open_loops WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(status, "open");
        assert_eq!(kind, "reply_needed");
        assert_eq!(provenance_count(&conn, StateTable::OpenLoops, id).unwrap(), 1);
    }
}
