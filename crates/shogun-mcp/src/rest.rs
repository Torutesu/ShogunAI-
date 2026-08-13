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

use shogun_agents::approval::{ApprovalQueue, Origin, Preview, Route};
use shogun_agents::approval::{ApprovalId, ApprovalStatus};
use shogun_agents::permission::{Action, Level, LocalAction, SendAction};

use crate::backend::{MemoryBackend, ReadParams};
use crate::memory_api::{read_inclusion, AuthResult, ReadInclusion, TokenRegistry, Tool};
use crate::visual_recall_api::{is_structured_read, render_structured};

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
    ApprovalPoll { id: u64 },
    /// The unauthenticated status/discovery endpoint (200).
    Status,
    /// In-product SLO metrics (200) — `shogun metrics` / Advanced UI (NFR-SLO-00). Open like
    /// `/v1/status`: it exposes only aggregate latency-vs-budget health, never capture content, and
    /// the listener is localhost-bound (NFR-SEC-03). The server fills the body from its injected
    /// metrics source.
    Metrics,
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
        ["v1", "metrics"] => method_is(method, Method::Get, Routed::Metrics),
        ["v1", "memory", "search"] => {
            method_is(method, Method::Get, Routed::Read { tool: Tool::MemorySearch, id: None })
        }
        ["v1", "memory", "context"] => {
            method_is(method, Method::Get, Routed::Read { tool: Tool::MemoryGetContext, id: None })
        }
        ["v1", "visual_recall", "status"] => {
            method_is(method, Method::Get, Routed::Read { tool: Tool::VisualRecallStatus, id: None })
        }
        ["v1", "visual_recall", "enabled"] => {
            method_is(method, Method::Post, Routed::Write { tool: Tool::VisualRecallSetEnabled, level: Level::L1 })
        }
        ["v1", "visual_recall", "frames", "search"] => {
            method_is(method, Method::Get, Routed::Read { tool: Tool::VisualRecallSearchFrames, id: None })
        }
        ["v1", "visual_recall", "frames", id, "rescan"] => match id.parse::<i64>() {
            Ok(parsed) => {
                method_is(method, Method::Post, Routed::Read { tool: Tool::VisualRecallRescanFrame, id: Some(parsed) })
            }
            Err(_) => Err(RouteMiss::NotFound),
        },
        ["v1", "visual_recall", "frames", "delete"] => {
            method_is(method, Method::Post, Routed::Write { tool: Tool::VisualRecallDeleteFrame, level: Level::L1 })
        },
        ["v1", "visual_recall", "frames", id] => match id.parse::<i64>() {
            Ok(parsed) => method_is(method, Method::Get, Routed::Read { tool: Tool::VisualRecallGetFrame, id: Some(parsed) }),
            Err(_) => Err(RouteMiss::NotFound),
        },
        ["v1", "memory", "notes"] => {
            method_is(method, Method::Post, Routed::Write { tool: Tool::MemoryAppendNote, level: Level::L1 })
        }
        ["v1", "profile", "whoami"] => {
            method_is(method, Method::Get, Routed::Read { tool: Tool::ProfileWhoami, id: None })
        }
        ["v1", "profile"] => {
            method_is(method, Method::Post, Routed::Write { tool: Tool::ProfileSet, level: Level::L1 })
        }
        ["v1", "state", "proposals"] => {
            method_is(method, Method::Post, Routed::Write { tool: Tool::StateProposeUpdate, level: Level::L2 })
        }
        ["v1", "actions", "execute"] => method_is(method, Method::Post, Routed::Action),
        ["v1", "actions", "poll", id] => match id.parse::<u64>() {
            Ok(id) => method_is(method, Method::Get, Routed::ApprovalPoll { id }),
            Err(_) => Err(RouteMiss::NotFound),
        },
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

/// Route a request: resolve the endpoint, then apply auth. `/v1/status` and `/v1/metrics` are the
/// two unauthenticated endpoints (localhost-bound health/discovery, no capture content); every tool
/// endpoint requires a valid token (FR-API-03).
pub fn route(req: &RestRequest, tokens: &TokenRegistry) -> Routed {
    match resolve(req.method, &req.path) {
        Err(RouteMiss::NotFound) => Routed::NotFound,
        Err(RouteMiss::MethodNotAllowed) => Routed::MethodNotAllowed,
        Ok(Routed::Status) => Routed::Status,   // unauthenticated discovery
        Ok(Routed::Metrics) => Routed::Metrics, // unauthenticated health (NFR-SLO-00)
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
        Routed::Read { .. } | Routed::Status | Routed::Metrics | Routed::ApprovalPoll { .. } => 200,
        // A write is accepted (L2 still confirms in the Notch); an action may be pending.
        Routed::Write { .. } | Routed::Action => 202,
    }
}

/// The stable wire name of a tool (delegates to the shared name).
fn tool_name(tool: Tool) -> &'static str {
    tool.wire_name()
}

/// Render backend read items to the API's confidence-gated JSON result (FR-API-06). Shared by the
/// REST and MCP faces so their read output is identical. Low-confidence items are dropped unless
/// `include_low`; medium ones are flagged `possibly`.
pub fn render_reads(tool: Tool, items: &[crate::backend::ReadItem], include_low: bool) -> String {
    let rendered: Vec<String> = items
        .iter()
        .filter_map(|item| match read_inclusion(item.confidence, include_low) {
            ReadInclusion::Included { possibly } => Some(format!(
                r#"{{"text":"{}","confidence":{},"possibly":{}}}"#,
                escape(&item.label),
                item.confidence,
                possibly
            )),
            ReadInclusion::Excluded => None,
        })
        .collect();
    format!(r#"{{"tool":"{}","results":[{}]}}"#, tool_name(tool), rendered.join(","))
}

/// Public JSON string escape (quotes, backslash, control chars) — used by the render helpers.
pub fn escape(s: &str) -> String {
    json_escape(s)
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
        // The server overrides this with live metrics; the placeholder keeps the layer pure.
        Routed::Metrics => r#"{"metrics":[]}"#.to_string(),
        Routed::Read { tool, .. } => format!(r#"{{"tool":"{}","results":[]}}"#, tool_name(*tool)),
        Routed::Write { tool, level } => {
            format!(r#"{{"tool":"{}","level":"{}","accepted":true}}"#, tool_name(*tool), level_label(*level))
        }
        Routed::Action => r#"{"tool":"actions.execute","status":"routed"}"#.to_string(),
        Routed::ApprovalPoll { id } => format!(r#"{{"approval_id":{},"status":"unknown"}}"#, id),
    }
}

/// Route + render with a stub body (no backend). The server uses [`respond_with`]; this stays for
/// callers/tests that don't need real data.
pub fn respond(req: &RestRequest, tokens: &TokenRegistry) -> (u16, String) {
    let routed = route(req, tokens);
    (status_code(&routed), body_for(&routed))
}

/// A parsed `actions.execute` request: either an on-device action or an external send (with the
/// L3 preview already built).
enum ActionSpec {
    Local(LocalAction),
    Send(SendAction, Preview),
}

/// Parse the `actions.execute` JSON body into an action. Only string-parameterised actions are
/// expressible over the API (`SaveDraft`/`UpdateState` carry `'static` targets and are launched
/// from the UI, not the wire). Unknown / malformed bodies return `None` (→ 400).
fn parse_action(body: &str) -> Option<ActionSpec> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let kind = v.get("kind")?.as_str()?;
    let field = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);

    // For a send, the L3 preview shows the full content (FR-AG-03). Gmail send routes via Composio.
    let send = |action: SendAction, full: String, route: Route| {
        let preview = Preview::for_send(&action, full, route);
        ActionSpec::Send(action, preview)
    };

    Some(match kind {
        "local_search" => ActionSpec::Local(LocalAction::LocalSearch { query: field("query")? }),
        "open_app" => ActionSpec::Local(LocalAction::OpenApp { bundle_id: field("bundle_id")? }),
        "reveal_file" => ActionSpec::Local(LocalAction::RevealFile { path: field("path")? }),
        "show_notification" => ActionSpec::Local(LocalAction::ShowNotification { text: field("text")? }),
        "copy_to_clipboard" => ActionSpec::Local(LocalAction::CopyToClipboard { text: field("text")? }),
        "send_email" => {
            let to = field("to")?;
            let full = format!("Subject: {}\n\n{}", field("subject").unwrap_or_default(), field("body").unwrap_or_default());
            send(SendAction::SendEmail { to }, full, Route::ViaComposio)
        }
        "post_message" => {
            let channel = field("channel")?;
            send(SendAction::PostMessage { channel }, field("body").unwrap_or_default(), Route::DirectMcp)
        }
        "create_calendar_event" => {
            let title = field("title")?;
            send(SendAction::CreateCalendarEvent { title: title.clone() }, title, Route::DirectMcp)
        }
        "post_comment" => {
            let target = field("target")?;
            send(SendAction::PostComment { target }, field("body").unwrap_or_default(), Route::DirectMcp)
        }
        _ => return None,
    })
}

/// Handle `actions.execute` (auth already enforced by [`route`]). A local action is authorized to
/// run (200); an external send is enqueued in the shared approval queue and returns pending +
/// approval id (202, FR-API-04) — it never runs here without a UI confirm.
pub fn act(body: Option<&str>, now_ms: i64, approvals: &mut ApprovalQueue) -> (u16, String) {
    let Some(body) = body else {
        return (400, r#"{"error":"missing_body"}"#.to_string());
    };
    match parse_action(body) {
        None => (400, r#"{"error":"bad_action_request"}"#.to_string()),
        Some(ActionSpec::Local(action)) => {
            let level = Action::Local(action).required_level();
            (200, format!(r#"{{"executed":"local","level":"{}"}}"#, level_label(level)))
        }
        Some(ActionSpec::Send(send, preview)) => {
            let now = u64::try_from(now_ms).unwrap_or(0);
            let id = match approvals.try_request(send, preview, Origin::AiApi, now) {
                Ok(id) => id,
                Err(error) => return (503, format!(r#"{{"error":"approval_store","message":"{}"}}"#, json_escape(error))),
            };
            (202, format!(r#"{{"pending":true,"approval_id":{},"level":"L3"}}"#, id.0))
        }
    }
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
fn structured_read_status(json: &str) -> u16 {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(v) => {
            let err = v.get("error").and_then(|e| e.as_str());
            match err {
                Some("not_found") | Some("missing_frame_id") => 404,
                Some(_) => 400,
                None => 200,
            }
        }
        Err(_) => 500,
    }
}

pub fn respond_with<B: MemoryBackend + ?Sized>(
    req: &RestRequest,
    tokens: &TokenRegistry,
    backend: &B,
) -> (u16, String) {
    match route(req, tokens) {
        Routed::Read { tool, id } => {
            let params = ReadParams {
                id,
                query: req.query.clone(),
                from_ms: req.from_ms,
                to_ms: req.to_ms,
            };
            if is_structured_read(tool) {
                let json = backend
                    .read_structured(tool, &params)
                    .unwrap_or_else(|| r#"{"error":"unavailable"}"#.to_string());
                let status = structured_read_status(&json);
                (status, render_structured(tool, &json))
            } else {
                let items = backend.read(tool, &params);
                (200, render_reads(tool, &items, req.include_low))
            }
        }
        Routed::Write { tool, level } => {
            match backend.write(tool, req.body.as_deref().unwrap_or("")) {
                Ok(Some(id)) => (
                    202,
                    format!(
                        r#"{{"tool":"{}","level":"{}","id":{},"accepted":true}}"#,
                        tool_name(tool),
                        level_label(level),
                        id
                    ),
                ),
                Ok(None) => (
                    202,
                    format!(r#"{{"tool":"{}","level":"{}","accepted":true}}"#, tool_name(tool), level_label(level)),
                ),
                Err(e) => (500, format!(r#"{{"error":"{}"}}"#, json_escape(&e))),
            }
        }
        other => (status_code(&other), body_for(&other)),
    }
}

pub fn poll_approval(id: u64, approvals: &mut ApprovalQueue, now_ms: i64) -> String {
    approvals.expire_due(u64::try_from(now_ms).unwrap_or(0));
    let status = approvals.status(ApprovalId(id));
    let status = match status {
        Some(ApprovalStatus::Pending) => "pending",
        Some(ApprovalStatus::Rejected) => "rejected",
        Some(ApprovalStatus::TimedOut) => "timed_out",
        Some(ApprovalStatus::Sent) => "sent",
        Some(ApprovalStatus::SendFailed) => "send_failed",
        Some(ApprovalStatus::DraftSaved) => "draft_saved",
        None => "unknown",
    };
    format!(r#"{{"approval_id":{},"status":"{}"}}"#, id, status)
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
        RestRequest {
            method,
            path: path.into(),
            token: token.map(str::to_string),
            include_low: false,
            query: None,
            body: None,
            from_ms: None,
            to_ms: None,
        }
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
    fn metrics_is_unauthenticated_and_get_only() {
        // health endpoint: open like status (NFR-SLO-00), no capture content, localhost-bound.
        assert_eq!(route(&req(Method::Get, "/v1/metrics", None), &reg()), Routed::Metrics);
        assert_eq!(status_code(&Routed::Metrics), 200);
        // still GET-only
        assert_eq!(route(&req(Method::Post, "/v1/metrics", Some("t")), &reg()), Routed::MethodNotAllowed);
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
    fn actions_poll_routes_by_approval_id() {
        assert_eq!(route(&req(Method::Get, "/v1/actions/poll/7", Some("t")), &reg()), Routed::ApprovalPoll { id: 7 });
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
    fn act_local_is_authorized_immediately() {
        let mut q = ApprovalQueue::new();
        let (s, b) = act(Some(r#"{"kind":"local_search","query":"budget"}"#), 0, &mut q);
        assert_eq!(s, 200);
        assert!(b.contains("\"executed\":\"local\""));
        assert!(b.contains("\"level\":\"L1\""));
        assert_eq!(q.pending_len(), 0, "a local action never enqueues an approval");
    }

    #[test]
    fn act_send_enqueues_pending_l3_approval() {
        let mut q = ApprovalQueue::new();
        let (s, b) = act(
            Some(r#"{"kind":"send_email","to":"a@b.com","subject":"Hi","body":"hello"}"#),
            1000,
            &mut q,
        );
        assert_eq!(s, 202);
        assert!(b.contains("\"pending\":true"));
        assert!(b.contains("\"approval_id\":"));
        assert!(b.contains("\"level\":\"L3\""));
        assert_eq!(q.pending_len(), 1, "the send awaits UI confirmation (FR-API-04)");
    }

    #[test]
    fn act_rejects_missing_and_malformed_bodies() {
        let mut q = ApprovalQueue::new();
        assert_eq!(act(None, 0, &mut q).0, 400);
        assert_eq!(act(Some("not json"), 0, &mut q).0, 400);
        assert_eq!(act(Some(r#"{"kind":"unknown_thing"}"#), 0, &mut q).0, 400);
        // a send kind missing a required field is also rejected
        assert_eq!(act(Some(r#"{"kind":"send_email"}"#), 0, &mut q).0, 400);
        assert_eq!(q.pending_len(), 0);
    }

    #[test]
    fn json_escape_handles_quotes_and_controls() {
        assert_eq!(json_escape(r#"a"b\c"#), "a\\\"b\\\\c");
        assert_eq!(json_escape("line\nbreak"), "line\\nbreak");
    }

    #[test]
    fn visual_recall_endpoints_resolve() {
        assert_eq!(
            route(&req(Method::Get, "/v1/visual_recall/status", Some("t")), &reg()),
            Routed::Read { tool: Tool::VisualRecallStatus, id: None }
        );
        assert_eq!(
            route(&req(Method::Post, "/v1/visual_recall/enabled", Some("t")), &reg()),
            Routed::Write { tool: Tool::VisualRecallSetEnabled, level: Level::L1 }
        );
        assert_eq!(
            route(&req(Method::Get, "/v1/visual_recall/frames/search", Some("t")), &reg()),
            Routed::Read { tool: Tool::VisualRecallSearchFrames, id: None }
        );
        assert_eq!(
            route(&req(Method::Get, "/v1/visual_recall/frames/12", Some("t")), &reg()),
            Routed::Read { tool: Tool::VisualRecallGetFrame, id: Some(12) }
        );
        assert_eq!(
            route(&req(Method::Post, "/v1/visual_recall/frames/12/rescan", Some("t")), &reg()),
            Routed::Read { tool: Tool::VisualRecallRescanFrame, id: Some(12) }
        );
        assert_eq!(
            route(&req(Method::Post, "/v1/visual_recall/frames/delete", Some("t")), &reg()),
            Routed::Write { tool: Tool::VisualRecallDeleteFrame, level: Level::L1 }
        );
        assert_eq!(
            route(&req(Method::Get, "/v1/profile/whoami", Some("t")), &reg()),
            Routed::Read { tool: Tool::ProfileWhoami, id: None }
        );
        assert_eq!(
            route(&req(Method::Post, "/v1/profile", Some("t")), &reg()),
            Routed::Write { tool: Tool::ProfileSet, level: Level::L1 }
        );
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
