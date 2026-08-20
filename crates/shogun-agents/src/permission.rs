//! The L1/L2/L3 permission model (spec §6.6.1) and the invariant-4 guarantee.
//!
//! CLAUDE.md invariant 4: an L1 (auto-executed) action must never send anything off the device
//! — sends, posts, and calendar creation are always L3 (explicit confirmation). This is
//! enforced structurally: [`Action`] splits into [`LocalAction`] (on-device only) and
//! [`SendAction`] (leaves the device), and the level assignment maps every `SendAction` to
//! [`Level::L3`]. There is no `SendAction` variant reachable from L1, so an auto-run can never
//! be a send — a mislabel is impossible, not merely discouraged.

/// The three permission levels (spec §6.6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Auto-execute (no confirmation). Local, reversible, on-device only.
    L1,
    /// One-tap confirmation.
    L2,
    /// Explicit confirmation — required for everything that leaves the device (invariant 4).
    L3,
}

/// On-device actions — none of these leave the device, so all are L1/L2 eligible (never a send).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalAction {
    /// Bring an app to the foreground.
    OpenApp { bundle_id: String },
    /// Reveal a file in Finder.
    RevealFile { path: String },
    /// Run a local memory search.
    LocalSearch { query: String },
    /// Show a local notification / Notch indicator.
    ShowNotification { text: String },
    /// Put text on the clipboard (e.g. a generated draft the user will paste — not a send).
    CopyToClipboard { text: String },
    /// Update a local state record (people/projects/…). Local mutation, not an external write.
    UpdateState { table: &'static str, state_id: i64 },
    /// Save a draft locally (the "draft-stop mode" default for email — never sends). Persists a
    /// row, so it is L2 like [`LocalAction::UpdateState`] (and like the preset tables'
    /// `DraftGenerate`).
    SaveDraft { target: &'static str },
}

/// Actions that leave the device. By CLAUDE.md invariant 4 these are **always L3**.
///
/// The set is deliberately total over the scope table's `ExternalSend` rows
/// (`shogun_mcp::scope`): every operation that can leave the device has a variant here, so the
/// routing from a service operation to an action is a derivation rather than a match with a
/// silent fallback. An operation with no representation would have to be special-cased at the
/// call site, and a special case in this particular map is how invariant 4 gets lost.
///
/// The variants describe *what the user is about to have happen*, not which vendor does it —
/// the confirmation UI reads these, and "post a message to #eng" is the sentence a person can
/// approve, while "slack.post_message" is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendAction {
    /// Send an email (via Composio, opt-in — §6.10).
    SendEmail { to: String },
    /// Post a message to a chat service (Slack, …).
    PostMessage { channel: String },
    /// React to someone's message (an emoji is still visible to everyone in the channel).
    AddReaction { target: String },
    /// Create a calendar event.
    CreateCalendarEvent { title: String },
    /// Change or cancel an existing calendar event — visible to every attendee.
    UpdateCalendarEvent { title: String },
    /// Post a comment / review on an issue/PR (GitHub, Linear, …).
    PostComment { target: String },
    /// Create a document, page or file in a service (Drive, Notion, …).
    CreateDocument { title: String },
    /// Change an existing document or page in a service.
    UpdateDocument { title: String },
    /// Move an issue to another state (Linear, …) — visible to the whole team.
    ChangeIssueStatus { target: String },
}

/// An agent action: either on-device or an external send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Local(LocalAction),
    Send(SendAction),
}

impl Action {
    /// The minimum permission level required to run this action.
    ///
    /// Every [`SendAction`] is L3 (invariant 4). Local actions are L1 when they are trivially
    /// reversible and side-effect-light, else L2. No local action is L3, and — crucially — no
    /// send is ever below L3.
    pub fn required_level(&self) -> Level {
        match self {
            Action::Send(_) => Level::L3,
            Action::Local(a) => local_level(a),
        }
    }

    /// Whether this action may auto-execute at L1. True only for L1 local actions; a send can
    /// never be L1 (it is not even representable as a `LocalAction`).
    pub fn is_l1_eligible(&self) -> bool {
        self.required_level() == Level::L1
    }

    /// Whether this action leaves the device (a send). Sends always need L3 confirmation.
    pub fn is_external_send(&self) -> bool {
        matches!(self, Action::Send(_))
    }
}

fn local_level(a: &LocalAction) -> Level {
    match a {
        // Trivially reversible, no external effect → auto.
        LocalAction::OpenApp { .. }
        | LocalAction::RevealFile { .. }
        | LocalAction::LocalSearch { .. }
        | LocalAction::ShowNotification { .. }
        | LocalAction::CopyToClipboard { .. } => Level::L1,
        // Mutates persisted state → one-tap confirmation. SaveDraft writes a note row, and the
        // preset tables mandate DraftGenerate at L2 (OpKind::mandated_level) — keeping it L1 here
        // let fusion's draft candidates auto-run a persisted write with zero confirmation.
        LocalAction::UpdateState { .. } | LocalAction::SaveDraft { .. } => Level::L2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_sends() -> Vec<Action> {
        vec![
            Action::Send(SendAction::SendEmail { to: "a@b.com".into() }),
            Action::Send(SendAction::PostMessage { channel: "#general".into() }),
            Action::Send(SendAction::CreateCalendarEvent { title: "Sync".into() }),
            Action::Send(SendAction::PostComment { target: "pr#12".into() }),
        ]
    }

    #[test]
    fn every_send_is_l3_and_never_l1() {
        for a in all_sends() {
            assert_eq!(a.required_level(), Level::L3, "{a:?} must be L3 (invariant 4)");
            assert!(!a.is_l1_eligible(), "{a:?} must never be L1-eligible");
            assert!(a.is_external_send());
        }
    }

    #[test]
    fn local_auto_actions_are_l1() {
        for a in [
            LocalAction::OpenApp { bundle_id: "com.apple.Safari".into() },
            LocalAction::RevealFile { path: "/x".into() },
            LocalAction::LocalSearch { query: "budget".into() },
            LocalAction::ShowNotification { text: "hi".into() },
            LocalAction::CopyToClipboard { text: "draft".into() },
        ] {
            let action = Action::Local(a);
            assert_eq!(action.required_level(), Level::L1);
            assert!(action.is_l1_eligible());
            assert!(!action.is_external_send());
        }
    }

    #[test]
    fn state_mutation_is_l2() {
        // Both persisted-write local actions require the one-tap confirm — matching the preset
        // tables, where DraftGenerate and StateWrite are mandated L2.
        for a in [
            LocalAction::UpdateState { table: "people", state_id: 1 },
            LocalAction::SaveDraft { target: "gmail" },
        ] {
            let a = Action::Local(a);
            assert_eq!(a.required_level(), Level::L2);
            assert!(!a.is_l1_eligible());
        }
    }

    #[test]
    fn no_local_action_is_ever_l3() {
        // Local actions top out at L2; L3 is reserved for sends. (Structural: local_level never
        // returns L3.)
        for a in [
            LocalAction::OpenApp { bundle_id: "x".into() },
            LocalAction::UpdateState { table: "projects", state_id: 9 },
            LocalAction::SaveDraft { target: "gmail" },
        ] {
            assert_ne!(Action::Local(a).required_level(), Level::L3);
        }
    }

    // Compile-time invariant-4 note: an "auto-executed send" is unrepresentable — an L1 action
    // is an `Action::Local(LocalAction::…)`, and `LocalAction` has no send variant. To send, one
    // must build `Action::Send(…)`, which `required_level` forces to L3. The mislabel simply
    // cannot be written.
}
