//! The Anthropic REST layer (WP3.1 network layer): pure request builders + response parsers, and
//! thin async clients that wire a [`HttpTransport`] to a [`TraceabilitySink`].
//!
//! Anthropic has no official Rust SDK, so this is raw HTTPS against the documented endpoints:
//! - Agent lane → `POST /v1/messages` (BYOK key).
//! - Batch lane → `POST /v1/messages/batches`, then poll `GET …/{id}` until `processing_status`
//!   is `"ended"`, then `GET …/{id}/results` (Select KK key). Results are keyed by `custom_id`
//!   and may arrive in any order.
//!
//! Everything except the actual socket write is a pure function ([`build_messages_request`],
//! [`parse_batch_results`], …) so request shape and response parsing are exhaustively tested on
//! Linux with a [`MockTransport`]. The invariant-5 key split is preserved end-to-end: the batch
//! client is constructed with a [`SelectKkKey`], the agent client with a [`ByokKey`].
//!
//! **Streaming note (SLO-03):** the transport returns a full response body, so token-by-token
//! first-token latency is not yet expressible here — [`AnthropicAgentClient::complete`] parses the
//! whole SSE body and returns the accumulated text. A streaming transport variant is the tracked
//! follow-up before the 1s-first-token SLO can be measured; the SSE parser ([`parse_sse_text`]) is
//! already in place for it.

use serde_json::{json, Value};

use super::traceability::{Route, TraceRecord, TraceabilitySink};
use super::transport::{HttpRequest, HttpTransport, Method};
use super::{ByokKey, LlmError, Secret, SelectKkKey};

/// The default `anthropic-version` header value.
pub const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";
/// The default API host.
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Connection + model settings. The model is a **configurable string**, never hardcoded: the
/// caller (settings / plan tier) supplies it, so the default model is a product decision, not a
/// value baked into the binary.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// Base URL, no trailing slash (e.g. `https://api.anthropic.com`).
    pub base_url: String,
    /// The `anthropic-version` header.
    pub version: String,
    /// The model id to request (supplied by the caller).
    pub model: String,
    /// `max_tokens` for the request.
    pub max_tokens: u32,
}

impl AnthropicConfig {
    /// Config for `model`, with the default host/version and a modest token cap. The model id is
    /// required — the layer never guesses one.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            version: DEFAULT_ANTHROPIC_VERSION.to_string(),
            model: model.into(),
            max_tokens: 1024,
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        // Normalise: no trailing slash so URL joins are simple.
        while self.base_url.ends_with('/') {
            self.base_url.pop();
        }
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// The destination host recorded in traceability (scheme stripped, no path).
    fn destination(&self) -> String {
        self.base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string()
    }

    /// Standard headers for a request on this config, carrying `key` as `x-api-key`.
    fn headers(&self, key: &Secret) -> Vec<(String, String)> {
        vec![
            ("x-api-key".to_string(), key.expose().to_string()),
            ("anthropic-version".to_string(), self.version.clone()),
            ("content-type".to_string(), "application/json".to_string()),
        ]
    }
}

// ---- pure request builders -----------------------------------------------------------------

/// One item of a batch: a processed chunk to classify, keyed by `custom_id`. `purpose` is for
/// traceability only and is not sent.
#[derive(Debug, Clone)]
pub struct BatchItem {
    pub custom_id: String,
    pub purpose: String,
    pub chunk: String,
}

/// Build the `POST /v1/messages` request (Agent lane). When `stream` is true the body sets
/// `"stream": true` and the response is SSE.
pub fn build_messages_request(
    cfg: &AnthropicConfig,
    key: &Secret,
    prompt: &str,
    stream: bool,
) -> Result<HttpRequest, LlmError> {
    let body = json!({
        "model": cfg.model,
        "max_tokens": cfg.max_tokens,
        "messages": [{ "role": "user", "content": prompt }],
        "stream": stream,
    });
    Ok(HttpRequest::new(
        Method::Post,
        format!("{}/v1/messages", cfg.base_url),
        cfg.headers(key),
        Some(body.to_string()),
    )?)
}

/// Build the `POST /v1/messages/batches` create request (Batch lane).
pub fn build_batch_create_request(
    cfg: &AnthropicConfig,
    key: &Secret,
    items: &[BatchItem],
) -> Result<HttpRequest, LlmError> {
    let requests: Vec<Value> = items
        .iter()
        .map(|it| {
            json!({
                "custom_id": it.custom_id,
                "params": {
                    "model": cfg.model,
                    "max_tokens": cfg.max_tokens,
                    "messages": [{ "role": "user", "content": it.chunk }],
                },
            })
        })
        .collect();
    let body = json!({ "requests": requests });
    Ok(HttpRequest::new(
        Method::Post,
        format!("{}/v1/messages/batches", cfg.base_url),
        cfg.headers(key),
        Some(body.to_string()),
    )?)
}

/// Build the `GET /v1/messages/batches/{id}` status-poll request.
pub fn build_batch_poll_request(
    cfg: &AnthropicConfig,
    key: &Secret,
    batch_id: &str,
) -> Result<HttpRequest, LlmError> {
    Ok(HttpRequest::new(
        Method::Get,
        format!("{}/v1/messages/batches/{}", cfg.base_url, batch_id),
        cfg.headers(key),
        None,
    )?)
}

/// Build the `GET /v1/messages/batches/{id}/results` request.
pub fn build_batch_results_request(
    cfg: &AnthropicConfig,
    key: &Secret,
    batch_id: &str,
) -> Result<HttpRequest, LlmError> {
    Ok(HttpRequest::new(
        Method::Get,
        format!("{}/v1/messages/batches/{}/results", cfg.base_url, batch_id),
        cfg.headers(key),
        None,
    )?)
}

// ---- pure response parsers -----------------------------------------------------------------

/// Where a batch is in its lifecycle (`processing_status`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchStatus {
    InProgress,
    Canceling,
    Ended,
    Other(String),
}

impl BatchStatus {
    fn from_str(s: &str) -> Self {
        match s {
            "in_progress" => BatchStatus::InProgress,
            "canceling" => BatchStatus::Canceling,
            "ended" => BatchStatus::Ended,
            other => BatchStatus::Other(other.to_string()),
        }
    }

    /// True once results are ready to fetch.
    pub fn is_ended(&self) -> bool {
        matches!(self, BatchStatus::Ended)
    }
}

/// A batch handle returned by create / poll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchHandle {
    pub id: String,
    pub status: BatchStatus,
}

/// Parse a batch create/poll response into an id + status.
pub fn parse_batch_handle(body: &str) -> Result<BatchHandle, LlmError> {
    let v: Value = serde_json::from_str(body).map_err(|e| LlmError::Parse(e.to_string()))?;
    let id = v
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| LlmError::Parse("batch response missing id".into()))?;
    let status = v
        .get("processing_status")
        .and_then(Value::as_str)
        .ok_or_else(|| LlmError::Parse("batch response missing processing_status".into()))?;
    Ok(BatchHandle { id: id.to_string(), status: BatchStatus::from_str(status) })
}

/// One batch result line, keyed by `custom_id`. Either succeeded (with accumulated text) or
/// carries an error type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchResult {
    pub custom_id: String,
    pub text: Option<String>,
    pub error: Option<String>,
}

/// Concatenate the `text` blocks of a message `content` array.
fn text_from_content(content: &Value) -> String {
    content
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<String>()
        })
        .unwrap_or_default()
}

/// Parse the JSONL results body (`GET …/results`). Each line is one `{custom_id, result}`; lines
/// arrive in any order, so callers key by `custom_id`. Malformed lines are surfaced as an error
/// entry, not silently dropped.
pub fn parse_batch_results(body: &str) -> Vec<BatchResult> {
    body.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| match serde_json::from_str::<Value>(line) {
            Ok(v) => {
                let custom_id = v
                    .get("custom_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let result = v.get("result");
                let result_type = result.and_then(|r| r.get("type")).and_then(Value::as_str);
                match result_type {
                    Some("succeeded") => {
                        let content = result
                            .and_then(|r| r.get("message"))
                            .and_then(|m| m.get("content"))
                            .cloned()
                            .unwrap_or(Value::Null);
                        BatchResult {
                            custom_id,
                            text: Some(text_from_content(&content)),
                            error: None,
                        }
                    }
                    Some(other) => BatchResult {
                        custom_id,
                        text: None,
                        error: Some(other.to_string()),
                    },
                    None => BatchResult {
                        custom_id,
                        text: None,
                        error: Some("missing result.type".into()),
                    },
                }
            }
            Err(e) => BatchResult { custom_id: String::new(), text: None, error: Some(e.to_string()) },
        })
        .collect()
}

/// Parse a non-streaming `POST /v1/messages` response into its text.
pub fn parse_messages_response(body: &str) -> Result<String, LlmError> {
    let v: Value = serde_json::from_str(body).map_err(|e| LlmError::Parse(e.to_string()))?;
    let content = v
        .get("content")
        .ok_or_else(|| LlmError::Parse("messages response missing content".into()))?;
    Ok(text_from_content(content))
}

/// Accumulate the assistant text from an SSE stream body. Reads `data:` lines whose event is a
/// `content_block_delta` carrying a `text_delta`, concatenating the deltas. Non-text events and
/// the `[DONE]`/`ping` lines are ignored.
pub fn parse_sse_text(body: &str) -> String {
    let mut out = String::new();
    for line in body.lines() {
        let line = line.trim_start();
        let Some(data) = line.strip_prefix("data:") else { continue };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
        if v.get("type").and_then(Value::as_str) != Some("content_block_delta") {
            continue;
        }
        if let Some(delta) = v.get("delta") {
            if delta.get("type").and_then(Value::as_str) == Some("text_delta") {
                if let Some(t) = delta.get("text").and_then(Value::as_str) {
                    out.push_str(t);
                }
            }
        }
    }
    out
}

/// Parse a completion body that may be either SSE (streamed) or a JSON object (non-streamed).
fn parse_completion_body(body: &str) -> Result<String, LlmError> {
    if body.trim_start().starts_with('{') {
        parse_messages_response(body)
    } else {
        Ok(parse_sse_text(body))
    }
}

// ---- async clients -------------------------------------------------------------------------

/// Agent-lane client (chat / drafts). Constructed with a [`ByokKey`] — invariant 5 means a
/// [`SelectKkKey`] cannot be substituted (type error). Every completion records one traceability
/// row for the prompt chunk that left the device (AR-11), digest-only (G8).
pub struct AnthropicAgentClient<T: HttpTransport, S: TraceabilitySink> {
    transport: T,
    sink: S,
    key: ByokKey,
    cfg: AnthropicConfig,
}

impl<T: HttpTransport, S: TraceabilitySink> AnthropicAgentClient<T, S> {
    pub fn new(transport: T, sink: S, key: ByokKey, cfg: AnthropicConfig) -> Self {
        Self { transport, sink, key, cfg }
    }

    /// Send `prompt` and return the assistant text. The traceability row is recorded at the TRUE
    /// egress point — before the request goes out — so a prompt that left the device but got a
    /// 401/timeout back is still traced (invariant 3: every send site logs, success or not).
    pub async fn complete(&self, prompt: &str) -> Result<String, LlmError> {
        let req = build_messages_request(&self.cfg, self.key.secret(), prompt, true)?;
        self.sink.record(TraceRecord::for_chunk(
            Route::MessagesApi,
            "agent",
            self.cfg.destination(),
            prompt,
            false,
        ));
        let resp = self.transport.send(req).await?;
        if !resp.is_success() {
            return Err(LlmError::Provider(format!("messages HTTP {}", resp.status)));
        }
        parse_completion_body(&resp.body)
    }
}

/// Classify a failed Batch-lane HTTP status. 401/403 means the credential is wrong, not that the
/// provider is having a bad night — and the Dream Cycle has to tell those apart, because it runs
/// unattended: a rejected key retried every night looks exactly like a service outage from the
/// outside, and the user is never told the one thing they could fix.
fn batch_status_error(step: &str, status: u16) -> LlmError {
    match status {
        401 | 403 => LlmError::Unauthorized(status),
        _ => LlmError::Provider(format!("batch {step} HTTP {status}")),
    }
}

/// Batch-lane client (indexing / classification / Dream Cycle / Morning Brief). Constructed with
/// a [`SelectKkKey`]. Exposes the three lifecycle steps separately — submit / poll / results — so
/// the Dream Cycle scheduler owns the poll cadence rather than this layer busy-waiting. Each
/// submitted chunk is recorded to traceability (AR-11), digest-only (G8).
pub struct AnthropicBatchClient<T: HttpTransport, S: TraceabilitySink> {
    transport: T,
    sink: S,
    key: SelectKkKey,
    cfg: AnthropicConfig,
}

impl<T: HttpTransport, S: TraceabilitySink> AnthropicBatchClient<T, S> {
    pub fn new(transport: T, sink: S, key: SelectKkKey, cfg: AnthropicConfig) -> Self {
        Self { transport, sink, key, cfg }
    }

    /// Create a batch from `items`. One traceability row per item, recorded at the TRUE egress
    /// point — before the request goes out — so chunks that left the device are traced even when
    /// the provider rejects the batch (invariant 3).
    pub async fn submit(&self, items: &[BatchItem]) -> Result<BatchHandle, LlmError> {
        let req = build_batch_create_request(&self.cfg, self.key.secret(), items)?;
        let dest = self.cfg.destination();
        for it in items {
            self.sink.record(TraceRecord::for_chunk(
                Route::BatchApi,
                it.purpose.clone(),
                dest.clone(),
                &it.chunk,
                false,
            ));
        }
        let resp = self.transport.send(req).await?;
        if !resp.is_success() {
            return Err(batch_status_error("create", resp.status));
        }
        parse_batch_handle(&resp.body)
    }

    /// Poll a batch's status. Callers loop this on their own cadence until [`BatchStatus::is_ended`].
    pub async fn poll(&self, batch_id: &str) -> Result<BatchHandle, LlmError> {
        let req = build_batch_poll_request(&self.cfg, self.key.secret(), batch_id)?;
        let resp = self.transport.send(req).await?;
        if !resp.is_success() {
            return Err(batch_status_error("poll", resp.status));
        }
        parse_batch_handle(&resp.body)
    }

    /// Fetch results once the batch has ended. Keyed by `custom_id` (any order).
    pub async fn results(&self, batch_id: &str) -> Result<Vec<BatchResult>, LlmError> {
        let req = build_batch_results_request(&self.cfg, self.key.secret(), batch_id)?;
        let resp = self.transport.send(req).await?;
        if !resp.is_success() {
            return Err(batch_status_error("results", resp.status));
        }
        Ok(parse_batch_results(&resp.body))
    }

    /// Run a batch to completion: submit, then poll until `ended` (up to `max_polls`), then fetch
    /// results. `sleep` is the delay between polls (injected so tests don't wait and the daemon
    /// controls the cadence — FR-DC-05: a batch that never ends within budget is an error the
    /// Dream Cycle carries to the next night, it does not block local features). Traceability is
    /// recorded by `submit` (one row per item).
    pub async fn run<F, Fut>(
        &self,
        items: &[BatchItem],
        max_polls: u32,
        mut sleep: F,
    ) -> Result<Vec<BatchResult>, LlmError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let handle = self.submit(items).await?;
        if handle.status.is_ended() {
            return self.results(&handle.id).await;
        }
        for _ in 0..max_polls {
            sleep().await;
            if self.poll(&handle.id).await?.status.is_ended() {
                return self.results(&handle.id).await;
            }
        }
        Err(LlmError::Provider("batch did not end within the poll budget".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::traceability::RecordingSink;
    use crate::llm::transport::{HttpResponse, MockTransport};

    fn cfg() -> AnthropicConfig {
        AnthropicConfig::new("test-model").with_base_url("https://api.anthropic.com")
    }

    /// A bad key and a bad night must not look the same to the Dream Cycle: it runs unattended, so
    /// only the first one is worth telling the user about, and only the second is worth retrying.
    #[tokio::test]
    async fn a_rejected_key_is_distinguishable_from_a_provider_failure() {
        use crate::llm::{Secret, SelectKkKey};
        let client = |status: u16| {
            AnthropicBatchClient::new(
                MockTransport::new([HttpResponse { status, body: "{}".into() }]),
                RecordingSink::new(),
                SelectKkKey::new(Secret::new("kk-123456")),
                cfg(),
            )
        };
        let items = [BatchItem {
            custom_id: "1".into(),
            purpose: "consolidation".into(),
            chunk: "x".into(),
        }];
        for status in [401u16, 403] {
            assert!(
                matches!(client(status).submit(&items).await, Err(LlmError::Unauthorized(s)) if s == status),
                "HTTP {status} is a credential problem"
            );
        }
        // a server-side failure stays a provider error, which is the retryable kind
        assert!(matches!(client(503).submit(&items).await, Err(LlmError::Provider(_))));
    }

    #[test]
    fn messages_request_shape_and_headers() {
        let key = Secret::new("byok-123456");
        let req = build_messages_request(&cfg(), &key, "hello", true).unwrap();
        assert_eq!(req.method, Method::Post);
        assert_eq!(req.url, "https://api.anthropic.com/v1/messages");
        // headers present
        assert!(req.headers.iter().any(|(k, v)| k == "x-api-key" && v == "byok-123456"));
        assert!(req.headers.iter().any(|(k, v)| k == "anthropic-version" && v == DEFAULT_ANTHROPIC_VERSION));
        assert!(req.headers.iter().any(|(k, v)| k == "content-type" && v == "application/json"));
        // body carries model + stream + prompt
        let body: Value = serde_json::from_str(req.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["model"], "test-model");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["content"], "hello");
    }

    #[test]
    fn batch_create_request_wraps_items() {
        let key = Secret::new("kk-abc");
        let items = vec![
            BatchItem { custom_id: "a".into(), purpose: "classify".into(), chunk: "chunk-a".into() },
            BatchItem { custom_id: "b".into(), purpose: "classify".into(), chunk: "chunk-b".into() },
        ];
        let req = build_batch_create_request(&cfg(), &key, &items).unwrap();
        assert_eq!(req.url, "https://api.anthropic.com/v1/messages/batches");
        let body: Value = serde_json::from_str(req.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["requests"].as_array().unwrap().len(), 2);
        assert_eq!(body["requests"][0]["custom_id"], "a");
        assert_eq!(body["requests"][1]["params"]["messages"][0]["content"], "chunk-b");
    }

    #[test]
    fn parse_handle_reads_id_and_status() {
        let h = parse_batch_handle(r#"{"id":"msgbatch_01","processing_status":"in_progress"}"#).unwrap();
        assert_eq!(h.id, "msgbatch_01");
        assert_eq!(h.status, BatchStatus::InProgress);
        assert!(!h.status.is_ended());

        let ended = parse_batch_handle(r#"{"id":"x","processing_status":"ended"}"#).unwrap();
        assert!(ended.status.is_ended());
    }

    #[test]
    fn parse_handle_rejects_malformed() {
        assert!(parse_batch_handle("not json").is_err());
        assert!(parse_batch_handle(r#"{"processing_status":"ended"}"#).is_err()); // no id
    }

    #[test]
    fn parse_results_keys_by_custom_id_any_order() {
        // two succeeded lines (out of submission order) + one errored
        let jsonl = concat!(
            r#"{"custom_id":"b","result":{"type":"succeeded","message":{"content":[{"type":"text","text":"B-label"}]}}}"#,
            "\n",
            r#"{"custom_id":"a","result":{"type":"succeeded","message":{"content":[{"type":"text","text":"A-"},{"type":"text","text":"label"}]}}}"#,
            "\n",
            r#"{"custom_id":"c","result":{"type":"errored","error":{"type":"invalid_request"}}}"#,
            "\n",
        );
        let results = parse_batch_results(jsonl);
        assert_eq!(results.len(), 3);
        let by_id = |id: &str| results.iter().find(|r| r.custom_id == id).unwrap();
        assert_eq!(by_id("a").text.as_deref(), Some("A-label")); // multi-block concatenated
        assert_eq!(by_id("b").text.as_deref(), Some("B-label"));
        assert!(by_id("c").text.is_none());
        assert_eq!(by_id("c").error.as_deref(), Some("errored"));
    }

    #[test]
    fn parse_messages_response_concatenates_text_blocks() {
        let body = r#"{"content":[{"type":"text","text":"draft "},{"type":"text","text":"reply"}]}"#;
        assert_eq!(parse_messages_response(body).unwrap(), "draft reply");
    }

    #[test]
    fn parse_sse_accumulates_text_deltas() {
        let sse = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\"}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n",
            "event: ping\n",
            "data: {\"type\":\"ping\"}\n\n",
            "data: [DONE]\n\n",
        );
        assert_eq!(parse_sse_text(sse), "Hello");
    }

    #[tokio::test]
    async fn agent_complete_returns_text_and_records_trace() {
        let sse = "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"hi there\"}}\n";
        let transport = MockTransport::ok(sse);
        let sink = RecordingSink::new();
        let client = AnthropicAgentClient::new(
            transport,
            sink,
            ByokKey::new(Secret::new("byok-xyz")),
            cfg(),
        );
        let out = client.complete("draft a reply to Alice").await.unwrap();
        assert_eq!(out, "hi there");
        // exactly one trace row, digest-only, correct route, no prompt text
        let recs = client.sink_records();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].route, Route::MessagesApi);
        assert_eq!(recs[0].chunk_bytes, "draft a reply to Alice".len());
        assert!(!format!("{:?}", recs[0]).contains("Alice"));
        // the api key never appears in the captured request Debug
        let sent = client.transport_sent();
        assert!(!format!("{:?}", sent[0]).contains("byok-xyz"));
    }

    #[tokio::test]
    async fn agent_complete_surfaces_http_error() {
        let transport = MockTransport::new([HttpResponse { status: 429, body: "rate limited".into() }]);
        let client = AnthropicAgentClient::new(
            transport,
            RecordingSink::new(),
            ByokKey::new(Secret::new("byok")),
            cfg(),
        );
        let err = client.complete("x").await.unwrap_err();
        assert!(matches!(err, LlmError::Provider(_)));
        // the prompt STILL left the device — the egress is traced even on provider failure
        // (invariant 3: send sites always log)
        assert_eq!(client.sink_records().len(), 1);
    }

    #[tokio::test]
    async fn batch_submit_records_one_trace_per_item() {
        let transport = MockTransport::ok(r#"{"id":"msgbatch_9","processing_status":"in_progress"}"#);
        let client = AnthropicBatchClient::new(
            transport,
            RecordingSink::new(),
            SelectKkKey::new(Secret::new("kk-999")),
            cfg(),
        );
        let items = vec![
            BatchItem { custom_id: "1".into(), purpose: "index".into(), chunk: "one".into() },
            BatchItem { custom_id: "2".into(), purpose: "index".into(), chunk: "two".into() },
        ];
        let handle = client.submit(&items).await.unwrap();
        assert_eq!(handle.id, "msgbatch_9");
        let recs = client.sink_records();
        assert_eq!(recs.len(), 2);
        assert!(recs.iter().all(|r| r.route == Route::BatchApi));
        assert_eq!(recs[0].chunk_bytes, 3);
    }

    #[tokio::test]
    async fn batch_poll_then_results_roundtrip() {
        // poll → ended, then fetch results
        let transport = MockTransport::new([
            HttpResponse { status: 200, body: r#"{"id":"b","processing_status":"ended"}"#.into() },
            HttpResponse {
                status: 200,
                body: r#"{"custom_id":"1","result":{"type":"succeeded","message":{"content":[{"type":"text","text":"meeting"}]}}}"#.into(),
            },
        ]);
        let client = AnthropicBatchClient::new(
            transport,
            RecordingSink::new(),
            SelectKkKey::new(Secret::new("kk")),
            cfg(),
        );
        let handle = client.poll("b").await.unwrap();
        assert!(handle.status.is_ended());
        let results = client.results("b").await.unwrap();
        assert_eq!(results[0].text.as_deref(), Some("meeting"));
        // poll/results do not write traceability (only submit does)
        assert!(client.sink_records().is_empty());
    }

    #[tokio::test]
    async fn run_submits_polls_until_ended_then_fetches_results() {
        // create (in_progress) → poll (in_progress) → poll (ended) → results
        let transport = MockTransport::new([
            HttpResponse { status: 200, body: r#"{"id":"b","processing_status":"in_progress"}"#.into() },
            HttpResponse { status: 200, body: r#"{"id":"b","processing_status":"in_progress"}"#.into() },
            HttpResponse { status: 200, body: r#"{"id":"b","processing_status":"ended"}"#.into() },
            HttpResponse {
                status: 200,
                body: r#"{"custom_id":"1","result":{"type":"succeeded","message":{"content":[{"type":"text","text":"consolidated"}]}}}"#.into(),
            },
        ]);
        let client = AnthropicBatchClient::new(
            transport,
            RecordingSink::new(),
            SelectKkKey::new(Secret::new("kk")),
            cfg(),
        );
        let items = vec![BatchItem { custom_id: "1".into(), purpose: "consolidation".into(), chunk: "today's events".into() }];
        let results = client.run(&items, 5, || async {}).await.unwrap();
        assert_eq!(results[0].text.as_deref(), Some("consolidated"));
        // submit recorded exactly one traceability row for the chunk
        assert_eq!(client.sink_records().len(), 1);
    }

    #[tokio::test]
    async fn run_short_circuits_when_batch_ends_immediately() {
        let transport = MockTransport::new([
            HttpResponse { status: 200, body: r#"{"id":"b","processing_status":"ended"}"#.into() },
            HttpResponse {
                status: 200,
                body: r#"{"custom_id":"1","result":{"type":"succeeded","message":{"content":[{"type":"text","text":"done"}]}}}"#.into(),
            },
        ]);
        let client = AnthropicBatchClient::new(transport, RecordingSink::new(), SelectKkKey::new(Secret::new("kk")), cfg());
        let items = vec![BatchItem { custom_id: "1".into(), purpose: "index".into(), chunk: "c".into() }];
        let results = client.run(&items, 3, || async {}).await.unwrap();
        assert_eq!(results[0].text.as_deref(), Some("done"));
    }

    #[tokio::test]
    async fn run_errors_when_batch_never_ends_within_budget() {
        // create + 2 polls, all in_progress; budget of 2 polls is exhausted
        let transport = MockTransport::new([
            HttpResponse { status: 200, body: r#"{"id":"b","processing_status":"in_progress"}"#.into() },
            HttpResponse { status: 200, body: r#"{"id":"b","processing_status":"in_progress"}"#.into() },
            HttpResponse { status: 200, body: r#"{"id":"b","processing_status":"in_progress"}"#.into() },
        ]);
        let client = AnthropicBatchClient::new(transport, RecordingSink::new(), SelectKkKey::new(Secret::new("kk")), cfg());
        let items = vec![BatchItem { custom_id: "1".into(), purpose: "index".into(), chunk: "c".into() }];
        let err = client.run(&items, 2, || async {}).await.unwrap_err();
        assert!(matches!(err, LlmError::Provider(_)));
    }

    // small accessors so the tests can read back the mock/sink held by the client
    impl<T: HttpTransport, S: TraceabilitySink> AnthropicAgentClient<T, S> {
        fn sink_records(&self) -> Vec<TraceRecord>
        where
            S: AsRecording,
        {
            self.sink.recorded()
        }
        fn transport_sent(&self) -> Vec<HttpRequest>
        where
            T: AsSent,
        {
            self.transport.sent_requests()
        }
    }
    impl<T: HttpTransport, S: TraceabilitySink> AnthropicBatchClient<T, S> {
        fn sink_records(&self) -> Vec<TraceRecord>
        where
            S: AsRecording,
        {
            self.sink.recorded()
        }
    }

    // test-only shims to read the concrete doubles through the generic clients
    trait AsRecording {
        fn recorded(&self) -> Vec<TraceRecord>;
    }
    impl AsRecording for RecordingSink {
        fn recorded(&self) -> Vec<TraceRecord> {
            self.records()
        }
    }
    trait AsSent {
        fn sent_requests(&self) -> Vec<HttpRequest>;
    }
    impl AsSent for MockTransport {
        fn sent_requests(&self) -> Vec<HttpRequest> {
            self.sent()
        }
    }
}
