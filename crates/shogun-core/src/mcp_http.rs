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

const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Parse JSON or Streamable-HTTP SSE MCP responses without exposing response content.
pub fn parse_mcp_response(content_type: Option<&str>, body: &str) -> Result<Value, String> {
    let envelope = if content_type.is_some_and(|v| v.to_ascii_lowercase().contains("text/event-stream")) {
        parse_sse_json_rpc(body)?
    } else {
        serde_json::from_str::<Value>(body).map_err(|_| "mcp response was not valid json".to_string())?
    };
    if let Some(error) = envelope.get("error") {
        let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
        return Err(format!("mcp json-rpc error {code}"));
    }
    let result = envelope.get("result").cloned().ok_or_else(|| "mcp response had no result".to_string())?;
    if result.get("isError").and_then(Value::as_bool).unwrap_or(false) {
        return Err("mcp tool call failed".to_string());
    }
    Ok(result)
}

fn parse_sse_json_rpc(body: &str) -> Result<Value, String> {
    let mut data = Vec::new();
    let mut candidates = Vec::new();
    let finish = |data: &mut Vec<String>, candidates: &mut Vec<Value>| {
        if data.is_empty() { return; }
        let joined = data.join("\n");
        data.clear();
        if let Ok(value) = serde_json::from_str::<Value>(&joined) {
            candidates.push(value);
        }
    };
    for raw in body.split_inclusive('\n') {
        let line = raw.strip_suffix('\n').unwrap_or(raw).strip_suffix('\r').unwrap_or(raw.strip_suffix('\n').unwrap_or(raw));
        if line.is_empty() {
            finish(&mut data, &mut candidates);
        } else if line.starts_with(':') {
            continue;
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value).to_string());
        }
    }
    finish(&mut data, &mut candidates);
    candidates.into_iter().find(|v| v.get("result").is_some() || v.get("error").is_some())
        .ok_or_else(|| "mcp response had no valid event".to_string())
}

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
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| format!("http client init failed: {e}"))?;
        Ok(Self { client, tokens })
    }
}

impl<P: TokenProvider> McpRpc for HttpMcpRpc<P> {
    fn call_tool(&self, service: Service, tool: &str, arguments: Value) -> Result<Value, String> {
        self.request(service, "tools/call", json!({ "name": tool, "arguments": arguments }))
    }

    fn list_tools(&self, service: Service) -> Result<Value, String> {
        self.request(service, "tools/list", json!({}))
    }
}

impl<P: TokenProvider> HttpMcpRpc<P> {
    fn request(&self, service: Service, method: &str, params: Value) -> Result<Value, String> {
        let endpoint = endpoints::endpoint(service)
            .ok_or_else(|| format!("{} has no official MCP endpoint", service.source_str()))?;
        let token = self.tokens.access_token(service)?;
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        let resp = self
            .client
            .post(endpoint.url)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/json, text/event-stream")
            .json(&body)
            .send()
            .map_err(|e| format!("mcp request failed: {}", redact(&e.to_string())))?;
        let status = resp.status();
        let content_type = resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(str::to_string);
        let text = resp.text().map_err(|e| format!("mcp read failed: {}", redact(&e.to_string())))?;
        if !status.is_success() {
            return Err(format!("mcp http {status}"));
        }
        parse_mcp_response(content_type.as_deref(), &text)
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
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
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

    #[test]
    fn parser_rejects_tool_level_error_without_body() {
        let err = parse_mcp_response(Some("application/json"), r#"{"jsonrpc":"2.0","result":{"isError":true,"content":[{"text":"SECRET"}]}}"#).unwrap_err();
        assert_eq!(err, "mcp tool call failed");
        assert!(!err.contains("SECRET"));
    }

    #[test]
    fn parser_accepts_sse_result_and_ignores_non_data_lines() {
        let value = parse_mcp_response(Some("text/event-stream"), ": keep-alive\nevent: message\ndata: {\"jsonrpc\":\"2.0\",\"result\":{\"ok\":true}}\n\n").unwrap();
        assert_eq!(value["ok"], true);
    }

    #[test]
    fn parser_joins_multiline_data_and_selects_result_event() {
        let value = parse_mcp_response(Some("text/event-stream"), "event: message\r\ndata: {\"jsonrpc\":\"2.0\",\r\ndata: \"result\":{\"ok\":true}}\r\n\r\n").unwrap();
        assert_eq!(value["ok"], true);
    }

    #[test]
    fn parser_skips_non_result_events_and_returns_content_free_rpc_error() {
        let body = "data: {\"jsonrpc\":\"2.0\",\"method\":\"notification\"}\n\n\ndata: {\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32001,\"message\":\"SECRET\"}}\n\n";
        let error = parse_mcp_response(Some("text/event-stream"), body).unwrap_err();
        assert_eq!(error, "mcp json-rpc error -32001");
        assert!(!error.contains("SECRET"));
    }
}
