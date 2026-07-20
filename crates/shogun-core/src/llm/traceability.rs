//! Traceability recording for every outbound chunk (AR-11 / §6.14, CLAUDE.md invariant 3 & G8).
//!
//! Whenever a processed chunk leaves the device, exactly one [`TraceRecord`] is written. The
//! record holds only the chunk's **byte length** and an **xxh64 digest** — never the text.
//! [`TraceRecord::for_chunk`] is the single constructor and it hashes-and-drops the chunk, so
//! there is no path by which a sink could persist the sent text. The record's fields line up
//! 1:1 with the `traceability_log` columns (shogun-memory V1__init.sql).

use std::hash::Hasher;

use twox_hash::XxHash64;

/// The route a chunk left by. Matches the `route` CHECK set in `traceability_log`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Batch API, Select KK key (indexing / classification / Dream Cycle / Morning Brief).
    BatchApi,
    /// Messages API, user BYOK key (agent inference / chat / drafts).
    MessagesApi,
    /// A remote MCP integration.
    Mcp,
    /// Composio (third-party relay; always surfaced as "via third party" in the UI).
    Composio,
    /// Billing / licensing calls.
    Billing,
}

impl Route {
    /// The exact string stored in `traceability_log.route`.
    pub fn as_db_str(self) -> &'static str {
        match self {
            Route::BatchApi => "batch_api",
            Route::MessagesApi => "messages_api",
            Route::Mcp => "mcp",
            Route::Composio => "composio",
            Route::Billing => "billing",
        }
    }
}

/// One traceability row. Carries no chunk text — only the length and digest (G8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceRecord {
    pub route: Route,
    pub purpose: String,
    pub destination: String,
    pub chunk_bytes: usize,
    /// Lower-hex xxh64 of the chunk's UTF-8 bytes. A digest, never the text.
    pub chunk_xxh64: String,
    /// True for Composio-relayed sends (third-party disclosure).
    pub third_party: bool,
}

/// The xxh64 digest recorded for an outbound chunk: 16 lower-hex chars. The only thing derived
/// from the chunk that is allowed to persist.
pub fn digest(chunk: &str) -> String {
    let mut h = XxHash64::with_seed(0);
    h.write(chunk.as_bytes());
    format!("{:016x}", h.finish())
}

impl TraceRecord {
    /// Build a record for a chunk that is about to be sent. Captures only its byte length and
    /// digest; the `chunk` argument is consumed for hashing and never stored.
    pub fn for_chunk(
        route: Route,
        purpose: impl Into<String>,
        destination: impl Into<String>,
        chunk: &str,
        third_party: bool,
    ) -> Self {
        Self {
            route,
            purpose: purpose.into(),
            destination: destination.into(),
            chunk_bytes: chunk.len(),
            chunk_xxh64: digest(chunk),
            third_party,
        }
    }
}

/// The sink every external send is recorded to (AR-11). The real implementation writes a
/// `traceability_log` row (shogun-memory); tests use [`RecordingSink`]. A sink is handed a
/// [`TraceRecord`] — which has no text field — so it is structurally unable to store the sent
/// content.
pub trait TraceabilitySink: Send + Sync {
    fn record(&self, rec: TraceRecord);
}

/// An in-memory sink for tests and offline development. Public so downstream crates can assert on
/// what would have been logged.
#[derive(Default)]
pub struct RecordingSink {
    records: std::sync::Mutex<Vec<TraceRecord>>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every record written, in order.
    pub fn records(&self) -> Vec<TraceRecord> {
        self.records.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

impl TraceabilitySink for RecordingSink {
    fn record(&self, rec: TraceRecord) {
        if let Ok(mut g) = self.records.lock() {
            g.push(rec);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_db_strings_match_schema() {
        assert_eq!(Route::BatchApi.as_db_str(), "batch_api");
        assert_eq!(Route::MessagesApi.as_db_str(), "messages_api");
        assert_eq!(Route::Mcp.as_db_str(), "mcp");
        assert_eq!(Route::Composio.as_db_str(), "composio");
        assert_eq!(Route::Billing.as_db_str(), "billing");
    }

    #[test]
    fn digest_is_deterministic_and_hex() {
        let d = digest("the quick brown fox");
        assert_eq!(d.len(), 16);
        assert!(d.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(d, digest("the quick brown fox"));
        assert_ne!(d, digest("the quick brown foy"));
    }

    #[test]
    fn record_holds_digest_and_length_never_text() {
        let chunk = "meeting notes: ship the roadmap by friday";
        let rec = TraceRecord::for_chunk(Route::BatchApi, "classify", "api.anthropic.com", chunk, false);
        assert_eq!(rec.chunk_bytes, chunk.len());
        assert_eq!(rec.chunk_xxh64, digest(chunk));
        // The record's own Debug must not surface the chunk text — there is no field for it.
        let dumped = format!("{rec:?}");
        assert!(!dumped.contains("roadmap"), "chunk text must never appear in a trace record");
    }

    #[test]
    fn recording_sink_captures_in_order() {
        let sink = RecordingSink::new();
        sink.record(TraceRecord::for_chunk(Route::BatchApi, "a", "d", "one", false));
        sink.record(TraceRecord::for_chunk(Route::MessagesApi, "b", "d", "two", false));
        let recs = sink.records();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].purpose, "a");
        assert_eq!(recs[1].route, Route::MessagesApi);
    }
}
