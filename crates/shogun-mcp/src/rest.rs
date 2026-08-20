//! REST routing for the Memory API (§6.11, FR-API-01) — the third face, alongside CLI and MCP.
//!
//! This is the **pure routing layer**: HTTP method + path + token → the resolved [`Tool`] (or a
//! 401/404/405), with no socket and no backend. The actual server (a localhost-bound listener on
//! `127.0.0.1:7464`, NFR-SEC-03) parses a request into a [`RestRequest`], calls [`route`], and — for
//! a resolved tool — runs it through the shared [`crate::dispatch::MemoryApi`] (the same gate the
//! CLI and MCP faces use, so the three stay symmetric). Keeping the routing pure makes the whole
//! endpoint table unit-testable here.
//!
//! Auth (FR-API-03): every tool endpoint requires a valid token — reads included; only the
//! unauthenticated `/v1/status` discovery endpoint is exempt.

use shogun_agents::permission::Level;

use crate::memory_api::Tool;

/// The HTTP methods the Memory API uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// A parsed REST request (the server fills this from the socket; the token is the `Bearer` value).
#[derive(Debug, Clone)]
pub struct RestRequest {
    pub method: Method,
    pub path: String,
    pub token: Option<String>,
    /// `?include_low` — include <0.5 confidence read results (FR-API-06 opt-in).
    pub include_low: bool,
    /// `?q=` — the search query (for `memory.search`).
    pub query: Option<String>,
    /// The request body (POST writes, e.g. the note text).
    pub body: Option<String>,
    /// `?from_ms=` — visual-recall frame search window start.
    pub from_ms: Option<i64>,
    /// `?to_ms=` — visual-recall frame search window end.
    pub to_ms: Option<i64>,
}

/// The routing decision. The server turns this into an HTTP response, running the backend for the
/// tool variants (via [`crate::dispatch::MemoryApi`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Routed {
    /// 401 — no valid token (FR-API-03).
    Unauthorized,
    /// 403 — valid token, but the plan does not include the Memory API (issue #97: Pro/Trial
    /// only; Standard and expired-trial devices are refused on every tool endpoint, reads
    /// included). `/v1/status` and `/v1/metrics` stay open (they expose no memory data).
    PlanLocked,
    /// 404 — no such endpoint.
    NotFound,
    /// 405 — path exists but not for this method.
    MethodNotAllowed,
    /// A read tool (200 after the backend read). `id` is set for a `get`-by-id path.
    Read { tool: Tool, id: Option<i64> },
    /// A write tool (202 accepted): append_note = L1, propose_update = L2.
    Write { tool: Tool, level: Level },
    /// `actions.execute` (200 local / 202 pending for an L3 send — decided by the backend body).
    Action,
    /// Body-free L3 outcome by approval id.
    ApprovalStatus { id: u64 },
    /// The unauthenticated status/discovery endpoint (200).
    Status,
    /// In-product SLO metrics (200) — `shogun metrics` / Advanced UI (NFR-SLO-00). Open like
    /// `/v1/status`: it exposes only aggregate latency-vs-budget health, never capture content, and
    /// the listener is localhost-bound (NFR-SEC-03). The server fills the body from its injected
    /// metrics source.
    Metrics,
}

#[path = "rest_actions.rs"]
mod rest_actions;
#[path = "rest_auth.rs"]
mod rest_auth;
#[path = "rest_render.rs"]
mod rest_render;
#[path = "rest_routing.rs"]
mod rest_routing;

pub use rest_actions::{act, ApprovalSurface};
pub use rest_auth::{bearer, route};
pub use rest_render::{
    body_for, escape, poll_approval, render_reads, respond, respond_with, status_code,
};
pub(super) use rest_render::{json_escape, level_label};

#[cfg(test)]
#[path = "rest_tests.rs"]
mod tests;
