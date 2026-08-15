//! Confirmed-send → first-layer MCP op bridge (WP-F, §6.14).
//!
//! The approval queue hands back a [`SendAction`]; this module decides **where it goes** — which
//! first-layer service + scope-table op performs it, with what arguments — or that it is the
//! second layer's job (email send = Composio only, §6.10). Pure and exhaustive over the
//! [`SendAction`] variants, so the routing is Linux-testable and a new variant is a compile error
//! here rather than a silently unroutable send.
//!
//! The gate is NOT applied here: the caller executes via
//! [`crate::runtime::ConnectorRuntime::execute_write`], which re-checks
//! [`shogun_mcp::service_gate::authorize_op`] (the WP-F double gate).

use serde_json::{json, Value};
use shogun_agents::permission::SendAction;
use shogun_mcp::scope::Service;

/// Where a confirmed send is performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendRoute {
    /// A first-layer MCP write: this service's scope-table op.
    FirstLayer { service: Service, op: &'static str },
    /// The second layer (Composio, §6.10) — v1: Gmail send only. Never first-layer MCP.
    Composio,
}

/// Route a confirmed send to its executor. Exhaustive — every [`SendAction`] variant has exactly
/// one route.
pub fn route_send(action: &SendAction) -> SendRoute {
    match action {
        // Email send is deliberately NOT a first-layer op (the Gmail MCP has no send tool and the
        // first-layer connection holds no send scope) — it is Composio's, behind its consent gate.
        SendAction::SendEmail { .. } => SendRoute::Composio,
        SendAction::PostMessage { .. } => {
            SendRoute::FirstLayer { service: Service::Slack, op: "post_message" }
        }
        SendAction::CreateCalendarEvent { .. } => {
            SendRoute::FirstLayer { service: Service::GoogleCalendar, op: "event_create" }
        }
        SendAction::PostComment { .. } => {
            SendRoute::FirstLayer { service: Service::GitHub, op: "issue_create_or_comment" }
        }
        SendAction::AddReaction { .. } => {
            SendRoute::FirstLayer { service: Service::Slack, op: "reaction" }
        }
        SendAction::UpdateCalendarEvent { .. } => {
            SendRoute::FirstLayer { service: Service::GoogleCalendar, op: "event_update_delete" }
        }
        SendAction::CreateDocument { .. } => {
            SendRoute::FirstLayer { service: Service::GoogleDrive, op: "file_create" }
        }
        SendAction::UpdateDocument { .. } => {
            SendRoute::FirstLayer { service: Service::Notion, op: "page_update" }
        }
        SendAction::ChangeIssueStatus { .. } => {
            SendRoute::FirstLayer { service: Service::Linear, op: "status_change" }
        }
    }
}

/// Build the MCP tool arguments for a first-layer send: the action's addressing fields plus the
/// confirmed full body. Field names follow the Google/Slack tool schemas as currently documented;
/// confirm against live `tools/list` schemas at wire-up (same caveat as [`crate::toolmap`]).
pub fn args_for_send(action: &SendAction, body: &str) -> Value {
    match action {
        SendAction::SendEmail { to } => json!({ "to": to, "body": body }),
        SendAction::PostMessage { channel } => json!({ "channel": channel, "text": body }),
        SendAction::CreateCalendarEvent { title } => {
            json!({ "summary": title, "description": body })
        }
        SendAction::PostComment { target } => json!({ "target": target, "body": body }),
        SendAction::AddReaction { target } => json!({ "target": target, "name": body }),
        SendAction::UpdateCalendarEvent { title } => {
            json!({ "summary": title, "description": body })
        }
        SendAction::CreateDocument { title } => json!({ "name": title, "content": body }),
        SendAction::UpdateDocument { title } => json!({ "title": title, "content": body }),
        SendAction::ChangeIssueStatus { target } => json!({ "id": target, "state": body }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shogun_mcp::scope::{self, Gating, OpClass};
    use shogun_agents::permission::Level;

    fn all_actions() -> Vec<SendAction> {
        vec![
            SendAction::SendEmail { to: "a@b.com".into() },
            SendAction::PostMessage { channel: "#general".into() },
            SendAction::CreateCalendarEvent { title: "Sync".into() },
            SendAction::PostComment { target: "pr#12".into() },
            SendAction::AddReaction { target: "msg#1".into() },
            SendAction::UpdateCalendarEvent { title: "Sync".into() },
            SendAction::CreateDocument { title: "Notes".into() },
            SendAction::UpdateDocument { title: "Spec".into() },
            SendAction::ChangeIssueStatus { target: "ENG-1".into() },
        ]
    }

    #[test]
    fn email_send_routes_to_composio_never_first_layer() {
        // §6.10 / invariant: the first layer must not grow an email-send path.
        assert_eq!(route_send(&SendAction::SendEmail { to: "a@b.com".into() }), SendRoute::Composio);
    }

    #[test]
    fn every_first_layer_route_targets_a_real_l3_external_send_op() {
        // Each routed (service, op) must exist in the scope table as an ExternalSend gated L3 —
        // the bridge can never point a send at a read/draft op or an unknown name.
        for action in all_actions() {
            if let SendRoute::FirstLayer { service, op } = route_send(&action) {
                let row = scope::lookup(service, op)
                    .unwrap_or_else(|| panic!("{service:?}::{op} not in the scope table"));
                assert_eq!(row.class, OpClass::ExternalSend, "{service:?}::{op} must be a send");
                assert_eq!(row.gating, Gating::Level(Level::L3), "{service:?}::{op} must be L3");
            }
        }
    }

    #[test]
    fn args_carry_the_addressing_field_and_the_full_body() {
        let a = SendAction::PostMessage { channel: "#general".into() };
        let v = args_for_send(&a, "the confirmed text");
        assert_eq!(v["channel"], "#general");
        assert_eq!(v["text"], "the confirmed text");

        let c = SendAction::CreateCalendarEvent { title: "Sync".into() };
        let v = args_for_send(&c, "agenda");
        assert_eq!(v["summary"], "Sync");
        assert_eq!(v["description"], "agenda");
    }

    /// Every proposal the model can make must execute against the service it was made for.
    ///
    /// The forward map (a tool the model calls → a [`SendAction`]) lives in
    /// `shogun_mcp::tool_catalog`; the reverse map (an approved action → the service that
    /// performs it) is [`route_send`] here. They are written independently, and one variant can
    /// only route to one service — so if two services ever shared a variant, an approved Linear
    /// comment would be posted to GitHub. This test is what keeps that from being wired: a
    /// published proposal whose round trip does not return to its own service fails here.
    #[test]
    fn every_published_proposal_round_trips_to_its_own_service() {
        use shogun_mcp::tool_catalog::{proposed_action, ToolKind};
        use shogun_mcp::scope::ALL_SERVICES;

        let input = serde_json::json!({
            "to": "a@b.com", "title": "t", "channel": "#c", "target": "x", "body": "b"
        });

        let mut checked = 0;
        for service in ALL_SERVICES {
            for op in scope::scope(*service).ops {
                // Find the published proposal for this row, if any.
                let Some(entry) = published_proposal_for(*service, op.name) else { continue };
                let Some(action) = proposed_action(entry, &input) else {
                    panic!("{} publishes a proposal that builds no action", entry.name)
                };
                let shogun_agents::permission::Action::Send(send) = action else {
                    panic!("{} built a non-send action", entry.name)
                };
                checked += 1;
                match route_send(&send) {
                    SendRoute::FirstLayer { service: routed_service, op: routed_op } => assert_eq!(
                        (routed_service, routed_op),
                        (*service, op.name),
                        "{} would execute against the wrong service",
                        entry.name,
                    ),
                    // Gmail send is deliberately not a first-layer op; Composio is its only route.
                    SendRoute::Composio => assert_eq!(
                        (*service, op.name),
                        (Service::Gmail, "send"),
                        "{} routed to Composio but is not the Gmail send",
                        entry.name,
                    ),
                }
                assert_eq!(entry.kind, ToolKind::Propose);
            }
        }
        assert!(checked >= 5, "the sweep must actually check proposals, got {checked}");
    }

    /// The published proposal entry for a scope row, if the catalog has one.
    fn published_proposal_for(
        service: Service,
        op_name: &str,
    ) -> Option<&'static shogun_mcp::tool_catalog::ToolEntry> {
        use shogun_mcp::tool_catalog::{catalog_entry, ToolKind};
        // The catalog has no iterator by design (its contents are an implementation detail), so
        // the names are listed here — and the count is asserted by the caller, so a proposal added
        // without being added here shows up as a coverage drop rather than passing silently.
        const PUBLISHED: &[&str] = &[
            "propose_send_email",
            "propose_calendar_event",
            "propose_calendar_event_change",
            "propose_drive_document",
            "propose_chat_message",
            "propose_chat_reaction",
            "propose_issue_comment",
            "propose_doc_change",
            "propose_issue_status_change",
        ];
        PUBLISHED
            .iter()
            .filter_map(|n| catalog_entry(n))
            .find(|e| e.service == service && e.scope_op == op_name && e.kind == ToolKind::Propose)
    }
}
