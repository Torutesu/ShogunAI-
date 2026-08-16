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

pub mod ai_session;
pub mod briefs;
pub mod cold;
pub mod compression_metrics;
pub mod embed;
pub mod embed_job;
/// The real ONNX embedder — see the crate feature note.
#[cfg(feature = "onnx")]
pub mod embed_onnx;
pub mod event_log;
pub mod extract;
pub mod hot;
pub mod identity;
pub mod jobs;
pub mod lessons;
pub mod maintenance;
pub mod meeting_index;
pub mod meeting_recaps;
pub mod quantize;
pub mod redact;
pub mod recompute;
pub mod retention;
pub mod screen_frames;
pub mod search;
pub mod session;
pub mod session_notes;
pub mod state;
pub mod thread;
pub mod transcript_segments;
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

/// Apply migrations that were skipped because a *later* version was recorded first.
///
/// refinery refuses to open a database where a migration on the filesystem carries a version
/// below the highest applied one and was never applied (`Kind::MissingVersion`), and the refusal
/// is total: the store does not open, so capture, search and ⌥-tap drafting are all dead while
/// the only visible symptom is one stderr line. No user can cause this — it takes two migrations
/// authored out of order, which is exactly what happened on 2026-08-09, when `V16__lessons`
/// landed 51 seconds before `V15__briefs`. Every database migrated in that window records V16
/// and no V15, and every build since refuses to open it.
///
/// Memory lives for years (CLAUDE.md), so an authoring slip must not be able to end one. The
/// stragglers are applied here in version order, each in its own transaction, recorded with the
/// embedded migration's own name and checksum so refinery's consistency check passes on the next
/// open rather than tripping on a forged row.
///
/// Returns the versions repaired, so the caller can say out loud that this database was healed
/// rather than opened clean.
fn repair_skipped_migrations(conn: &mut Connection) -> Result<Vec<u32>, MemoryError> {
    use rusqlite::OptionalExtension;

    // A fresh database has no history table: nothing was skipped, and every later query here
    // would fail on a table that is about to be created by the runner itself.
    let has_history = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'refinery_schema_history'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !has_history {
        return Ok(Vec::new());
    }

    let applied: Vec<u32> = {
        let mut stmt = conn.prepare("SELECT version FROM refinery_schema_history")?;
        let rows = stmt.query_map([], |r| r.get::<_, u32>(0))?;
        rows.collect::<Result<Vec<u32>, _>>()?
    };
    // An empty history is the same "nothing to repair" case as no table at all.
    let Some(&current) = applied.iter().max() else {
        return Ok(Vec::new());
    };

    // The same predicate refinery aborts on: on the filesystem, not applied, and not above the
    // current version (anything above is ordinary pending work the runner will apply itself).
    let mut skipped: Vec<refinery::Migration> = embedded::runner()
        .get_migrations()
        .iter()
        .filter(|m| m.version() <= current && !applied.contains(&m.version()))
        .cloned()
        .collect();
    skipped.sort_by_key(|m| m.version());

    let mut repaired = Vec::new();
    for migration in skipped {
        // A migration with no SQL body cannot be replayed. Skip it rather than record a history
        // row for something that never ran — a truthful failure beats a forged success.
        let Some(sql) = migration.sql() else { continue };
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        // `applied_on` is written by SQLite so this needs no clock dependency; the format is the
        // RFC 3339 refinery parses back on every subsequent open.
        tx.execute(
            "INSERT INTO refinery_schema_history (version, name, applied_on, checksum) \
             VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%S+00:00', 'now'), ?3)",
            rusqlite::params![migration.version(), migration.name(), migration.checksum().to_string()],
        )?;
        tx.commit()?;
        repaired.push(migration.version());
    }
    Ok(repaired)
}

/// Run migrations to the latest version, then a quick integrity check (NFR-REL-01).
fn migrate_and_check(conn: &mut Connection) -> Result<(), MemoryError> {
    let repaired = repair_skipped_migrations(conn)?;
    if !repaired.is_empty() {
        // Loud on purpose. This is a schema history that should never have existed, and the line
        // is the only record that the store was healed instead of opened clean. Version numbers
        // only — nothing from a row reaches this.
        eprintln!("[memory] repaired out-of-order migration history — applied skipped {repaired:?}");
    }
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

/// A 256-bit database encryption key.
///
/// The key never appears in a `Debug` render, so it cannot reach a log through a derived
/// `{:?}` on some enclosing struct. It is passed in rather than read here: this crate is
/// Linux-testable and must not know about the Keychain (the key's only permitted home,
/// invariant 7) — the desktop layer reads it and hands it over.
#[derive(Clone)]
pub struct DbKey([u8; 32]);

impl DbKey {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Parse the 64-char hex form used to store the key in the Keychain.
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(Self(out))
    }

    /// The hex form, for storing in the Keychain.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The SQLCipher `PRAGMA key` argument. Raw-key form (`x'…'`) so SQLCipher uses these bytes
    /// directly instead of running a passphrase KDF, and so the value can never need quoting or
    /// escaping — it is hex digits between fixed delimiters.
    fn pragma_value(&self) -> String {
        format!("\"x'{}'\"", self.to_hex())
    }
}

impl std::fmt::Debug for DbKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DbKey(***redacted***)")
    }
}

/// Apply the encryption key. Must run before any other statement on the connection — SQLCipher
/// reads the header on first access, and a key set afterwards is rejected.
fn apply_key(conn: &Connection, key: &DbKey) -> Result<(), MemoryError> {
    conn.execute_batch(&format!("PRAGMA key = {};", key.pragma_value()))?;
    Ok(())
}

/// Open (creating if needed) an **encrypted** database at `path` (FR-SEC: memory at rest).
///
/// Same contract as [`open`] otherwise. A wrong key surfaces as an integrity error from the first
/// read, not as silent garbage: SQLCipher cannot decrypt the header and the migration runner's
/// first query fails.
pub fn open_encrypted(path: impl AsRef<Path>, key: &DbKey) -> Result<Connection, MemoryError> {
    vector::register_extension();
    let mut conn = Connection::open(path)?;
    apply_key(&conn, key)?;
    apply_pragmas(&conn, true)?;
    migrate_and_check(&mut conn)?;
    Ok(conn)
}

/// Convert an existing plaintext database into an encrypted one at `dest`, via SQLCipher's
/// `sqlcipher_export`. The source is left untouched, so a failure loses nothing and the caller
/// decides when to swap the files.
///
/// This is the upgrade path for installs created before encryption: a plaintext database cannot
/// simply be given a key, its pages have to be rewritten.
pub fn encrypt_existing(
    plaintext: impl AsRef<Path>,
    dest: impl AsRef<Path>,
    key: &DbKey,
) -> Result<(), MemoryError> {
    vector::register_extension();
    let conn = Connection::open(plaintext)?;
    let dest = dest.as_ref().to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!(
        "ATTACH DATABASE '{dest}' AS encrypted KEY {};
         SELECT sqlcipher_export('encrypted');
         DETACH DATABASE encrypted;",
        key.pragma_value()
    ))?;
    Ok(())
}

/// Whether the file at `path` is an unencrypted SQLite database — i.e. still needs
/// [`encrypt_existing`]. A plaintext database starts with SQLite's magic header; an encrypted one
/// starts with random salt, so the check is a 16-byte read and never opens the database.
pub fn is_plaintext_db(path: impl AsRef<Path>) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    let mut head = [0u8; 16];
    if f.read_exact(&mut head).is_err() {
        return false;
    }
    &head == b"SQLite format 3\0"
}

/// Open an in-memory database, migrated to the latest schema — for tests and ephemeral use.
pub fn open_in_memory() -> Result<Connection, MemoryError> {
    vector::register_extension();
    let mut conn = Connection::open_in_memory()?;
    apply_pragmas(&conn, false)?;
    migrate_and_check(&mut conn)?;
    Ok(conn)
}

/// The highest migration bundled in `src/migrations`. Tests assert against this rather than a
/// literal so that adding a migration updates one place, not five — and so a *drop* in version
/// (a migration file lost in a merge) still fails loudly.
pub const LATEST_SCHEMA_VERSION: u32 = 19;

/// The schema version the migrations bring the database to (max applied version), or `None`
/// if no migrations are recorded.
pub fn schema_version(conn: &Connection) -> Result<Option<u32>, rusqlite::Error> {
    conn.query_row(
        "SELECT MAX(version) FROM refinery_schema_history",
        [],
        |r| r.get::<_, Option<u32>>(0),
    )
}

/// Encryption at rest. These tests assert the property that matters — the file on disk is not
/// readable without the key — rather than just that the API returns Ok.
#[cfg(test)]
mod encryption_tests {
    use super::*;

    fn key(seed: u8) -> DbKey {
        DbKey::new([seed; 32])
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("shogun_enc_{}_{}.db", std::process::id(), name))
    }

    #[test]
    fn hex_round_trips_and_rejects_malformed_keys() {
        let k = key(0xab);
        assert_eq!(k.to_hex().len(), 64);
        assert_eq!(DbKey::from_hex(&k.to_hex()).unwrap().to_hex(), k.to_hex());
        assert!(DbKey::from_hex("too short").is_none());
        assert!(DbKey::from_hex(&"z".repeat(64)).is_none(), "non-hex rejected");
    }

    #[test]
    fn the_key_never_renders_in_debug_output() {
        let rendered = format!("{:?}", key(0xff));
        assert!(!rendered.contains("ff"), "key bytes must not reach a log: {rendered}");
        assert_eq!(rendered, "DbKey(***redacted***)");
    }

    #[test]
    fn an_encrypted_database_is_not_readable_without_the_key() {
        let path = temp("locked");
        let _ = std::fs::remove_file(&path);
        {
            let conn = open_encrypted(&path, &key(1)).expect("create encrypted");
            conn.execute(
                "INSERT INTO event_log (ts, source, kind, content, content_hash, last_seen_at, dwell_ms)
                 VALUES (1, 'capture', 'text', 'the quarterly numbers', 'h1', 1, 0)",
                [],
            )
            .unwrap();
        }

        // The bytes on disk must not be a plaintext SQLite file, and must not contain the content.
        assert!(!is_plaintext_db(&path), "an encrypted DB must not carry the SQLite header");
        let raw = std::fs::read(&path).unwrap();
        assert!(
            !raw.windows(21).any(|w| w == b"the quarterly numbers"),
            "captured content must not be readable in the file"
        );

        // Wrong key: refused, not silently empty.
        assert!(open_encrypted(&path, &key(2)).is_err(), "a wrong key must fail loudly");
        // Right key: the row is there.
        let conn = open_encrypted(&path, &key(1)).expect("reopen with the right key");
        let n: i64 = conn.query_row("SELECT count(*) FROM event_log", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_existing_plaintext_database_can_be_migrated_without_losing_data() {
        let plain = temp("plain");
        let enc = temp("converted");
        let _ = std::fs::remove_file(&plain);
        let _ = std::fs::remove_file(&enc);
        {
            let conn = open(&plain).expect("create plaintext");
            conn.execute(
                "INSERT INTO event_log (ts, source, kind, content, content_hash, last_seen_at, dwell_ms)
                 VALUES (1, 'capture', 'text', 'pre-existing memory', 'h1', 1, 0)",
                [],
            )
            .unwrap();
        }
        assert!(is_plaintext_db(&plain), "the old format is detectable");

        encrypt_existing(&plain, &enc, &key(7)).expect("convert");

        // Converted copy: same data, now encrypted. The source is untouched, so a failed upgrade
        // never destroys the user's memory.
        assert!(!is_plaintext_db(&enc));
        assert!(is_plaintext_db(&plain), "source left intact");
        let conn = open_encrypted(&enc, &key(7)).expect("open converted");
        let content: String =
            conn.query_row("SELECT content FROM event_log", [], |r| r.get(0)).unwrap();
        assert_eq!(content, "pre-existing memory");
        assert_eq!(schema_version(&conn).unwrap(), Some(LATEST_SCHEMA_VERSION), "schema carried over");

        let _ = std::fs::remove_file(&plain);
        let _ = std::fs::remove_file(&enc);
    }
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
            "feedback_events",
            "lessons",
            "lesson_provenance",
            "briefs",
        ] {
            assert!(tables.iter().any(|t| t == expected), "missing table {expected}");
        }
        assert_eq!(schema_version(&conn).unwrap(), Some(LATEST_SCHEMA_VERSION));
    }

    #[test]
    fn migrations_are_idempotent_on_reopen() {
        // Apply to a temp file, close, reopen — the second run must be a no-op, not re-run.
        let dir = std::env::temp_dir();
        let path = dir.join(format!("shogun_mem_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);

        {
            let conn = open(&path).unwrap();
            assert_eq!(schema_version(&conn).unwrap(), Some(LATEST_SCHEMA_VERSION));
        }
        {
            // Reopen: migrate_and_check runs again, finds nothing new, and passes quick_check.
            let conn = open(&path).unwrap();
            assert_eq!(schema_version(&conn).unwrap(), Some(LATEST_SCHEMA_VERSION));
            let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
            assert_eq!(mode.to_lowercase(), "wal");
        }
        let _ = std::fs::remove_file(&path);
    }

    /// The 2026-08-09 authoring slip, pinned: `V16__lessons` was committed 51 seconds before
    /// `V15__briefs`, so a database migrated in that window records V16 and never V15. refinery
    /// answers that with a hard error and the whole store stops opening — capture, search and
    /// drafting all die on a mistake no user made. The repair must reopen it.
    #[test]
    fn a_skipped_migration_is_repaired_instead_of_bricking_the_store() {
        // `open_in_memory` would migrate on the way out; this test needs to drive the migration
        // steps by hand, so it does that function's other half — vec0 — itself.
        vector::register_extension();
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_and_check(&mut conn).unwrap();

        // Reproduce the broken history exactly: V15 never applied, everything above it recorded.
        conn.execute("DELETE FROM refinery_schema_history WHERE version = 15", []).unwrap();
        conn.execute_batch("DROP TABLE briefs;").unwrap();

        // Left to itself, refinery refuses this database outright — that is the bug being fixed.
        assert!(
            embedded::runner().run(&mut conn).is_err(),
            "refinery must still reject the skipped version, or this test proves nothing"
        );

        // The repair replays the straggler, and the normal open path then completes.
        migrate_and_check(&mut conn).unwrap();
        let recorded: i64 = conn
            .query_row("SELECT COUNT(*) FROM refinery_schema_history WHERE version = 15", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(recorded, 1, "the repaired migration must be recorded once");
        // The table the migration exists to create is back, and the schema is whole again.
        assert!(table_names(&conn).contains(&"briefs".to_string()));
        assert_eq!(schema_version(&conn).unwrap(), Some(LATEST_SCHEMA_VERSION));

        // And the healed database opens again with no further repair — the history it wrote has
        // to satisfy refinery's own name/checksum check, not merely silence this run.
        migrate_and_check(&mut conn).unwrap();
        assert!(repair_skipped_migrations(&mut conn).unwrap().is_empty());
    }

    /// The repair must be inert on every healthy database: a fresh store and a fully-migrated one
    /// both have nothing to replay, and a repair that fired here would be rewriting real history.
    #[test]
    fn repair_is_a_no_op_on_a_healthy_database() {
        vector::register_extension();
        let mut fresh = Connection::open_in_memory().unwrap();
        assert!(repair_skipped_migrations(&mut fresh).unwrap().is_empty(), "no history table yet");

        migrate_and_check(&mut fresh).unwrap();
        assert!(repair_skipped_migrations(&mut fresh).unwrap().is_empty(), "nothing was skipped");
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
