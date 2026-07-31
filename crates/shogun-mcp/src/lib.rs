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

pub mod backend;
pub mod composio;
pub mod connection;
pub mod dispatch;
pub mod mcp;
pub mod memory_api;
pub mod plan_source;
pub mod rest;
pub mod scope;
pub mod service_gate;
pub mod slack;
pub mod sync;
/// The REST listener (feature `server`): a localhost-bound axum adapter over [`rest`].
#[cfg(feature = "server")]
pub mod server;
