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

        // Anything else (unknown op, or a non-Google service) has no Google MCP tool.
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
    use shogun_mcp::scope::{self, Gating};

    #[test]
    fn every_google_read_sync_has_a_tool() {
        for s in [Service::Gmail, Service::GoogleCalendar, Service::GoogleDrive] {
            assert!(read_sync_tool(s).is_some(), "{s:?} read_sync must map to a tool");
        }
    }

    #[test]
    fn gmail_send_has_no_mcp_tool() {
        // §6.10: send is Composio-only, so there is no first-layer MCP tool for it.
        assert_eq!(tool_for(Service::Gmail, "send"), None);
    }

    #[test]
    fn non_google_services_map_to_no_tool() {
        assert_eq!(tool_for(Service::Slack, "read_sync"), None);
        assert_eq!(tool_for(Service::Notion, "read_sync"), None);
    }

    #[test]
    fn every_implemented_google_op_except_send_maps_to_a_tool() {
        // For each Google service, every scope-table op that is implemented (not NotImplemented and
        // not the Composio-only send) must have a Google MCP tool — no implemented op is left
        // unroutable.
        for s in [Service::Gmail, Service::GoogleCalendar, Service::GoogleDrive] {
            for op in scope::scope(s).ops {
                let unroutable = matches!(op.gating, Gating::NotImplemented | Gating::ComposioOnly);
                if unroutable {
                    continue;
                }
                assert!(
                    tool_for(s, op.name).is_some(),
                    "{s:?}::{} is implemented but maps to no MCP tool",
                    op.name
                );
            }
        }
    }
}
