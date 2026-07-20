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

use crate::backend::{MemoryBackend, ReadParams};
use crate::memory_api::{read_inclusion, AuthResult, ReadInclusion, TokenRegistry, Tool};

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
}

/// The routing decision. The server turns this into an HTTP response, running the backend for the
/// tool variants (via [`crate::dispatch::MemoryApi`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Routed {
    /// 401 — no valid token (FR-API-03).
    Unauthorized,
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
    /// The unauthenticated status/discovery endpoint (200).
    Status,
}

/// Extract the `Bearer` token from an `Authorization` header value, if present and well-formed.
pub fn bearer(authorization: Option<&str>) -> Option<String> {
    authorization?.strip_prefix("Bearer ").map(|t| t.trim().to_string())
}

/// Resolve `(method, path)` to a tool endpoint. A trailing numeric segment (`/state/people/42`)
/// selects the `get` variant; its bare form selects `list`.
fn resolve(method: Method, path: &str) -> Result<Routed, RouteMiss> {
    // trim a trailing slash, then split.
    let path = path.strip_suffix('/').unwrap_or(path);
    let segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();

    // A state noun endpoint: /v1/state/<noun>[/<id>]
    let state_tool = |noun: &str, has_id: bool| -> Option<Tool> {
        Some(match (noun, has_id) {
            ("people", false) => Tool::StatePeopleList,
            ("people", true) => Tool::StatePeopleGet,
            ("projects", false) => Tool::StateProjectsList,
            ("projects", true) => Tool::StateProjectsGet,
            ("commitments", false) => Tool::StateCommitmentsList,
            ("commitments", true) => Tool::StateCommitmentsGet,
            ("open_loops", false) => Tool::StateOpenLoopsList,
            ("open_loops", true) => Tool::StateOpenLoopsGet,
            _ => return None,
        })
    };

    match segs.as_slice() {
        ["v1", "status"] => method_is(method, Method::Get, Routed::Status),
        ["v1", "memory", "search"] => {
            method_is(method, Method::Get, Routed::Read { tool: Tool::MemorySearch, id: None })
        }
        ["v1", "memory", "context"] => {
            method_is(method, Method::Get, Routed::Read { tool: Tool::MemoryGetContext, id: None })
        }
        ["v1", "memory", "notes"] => {
            method_is(method, Method::Post, Routed::Write { tool: Tool::MemoryAppendNote, level: Level::L1 })
        }
        ["v1", "state", "proposals"] => {
            method_is(method, Method::Post, Routed::Write { tool: Tool::StateProposeUpdate, level: Level::L2 })
        }
        ["v1", "actions", "execute"] => method_is(method, Method::Post, Routed::Action),
        // state list: /v1/state/<noun>
        ["v1", "state", noun] => match state_tool(noun, false) {
            Some(tool) => method_is(method, Method::Get, Routed::Read { tool, id: None }),
            None => Err(RouteMiss::NotFound),
        },
        // state get: /v1/state/<noun>/<id>
        ["v1", "state", noun, id] => match (state_tool(noun, true), id.parse::<i64>()) {
            (Some(tool), Ok(parsed)) => {
                method_is(method, Method::Get, Routed::Read { tool, id: Some(parsed) })
            }
            _ => Err(RouteMiss::NotFound),
        },
        _ => Err(RouteMiss::NotFound),
    }
}

enum RouteMiss {
    NotFound,
    MethodNotAllowed,
}

fn method_is(actual: Method, expected: Method, ok: Routed) -> Result<Routed, RouteMiss> {
    if actual == expected {
        Ok(ok)
    } else {
        Err(RouteMiss::MethodNotAllowed)
    }
}

/// Route a request: resolve the endpoint, then apply auth. `/v1/status` is exempt; every tool
/// endpoint requires a valid token (FR-API-03).
pub fn route(req: &RestRequest, tokens: &TokenRegistry) -> Routed {
    match resolve(req.method, &req.path) {
        Err(RouteMiss::NotFound) => Routed::NotFound,
        Err(RouteMiss::MethodNotAllowed) => Routed::MethodNotAllowed,
        Ok(Routed::Status) => Routed::Status, // unauthenticated discovery
        Ok(resolved) => match tokens.authenticate(req.token.as_deref()) {
            AuthResult::Granted => resolved,
            _ => Routed::Unauthorized,
        },
    }
}

/// The HTTP status a routing decision maps to (the server sets the body).
pub fn status_code(routed: &Routed) -> u16 {
    match routed {
        Routed::Unauthorized => 401,
        Routed::NotFound => 404,
        Routed::MethodNotAllowed => 405,
        Routed::Read { .. } | Routed::Status => 200,
        // A write is accepted (L2 still confirms in the Notch); an action may be pending.
        Routed::Write { .. } | Routed::Action => 202,
    }
}

/// The stable wire name of a tool (matches the CLI's names).
fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::MemorySearch => "memory.search",
        Tool::MemoryGetContext => "memory.get_context",
        Tool::StatePeopleList => "state.people.list",
        Tool::StatePeopleGet => "state.people.get",
        Tool::StateProjectsList => "state.projects.list",
        Tool::StateProjectsGet => "state.projects.get",
        Tool::StateCommitmentsList => "state.commitments.list",
        Tool::StateCommitmentsGet => "state.commitments.get",
        Tool::StateOpenLoopsList => "state.open_loops.list",
        Tool::StateOpenLoopsGet => "state.open_loops.get",
        Tool::MemoryAppendNote => "memory.append_note",
        Tool::StateProposeUpdate => "state.propose_update",
        Tool::ActionsExecute => "actions.execute",
    }
}

fn level_label(level: Level) -> &'static str {
    match level {
        Level::L1 => "L1",
        Level::L2 => "L2",
        Level::L3 => "L3",
    }
}

/// The JSON body for a routing decision. Tool responses stub the data (`results: []`) until the
/// server's backend is wired; the auth/routing envelope is real. Hand-built JSON (no serde dep).
pub fn body_for(routed: &Routed) -> String {
    match routed {
        Routed::Unauthorized => r#"{"error":"unauthorized"}"#.to_string(),
        Routed::NotFound => r#"{"error":"not_found"}"#.to_string(),
        Routed::MethodNotAllowed => r#"{"error":"method_not_allowed"}"#.to_string(),
        Routed::Status => r#"{"status":"ok","service":"shogun-memory-api"}"#.to_string(),
        Routed::Read { tool, .. } => format!(r#"{{"tool":"{}","results":[]}}"#, tool_name(*tool)),
        Routed::Write { tool, level } => {
            format!(r#"{{"tool":"{}","level":"{}","accepted":true}}"#, tool_name(*tool), level_label(*level))
        }
        Routed::Action => r#"{"tool":"actions.execute","status":"routed"}"#.to_string(),
    }
}

/// Route + render with a stub body (no backend). The server uses [`respond_with`]; this stays for
/// callers/tests that don't need real data.
pub fn respond(req: &RestRequest, tokens: &TokenRegistry) -> (u16, String) {
    let routed = route(req, tokens);
    (status_code(&routed), body_for(&routed))
}

/// Minimal JSON string escaping for a label (quotes, backslash, and control chars).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Route + render **with real data** from `backend`. For a read tool, the backend supplies rows and
/// this applies the confidence gate (FR-API-06: Low excluded unless `?include_low`, Medium flagged
/// `possibly`); other decisions render as [`body_for`]. This is what the server calls.
pub fn respond_with<B: MemoryBackend + ?Sized>(
    req: &RestRequest,
    tokens: &TokenRegistry,
    backend: &B,
) -> (u16, String) {
    match route(req, tokens) {
        Routed::Read { tool, id } => {
            let params = ReadParams { id, query: req.query.clone() };
            let rendered: Vec<String> = backend
                .read(tool, &params)
                .into_iter()
                .filter_map(|item| match read_inclusion(item.confidence, req.include_low) {
                    ReadInclusion::Included { possibly } => Some(format!(
                        r#"{{"text":"{}","confidence":{},"possibly":{}}}"#,
                        json_escape(&item.label),
                        item.confidence,
                        possibly
                    )),
                    ReadInclusion::Excluded => None,
                })
                .collect();
            (200, format!(r#"{{"tool":"{}","results":[{}]}}"#, tool_name(tool), rendered.join(",")))
        }
        other => (status_code(&other), body_for(&other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_api::{tool_level, ApiLevel};

    fn reg() -> TokenRegistry {
        let mut r = TokenRegistry::new();
        r.issue("t");
        r
    }
    fn req(method: Method, path: &str, token: Option<&str>) -> RestRequest {
        RestRequest { method, path: path.into(), token: token.map(str::to_string), include_low: false, query: None }
    }

    #[test]
    fn bearer_parsing() {
        assert_eq!(bearer(Some("Bearer abc123")), Some("abc123".into()));
        assert_eq!(bearer(Some("Basic abc")), None);
        assert_eq!(bearer(None), None);
    }

    #[test]
    fn unknown_path_is_404_even_with_a_token() {
        assert_eq!(route(&req(Method::Get, "/v1/nope", Some("t")), &reg()), Routed::NotFound);
    }

    #[test]
    fn wrong_method_is_405() {
        // search is GET-only
        assert_eq!(route(&req(Method::Post, "/v1/memory/search", Some("t")), &reg()), Routed::MethodNotAllowed);
    }

    #[test]
    fn tool_endpoints_require_a_token_including_reads() {
        assert_eq!(route(&req(Method::Get, "/v1/memory/search", None), &reg()), Routed::Unauthorized);
        assert_eq!(route(&req(Method::Get, "/v1/state/people", Some("wrong")), &reg()), Routed::Unauthorized);
    }

    #[test]
    fn status_is_unauthenticated() {
        assert_eq!(route(&req(Method::Get, "/v1/status", None), &reg()), Routed::Status);
        assert_eq!(status_code(&Routed::Status), 200);
    }

    #[test]
    fn read_endpoints_resolve_to_read_tools() {
        assert_eq!(
            route(&req(Method::Get, "/v1/memory/search", Some("t")), &reg()),
            Routed::Read { tool: Tool::MemorySearch, id: None }
        );
        assert_eq!(
            route(&req(Method::Get, "/v1/state/commitments", Some("t")), &reg()),
            Routed::Read { tool: Tool::StateCommitmentsList, id: None }
        );
        // trailing id selects the get variant
        assert_eq!(
            route(&req(Method::Get, "/v1/state/people/42", Some("t")), &reg()),
            Routed::Read { tool: Tool::StatePeopleGet, id: Some(42) }
        );
    }

    #[test]
    fn write_endpoints_carry_their_levels_and_202() {
        let note = route(&req(Method::Post, "/v1/memory/notes", Some("t")), &reg());
        assert_eq!(note, Routed::Write { tool: Tool::MemoryAppendNote, level: Level::L1 });
        assert_eq!(status_code(&note), 202);

        let propose = route(&req(Method::Post, "/v1/state/proposals", Some("t")), &reg());
        assert_eq!(propose, Routed::Write { tool: Tool::StateProposeUpdate, level: Level::L2 });
    }

    #[test]
    fn actions_execute_routes_to_action() {
        assert_eq!(route(&req(Method::Post, "/v1/actions/execute", Some("t")), &reg()), Routed::Action);
        assert_eq!(status_code(&Routed::Action), 202);
    }

    #[test]
    fn trailing_slash_is_tolerated() {
        assert_eq!(
            route(&req(Method::Get, "/v1/state/projects/", Some("t")), &reg()),
            Routed::Read { tool: Tool::StateProjectsList, id: None }
        );
    }

    #[test]
    fn respond_renders_status_and_json_body() {
        let tokens = reg();
        // unauthenticated status
        let (s, b) = respond(&req(Method::Get, "/v1/status", None), &tokens);
        assert_eq!(s, 200);
        assert!(b.contains("shogun-memory-api"));
        // authed read → 200 with tool + empty results
        let (s, b) = respond(&req(Method::Get, "/v1/memory/search", Some("t")), &tokens);
        assert_eq!(s, 200);
        assert!(b.contains("\"tool\":\"memory.search\""));
        assert!(b.contains("\"results\":[]"));
        // missing token → 401
        let (s, b) = respond(&req(Method::Get, "/v1/memory/search", None), &tokens);
        assert_eq!(s, 401);
        assert!(b.contains("unauthorized"));
        // write → 202 with level
        let (s, b) = respond(&req(Method::Post, "/v1/memory/notes", Some("t")), &tokens);
        assert_eq!(s, 202);
        assert!(b.contains("\"level\":\"L1\""));
    }

    #[test]
    fn respond_with_backend_returns_data_confidence_filtered() {
        use crate::backend::{MemoryBackend, ReadItem};

        struct Fake;
        impl MemoryBackend for Fake {
            fn read(&self, _tool: Tool, _params: &crate::backend::ReadParams) -> Vec<ReadItem> {
                vec![
                    ReadItem::new("high", 0.9),   // included, not possibly
                    ReadItem::new("medium", 0.6), // included, possibly
                    ReadItem::new("low", 0.3),    // excluded by default
                ]
            }
        }
        let tokens = reg();

        // default: low excluded, medium flagged possibly
        let (s, b) = respond_with(&req(Method::Get, "/v1/state/people", Some("t")), &tokens, &Fake);
        assert_eq!(s, 200);
        assert!(b.contains("\"text\":\"high\""));
        assert!(b.contains(r#""text":"medium","confidence":0.6,"possibly":true"#));
        assert!(!b.contains("\"low\""), "low confidence excluded by default");

        // include_low pulls the low one in
        let with_low = RestRequest { include_low: true, ..req(Method::Get, "/v1/state/people", Some("t")) };
        let (_, b2) = respond_with(&with_low, &tokens, &Fake);
        assert!(b2.contains("\"text\":\"low\""));
    }

    #[test]
    fn respond_with_still_enforces_auth_and_404() {
        use crate::backend::StubBackend;
        let tokens = reg();
        let (s, _) = respond_with(&req(Method::Get, "/v1/state/people", None), &tokens, &StubBackend);
        assert_eq!(s, 401, "no token still 401 even with a backend");
        let (s, _) = respond_with(&req(Method::Get, "/v1/nope", Some("t")), &tokens, &StubBackend);
        assert_eq!(s, 404);
    }

    #[test]
    fn json_escape_handles_quotes_and_controls() {
        assert_eq!(json_escape(r#"a"b\c"#), "a\\\"b\\\\c");
        assert_eq!(json_escape("line\nbreak"), "line\\nbreak");
    }

    #[test]
    fn resolved_read_tool_is_actually_a_read() {
        // guard against a routing table that points a read path at a write tool
        if let Routed::Read { tool, .. } = route(&req(Method::Get, "/v1/state/open_loops", Some("t")), &reg()) {
            assert_eq!(tool_level(tool), ApiLevel::Read);
        } else {
            panic!("expected a read");
        }
    }
}
