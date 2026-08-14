//! The composed first-layer integration gate (WP4.2, §6.9.2). One decision that layers everything
//! a service operation must satisfy *before* it runs, on top of the raw scope table
//! ([`crate::scope::authorize`]):
//!
//! 1. **Wave release** (FR-INT-03) — an operation on an unreleased wave is denied outright.
//! 2. **Scope table** — unknown / not-implemented operations are denied.
//! 3. **Connection state** (FR-INT-06/07) — a disconnected service can do nothing; an *amber*
//!    (needs-reauth) service serves cached **reads** but no writes (the token is invalid).
//! 4. **Draft-stop mode** — the Gmail "draft-stop" setting (§6.10) blocks Gmail send entirely, even
//!    though send would otherwise route to Composio.
//!
//! Invariant 4 is preserved end to end: the scope table only ever gates an [`OpClass::ExternalSend`]
//! at L3 / Composio / not-implemented, and this gate maps that faithfully — it can never turn a send
//! into an L1/L2 auto-run (test-asserted).

use shogun_agents::permission::Level;

use crate::connection::ConnState;
use crate::scope::{self, Gating, OpClass, Service, Wave};

/// The runtime context the gate needs beyond the static scope table.
#[derive(Debug, Clone, Copy)]
pub struct OpContext {
    /// The highest wave rolled out (FR-INT-03).
    pub highest_released: Wave,
    /// The service's current connection state (FR-INT-06/07).
    pub conn: ConnState,
    /// The global Gmail draft-stop setting (§6.10): when true, Gmail never sends.
    pub draft_stop: bool,
}

/// Why an operation was refused by the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    /// The service's wave is not rolled out yet (FR-INT-03).
    UnreleasedWave,
    /// The operation is not in the service's scope table.
    UnknownOp,
    /// The operation is explicitly out of v1 scope (the "—" rows).
    NotImplemented,
    /// The service is not connected (no token).
    NotConnected,
    /// The service is amber (needs reauth) and this is a write — the token is invalid.
    NeedsReauth,
    /// Gmail draft-stop is on, so send is blocked (§6.10).
    DraftStop,
}

/// The gate's verdict for one service operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpDecision {
    /// Allowed as a background read (no per-action confirm).
    Background,
    /// Allowed after a confirm at this permission level.
    RequiresLevel(Level),
    /// A first-layer-denied send routed to Composio (second layer, itself L3) — Gmail send with
    /// draft-stop OFF.
    RequiresComposio,
    /// Refused, with the reason.
    Denied(DenyReason),
}

impl OpDecision {
    /// Whether the operation may proceed (in any allowed form).
    pub fn is_allowed(self) -> bool {
        !matches!(self, OpDecision::Denied(_))
    }
}

/// Authorize one service operation against the full first-layer policy (§6.9.2).
pub fn authorize_op(service: Service, op_name: &str, ctx: &OpContext) -> OpDecision {
    // 1. Wave release — an unreleased service is entirely unreachable.
    if !service.is_released(ctx.highest_released) {
        return OpDecision::Denied(DenyReason::UnreleasedWave);
    }
    // 2. Scope table — the op must exist and be implemented.
    let Some(op) = scope::lookup(service, op_name) else {
        return OpDecision::Denied(DenyReason::UnknownOp);
    };
    if matches!(op.gating, Gating::NotImplemented) {
        return OpDecision::Denied(DenyReason::NotImplemented);
    }
    // 3. Connection — disconnected does nothing; amber serves cached reads only.
    match ctx.conn {
        ConnState::Disconnected => return OpDecision::Denied(DenyReason::NotConnected),
        ConnState::NeedsReauth { .. } if op.class != OpClass::Read => {
            return OpDecision::Denied(DenyReason::NeedsReauth);
        }
        _ => {}
    }
    // 4. Draft-stop — Gmail send (the only Composio-routed op) is blocked while it is on.
    if matches!(op.gating, Gating::ComposioOnly) && ctx.draft_stop {
        return OpDecision::Denied(DenyReason::DraftStop);
    }
    // 5. Map the (surviving) gating to a decision.
    match op.gating {
        Gating::Background => OpDecision::Background,
        Gating::Level(l) => OpDecision::RequiresLevel(l),
        Gating::ComposioOnly => OpDecision::RequiresComposio,
        // NotImplemented is handled in step 2; it cannot reach here.
        Gating::NotImplemented => OpDecision::Denied(DenyReason::NotImplemented),
    }
}

/// Whether an *allowed* operation constitutes an egress that must be recorded in the traceability
/// log (invariant 3): any external send, or a service write that leaves the device. Reads and
/// device-local drafts do not egress. The executor calls this to decide whether to trace.
pub fn requires_traceability(service: Service, op_name: &str) -> bool {
    matches!(
        scope::lookup(service, op_name).map(|o| o.class),
        Some(OpClass::ExternalSend | OpClass::ServiceStateChange)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::ReauthReason;

    fn connected() -> ConnState {
        ConnState::Connected {
            last_sync_ms: 1_000,
        }
    }
    fn amber() -> ConnState {
        ConnState::NeedsReauth {
            reason: ReauthReason::TokenExpired,
            last_sync_ms: 500,
        }
    }
    fn ctx(conn: ConnState, draft_stop: bool) -> OpContext {
        OpContext {
            highest_released: Wave::One,
            conn,
            draft_stop,
        }
    }

    #[test]
    fn unreleased_wave_is_denied() {
        // Slack is Wave 2; with only Wave 1 released it is unreachable even for a read.
        let d = authorize_op(Service::Slack, "read_sync", &ctx(connected(), false));
        assert_eq!(d, OpDecision::Denied(DenyReason::UnreleasedWave));
    }

    #[test]
    fn unknown_and_not_implemented_ops_are_denied() {
        assert_eq!(
            authorize_op(Service::Gmail, "nope", &ctx(connected(), false)),
            OpDecision::Denied(DenyReason::UnknownOp)
        );
        assert_eq!(
            authorize_op(Service::Gmail, "delete_archive", &ctx(connected(), false)),
            OpDecision::Denied(DenyReason::NotImplemented)
        );
    }

    #[test]
    fn disconnected_denies_everything() {
        let d = authorize_op(
            Service::Gmail,
            "read_sync",
            &ctx(ConnState::Disconnected, false),
        );
        assert_eq!(d, OpDecision::Denied(DenyReason::NotConnected));
    }

    #[test]
    fn amber_serves_reads_but_not_writes() {
        // read on amber → still allowed (cached), a draft write → denied
        assert_eq!(
            authorize_op(Service::Gmail, "read_sync", &ctx(amber(), false)),
            OpDecision::Background
        );
        assert_eq!(
            authorize_op(Service::Gmail, "draft_create_update", &ctx(amber(), false)),
            OpDecision::Denied(DenyReason::NeedsReauth)
        );
    }

    #[test]
    fn gmail_send_is_composio_and_draft_stop_blocks_it() {
        // draft-stop OFF → send routes to Composio
        assert_eq!(
            authorize_op(Service::Gmail, "send", &ctx(connected(), false)),
            OpDecision::RequiresComposio
        );
        // draft-stop ON → send blocked entirely
        assert_eq!(
            authorize_op(Service::Gmail, "send", &ctx(connected(), true)),
            OpDecision::Denied(DenyReason::DraftStop)
        );
    }

    #[test]
    fn draft_stop_does_not_block_reads_or_drafts_or_calendar() {
        // draft-stop only blocks the Gmail send; everything else is unaffected.
        assert_eq!(
            authorize_op(Service::Gmail, "read_sync", &ctx(connected(), true)),
            OpDecision::Background
        );
        assert_eq!(
            authorize_op(
                Service::Gmail,
                "draft_create_update",
                &ctx(connected(), true)
            ),
            OpDecision::RequiresLevel(Level::L2)
        );
        assert_eq!(
            authorize_op(
                Service::GoogleCalendar,
                "event_create",
                &ctx(connected(), true)
            ),
            OpDecision::RequiresLevel(Level::L3)
        );
    }

    #[test]
    fn calendar_event_is_l3() {
        assert_eq!(
            authorize_op(
                Service::GoogleCalendar,
                "event_create",
                &ctx(connected(), false)
            ),
            OpDecision::RequiresLevel(Level::L3)
        );
    }

    #[test]
    fn no_send_is_ever_auto_run_invariant_4() {
        // Across every Wave-1 op, an allowed decision for an ExternalSend/Composio op must be L3 or
        // Composio — never Background or a sub-L3 confirm.
        for service in [Service::Gmail, Service::GoogleCalendar] {
            for op in scope::scope(service).ops {
                let d = authorize_op(service, op.name, &ctx(connected(), false));
                if op.class == OpClass::ExternalSend {
                    match d {
                        OpDecision::RequiresLevel(l) => {
                            assert_eq!(l, Level::L3, "{} send below L3", op.name)
                        }
                        OpDecision::RequiresComposio | OpDecision::Denied(_) => {}
                        OpDecision::Background => panic!("send {} auto-ran (invariant 4)", op.name),
                    }
                }
            }
        }
    }

    #[test]
    fn traceability_required_for_writes_not_reads() {
        assert!(requires_traceability(Service::Gmail, "send"));
        assert!(requires_traceability(Service::Gmail, "draft_create_update"));
        assert!(requires_traceability(
            Service::GoogleCalendar,
            "event_create"
        ));
        assert!(!requires_traceability(Service::Gmail, "read_sync"));
    }
}
