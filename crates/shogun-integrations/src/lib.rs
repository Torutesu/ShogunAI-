//! SHOGUN first-layer connector adapter — the effectful side of §6.9 that turns the pure scope
//! table ([`shogun_mcp`]) into real reads/writes against Google Workspace's **official** remote MCP
//! servers (Gmail / Calendar / Drive), direct user→Google with no third party in the data path.
//!
//! Layering (the pure/effect split used across the workspace):
//! - `endpoints` — Service → Google MCP URL + least-privilege OAuth scopes (pure data).
//! - `toolmap` — our scope-op name → Google MCP tool name (pure data).
//! - `result` — MCP `tools/call` reply → normalized `FetchedItem` (pure).
//! - `rpc` — the `McpRpc` / `TokenProvider` seams (pure traits).
//! - `transport` — `RemoteMcpTransport` implements `IntegrationTransport`, composing the above.
//! - `live` — feature `live`: the blocking HTTPS JSON-RPC client + macOS Keychain (only effectful module).
//!
//! Scope notes (product decision 2026-07-23):
//! - Google Docs / Sheets have no dedicated official MCP server — their content is read via Drive's `read_file_content`, and there is no first-layer write path for them.
//! - Gmail send is intentionally unreachable here (no `send` tool, no `gmail.send` scope) — sending is the second layer (Composio, §6.10); first-layer Gmail reads and drafts only.
//! - Policy is not decided in this crate — the daemon gates every op through `service_gate::authorize_op` before calling the transport.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod endpoints;
pub mod oauth;
pub mod result;
pub mod rpc;
pub mod runtime;
pub mod token;
pub mod toolmap;
pub mod transport;

#[cfg(feature = "live")]
pub mod live;
#[cfg(feature = "live")]
pub mod oauth_flow;

pub use oauth::{AuthConfig, Pkce, TokenExchange, TokenSet};
pub use rpc::{McpRpc, StaticTokenProvider, TokenProvider};
pub use runtime::{ConnUi, ConnectorRuntime, IngestSink, ServiceStatus, SyncReport};
pub use token::{MemoryTokenStore, TokenError, TokenManager, TokenStore};
pub use transport::{RemoteMcpTransport, WriteExecutor};
