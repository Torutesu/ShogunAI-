//! Integration read-sync ingest (WP4.2, §6.9: "同期は `integration.synced` → event log（source列で
//! 区別）→ 検索/Fusionに合流"). The composition that turns a released, connected service into actual
//! memory: gate `read_sync` → fetch via the transport → normalize each item into a **source-tagged**
//! ingest record the daemon appends to the event log.
//!
//! Layering (mirrors the pure/effect split used elsewhere): this module is pure except for the one
//! [`IntegrationTransport`] call, and produces [`IngestItem`]s — it never touches the DB itself
//! (invariant 1, data gravity in the Rust core). The daemon persists the records and emits
//! `IntegrationSynced` on the bus.
//!
//! The gate is the same [`crate::service_gate::authorize_op`] every operation goes through: a sync
//! only proceeds when `read_sync` resolves to [`OpDecision::Background`] (released + connected, or
//! amber serving cached reads). An unreleased / disconnected service can never sync. A direct
//! first-layer read carries no user data off the device and writes no traceability row — but a
//! read that crosses a third-party boundary (Gmail via Composio, the 2026-07 decision) must be
//! traced by its executor (digest/flag only): consult
//! [`crate::service_gate::requires_traceability`], which is the oracle for both cases.

use crate::scope::Service;
use crate::service_gate::{authorize_op, DenyReason, OpContext, OpDecision};

/// The operation name a read-sync gates on (the `read_sync` row in every service's scope table).
pub const READ_SYNC: &str = "read_sync";

/// One item fetched from a service, normalized across integrations. The transport (real MCP, or a
/// fake in tests) yields these; v1 carries only text fields — no attachments, no raw payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedItem {
    /// The service-side id (message id, event id, …) — used only for de-duplication upstream; it is
    /// **not** stored as content.
    pub external_id: String,
    /// A short human title (subject / event name / message preview). May be empty.
    pub title: String,
    /// The item's text body — the only thing that becomes searchable memory.
    pub body: String,
    /// The item's own timestamp (unix ms) — when it happened on the service, not when we synced.
    pub ts_ms: i64,
}

/// A normalized, **source-tagged** record ready for the event log. The daemon computes the content
/// hash and appends it (source = the service's [`Service::source_str`]), so a synced email is a
/// first-class event alongside a captured window (FR-INT-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestItem {
    /// The `event_log.source` tag (`gmail` / `gcal` / …).
    pub source: &'static str,
    /// The `event_log.kind` (`email` / `calendar_event` / `message` / …).
    pub kind: &'static str,
    /// The item title → `window_title` (the log's short-label column).
    pub title: String,
    /// The item body → `content` (searchable text).
    pub body: String,
    /// The item's own timestamp (unix ms).
    pub ts_ms: i64,
}

/// Why a sync could not be performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncFailure {
    /// The gate refused the read (unreleased wave / disconnected / unknown op).
    Denied(DenyReason),
    /// The transport failed to fetch (network / auth). The string is a non-sensitive reason (no
    /// item content) suitable for a log.
    Transport(String),
}

/// A transport that fetches a service's recent items. The real implementation is a remote-MCP
/// client (Category C — needs OAuth tokens); tests inject a fake. Kept as a seam so the whole
/// ingest composition is Linux-testable without a network.
pub trait IntegrationTransport {
    /// Fetch the recent items for a `read_sync`. Returns a non-sensitive error string on failure.
    fn read_sync(&self, service: Service) -> Result<Vec<FetchedItem>, String>;

    /// Fetch a specific item on demand (§6.9 `read_on_demand`, e.g. the Gmail thread currently on
    /// screen). `query` identifies it — a thread/message id, a file id, or a search string. The
    /// default is "not supported" (fakes / read-sync-only transports); the real remote-MCP
    /// transport overrides it. Kept a seam so the whole on-demand composition is Linux-testable.
    fn fetch_on_demand(&self, service: Service, query: &str) -> Result<Vec<FetchedItem>, String> {
        let _ = (service, query);
        Err("on-demand fetch not supported by this transport".to_string())
    }
}

/// The `event_log.kind` a service's items are tagged with.
fn item_kind(service: Service) -> &'static str {
    match service {
        Service::Gmail => "email",
        Service::GoogleCalendar => "calendar_event",
        Service::GoogleDrive => "file",
        Service::Slack => "message",
        Service::Notion => "page",
        Service::GitHub => "issue",
        Service::Linear => "issue",
    }
}

/// Check that a `read_sync` is allowed right now (FR-INT-03/06). A sync is permitted only when the
/// gate resolves `read_sync` to a background read; every other decision (including any required
/// confirmation, which a background sync must never trigger) refuses the sync.
pub fn authorize_sync(service: Service, ctx: &OpContext) -> Result<(), DenyReason> {
    match authorize_op(service, READ_SYNC, ctx) {
        OpDecision::Background => Ok(()),
        OpDecision::Denied(reason) => Err(reason),
        // read_sync is a Background op in every service table; anything else is a table/gate bug.
        // Refuse rather than silently ingesting under the wrong gating.
        _ => Err(DenyReason::UnknownOp),
    }
}

/// Normalize a fetched item into a source-tagged ingest record, dropping empty-body items (nothing
/// to remember). Empty titles are kept (some items have only a body).
fn normalize(service: Service, item: FetchedItem) -> Option<IngestItem> {
    let body = item.body.trim();
    if body.is_empty() {
        return None;
    }
    Some(IngestItem {
        source: service.source_str(),
        kind: item_kind(service),
        title: item.title,
        body: body.to_string(),
        ts_ms: item.ts_ms,
    })
}

/// The full ingest composition: gate the sync, fetch via the transport, and normalize the results
/// into source-tagged records the daemon can append. Returns the records (possibly empty) or a
/// [`SyncFailure`]. Does no DB I/O.
pub fn collect_sync<T: IntegrationTransport + ?Sized>(
    service: Service,
    ctx: &OpContext,
    transport: &T,
) -> Result<Vec<IngestItem>, SyncFailure> {
    authorize_sync(service, ctx).map_err(SyncFailure::Denied)?;
    let fetched = transport.read_sync(service).map_err(SyncFailure::Transport)?;
    Ok(fetched.into_iter().filter_map(|it| normalize(service, it)).collect())
}

/// The op name an on-demand fetch gates on (the `read_on_demand` row; only Gmail + Drive have it).
pub const READ_ON_DEMAND: &str = "read_on_demand";

/// The on-demand fetch composition (§6.9 `read_on_demand`, L2): gate → fetch a specific item via the
/// transport → normalize. Same shape as [`collect_sync`] but for a targeted fetch rather than the
/// background poll. Denied for a service without a `read_on_demand` op (Slack/Calendar), or when the
/// gate refuses (unreleased / disconnected / amber-write). Does no DB I/O.
pub fn collect_on_demand<T: IntegrationTransport + ?Sized>(
    service: Service,
    query: &str,
    ctx: &OpContext,
    transport: &T,
) -> Result<Vec<IngestItem>, SyncFailure> {
    // read_on_demand is gated L2 (a read the user's focus context requested); any non-allowed
    // decision refuses the fetch.
    match authorize_op(service, READ_ON_DEMAND, ctx) {
        OpDecision::RequiresLevel(_) => {}
        OpDecision::Denied(reason) => return Err(SyncFailure::Denied(reason)),
        // read_on_demand is an L2 op in every table that has it; anything else is a gate bug —
        // refuse rather than fetch under the wrong gating.
        _ => return Err(SyncFailure::Denied(DenyReason::UnknownOp)),
    }
    let fetched = transport.fetch_on_demand(service, query).map_err(SyncFailure::Transport)?;
    Ok(fetched.into_iter().filter_map(|it| normalize(service, it)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{ConnState, ReauthReason};
    use crate::scope::Wave;

    fn connected() -> ConnState {
        ConnState::Connected { last_sync_ms: 1_000 }
    }
    fn amber() -> ConnState {
        ConnState::NeedsReauth { reason: ReauthReason::TokenExpired, last_sync_ms: 500 }
    }
    fn ctx(highest: Wave, conn: ConnState) -> OpContext {
        OpContext { highest_released: highest, conn, draft_stop: false, plan: shogun_agents::entitlement::Entitlements::trial_not_started() }
    }
    fn item(id: &str, title: &str, body: &str, ts: i64) -> FetchedItem {
        FetchedItem { external_id: id.into(), title: title.into(), body: body.into(), ts_ms: ts }
    }

    /// A transport that yields a fixed list, or errors.
    struct Fake {
        items: Vec<FetchedItem>,
        err: Option<String>,
    }
    impl IntegrationTransport for Fake {
        fn read_sync(&self, _service: Service) -> Result<Vec<FetchedItem>, String> {
            match &self.err {
                Some(e) => Err(e.clone()),
                None => Ok(self.items.clone()),
            }
        }
        fn fetch_on_demand(&self, _service: Service, _query: &str) -> Result<Vec<FetchedItem>, String> {
            match &self.err {
                Some(e) => Err(e.clone()),
                None => Ok(self.items.clone()),
            }
        }
    }

    #[test]
    fn released_connected_service_ingests_source_tagged_items() {
        let fake = Fake {
            items: vec![
                item("m1", "Roadmap", "Let's ship the deck Friday", 10),
                item("m2", "Invoice", "Payment due next week", 20),
            ],
            err: None,
        };
        let out = collect_sync(Service::Gmail, &ctx(Wave::One, connected()), &fake).unwrap();
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|i| i.source == "gmail" && i.kind == "email"));
        assert_eq!(out[0].body, "Let's ship the deck Friday");
        assert_eq!(out[0].title, "Roadmap");
        assert_eq!(out[0].ts_ms, 10);
    }

    #[test]
    fn unreleased_wave_refuses_sync() {
        // Slack is Wave 2; with only Wave 1 released a sync is denied before any fetch.
        let fake = Fake { items: vec![item("s1", "t", "b", 1)], err: None };
        let err = collect_sync(Service::Slack, &ctx(Wave::One, connected()), &fake).unwrap_err();
        assert_eq!(err, SyncFailure::Denied(DenyReason::UnreleasedWave));
    }

    #[test]
    fn disconnected_service_refuses_sync() {
        let fake = Fake { items: vec![], err: None };
        let err = collect_sync(Service::Gmail, &ctx(Wave::One, ConnState::Disconnected), &fake).unwrap_err();
        assert_eq!(err, SyncFailure::Denied(DenyReason::NotConnected));
    }

    #[test]
    fn amber_service_still_syncs_cached_reads() {
        // FR-INT-06: an amber (needs-reauth) service serves cached reads — a sync is still allowed.
        let fake = Fake { items: vec![item("m1", "t", "body", 5)], err: None };
        let out = collect_sync(Service::Gmail, &ctx(Wave::One, amber()), &fake).unwrap();
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn transport_error_propagates_without_content() {
        let fake = Fake { items: vec![], err: Some("token refresh failed".into()) };
        let err = collect_sync(Service::Gmail, &ctx(Wave::One, connected()), &fake).unwrap_err();
        assert_eq!(err, SyncFailure::Transport("token refresh failed".into()));
    }

    #[test]
    fn empty_body_items_are_dropped() {
        let fake = Fake {
            items: vec![item("m1", "only a title", "   ", 1), item("m2", "", "real body", 2)],
            err: None,
        };
        let out = collect_sync(Service::Gmail, &ctx(Wave::One, connected()), &fake).unwrap();
        assert_eq!(out.len(), 1, "the whitespace-only body is dropped");
        assert_eq!(out[0].body, "real body");
    }

    #[test]
    fn on_demand_fetch_ingests_for_a_service_with_read_on_demand() {
        // Gmail has a read_on_demand (L2) row; a connected, released Gmail fetches on demand.
        let fake = Fake { items: vec![item("t1", "Thread", "the thread body", 7)], err: None };
        let out = collect_on_demand(Service::Gmail, "thread-123", &ctx(Wave::One, connected()), &fake).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, "gmail");
        assert_eq!(out[0].body, "the thread body");
    }

    #[test]
    fn on_demand_denied_for_a_service_without_that_op() {
        // Calendar has no read_on_demand row → denied (unknown op), no fetch.
        let fake = Fake { items: vec![item("e", "t", "b", 1)], err: None };
        let err = collect_on_demand(Service::GoogleCalendar, "q", &ctx(Wave::One, connected()), &fake).unwrap_err();
        assert_eq!(err, SyncFailure::Denied(DenyReason::UnknownOp));
    }

    #[test]
    fn on_demand_denied_when_disconnected() {
        let fake = Fake { items: vec![item("t", "t", "b", 1)], err: None };
        let err = collect_on_demand(Service::Gmail, "q", &ctx(Wave::One, ConnState::Disconnected), &fake).unwrap_err();
        assert_eq!(err, SyncFailure::Denied(DenyReason::NotConnected));
    }

    #[test]
    fn calendar_items_get_the_calendar_kind() {
        let fake = Fake { items: vec![item("e1", "Standup", "Daily standup 9am", 1)], err: None };
        let out = collect_sync(Service::GoogleCalendar, &ctx(Wave::One, connected()), &fake).unwrap();
        assert_eq!(out[0].source, "gcal");
        assert_eq!(out[0].kind, "calendar_event");
    }
}
