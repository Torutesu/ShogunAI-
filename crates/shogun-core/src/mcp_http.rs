//! The concrete HTTPS clients for first-layer connectors (feature `net`).
//!
//! These live in shogun-core because it is the single crate allowlisted to hold an HTTP client
//! (FR-TR-03 / invariant 3: external egress goes through one place). shogun-integrations stays
//! HTTP-client-free — it defines the seams ([`shogun_integrations::McpRpc`],
//! [`shogun_integrations::oauth::TokenExchange`]) and the pure logic; this module supplies the
//! reqwest implementations the daemon wires in.
//!
//! Two blocking clients (the seams are synchronous):
//! - [`HttpMcpRpc`] — MCP `tools/call` over the Streamable-HTTP transport (JSON-RPC 2.0, Bearer).
//! - [`HttpTokenExchange`] — the OAuth token endpoint form POST.
//!
//! Errors are content-free (a request/response body can echo a token or user data, so it is never
//! surfaced or logged).

use serde_json::{json, Value};
use shogun_integrations::endpoints;
use shogun_integrations::oauth::TokenExchange;
use shogun_integrations::rpc::{McpRpc, TokenProvider};
use shogun_mcp::scope::Service;

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

        // JSON-RPC 2.0 tools/call over the MCP Streamable-HTTP transport. One request per call.
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

/// A blocking `reqwest` OAuth token exchange (implements [`shogun_integrations::oauth::TokenExchange`]).
pub struct HttpTokenExchange {
    client: reqwest::blocking::Client,
}

impl HttpTokenExchange {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("shogun/1.0")
            .build()
            .map_err(|e| format!("http client init failed: {e}"))?;
        Ok(Self { client })
    }
}

impl TokenExchange for HttpTokenExchange {
    fn post_form(&self, token_endpoint: &str, form: &[(String, String)]) -> Result<String, String> {
        let resp = self
            .client
            .post(token_endpoint)
            .form(form)
            .send()
            .map_err(|e| format!("token exchange failed: {}", redact(&e.to_string())))?;
        let status = resp.status();
        let body = resp.text().map_err(|e| format!("token read failed: {}", redact(&e.to_string())))?;
        if !status.is_success() {
            // Status only — the body can echo the code/verifier, so it is not surfaced.
            return Err(format!("token endpoint http {status}"));
        }
        Ok(body)
    }
}

/// Strip anything after a `?` from an error string before returning it for logging (defense in
/// depth — reqwest errors can embed the URL with a query).
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
    fn redact_strips_query_string() {
        assert_eq!(redact("https://x.example/mcp/v1?access_token=SECRET"), "https://x.example/mcp/v1…");
        assert_eq!(redact("plain error"), "plain error");
    }
}
