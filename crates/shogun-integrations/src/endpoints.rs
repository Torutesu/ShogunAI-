//! Service → Google Workspace official remote MCP endpoint + OAuth scopes.
//!
//! First-layer connections go **directly** to Google's own first-party MCP servers (§6.9,
//! FR-INT-01/02): user→Google OAuth, no third party in the data path (unlike Composio, the second
//! layer). Only the services Google actually ships a remote MCP server for are mappable here.
//!
//! Coverage (Google Workspace Developer Preview, verified 2026-07):
//! - Gmail   → `gmailmcp.googleapis.com`
//! - Calendar→ `calendarmcp.googleapis.com`
//! - Drive → `drivemcp.googleapis.com` (also the read path for Google Docs/Sheets content, which have no dedicated MCP server of their own)
//!
//! Slack / Notion / GitHub / Linear are later waves and are not Google endpoints — [`endpoint`]
//! returns `None` for them (and for any service without an official remote MCP server).

use shogun_mcp::scope::Service;

/// An official remote MCP endpoint and the OAuth scopes needed for the operations we call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McpEndpoint {
    /// The MCP Streamable-HTTP JSON-RPC URL.
    pub url: &'static str,
    /// The minimal OAuth scopes for this service's read + first-layer-write operations. Requested
    /// least-privilege (FR-INT-05). Gmail deliberately has no `send` scope — sending is the second
    /// layer (Composio), so the first-layer connection can never send.
    pub scopes: &'static [&'static str],
}

const GMAIL: McpEndpoint = McpEndpoint {
    url: "https://gmailmcp.googleapis.com/mcp/v1",
    scopes: &[
        "https://www.googleapis.com/auth/gmail.readonly",
        // compose = create/update drafts (L2). NOT gmail.send — send is Composio-only (§6.10).
        "https://www.googleapis.com/auth/gmail.compose",
    ],
};

const CALENDAR: McpEndpoint = McpEndpoint {
    url: "https://calendarmcp.googleapis.com/mcp/v1",
    scopes: &[
        "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
        "https://www.googleapis.com/auth/calendar.events.readonly",
        "https://www.googleapis.com/auth/calendar.events.freebusy",
        // writable events scope — required for the L3 create/update operations.
        "https://www.googleapis.com/auth/calendar.events",
    ],
};

const DRIVE: McpEndpoint = McpEndpoint {
    url: "https://drivemcp.googleapis.com/mcp/v1",
    scopes: &[
        "https://www.googleapis.com/auth/drive.readonly",
        // per-file access created by the app — the least-privilege write scope for file_create.
        "https://www.googleapis.com/auth/drive.file",
    ],
};

/// The official remote MCP endpoint for a service, if Google ships one. `None` means the service is
/// not reachable over first-layer MCP (a non-Google service, or one without an MCP server).
pub fn endpoint(service: Service) -> Option<McpEndpoint> {
    match service {
        Service::Gmail => Some(GMAIL),
        Service::GoogleCalendar => Some(CALENDAR),
        Service::GoogleDrive => Some(DRIVE),
        // Later waves / non-Google — no Google MCP endpoint.
        Service::Slack | Service::Notion | Service::GitHub | Service::Linear => None,
    }
}

/// Whether a service is reachable over an official first-layer MCP endpoint today.
pub fn has_endpoint(service: Service) -> bool {
    endpoint(service).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_workspace_services_have_endpoints() {
        for s in [Service::Gmail, Service::GoogleCalendar, Service::GoogleDrive] {
            let ep = endpoint(s).expect("google service has an endpoint");
            assert!(ep.url.starts_with("https://"), "{s:?} url must be https");
            assert!(ep.url.ends_with("/mcp/v1"), "{s:?} url must be the mcp/v1 path");
            assert!(!ep.scopes.is_empty(), "{s:?} must request scopes");
        }
    }

    #[test]
    fn non_google_services_have_no_endpoint() {
        for s in [Service::Slack, Service::Notion, Service::GitHub, Service::Linear] {
            assert!(endpoint(s).is_none(), "{s:?} is not a Google MCP endpoint");
            assert!(!has_endpoint(s));
        }
    }

    #[test]
    fn gmail_scopes_never_include_send() {
        // Invariant 4 / §6.10: the first-layer Gmail connection can read and draft, never send.
        let gmail = endpoint(Service::Gmail).unwrap();
        assert!(gmail.scopes.iter().any(|s| s.ends_with("gmail.compose")));
        assert!(
            !gmail.scopes.iter().any(|s| s.contains("gmail.send")),
            "first-layer Gmail must not hold a send scope"
        );
    }

    #[test]
    fn calendar_has_a_writable_events_scope_for_l3_creates() {
        let cal = endpoint(Service::GoogleCalendar).unwrap();
        assert!(cal.scopes.contains(&"https://www.googleapis.com/auth/calendar.events"));
    }
}
