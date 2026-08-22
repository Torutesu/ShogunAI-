//! SHOGUN first-layer MCP integration (§6.9). v1 milestone: the per-service **permission scope
//! table** (§6.9.2), which is the source of truth for what each connected service may do and at
//! which permission level.
//!
//! The load-bearing acceptance rule (§6.9): *an operation not in the table is denied by the
//! permission table*, and every external send is L3 (invariant 4). Both are encoded in
//! [`scope`] and enforced by tests over the tables — no service adapter re-derives the policy.
//!
//! Platform-independent so the policy is exhaustively Linux-testable; the MCP client transport and
//! OAuth-to-Keychain live in the desktop adapter.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

/// Re-export of the plan entitlement vocabulary (issue #97) so downstream crates that reach the
/// Memory API through shogun-mcp alone (e.g. shogun-core's `db` feature) can name it without a
/// direct shogun-agents dependency.
pub use shogun_agents::entitlement;

pub mod approval_store;
pub mod backend;
pub mod composio;
pub mod connection;
pub mod desktop_heartbeat;
pub mod dispatch;
pub mod mcp;
pub mod memory_api;
/// Memory API opt-in, profile, and hashed bearer-token persistence.
pub mod memory_api_settings;
pub mod meeting_microphone_api;
pub mod plan_source;
pub mod rest;
pub mod scope;
/// The REST listener (feature `server`): a localhost-bound axum adapter over [`rest`].
#[cfg(feature = "server")]
pub mod server;
pub mod service_gate;
pub mod slack;
pub mod sync;
/// What the model may see: the LLM-facing tool catalog + the "Connected services" prompt block
/// (issue #81, `docs/mcp/01-architecture.md` §5).
pub mod tool_catalog;
/// The read-tool conversation loop (issue #81 step 2): resolve → gate → run → tool_result.
pub mod tool_loop;
/// Visual recall structured API helpers (Memory API symmetry).
pub mod visual_recall_api;
/// Closed, typed CRUD contract for private voice dictionary records.
pub mod voice_dictionary_api;
