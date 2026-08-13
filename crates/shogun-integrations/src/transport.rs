//! [`RemoteMcpTransport`] — the first-layer connector adapter.
//!
//! It composes the pure pieces into the effect the daemon needs: given a service, pick its Google
//! MCP tool ([`crate::toolmap`]), call it over the [`McpRpc`] seam, and normalize the reply into
//! [`FetchedItem`]s ([`crate::result`]). It implements [`shogun_mcp::sync::IntegrationTransport`], so
//! it drops straight into the existing gate → fetch → normalize ingest ([`shogun_mcp::sync`]).
//!
//! Policy is **not** re-decided here: the daemon runs [`shogun_mcp::service_gate::authorize_op`]
//! before calling us (reads via [`shogun_mcp::sync::collect_sync`], writes via [`Self::execute`]).
//! This module only maps ops to tools and moves bytes.

use serde_json::{json, Value};
use shogun_mcp::scope::Service;
use shogun_mcp::sync::{FetchedItem, IntegrationTransport};

use crate::rpc::McpRpc;
use crate::runtime::CapabilityProbe;
use crate::toolmap;

/// A first-layer transport backed by an [`McpRpc`] (a fake in tests, the live HTTPS client in
/// production).
pub struct RemoteMcpTransport<R: McpRpc> {
    rpc: R,
    /// How many recent items a background read-sync requests per service.
    page_size: u32,
}

impl<R: McpRpc> RemoteMcpTransport<R> {
    pub fn new(rpc: R) -> Self {
        Self { rpc, page_size: 25 }
    }

    /// Set the read-sync page size (default 25).
    pub fn with_page_size(mut self, n: u32) -> Self {
        self.page_size = n;
        self
    }

    pub fn validate_capabilities(&self, service: Service) -> Result<(), String> {
        let result = self.rpc.list_tools(service)?;
        toolmap::validate_write_capabilities(service, &result)
    }

    /// Execute a first-layer **write** op that the daemon has already authorized (an L2 draft or an
    /// L3 create, post-confirmation). Maps the scope op to its Google MCP tool and calls it with
    /// `arguments`. Returns the raw `CallToolResult` for the caller to record/inspect. An op with no
    /// Google MCP tool (e.g. Gmail `send`, which is Composio-only) is refused here rather than
    /// mis-routed.
    pub fn execute(&self, service: Service, op_name: &str, arguments: Value) -> Result<Value, String> {
        let tool = toolmap::tool_for(service, op_name)
            .ok_or_else(|| format!("{}::{op_name} has no Google MCP tool", service.source_str()))?;
        toolmap::validate_write_arguments(service, op_name, &arguments)?;
        self.rpc.call_tool(service, tool, arguments)
    }
}

impl<R: McpRpc> CapabilityProbe for RemoteMcpTransport<R> {
    fn validate_capabilities(&self, service: Service) -> Result<(), String> {
        RemoteMcpTransport::validate_capabilities(self, service)
    }
}

/// The write half of the transport, as a seam so the runtime's confirmed-write path
/// ([`crate::runtime::ConnectorRuntime::execute_write`]) is testable without a live RPC.
pub trait WriteExecutor {
    /// Execute an already-authorized, already-confirmed first-layer write op. Maps the scope op to
    /// its Google MCP tool and calls it.
    fn execute(&self, service: Service, op_name: &str, arguments: Value) -> Result<Value, String>;
}

impl<R: McpRpc> WriteExecutor for RemoteMcpTransport<R> {
    fn execute(&self, service: Service, op_name: &str, arguments: Value) -> Result<Value, String> {
        RemoteMcpTransport::execute(self, service, op_name, arguments)
    }
}

impl<R: McpRpc> IntegrationTransport for RemoteMcpTransport<R> {
    fn read_sync(&self, service: Service) -> Result<Vec<FetchedItem>, String> {
        let tool = toolmap::read_sync_tool(service)
            .ok_or_else(|| format!("{} has no read_sync MCP tool", service.source_str()))?;
        // A modest recent-window request; the exact arg schema per Google server is confirmed at
        // wire-up time — unknown args are ignored by the server, so a conservative page size is safe.
        let args = json!({ "max_results": self.page_size });
        let result = self.rpc.call_tool(service, tool, args)?;
        crate::result::parse_items(&result)
    }

    fn fetch_on_demand(&self, service: Service, query: &str) -> Result<Vec<FetchedItem>, String> {
        let tool = toolmap::tool_for(service, "read_on_demand")
            .ok_or_else(|| format!("{} has no read_on_demand MCP tool", service.source_str()))?;
        // `id` carries the thread/file id (or search string). The exact per-tool arg name
        // (thread_id / file_id / query) is confirmed against live tools/list at wire-up — same
        // caveat as read_sync's page arg.
        let args = json!({ "id": query });
        let result = self.rpc.call_tool(service, tool, args)?;
        crate::result::parse_items(&result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Records the last call and returns a canned result (or error).
    struct FakeRpc {
        last: RefCell<Option<(Service, String)>>,
        reply: Result<Value, String>,
    }
    impl FakeRpc {
        fn ok(reply: Value) -> Self {
            Self { last: RefCell::new(None), reply: Ok(reply) }
        }
        fn err(msg: &str) -> Self {
            Self { last: RefCell::new(None), reply: Err(msg.to_string()) }
        }
    }
    impl McpRpc for FakeRpc {
        fn call_tool(&self, service: Service, tool: &str, _args: Value) -> Result<Value, String> {
            *self.last.borrow_mut() = Some((service, tool.to_string()));
            self.reply.clone()
        }
    }

    #[test]
    fn read_sync_calls_the_mapped_tool_and_normalizes() {
        let reply = json!({
            "structuredContent": [ { "id": "m1", "subject": "Hi", "snippet": "hello", "internalDate": 5 } ]
        });
        let rpc = FakeRpc::ok(reply);
        let t = RemoteMcpTransport::new(rpc);
        let items = t.read_sync(Service::Gmail).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].body, "hello");
        // it dispatched to Gmail's read_sync tool
        assert_eq!(t.rpc.last.borrow().as_ref().unwrap(), &(Service::Gmail, "search_threads".to_string()));
    }

    #[test]
    fn transport_error_propagates() {
        let t = RemoteMcpTransport::new(FakeRpc::err("network down"));
        assert_eq!(t.read_sync(Service::GoogleCalendar).unwrap_err(), "network down");
    }

    #[test]
    fn fetch_on_demand_calls_the_read_on_demand_tool() {
        let reply = json!({ "structuredContent": [ { "id": "t", "subject": "s", "body": "thread text" } ] });
        let t = RemoteMcpTransport::new(FakeRpc::ok(reply));
        let items = t.fetch_on_demand(Service::Gmail, "thread-9").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].body, "thread text");
        assert_eq!(t.rpc.last.borrow().as_ref().unwrap(), &(Service::Gmail, "get_thread".to_string()));
    }

    #[test]
    fn fetch_on_demand_errors_when_service_has_no_such_tool() {
        let t = RemoteMcpTransport::new(FakeRpc::ok(json!({})));
        assert!(t.fetch_on_demand(Service::GoogleCalendar, "x").is_err());
        assert!(t.rpc.last.borrow().is_none());
    }

    #[test]
    fn execute_maps_write_op_to_tool() {
        let t = RemoteMcpTransport::new(FakeRpc::ok(json!({ "isError": false, "content": [] })));
        t.execute(
            Service::GoogleCalendar,
            "event_create",
            json!({ "summary": "Sync", "startTime": "2026-08-13T10:00:00Z", "endTime": "2026-08-13T11:00:00Z" }),
        )
        .unwrap();
        assert_eq!(
            t.rpc.last.borrow().as_ref().unwrap(),
            &(Service::GoogleCalendar, "create_event".to_string())
        );
    }

    #[test]
    fn execute_refuses_gmail_send_composio_only() {
        // send has no MCP tool — it must be refused here, never mis-routed to another tool.
        let t = RemoteMcpTransport::new(FakeRpc::ok(json!({})));
        let err = t.execute(Service::Gmail, "send", json!({})).unwrap_err();
        assert!(err.contains("no Google MCP tool"));
        assert!(t.rpc.last.borrow().is_none());
    }

    #[test]
    fn calendar_create_refuses_incomplete_action_args_before_network() {
        let t = RemoteMcpTransport::new(FakeRpc::ok(json!({})));
        let err = t.execute(Service::GoogleCalendar, "event_create", json!({"summary":"Sync"})).unwrap_err();
        assert_eq!(err, "GoogleCalendar event_create requires startTime");
        assert!(t.rpc.last.borrow().is_none());
    }
}
