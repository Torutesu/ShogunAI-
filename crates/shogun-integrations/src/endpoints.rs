//! Service → official first-party remote MCP endpoint + OAuth scopes.
//!
//! First-layer connections go **directly** to each vendor's own first-party MCP server (§6.9,
//! FR-INT-01/02): user→service OAuth, no third party in the data path (unlike Composio, the second
//! layer). Only services that actually ship an official remote MCP server are mappable here.
//!
//! Coverage (verified 2026-07):
//! - Gmail   → `gmailmcp.googleapis.com` (Google Workspace Developer Preview)
//! - Calendar→ `calendarmcp.googleapis.com` (same)
//! - Drive → `drivemcp.googleapis.com` (same; also the read path for Google Docs/Sheets content, which have no dedicated MCP server of their own)
//! - Slack → `mcp.slack.com` (Wave 2; OPEN-03 resolved — Slack ships an official remote MCP, JSON-RPC 2.0 over Streamable HTTP, workspace-admin approved)
//!
//! Notion / GitHub / Linear are Wave 3 and unverified — [`endpoint`] returns `None` for them (and
//! for any service without an official remote MCP server).

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

const SLACK: McpEndpoint = McpEndpoint {
    url: "https://mcp.slack.com/mcp",
    // User-token scopes for the Wave-2 op set (read_sync / post_message / reaction). Per-tool
    // scopes are Slack-documented; confirm the final set against the Slack app config at wire-up.
    scopes: &["search:read.public", "chat:write", "reactions:write"],
};

/// The official remote MCP endpoint for a service, if the vendor ships one. `None` means the
/// service is not reachable over first-layer MCP today.
pub fn endpoint(service: Service) -> Option<McpEndpoint> {
    match service {
        Service::Gmail => Some(GMAIL),
        Service::GoogleCalendar => Some(CALENDAR),
        Service::GoogleDrive => Some(DRIVE),
        Service::Slack => Some(SLACK),
        // Wave 3 — official remote MCP availability unverified; add when confirmed.
        Service::Notion | Service::GitHub | Service::Linear => None,
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
    fn wave1_and_wave2_services_have_https_endpoints_with_scopes() {
        for s in [Service::Gmail, Service::GoogleCalendar, Service::GoogleDrive, Service::Slack] {
            let ep = endpoint(s).expect("service has an endpoint");
            assert!(ep.url.starts_with("https://"), "{s:?} url must be https");
            assert!(!ep.scopes.is_empty(), "{s:?} must request scopes");
        }
        // Google servers share the /mcp/v1 path; Slack's is /mcp.
        assert!(endpoint(Service::Gmail).unwrap().url.ends_with("/mcp/v1"));
        assert_eq!(endpoint(Service::Slack).unwrap().url, "https://mcp.slack.com/mcp");
    }

    #[test]
    fn wave3_services_have_no_endpoint_yet() {
        for s in [Service::Notion, Service::GitHub, Service::Linear] {
            assert!(endpoint(s).is_none(), "{s:?} has no verified official MCP endpoint");
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
