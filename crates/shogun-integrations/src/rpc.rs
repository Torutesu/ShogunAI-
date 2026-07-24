//! The transport seam: an [`McpRpc`] makes one MCP `tools/call` and returns the JSON-RPC `result`.
//!
//! Keeping this a trait lets the whole adapter composition ([`crate::transport`]) be tested on Linux
//! with a fake, while the real HTTPS client ([`crate::live`], feature `live`) is the only effectful
//! piece. Access tokens come from a separate [`TokenProvider`] so the network client never knows
//! where secrets live (Keychain on macOS; env/static for dev + integration).

use serde_json::Value;
use shogun_mcp::scope::Service;

/// Performs a single MCP tool call against a service's remote MCP server.
pub trait McpRpc {
    /// Call `tool` with `arguments`, returning the JSON-RPC `result` object (the `CallToolResult`),
    /// or a short, content-free error reason.
    fn call_tool(&self, service: Service, tool: &str, arguments: Value) -> Result<Value, String>;
}

/// Supplies the OAuth access token for a service. The real macOS implementation reads the Keychain
/// (invariant 7); dev/integration can use [`StaticTokenProvider`].
pub trait TokenProvider {
    /// The current access token for `service`, or a reason it is unavailable (drives the amber /
    /// needs-reauth state upstream).
    fn access_token(&self, service: Service) -> Result<String, String>;
}

/// A fixed token, for dev and integration tests. Never used in production (production reads the
/// Keychain). Holds a single token used for every service.
#[derive(Debug, Clone)]
pub struct StaticTokenProvider {
    token: String,
}

impl StaticTokenProvider {
    pub fn new(token: impl Into<String>) -> Self {
        Self { token: token.into() }
    }
}

impl TokenProvider for StaticTokenProvider {
    fn access_token(&self, _service: Service) -> Result<String, String> {
        if self.token.is_empty() {
            return Err("no access token configured".to_string());
        }
        Ok(self.token.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_provider_returns_token_or_errors_when_empty() {
        assert_eq!(StaticTokenProvider::new("abc").access_token(Service::Gmail).as_deref(), Ok("abc"));
        assert!(StaticTokenProvider::new("").access_token(Service::Gmail).is_err());
    }
}
