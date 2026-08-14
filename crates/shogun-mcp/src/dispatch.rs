//! The shared Memory API dispatcher (§6.11, FR-API-01/03/04/06). MCP / CLI / REST are all thin
//! wrappers over *this* — one place that enforces the policy so the three faces cannot drift and
//! the AI-API stays perfectly symmetric with the human UI (invariant 6).
//!
//! What it enforces, before any backend work happens:
//! - **Auth** (FR-API-03): no valid token → every call denied, reads included.
//! - **Tool kind** (FR-API-02): a read handler only accepts read tools, a write handler only
//!   accepts write tools — a mismatched call is refused, never mis-routed.
//! - **Read confidence** (FR-API-06): read results are filtered/flagged by the same
//!   [`crate::memory_api::read_inclusion`] rule the UI uses.
//! - **L3 goes to the same approval queue as the UI** (FR-API-04): an API-requested external send
//!   is enqueued and the caller gets a *pending* [`ApprovalId`] — it never completes without an
//!   explicit UI confirmation. A local L1/L2 action is authorized to run via the engine.
//!
//! The actual data read/write/execute is the backend's job (the engine + memory layer); this
//! module is the pure gate in front of it, so the policy is exhaustively Linux-testable.

use shogun_agents::approval::{ApprovalId, ApprovalQueue, Origin, Preview};
use shogun_agents::permission::{Action, Level, LocalAction, SendAction};

use crate::memory_api::{
    read_inclusion, tool_level, ApiLevel, AuthResult, ReadInclusion, TokenRegistry, Tool,
};

/// Why a call was refused before reaching the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    /// No token presented (FR-API-03: reads included).
    NoToken,
    /// Token presented but not valid.
    InvalidToken,
    /// The tool passed to a handler is not of that handler's kind (e.g. a write tool to `read`).
    WrongToolKind,
}

/// The result of a read call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOutcome {
    Denied(Denied),
    /// The read is authorized; `included` items passed the confidence gate, of which `possibly`
    /// are medium-confidence (flagged) — the rest were excluded (FR-API-06).
    Items {
        included: usize,
        possibly: usize,
    },
}

/// The result of a write call (append_note = L1, propose_update = L2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    Denied(Denied),
    /// Authorized to write at this level (the L2 propose still surfaces in the Notch downstream).
    Accepted {
        level: Level,
    },
}

/// The result of an `actions.execute` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionOutcome {
    Denied(Denied),
    /// A local L1/L2 action — authorized to run via the engine (no external send).
    Authorized {
        level: Level,
    },
    /// An external send (L3) — enqueued for explicit confirmation; the caller holds this pending
    /// id and polls the approval queue. It never completes without a UI confirm (FR-API-04).
    PendingApproval(ApprovalId),
}

/// The Memory API gate. Borrows the token registry and the shared approval queue (the same queue
/// the human UI drains — that shared queue is what makes API and UI symmetric).
pub struct MemoryApi<'a> {
    tokens: &'a TokenRegistry,
    approvals: &'a mut ApprovalQueue,
}

impl<'a> MemoryApi<'a> {
    pub fn new(tokens: &'a TokenRegistry, approvals: &'a mut ApprovalQueue) -> Self {
        Self { tokens, approvals }
    }

    /// Auth gate shared by every handler. `Ok(())` only for a valid token.
    fn authed(&self, token: Option<&str>) -> Result<(), Denied> {
        match self.tokens.authenticate(token) {
            AuthResult::Granted => Ok(()),
            AuthResult::DeniedNoToken => Err(Denied::NoToken),
            AuthResult::DeniedInvalidToken => Err(Denied::InvalidToken),
        }
    }

    /// Handle a read tool. `item_confidences` are the candidate results' confidences; the gate
    /// counts how many are included and how many of those are `possibly`-flagged (FR-API-06).
    pub fn handle_read(
        &self,
        token: Option<&str>,
        tool: Tool,
        item_confidences: &[f64],
        include_low: bool,
    ) -> ReadOutcome {
        if let Err(d) = self.authed(token) {
            return ReadOutcome::Denied(d);
        }
        if tool_level(tool) != ApiLevel::Read {
            return ReadOutcome::Denied(Denied::WrongToolKind);
        }
        let mut included = 0;
        let mut possibly = 0;
        for &c in item_confidences {
            match read_inclusion(c, include_low) {
                ReadInclusion::Included { possibly: p } => {
                    included += 1;
                    if p {
                        possibly += 1;
                    }
                }
                ReadInclusion::Excluded => {}
            }
        }
        ReadOutcome::Items { included, possibly }
    }

    /// Handle a write tool (append_note = L1, propose_update = L2).
    pub fn handle_write(&self, token: Option<&str>, tool: Tool) -> WriteOutcome {
        if let Err(d) = self.authed(token) {
            return WriteOutcome::Denied(d);
        }
        match tool_level(tool) {
            ApiLevel::Write(level) => WriteOutcome::Accepted { level },
            _ => WriteOutcome::Denied(Denied::WrongToolKind),
        }
    }

    /// Execute a local (on-device) action via `actions.execute`. Never a send — L1/L2 only.
    pub fn execute_local(&self, token: Option<&str>, action: LocalAction) -> ActionOutcome {
        if let Err(d) = self.authed(token) {
            return ActionOutcome::Denied(d);
        }
        let level = Action::Local(action).required_level();
        ActionOutcome::Authorized { level }
    }

    /// Submit an external send via `actions.execute`. Always L3: it is enqueued in the shared
    /// approval queue and the caller receives a pending id — it does not run here (FR-API-04). The
    /// same queue the Notch confirm UI drains, so an API L3 and a UI L3 are one flow.
    pub fn submit_send(
        &mut self,
        token: Option<&str>,
        send: SendAction,
        preview: Preview,
        now_ms: u64,
    ) -> ActionOutcome {
        if let Err(d) = self.authed(token) {
            return ActionOutcome::Denied(d);
        }
        // Origin::AiApi — the request came through the Memory API (FR-API-04).
        let id = self.approvals.request(send, preview, Origin::AiApi, now_ms);
        ActionOutcome::PendingApproval(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shogun_agents::approval::{ConfirmIntent, Decision, Route};

    fn reg() -> TokenRegistry {
        let mut r = TokenRegistry::new();
        r.issue("client-1");
        r
    }

    #[test]
    fn no_token_denies_every_handler_including_reads() {
        let tokens = reg();
        let mut approvals = ApprovalQueue::new();
        let api = MemoryApi::new(&tokens, &mut approvals);
        assert_eq!(
            api.handle_read(None, Tool::MemorySearch, &[0.9], false),
            ReadOutcome::Denied(Denied::NoToken)
        );
        assert_eq!(
            api.handle_write(None, Tool::MemoryAppendNote),
            WriteOutcome::Denied(Denied::NoToken)
        );
        assert_eq!(
            api.execute_local(None, LocalAction::LocalSearch { query: "x".into() }),
            ActionOutcome::Denied(Denied::NoToken)
        );
    }

    #[test]
    fn invalid_token_is_distinguished() {
        let tokens = reg();
        let mut approvals = ApprovalQueue::new();
        let api = MemoryApi::new(&tokens, &mut approvals);
        assert_eq!(
            api.handle_read(Some("nope"), Tool::MemorySearch, &[0.9], false),
            ReadOutcome::Denied(Denied::InvalidToken)
        );
    }

    #[test]
    fn read_applies_confidence_rule() {
        let tokens = reg();
        let mut approvals = ApprovalQueue::new();
        let api = MemoryApi::new(&tokens, &mut approvals);
        // 0.9 High (included, not possibly), 0.6 Medium (included, possibly), 0.3 Low (excluded).
        let out = api.handle_read(
            Some("client-1"),
            Tool::StatePeopleList,
            &[0.9, 0.6, 0.3],
            false,
        );
        assert_eq!(
            out,
            ReadOutcome::Items {
                included: 2,
                possibly: 1
            }
        );
        // include_low=true pulls the 0.3 in, flagged possibly.
        let out2 = api.handle_read(
            Some("client-1"),
            Tool::StatePeopleList,
            &[0.9, 0.6, 0.3],
            true,
        );
        assert_eq!(
            out2,
            ReadOutcome::Items {
                included: 3,
                possibly: 2
            }
        );
    }

    #[test]
    fn read_handler_refuses_a_write_tool() {
        let tokens = reg();
        let mut approvals = ApprovalQueue::new();
        let api = MemoryApi::new(&tokens, &mut approvals);
        assert_eq!(
            api.handle_read(Some("client-1"), Tool::MemoryAppendNote, &[], false),
            ReadOutcome::Denied(Denied::WrongToolKind)
        );
    }

    #[test]
    fn writes_carry_their_levels() {
        let tokens = reg();
        let mut approvals = ApprovalQueue::new();
        let api = MemoryApi::new(&tokens, &mut approvals);
        assert_eq!(
            api.handle_write(Some("client-1"), Tool::MemoryAppendNote),
            WriteOutcome::Accepted { level: Level::L1 }
        );
        assert_eq!(
            api.handle_write(Some("client-1"), Tool::StateProposeUpdate),
            WriteOutcome::Accepted { level: Level::L2 }
        );
        // a read tool is not a write
        assert_eq!(
            api.handle_write(Some("client-1"), Tool::MemorySearch),
            WriteOutcome::Denied(Denied::WrongToolKind)
        );
    }

    #[test]
    fn local_action_is_authorized_at_its_level() {
        let tokens = reg();
        let mut approvals = ApprovalQueue::new();
        let api = MemoryApi::new(&tokens, &mut approvals);
        assert_eq!(
            api.execute_local(
                Some("client-1"),
                LocalAction::LocalSearch { query: "q".into() }
            ),
            ActionOutcome::Authorized { level: Level::L1 }
        );
        assert_eq!(
            api.execute_local(
                Some("client-1"),
                LocalAction::UpdateState {
                    table: "people",
                    state_id: 1
                }
            ),
            ActionOutcome::Authorized { level: Level::L2 }
        );
    }

    #[test]
    fn api_l3_send_is_pending_and_never_completes_without_ui_confirm() {
        let tokens = reg();
        let mut approvals = ApprovalQueue::new();
        let send = SendAction::SendEmail {
            to: "a@b.com".into(),
        };
        let preview = Preview::for_send(&send, "body", Route::DirectMcp);

        let id = {
            let mut api = MemoryApi::new(&tokens, &mut approvals);
            match api.submit_send(Some("client-1"), send.clone(), preview, 0) {
                ActionOutcome::PendingApproval(id) => id,
                other => panic!("expected pending, got {other:?}"),
            }
        };
        // The request is pending in the shared queue — not executed by the API call (FR-API-04).
        assert_eq!(approvals.poll(id), Decision::StillPending);
        assert_eq!(approvals.pending_len(), 1);
        // Only a UI confirm (dedicated button) completes it — the same flow as a human L3.
        assert!(matches!(
            approvals.confirm(id, ConfirmIntent::DedicatedButton, 1000),
            Decision::Confirmed(cs) if cs.action == send
        ));
    }

    #[test]
    fn api_l3_send_requires_auth_before_enqueuing() {
        let tokens = reg();
        let mut approvals = ApprovalQueue::new();
        let send = SendAction::SendEmail {
            to: "a@b.com".into(),
        };
        let preview = Preview::for_send(&send, "body", Route::DirectMcp);
        let mut api = MemoryApi::new(&tokens, &mut approvals);
        assert_eq!(
            api.submit_send(None, send, preview, 0),
            ActionOutcome::Denied(Denied::NoToken)
        );
        // nothing was enqueued
        assert_eq!(approvals.pending_len(), 0);
    }
}
