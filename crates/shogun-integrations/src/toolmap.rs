//! Our scope-table op name → the Google MCP tool that performs it.
//!
//! The scope table ([`shogun_mcp::scope`]) is service-agnostic (`read_sync`, `draft_create_update`,
//! `event_create`, …). Each Google MCP server exposes its own tool names; this is the single place
//! that bridges the two, so the transport never hard-codes tool strings inline.
//!
//! Tool names are from the Google Workspace MCP servers (Developer Preview, verified 2026-07). A
//! `None` means "this op has no Google MCP tool" — either the op is Composio-routed (`gmail::send`)
//! or the service is not a Google endpoint. It is never a silent default.

use shogun_mcp::scope::Service;

/// The Google MCP tool name for `(service, op_name)`, if one exists.
pub fn tool_for(service: Service, op_name: &str) -> Option<&'static str> {
    match (service, op_name) {
        // ---- Gmail (gmailmcp.googleapis.com) ------------------------------------------------
        (Service::Gmail, "read_sync") => Some("search_threads"),
        (Service::Gmail, "read_on_demand") => Some("get_thread"),
        (Service::Gmail, "draft_create_update") => Some("create_draft"),
        (Service::Gmail, "label_and_read_state") => Some("label_message"),
        // send has NO Gmail MCP tool — it is the second layer (Composio), §6.10.
        (Service::Gmail, "send") => None,

        // ---- Google Calendar (calendarmcp.googleapis.com) -----------------------------------
        (Service::GoogleCalendar, "read_sync") => Some("list_events"),
        (Service::GoogleCalendar, "free_busy") => Some("suggest_time"),
        (Service::GoogleCalendar, "event_create") => Some("create_event"),
        (Service::GoogleCalendar, "event_update_delete") => Some("update_event"),

        // ---- Google Drive (drivemcp.googleapis.com) -----------------------------------------
        (Service::GoogleDrive, "read_sync") => Some("list_recent_files"),
        // read_file_content also serves Google Docs/Sheets content (no dedicated MCP server).
        (Service::GoogleDrive, "read_on_demand") => Some("read_file_content"),
        (Service::GoogleDrive, "file_create") => Some("create_file"),

        // ---- Slack (mcp.slack.com, Wave 2) ---------------------------------------------------
        // PROVISIONAL names from Slack's documented capability list (search / send / react);
        // confirm against the live server's tools/list at Wave-2 wire-up before first use.
        (Service::Slack, "read_sync") => Some("search_messages"),
        (Service::Slack, "post_message") => Some("send_message"),
        (Service::Slack, "reaction") => Some("add_reaction"),
        // draft_local / copy_to_clipboard are DEVICE-LOCAL by design (FR-INT-30) — never MCP.
        (Service::Slack, "draft_local" | "copy_to_clipboard") => None,

        // Anything else (unknown op, or a service without an MCP endpoint) has no tool.
        _ => None,
    }
}

/// The tool used to satisfy a background read-sync for a service, if it has one.
pub fn read_sync_tool(service: Service) -> Option<&'static str> {
    tool_for(service, "read_sync")
}

#[cfg(test)]
mod tests {
    use super::*;
    use shogun_mcp::scope::{self, Gating, OpClass};

    /// Every service with an official MCP endpoint today (Waves 1–2).
    const MAPPED: [Service; 4] =
        [Service::Gmail, Service::GoogleCalendar, Service::GoogleDrive, Service::Slack];

    #[test]
    fn every_mapped_service_read_sync_has_a_tool() {
        for s in MAPPED {
            assert!(read_sync_tool(s).is_some(), "{s:?} read_sync must map to a tool");
        }
    }

    #[test]
    fn gmail_send_has_no_mcp_tool() {
        // §6.10: send is Composio-only, so there is no first-layer MCP tool for it.
        assert_eq!(tool_for(Service::Gmail, "send"), None);
    }

    #[test]
    fn device_local_slack_ops_have_no_mcp_tool() {
        // FR-INT-30: the local draft and the clipboard fallback never leave the device.
        assert_eq!(tool_for(Service::Slack, "draft_local"), None);
        assert_eq!(tool_for(Service::Slack, "copy_to_clipboard"), None);
    }

    #[test]
    fn unmapped_wave3_services_map_to_no_tool() {
        assert_eq!(tool_for(Service::Notion, "read_sync"), None);
        assert_eq!(tool_for(Service::GitHub, "read_sync"), None);
        assert_eq!(tool_for(Service::Linear, "read_sync"), None);
    }

    #[test]
    fn every_routable_op_on_a_mapped_service_has_a_tool() {
        // For each service with an endpoint, every scope-table op that goes over MCP (implemented,
        // not the Composio-only send, not device-local) must map to a tool — nothing routable is
        // left unroutable, and nothing device-local grows a network path.
        for s in MAPPED {
            for op in scope::scope(s).ops {
                let over_mcp = !matches!(op.gating, Gating::NotImplemented | Gating::ComposioOnly)
                    && op.class != OpClass::DraftLocal;
                assert_eq!(
                    tool_for(s, op.name).is_some(),
                    over_mcp,
                    "{s:?}::{} routing does not match its class/gating",
                    op.name
                );
            }
        }
    }
}
