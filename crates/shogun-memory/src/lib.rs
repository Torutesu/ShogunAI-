//! SHOGUN memory: SQLite schema, versioned migrations, and (in later WPs) the event log,
//! 3-tier memory, state tables, and hybrid search.
//!
//! This WP (WP2.1) establishes the durable substrate: a WAL-mode connection with crash-safe
//! pragmas (NFR-REL-01), refinery-managed versioned migrations (FR-MEM-30 — no hand-written
//! ALTER TABLE in app code), and a startup integrity check. The schema is in
//! `src/migrations/*.sql`; it is the source of truth (FR-MEM-11 says "the migration is
//! authoritative").
//!
//! Data centre of gravity is Rust (AR-04): the webview never opens this database.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::Path;

use rusqlite::Connection;

pub mod cold;
pub mod embed;
pub mod embed_job;
pub mod event_log;
pub mod extract;
pub mod hot;
pub mod jobs;
pub mod maintenance;
pub mod quantize;
pub mod recompute;
pub mod search;
pub mod state;
pub mod traceability;
pub mod vector;

/// refinery embeds the `src/migrations/V*.sql` files at compile time; `migrations::runner()`
/// applies any not yet recorded in the `refinery_schema_history` table.
mod embedded {
    refinery::embed_migrations!("src/migrations");
    pub use migrations::runner;
}

/// Errors from opening / migrating / checking the database.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration: {0}")]
    Migration(#[from] refinery::Error),
    #[error("integrity check failed: {0}")]
    Integrity(String),
    /// A state row was asked to be inserted with no provenance (FR-ST-02 forbids it).
    #[error("state row insert requires at least one provenance event (FR-ST-02)")]
    EmptyProvenance,
}

/// Apply the crash-safety pragmas (NFR-REL-01). `wal` is skipped for in-memory databases,
/// where journal modes other than MEMORY are not meaningful.
fn apply_pragmas(conn: &Connection, wal: bool) -> Result<(), rusqlite::Error> {
    if wal {
        // WAL + NORMAL sync survives power loss without the full fsync cost (NFR-REL-01).
        conn.pragma_update(None, "journal_mode", "WAL")?;
    }
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// Run migrations to the latest version, then a quick integrity check (NFR-REL-01).
fn migrate_and_check(conn: &mut Connection) -> Result<(), MemoryError> {
    embedded::runner().run(conn)?;
    let status: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if status != "ok" {
        return Err(MemoryError::Integrity(status));
    }
    Ok(())
}

/// Open (creating if needed) the database at `path` in WAL mode, apply pragmas, migrate to the
/// latest schema, and run an integrity check. This is the product entry point.
pub fn open(path: impl AsRef<Path>) -> Result<Connection, MemoryError> {
    vector::register_extension();
    let mut conn = Connection::open(path)?;
    apply_pragmas(&conn, true)?;
    migrate_and_check(&mut conn)?;
    Ok(conn)
}

/// Open an in-memory database, migrated to the latest schema — for tests and ephemeral use.
pub fn open_in_memory() -> Result<Connection, MemoryError> {
    vector::register_extension();
    let mut conn = Connection::open_in_memory()?;
    apply_pragmas(&conn, false)?;
    migrate_and_check(&mut conn)?;
    Ok(conn)
}

/// The schema version the migrations bring the database to (max applied version), or `None`
/// if no migrations are recorded.
pub fn schema_version(conn: &Connection) -> Result<Option<u32>, rusqlite::Error> {
    conn.query_row(
        "SELECT MAX(version) FROM refinery_schema_history",
        [],
        |r| r.get::<_, Option<u32>>(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    }

    #[test]
    fn migrations_create_the_v1_schema() {
        let conn = open_in_memory().unwrap();
        let tables = table_names(&conn);
        for expected in [
            "event_log",
            "people",
            "projects",
            "commitments",
            "open_loops",
            "state_provenance",
            "traceability_log",
        ] {
            assert!(tables.iter().any(|t| t == expected), "missing table {expected}");
        }
        assert_eq!(schema_version(&conn).unwrap(), Some(4));
    }

    #[test]
    fn migrations_are_idempotent_on_reopen() {
        // Apply to a temp file, close, reopen — the second run must be a no-op, not re-run.
        let dir = std::env::temp_dir();
        let path = dir.join(format!("shogun_mem_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);

        {
            let conn = open(&path).unwrap();
            assert_eq!(schema_version(&conn).unwrap(), Some(4));
        }
        {
            // Reopen: migrate_and_check runs again, finds nothing new, and passes quick_check.
            let conn = open(&path).unwrap();
            assert_eq!(schema_version(&conn).unwrap(), Some(4));
            let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
            assert_eq!(mode.to_lowercase(), "wal");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fts_indexes_inserted_events() {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO event_log (ts, source, kind, content, content_hash, last_seen_at)
             VALUES (?1, 'capture', 'text', ?2, 'h1', ?1)",
            rusqlite::params![1000i64, "the quarterly roadmap review"],
        )
        .unwrap();
        // Trigram FTS match on a substring.
        let hits: i64 = conn
            .query_row(
                "SELECT count(*) FROM event_fts WHERE event_fts MATCH 'roadmap'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);
    }

    #[test]
    fn fts_follows_delete() {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO event_log (ts, source, kind, content, content_hash, last_seen_at)
             VALUES (1, 'capture', 'text', 'deletable content', 'h', 1)",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM event_log WHERE content_hash='h'", []).unwrap();
        let hits: i64 = conn
            .query_row("SELECT count(*) FROM event_fts WHERE event_fts MATCH 'deletable'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hits, 0);
    }

    #[test]
    fn confidence_out_of_range_is_rejected() {
        let conn = open_in_memory().unwrap();
        let bad = conn.execute(
            "INSERT INTO people (display_name, confidence, created_at, updated_at, last_evidence_at)
             VALUES ('x', 1.5, 0, 0, 0)",
            [],
        );
        assert!(bad.is_err(), "confidence 1.5 must violate the CHECK");
        let ok = conn.execute(
            "INSERT INTO people (display_name, confidence, created_at, updated_at, last_evidence_at)
             VALUES ('x', 0.7, 0, 0, 0)",
            [],
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn state_enums_are_constrained() {
        let conn = open_in_memory().unwrap();
        let bad = conn.execute(
            "INSERT INTO projects (name, status, confidence, created_at, updated_at, last_evidence_at)
             VALUES ('p', 'bogus', 0.9, 0, 0, 0)",
            [],
        );
        assert!(bad.is_err(), "status='bogus' must violate the CHECK");
    }

    #[test]
    fn strict_typing_rejects_wrong_type() {
        let conn = open_in_memory().unwrap();
        // ts is INTEGER in a STRICT table; a non-integer must be rejected.
        let bad = conn.execute(
            "INSERT INTO event_log (ts, source, kind, content, content_hash, last_seen_at)
             VALUES ('not-a-number', 'capture', 'text', 'c', 'h', 1)",
            [],
        );
        assert!(bad.is_err(), "STRICT table must reject a text ts");
    }
}
