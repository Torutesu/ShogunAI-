//! LLM client abstraction with compile-time key separation (WP3.1, CLAUDE.md invariant 5 / G5).
//!
//! Two model-access lanes must never be crossed:
//! - **Batch lane** — indexing, classification, Dream Cycle, Morning Brief — uses the **Select
//!   KK** key over the Batch API.
//! - **Agent lane** — agent inference, chat, drafts — uses the **user's BYOK** key over the
//!   Messages API.
//!
//! Invariant 5 ("keys must not be swapped") is enforced *in the type system*: the two keys are
//! distinct newtypes and each client lane accepts only its own key. A function that wants a
//! [`SelectKkKey`] cannot be handed a [`ByokKey`] — it does not compile. There is no runtime
//! check to forget.
//!
//! Secrets never reach a log: [`Secret`] redacts under `Debug`/`Display`, and the raw value is
//! only reachable through [`Secret::expose`] (NFR-SEC-01/02, G7).
//!
//! ## Layers
//! - This file defines the **lanes** ([`SelectKkKey`]/[`ByokKey`], [`BatchClient`]/[`AgentClient`])
//!   and the offline **mocks**.
//! - [`transport`] is the HTTP seam ([`HttpTransport`]) so the network clients are testable with
//!   no socket; [`traceability`] is the send-log seam ([`TraceabilitySink`]) that records only a
//!   digest + byte-length of every outbound chunk (AR-11 / G8, never the text).
//! - [`anthropic`] is the real Anthropic REST layer: pure request builders + response parsers
//!   (Linux-testable) plus thin async clients that wire transport + sink together. The
//!   Batch-lane client takes a [`SelectKkKey`] and the Agent-lane client a [`ByokKey`], so
//!   invariant 5 stays compile-enforced end-to-end.

pub mod anthropic;
pub mod openai_compat;
pub mod traceability;
pub mod transport;

use std::fmt;

/// A secret string that never appears in logs. `Debug` and `Display` render a fixed redaction;
/// the real value is only reachable via [`Secret::expose`], which callers must go out of their
/// way to use (and never log).
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    /// The raw secret. The single choke point for reading it — never pass the result to a log,
    /// a record, or telemetry (NFR-SEC-01/02).
    pub fn expose(&self) -> &str {
        &self.0
    }
    /// Last-4 for a UI echo (NFR-SEC-02: BYOK read-back shows only the last four chars).
    pub fn last4(&self) -> String {
        let n = self.0.chars().count();
        self.0.chars().skip(n.saturating_sub(4)).collect()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***redacted***)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***redacted***")
    }
}

/// The Select KK key — the Batch lane only (indexing / classification / Dream Cycle / Morning
/// Brief). A distinct type from [`ByokKey`] so the two can never be interchanged (invariant 5).
#[derive(Clone, Debug)]
pub struct SelectKkKey(Secret);

impl SelectKkKey {
    pub fn new(secret: Secret) -> Self {
        Self(secret)
    }
    pub fn secret(&self) -> &Secret {
        &self.0
    }
}

/// The user's BYOK key — the Agent lane only (agent inference / chat / drafts). Distinct from
/// [`SelectKkKey`] (invariant 5).
#[derive(Clone, Debug)]
pub struct ByokKey(Secret);

impl ByokKey {
    pub fn new(secret: Secret) -> Self {
        Self(secret)
    }
    pub fn secret(&self) -> &Secret {
        &self.0
    }
}

/// What a lane is allowed to do — used for traceability tagging (AR-11) and to document intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// Batch API with the Select KK key.
    Batch,
    /// Messages API with the user's BYOK key.
    Agent,
}

/// A batch request (classification / summarisation over a processed chunk). Payloads carry only
/// the chunk to process, never a whole event-log row (AR-12).
#[derive(Debug, Clone)]
pub struct BatchRequest {
    pub purpose: &'static str,
    pub chunk: String,
}

/// The Batch lane client. Constructed with a [`SelectKkKey`] — an implementation cannot be
/// built from a [`ByokKey`], which is the compile-time half of invariant 5.
pub trait BatchClient: Send + Sync {
    fn lane(&self) -> Lane {
        Lane::Batch
    }
    /// Run a batch job. Async in the real (Anthropic Batch API) impl.
    fn classify(&self, req: &BatchRequest) -> Result<String, LlmError>;
}

/// The Agent lane client. Constructed with a [`ByokKey`].
pub trait AgentClient: Send + Sync {
    fn lane(&self) -> Lane {
        Lane::Agent
    }
    /// Produce a draft/response. The real impl streams (SLO-03); this synchronous signature is
    /// the abstraction seam.
    fn complete(&self, prompt: &str) -> Result<String, LlmError>;
}

/// LLM errors.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("provider error: {0}")]
    Provider(String),
    /// The credential itself was rejected (HTTP 401/403). Distinct from [`Provider`] because the
    /// caller's response has to be different: retrying a rejected key tonight, tomorrow night and
    /// the night after is not resilience, it is a silent outage. Callers fall back and say so.
    #[error("credential rejected (HTTP {0})")]
    Unauthorized(u16),
    #[error("not configured (missing key)")]
    NotConfigured,
    #[error("transport: {0}")]
    Transport(#[from] transport::TransportError),
    #[error("malformed response: {0}")]
    Parse(String),
}

/// Turn a failed HTTP status into the right error. 401/403 means the credential is wrong, not that
/// the provider is having a bad moment, and callers have to act differently: a rejected key is
/// worth telling the user about and pointless to retry, while a 5xx is the opposite.
pub fn status_error(step: &str, status: u16) -> LlmError {
    match status {
        401 | 403 => LlmError::Unauthorized(status),
        _ => LlmError::Provider(format!("{step} HTTP {status}")),
    }
}

// ---- mocks (tests / offline) -------------------------------------------------------------

/// A Batch client that echoes a canned label — for tests and offline development. Holds a
/// SelectKkKey, so constructing it with a ByokKey does not compile.
pub struct MockBatchClient {
    _key: SelectKkKey,
    label: String,
}

impl MockBatchClient {
    pub fn new(key: SelectKkKey, label: impl Into<String>) -> Self {
        Self { _key: key, label: label.into() }
    }
}

impl BatchClient for MockBatchClient {
    fn classify(&self, _req: &BatchRequest) -> Result<String, LlmError> {
        Ok(self.label.clone())
    }
}

/// An Agent client that echoes the prompt — for tests and offline development. Holds a ByokKey.
pub struct MockAgentClient {
    _key: ByokKey,
}

impl MockAgentClient {
    pub fn new(key: ByokKey) -> Self {
        Self { _key: key }
    }
}

impl AgentClient for MockAgentClient {
    fn complete(&self, prompt: &str) -> Result<String, LlmError> {
        Ok(format!("draft: {prompt}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_is_redacted_in_debug_and_display() {
        let s = Secret::new("sk-super-secret-value");
        assert_eq!(format!("{s:?}"), "Secret(***redacted***)");
        assert_eq!(format!("{s}"), "***redacted***");
        // The raw value is only reachable via expose().
        assert_eq!(s.expose(), "sk-super-secret-value");
    }

    #[test]
    fn key_wrappers_do_not_leak_the_secret_in_debug() {
        let kk = SelectKkKey::new(Secret::new("kk-key-123456"));
        let byok = ByokKey::new(Secret::new("byok-key-abcdef"));
        assert!(!format!("{kk:?}").contains("123456"));
        assert!(!format!("{byok:?}").contains("abcdef"));
    }

    #[test]
    fn last4_for_ui_readback() {
        let s = Secret::new("abcd1234wxyz");
        assert_eq!(s.last4(), "wxyz");
        assert_eq!(Secret::new("ab").last4(), "ab");
    }

    #[test]
    fn batch_and_agent_lanes_are_tagged_distinctly() {
        let batch = MockBatchClient::new(SelectKkKey::new(Secret::new("kk")), "meeting");
        let agent = MockAgentClient::new(ByokKey::new(Secret::new("byok")));
        assert_eq!(batch.lane(), Lane::Batch);
        assert_eq!(agent.lane(), Lane::Agent);
    }

    #[test]
    fn mock_clients_route_through_their_lane() {
        let batch = MockBatchClient::new(SelectKkKey::new(Secret::new("kk")), "label-x");
        let out = batch.classify(&BatchRequest { purpose: "classify", chunk: "hello".into() }).unwrap();
        assert_eq!(out, "label-x");

        let agent = MockAgentClient::new(ByokKey::new(Secret::new("byok")));
        assert_eq!(agent.complete("hi").unwrap(), "draft: hi");
    }

    // Compile-time invariant-5 note: `MockBatchClient::new` takes `SelectKkKey` and
    // `MockAgentClient::new` takes `ByokKey`. Passing the wrong key type is a type error, so a
    // BYOK key can never reach the Batch lane (or vice versa). The following, if uncommented,
    // must NOT compile:
    //     MockBatchClient::new(ByokKey::new(Secret::new("x")), "l");
    //     MockAgentClient::new(SelectKkKey::new(Secret::new("x")));
}
