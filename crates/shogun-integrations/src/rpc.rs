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

    /// Return one stateless `tools/list` result. Providers that cannot probe capabilities must
    /// fail closed instead of allowing writes based on provisional names.
    fn list_tools(&self, _service: Service) -> Result<Value, String> {
        Err("tools/list capability probe unavailable".to_string())
    }
}

/// Dispatch Gmail through Composio and released official providers through direct MCP.
pub struct DispatchRpc<C, O> {
    pub composio: C,
    pub official: O,
}

impl<C, O> DispatchRpc<C, O> {
    pub fn new(composio: C, official: O) -> Self {
        Self { composio, official }
    }
}

impl<C: McpRpc, O: McpRpc> McpRpc for DispatchRpc<C, O> {
    fn call_tool(&self, service: Service, tool: &str, arguments: Value) -> Result<Value, String> {
        if service == Service::Gmail {
            self.composio.call_tool(service, tool, arguments)
        } else if service.is_released(shogun_mcp::scope::Wave::One) {
            self.official.call_tool(service, tool, arguments)
        } else {
            Err(format!("{} is unreleased at Wave One", service.source_str()))
        }
    }

    fn list_tools(&self, service: Service) -> Result<Value, String> {
        if service == Service::Gmail {
            self.composio.list_tools(service)
        } else if service.is_released(shogun_mcp::scope::Wave::One) {
            self.official.list_tools(service)
        } else {
            Err(format!("{} is unreleased at Wave One", service.source_str()))
        }
    }
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

    struct RecordingRpc(std::cell::RefCell<Vec<Service>>);
    impl McpRpc for RecordingRpc {
        fn call_tool(&self, service: Service, _tool: &str, _arguments: Value) -> Result<Value, String> {
            self.0.borrow_mut().push(service);
            Ok(Value::Null)
        }
    }

    #[test]
    fn dispatch_keeps_gmail_on_composio_and_calendar_on_official() {
        let rpc = DispatchRpc::new(RecordingRpc(std::cell::RefCell::new(Vec::new())), RecordingRpc(std::cell::RefCell::new(Vec::new())));
        rpc.call_tool(Service::Gmail, "search_threads", Value::Null).unwrap();
        rpc.call_tool(Service::GoogleCalendar, "create_event", Value::Null).unwrap();
        assert_eq!(rpc.composio.0.borrow().as_slice(), &[Service::Gmail]);
        assert_eq!(rpc.official.0.borrow().as_slice(), &[Service::GoogleCalendar]);
        assert!(rpc.call_tool(Service::Slack, "send_message", Value::Null).is_err());
    }

    #[test]
    fn unreleased_dispatch_refuses_slack_github_notion_linear() {
        let rpc = DispatchRpc::new(RecordingRpc(std::cell::RefCell::new(Vec::new())), RecordingRpc(std::cell::RefCell::new(Vec::new())));
        for service in [Service::Slack, Service::GitHub, Service::Notion, Service::Linear] {
            let error = rpc.call_tool(service, "provisional", Value::Null).unwrap_err();
            assert!(error.contains("unreleased"));
        }
        assert!(rpc.composio.0.borrow().is_empty());
        assert!(rpc.official.0.borrow().is_empty());
    }
}
