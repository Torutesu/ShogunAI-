//! SHOGUN core — pure, platform-independent behavioural logic.
//!
//! The macOS shell (`apps/desktop/src-tauri`) is a thin adapter: it sources OS events
//! (CGEventTap, NSWorkspace, AXUIElement, timers) and feeds them into these modules, then
//! applies the emitted effects. Keeping the logic here makes the notch behaviour (Q2 expand,
//! Q4 false-positive) and the capture walk policy unit-testable without a Mac.
//!
//! Module layout (docs/phase1-implementation-plan.md WP1.1):
//! - [`notch`] — the notch UI: screen geometry / hit regions, hover judgement, the state
//!   machine, and the integrated engine that wires them together.
//! - [`capture`] — the context-capture policy: the bounded accessibility-tree walk.
//! - [`metrics`] — the always-on SLO histograms (NFR-SLO-00): fixed-bucket latency/percent
//!   tracking with pass/fail against each SLO budget.
//! - [`llm`] — the two model-access lanes (Batch / Agent) with compile-time key separation
//!   (invariant 5) and secret redaction.
//! - [`bus`] — the internal event bus (§5.3, AR-06/07): a non-blocking broadcast with
//!   backpressure-by-drop and a drop metric.
//! - [`dreamcycle`] — the nightly-batch orchestration logic (§6.7): run-condition gate, resumable
//!   job plan, and Batch-API failure escalation. Effects stay behind seams; the rules are pure.
//!
//! This crate makes no process-boundary assumptions (AR-03): it is a library the Tauri
//! backend hosts today and a future daemon could host unchanged.
//!
//! CLAUDE.md forbids `unwrap()`/`expect()` outside tests; test modules are exempted below.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod bus;
pub mod capture;
/// The daemon's shared DB handle — one connection every writer/reader uses (feature `db`; the
/// first shogun-core → shogun-memory edge).
#[cfg(feature = "db")]
pub mod daemon;
/// The daemon's Memory API data backend — implements shogun-mcp's `MemoryBackend` over `Db`
/// (feature `db`).
#[cfg(feature = "db")]
pub mod db_backend;
/// DB-backed traceability sink — the daemon's `TraceabilitySink` → `traceability_log` adapter
/// (feature `db`).
#[cfg(feature = "db")]
pub mod db_sink;
pub mod dreamcycle;
pub mod inline;
pub mod llm;
/// The second-layer (Composio) Gmail-send executor + its HTTP client (allowlisted egress, FR-TR-03).
#[cfg(feature = "net")]
pub mod composio_send;
/// Concrete HTTPS clients for first-layer connectors (the allowlisted egress, FR-TR-03).
#[cfg(feature = "net")]
pub mod mcp_http;
pub mod metrics;
pub mod notch;
/// Post-approval L3 send execution + mandatory traceability (needs the approval types from
/// shogun-agents; available under `exec` — the desktop — and `daemon-server`).
#[cfg(feature = "exec")]
pub mod send_exec;
pub mod traceview;
