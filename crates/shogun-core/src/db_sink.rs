//! The DB-backed traceability sink (daemon wiring, §6.14 / FR-TR-03). Feature `db`.
//!
//! This is the adapter that closes the traceability loop: the LLM egress records every outbound
//! chunk to a [`TraceabilitySink`] ([`crate::llm::traceability`]), and here that sink persists the
//! record to `traceability_log` via the storage layer ([`shogun_memory::traceability`]). Because
//! the single HTTP egress goes through the sink, and the sink writes a row, an external send that
//! reaches the wire always leaves a trace (FR-TR-03) — enforced structurally, not by discipline.
//!
//! Fire-and-forget by contract: [`TraceabilitySink::record`] returns `()`, so a write failure must
//! never panic or interrupt the user's work (CLAUDE.md crash-resilience). A failed write increments
//! a dropped counter the daemon surfaces via the Notch indicator; it is never an `unwrap`.
//!
//! The sink shares the daemon's connection ([`crate::daemon::SharedConn`]) so traceability rows and
//! capture writes hit the same SQLite handle.

use std::sync::atomic::{AtomicU64, Ordering};

use shogun_memory::traceability::{insert, list, Filter, Route as DbRoute, TraceRow};

use crate::daemon::{Clock, SharedConn};
use crate::llm::traceability::{Route, TraceRecord, TraceabilitySink};

/// Map the LLM-layer route onto the storage-layer route (1:1; the DB CHECK is the shared contract).
fn map_route(route: Route) -> DbRoute {
    match route {
        Route::BatchApi => DbRoute::BatchApi,
        Route::BatchRelay => DbRoute::BatchRelay,
        Route::MessagesApi => DbRoute::MessagesApi,
        Route::Mcp => DbRoute::Mcp,
        Route::Composio => DbRoute::Composio,
        Route::Billing => DbRoute::Billing,
        Route::Asr => DbRoute::Asr,
        Route::LocalAgent => DbRoute::LocalAgent,
        Route::Support => DbRoute::Support,
    }
}

/// A [`TraceabilitySink`] that persists every record to `traceability_log`, sharing the daemon's
/// connection and clock (so the row's `ts` is injectable and deterministic under test).
pub struct DbTraceabilitySink {
    conn: SharedConn,
    clock: Clock,
    dropped: AtomicU64,
}

impl DbTraceabilitySink {
    /// Build a sink over the daemon's shared connection.
    pub fn new(conn: SharedConn, clock: Clock) -> Self {
        Self { conn, clock, dropped: AtomicU64::new(0) }
    }

    /// Number of records that failed to persist (surfaced via the indicator, never fatal).
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Read back rows for the traceability viewer (FR-TR-02) through the same handle. Returns an
    /// empty vec if the lock or query fails (read-only; never panics).
    pub fn rows(&self, filter: &Filter) -> Vec<TraceRow> {
        self.conn.lock().ok().and_then(|c| list(&c, filter).ok()).unwrap_or_default()
    }
}

impl TraceabilitySink for DbTraceabilitySink {
    fn record(&self, rec: TraceRecord) {
        let ts = (self.clock)();
        let chunk_bytes = i64::try_from(rec.chunk_bytes).unwrap_or(i64::MAX);
        let mut row = TraceRow::new(ts, map_route(rec.route), rec.purpose, rec.destination, chunk_bytes, rec.chunk_xxh64);
        // Respect the record's own third-party flag (e.g. a Composio-routed send).
        row.third_party = rec.third_party;

        let wrote = self.conn.lock().ok().and_then(|c| insert(&c, &row).ok());
        if wrote.is_none() {
            // Never interrupt the user's work — count the drop for the indicator and move on.
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Db;
    use crate::llm::traceability::{Route, TraceRecord};

    fn db() -> Db {
        Db::open_in_memory(std::sync::Arc::new(|| 1_000)).expect("in-memory db")
    }

    #[test]
    fn record_persists_a_row_with_digest_only() {
        let db = db();
        let sink = db.traceability_sink();
        // A record carrying obviously-sensitive text; only its digest+length may land in the DB.
        let rec = TraceRecord::for_chunk(Route::BatchApi, "dream_cycle", "api.anthropic.com", "TOP SECRET chunk", false);
        let expected_digest = rec.chunk_xxh64.clone();
        let expected_bytes = rec.chunk_bytes as i64;
        sink.record(rec);

        let rows = sink.rows(&Filter::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].purpose, "dream_cycle");
        assert_eq!(rows[0].route, DbRoute::BatchApi);
        assert_eq!(rows[0].chunk_bytes, expected_bytes);
        assert_eq!(rows[0].chunk_xxh64, expected_digest);
        // The sent text appears nowhere in the persisted row.
        assert!(!format!("{:?}", rows[0]).contains("SECRET"));
        assert_eq!(sink.dropped(), 0);
    }

    #[test]
    fn composio_record_persists_third_party() {
        let sink = db().traceability_sink();
        sink.record(TraceRecord::for_chunk(Route::Composio, "agent", "gmail.com", "body", true));
        let rows = sink.rows(&Filter::default());
        assert_eq!(rows.len(), 1);
        assert!(rows[0].third_party);
    }

    #[test]
    fn each_send_records_exactly_one_row() {
        let sink = db().traceability_sink();
        for i in 0..5 {
            sink.record(TraceRecord::for_chunk(Route::MessagesApi, "agent", "api.anthropic.com", &format!("c{i}"), false));
        }
        assert_eq!(sink.rows(&Filter::default()).len(), 5);
    }

    #[test]
    fn filter_reads_back_through_the_same_handle() {
        let sink = db().traceability_sink();
        sink.record(TraceRecord::for_chunk(Route::BatchApi, "indexing", "api.anthropic.com", "a", false));
        sink.record(TraceRecord::for_chunk(Route::Composio, "agent", "gmail.com", "b", true));
        let composio = sink.rows(&Filter { route: Some(DbRoute::Composio), ..Default::default() });
        assert_eq!(composio.len(), 1);
        assert_eq!(composio[0].destination, "gmail.com");
    }
}
