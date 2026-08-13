//! The send producer (§6.6.3): turn a preset agent's drafted proposal into an L3 entry on the
//! approval queue. This is the piece that connects an agent (Reply Drafter, Calendar Scheduler,
//! Issue Triage, Note Capture) to the confirmed-send execution path — the agent drafts, the human
//! confirms (FR-AG-03), the executor sends.
//!
//! Pure: it builds the [`SendAction`] + [`Preview`] and enqueues. The actual body drafting (BYOK
//! LLM) and execution are the effectful layers around it. Centralizing the action/preview
//! construction here means the routing (email → Composio second layer; everything else → direct
//! first layer) and the Gmail "Subject: …\n\n…" body shape are defined once.

use crate::approval::{ApprovalId, ApprovalQueue, Origin, Preview, Route};
use crate::permission::SendAction;

/// A drafted, ready-to-confirm send produced by an agent. The body is already generated (by the
/// BYOK LLM upstream); this type only carries the addressing + content needed to enqueue it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposedSend {
    /// Gmail reply/compose — routed through Composio (second layer, §6.10).
    Email { to: String, subject: String, body: String },
    /// A Slack channel/thread post (first layer, L3).
    SlackPost { channel: String, body: String },
    /// A Google Calendar event (first layer, L3; irreversible → invariant 4).
    CalendarEvent {
        title: String,
        start_time: String,
        end_time: String,
        calendar_id: Option<String>,
        description: String,
    },
    /// A GitHub/Linear issue comment (first layer, L3).
    IssueComment { target: String, body: String },
}

impl ProposedSend {
    /// The [`SendAction`], the full preview body (the exact text the executor will send), and the
    /// route. Email uses the `Subject: …\n\n…` shape the Composio executor splits back
    /// ([`crate::approval`] / shogun-mcp `parse_gmail_full_body`).
    pub fn parts(&self) -> (SendAction, String, Route) {
        match self {
            ProposedSend::Email { to, subject, body } => (
                SendAction::SendEmail { to: to.clone() },
                format!("Subject: {subject}\n\n{body}"),
                Route::ViaComposio,
            ),
            ProposedSend::SlackPost { channel, body } => {
                (SendAction::PostMessage { channel: channel.clone() }, body.clone(), Route::DirectMcp)
            }
            ProposedSend::CalendarEvent { title, start_time, end_time, calendar_id, description } => {
                let action = SendAction::CreateCalendarEvent {
                    title: title.clone(),
                    start_time: start_time.clone(),
                    end_time: end_time.clone(),
                    calendar_id: calendar_id.clone(),
                    description: description.clone(),
                };
                (action.clone(), action.calendar_preview_body(), Route::DirectMcp)
            }
            ProposedSend::IssueComment { target, body } => {
                (SendAction::PostComment { target: target.clone() }, body.clone(), Route::DirectMcp)
            }
        }
    }
}

/// Enqueue an agent-drafted send for L3 confirmation. Returns the pending [`ApprovalId`]; the send
/// runs only after a dedicated-button confirm (the existing confirm → execute path).
pub fn propose(
    queue: &mut ApprovalQueue,
    proposal: &ProposedSend,
    origin: Origin,
    now_ms: u64,
) -> ApprovalId {
    let (action, full_body, route) = proposal.parts();
    let preview = Preview::for_send(&action, full_body, route);
    queue.request(action, preview, origin, now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::{ConfirmIntent, Decision};

    #[test]
    fn email_proposal_routes_via_composio_with_subject_body_shape() {
        let p = ProposedSend::Email {
            to: "bob@example.com".into(),
            subject: "Ship date".into(),
            body: "Friday works.".into(),
        };
        let (action, full, route) = p.parts();
        assert!(matches!(action, SendAction::SendEmail { .. }));
        assert_eq!(route, Route::ViaComposio);
        assert_eq!(full, "Subject: Ship date\n\nFriday works.");
    }

    #[test]
    fn non_email_proposals_route_direct() {
        for (p, dest) in [
            (ProposedSend::SlackPost { channel: "#g".into(), body: "hi".into() }, "#g"),
            (ProposedSend::CalendarEvent {
                title: "Sync".into(),
                start_time: "2026-08-13T10:00:00Z".into(),
                end_time: "2026-08-13T11:00:00Z".into(),
                calendar_id: None,
                description: "agenda".into(),
            }, "Sync"),
            (ProposedSend::IssueComment { target: "pr#12".into(), body: "lgtm".into() }, "pr#12"),
        ] {
            let (action, full, route) = p.parts();
            assert_eq!(route, Route::DirectMcp);
            assert_eq!(Preview::for_send(&action, full, route).destination, dest);
        }
    }

    #[test]
    fn propose_enqueues_an_l3_send_the_confirm_path_can_run() {
        let mut q = ApprovalQueue::new();
        let p = ProposedSend::CalendarEvent {
            title: "Sync".into(),
            start_time: "2026-08-13T10:00:00Z".into(),
            end_time: "2026-08-13T11:00:00Z".into(),
            calendar_id: None,
            description: "agenda".into(),
        };
        let id = propose(&mut q, &p, Origin::Human, 0);
        assert_eq!(q.pending_len(), 1);
        // it is a normal L3 entry: a dedicated-button confirm yields the ConfirmedSend to execute.
        match q.confirm(id, ConfirmIntent::DedicatedButton, 1000) {
            Decision::Confirmed(cs) => {
                assert!(matches!(cs.action, SendAction::CreateCalendarEvent { .. }));
                assert!(cs.preview.full_body.contains("startTime: 2026-08-13T10:00:00Z"));
            }
            other => panic!("expected Confirmed, got {other:?}"),
        }
    }
}
