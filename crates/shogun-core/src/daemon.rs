//! The daemon's DB handle (daemon wiring, feature `db`). One shared SQLite connection that every
//! writer and reader uses — the data-gravity point of the whole system (CLAUDE.md invariant 1: the
//! DB is owned by the Rust core). Cheap to clone (it's an `Arc`), so the capture thread, the LLM
//! egress sink, and the traceability viewer all hold the same handle.
//!
//! Every method here swallows storage errors into an `Option`/empty result rather than
//! propagating a panic: the capture daemon must never crash on a write hiccup (CLAUDE.md
//! crash-resilience). Durable-write concerns (WAL, transactions) live in [`shogun_memory`].
//!
//! Clocks are injected ([`Clock`]) so timestamps are deterministic under test; production passes a
//! real wall-clock closure.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use shogun_memory::event_log::{self, NewEvent};
use shogun_memory::traceability::{Filter, TraceRow};
use shogun_memory::MemoryError;

use crate::db_sink::DbTraceabilitySink;

/// The shared connection handle. `Connection` is `Send` but not `Sync`, so it lives behind a
/// `Mutex`; the `Arc` lets every daemon component share the one handle.
pub type SharedConn = Arc<Mutex<Connection>>;

/// An injected millisecond clock (unix ms). Shared so every writer stamps from the same source.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The daemon's DB handle. Clone freely — clones share the same underlying connection.
#[derive(Clone)]
pub struct Db {
    conn: SharedConn,
    clock: Clock,
}

impl Db {
    /// Wrap an already-open, migrated connection.
    pub fn new(conn: Connection, clock: Clock) -> Self {
        Self { conn: Arc::new(Mutex::new(conn)), clock }
    }

    /// Open the on-disk database (runs migrations) and wrap it.
    pub fn open(path: impl AsRef<std::path::Path>, clock: Clock) -> Result<Self, MemoryError> {
        Ok(Self::new(shogun_memory::open(path)?, clock))
    }

    /// Open a fresh in-memory database (migrations applied) — for tests and ephemeral use.
    pub fn open_in_memory(clock: Clock) -> Result<Self, MemoryError> {
        Ok(Self::new(shogun_memory::open_in_memory()?, clock))
    }

    /// Record a captured event (capture → memory, FR-CAP-03 dedup-touch). Swallows storage errors
    /// so the capture daemon never crashes on a write hiccup; returns `(id, touched)` on success.
    pub fn capture(&self, ev: &NewEvent<'_>) -> Option<(i64, bool)> {
        self.conn.lock().ok().and_then(|c| event_log::insert_or_touch(&c, ev).ok())
    }

    /// A traceability sink that writes through this same handle (the LLM egress records here).
    pub fn traceability_sink(&self) -> DbTraceabilitySink {
        DbTraceabilitySink::new(self.conn.clone(), self.clock.clone())
    }

    /// Read traceability rows for the viewer (FR-TR-02). Empty on any read failure.
    pub fn trace_rows(&self, filter: &Filter) -> Vec<TraceRow> {
        self.conn
            .lock()
            .ok()
            .and_then(|c| shogun_memory::traceability::list(&c, filter).ok())
            .unwrap_or_default()
    }

    /// The current time via the injected clock.
    pub fn now_ms(&self) -> i64 {
        (self.clock)()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::traceability::{Route, TraceRecord};
    use crate::llm::traceability::TraceabilitySink;

    fn clock(v: i64) -> Clock {
        Arc::new(move || v)
    }

    fn ev<'a>(content: &'a str, hash: &'a str, ts: i64) -> NewEvent<'a> {
        NewEvent {
            ts,
            source: "capture",
            kind: "text",
            app_bundle_id: Some("com.apple.Safari"),
            window_title: Some("t"),
            content,
            content_hash: hash,
            dwell_ms: 0,
            display_id: Some(1),
            window_bounds: None,
        }
    }

    #[test]
    fn capture_writes_then_dedup_touches_same_row() {
        let db = Db::open_in_memory(clock(1)).unwrap();
        let (id1, touched1) = db.capture(&ev("hello", "h", 100)).unwrap();
        assert!(!touched1);
        let (id2, touched2) = db.capture(&ev("hello", "h", 200)).unwrap();
        assert!(touched2, "same content_hash must touch, not append");
        assert_eq!(id1, id2);
    }

    #[test]
    fn clones_share_one_connection() {
        // A capture through the handle is visible to a traceability read on a *clone* — proving it
        // is the same underlying connection, not a copy.
        let db = Db::open_in_memory(clock(7)).unwrap();
        let sink = db.clone().traceability_sink();
        sink.record(TraceRecord::for_chunk(Route::BatchApi, "indexing", "api.anthropic.com", "x", false));
        // read via the original handle
        let rows = db.trace_rows(&Filter::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].ts, 7, "the injected clock stamped the row");
    }

    #[test]
    fn capture_and_traceability_hit_the_same_handle() {
        let db = Db::open_in_memory(clock(42)).unwrap();
        db.capture(&ev("note", "h1", 10)).unwrap();
        db.traceability_sink()
            .record(TraceRecord::for_chunk(Route::MessagesApi, "agent", "api.anthropic.com", "chunk", false));
        // both writes landed on the one connection
        assert_eq!(db.trace_rows(&Filter::default()).len(), 1);
    }
}
