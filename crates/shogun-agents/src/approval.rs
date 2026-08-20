//! The L3 explicit-approval flow (WP4 pure core; §6.6.1, FR-AG-03/04). Every external send waits
//! here for a deliberate human confirmation before it runs.
//!
//! Structural guarantees:
//! - An approval request holds a [`SendAction`], not a generic `Action`. Since only sends are L3
//!   (invariant 4) and every send is L3, the queue is L3-by-construction — there is no path to
//!   enqueue a non-send, and none to downgrade one (FR-AG-02: no dynamic L3→L2).
//! - Confirmation requires the dedicated button ([`ConfirmIntent::DedicatedButton`]); the Enter
//!   key alone never confirms (FR-AG-03) — a distinct intent that the state machine refuses.
//! - The same flow serves human-UI and AI-API origins (FR-AG-04 / invariant 6). An API-origin
//!   request stays [`ApprovalState::Pending`] (the API call returns pending) until the user
//!   confirms or the 10-minute timeout rejects it.
//!
//! The preview carries the *full* content (FR-AG-03: full text, not a summary) plus destination,
//! route, and key kind, so the confirm UI can show everything required. The text lives only in the
//! in-memory request; traceability persists a digest, never this body.

use crate::permission::SendAction;

/// The route a send takes (FR-AG-03: 経路表示). Composio entries also get a "third-party" badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Direct to the service's official remote MCP.
    DirectMcp,
    /// Via Composio (second layer, §6.10) — surfaced as "third-party".
    ViaComposio,
}

/// The key kind used (FR-AG-03: 使用キー種別). L3 sends always run on the user's BYOK key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Byok,
}

/// The surface a pending approval was requested from (B-3 / E-08 queue unification). The flow is
/// identical for all three (FR-AG-04 / invariant 6) — the origin never changes permissions,
/// confirmation rules, or expiry. It is carried so the single shared queue's listing can label
/// each entry (UI / API / MCP) and so an API/MCP caller's waiting semantics (holding a pending
/// result) are visible to the confirm UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalOrigin {
    /// Enqueued by the human UI (Notch / settings / an agent the user launched from the UI).
    Ui,
    /// Enqueued through the REST/CLI Memory API face.
    Api,
    /// Enqueued through the MCP (stdio) face.
    Mcp,
}

impl ApprovalOrigin {
    /// Stable wire label for list output ("ui" / "api" / "mcp").
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalOrigin::Ui => "ui",
            ApprovalOrigin::Api => "api",
            ApprovalOrigin::Mcp => "mcp",
        }
    }
}

/// The full preview shown at L3 confirmation (FR-AG-03). `full_body` is the complete content, never
/// a summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub op_type: &'static str,
    pub destination: String,
    pub full_body: String,
    pub route: Route,
    pub key_kind: KeyKind,
}

impl Preview {
    /// Build the preview for a send, deriving the operation type and destination from the action so
    /// they cannot disagree with what will actually run.
    pub fn for_send(action: &SendAction, full_body: impl Into<String>, route: Route) -> Self {
        let (op_type, destination) = describe(action);
        Self {
            op_type,
            destination,
            full_body: full_body.into(),
            route,
            key_kind: KeyKind::Byok,
        }
    }
}

/// Human-readable op type + full destination for a send (FR-AG-03: 完全表記).
fn describe(action: &SendAction) -> (&'static str, String) {
    match action {
        SendAction::SendEmail { to } => ("Send email", to.clone()),
        SendAction::PostMessage { channel } => ("Post message", channel.clone()),
        SendAction::AddReaction { target } => ("Add reaction", target.clone()),
        SendAction::CreateCalendarEvent { title } => ("Create calendar event", title.clone()),
        // Named as a change rather than an update: the attendees are the ones who find out, and
        // "Update event" reads like an edit to a private note.
        SendAction::UpdateCalendarEvent { title } => ("Change calendar event", title.clone()),
        SendAction::PostComment { target } => ("Post comment", target.clone()),
        SendAction::CreateDocument { title } => ("Create document", title.clone()),
        SendAction::UpdateDocument { title } => ("Change document", title.clone()),
        SendAction::ChangeIssueStatus { target } => ("Change issue status", target.clone()),
    }
}

/// A handle for a pending approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ApprovalId(pub u64);

/// Why an approval was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectCause {
    /// The user explicitly rejected.
    UserRejected,
    /// The 10-minute window elapsed (FR-AG-04).
    TimedOut,
    /// The requester cancelled it.
    Cancelled,
}

/// The state of an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalState {
    Pending,
    Confirmed,
    Rejected(RejectCause),
}

/// Durable, body-free outcome exposed to every API face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalStatus {
    Pending,
    Rejected,
    TimedOut,
    Sent,
    SendFailed,
    DraftSaved,
}

/// Body-free terminal ledger record. Previews remain only while an item is pending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRecord {
    pub id: ApprovalId,
    pub status: ApprovalStatus,
    pub resolved_ms: u64,
}

/// How the user attempted to confirm. Only [`ConfirmIntent::DedicatedButton`] confirms; the Enter
/// key alone must not (FR-AG-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmIntent {
    DedicatedButton,
    EnterKey,
}

/// A confirmed send, ready to execute. Carries the action *and* its [`Preview`] so the executor
/// has the egress details it needs to record traceability (route, destination, and the full body
/// to digest) without re-deriving them — the trace and the send can never disagree (invariant 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmedSend {
    pub action: SendAction,
    pub preview: Preview,
}

/// The result of a confirm/reject/poll interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Confirmed and removed from the queue — the caller may now execute the send. Carries the
    /// action and its preview (see [`ConfirmedSend`]).
    Confirmed(ConfirmedSend),
    /// Rejected (with cause) and removed.
    Rejected(RejectCause),
    /// Still awaiting confirmation.
    StillPending,
    /// The Enter key alone was used — no state change; the dedicated button is required.
    RequiresDedicatedButton,
    /// No pending request with that id.
    Unknown,
}

/// The 10-minute L3 approval timeout (FR-AG-04).
pub const APPROVAL_TIMEOUT_MS: u64 = 10 * 60 * 1000;

/// A pending L3 request.
#[derive(Debug, Clone)]
struct PendingApproval {
    id: ApprovalId,
    action: SendAction,
    preview: Preview,
    origin: ApprovalOrigin,
    created_ms: u64,
}

/// Persistable pending approval, used only by the local protected L3 store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRecord {
    pub id: ApprovalId,
    pub action: SendAction,
    pub preview: Preview,
    pub origin: ApprovalOrigin,
    pub created_ms: u64,
}

/// The L3 approval queue. Holds pending sends until confirmed, rejected, or timed out.
#[derive(Default)]
pub struct ApprovalQueue {
    pending: Vec<PendingApproval>,
    terminal: Vec<TerminalRecord>,
    in_flight: Vec<ApprovalId>,
    next_id: u64,
}

pub const TERMINAL_RETENTION: usize = 256;

impl ApprovalQueue {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            terminal: Vec::new(),
            in_flight: Vec::new(),
            next_id: 1,
        }
    }

    /// Start a restored queue at a nonzero id; exhausted ids are never reused.
    pub fn with_next_id(next_id: u64) -> Self {
        Self {
            pending: Vec::new(),
            terminal: Vec::new(),
            in_flight: Vec::new(),
            next_id: next_id.max(1),
        }
    }

    pub fn export(&self) -> (u64, Vec<PendingRecord>) {
        (
            self.next_id,
            self.pending
                .iter()
                .map(|p| PendingRecord {
                    id: p.id,
                    action: p.action.clone(),
                    preview: p.preview.clone(),
                    origin: p.origin,
                    created_ms: p.created_ms,
                })
                .collect(),
        )
    }

    pub fn import_with_terminal(
        next_id: u64,
        records: Vec<PendingRecord>,
        terminal: Vec<TerminalRecord>,
        in_flight: Vec<ApprovalId>,
    ) -> Self {
        let mut queue = Self::with_next_id(next_id);
        queue.pending = records
            .into_iter()
            .map(|r| PendingApproval {
                id: r.id,
                action: r.action,
                preview: r.preview,
                origin: r.origin,
                created_ms: r.created_ms,
            })
            .collect();
        queue.terminal = terminal;
        queue.in_flight = in_flight;
        for id in queue
            .pending
            .iter()
            .map(|p| p.id)
            .chain(queue.terminal.iter().map(|r| r.id))
            .chain(queue.in_flight.iter().copied())
        {
            if id.0 >= queue.next_id {
                queue.next_id = id.0.saturating_add(1);
            }
        }
        queue.trim_terminal();
        queue
    }

    pub fn terminal_records(&self) -> &[TerminalRecord] {
        &self.terminal
    }
    pub fn in_flight_ids(&self) -> &[ApprovalId] {
        &self.in_flight
    }

    fn alloc_id(&mut self) -> Result<ApprovalId, &'static str> {
        if self.next_id == u64::MAX {
            return Err("approval id space exhausted");
        }
        let id = ApprovalId(self.next_id);
        self.next_id += 1;
        Ok(id)
    }

    /// Enqueue a send for L3 approval. Returns the handle; the request starts [`ApprovalState::Pending`]
    /// (an AI-API caller holds this pending result until resolved — FR-AG-04).
    pub fn try_request(
        &mut self,
        action: SendAction,
        preview: Preview,
        origin: ApprovalOrigin,
        now_ms: u64,
    ) -> Result<ApprovalId, &'static str> {
        let id = self.alloc_id()?;
        self.pending.push(PendingApproval {
            id,
            action,
            preview,
            origin,
            created_ms: now_ms,
        });
        Ok(id)
    }

    pub fn request(
        &mut self,
        action: SendAction,
        preview: Preview,
        origin: ApprovalOrigin,
        now_ms: u64,
    ) -> ApprovalId {
        self.try_request(action, preview, origin, now_ms)
            .expect("approval id space exhausted")
    }

    fn position(&self, id: ApprovalId) -> Option<usize> {
        self.pending.iter().position(|p| p.id == id)
    }

    fn is_expired(p: &PendingApproval, now_ms: u64) -> bool {
        now_ms.saturating_sub(p.created_ms) > APPROVAL_TIMEOUT_MS
    }

    /// Attempt to confirm. Only the dedicated button confirms (FR-AG-03); an expired request is
    /// rejected as timed-out instead of running.
    pub fn confirm(&mut self, id: ApprovalId, intent: ConfirmIntent, now_ms: u64) -> Decision {
        if intent != ConfirmIntent::DedicatedButton {
            // Enter-alone (or any non-button intent) never confirms; the request stays pending.
            return match self.position(id) {
                Some(_) => Decision::RequiresDedicatedButton,
                None => Decision::Unknown,
            };
        }
        let Some(idx) = self.position(id) else {
            return Decision::Unknown;
        };
        if Self::is_expired(&self.pending[idx], now_ms) {
            self.pending.remove(idx);
            self.record_terminal(id, ApprovalStatus::TimedOut, now_ms);
            return Decision::Rejected(RejectCause::TimedOut);
        }
        let p = self.pending.remove(idx);
        self.in_flight.push(id);
        Decision::Confirmed(ConfirmedSend {
            action: p.action,
            preview: p.preview,
        })
    }

    /// Explicitly reject a pending request.
    pub fn reject(&mut self, id: ApprovalId, cause: RejectCause) -> Decision {
        match self.position(id) {
            Some(idx) => {
                self.pending.remove(idx);
                self.record_terminal(
                    id,
                    if cause == RejectCause::TimedOut {
                        ApprovalStatus::TimedOut
                    } else {
                        ApprovalStatus::Rejected
                    },
                    0,
                );
                Decision::Rejected(cause)
            }
            None => Decision::Unknown,
        }
    }

    /// Poll a request's current state (for an AI-API caller awaiting resolution).
    pub fn poll(&self, id: ApprovalId) -> Decision {
        match self.position(id) {
            Some(_) => Decision::StillPending,
            None => Decision::Unknown,
        }
    }

    pub fn status(&self, id: ApprovalId) -> Option<ApprovalStatus> {
        if self.position(id).is_some() || self.in_flight.contains(&id) {
            return Some(ApprovalStatus::Pending);
        }
        self.terminal
            .iter()
            .find(|record| record.id == id)
            .map(|record| record.status)
    }

    /// Record outcome only after a dedicated confirmation moved an item in flight.
    pub fn mark_status(
        &mut self,
        id: ApprovalId,
        status: ApprovalStatus,
        resolved_ms: u64,
    ) -> bool {
        if status == ApprovalStatus::Pending || !self.in_flight.contains(&id) {
            return false;
        }
        self.in_flight.retain(|candidate| *candidate != id);
        self.record_terminal(id, status, resolved_ms);
        true
    }

    /// Resolve crash-recovered in-flight items conservatively; never claim a send succeeded.
    pub fn recover_in_flight(&mut self, now_ms: u64) -> Vec<ApprovalId> {
        let ids = std::mem::take(&mut self.in_flight);
        for id in &ids {
            self.record_terminal(*id, ApprovalStatus::SendFailed, now_ms);
        }
        ids
    }

    /// Reject every request past the timeout (FR-AG-04). Returns the timed-out ids. The daemon
    /// calls this on a timer tick.
    pub fn expire_due(&mut self, now_ms: u64) -> Vec<ApprovalId> {
        let (expired, live): (Vec<PendingApproval>, Vec<PendingApproval>) = self
            .pending
            .drain(..)
            .partition(|p| Self::is_expired(p, now_ms));
        self.pending = live;
        let ids: Vec<_> = expired.iter().map(|p| p.id).collect();
        for id in &ids {
            self.record_terminal(*id, ApprovalStatus::TimedOut, now_ms);
        }
        ids
    }

    /// The preview for a pending request (what the confirm UI renders).
    pub fn preview(&self, id: ApprovalId) -> Option<&Preview> {
        self.pending.iter().find(|p| p.id == id).map(|p| &p.preview)
    }

    /// The origin of a pending request (which surface enqueued it — shown in the list UI).
    pub fn origin(&self, id: ApprovalId) -> Option<ApprovalOrigin> {
        self.pending.iter().find(|p| p.id == id).map(|p| p.origin)
    }

    /// The action of a pending request (read-only; confirmation still goes through
    /// [`Self::confirm`]). Lets approval-time hooks derive per-action metadata (e.g. the L5
    /// feedback scope) without dequeuing.
    pub fn action(&self, id: ApprovalId) -> Option<&SendAction> {
        self.pending.iter().find(|p| p.id == id).map(|p| &p.action)
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// The ids of all pending approvals, oldest first — for a UI that lists the L3 confirm queue
    /// (each id resolves to its [`Preview`] via [`Self::preview`]).
    pub fn pending_ids(&self) -> Vec<ApprovalId> {
        self.pending.iter().map(|p| p.id).collect()
    }

    fn record_terminal(&mut self, id: ApprovalId, status: ApprovalStatus, resolved_ms: u64) {
        self.terminal.retain(|record| record.id != id);
        self.terminal.push(TerminalRecord {
            id,
            status,
            resolved_ms,
        });
        self.trim_terminal();
    }

    fn trim_terminal(&mut self) {
        if self.terminal.len() > TERMINAL_RETENTION {
            self.terminal
                .drain(..self.terminal.len() - TERMINAL_RETENTION);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn email() -> SendAction {
        SendAction::SendEmail {
            to: "alice@example.com".into(),
        }
    }

    fn preview() -> Preview {
        Preview::for_send(&email(), "Hi Alice, shipping Friday.", Route::ViaComposio)
    }

    #[test]
    fn pending_ids_lists_requests_oldest_first() {
        let mut q = ApprovalQueue::new();
        let a = q.request(email(), preview(), ApprovalOrigin::Ui, 0);
        let b = q.request(email(), preview(), ApprovalOrigin::Ui, 1);
        assert_eq!(q.pending_ids(), vec![a, b]);
        q.confirm(a, ConfirmIntent::DedicatedButton, 2);
        assert_eq!(q.pending_ids(), vec![b]);
    }

    #[test]
    fn preview_is_derived_from_the_action_and_is_full_text() {
        let p = Preview::for_send(&email(), "full body here", Route::ViaComposio);
        assert_eq!(p.op_type, "Send email");
        assert_eq!(p.destination, "alice@example.com");
        assert_eq!(p.full_body, "full body here"); // not summarized
        assert_eq!(p.route, Route::ViaComposio);
        assert_eq!(p.key_kind, KeyKind::Byok);
    }

    #[test]
    fn request_starts_pending() {
        let mut q = ApprovalQueue::new();
        let id = q.request(email(), preview(), ApprovalOrigin::Api, 0);
        assert_eq!(q.poll(id), Decision::StillPending);
        assert_eq!(q.pending_len(), 1);
        assert_eq!(q.origin(id), Some(ApprovalOrigin::Api));
    }

    #[test]
    fn enter_key_alone_does_not_confirm() {
        let mut q = ApprovalQueue::new();
        let id = q.request(email(), preview(), ApprovalOrigin::Ui, 0);
        // Enter → refused, still pending.
        assert_eq!(
            q.confirm(id, ConfirmIntent::EnterKey, 1000),
            Decision::RequiresDedicatedButton
        );
        assert_eq!(q.poll(id), Decision::StillPending);
        assert_eq!(q.pending_len(), 1);
    }

    #[test]
    fn dedicated_button_confirms_and_yields_the_send() {
        let mut q = ApprovalQueue::new();
        let id = q.request(email(), preview(), ApprovalOrigin::Ui, 0);
        let decision = q.confirm(id, ConfirmIntent::DedicatedButton, 1000);
        // the confirmed decision carries both the action and its preview (for traceability)
        assert_eq!(
            decision,
            Decision::Confirmed(ConfirmedSend {
                action: email(),
                preview: preview()
            })
        );
        // removed from the queue after resolution
        assert_eq!(q.pending_len(), 0);
        assert_eq!(q.poll(id), Decision::Unknown);
    }

    #[test]
    fn confirm_after_timeout_is_rejected_not_run() {
        let mut q = ApprovalQueue::new();
        let id = q.request(email(), preview(), ApprovalOrigin::Api, 0);
        // 10min + 1ms later
        let decision = q.confirm(id, ConfirmIntent::DedicatedButton, APPROVAL_TIMEOUT_MS + 1);
        assert_eq!(decision, Decision::Rejected(RejectCause::TimedOut));
        assert_eq!(q.pending_len(), 0);
    }

    #[test]
    fn expire_due_rejects_only_stale_requests() {
        let mut q = ApprovalQueue::new();
        let a = q.request(email(), preview(), ApprovalOrigin::Api, 0);
        let b = q.request(email(), preview(), ApprovalOrigin::Ui, APPROVAL_TIMEOUT_MS); // newer
                                                                                        // at now = TIMEOUT+1: a is stale, b is fresh
        let expired = q.expire_due(APPROVAL_TIMEOUT_MS + 1);
        assert_eq!(expired, vec![a]);
        assert_eq!(q.pending_len(), 1);
        // b still confirmable
        assert!(matches!(
            q.confirm(b, ConfirmIntent::DedicatedButton, APPROVAL_TIMEOUT_MS + 2),
            Decision::Confirmed(ConfirmedSend { action, .. }) if action == email()
        ));
    }

    #[test]
    fn user_reject_removes_the_request() {
        let mut q = ApprovalQueue::new();
        let id = q.request(email(), preview(), ApprovalOrigin::Ui, 0);
        assert_eq!(
            q.reject(id, RejectCause::UserRejected),
            Decision::Rejected(RejectCause::UserRejected)
        );
        assert_eq!(q.pending_len(), 0);
    }

    #[test]
    fn confirm_or_poll_unknown_id_is_unknown() {
        let mut q = ApprovalQueue::new();
        assert_eq!(
            q.confirm(ApprovalId(42), ConfirmIntent::DedicatedButton, 0),
            Decision::Unknown
        );
        assert_eq!(q.poll(ApprovalId(42)), Decision::Unknown);
    }

    #[test]
    fn preview_is_available_for_the_confirm_ui() {
        let mut q = ApprovalQueue::new();
        let id = q.request(email(), preview(), ApprovalOrigin::Ui, 0);
        let p = q.preview(id).unwrap();
        assert_eq!(p.destination, "alice@example.com");
        assert_eq!(p.route, Route::ViaComposio);
    }

    // Structural note: `request` takes a `SendAction`, so only L3 sends can enter the approval
    // queue. There is no constructor that accepts a `LocalAction`/L1/L2 action — a non-send simply
    // cannot be enqueued here, which is the type-level half of FR-AG-02 (no dynamic downgrade).
    #[test]
    fn every_send_variant_can_be_previewed() {
        for a in [
            SendAction::SendEmail {
                to: "a@b.com".into(),
            },
            SendAction::PostMessage {
                channel: "#eng".into(),
            },
            SendAction::CreateCalendarEvent {
                title: "Sync".into(),
            },
            SendAction::PostComment {
                target: "org/repo#1".into(),
            },
        ] {
            let p = Preview::for_send(&a, "body", Route::DirectMcp);
            assert!(!p.op_type.is_empty());
            assert!(!p.destination.is_empty());
        }
    }
}
