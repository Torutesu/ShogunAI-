//! The Batch-relay client (docs/batch-relay-design.md, Plan C-2).
//!
//! The shipping Batch lane never holds an Anthropic key: the device holds only a **license
//! token** (FR-BIL-08 — signed, device-bound and ~24h-lived, so it is cached in `billing.json`
//! rather than the Keychain; CLAUDE.md invariant 7 の 2026-08-13 例外) and talks to the
//! Select-operated relay
//! (`relay.shogun.app`), which verifies the token, enforces the plan's daily chunk cap, and
//! delegates to the Anthropic Batch API with the operator's server-side key. Two consequences
//! are encoded here:
//!
//! - **The device never names a model** (§4.4): a client that can pick the model can pick an
//!   expensive one, so the request carries only a [`ModelClass`] intent and the relay chooses.
//! - **The route is distinguishable** (§3.3): every submitted chunk records
//!   [`Route::BatchRelay`] — not `BatchApi` — so the traceability screen can show "via operator
//!   server", the same way Composio sends show "via third party".
//!
//! Same lifecycle as [`AnthropicBatchClient`](super::anthropic::AnthropicBatchClient) — submit /
//! poll / results, unified by the [`BatchLane`] trait — so the Dream Cycle scheduler is oblivious
//! to which lane is wired in. The relay wire shape differs (see §4.2/§4.3): create returns
//! `{batch_id, accepted}`, and status/results share one `GET /v1/batch/{id}` endpoint.
//!
//! Everything except the socket write is a pure function, exhaustively tested on Linux with the
//! same [`MockTransport`](super::transport::MockTransport) the Anthropic client uses.

use serde_json::{json, Value};

use super::anthropic::{BatchHandle, BatchItem, BatchLane, BatchResult, BatchStatus};
use super::traceability::{Route, TraceRecord, TraceabilitySink};
use super::transport::{HttpRequest, HttpTransport, Method};
use super::{LicenseBearer, LlmError, Secret};

/// The production relay host. Overridable per-config for a staging relay; never overridable to
/// plain HTTP ([`HttpRequest::new`] rejects non-HTTPS).
pub const DEFAULT_RELAY_BASE_URL: &str = "https://relay.shogun.app";

/// The *intent* the device sends instead of a model id (§4.4). The relay maps each class to a
/// concrete model server-side, so model choice — and therefore unit cost — is an operator
/// decision, not a client capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelClass {
    /// Per-event labelling (Dream Cycle consolidation / indexing).
    Classify,
    /// Abstractive summarisation (Compression).
    Summarize,
    /// Morning Brief generation.
    Brief,
}

impl ModelClass {
    /// The wire string in the `model_class` field.
    pub fn as_str(self) -> &'static str {
        match self {
            ModelClass::Classify => "classify",
            ModelClass::Summarize => "summarize",
            ModelClass::Brief => "brief",
        }
    }
}

/// Relay connection settings. Deliberately has **no model and no max_tokens field** — those are
/// the relay's decisions (§4.4). Compare [`super::anthropic::AnthropicConfig`], which must carry
/// both because it talks to Anthropic directly.
#[derive(Debug, Clone)]
pub struct RelayConfig {
    /// Base URL, no trailing slash (e.g. `https://relay.shogun.app`).
    pub base_url: String,
    /// The intent sent as `model_class`.
    pub model_class: ModelClass,
}

impl RelayConfig {
    /// Config for the default relay host.
    pub fn new(model_class: ModelClass) -> Self {
        Self { base_url: DEFAULT_RELAY_BASE_URL.to_string(), model_class }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        // Normalise: no trailing slash so URL joins are simple.
        while self.base_url.ends_with('/') {
            self.base_url.pop();
        }
        self
    }

    /// The destination host recorded in traceability (scheme stripped, no path).
    fn destination(&self) -> String {
        self.base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string()
    }

    /// Standard headers, carrying the license token as `Authorization: Bearer …` (§4.1 — the
    /// FR-BIL-08 license JWT, never an Anthropic key). `expose()` here is the single egress
    /// point, mirroring the `x-api-key` builder in `anthropic.rs`; the transport's `Debug`
    /// redacts the `authorization` header, so the token cannot leak through a captured request.
    fn headers(&self, token: &Secret) -> Vec<(String, String)> {
        vec![
            ("authorization".to_string(), format!("Bearer {}", token.expose())),
            ("content-type".to_string(), "application/json".to_string()),
        ]
    }
}

// ---- pure request builders -----------------------------------------------------------------

/// Build the `POST /v1/batch` create request (§4.2). The body carries `purpose` (traceability
/// vocabulary, taken from the items), `model_class` (the intent — never a model id), and the
/// items as `{custom_id, chunk}` pairs. There is deliberately no way to put a `model` key in
/// this body.
pub fn build_relay_submit_request(
    cfg: &RelayConfig,
    token: &Secret,
    items: &[BatchItem],
) -> Result<HttpRequest, LlmError> {
    let purpose = items.first().map(|it| it.purpose.as_str()).unwrap_or("batch");
    let wire_items: Vec<Value> = items
        .iter()
        .map(|it| json!({ "custom_id": it.custom_id, "chunk": it.chunk }))
        .collect();
    let body = json!({
        "purpose": purpose,
        "model_class": cfg.model_class.as_str(),
        "items": wire_items,
    });
    Ok(HttpRequest::new(
        Method::Post,
        format!("{}/v1/batch", cfg.base_url),
        cfg.headers(token),
        Some(body.to_string()),
    )?)
}

/// Build the `GET /v1/batch/{id}` request (§4.3). One endpoint serves both status and, once
/// ended, the results.
pub fn build_relay_status_request(
    cfg: &RelayConfig,
    token: &Secret,
    batch_id: &str,
) -> Result<HttpRequest, LlmError> {
    Ok(HttpRequest::new(
        Method::Get,
        format!("{}/v1/batch/{}", cfg.base_url, batch_id),
        cfg.headers(token),
        None,
    )?)
}

// ---- pure response parsers -----------------------------------------------------------------

/// Parse the `202 Accepted` create response (`{"batch_id": "rb_…", "accepted": n}`) into a
/// handle. A freshly accepted batch is in progress by definition.
pub fn parse_relay_submit(body: &str) -> Result<BatchHandle, LlmError> {
    let v: Value = serde_json::from_str(body).map_err(|e| LlmError::Parse(e.to_string()))?;
    let id = v
        .get("batch_id")
        .and_then(Value::as_str)
        .ok_or_else(|| LlmError::Parse("relay create response missing batch_id".into()))?;
    Ok(BatchHandle { id: id.to_string(), status: BatchStatus::InProgress })
}

/// Parse the status half of a `GET /v1/batch/{id}` response. The relay echoes no id, so the
/// caller supplies the one it asked about.
pub fn parse_relay_status(batch_id: &str, body: &str) -> Result<BatchHandle, LlmError> {
    let v: Value = serde_json::from_str(body).map_err(|e| LlmError::Parse(e.to_string()))?;
    let status = v
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| LlmError::Parse("relay status response missing status".into()))?;
    let status = match status {
        "in_progress" => BatchStatus::InProgress,
        "ended" => BatchStatus::Ended,
        other => BatchStatus::Other(other.to_string()),
    };
    Ok(BatchHandle { id: batch_id.to_string(), status })
}

/// Parse the results half of an ended `GET /v1/batch/{id}` response:
/// `{"status":"ended","results":[{"custom_id":"…","text":"…"}]}`. Results are keyed by
/// `custom_id` and may arrive in any order — the same contract `parse_batch_classification`
/// already reads (§4.3). A result carrying `error` instead of `text` surfaces as an error entry,
/// not a silent drop.
pub fn parse_relay_results(body: &str) -> Result<Vec<BatchResult>, LlmError> {
    let v: Value = serde_json::from_str(body).map_err(|e| LlmError::Parse(e.to_string()))?;
    if v.get("status").and_then(Value::as_str) != Some("ended") {
        return Err(LlmError::Provider("relay batch has not ended; no results yet".into()));
    }
    let results = v
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| LlmError::Parse("relay ended response missing results".into()))?;
    Ok(results
        .iter()
        .map(|r| {
            let custom_id = r.get("custom_id").and_then(Value::as_str).unwrap_or("").to_string();
            let text = r.get("text").and_then(Value::as_str).map(str::to_string);
            let error = r.get("error").and_then(Value::as_str).map(str::to_string);
            match (text, error) {
                (Some(text), _) => BatchResult { custom_id, text: Some(text), error: None },
                (None, Some(err)) => BatchResult { custom_id, text: None, error: Some(err) },
                (None, None) => BatchResult {
                    custom_id,
                    text: None,
                    error: Some("missing text and error".into()),
                },
            }
        })
        .collect())
}

/// A failed relay call, mapped per §4.5. 401/403 → `Unauthorized` (re-verify the license),
/// 429 → `RateLimited` (daily cap reached — tonight runs the local lane), 402 →
/// `QuotaExhausted` (plan does not cover the Batch lane — not a bug, not the user's key), and
/// anything else stays a retryable `Provider` failure.
fn relay_status_error(step: &str, status: u16, body: &str) -> LlmError {
    if status == 402 {
        return LlmError::QuotaExhausted(
            "the current plan does not include the Batch lane (HTTP 402)".into(),
        );
    }
    crate::llm::status_error(&format!("relay {step}"), status, body)
}

// ---- async client --------------------------------------------------------------------------

/// Batch-lane client that goes through the Select-operated relay. Constructed with a
/// [`LicenseBearer`] — the FR-BIL-08 licence token, never an Anthropic key: the type makes
/// wiring a raw `sk-ant-` credential into this path (and so sending the operator key to a
/// non-Anthropic host) a compile error, the same way invariant 5 separates the lanes. Same
/// submit / poll / results lifecycle as the direct client, via [`BatchLane`].
pub struct RelayBatchClient<T: HttpTransport, S: TraceabilitySink> {
    transport: T,
    sink: S,
    token: LicenseBearer,
    cfg: RelayConfig,
}

impl<T: HttpTransport, S: TraceabilitySink> RelayBatchClient<T, S> {
    pub fn new(transport: T, sink: S, token: LicenseBearer, cfg: RelayConfig) -> Self {
        Self { transport, sink, token, cfg }
    }

    /// Create a batch from `items`. One traceability row per item at the TRUE egress point —
    /// before the request goes out — with [`Route::BatchRelay`] so the viewer can say "via
    /// operator server" (§3.3, invariant 3).
    pub async fn submit(&self, items: &[BatchItem]) -> Result<BatchHandle, LlmError> {
        let req = build_relay_submit_request(&self.cfg, self.token.secret(), items)?;
        let dest = self.cfg.destination();
        for it in items {
            self.sink.record(TraceRecord::for_chunk(
                Route::BatchRelay,
                it.purpose.clone(),
                dest.clone(),
                &it.chunk,
                false,
            ));
        }
        let resp = self.transport.send(req).await?;
        if !resp.is_success() {
            return Err(relay_status_error("create", resp.status, &resp.body));
        }
        parse_relay_submit(&resp.body)
    }

    /// Poll a batch's status. Callers loop on their own cadence until
    /// [`BatchStatus::is_ended`].
    pub async fn poll(&self, batch_id: &str) -> Result<BatchHandle, LlmError> {
        let req = build_relay_status_request(&self.cfg, self.token.secret(), batch_id)?;
        let resp = self.transport.send(req).await?;
        if !resp.is_success() {
            return Err(relay_status_error("poll", resp.status, &resp.body));
        }
        parse_relay_status(batch_id, &resp.body)
    }

    /// Fetch results once the batch has ended (same endpoint as poll; the ended response carries
    /// them inline). Keyed by `custom_id` (any order).
    pub async fn results(&self, batch_id: &str) -> Result<Vec<BatchResult>, LlmError> {
        let req = build_relay_status_request(&self.cfg, self.token.secret(), batch_id)?;
        let resp = self.transport.send(req).await?;
        if !resp.is_success() {
            return Err(relay_status_error("results", resp.status, &resp.body));
        }
        parse_relay_results(&resp.body)
    }

    /// Run a batch to completion: submit → poll until ended (≤ `max_polls`) → results. Same
    /// contract as the direct client's `run` (FR-DC-05: a batch that never ends within budget is
    /// an error carried to the next night).
    pub async fn run<F, Fut>(
        &self,
        items: &[BatchItem],
        max_polls: u32,
        sleep: F,
    ) -> Result<Vec<BatchResult>, LlmError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        super::anthropic::run_batch_to_completion(self, items, max_polls, sleep).await
    }
}

impl<T: HttpTransport, S: TraceabilitySink> BatchLane for RelayBatchClient<T, S> {
    fn submit(
        &self,
        items: &[BatchItem],
    ) -> impl std::future::Future<Output = Result<BatchHandle, LlmError>> + Send {
        RelayBatchClient::submit(self, items)
    }

    fn poll(
        &self,
        batch_id: &str,
    ) -> impl std::future::Future<Output = Result<BatchHandle, LlmError>> + Send {
        RelayBatchClient::poll(self, batch_id)
    }

    fn results(
        &self,
        batch_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<BatchResult>, LlmError>> + Send {
        RelayBatchClient::results(self, batch_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::traceability::RecordingSink;
    use crate::llm::transport::{HttpResponse, MockTransport};

    fn cfg() -> RelayConfig {
        RelayConfig::new(ModelClass::Classify).with_base_url("https://relay.shogun.app")
    }

    fn client(
        responses: impl IntoIterator<Item = HttpResponse>,
    ) -> RelayBatchClient<MockTransport, RecordingSink> {
        RelayBatchClient::new(
            MockTransport::new(responses),
            RecordingSink::new(),
            LicenseBearer::new(Secret::new("v1.license-token-123456")),
            cfg(),
        )
    }

    fn items() -> Vec<BatchItem> {
        vec![
            BatchItem { custom_id: "1".into(), purpose: "consolidation".into(), chunk: "one".into() },
            BatchItem { custom_id: "2".into(), purpose: "consolidation".into(), chunk: "two".into() },
        ]
    }

    #[test]
    fn submit_request_carries_bearer_token_and_model_class_never_a_model_id() {
        let token = Secret::new("eyJ-license-jwt");
        let req = build_relay_submit_request(&cfg(), &token, &items()).unwrap();
        assert_eq!(req.method, Method::Post);
        assert_eq!(req.url, "https://relay.shogun.app/v1/batch");
        assert!(req
            .headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer eyJ-license-jwt"));
        // The relay lane never sends an x-api-key — the device holds no Anthropic key.
        assert!(!req.headers.iter().any(|(k, _)| k == "x-api-key"));

        let body: Value = serde_json::from_str(req.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["purpose"], "consolidation");
        assert_eq!(body["model_class"], "classify");
        assert_eq!(body["items"].as_array().unwrap().len(), 2);
        assert_eq!(body["items"][0]["custom_id"], "1");
        assert_eq!(body["items"][1]["chunk"], "two");
        // §4.4: the device does not name a model — there must be no model key anywhere.
        assert!(body.get("model").is_none());
        assert!(!req.body.as_deref().unwrap().contains("\"model\":"));
    }

    #[test]
    fn the_license_token_is_redacted_in_a_captured_request_debug() {
        let token = Secret::new("eyJ-license-jwt-super-secret");
        let req = build_relay_submit_request(&cfg(), &token, &items()).unwrap();
        let dumped = format!("{req:?}");
        assert!(!dumped.contains("super-secret"), "license token leaked via Debug: {dumped}");
    }

    #[test]
    fn parse_submit_reads_batch_id_and_is_in_progress() {
        let h = parse_relay_submit(r#"{"batch_id":"rb_01","accepted":812}"#).unwrap();
        assert_eq!(h.id, "rb_01");
        assert_eq!(h.status, BatchStatus::InProgress);
        assert!(parse_relay_submit(r#"{"accepted":1}"#).is_err()); // no batch_id
        assert!(parse_relay_submit("not json").is_err());
    }

    #[test]
    fn parse_status_maps_the_relay_vocabulary() {
        let h = parse_relay_status("rb_1", r#"{"status":"in_progress","completed":300,"total":812}"#)
            .unwrap();
        assert_eq!(h.id, "rb_1");
        assert!(!h.status.is_ended());
        let ended = parse_relay_status("rb_1", r#"{"status":"ended","results":[]}"#).unwrap();
        assert!(ended.status.is_ended());
        assert!(parse_relay_status("rb_1", r#"{"nope":true}"#).is_err());
    }

    #[test]
    fn parse_results_keys_by_custom_id_any_order_and_surfaces_errors() {
        let body = r#"{"status":"ended","results":[
            {"custom_id":"b","text":"B-label"},
            {"custom_id":"a","text":"A-label"},
            {"custom_id":"c","error":"errored"}
        ]}"#;
        let results = parse_relay_results(body).unwrap();
        assert_eq!(results.len(), 3);
        let by_id = |id: &str| results.iter().find(|r| r.custom_id == id).unwrap();
        assert_eq!(by_id("a").text.as_deref(), Some("A-label"));
        assert_eq!(by_id("b").text.as_deref(), Some("B-label"));
        assert!(by_id("c").text.is_none());
        assert_eq!(by_id("c").error.as_deref(), Some("errored"));
    }

    #[test]
    fn parse_results_refuses_a_batch_that_has_not_ended() {
        let err = parse_relay_results(r#"{"status":"in_progress","completed":1,"total":2}"#);
        assert!(matches!(err, Err(LlmError::Provider(_))));
    }

    #[tokio::test]
    async fn submit_records_one_batch_relay_trace_per_item() {
        let c = client([HttpResponse { status: 202, body: r#"{"batch_id":"rb_9","accepted":2}"#.into() }]);
        let handle = c.submit(&items()).await.unwrap();
        assert_eq!(handle.id, "rb_9");
        let recs = c.sink.records();
        assert_eq!(recs.len(), 2);
        // §3.3: the relay route is distinguishable from the direct one.
        assert!(recs.iter().all(|r| r.route == Route::BatchRelay));
        assert!(recs.iter().all(|r| r.destination == "relay.shogun.app"));
        // Digest-only, as everywhere (G8).
        assert!(!format!("{:?}", recs[0]).contains("one"));
    }

    /// Same property the direct client tests pin down: a rejected credential must be
    /// distinguishable from a bad night (§4.5 — 401 falls back to the local lane, 5xx carries
    /// over).
    #[tokio::test]
    async fn a_rejected_license_is_distinguishable_from_an_outage() {
        for status in [401u16, 403] {
            let c = client([HttpResponse { status, body: "{}".into() }]);
            assert!(
                matches!(c.submit(&items()).await, Err(LlmError::Unauthorized(s, _)) if s == status),
                "HTTP {status} is a credential problem"
            );
        }
        let c = client([HttpResponse { status: 503, body: "{}".into() }]);
        assert!(matches!(c.submit(&items()).await, Err(LlmError::Provider(_))));
    }

    /// §4.5: 402 (plan does not cover the lane) and 429 (daily cap) are their own outcomes —
    /// the caller continues on the local lane rather than retrying or blaming the credential.
    #[tokio::test]
    async fn plan_and_cap_refusals_map_to_their_own_errors() {
        let c = client([HttpResponse { status: 402, body: "{}".into() }]);
        assert!(matches!(c.submit(&items()).await, Err(LlmError::QuotaExhausted(_))));
        let c = client([HttpResponse { status: 429, body: "daily cap".into() }]);
        assert!(matches!(c.submit(&items()).await, Err(LlmError::RateLimited(429, _))));
    }

    #[tokio::test]
    async fn poll_then_results_roundtrip_on_the_single_endpoint() {
        let c = client([
            HttpResponse { status: 200, body: r#"{"status":"ended"}"#.into() },
            HttpResponse {
                status: 200,
                body: r#"{"status":"ended","results":[{"custom_id":"1","text":"meeting"}]}"#.into(),
            },
        ]);
        let handle = c.poll("rb_1").await.unwrap();
        assert!(handle.status.is_ended());
        let results = c.results("rb_1").await.unwrap();
        assert_eq!(results[0].text.as_deref(), Some("meeting"));
        // poll/results do not write traceability (only submit does) — same as the direct client.
        assert!(c.sink.records().is_empty());
        // Both calls hit the one status endpoint.
        let sent = c.transport.sent();
        assert!(sent.iter().all(|r| r.url == "https://relay.shogun.app/v1/batch/rb_1"));
    }

    #[tokio::test]
    async fn run_submits_polls_until_ended_then_fetches_results() {
        let c = client([
            HttpResponse { status: 202, body: r#"{"batch_id":"rb_1","accepted":1}"#.into() },
            HttpResponse { status: 200, body: r#"{"status":"in_progress","completed":0,"total":1}"#.into() },
            HttpResponse { status: 200, body: r#"{"status":"ended"}"#.into() },
            HttpResponse {
                status: 200,
                body: r#"{"status":"ended","results":[{"custom_id":"1","text":"consolidated"}]}"#.into(),
            },
        ]);
        let one = vec![BatchItem {
            custom_id: "1".into(),
            purpose: "consolidation".into(),
            chunk: "today's events".into(),
        }];
        let results = c.run(&one, 5, || async {}).await.unwrap();
        assert_eq!(results[0].text.as_deref(), Some("consolidated"));
        assert_eq!(c.sink.records().len(), 1);
    }

    #[tokio::test]
    async fn run_errors_when_the_batch_never_ends_within_budget() {
        let c = client([
            HttpResponse { status: 202, body: r#"{"batch_id":"rb_1","accepted":1}"#.into() },
            HttpResponse { status: 200, body: r#"{"status":"in_progress","completed":0,"total":1}"#.into() },
            HttpResponse { status: 200, body: r#"{"status":"in_progress","completed":0,"total":1}"#.into() },
        ]);
        let one = vec![BatchItem { custom_id: "1".into(), purpose: "index".into(), chunk: "c".into() }];
        let err = c.run(&one, 2, || async {}).await.unwrap_err();
        assert!(matches!(err, LlmError::Provider(_)));
    }

    // Invariant-5 note: `RelayBatchClient::new` takes a `LicenseBearer` (the licence token).
    // Handing it a `ByokKey` OR a `SelectKkKey` (a raw Anthropic key) does not compile:
    //     RelayBatchClient::new(t, s, SelectKkKey::new(Secret::new("sk-ant-x")), cfg());
}
