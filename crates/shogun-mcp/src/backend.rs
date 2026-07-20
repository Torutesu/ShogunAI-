//! The Memory API data backend seam (§6.11). The routing/auth/confidence policy lives in
//! [`crate::rest`] and [`crate::dispatch`]; the *data* (search hits, state rows) comes from a
//! [`MemoryBackend`]. Dependency is inverted: this trait is defined here, and the daemon
//! (shogun-core, over its `Db`) implements it — so the API layer never depends on the core.
//!
//! Read items carry their `confidence` so the server applies the single confidence rule
//! ([`crate::memory_api::read_inclusion`], FR-API-06) uniformly — the backend supplies rows, it does
//! not decide inclusion.

use crate::memory_api::Tool;

/// One read result: a display label and its confidence (FR-ST-02). The server filters/flags these.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadItem {
    pub label: String,
    pub confidence: f64,
}

impl ReadItem {
    pub fn new(label: impl Into<String>, confidence: f64) -> Self {
        Self { label: label.into(), confidence }
    }
}

/// The data source behind the Memory API's read tools. Implemented by the daemon over its DB.
pub trait MemoryBackend: Send + Sync {
    /// Rows for a read/list tool. `get`/`search` variants that need an id/query return their
    /// matches; the daemon impl decides. Rows are unfiltered — the server applies the confidence
    /// gate.
    fn read_list(&self, tool: Tool) -> Vec<ReadItem>;
}

/// A backend that returns nothing — the server is runnable/testable before the DB is wired.
pub struct StubBackend;

impl MemoryBackend for StubBackend {
    fn read_list(&self, _tool: Tool) -> Vec<ReadItem> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_returns_empty() {
        assert!(StubBackend.read_list(Tool::StatePeopleList).is_empty());
    }
}
