//! OpenAI-compatible Agent-lane client (`POST {base}/chat/completions`) — the provider
//! abstraction's second implementation, covering OpenRouter, OpenAI, and any local/hosted
//! endpoint that speaks the same schema (LM Studio, Ollama's OpenAI mode, …).
//!
//! Scope: **Agent lane only** (chat / drafts, the user's own key). The Batch lane (indexing /
//! Dream Cycle / Morning Brief) stays on the Anthropic Batch API with the Select KK key —
//! invariant 5's lane split is untouched, and this client is constructed with a [`ByokKey`]
//! so a Select KK key cannot reach it (type error).
//!
//! Same seams as [`super::anthropic`]: pure request builder + response parser (Linux-tested via
//! [`MockTransport`](super::transport::MockTransport)), a thin async client wiring
//! [`HttpTransport`] to a [`TraceabilitySink`], one digest-only trace per completion (AR-11/G8).

use serde_json::{json, Value};

use super::traceability::{Route, TraceRecord, TraceabilitySink};
use super::transport::{HttpRequest, HttpTransport, Method};
use super::{ByokKey, LlmError, Secret};

/// OpenRouter's OpenAI-compatible base URL.
pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
/// OpenAI's base URL.
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// Connection + model settings. Like [`super::anthropic::AnthropicConfig`], the model id is a
/// configurable string the caller supplies — never guessed here.
#[derive(Debug, Clone)]
pub struct OpenAiCompatConfig {
    /// Base URL up to (not including) `/chat/completions`, no trailing slash
    /// (e.g. `https://openrouter.ai/api/v1`).
    pub base_url: String,
    /// The model id to request (provider-specific, e.g. `anthropic/claude-sonnet-4.5` on
    /// OpenRouter or `gpt-4o-mini` on OpenAI).
    pub model: String,
    /// `max_tokens` for the request.
    pub max_tokens: u32,
}

impl OpenAiCompatConfig {
    /// Config for `model` against `base_url`, with a modest token cap.
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        let mut base_url: String = base_url.into();
        while base_url.ends_with('/') {
            base_url.pop();
        }
        Self { base_url, model: model.into(), max_tokens: 1024 }
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// The destination host recorded in traceability (scheme stripped, no path).
    fn destination(&self) -> String {
        let host = self.base_url.trim_start_matches("https://").trim_start_matches("http://");
        host.split('/').next().unwrap_or(host).to_string()
    }

    /// Standard headers, carrying `key` as `Authorization: Bearer …` (the OpenAI scheme).
    fn headers(&self, key: &Secret) -> Vec<(String, String)> {
        vec![
            ("authorization".to_string(), format!("Bearer {}", key.expose())),
            ("content-type".to_string(), "application/json".to_string()),
        ]
    }
}

/// Build the `POST {base}/chat/completions` request (non-streamed JSON response).
pub fn build_chat_request(
    cfg: &OpenAiCompatConfig,
    key: &Secret,
    prompt: &str,
) -> Result<HttpRequest, LlmError> {
    let body = json!({
        "model": cfg.model,
        "max_tokens": cfg.max_tokens,
        "messages": [{ "role": "user", "content": prompt }],
    });
    Ok(HttpRequest::new(
        Method::Post,
        format!("{}/chat/completions", cfg.base_url),
        cfg.headers(key),
        Some(body.to_string()),
    )?)
}

/// Parse a `chat/completions` JSON response into the assistant text
/// (`choices[0].message.content`).
pub fn parse_chat_response(body: &str) -> Result<String, LlmError> {
    let v: Value =
        serde_json::from_str(body).map_err(|e| LlmError::Parse(format!("chat response: {e}")))?;
    // Some providers put an application-level error object in a 200 body — surface it.
    if let Some(err) = v.get("error") {
        let msg = err.get("message").and_then(Value::as_str).unwrap_or("unknown provider error");
        return Err(LlmError::Provider(msg.to_string()));
    }
    v.get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| LlmError::Parse("chat response: no choices[0].message.content".into()))
}

/// Agent-lane client over an OpenAI-compatible endpoint. Constructed with a [`ByokKey`]
/// (invariant 5 — a Select KK key is a type error). Every completion records one digest-only
/// traceability row for the prompt chunk that left the device (AR-11 / G8); the row's
/// `destination` carries the real host (openrouter.ai, api.openai.com, …).
pub struct OpenAiCompatAgentClient<T: HttpTransport, S: TraceabilitySink> {
    transport: T,
    sink: S,
    key: ByokKey,
    cfg: OpenAiCompatConfig,
}

impl<T: HttpTransport, S: TraceabilitySink> OpenAiCompatAgentClient<T, S> {
    pub fn new(transport: T, sink: S, key: ByokKey, cfg: OpenAiCompatConfig) -> Self {
        Self { transport, sink, key, cfg }
    }

    /// Send `prompt` and return the assistant text. Records the send to traceability on success.
    pub async fn complete(&self, prompt: &str) -> Result<String, LlmError> {
        let req = build_chat_request(&self.cfg, self.key.secret(), prompt)?;
        let resp = self.transport.send(req).await?;
        if !resp.is_success() {
            return Err(LlmError::Provider(format!("chat/completions HTTP {}", resp.status)));
        }
        let text = parse_chat_response(&resp.body)?;
        self.sink.record(TraceRecord::for_chunk(
            Route::MessagesApi,
            "agent",
            self.cfg.destination(),
            prompt,
            false,
        ));
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::super::traceability::RecordingSink;
    use super::super::transport::{HttpResponse, MockTransport};
    use super::*;

    fn cfg() -> OpenAiCompatConfig {
        OpenAiCompatConfig::new(OPENROUTER_BASE_URL, "openai/gpt-4o-mini")
    }

    #[test]
    fn request_carries_bearer_auth_model_and_prompt() {
        let req = build_chat_request(&cfg(), &Secret::new("sk-or-123"), "write hi").unwrap();
        assert_eq!(req.url, "https://openrouter.ai/api/v1/chat/completions");
        assert!(req
            .headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer sk-or-123"));
        let body: Value = serde_json::from_str(req.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["model"], "openai/gpt-4o-mini");
        assert_eq!(body["messages"][0]["content"], "write hi");
    }

    #[test]
    fn trailing_slashes_in_base_url_are_normalised() {
        let c = OpenAiCompatConfig::new("https://api.openai.com/v1///", "gpt-4o-mini");
        assert_eq!(c.base_url, "https://api.openai.com/v1");
        assert_eq!(c.destination(), "api.openai.com");
    }

    #[test]
    fn parses_the_assistant_text() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"hello there"}}]}"#;
        assert_eq!(parse_chat_response(body).unwrap(), "hello there");
    }

    #[test]
    fn surfaces_provider_error_objects() {
        let body = r#"{"error":{"message":"invalid model"}}"#;
        assert!(matches!(parse_chat_response(body), Err(LlmError::Provider(m)) if m == "invalid model"));
    }

    #[test]
    fn missing_choices_is_a_parse_error() {
        assert!(matches!(parse_chat_response("{}"), Err(LlmError::Parse(_))));
    }

    #[tokio::test]
    async fn complete_returns_text_and_records_one_digest_trace() {
        let transport = MockTransport::ok(
            r#"{"choices":[{"message":{"role":"assistant","content":"the draft"}}]}"#,
        );
        let sink = RecordingSink::new();
        let client = OpenAiCompatAgentClient::new(
            transport,
            sink,
            ByokKey::new(Secret::new("sk-or-xyz")),
            cfg(),
        );
        let out = client.complete("prompt text").await.unwrap();
        assert_eq!(out, "the draft");
        let recs = client.sink.records();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].destination, "openrouter.ai");
        assert_eq!(recs[0].chunk_bytes, "prompt text".len());
    }

    #[tokio::test]
    async fn non_2xx_is_a_provider_error_and_traces_nothing() {
        let transport = MockTransport::new([HttpResponse { status: 401, body: String::new() }]);
        let sink = RecordingSink::new();
        let client = OpenAiCompatAgentClient::new(
            transport,
            sink,
            ByokKey::new(Secret::new("bad")),
            cfg(),
        );
        let err = client.complete("p").await.unwrap_err();
        assert!(matches!(err, LlmError::Provider(m) if m.contains("401")));
        assert!(client.sink.records().is_empty());
    }
}
