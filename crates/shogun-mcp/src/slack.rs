//! Slack posting fallback (FR-INT-30, §6.9.2 / CLAUDE.md 連携実装ルール). Some workspaces require an
//! admin to approve third-party apps before they may post. When that approval isn't granted, SHOGUN
//! must not dead-end a "post message" intent: it **degrades to a device-local draft on the
//! clipboard** — the user pastes it into Slack by hand.
//!
//! The fallback is not a send. A real post is an [`OpClass::ExternalSend`](crate::scope::OpClass)
//! gated L3; the clipboard draft is device-local, gated L2, and never leaves the device — so the
//! degrade also drops the gate from L3 to L2 without ever weakening invariant 4 (a send stays L3;
//! the fallback simply isn't one). Both arms resolve through the shared scope table
//! ([`crate::service_gate::authorize_op`]), so this stays consistent with the rest of the policy.

use shogun_agents::permission::Level;

use crate::scope::Service;
use crate::service_gate::{authorize_op, OpContext, OpDecision};

/// The Slack op names this fallback chooses between (rows in the Slack scope table).
const POST_MESSAGE: &str = "post_message";
const COPY_TO_CLIPBOARD: &str = "copy_to_clipboard";

/// Whether this workspace currently permits the app to post — a workspace-policy axis separate from
/// the OAuth connection state (a connected app can still be blocked from posting by admin policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostCapability {
    /// The workspace admin has approved posting — a real post is possible.
    Approved,
    /// Posting requires admin approval that isn't granted (FR-INT-30) — fall back to clipboard.
    AdminApprovalRequired,
}

/// How a Slack "post message" intent is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlackDelivery {
    /// A real post to the channel — an external send, confirmed at L3.
    Post { level: Level },
    /// Fallback: a device-local draft placed on the clipboard to paste by hand. Never egresses;
    /// gated L2 (FR-INT-30).
    ClipboardDraft { level: Level },
    /// Neither is possible (Slack unreleased / disconnected / needs reauth).
    Denied,
}

/// Resolve how a Slack post intent is delivered given the gate context and the workspace's posting
/// capability. When posting is approved a real L3 send is offered; when it is admin-blocked the
/// intent degrades to an L2 clipboard draft. Both arms go through the scope table, so an unreleased
/// or disconnected Slack denies either way.
pub fn resolve_post(ctx: &OpContext, capability: PostCapability) -> SlackDelivery {
    if capability == PostCapability::Approved {
        if let OpDecision::RequiresLevel(level) = authorize_op(Service::Slack, POST_MESSAGE, ctx) {
            return SlackDelivery::Post { level };
        }
    }
    // Admin-blocked, or a post that the gate refused — offer the device-local clipboard draft.
    match authorize_op(Service::Slack, COPY_TO_CLIPBOARD, ctx) {
        OpDecision::RequiresLevel(level) => SlackDelivery::ClipboardDraft { level },
        _ => SlackDelivery::Denied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{ConnState, ReauthReason};
    use crate::scope::Wave;

    fn connected() -> ConnState {
        ConnState::Connected { last_sync_ms: 1_000 }
    }
    // Slack is Wave 2, so it must be released for any of this to be reachable.
    fn ctx(conn: ConnState) -> OpContext {
        OpContext { highest_released: Wave::Two, conn, draft_stop: false }
    }

    #[test]
    fn approved_workspace_posts_at_l3() {
        let d = resolve_post(&ctx(connected()), PostCapability::Approved);
        assert_eq!(d, SlackDelivery::Post { level: Level::L3 });
    }

    #[test]
    fn admin_blocked_workspace_falls_back_to_clipboard_at_l2() {
        // FR-INT-30: no post, but the draft still reaches the user via the clipboard — and as a
        // device-local op it is L2, never a send.
        let d = resolve_post(&ctx(connected()), PostCapability::AdminApprovalRequired);
        assert_eq!(d, SlackDelivery::ClipboardDraft { level: Level::L2 });
    }

    #[test]
    fn the_fallback_is_never_a_send() {
        // The chosen fallback op must be a non-send in the scope table (invariant 4).
        let op = crate::scope::lookup(Service::Slack, COPY_TO_CLIPBOARD).unwrap();
        assert!(!op.class.is_external_send(), "the clipboard fallback must never be a send");
    }

    #[test]
    fn unreleased_slack_denies_either_way() {
        // At Wave 1 Slack isn't rolled out — neither a post nor the fallback is reachable.
        let ctx1 = OpContext { highest_released: Wave::One, conn: connected(), draft_stop: false };
        assert_eq!(resolve_post(&ctx1, PostCapability::Approved), SlackDelivery::Denied);
        assert_eq!(resolve_post(&ctx1, PostCapability::AdminApprovalRequired), SlackDelivery::Denied);
    }

    #[test]
    fn disconnected_slack_denies() {
        assert_eq!(resolve_post(&ctx(ConnState::Disconnected), PostCapability::Approved), SlackDelivery::Denied);
    }

    #[test]
    fn amber_slack_denies_the_write_fallback() {
        // A needs-reauth Slack serves cached reads only; a clipboard draft is a write-class op, so
        // the user must reauth first rather than silently draft against a stale token.
        let amber = ConnState::NeedsReauth { reason: ReauthReason::TokenExpired, last_sync_ms: 5 };
        assert_eq!(resolve_post(&ctx(amber), PostCapability::AdminApprovalRequired), SlackDelivery::Denied);
    }
}
