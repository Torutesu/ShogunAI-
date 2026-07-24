//! The Composio HTTP client (feature `net`) — the reqwest half of the second-layer Gmail send
//! (WP-D, §6.10 / FR-C2-01).
//!
//! Composio is a third party; per FR-TR-03 its HTTP client lives here in shogun-core (the single
//! allowlisted egress). The *transport* that turns a confirmed email send into a `GMAIL_SEND_EMAIL`
//! execution is pure over the [`ComposioApi`](shogun_integrations::composio::ComposioApi) seam and
//! lives in [`crate::send_exec`] (so it builds under `daemon-server` without a TLS stack). This
//! module is only the effectful client the daemon injects.

use serde_json::{json, Value};
use shogun_integrations::composio::ComposioApi;

/// A blocking `reqwest` implementation of the Composio execute API
/// (`POST /api/v3/tools/execute/{tool_slug}`, `x-api-key` auth).
pub struct HttpComposioApi {
    client: reqwest::blocking::Client,
    api_key: String,
    base_url: String,
}

impl HttpComposioApi {
    /// `api_key` is the Composio project key (read from the Keychain by the caller — invariant 7).
    pub fn new(api_key: impl Into<String>) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("shogun/1.0")
            .build()
            .map_err(|e| format!("http client init failed: {e}"))?;
        Ok(Self {
            client,
            api_key: api_key.into(),
            base_url: "https://backend.composio.dev/api/v3".to_string(),
        })
    }
}

impl ComposioApi for HttpComposioApi {
    fn execute(&self, tool: &str, user_id: &str, arguments: Value) -> Result<Value, String> {
        let url = format!("{}/tools/execute/{tool}", self.base_url);
        let body = json!({ "user_id": user_id, "arguments": arguments });
        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .json(&body)
            .send()
            .map_err(|e| format!("composio request failed: {}", redact(&e.to_string())))?;
        let status = resp.status();
        let text = resp.text().map_err(|e| format!("composio read failed: {}", redact(&e.to_string())))?;
        if !status.is_success() {
            // Status only — the body can echo the message, so it is never surfaced.
            return Err(format!("composio http {status}"));
        }
        serde_json::from_str(&text).map_err(|_| "composio response was not valid json".to_string())
    }
}

/// Strip anything after a `?` from an error string before returning it for logging.
fn redact(msg: &str) -> String {
    match msg.find('?') {
        Some(i) => format!("{}…", &msg[..i]),
        None => msg.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_strips_query() {
        assert_eq!(redact("https://x/api/v3/tools/execute/T?u=SECRET"), "https://x/api/v3/tools/execute/T…");
        assert_eq!(redact("plain"), "plain");
    }
}
