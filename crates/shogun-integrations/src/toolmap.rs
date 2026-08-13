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
use serde_json::Value;

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

        // ---- Notion (mcp.notion.com, Wave 3) — PROVISIONAL names, confirm at wire-up -----------
        (Service::Notion, "read_sync") => Some("search"),
        (Service::Notion, "page_or_row_create") => Some("create-pages"),
        (Service::Notion, "page_update") => Some("update-page"),

        // ---- GitHub (api.githubcopilot.com, Wave 3) — PROVISIONAL --------------------------------
        (Service::GitHub, "read_sync") => Some("search_issues"),
        (Service::GitHub, "issue_create_or_comment") => Some("add_issue_comment"),
        // comment_draft is DEVICE-LOCAL (L2) — a local draft, never an MCP call.
        (Service::GitHub, "comment_draft") => None,

        // ---- Linear (mcp.linear.app, Wave 3) — PROVISIONAL --------------------------------------
        (Service::Linear, "read_sync") => Some("list_issues"),
        (Service::Linear, "issue_create_update_comment") => Some("create_comment"),
        (Service::Linear, "status_change") => Some("update_issue"),
        // issue_draft is DEVICE-LOCAL (L2).
        (Service::Linear, "issue_draft") => None,

        // Anything else (unknown op, not-implemented row, or device-local) has no tool.
        _ => None,
    }
}

/// The tool used to satisfy a background read-sync for a service, if it has one.
pub fn read_sync_tool(service: Service) -> Option<&'static str> {
    tool_for(service, "read_sync")
}

/// Validate arguments at execution time, including conditionally required Drive content type.
pub fn validate_write_arguments(service: Service, op_name: &str, arguments: &Value) -> Result<(), String> {
    let string = |name: &str, required: bool| {
        match arguments.get(name) {
            Some(Value::String(_)) => Ok(()),
            Some(_) => Err(format!("{service:?} {op_name} requires {name} string")),
            None if required => Err(format!("{service:?} {op_name} requires {name}")),
            None => Ok(()),
        }
    };
    match (service, op_name) {
        (Service::GoogleCalendar, "event_create") => {
            string("startTime", true)?; string("endTime", true)?;
        }
        (Service::GoogleCalendar, "event_update_delete") => string("eventId", true)?,
        (Service::GoogleDrive, "file_create") => {
            string("title", false)?;
            let has_content = arguments.get("textContent").is_some() || arguments.get("base64Content").is_some();
            if has_content { string("contentMimeType", true)?; }
            string("contentMimeType", false)?; string("textContent", false)?; string("base64Content", false)?;
        }
        _ => {}
    }
    Ok(())
}

/// Validate live server capabilities before a released connector becomes connected. The response
/// must be a `tools/list` result with JSON schemas; names alone are insufficient for Calendar
/// writes because `create_event` requires startTime and endTime.
pub fn validate_write_capabilities(service: Service, result: &Value) -> Result<(), String> {
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| "tools/list response had no tools".to_string())?;
    for op in shogun_mcp::scope::scope(service).ops {
        if !matches!(op.class, shogun_mcp::scope::OpClass::ServiceStateChange | shogun_mcp::scope::OpClass::ExternalSend)
            || matches!(op.gating, shogun_mcp::scope::Gating::NotImplemented | shogun_mcp::scope::Gating::ComposioOnly) {
            continue;
        }
        let Some(expected) = tool_for(service, op.name) else { continue };
        let tool = tools.iter().find(|tool| tool.get("name").and_then(Value::as_str) == Some(expected))
            .ok_or_else(|| format!("{service:?} missing released tool"))?;
        let schema = tool.get("inputSchema").and_then(Value::as_object)
            .ok_or_else(|| format!("{service:?} tool schema unavailable"))?;
        if schema.get("type").and_then(Value::as_str) != Some("object") {
            return Err(format!("{service:?} {expected} schema must be object"));
        }
        let properties = schema.get("properties").and_then(Value::as_object)
            .ok_or_else(|| format!("{service:?} {expected} schema has no properties"))?;
        let expected_fields: &[(&str, bool)] = match (service, op.name) {
            (Service::GoogleCalendar, "event_create") => &[("summary", false), ("startTime", true), ("endTime", true), ("calendarId", false), ("description", false)],
            (Service::GoogleCalendar, "event_update_delete") => &[("eventId", true)],
            (Service::GoogleDrive, "file_create") => &[("title", false), ("contentMimeType", false), ("textContent", false), ("base64Content", false)],
            _ => &[],
        };
        for (field, _) in expected_fields {
            let property = properties.get(*field).ok_or_else(|| format!("{expected} schema missing property {field}"))?;
            if property.get("type").and_then(Value::as_str) != Some("string") {
                return Err(format!("{expected} schema property {field} must be string"));
            }
        }
        let required = schema.get("required").and_then(Value::as_array);
        for (field, must_require) in expected_fields {
            if *must_require && !required.is_some_and(|values| values.iter().any(|v| v.as_str() == Some(*field))) {
                return Err(format!("{expected} schema missing required field {field}"));
            }
        }
        if service == Service::GoogleDrive && op.name == "file_create" {
            // contentMimeType is required only when textContent/base64Content is supplied;
            // schema validation must not invent an unconditional requirement.
            if required.is_some_and(|values| values.iter().any(|v| matches!(v.as_str(), Some("textContent") | Some("base64Content"))))
                && !required.is_some_and(|values| values.iter().any(|v| v.as_str() == Some("contentMimeType"))) {
                return Err("create_file schema requires contentMimeType with content".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shogun_mcp::scope::{self, Gating, OpClass};

    /// Every first-layer service — all now ship an official MCP endpoint (Waves 1–3).
    const MAPPED: [Service; 7] = [
        Service::Gmail,
        Service::GoogleCalendar,
        Service::GoogleDrive,
        Service::Slack,
        Service::Notion,
        Service::GitHub,
        Service::Linear,
    ];

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
    fn device_local_wave3_drafts_have_no_mcp_tool() {
        // GitHub comment_draft and Linear issue_draft are device-local (L2) — never MCP.
        assert_eq!(tool_for(Service::GitHub, "comment_draft"), None);
        assert_eq!(tool_for(Service::Linear, "issue_draft"), None);
        // Not-implemented rows are unroutable too.
        assert_eq!(tool_for(Service::Notion, "delete"), None);
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

    #[test]
    fn calendar_capability_probe_requires_usable_create_event_schema() {
        let result = serde_json::json!({"tools": [
            {"name":"suggest_time","inputSchema":{"type":"object"}},
            {"name":"create_event","inputSchema":{"type":"object","properties":{"summary":{"type":"string"},"startTime":{"type":"string"},"endTime":{"type":"string"},"calendarId":{"type":"string"},"description":{"type":"string"}}}},
            {"name":"update_event","inputSchema":{"type":"object","properties":{"eventId":{"type":"string"}},"required":["eventId"]}}
        ]});
        let err = validate_write_capabilities(Service::GoogleCalendar, &result).unwrap_err();
        assert!(err.contains("startTime"));
    }

    #[test]
    fn calendar_capability_probe_accepts_required_times() {
        let result = serde_json::json!({"tools": [
            {"name":"suggest_time","inputSchema":{"type":"object"}},
            {"name":"create_event","inputSchema":{"type":"object","properties":{"summary":{"type":"string"},"startTime":{"type":"string"},"endTime":{"type":"string"},"calendarId":{"type":"string"},"description":{"type":"string"}},"required":["startTime","endTime"]}},
            {"name":"update_event","inputSchema":{"type":"object","properties":{"eventId":{"type":"string"}},"required":["eventId"]}}
        ]});
        assert!(validate_write_capabilities(Service::GoogleCalendar, &result).is_ok());
    }

    #[test]
    fn capability_probe_does_not_require_unrelated_read_tools() {
        let result = serde_json::json!({"tools": [
            {"name":"create_event","inputSchema":{"type":"object","properties":{"summary":{"type":"string"},"startTime":{"type":"string"},"endTime":{"type":"string"},"calendarId":{"type":"string"},"description":{"type":"string"}},"required":["startTime","endTime"]}},
            {"name":"update_event","inputSchema":{"type":"object","properties":{"eventId":{"type":"string"}},"required":["eventId"]}}
        ]});
        assert!(validate_write_capabilities(Service::GoogleCalendar, &result).is_ok());
    }

    #[test]
    fn drive_create_validates_types_and_conditional_content_type() {
        let result = serde_json::json!({"tools": [
            {"name":"create_file","inputSchema":{"type":"object","properties":{"title":{"type":"string"},"contentMimeType":{"type":"string"},"textContent":{"type":"string"},"base64Content":{"type":"string"}},"required":["title"]}}
        ]});
        assert!(validate_write_capabilities(Service::GoogleDrive, &result).is_ok());
        assert!(validate_write_arguments(Service::GoogleDrive, "file_create", &serde_json::json!({"title":"x","textContent":"body"})).is_err());
        assert!(validate_write_arguments(Service::GoogleDrive, "file_create", &serde_json::json!({"title":"x"})).is_ok());
        assert!(validate_write_arguments(Service::GoogleCalendar, "event_update_delete", &serde_json::json!({"eventId":3})).is_err());
    }
}
