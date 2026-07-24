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
}
