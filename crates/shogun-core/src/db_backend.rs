//! The daemon's Memory API data backend (§6.11). Feature `db`. Implements shogun-mcp's
//! [`MemoryBackend`] over the daemon's [`Db`], so the REST / CLI / MCP faces read real state.
//!
//! Dependency-inverted: the trait is owned by the API layer (shogun-mcp) and implemented here by
//! the daemon — the API layer never depends on the core. The confidence gate stays in the API
//! layer (FR-API-06); this backend only supplies rows.

use shogun_mcp::backend::{MemoryBackend, ReadItem, ReadParams, WriteResult};
use shogun_mcp::memory_api::Tool;

use crate::daemon::Db;

/// Max search hits returned by `memory.search` over the API.
const SEARCH_LIMIT: usize = 20;
/// Confidence assigned to a captured event in search results: events are ground truth, not inferred
/// state, so they always pass the confidence gate (they are not "possibly").
const EVENT_CONFIDENCE: f64 = 1.0;

/// A [`MemoryBackend`] backed by the daemon's DB handle.
pub struct DbBackend {
    db: Db,
}

impl DbBackend {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl MemoryBackend for DbBackend {
    fn read(&self, tool: Tool, params: &ReadParams) -> Vec<ReadItem> {
        // A single row `Option` → a 0/1-length result (for the `get` variants).
        fn one<T>(row: Option<T>, f: impl Fn(T) -> ReadItem) -> Vec<ReadItem> {
            row.into_iter().map(f).collect()
        }
        let id = params.id;

        match tool {
            Tool::MemorySearch => self
                .db
                .search(params.query.as_deref().unwrap_or(""), SEARCH_LIMIT)
                .into_iter()
                .map(|hit| ReadItem::new(hit.content, EVENT_CONFIDENCE))
                .collect(),
            // `get_context` isn't a persisted read (the cache is RAM-only, AR-10) — empty here.
            Tool::MemoryGetContext => Vec::new(),

            Tool::StatePeopleList => {
                self.db.people().into_iter().map(|p| ReadItem::new(p.display_name, p.confidence)).collect()
            }
            Tool::StatePeopleGet => {
                one(id.and_then(|i| self.db.person(i)), |p| ReadItem::new(p.display_name, p.confidence))
            }
            Tool::StateProjectsList => {
                self.db.projects().into_iter().map(|p| ReadItem::new(p.name, p.confidence)).collect()
            }
            Tool::StateProjectsGet => {
                one(id.and_then(|i| self.db.project(i)), |p| ReadItem::new(p.name, p.confidence))
            }
            // Commitments/open loops reuse the Fusion supply. `now` from the daemon clock so
            // `overdue` is consistent with the rest of the daemon.
            Tool::StateCommitmentsList => self
                .db
                .commitments_due(self.db.now_ms())
                .into_iter()
                .map(|c| ReadItem::new(c.description, c.confidence))
                .collect(),
            Tool::StateCommitmentsGet => {
                one(id.and_then(|i| self.db.commitment(i)), |c| ReadItem::new(c.description, c.confidence))
            }
            Tool::StateOpenLoopsList => {
                self.db.open_loops().into_iter().map(|o| ReadItem::new(o.description, o.confidence)).collect()
            }
            Tool::StateOpenLoopsGet => {
                one(id.and_then(|i| self.db.open_loop(i)), |o| ReadItem::new(o.description, o.confidence))
            }

            // Onboarding / first-run state (issue #6) is owned by the desktop layer's app-settings
            // (app_data/onboarding.json), not the daemon DB, so this DB-backed face has no row to
            // supply — an empty result is the honest answer here rather than a fabricated one. The
            // tool exists on the shared surface so the contract is symmetric (invariant 6); serving
            // its live value is deferred to a shared-store follow-up.
            Tool::DeviceOnboardingGet => Vec::new(),

            // Not a read tool (write / action) — never routed here.
            Tool::MemoryAppendNote | Tool::StateProposeUpdate | Tool::ActionsExecute => Vec::new(),
        }
    }

    fn write(&self, tool: Tool, body: &str) -> WriteResult {
        match tool {
            // Persist the note to the event log (L1, reversible).
            Tool::MemoryAppendNote => match self.db.append_note(body) {
                Some(id) => Ok(Some(id)),
                None => Err("append_note failed".to_string()),
            },
            // A proposed state change is accepted here and surfaces in the Notch for L2 confirm; a
            // proposals table is future work, so nothing is persisted yet.
            Tool::StateProposeUpdate => Ok(None),
            // Not a write tool.
            _ => Err("not a write tool".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shogun_memory::state::{CommitmentDirection, CommitmentStatus, NewCommitment, Provenance};
    use std::sync::Arc;

    fn ev<'a>(hash: &'a str) -> shogun_memory::event_log::NewEvent<'a> {
        shogun_memory::event_log::NewEvent {
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
        }
    }

    fn seed() -> Db {
        let db = Db::open_in_memory(Arc::new(|| 100)).unwrap();
        let (e, _) = db.capture(&ev("h1")).unwrap();
        db.insert_commitment(
            &NewCommitment {
                direction: CommitmentDirection::Mine,
                counterparty_id: None,
                description: "send the report",
                due_at: Some(50),
                status: CommitmentStatus::Open,
                project_id: None,
                confidence: 0.9,
                now: 1,
            },
            &[Provenance::new(e)],
        )
        .unwrap();
        db
    }

    fn params() -> ReadParams {
        ReadParams::default()
    }
    fn get(id: i64) -> ReadParams {
        ReadParams { id: Some(id), query: None }
    }

    #[test]
    fn backend_reads_commitments_from_the_db() {
        let backend = DbBackend::new(seed());
        let items = backend.read(Tool::StateCommitmentsList, &params());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "send the report");
        assert_eq!(items[0].confidence, 0.9);
    }

    #[test]
    fn backend_reads_people_list_and_get() {
        use shogun_memory::state::{NewPerson, Provenance};
        let db = Db::open_in_memory(Arc::new(|| 1)).unwrap();
        let (e, _) = db.capture(&ev("h1")).unwrap();
        let id = db
            .insert_person(&NewPerson { display_name: "Alice", confidence: 0.85, now: 1, ..Default::default() }, &[Provenance::new(e)])
            .unwrap();
        let backend = DbBackend::new(db);

        let list = backend.read(Tool::StatePeopleList, &params());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].label, "Alice");

        let got = backend.read(Tool::StatePeopleGet, &get(id));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].label, "Alice");

        // a missing id → empty
        assert!(backend.read(Tool::StatePeopleGet, &get(9999)).is_empty());
    }

    #[test]
    fn backend_search_returns_event_content_as_ground_truth() {
        let db = Db::open_in_memory(Arc::new(|| 1)).unwrap();
        db.capture(&shogun_memory::event_log::NewEvent {
            ts: 1,
            source: "capture",
            kind: "text",
            app_bundle_id: None,
            window_title: None,
            content: "the quarterly roadmap review",
            content_hash: "h1",
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        })
        .unwrap();
        let backend = DbBackend::new(db);
        let hits = backend.read(Tool::MemorySearch, &ReadParams { id: None, query: Some("roadmap".into()) });
        assert_eq!(hits.len(), 1);
        assert!(hits[0].label.contains("roadmap"));
        assert_eq!(hits[0].confidence, EVENT_CONFIDENCE); // events always pass the gate
        // empty query → no search
        assert!(backend.read(Tool::MemorySearch, &params()).is_empty());
    }

    #[test]
    fn write_and_action_tools_are_not_reads() {
        let backend = DbBackend::new(seed());
        assert!(backend.read(Tool::MemoryAppendNote, &params()).is_empty());
        assert!(backend.read(Tool::ActionsExecute, &params()).is_empty());
    }

    #[test]
    fn append_note_persists_to_the_event_log() {
        let db = Db::open_in_memory(Arc::new(|| 555)).unwrap();
        let backend = DbBackend::new(db.clone());
        let id = match backend.write(Tool::MemoryAppendNote, "remember to call Alice") {
            Ok(Some(id)) => id,
            other => panic!("expected a persisted note id, got {other:?}"),
        };
        assert!(id > 0);
        // it's a searchable user note in the event log
        let hits = backend.read(Tool::MemorySearch, &ReadParams { id: None, query: Some("Alice".into()) });
        assert_eq!(hits.len(), 1);
        assert!(hits[0].label.contains("call Alice"));
    }

    #[test]
    fn propose_is_accepted_without_persistence_and_non_writes_error() {
        let backend = DbBackend::new(seed());
        assert_eq!(backend.write(Tool::StateProposeUpdate, "{}"), Ok(None));
        assert!(backend.write(Tool::MemorySearch, "x").is_err());
    }

    #[test]
    fn full_stack_note_write_through_rest_respond_with() {
        use shogun_mcp::memory_api::TokenRegistry;
        use shogun_mcp::rest::{respond_with, Method, RestRequest};

        let backend = DbBackend::new(Db::open_in_memory(Arc::new(|| 1)).unwrap());
        let mut tokens = TokenRegistry::new();
        tokens.issue("t");
        let req = RestRequest {
            method: Method::Post,
            path: "/v1/memory/notes".into(),
            token: Some("t".into()),
            include_low: false,
            query: None,
            body: Some("ship v1 on friday".into()),
        };
        let (status, body) = respond_with(&req, &tokens, &backend);
        assert_eq!(status, 202);
        assert!(body.contains("memory.append_note"));
        assert!(body.contains("\"level\":\"L1\""));
        assert!(body.contains("\"id\":"), "persisted note id missing: {body}");
    }

    #[test]
    fn full_stack_db_backend_through_rest_respond_with() {
        use shogun_mcp::memory_api::TokenRegistry;
        use shogun_mcp::rest::{respond_with, Method, RestRequest};

        let backend = DbBackend::new(seed());
        let mut tokens = TokenRegistry::new();
        tokens.issue("t");
        let req = RestRequest {
            method: Method::Get,
            path: "/v1/state/commitments".into(),
            token: Some("t".into()),
            include_low: false,
            query: None,
            body: None,
        };
        let (status, body) = respond_with(&req, &tokens, &backend);
        assert_eq!(status, 200);
        // real DB data rendered through the API layer's confidence-gated JSON
        assert!(body.contains("send the report"), "body: {body}");
        assert!(body.contains("state.commitments.list"));
    }
}
