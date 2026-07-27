//! Composio-based Gmail read transport — routes Gmail read and draft calls through the Composio
//! tool-execution API. This is the single transport for all Gmail operations (reads + drafts).
//!
//! This is the second-layer read transport (§6.10). Per the allowlisted-egress rule (FR-TR-03),
//! the HTTP client stays in shogun-core; the trait seam ([`ComposioApi`]) is in
//! shogun-integrations so the pure logic compiles without reqwest.
//!
//! The Composio response shape was VERIFIED against a live `GMAIL_FETCH_EMAILS` call (2026-07):
//! envelope `{ data: { messages: [...] }, successful: bool, error }`; each message carries
//! `messageId` / `threadId`, a TOP-LEVEL `subject`, the decoded plaintext under `messageText`
//! (with `preview.body` as a shorter fallback), and an ISO-8601 `messageTimestamp`. Note it does
//! NOT use the raw Gmail-REST names (`snippet`, `internalDate`) — the mapping in
//! [`record_from_composio_group`] is Composio-native.
//!
//! **Envelope extraction is isolated in [`extract_messages`]** and field mapping in
//! [`record_from_composio_group`] — patch those two if a future Composio version differs.
//!
//! No item content, tokens, or secrets are ever surfaced in error strings (invariant 7).

use serde_json::{json, Value};
use shogun_integrations::composio::{parse_execute_response, ComposioApi};
use shogun_integrations::rpc::McpRpc;
use shogun_mcp::scope::Service;

use crate::gmail_shape::{base64_url_decode, envelope};

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

    /// Extract messages from the Composio response, group them by threadId, shape each group into
    /// a normalized record ([`record_from_composio_group`]), and return a `structuredContent`
    /// envelope for [`parse_items`].
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

/// Cap on a thread's concatenated body (mirrors gmail_shape::thread_body).
const MAX_BODY_BYTES: usize = 16 * 1024;

/// Group a flat list of Composio messages by `threadId` and shape each group into one
/// `parse_items`-compatible record `{ threadId, subject, body, ts_ms }`.
///
/// VERIFIED against a live `GMAIL_FETCH_EMAILS` response (2026-07): Composio does NOT use the raw
/// Gmail-REST field names. A message carries `messageId` / `threadId`, a TOP-LEVEL `subject`, the
/// decoded plaintext under `messageText` (with `preview.body` as a shorter fallback), and an
/// ISO-8601 `messageTimestamp` — there is no `snippet` and no `internalDate`. Reading it as a
/// Gmail-REST thread (the earlier approach) produced ts=0 and an empty snippet, so the mapping is
/// Composio-native here. `parse_items` maps `threadId`→external_id, `subject`→title, `body`→body,
/// `ts_ms`→timestamp.
fn group_and_shape(messages: Vec<Value>) -> Vec<Value> {
    // Preserve first-seen order.
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<Value>> =
        std::collections::HashMap::new();

    for msg in messages {
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
            Some(record_from_composio_group(&thread_id, &msgs))
        })
        .collect()
}

/// Shape one thread's Composio messages into a normalized record.
fn record_from_composio_group(thread_id: &str, msgs: &[Value]) -> Value {
    // Subject/timestamp come from the latest message in the group.
    let last = msgs.last();
    let subject = last
        .and_then(|m| m.get("subject").and_then(Value::as_str))
        .or_else(|| last.and_then(|m| m.get("preview").and_then(|p| p.get("subject")).and_then(Value::as_str)))
        .unwrap_or_default()
        .to_string();
    let ts_ms = last
        .and_then(|m| m.get("messageTimestamp").and_then(Value::as_str))
        .map(iso8601_to_ms)
        .unwrap_or(0);

    // Body: join each message's plaintext, capped. Composio gives the decoded text directly.
    let mut parts: Vec<String> = Vec::new();
    for m in msgs {
        let body = composio_message_body(m);
        if !body.trim().is_empty() {
            parts.push(body);
        }
    }
    let joined = parts.join("\n\n---\n\n");
    let body = if joined.len() <= MAX_BODY_BYTES {
        joined
    } else {
        let mut end = MAX_BODY_BYTES;
        while !joined.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &joined[..end])
    };

    json!({
        "threadId": thread_id,
        "subject": subject,
        "body": body,
        "ts_ms": ts_ms,
    })
}

/// Extract one Composio message's plaintext body: prefer the pre-decoded `messageText`, then
/// `preview.body`, then base64url-decode `payload.body.data` (single-part text/plain).
fn composio_message_body(m: &Value) -> String {
    if let Some(t) = m.get("messageText").and_then(Value::as_str) {
        if !t.trim().is_empty() {
            return t.to_string();
        }
    }
    if let Some(p) = m.get("preview").and_then(|p| p.get("body")).and_then(Value::as_str) {
        if !p.trim().is_empty() {
            return p.to_string();
        }
    }
    if let Some(data) = m
        .get("payload")
        .and_then(|p| p.get("body"))
        .and_then(|b| b.get("data"))
        .and_then(Value::as_str)
    {
        if !data.is_empty() {
            return String::from_utf8_lossy(&base64_url_decode(data)).into_owned();
        }
    }
    String::new()
}

/// Parse a fixed-format ISO-8601 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`, optional fractional
/// seconds) to unix milliseconds. Returns 0 on any parse failure — no dependency on a date crate.
fn iso8601_to_ms(s: &str) -> i64 {
    // Split date and time on 'T'.
    let (date, time) = match s.split_once('T') {
        Some(v) => v,
        None => return 0,
    };
    let d: Vec<&str> = date.split('-').collect();
    if d.len() != 3 {
        return 0;
    }
    // Time: drop trailing 'Z' and any fractional part, then split H:M:S.
    let time = time.trim_end_matches('Z');
    let time = time.split('.').next().unwrap_or(time);
    let t: Vec<&str> = time.split(':').collect();
    if t.len() != 3 {
        return 0;
    }
    let parse = |x: &str| x.parse::<i64>().ok();
    let (Some(y), Some(mo), Some(day), Some(h), Some(mi), Some(se)) = (
        parse(d[0]),
        parse(d[1]),
        parse(d[2]),
        parse(t[0]),
        parse(t[1]),
        parse(t[2]),
    ) else {
        return 0;
    };
    // days_from_civil (Howard Hinnant): days since 1970-01-01.
    let y = if mo <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    (days * 86400 + h * 3600 + mi * 60 + se) * 1000
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
    fn fetch_emails_response_shapes_to_fetched_item() {
        // The VERIFIED live GMAIL_FETCH_EMAILS shape: top-level subject, decoded messageText,
        // ISO-8601 messageTimestamp — no snippet, no internalDate.
        let composio_resp = json!({
            "data": {
                "messages": [{
                    "messageId": "m1",
                    "threadId": "t1",
                    "subject": "Test Subject",
                    "messageText": "Hello from Composio",
                    "messageTimestamp": "2000-01-01T00:00:00Z",
                    "preview": { "subject": "Test Subject", "body": "Hello from" },
                    "payload": { "mimeType": "text/plain", "body": { "data": "" } }
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
            "messageText should appear in item.body, got: {:?}",
            item.body
        );
        // 2000-01-01T00:00:00Z = 946684800 s = 946684800000 ms.
        assert_eq!(item.ts_ms, 946_684_800_000i64, "ISO timestamp must parse (not 0)");
    }

    #[test]
    fn iso8601_parses_to_unix_ms() {
        assert_eq!(iso8601_to_ms("1970-01-01T00:00:00Z"), 0);
        assert_eq!(iso8601_to_ms("1970-01-01T00:00:01Z"), 1000);
        assert_eq!(iso8601_to_ms("2000-01-01T00:00:00Z"), 946_684_800_000);
        // fractional seconds tolerated, trailing Z optional
        assert_eq!(iso8601_to_ms("2000-01-01T00:00:00.123Z"), 946_684_800_000);
        // malformed → 0 (never panics)
        assert_eq!(iso8601_to_ms("not-a-date"), 0);
        assert_eq!(iso8601_to_ms(""), 0);
    }

    // ── two messages with the same threadId → single grouped record ─────────────

    #[test]
    fn two_messages_same_thread_group_into_one_record_with_both_bodies() {
        let composio_resp = json!({
            "data": {
                "messages": [
                    {
                        "messageId": "m1",
                        "threadId": "thread-abc",
                        "subject": "Thread Convo",
                        "messageText": "First message body",
                        "messageTimestamp": "2000-01-01T00:00:00Z"
                    },
                    {
                        "messageId": "m2",
                        "threadId": "thread-abc",
                        "subject": "Thread Convo",
                        "messageText": "Second message body",
                        "messageTimestamp": "2000-01-01T00:01:40Z"
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
