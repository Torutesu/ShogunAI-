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

/// Cross-process L3 approval file store (`l3_approvals.json`) shared by stdio MCP + desktop.
pub mod approval_store;
pub mod backend;
pub mod composio;
pub mod connection;
pub mod dispatch;
pub mod mcp;
pub mod memory_api;
/// Memory API enable gate + profile prefs (`memory_api.json`).
pub mod memory_api_settings;
pub mod rest;
pub mod scope;
pub mod service_gate;
pub mod slack;
pub mod sync;
/// Visual recall structured API helpers (Memory API symmetry).
pub mod visual_recall_api;
/// The REST listener (feature `server`): a localhost-bound axum adapter over [`rest`].
#[cfg(feature = "server")]
pub mod server;
