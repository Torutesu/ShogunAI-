//! The MCP server face (§6.11, FR-API-01) — the third Memory API face, alongside CLI and REST. A
//! JSON-RPC 2.0 handler over the Model Context Protocol: `initialize`, `tools/list`, `tools/call`.
//!
//! Symmetry (invariant 6): `tools/call` dispatches to the **same** [`MemoryBackend`] and the
//! **same** approval flow as the REST/CLI faces, and reads render through the shared
//! [`crate::rest::render_reads`], so all three faces return identical data at identical levels.
//!
//! Transport: this is the pure protocol handler ([`McpServer::handle_line`]); the stdio loop
//! ([`crate::mcp_stdio`], feature `server`) is the thin I/O around it. Over stdio the client is a
//! local subprocess the user launched, so calls are process-trusted (no bearer token — that gate
//! is for the REST/HTTP face, FR-API-03). Levels still apply: an external send routes to the
//! shared approval queue and returns pending, never running without a UI confirm (FR-API-04).

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{json, Value};
use shogun_agents::approval::ApprovalQueue;

use crate::approval_store;
use crate::backend::{MemoryBackend, ReadParams};
use crate::memory_api::{tool_level, ApiLevel, Tool, ALL_TOOLS};
use crate::rest;
use crate::visual_recall_api::{is_structured_read, render_structured};

/// The MCP protocol version this server speaks.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// The MCP server: a backend, the shared approval queue, and a clock.
pub struct McpServer<B: MemoryBackend> {
    backend: B,
    approvals: Mutex<ApprovalQueue>,
    /// When set, L3 sends persist to `l3_approvals.json` so the desktop Approvals UI can confirm.
    approvals_path: Option<PathBuf>,
    clock: Box<dyn Fn() -> i64 + Send + Sync>,
}

impl<B: MemoryBackend> McpServer<B> {
    pub fn new(backend: B, clock: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        Self {
            backend,
            approvals: Mutex::new(ApprovalQueue::new()),
            approvals_path: None,
            clock: Box::new(clock),
        }
    }

    /// Persist L3 pending approvals to `path` (shared with the desktop app).
    pub fn with_approvals_path(mut self, path: PathBuf) -> Self {
        let loaded = approval_store::load_queue(&path);
        self.approvals = Mutex::new(loaded);
        self.approvals_path = Some(path);
        self
    }

    /// Handle one JSON-RPC line. Returns the response line, or `None` for a notification (no id).
    pub fn handle_line(&self, line: &str) -> Option<String> {
        let req: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return Some(error(Value::Null, -32700, "parse error")),
        };
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(Value::as_str).unwrap_or_default();

        match method {
            "initialize" => id.map(|id| result(id, self.initialize())),
            // notifications carry no id and get no response.
            "notifications/initialized" | "notifications/cancelled" => None,
            "tools/list" => id.map(|id| result(id, self.tools_list())),
            "tools/call" => id.map(|id| self.tools_call(id, req.get("params"))),
            "ping" => id.map(|id| result(id, json!({}))),
            _ => id.map(|id| error(id, -32601, "method not found")),
        }
    }

    fn initialize(&self) -> Value {
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "shogun-memory", "version": env!("CARGO_PKG_VERSION") },
        })
    }

    fn tools_list(&self) -> Value {
        let tools: Vec<Value> = ALL_TOOLS.iter().map(|t| tool_descriptor(*t)).collect();
        json!({ "tools": tools })
    }

    fn tools_call(&self, id: Value, params: Option<&Value>) -> String {
        let Some(params) = params else {
            return error(id, -32602, "missing params");
        };
        let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
        let Some(tool) = Tool::from_wire(name) else {
            return error(id, -32602, "unknown tool");
        };
        let args = params.get("arguments").cloned().unwrap_or(json!({}));

        let text = match tool_level(tool) {
            ApiLevel::Read => {
                let read_params = ReadParams {
                    id: args.get("id").and_then(Value::as_i64),
                    query: args.get("query").and_then(Value::as_str).map(str::to_string),
                    from_ms: args.get("from_ms").and_then(Value::as_i64),
                    to_ms: args.get("to_ms").and_then(Value::as_i64),
                };
                if is_structured_read(tool) {
                    self.backend
                        .read_structured(tool, &read_params)
                        .map(|json| render_structured(tool, &json))
                        .unwrap_or_else(|| r#"{"error":"unavailable"}"#.to_string())
                } else {
                    let include_low = args.get("include_low").and_then(Value::as_bool).unwrap_or(false);
                    let items = self.backend.read(tool, &read_params);
                    rest::render_reads(tool, &items, include_low)
                }
            }
            ApiLevel::Write(_) => {
                let body = if tool == Tool::MemoryAppendNote {
                    args.get("text").and_then(Value::as_str).unwrap_or_default().to_string()
                } else if matches!(
                    tool,
                    Tool::VisualRecallSetEnabled | Tool::VisualRecallDeleteFrame | Tool::ProfileSet
                ) {
                    args.to_string()
                } else {
                    args.to_string()
                };
                match self.backend.write(tool, &body) {
                    Ok(Some(row_id)) => format!(r#"{{"accepted":true,"id":{row_id}}}"#),
                    Ok(None) => r#"{"accepted":true}"#.to_string(),
                    Err(e) => return error(id, -32000, &e),
                }
            }
            ApiLevel::PerAction => {
                // The arguments ARE the action spec; route through act() + shared L3 store.
                let body = args.to_string();
                let now = (self.clock)();
                // local_search: actually run hybrid search via the same backend as memory.search.
                if let Ok(v) = serde_json::from_str::<Value>(&body) {
                    if v.get("kind").and_then(Value::as_str) == Some("local_search") {
                        let query = v.get("query").and_then(Value::as_str).unwrap_or_default();
                        let items = self.backend.read(
                            Tool::MemorySearch,
                            &ReadParams { query: Some(query.to_string()), ..ReadParams::default() },
                        );
                        let rendered = rest::render_reads(Tool::MemorySearch, &items, false);
                        format!(r#"{{"executed":"local","level":"L1","kind":"local_search","results":{rendered}}}"#)
                    } else {
                        self.execute_action(&body, now)
                    }
                } else {
                    self.execute_action(&body, now)
                }
            }
        };

        // MCP tool results are content blocks; we return the tool's JSON as a text block.
        result(id, json!({ "content": [{ "type": "text", "text": text }], "isError": false }))
    }

    /// Run `actions.execute` against the in-memory queue, then flush to the shared file when set.
    fn execute_action(&self, body: &str, now: i64) -> String {
        if let Some(path) = &self.approvals_path {
            match approval_store::with_queue(path, |queue| rest::act(Some(body), now, queue).1) {
                Ok(text) => {
                    // Keep the in-memory mutex mirror in sync for same-process polls/tests.
                    if let Ok(mut guard) = self.approvals.lock() {
                        *guard = approval_store::load_queue(path);
                    }
                    text
                }
                Err(e) => format!(r#"{{"error":"approval_store","message":{}}}"#, json!(e)),
            }
        } else {
            match self.approvals.lock() {
                Ok(mut queue) => rest::act(Some(body), now, &mut queue).1,
                Err(_) => r#"{"error":"internal"}"#.to_string(),
            }
        }
    }
}

/// Run the MCP server over a line-delimited transport (stdio in production). Reads one JSON-RPC
/// message per line, writes each response as a line. Blank lines are skipped; notifications produce
/// no output. Generic over the streams so it is testable without real stdio.
pub fn serve<B: MemoryBackend>(
    server: &McpServer<B>,
    input: impl std::io::BufRead,
    mut output: impl std::io::Write,
) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = server.handle_line(&line) {
            writeln!(output, "{response}")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// A JSON-RPC success response line.
fn result(id: Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

/// A JSON-RPC error response line.
fn error(id: Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

/// A minimal MCP tool descriptor (name + description + a permissive input schema).
fn tool_descriptor(tool: Tool) -> Value {
    let (desc, props): (&str, Value) = match tool {
        Tool::MemorySearch => (
            "Hybrid search over captured events and notes. Use for a specific question or keyword recall. Prefer profile.whoami for identity/prefs and memory.get_context for a compact work snapshot.",
            json!({ "query": { "type": "string" } }),
        ),
        Tool::MemoryGetContext => (
            "DB-derived work context snapshot (state facts + recent user notes). Live AX Notch screen cache is NOT available to standalone MCP — use this for orientation, then memory.search for specifics.",
            json!({}),
        ),
        Tool::StatePeopleGet
        | Tool::StateProjectsGet
        | Tool::StateCommitmentsGet
        | Tool::StateOpenLoopsGet => ("Get a state record by id", json!({ "id": { "type": "integer" } })),
        Tool::StatePeopleList
        | Tool::StateProjectsList
        | Tool::StateCommitmentsList
        | Tool::StateOpenLoopsList => ("List state records", json!({ "include_low": { "type": "boolean" } })),
        Tool::MemoryAppendNote => ("Append a user note (L1)", json!({ "text": { "type": "string" } })),
        Tool::StateProposeUpdate => ("Propose a state change (L2)", json!({})),
        Tool::ActionsExecute => (
            "Execute an action. Local kinds (local_search, open_app, reveal_file, show_notification, copy_to_clipboard) are L1/L2 — local_search runs hybrid search immediately. External sends (send_email, post_message, create_calendar_event, post_comment) are always L3: enqueued to the shared approval queue and return {pending, approval_id}; they never leave the device until the user confirms in SHOGUN Settings → Approvals (dedicated button; Enter alone never confirms). Prefer this over inventing a send path.",
            json!({
                "kind": {
                    "type": "string",
                    "description": "local_search | open_app | reveal_file | show_notification | copy_to_clipboard | send_email | post_message | create_calendar_event | post_comment"
                },
                "query": { "type": "string", "description": "local_search query" },
                "to": { "type": "string", "description": "send_email recipient" },
                "subject": { "type": "string" },
                "body": { "type": "string" },
                "channel": { "type": "string", "description": "post_message channel" },
                "title": { "type": "string", "description": "create_calendar_event title" },
                "target": { "type": "string", "description": "post_comment target" },
                "bundle_id": { "type": "string" },
                "path": { "type": "string" },
                "text": { "type": "string" }
            }),
        ),
        Tool::VisualRecallStatus => ("Visual recall status (enabled, frame stats, recent OCR)", json!({})),
        Tool::VisualRecallSetEnabled => ("Enable or disable visual recall (L1)", json!({ "enabled": { "type": "boolean" } })),
        Tool::VisualRecallSearchFrames => (
            "Search stored screen frames by OCR text",
            json!({
                "query": { "type": "string" },
                "from_ms": { "type": "integer" },
                "to_ms": { "type": "integer" }
            }),
        ),
        Tool::VisualRecallGetFrame => (
            "Get one stored frame's metadata and OCR text",
            json!({ "id": { "type": "integer" } }),
        ),
        Tool::VisualRecallRescanFrame => (
            "Re-OCR a stored JPEG via on-device Vision",
            json!({ "id": { "type": "integer" } }),
        ),
        Tool::VisualRecallDeleteFrame => ("Delete one stored frame and its OCR event (L1)", json!({ "id": { "type": "integer" } })),
        Tool::ProfileWhoami => (
            "Who the user is: Settings profile (display_name, role, prefs) plus a short work summary (counts/names of people, projects, commitments, open loops). Call first in a session before search/get_context when you need identity or standing prefs. Does not search history.",
            json!({}),
        ),
        Tool::ProfileSet => (
            "Update user-owned profile fields (display_name, role, prefs). L1 — persists to memory_api.json. Only set fields the user explicitly wants stored.",
            json!({
                "display_name": { "type": "string" },
                "role": { "type": "string" },
                "prefs": { "type": "string" }
            }),
        ),
    };
    json!({
        "name": tool.wire_name(),
        "description": desc,
        "inputSchema": { "type": "object", "properties": props },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{MemoryBackend, ReadItem, ReadParams, WriteResult};

    struct Fake;
    impl MemoryBackend for Fake {
        fn read(&self, tool: Tool, _p: &ReadParams) -> Vec<ReadItem> {
            if tool == Tool::StateCommitmentsList {
                vec![ReadItem::new("ship v1", 0.9), ReadItem::new("maybe refactor", 0.6), ReadItem::new("shaky", 0.3)]
            } else {
                Vec::new()
            }
        }
        fn write(&self, tool: Tool, body: &str) -> WriteResult {
            if tool == Tool::MemoryAppendNote {
                assert_eq!(body, "buy milk");
                Ok(Some(42))
            } else {
                Ok(None)
            }
        }
    }

    fn server() -> McpServer<Fake> {
        McpServer::new(Fake, || 1000)
    }

    fn call(server: &McpServer<Fake>, line: &str) -> Value {
        serde_json::from_str(&server.handle_line(line).unwrap()).unwrap()
    }

    #[test]
    fn initialize_reports_protocol_and_server_info() {
        let v = call(&server(), r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
        assert_eq!(v["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(v["result"]["serverInfo"]["name"], "shogun-memory");
    }

    #[test]
    fn notifications_get_no_response() {
        assert!(server().handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    }

    #[test]
    fn tools_list_has_all_twenty_one() {
        let v = call(&server(), r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
        let tools = v["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 21);
        assert!(tools.iter().any(|t| t["name"] == "memory.search"));
        assert!(tools.iter().any(|t| t["name"] == "visual_recall.status"));
        assert!(tools.iter().any(|t| t["name"] == "profile.whoami"));
        assert!(tools.iter().any(|t| t["name"] == "profile.set"));
        assert!(tools.iter().any(|t| t["name"] == "actions.execute"));
    }

    #[test]
    fn tools_call_read_applies_confidence_gate() {
        let v = call(
            &server(),
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"state.commitments.list","arguments":{}}}"#,
        );
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("ship v1"));
        assert!(text.contains("maybe refactor")); // medium included
        assert!(!text.contains("shaky")); // low excluded by default
    }

    #[test]
    fn tools_call_append_note_writes() {
        let v = call(
            &server(),
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"memory.append_note","arguments":{"text":"buy milk"}}}"#,
        );
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"id\":42"));
    }

    #[test]
    fn tools_call_send_is_pending_l3_via_shared_queue() {
        let v = call(
            &server(),
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"actions.execute","arguments":{"kind":"send_email","to":"a@b.com","subject":"s","body":"b"}}}"#,
        );
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"pending\":true"));
        assert!(text.contains("\"approval_id\":"));
    }

    #[test]
    fn tools_call_send_persists_to_shared_store_file() {
        let mut path = std::env::temp_dir();
        path.push(format!("mcp-l3-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let server = McpServer::new(Fake, || 1000).with_approvals_path(path.clone());
        let v = call(
            &server,
            r#"{"jsonrpc":"2.0","id":50,"method":"tools/call","params":{"name":"actions.execute","arguments":{"kind":"send_email","to":"a@b.com","subject":"s","body":"b"}}}"#,
        );
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"pending\":true"));
        let loaded = crate::approval_store::load_queue(&path);
        assert_eq!(loaded.pending_len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tools_call_local_search_runs_backend() {
        let v = call(
            &server(),
            r#"{"jsonrpc":"2.0","id":51,"method":"tools/call","params":{"name":"actions.execute","arguments":{"kind":"local_search","query":"x"}}}"#,
        );
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"executed\":\"local\""));
        assert!(text.contains("\"kind\":\"local_search\""));
    }

    #[test]
    fn unknown_tool_and_method_are_errors() {
        let v = call(
            &server(),
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
        );
        assert_eq!(v["error"]["code"], -32602);
        let v = call(&server(), r#"{"jsonrpc":"2.0","id":7,"method":"frobnicate"}"#);
        assert_eq!(v["error"]["code"], -32601);
    }

    #[test]
    fn malformed_json_is_parse_error() {
        let v: Value = serde_json::from_str(&server().handle_line("not json").unwrap()).unwrap();
        assert_eq!(v["error"]["code"], -32700);
    }

    #[test]
    fn serve_loop_reads_lines_and_writes_responses() {
        // two requests + one notification (no response) + a blank line
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "\n",
        );
        let mut out: Vec<u8> = Vec::new();
        serve(&server(), std::io::Cursor::new(input), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        // exactly two responses (the notification produced none)
        assert_eq!(lines.len(), 2, "got: {text}");
        assert!(lines[0].contains("\"protocolVersion\""));
        assert!(lines[1].contains("\"tools\""));
    }
}
