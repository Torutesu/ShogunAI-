//! The send producer (§6.6.3): turn a preset agent's drafted proposal into an L3 entry on the
//! approval queue. This is the piece that connects an agent (Reply Drafter, Calendar Scheduler,
//! Issue Triage, Note Capture) to the confirmed-send execution path — the agent drafts, the human
//! confirms (FR-AG-03), the executor sends.
//!
//! Pure: it builds the [`SendAction`] + [`Preview`] and enqueues. The actual body drafting (BYOK
//! LLM) and execution are the effectful layers around it. Centralizing the action/preview
//! construction here means the routing (email → Composio second layer; everything else → direct
//! first layer) and the Gmail "Subject: …\n\n…" body shape are defined once.

use crate::approval::{ApprovalId, ApprovalOrigin, ApprovalQueue, Preview, Route};
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
    CalendarEvent { title: String, body: String },
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
                // The subject must never contain a newline: the executor splits this shape back
                // on the FIRST blank line, so an embedded "\n\n" would make the approved preview
                // and the sent mail disagree (part of the subject would become body).
                format!("Subject: {}\n\n{body}", subject.replace(['\r', '\n'], " ")),
                Route::ViaComposio,
            ),
            ProposedSend::SlackPost { channel, body } => {
                (SendAction::PostMessage { channel: channel.clone() }, body.clone(), Route::DirectMcp)
            }
            ProposedSend::CalendarEvent { title, body } => {
                (SendAction::CreateCalendarEvent { title: title.clone() }, body.clone(), Route::DirectMcp)
            }
            ProposedSend::IssueComment { target, body } => {
                (SendAction::PostComment { target: target.clone() }, body.clone(), Route::DirectMcp)
            }
        }
    }
}

/// Enqueue an agent-drafted send for L3 confirmation. Returns the pending [`ApprovalId`]; the send
/// runs only after a dedicated-button confirm (the existing confirm → execute path). `Err` only on
/// approval-id exhaustion — refused rather than panicking, since callers hold the shared queue
/// lock (a panic there would poison it for the whole app).
pub fn propose(
    queue: &mut ApprovalQueue,
    proposal: &ProposedSend,
    origin: ApprovalOrigin,
    now_ms: u64,
) -> Result<ApprovalId, &'static str> {
    let (action, full_body, route) = proposal.parts();
    let preview = Preview::for_send(&action, full_body, route);
    queue.try_request(action, preview, origin, now_ms)
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
    fn a_newline_in_the_subject_cannot_shift_content_into_the_body() {
        // The executor splits "Subject: …\n\n…" on the first blank line. An LLM- or
        // capture-sourced subject containing "\n\n" must not smuggle text past what the
        // human approved as the subject line.
        let p = ProposedSend::Email {
            to: "bob@example.com".into(),
            subject: "Renewal\n\nplease don't quote me".into(),
            body: "Hi Dave".into(),
        };
        let (_, full, _) = p.parts();
        let (before, after) = full.split_once("\n\n").expect("shape");
        assert_eq!(before, "Subject: Renewal  please don't quote me");
        assert_eq!(after, "Hi Dave");
    }

    #[test]
    fn non_email_proposals_route_direct() {
        for (p, dest) in [
            (ProposedSend::SlackPost { channel: "#g".into(), body: "hi".into() }, "#g"),
            (ProposedSend::CalendarEvent { title: "Sync".into(), body: "agenda".into() }, "Sync"),
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
        let p = ProposedSend::CalendarEvent { title: "Sync".into(), body: "agenda".into() };
        let id = propose(&mut q, &p, ApprovalOrigin::Ui, 0).expect("enqueue");
        assert_eq!(q.pending_len(), 1);
        // it is a normal L3 entry: a dedicated-button confirm yields the ConfirmedSend to execute.
        match q.confirm(id, ConfirmIntent::DedicatedButton, 1000) {
            Decision::Confirmed(cs) => {
                assert!(matches!(cs.action, SendAction::CreateCalendarEvent { .. }));
                assert_eq!(cs.preview.full_body, "agenda");
            }
            other => panic!("expected Confirmed, got {other:?}"),
        }
    }
}
