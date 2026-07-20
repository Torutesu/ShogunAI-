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

use crate::memory_api::{AuthResult, TokenRegistry, Tool};

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
    /// A read tool (200 after the backend read). `include_low` from the `?include_low` query.
    Read { tool: Tool },
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
        ["v1", "memory", "search"] => method_is(method, Method::Get, Routed::Read { tool: Tool::MemorySearch }),
        ["v1", "memory", "context"] => method_is(method, Method::Get, Routed::Read { tool: Tool::MemoryGetContext }),
        ["v1", "memory", "notes"] => {
            method_is(method, Method::Post, Routed::Write { tool: Tool::MemoryAppendNote, level: Level::L1 })
        }
        ["v1", "state", "proposals"] => {
            method_is(method, Method::Post, Routed::Write { tool: Tool::StateProposeUpdate, level: Level::L2 })
        }
        ["v1", "actions", "execute"] => method_is(method, Method::Post, Routed::Action),
        // state list: /v1/state/<noun>
        ["v1", "state", noun] => match state_tool(noun, false) {
            Some(tool) => method_is(method, Method::Get, Routed::Read { tool }),
            None => Err(RouteMiss::NotFound),
        },
        // state get: /v1/state/<noun>/<id>
        ["v1", "state", noun, id] if id.parse::<i64>().is_ok() => match state_tool(noun, true) {
            Some(tool) => method_is(method, Method::Get, Routed::Read { tool }),
            None => Err(RouteMiss::NotFound),
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
        RestRequest { method, path: path.into(), token: token.map(str::to_string) }
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
            Routed::Read { tool: Tool::MemorySearch }
        );
        assert_eq!(
            route(&req(Method::Get, "/v1/state/commitments", Some("t")), &reg()),
            Routed::Read { tool: Tool::StateCommitmentsList }
        );
        // trailing id selects the get variant
        assert_eq!(
            route(&req(Method::Get, "/v1/state/people/42", Some("t")), &reg()),
            Routed::Read { tool: Tool::StatePeopleGet }
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
            Routed::Read { tool: Tool::StateProjectsList }
        );
    }

    #[test]
    fn resolved_read_tool_is_actually_a_read() {
        // guard against a routing table that points a read path at a write tool
        if let Routed::Read { tool } = route(&req(Method::Get, "/v1/state/open_loops", Some("t")), &reg()) {
            assert_eq!(tool_level(tool), ApiLevel::Read);
        } else {
            panic!("expected a read");
        }
    }
}
