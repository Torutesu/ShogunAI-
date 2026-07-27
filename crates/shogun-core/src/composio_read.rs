//! Composio-based Gmail read transport — routes Gmail read and draft calls through the Composio
//! tool-execution API. This is the single transport for all Gmail operations (reads + drafts).
//!
//! This is the second-layer read transport (§6.10). Per the allowlisted-egress rule (FR-TR-03),
//! the HTTP client stays in shogun-core; the trait seam ([`ComposioApi`]) is in
//! shogun-integrations so the pure logic compiles without reqwest.
//!
//! The Composio tool shapes used here (as of 2026-07; verify on first live call):
//! - `GMAIL_FETCH_EMAILS` → list of messages with Gmail-native fields.
//! - `GMAIL_FETCH_MESSAGE_BY_THREAD_ID` → messages for one thread.
//!
//! **Field extraction is isolated in [`extract_messages`]** — the single function to fix if live
//! Composio responses use different nesting than the documented `data.messages` path.
//!
//! No item content, tokens, or secrets are ever surfaced in error strings (invariant 7).

use serde_json::{json, Value};
use shogun_integrations::composio::{parse_execute_response, ComposioApi};
use shogun_integrations::rpc::McpRpc;
use shogun_mcp::scope::Service;

use crate::gmail_shape::{envelope, record_from_thread};

/// Gmail read transport over the Composio tool-execution API.
///
/// Implements [`McpRpc`] and accepts the same tool names that [`RemoteMcpTransport`] sends
/// (`"search_threads"` / `"get_thread"`), mapping them to the corresponding Composio slugs.
pub struct ComposioReadRpc<A: ComposioApi> {
    api: A,
    /// Composio `user_id` (the connected-account identifier, not a secret).
    user_id: String,
    /// Max messages fetched per `search_threads` call.
    page_size: u32,
}

impl<A: ComposioApi> ComposioReadRpc<A> {
    /// Construct with a default `page_size` of 15.
    pub fn new(api: A, user_id: impl Into<String>) -> Self {
        Self { api, user_id: user_id.into(), page_size: 15 }
    }

    /// Override the fetch page size (useful for tests and operator tuning).
    #[allow(dead_code)]
    pub fn with_page_size(mut self, n: u32) -> Self {
        self.page_size = n;
        self
    }

    /// Create a Gmail draft via `GMAIL_CREATE_EMAIL_DRAFT`.
    ///
    /// Incoming `arguments` are `{to, subject, body}` (the same field names used by
    /// `save_gmail_draft` / `draft_request_body`). These are mapped to the Composio field names
    /// `recipient_email`/`subject`/`body` — mirroring `gmail_send_arguments` in shogun-integrations.
    /// The response body is returned as-is; the caller (`execute_write_owned`) ignores it.
    fn create_draft(&self, arguments: &Value) -> Result<Value, String> {
        let to = arguments
            .get("to")
            .and_then(Value::as_str)
            .ok_or("create_draft: missing to")?;
        let subject = arguments.get("subject").and_then(Value::as_str).unwrap_or("");
        let body = arguments.get("body").and_then(Value::as_str).unwrap_or("");
        let args = serde_json::json!({
            "recipient_email": to,
            "subject": subject,
            "body": body,
        });
        let resp = self.api.execute("GMAIL_CREATE_EMAIL_DRAFT", &self.user_id, args)?;
        parse_execute_response(&resp)?;
        Ok(resp)
    }

    /// Fetch the recent thread list via `GMAIL_FETCH_EMAILS`.
    fn search_threads(&self) -> Result<Value, String> {
        let resp = self.api.execute(
            "GMAIL_FETCH_EMAILS",
            &self.user_id,
            json!({
                "user_id": self.user_id,
                "max_results": self.page_size,
                "include_payload": true,
            }),
        )?;
        parse_execute_response(&resp)?;
        self.shape_into_envelope(&resp)
    }

    /// Fetch one thread by id via `GMAIL_FETCH_MESSAGE_BY_THREAD_ID`.
    fn get_thread(&self, arguments: &Value) -> Result<Value, String> {
        let thread_id =
            arguments.get("id").and_then(Value::as_str).ok_or("get_thread: missing id")?;
        let resp = self.api.execute(
            "GMAIL_FETCH_MESSAGE_BY_THREAD_ID",
            &self.user_id,
            json!({
                "user_id": self.user_id,
                "thread_id": thread_id,
            }),
        )?;
        parse_execute_response(&resp)?;
        self.shape_into_envelope(&resp)
    }

    /// Extract messages from the Composio response, group them by threadId, run each group
    /// through [`record_from_thread`], and return a `structuredContent` envelope.
    fn shape_into_envelope(&self, resp: &Value) -> Result<Value, String> {
        let messages = extract_messages(resp);
        let records = group_and_shape(messages);
        Ok(envelope(records))
    }
}

impl<A: ComposioApi> McpRpc for ComposioReadRpc<A> {
    fn call_tool(&self, service: Service, tool: &str, arguments: Value) -> Result<Value, String> {
        if service != Service::Gmail {
            return Err(format!(
                "ComposioReadRpc only serves Gmail, got {}",
                service.source_str()
            ));
        }
        match tool {
            "search_threads" => self.search_threads(),
            "get_thread" => self.get_thread(&arguments),
            // create_draft is the write tool name used by toolmap::tool_for(Gmail, "draft_create_update").
            // The transport also handles it here so the single Composio transport serves both the
            // read-sync path and the draft-create write path (FR-C2-05 fallback).
            "create_draft" => self.create_draft(&arguments),
            other => Err(format!("ComposioReadRpc has no mapping for tool '{other}'")),
        }
    }
}

// ─── message extraction (isolate here for easy live-fix) ─────────────────────────────────────

/// Extract the flat message array from a raw Composio execute response.
///
/// **This is the single point to patch if Composio's live response differs from the documented
/// shape.** The strategy (in priority order):
/// 1. `data.messages` — the documented path.
/// 2. `data` itself if it is an array — some tools return the array directly.
/// 3. Any single array-valued field directly under `data` — tolerant fallback.
/// 4. An empty vec if nothing matches (callers return an empty envelope rather than an error).
fn extract_messages(resp: &Value) -> Vec<Value> {
    let data = match resp.get("data") {
        Some(d) => d,
        None => return Vec::new(),
    };

    // 1. data.messages
    if let Some(arr) = data.get("messages").and_then(Value::as_array) {
        return arr.clone();
    }

    // 2. data itself is an array
    if let Some(arr) = data.as_array() {
        return arr.clone();
    }

    // 3. single array field anywhere under data
    if let Some(obj) = data.as_object() {
        let mut arrays = obj.values().filter(|v| v.is_array());
        if let (Some(first), None) = (arrays.next(), arrays.next()) {
            if let Some(arr) = first.as_array() {
                return arr.clone();
            }
        }
    }

    Vec::new()
}

// ─── grouping + shaping ───────────────────────────────────────────────────────────────────────

/// Group a flat list of Composio messages by `threadId` (falling back to `messageId` / `id`) and
/// call [`record_from_thread`] on each group to produce the normalized record array.
///
/// This reuses gmail_shape's multi-message body concatenation: grouping the messages for the
/// same thread together means `thread_body` joins all of them the same way it does for a
/// `threads.get(format=full)` response.
fn group_and_shape(messages: Vec<Value>) -> Vec<Value> {
    // Preserve insertion order (first-seen wins for ordering).
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<Value>> =
        std::collections::HashMap::new();

    for msg in messages {
        // Composio docs say `threadId`; fall back to `messageId` then `id`.
        let key = msg
            .get("threadId")
            .or_else(|| msg.get("messageId"))
            .or_else(|| msg.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if key.is_empty() {
            continue;
        }
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(msg);
    }

    order
        .into_iter()
        .filter_map(|thread_id| {
            let msgs = groups.remove(&thread_id)?;
            // Build a synthetic thread object matching what record_from_thread expects:
            // { id: threadId, messages: [...] }
            let thread = json!({ "id": thread_id, "messages": msgs });
            Some(record_from_thread(&thread))
        })
        .collect()
}

// ─── tests ────────────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use shogun_integrations::result::parse_items;
    use shogun_mcp::scope::Service;

    // ── Fake ComposioApi ────────────────────────────────────────────────────────

    struct FakeComposio {
        response: Result<Value, String>,
    }

    impl FakeComposio {
        fn ok(v: Value) -> Self {
            Self { response: Ok(v) }
        }
        fn err(msg: &str) -> Self {
            Self { response: Err(msg.to_string()) }
        }
    }

    impl ComposioApi for FakeComposio {
        fn execute(
            &self,
            _tool: &str,
            _user_id: &str,
            _arguments: Value,
        ) -> Result<Value, String> {
            self.response.clone()
        }
    }

    // ── service / tool rejection ────────────────────────────────────────────────

    #[test]
    fn rejects_non_gmail_service() {
        let rpc = ComposioReadRpc::new(FakeComposio::ok(json!({})), "uid");
        let err = rpc
            .call_tool(Service::Slack, "search_threads", json!({}))
            .unwrap_err();
        assert!(err.contains("only serves Gmail"), "{err}");
    }

    #[test]
    fn rejects_unknown_tool() {
        let rpc = ComposioReadRpc::new(FakeComposio::ok(json!({})), "uid");
        let err = rpc.call_tool(Service::Gmail, "send_email", json!({})).unwrap_err();
        assert!(err.contains("no mapping"), "{err}");
    }

    // ── successful:false propagates as Err ─────────────────────────────────────

    #[test]
    fn successful_false_is_an_error() {
        let rpc = ComposioReadRpc::new(
            FakeComposio::ok(json!({ "successful": false })),
            "uid",
        );
        let err = rpc
            .call_tool(Service::Gmail, "search_threads", json!({}))
            .unwrap_err();
        // Error is content-free — just check it is an Err.
        assert!(!err.is_empty(), "expected a non-empty error message");
    }

    // ── end-to-end: GMAIL_FETCH_EMAILS shape → parse_items ─────────────────────

    #[test]
    fn fetch_emails_response_shapes_to_fetched_item_with_decoded_body() {
        // "Hello from Composio" base64url-encoded = "SGVsbG8gZnJvbSBDb21wb3Npbw"
        let b64 = {
            use crate::gmail_shape::base64_url_decode;
            // encode "Hello from Composio" ourselves using the inverse function from gmail_shape
            // (we test that the round-trip works).
            fn b64url(s: &[u8]) -> String {
                const ALPHA: &[u8; 64] =
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
                let mut out = String::new();
                for chunk in s.chunks(3) {
                    let b =
                        [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
                    let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
                    out.push(ALPHA[((n >> 18) & 63) as usize] as char);
                    out.push(ALPHA[((n >> 12) & 63) as usize] as char);
                    if chunk.len() > 1 {
                        out.push(ALPHA[((n >> 6) & 63) as usize] as char);
                    }
                    if chunk.len() > 2 {
                        out.push(ALPHA[(n & 63) as usize] as char);
                    }
                }
                out
            }
            let enc = b64url(b"Hello from Composio");
            // sanity-check decode round-trip
            assert_eq!(
                String::from_utf8(base64_url_decode(&enc)).unwrap(),
                "Hello from Composio"
            );
            enc
        };

        let composio_resp = json!({
            "data": {
                "messages": [{
                    "messageId": "m1",
                    "threadId": "t1",
                    "subject": "Test Subject",
                    "snippet": "short snippet",
                    "payload": {
                        "mimeType": "text/plain",
                        "headers": [{"name": "Subject", "value": "Test Subject"}],
                        "body": { "data": b64 }
                    },
                    "internalDate": "1699900000000"
                }]
            },
            "successful": true
        });

        let rpc = ComposioReadRpc::new(FakeComposio::ok(composio_resp), "uid");
        let envelope = rpc
            .call_tool(Service::Gmail, "search_threads", json!({}))
            .unwrap();

        let items = parse_items(&envelope).expect("parse_items should succeed");
        assert_eq!(items.len(), 1, "expected exactly one item");
        let item = &items[0];
        assert_eq!(item.external_id, "t1", "threadId should be the external_id");
        assert_eq!(item.title, "Test Subject");
        assert!(
            item.body.contains("Hello from Composio"),
            "decoded body should appear in item.body, got: {:?}",
            item.body
        );
        assert_eq!(item.ts_ms, 1699900000000i64);
    }

    // ── two messages with the same threadId → single grouped record ─────────────

    #[test]
    fn two_messages_same_thread_group_into_one_record_with_both_bodies() {
        fn b64url(s: &[u8]) -> String {
            const ALPHA: &[u8; 64] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut out = String::new();
            for chunk in s.chunks(3) {
                let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
                let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
                out.push(ALPHA[((n >> 18) & 63) as usize] as char);
                out.push(ALPHA[((n >> 12) & 63) as usize] as char);
                if chunk.len() > 1 {
                    out.push(ALPHA[((n >> 6) & 63) as usize] as char);
                }
                if chunk.len() > 2 {
                    out.push(ALPHA[(n & 63) as usize] as char);
                }
            }
            out
        }

        let composio_resp = json!({
            "data": {
                "messages": [
                    {
                        "messageId": "m1",
                        "threadId": "thread-abc",
                        "snippet": "first message",
                        "payload": {
                            "mimeType": "text/plain",
                            "headers": [{"name": "Subject", "value": "Thread Convo"}],
                            "body": { "data": b64url(b"First message body") }
                        },
                        "internalDate": "1699900000000"
                    },
                    {
                        "messageId": "m2",
                        "threadId": "thread-abc",
                        "snippet": "second message",
                        "payload": {
                            "mimeType": "text/plain",
                            "headers": [{"name": "Subject", "value": "Thread Convo"}],
                            "body": { "data": b64url(b"Second message body") }
                        },
                        "internalDate": "1699900100000"
                    }
                ]
            },
            "successful": true
        });

        let rpc = ComposioReadRpc::new(FakeComposio::ok(composio_resp), "uid");
        let envelope = rpc
            .call_tool(Service::Gmail, "search_threads", json!({}))
            .unwrap();

        let items = parse_items(&envelope).expect("parse_items should succeed");
        assert_eq!(items.len(), 1, "two messages in same thread should produce ONE item");
        let body = &items[0].body;
        assert!(body.contains("First message body"), "first message body missing: {body:?}");
        assert!(body.contains("Second message body"), "second message body missing: {body:?}");
    }

    // ── get_thread routes to GMAIL_FETCH_MESSAGE_BY_THREAD_ID ──────────────────

    #[test]
    fn get_thread_requires_id_argument() {
        let rpc = ComposioReadRpc::new(FakeComposio::ok(json!({ "successful": true, "data": {} })), "uid");
        let err = rpc
            .call_tool(Service::Gmail, "get_thread", json!({}))
            .unwrap_err();
        assert!(err.contains("missing id"), "{err}");
    }

    // ── tolerant extraction: single array field under data ─────────────────────

    #[test]
    fn extract_messages_tolerant_single_array_field() {
        // Composio might return { data: { emails: [...] } } instead of { data: { messages: [...] } }
        let resp = json!({
            "data": {
                "emails": [
                    { "messageId": "m1", "threadId": "t1", "snippet": "hi",
                      "payload": { "mimeType": "text/plain", "body": { "data": "" } },
                      "internalDate": "0" }
                ]
            },
            "successful": true
        });
        let msgs = extract_messages(&resp);
        assert_eq!(msgs.len(), 1);
    }

    // ── tolerant extraction: data itself is an array ────────────────────────────

    #[test]
    fn extract_messages_tolerant_data_is_array() {
        let resp = json!({
            "data": [
                { "messageId": "m1", "threadId": "t1", "snippet": "hi",
                  "payload": { "mimeType": "text/plain", "body": { "data": "" } },
                  "internalDate": "0" }
            ],
            "successful": true
        });
        let msgs = extract_messages(&resp);
        assert_eq!(msgs.len(), 1);
    }

    // ── create_draft: routes to GMAIL_CREATE_EMAIL_DRAFT with mapped field names ─

    /// A recording fake that captures the last tool call so we can assert on arg field names.
    struct RecordingComposio {
        last_tool: std::cell::RefCell<String>,
        last_args: std::cell::RefCell<Value>,
        response: Result<Value, String>,
    }
    impl RecordingComposio {
        fn ok(v: Value) -> Self {
            Self {
                last_tool: std::cell::RefCell::new(String::new()),
                last_args: std::cell::RefCell::new(json!({})),
                response: Ok(v),
            }
        }
        fn failing(v: Value) -> Self {
            Self {
                last_tool: std::cell::RefCell::new(String::new()),
                last_args: std::cell::RefCell::new(json!({})),
                response: Ok(v),
            }
        }
    }
    impl ComposioApi for RecordingComposio {
        fn execute(&self, tool: &str, _user_id: &str, arguments: Value) -> Result<Value, String> {
            *self.last_tool.borrow_mut() = tool.to_string();
            *self.last_args.borrow_mut() = arguments;
            self.response.clone()
        }
    }

    #[test]
    fn create_draft_calls_gmail_create_email_draft_with_composio_field_names() {
        let fake = RecordingComposio::ok(json!({ "successful": true, "data": {} }));
        let rpc = ComposioReadRpc::new(fake, "uid");
        let result = rpc.call_tool(
            Service::Gmail,
            "create_draft",
            json!({ "to": "alice@example.com", "subject": "Hello", "body": "World" }),
        );
        assert!(result.is_ok(), "create_draft should succeed: {result:?}");
        assert_eq!(*rpc.api.last_tool.borrow(), "GMAIL_CREATE_EMAIL_DRAFT");
        let args = rpc.api.last_args.borrow();
        assert_eq!(args["recipient_email"], "alice@example.com", "should use recipient_email");
        assert_eq!(args["subject"], "Hello");
        assert_eq!(args["body"], "World");
    }

    #[test]
    fn create_draft_missing_to_returns_error() {
        let fake = RecordingComposio::ok(json!({ "successful": true, "data": {} }));
        let rpc = ComposioReadRpc::new(fake, "uid");
        let err = rpc
            .call_tool(Service::Gmail, "create_draft", json!({ "subject": "S", "body": "B" }))
            .unwrap_err();
        assert!(err.contains("missing to"), "expected 'missing to', got: {err}");
    }

    #[test]
    fn create_draft_successful_false_returns_error() {
        let fake = RecordingComposio::failing(json!({ "successful": false }));
        let rpc = ComposioReadRpc::new(fake, "uid");
        let err = rpc
            .call_tool(
                Service::Gmail,
                "create_draft",
                json!({ "to": "b@b.com", "subject": "S", "body": "B" }),
            )
            .unwrap_err();
        assert!(!err.is_empty(), "successful:false must produce a non-empty error");
    }

    // ── transport-level API error (api.execute returns Err) ─────────────────────

    #[test]
    fn transport_err_propagates() {
        let rpc = ComposioReadRpc::new(FakeComposio::err("composio http 503"), "uid");
        let err = rpc
            .call_tool(Service::Gmail, "search_threads", json!({}))
            .unwrap_err();
        // Should surface the execute error (content-free status code only).
        assert!(!err.is_empty());
    }
}
