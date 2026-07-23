//! The effectful transport (feature `live`): a blocking HTTPS JSON-RPC client for the MCP
//! Streamable-HTTP transport, plus the macOS Keychain token source.
//!
//! This is the ONLY module that touches the network or secrets. It is off by default so the pure
//! CI (endpoints / toolmap / result / transport-over-fake) never links a TLS stack — mirroring
//! shogun-core's `net` feature. It compiles under `--features live` on any target; the macOS
//! Keychain provider is additionally `#[cfg(target_os = "macos")]`.
//!
//! NOTE: the actual request/response round-trip cannot be exercised on Linux CI (it needs live
//! Google OAuth tokens and network). The request construction below follows the MCP Streamable-HTTP
//! spec (JSON-RPC 2.0 `tools/call` over POST with a Bearer token); confirm the per-tool argument
//! and result schemas against live responses when wiring this into the daemon on macOS.

use serde_json::{json, Value};
use shogun_mcp::scope::Service;

use crate::endpoints;
use crate::rpc::{McpRpc, TokenProvider};

/// A blocking HTTPS client that speaks MCP `tools/call` to Google's official remote MCP servers.
pub struct HttpMcpRpc<P: TokenProvider> {
    client: reqwest::blocking::Client,
    tokens: P,
}

impl<P: TokenProvider> HttpMcpRpc<P> {
    /// Build the client with a token source. Fails only if the TLS backend cannot initialize.
    pub fn new(tokens: P) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("shogun/1.0")
            .build()
            .map_err(|e| format!("http client init failed: {e}"))?;
        Ok(Self { client, tokens })
    }
}

impl<P: TokenProvider> McpRpc for HttpMcpRpc<P> {
    fn call_tool(&self, service: Service, tool: &str, arguments: Value) -> Result<Value, String> {
        let endpoint = endpoints::endpoint(service)
            .ok_or_else(|| format!("{} has no official MCP endpoint", service.source_str()))?;
        let token = self.tokens.access_token(service)?;

        // JSON-RPC 2.0 tools/call over the MCP Streamable-HTTP transport. `id` is fixed at 1: one
        // request per call, no batching.
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": tool, "arguments": arguments }
        });

        let resp = self
            .client
            .post(endpoint.url)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/json, text/event-stream")
            .json(&body)
            .send()
            .map_err(|e| format!("mcp request failed: {}", redact(&e.to_string())))?;

        let status = resp.status();
        let text = resp.text().map_err(|e| format!("mcp read failed: {}", redact(&e.to_string())))?;
        if !status.is_success() {
            // Status only — the body may echo request content, so it is not surfaced.
            return Err(format!("mcp http {status}"));
        }
        let envelope: Value =
            serde_json::from_str(&text).map_err(|_| "mcp response was not valid json".to_string())?;
        // A JSON-RPC error is a protocol failure (distinct from a tool `isError`, handled in result).
        if let Some(err) = envelope.get("error") {
            let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
            return Err(format!("mcp json-rpc error {code}"));
        }
        envelope
            .get("result")
            .cloned()
            .ok_or_else(|| "mcp response had no result".to_string())
    }
}

/// Strip anything that could carry a token/URL query from an error string before it is returned for
/// logging (defense in depth — reqwest errors can embed the URL).
fn redact(msg: &str) -> String {
    match msg.find('?') {
        Some(i) => format!("{}…", &msg[..i]),
        None => msg.to_string(),
    }
}

/// The macOS Keychain token source (invariant 7). One generic-password entry per service under the
/// SHOGUN service name; the account is the service's `source_str` (e.g. `gmail`). OAuth refresh is
/// the connection layer's job — this only reads the current access token.
#[cfg(target_os = "macos")]
pub struct KeychainTokenProvider {
    keychain_service: String,
}

#[cfg(target_os = "macos")]
impl KeychainTokenProvider {
    /// `keychain_service` is the Keychain "service" field, e.g. `com.selectkk.shogun`.
    pub fn new(keychain_service: impl Into<String>) -> Self {
        Self { keychain_service: keychain_service.into() }
    }
}

#[cfg(target_os = "macos")]
impl TokenProvider for KeychainTokenProvider {
    fn access_token(&self, service: Service) -> Result<String, String> {
        // Account key format: "<source>-access" (e.g. "gmail-access").
        let account = format!("{}-access", service.source_str());
        let bytes =
            security_framework::passwords::get_generic_password(&self.keychain_service, &account)
                .map_err(|_| format!("no keychain token for {}", service.source_str()))?;
        String::from_utf8(bytes).map_err(|_| "keychain token was not utf-8".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_strips_query_string() {
        assert_eq!(redact("https://x.example/mcp/v1?access_token=SECRET"), "https://x.example/mcp/v1…");
        assert_eq!(redact("plain error"), "plain error");
    }
}
