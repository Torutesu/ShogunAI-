//! The per-service permission scope table (§6.9.2) — the source of truth for first-layer MCP.
//!
//! "以下の表が各サービスの実装範囲の正である。表にない操作を実装しない。" This module encodes those
//! tables in code and gates every service operation through [`authorize`]:
//! - an operation **not in the table** is denied ([`Authorization::DeniedUnknownOp`]);
//! - an explicitly out-of-v1-scope operation is denied ([`Authorization::DeniedNotImplemented`]);
//! - Gmail send is first-layer-denied and routed to Composio ([`Authorization::RequiresComposio`]);
//! - reads sync in the background; everything that writes carries its L2/L3 level.
//!
//! Invariant 4 is enforced over the whole table (see tests): every [`OpClass::ExternalSend`] is
//! gated L3 (or Composio, itself L3) — no service can post/create/react below L3.

use shogun_agents::permission::Level;

/// The six first-layer services (§6.9.2), grouped into rollout waves (FR-INT-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Gmail,
    GoogleCalendar,
    Slack,
    Notion,
    GitHub,
    Linear,
}

/// Rollout wave (FR-INT-03): Wave 1 = Gmail + Calendar → Wave 2 = Slack → Wave 3 = Notion + GitHub
/// + Linear. Ordered so a service is enabled only once its wave is released.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Wave {
    One = 1,
    Two = 2,
    Three = 3,
}

impl Service {
    /// The wave this service ships in.
    pub fn wave(self) -> Wave {
        match self {
            Service::Gmail | Service::GoogleCalendar => Wave::One,
            Service::Slack => Wave::Two,
            Service::Notion | Service::GitHub | Service::Linear => Wave::Three,
        }
    }

    /// Whether this service is released given the highest released wave (FR-INT-03). Unreleased
    /// services show as "Coming soon" and must not sync.
    pub fn is_released(self, highest_released: Wave) -> bool {
        self.wave() <= highest_released
    }

    /// The `event_log.source` discriminator for items ingested from this service (§6.9, the source
    /// column values). Synced integration items land in the log tagged with this, so search and
    /// Context Fusion can tell an email from a captured window (FR-INT-05).
    pub fn source_str(self) -> &'static str {
        match self {
            Service::Gmail => "gmail",
            Service::GoogleCalendar => "gcal",
            Service::Slack => "slack",
            Service::Notion => "notion",
            Service::GitHub => "github",
            Service::Linear => "linear",
        }
    }
}

/// What an operation does — this fixes the gating it is allowed to have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpClass {
    /// Read the service's data (into the event log).
    Read,
    /// A device-local draft (never written to the service).
    DraftLocal,
    /// A reversible write to the service that is not a send (e.g. a Gmail draft/label/read-state).
    ServiceStateChange,
    /// A send/post/create/react/event-create — leaves the device irreversibly (always L3).
    ExternalSend,
}

impl OpClass {
    /// Whether this class leaves the device as an irreversible external write (a send).
    pub fn is_external_send(self) -> bool {
        matches!(self, OpClass::ExternalSend)
    }
}

/// How an operation is gated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gating {
    /// Background sync — no per-action confirmation (reads).
    Background,
    /// Requires this permission level.
    Level(Level),
    /// Not implemented in v1 (explicitly out of scope — the "—" rows).
    NotImplemented,
    /// First-layer-denied; provided only via Composio (second layer, itself L3). Gmail send.
    ComposioOnly,
}

/// One operation in a service's scope table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopedOp {
    pub name: &'static str,
    pub class: OpClass,
    pub gating: Gating,
}

/// A service's full scope table.
#[derive(Debug, Clone, Copy)]
pub struct ServiceScope {
    pub service: Service,
    pub ops: &'static [ScopedOp],
}

use Gating::*;
use OpClass::*;

const fn op(name: &'static str, class: OpClass, gating: Gating) -> ScopedOp {
    ScopedOp { name, class, gating }
}

// ---- Gmail (Wave 1) ------------------------------------------------------------------------
const GMAIL: &[ScopedOp] = &[
    op("read_sync", Read, Background),
    op("read_on_demand", Read, Level(Level::L2)),
    op("draft_create_update", ServiceStateChange, Level(Level::L2)),
    op("label_and_read_state", ServiceStateChange, Level(Level::L2)),
    // Send is NOT a first-layer operation — Composio only (§6.10), itself L3.
    op("send", ExternalSend, ComposioOnly),
    op("delete_archive", ExternalSend, NotImplemented),
];

// ---- Google Calendar (Wave 1) --------------------------------------------------------------
const GOOGLE_CALENDAR: &[ScopedOp] = &[
    op("read_sync", Read, Background),
    op("free_busy", Read, Level(Level::L2)),
    op("event_create", ExternalSend, Level(Level::L3)),
    op("event_update_delete", ExternalSend, Level(Level::L3)),
];

// ---- Slack (Wave 2) ------------------------------------------------------------------------
const SLACK: &[ScopedOp] = &[
    op("read_sync", Read, Background),
    op("draft_local", DraftLocal, Level(Level::L2)),
    // FR-INT-30 fallback: no external send, so L2.
    op("copy_to_clipboard", DraftLocal, Level(Level::L2)),
    op("post_message", ExternalSend, Level(Level::L3)),
    op("reaction", ExternalSend, Level(Level::L3)),
];

// ---- Notion (Wave 3) -----------------------------------------------------------------------
const NOTION: &[ScopedOp] = &[
    op("read_sync", Read, Background),
    op("page_or_row_create", ExternalSend, Level(Level::L3)),
    op("page_update", ExternalSend, Level(Level::L3)),
    op("delete", ExternalSend, NotImplemented),
];

// ---- GitHub (Wave 3) -----------------------------------------------------------------------
const GITHUB: &[ScopedOp] = &[
    op("read_sync", Read, Background),
    op("comment_draft", DraftLocal, Level(Level::L2)),
    op("issue_create_or_comment", ExternalSend, Level(Level::L3)),
    op("pr_merge_close_branch", ExternalSend, NotImplemented),
];

// ---- Linear (Wave 3) -----------------------------------------------------------------------
const LINEAR: &[ScopedOp] = &[
    op("read_sync", Read, Background),
    op("issue_draft", DraftLocal, Level(Level::L2)),
    op("issue_create_update_comment", ExternalSend, Level(Level::L3)),
    op("status_change", ExternalSend, Level(Level::L3)),
];

/// The scope table for a service.
pub fn scope(service: Service) -> ServiceScope {
    let ops = match service {
        Service::Gmail => GMAIL,
        Service::GoogleCalendar => GOOGLE_CALENDAR,
        Service::Slack => SLACK,
        Service::Notion => NOTION,
        Service::GitHub => GITHUB,
        Service::Linear => LINEAR,
    };
    ServiceScope { service, ops }
}

/// Every service, for exhaustive iteration in tests / settings.
pub const ALL_SERVICES: &[Service] = &[
    Service::Gmail,
    Service::GoogleCalendar,
    Service::Slack,
    Service::Notion,
    Service::GitHub,
    Service::Linear,
];

/// The result of authorizing a service operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorization {
    /// Allowed as a background sync (reads).
    Background,
    /// Allowed after confirmation at this level.
    RequiresLevel(Level),
    /// The operation is not in the service's table (§6.9.2: "表にない操作を実装しない").
    DeniedUnknownOp,
    /// The operation is explicitly out of v1 scope.
    DeniedNotImplemented,
    /// First-layer-denied; only Composio (second layer, L3) can perform it (Gmail send).
    RequiresComposio,
}

/// Authorize an operation by name against a service's scope table. The only entry point the
/// execution engine should use for MCP operations — an unknown name is denied, never defaulted.
/// Look up an operation's row (class + gating) in a service's scope table, if present. The
/// composed integration gate ([`crate::service_gate`]) uses this to apply the amber read/write
/// rule and the draft-stop rule on top of the raw authorization.
pub fn lookup(service: Service, op_name: &str) -> Option<ScopedOp> {
    scope(service).ops.iter().copied().find(|o| o.name == op_name)
}

pub fn authorize(service: Service, op_name: &str) -> Authorization {
    match scope(service).ops.iter().find(|o| o.name == op_name) {
        None => Authorization::DeniedUnknownOp,
        Some(o) => match o.gating {
            Gating::Background => Authorization::Background,
            Gating::Level(l) => Authorization::RequiresLevel(l),
            Gating::NotImplemented => Authorization::DeniedNotImplemented,
            Gating::ComposioOnly => Authorization::RequiresComposio,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_external_send_is_l3_or_composio_never_below() {
        // Invariant 4 across the entire table: a send is never gated Background/L1/L2.
        for &service in ALL_SERVICES {
            for o in scope(service).ops {
                if o.class.is_external_send() {
                    match o.gating {
                        Gating::Level(Level::L3) | Gating::ComposioOnly | Gating::NotImplemented => {}
                        other => panic!("{service:?}::{} send gated {other:?} — must be L3", o.name),
                    }
                }
            }
        }
    }

    #[test]
    fn class_and_gating_are_consistent() {
        for &service in ALL_SERVICES {
            for o in scope(service).ops {
                match (o.class, o.gating) {
                    // Reads sync in the background or are on-demand L2.
                    (OpClass::Read, Gating::Background) => {}
                    (OpClass::Read, Gating::Level(Level::L2)) => {}
                    // Local drafts are L2 (device-local).
                    (OpClass::DraftLocal, Gating::Level(Level::L2)) => {}
                    // Reversible service writes (Gmail draft/label) are L2.
                    (OpClass::ServiceStateChange, Gating::Level(Level::L2)) => {}
                    // Sends are L3 / Composio / not-implemented.
                    (OpClass::ExternalSend, Gating::Level(Level::L3)) => {}
                    (OpClass::ExternalSend, Gating::ComposioOnly) => {}
                    (OpClass::ExternalSend, Gating::NotImplemented) => {}
                    pair => panic!("{service:?}::{} has inconsistent (class, gating): {pair:?}", o.name),
                }
            }
        }
    }

    #[test]
    fn unknown_operation_is_denied() {
        assert_eq!(authorize(Service::Gmail, "read_all_mailboxes"), Authorization::DeniedUnknownOp);
        assert_eq!(authorize(Service::Slack, "delete_workspace"), Authorization::DeniedUnknownOp);
    }

    #[test]
    fn gmail_send_is_first_layer_denied_and_routed_to_composio() {
        assert_eq!(authorize(Service::Gmail, "send"), Authorization::RequiresComposio);
    }

    #[test]
    fn not_implemented_operations_are_denied() {
        assert_eq!(authorize(Service::Gmail, "delete_archive"), Authorization::DeniedNotImplemented);
        assert_eq!(authorize(Service::Notion, "delete"), Authorization::DeniedNotImplemented);
        assert_eq!(authorize(Service::GitHub, "pr_merge_close_branch"), Authorization::DeniedNotImplemented);
    }

    #[test]
    fn reads_sync_in_background_and_on_demand_is_l2() {
        assert_eq!(authorize(Service::GoogleCalendar, "read_sync"), Authorization::Background);
        assert_eq!(authorize(Service::Gmail, "read_on_demand"), Authorization::RequiresLevel(Level::L2));
        assert_eq!(authorize(Service::GoogleCalendar, "free_busy"), Authorization::RequiresLevel(Level::L2));
    }

    #[test]
    fn writes_and_posts_carry_their_levels() {
        assert_eq!(authorize(Service::Gmail, "draft_create_update"), Authorization::RequiresLevel(Level::L2));
        assert_eq!(authorize(Service::GoogleCalendar, "event_create"), Authorization::RequiresLevel(Level::L3));
        assert_eq!(authorize(Service::Slack, "post_message"), Authorization::RequiresLevel(Level::L3));
        assert_eq!(authorize(Service::Slack, "reaction"), Authorization::RequiresLevel(Level::L3));
        assert_eq!(authorize(Service::Notion, "page_or_row_create"), Authorization::RequiresLevel(Level::L3));
        assert_eq!(authorize(Service::Linear, "status_change"), Authorization::RequiresLevel(Level::L3));
    }

    #[test]
    fn slack_fallback_copy_is_l2_no_send() {
        // FR-INT-30: clipboard copy leaves no device — L2, and never a send.
        assert_eq!(authorize(Service::Slack, "copy_to_clipboard"), Authorization::RequiresLevel(Level::L2));
        let o = scope(Service::Slack).ops.iter().find(|o| o.name == "copy_to_clipboard").unwrap();
        assert!(!o.class.is_external_send());
    }

    #[test]
    fn every_service_has_a_read_and_at_least_one_op() {
        for &service in ALL_SERVICES {
            let ops = scope(service).ops;
            assert!(!ops.is_empty(), "{service:?} has no ops");
            assert!(
                ops.iter().any(|o| o.class == OpClass::Read),
                "{service:?} must have a read operation"
            );
        }
    }

    #[test]
    fn source_strings_are_distinct_and_stable() {
        // every service maps to a distinct event_log.source tag (FR-INT-05).
        let tags: Vec<&str> = ALL_SERVICES.iter().map(|s| s.source_str()).collect();
        let mut uniq = tags.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(uniq.len(), tags.len(), "source tags must be unique: {tags:?}");
        assert_eq!(Service::Gmail.source_str(), "gmail");
        assert_eq!(Service::GoogleCalendar.source_str(), "gcal");
    }

    #[test]
    fn wave_rollout_gates_unreleased_services() {
        // At Wave 1, only Gmail + Calendar are released.
        assert!(Service::Gmail.is_released(Wave::One));
        assert!(Service::GoogleCalendar.is_released(Wave::One));
        assert!(!Service::Slack.is_released(Wave::One));
        assert!(!Service::Notion.is_released(Wave::One));
        // At Wave 2, Slack joins; Wave-3 services still gated.
        assert!(Service::Slack.is_released(Wave::Two));
        assert!(!Service::GitHub.is_released(Wave::Two));
        // At Wave 3, everything is released.
        assert!(Service::Linear.is_released(Wave::Three));
    }
}
