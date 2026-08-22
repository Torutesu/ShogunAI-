//! The daemon-side wiring skeleton: [`ConnectorRuntime`] ties the connection state
//! ([`shogun_mcp::connection`]), the gate ([`shogun_mcp::service_gate`]), the sync ingest
//! ([`shogun_mcp::sync`]), and the transport ([`crate::transport`]) into the two things the daemon
//! actually drives: **poll a read-sync** and **execute a confirmed write**.
//!
//! Data gravity stays in the core (invariant 1): the runtime never touches the DB. It hands synced
//! items to an [`IngestSink`] the daemon implements over its `Db` (mirrors the `MemoryBackend`
//! seam). The 15-minute scheduler (FR-INT-04) is the daemon's tokio loop calling [`ConnectorRuntime::poll_tick`];
//! this crate stays runtime-free and Linux-testable.
//!
//! Policy is never re-decided ad hoc: reads go through [`shogun_mcp::sync::collect_sync`] (which
//! calls the gate) and writes re-check [`shogun_mcp::service_gate::authorize_op`] before running
//! (the WP-F "double gate").

use serde_json::Value;
use shogun_mcp::connection::{ConnEvent, ConnState, ConnectionRegistry, DisconnectOutcome};
use shogun_mcp::scope::{Service, Wave, ALL_SERVICES};
use shogun_mcp::service_gate::{authorize_op, OpContext, OpDecision};
use shogun_mcp::sync::{collect_on_demand, collect_sync, IngestItem, IntegrationTransport, SyncFailure};

use crate::transport::WriteExecutor;

/// The default first-layer poll cadence (FR-INT-04: 15-minute read-sync).
pub const DEFAULT_SYNC_INTERVAL_MS: i64 = 15 * 60 * 1000;

/// Persists synced items into the event log. Implemented by the daemon over its `Db`
/// (`Db::ingest_integration`); returns the count of *newly inserted* items (what an
/// `IntegrationSynced` bus event reports).
pub trait IngestSink {
    fn ingest(&self, items: &[IngestItem]) -> usize;
}

/// The result of one read-sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncReport {
    pub fetched: usize,
    pub inserted: usize,
}

/// A service's connection state as the UI shows it (serializable for the Tauri command).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnUi {
    Connected,
    /// Amber — token expired/revoked; cached reads still serve, writes are blocked.
    NeedsReauth,
    Disconnected,
    /// The service's wave is not rolled out yet (FR-INT-03) — shown as "Coming soon".
    ComingSoon,
}

/// One row for the connections screen.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceStatus {
    /// The service (Rust-side; not serialized — the UI keys off `source`).
    #[serde(skip)]
    pub service: Service,
    /// Stable id the UI uses (`gmail` / `gcal` / `gdrive` / …).
    pub source: &'static str,
    pub state: ConnUi,
    /// Absolute unix-ms of the last successful sync, if any.
    pub last_sync_ms: Option<i64>,
    /// Whether a first-layer official MCP endpoint exists for this service today.
    pub has_endpoint: bool,
}

/// Owns per-service connection state and drives sync/write over a transport. Generic over the
/// transport so the daemon uses the live [`crate::RemoteMcpTransport`] and tests use a fake.
pub struct ConnectorRuntime<T> {
    transport: T,
    registry: ConnectionRegistry,
    highest_released: Wave,
    draft_stop: bool,
    /// The plan entitlements the gate applies (issue #97). Starts at the documented default
    /// (trial-not-started = full access until a stamp/billing state is known); the desktop layer
    /// refreshes it via [`ConnectorRuntime::set_plan`] before each sync tick / write.
    plan: shogun_agents::entitlement::Entitlements,
}

impl<T> ConnectorRuntime<T> {
    pub fn new(transport: T, highest_released: Wave, draft_stop: bool) -> Self {
        Self {
            transport,
            registry: ConnectionRegistry::new(),
            highest_released,
            draft_stop,
            plan: shogun_agents::entitlement::Entitlements::trial_not_started(),
        }
    }

    pub fn registry(&self) -> &ConnectionRegistry {
        &self.registry
    }

    /// Set the global Gmail draft-stop flag (§6.10) — flows into the gate context.
    pub fn set_draft_stop(&mut self, on: bool) {
        self.draft_stop = on;
    }

    /// Refresh the plan entitlements the gate applies (issue #97). The desktop layer resolves the
    /// plan (onboarding trial stamp + billing) and pushes it here — the runtime never reads the
    /// clock or the plan sources itself.
    pub fn set_plan(&mut self, plan: shogun_agents::entitlement::Entitlements) {
        self.plan = plan;
    }

    /// Record a completed OAuth connection for a service (drives it out of `Disconnected`).
    pub fn mark_connected(&mut self, service: Service, now_ms: i64) {
        self.registry.apply(service, ConnEvent::Connected { ts: now_ms });
    }

    /// Rehydrate a service the app already holds durable credentials for (FR-INT-03).
    ///
    /// The registry lives in memory and starts every service `Disconnected`, but a connection does
    /// not end when the process does — the token sits in the Keychain across relaunches. Without
    /// this the panel reports "Not connected" for a service that is connected, and — worse, because
    /// it is silent — [`Self::services_due`] skips it, so the 15-minute read-sync never runs again
    /// until the user redoes the whole browser OAuth flow.
    ///
    /// Restores with no last-sync time on purpose: the state is "connected, never synced this
    /// process", which makes the service due on the next tick instead of idling out one more
    /// interval, and shows no freshness rather than a fabricated one.
    pub fn restore_connected(&mut self, service: Service) {
        self.registry.apply(service, ConnEvent::Connected { ts: 0 });
    }

    /// Record that a service's token expired / was revoked (amber).
    pub fn mark_token_expired(&mut self, service: Service) {
        self.registry.apply(service, ConnEvent::TokenExpired);
    }

    /// Record that an interactive connect attempt failed mid-flight (browser denial, timeout,
    /// token exchange, persist) — amber with the reauth affordance (FR-INT-06), the same event a
    /// failed sync applies. Never called for precondition problems (missing OAuth client config),
    /// which leave the service Disconnected.
    pub fn mark_connect_failed(&mut self, service: Service) {
        self.registry.apply(service, ConnEvent::ConnectFailed);
    }

    /// Disconnect a service (FR-INT-07): the token is deleted by the caller; this clears state.
    pub fn disconnect(&mut self, service: Service, delete_events: bool) -> DisconnectOutcome {
        self.registry.disconnect(service, delete_events)
    }

    /// A UI-facing status for every service: connection state, freshness, wave availability, and
    /// whether a first-layer MCP endpoint exists yet. This is what the connections screen renders.
    pub fn statuses(&self, now_ms: i64) -> Vec<ServiceStatus> {
        ALL_SERVICES
            .iter()
            .copied()
            .map(|service| {
                let released = service.is_released(self.highest_released);
                let state = if !released {
                    ConnUi::ComingSoon
                } else {
                    match self.registry.state(service) {
                        ConnState::Connected { .. } => ConnUi::Connected,
                        ConnState::NeedsReauth { .. } => ConnUi::NeedsReauth,
                        ConnState::Disconnected => ConnUi::Disconnected,
                    }
                };
                ServiceStatus {
                    service,
                    source: service.source_str(),
                    state,
                    last_sync_ms: self.registry.freshness_ms(service, now_ms).map(|f| now_ms - f),
                    has_endpoint: crate::endpoints::has_endpoint(service),
                }
            })
            .collect()
    }

    fn ctx(&self, service: Service) -> OpContext {
        OpContext {
            highest_released: self.highest_released,
            conn: self.registry.state(service),
            draft_stop: self.draft_stop,
            plan: self.plan,
        }
    }
}

impl<T: IntegrationTransport> ConnectorRuntime<T> {
    /// Run one read-sync for a service: gate → fetch → ingest. On success, mark the service synced;
    /// on a transport failure, turn it amber (needs-reauth) without touching other services
    /// (FR-INT-06). A gate denial (unreleased / disconnected) changes no state.
    pub fn sync_service<S: IngestSink>(
        &mut self,
        service: Service,
        now_ms: i64,
        sink: &S,
    ) -> Result<SyncReport, SyncFailure> {
        let ctx = self.ctx(service);
        match collect_sync(service, &ctx, &self.transport) {
            Ok(items) => {
                let inserted = sink.ingest(&items);
                self.registry.apply(service, ConnEvent::Synced { ts: now_ms });
                Ok(SyncReport { fetched: items.len(), inserted })
            }
            Err(SyncFailure::Transport(e)) => {
                // A live fetch failure means the token/connection is bad → amber (FR-INT-06).
                self.registry.apply(service, ConnEvent::ConnectFailed);
                Err(SyncFailure::Transport(e))
            }
            // Denied (unreleased wave / disconnected / unknown op): no state change.
            Err(other) => Err(other),
        }
    }

    /// Fetch a specific item on demand (§6.9 `read_on_demand`, L2) and ingest it — e.g. the Gmail
    /// thread the user just opened. Unlike a background sync this does not touch the poll schedule
    /// (it is a targeted fetch, not a full refresh); on a transport failure the service still goes
    /// amber (a real connection problem). Denied for a service without a `read_on_demand` op or when
    /// the gate refuses.
    pub fn fetch_on_demand<S: IngestSink>(
        &mut self,
        service: Service,
        query: &str,
        sink: &S,
    ) -> Result<SyncReport, SyncFailure> {
        let ctx = self.ctx(service);
        match collect_on_demand(service, query, &ctx, &self.transport) {
            Ok(items) => {
                let inserted = sink.ingest(&items);
                Ok(SyncReport { fetched: items.len(), inserted })
            }
            Err(SyncFailure::Transport(e)) => {
                self.registry.apply(service, ConnEvent::ConnectFailed);
                Err(SyncFailure::Transport(e))
            }
            Err(other) => Err(other),
        }
    }

    /// Services whose read-sync is due now: released, connected-or-amber, and either never synced or
    /// staler than `interval_ms`. The scheduler basis for [`Self::poll_tick`].
    pub fn services_due(&self, now_ms: i64, interval_ms: i64) -> Vec<Service> {
        ALL_SERVICES
            .iter()
            .copied()
            .filter(|&s| s.is_released(self.highest_released))
            .filter(|&s| matches!(self.registry.state(s), ConnState::Connected { .. } | ConnState::NeedsReauth { .. }))
            .filter(|&s| self.registry.freshness_ms(s, now_ms).map_or(true, |f| f >= interval_ms))
            .collect()
    }

    /// Sync every due service once. This is the body the daemon's 15-minute tokio interval calls;
    /// it returns each service's outcome so the daemon can log/emit per service.
    pub fn poll_tick<S: IngestSink>(
        &mut self,
        now_ms: i64,
        interval_ms: i64,
        sink: &S,
    ) -> Vec<(Service, Result<SyncReport, SyncFailure>)> {
        self.services_due(now_ms, interval_ms)
            .into_iter()
            .map(|s| (s, self.sync_service(s, now_ms, sink)))
            .collect()
    }

    /// Execute a first-layer write that has already passed L2/L3 confirmation upstream (the approval
    /// queue in `shogun-agents`). Re-checks the gate as defense in depth (WP-F double gate) before
    /// touching the service; a Composio-routed op (Gmail send) is refused here — it is the second
    /// layer, not first-layer MCP.
    pub fn execute_write<W: WriteExecutor>(
        &self,
        service: Service,
        op_name: &str,
        arguments: Value,
        exec: &W,
    ) -> Result<Value, String> {
        match authorize_op(service, op_name, &self.ctx(service)) {
            OpDecision::RequiresLevel(_) => exec.execute(service, op_name, arguments),
            OpDecision::RequiresComposio => {
                Err(format!("{}::{op_name} is second-layer (Composio), not first-layer MCP", service.source_str()))
            }
            other => Err(format!("write not authorized: {other:?}")),
        }
    }
}

impl<T: IntegrationTransport + WriteExecutor> ConnectorRuntime<T> {
    /// The normal write path: gate + execute using the runtime's **own** transport as the executor
    /// (the live `RemoteMcpTransport` is a [`WriteExecutor`]). [`Self::execute_write`] keeps a
    /// separate-executor form for tests; production wiring uses this so it never has to hand the
    /// runtime its own transport from outside.
    pub fn execute_write_owned(
        &self,
        service: Service,
        op_name: &str,
        arguments: Value,
    ) -> Result<Value, String> {
        self.execute_write(service, op_name, arguments, &self.transport)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use shogun_mcp::sync::FetchedItem;

    /// A transport yielding a fixed fetch or an error.
    struct FakeTransport {
        items: Vec<FetchedItem>,
        err: Option<String>,
    }
    impl IntegrationTransport for FakeTransport {
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

    /// Counts ingested items.
    struct CountingSink(RefCell<usize>);
    impl IngestSink for CountingSink {
        fn ingest(&self, items: &[IngestItem]) -> usize {
            *self.0.borrow_mut() += items.len();
            items.len()
        }
    }

    fn item(body: &str) -> FetchedItem {
        FetchedItem { external_id: "x".into(), title: "t".into(), body: body.into(), ts_ms: 1 }
    }

    #[test]
    fn statuses_report_coming_soon_connected_and_endpoint_availability() {
        let mut rt = runtime(vec![], None);
        rt.mark_connected(Service::Gmail, 1_000);
        let statuses = rt.statuses(2_000);
        let gmail = statuses.iter().find(|s| s.source == "gmail").unwrap();
        assert_eq!(gmail.state, ConnUi::Connected);
        assert!(gmail.has_endpoint);
        assert_eq!(gmail.last_sync_ms, Some(1_000));
        // Slack is Wave 2 → coming soon at Wave 1; its official MCP endpoint exists (OPEN-03).
        let slack = statuses.iter().find(|s| s.source == "slack").unwrap();
        assert_eq!(slack.state, ConnUi::ComingSoon);
        assert!(slack.has_endpoint);
        // All first-layer services now have an official endpoint (Waves 1-3); Notion is Wave 3, so
        // still "coming soon" at Wave 1 but its endpoint exists.
        let notion = statuses.iter().find(|s| s.source == "notion").unwrap();
        assert_eq!(notion.state, ConnUi::ComingSoon);
        assert!(notion.has_endpoint);
        // Calendar released but not connected → disconnected.
        let cal = statuses.iter().find(|s| s.source == "gcal").unwrap();
        assert_eq!(cal.state, ConnUi::Disconnected);
    }

    fn runtime(items: Vec<FetchedItem>, err: Option<String>) -> ConnectorRuntime<FakeTransport> {
        // Wave 1 released, draft-stop on (irrelevant to reads).
        ConnectorRuntime::new(FakeTransport { items, err }, Wave::One, true)
    }

    #[test]
    fn a_restored_connection_shows_connected_and_syncs_on_the_next_tick() {
        // The relaunch case: the credential outlived the process, so the runtime is told to
        // restore. Before this existed the registry started Disconnected and stayed there, which
        // silently excluded the service from `services_due` — the read-sync never ran again.
        let mut rt = runtime(vec![], None);
        rt.restore_connected(Service::GoogleCalendar);

        assert!(
            matches!(rt.registry().state(Service::GoogleCalendar), ConnState::Connected { .. }),
            "a restored service reads as connected"
        );
        assert!(
            rt.services_due(60_000, 15 * 60_000).contains(&Service::GoogleCalendar),
            "restored with no last-sync time, so it is due immediately rather than idling out one \
             more interval"
        );
        assert_eq!(
            rt.registry().state(Service::Gmail),
            ConnState::Disconnected,
            "restoring one service does not touch another"
        );
    }

    #[test]
    fn mark_connect_failed_turns_only_that_service_amber() {
        let mut rt = runtime(vec![], None);
        rt.mark_connect_failed(Service::GoogleCalendar);
        assert!(rt.registry().state(Service::GoogleCalendar).is_amber());
        assert_eq!(rt.registry().state(Service::Gmail), ConnState::Disconnected);
    }

    #[test]
    fn sync_ingests_and_marks_synced() {
        let mut rt = runtime(vec![item("hello"), item("world")], None);
        rt.mark_connected(Service::Gmail, 100);
        let sink = CountingSink(RefCell::new(0));
        let report = rt.sync_service(Service::Gmail, 200, &sink).unwrap();
        assert_eq!(report, SyncReport { fetched: 2, inserted: 2 });
        assert_eq!(*sink.0.borrow(), 2);
        // freshness now reflects the sync at t=200
        assert_eq!(rt.registry().freshness_ms(Service::Gmail, 250), Some(50));
    }

    #[test]
    fn on_demand_fetch_ingests_without_touching_the_poll_schedule() {
        let mut rt = runtime(vec![item("thread body")], None);
        rt.mark_connected(Service::Gmail, 1_000);
        let before = rt.registry().freshness_ms(Service::Gmail, 5_000);
        let sink = CountingSink(RefCell::new(0));
        let report = rt.fetch_on_demand(Service::Gmail, "thread-9", &sink).unwrap();
        assert_eq!(report.inserted, 1);
        assert_eq!(*sink.0.borrow(), 1);
        // last-sync (freshness basis) is unchanged — on-demand is not a background sync.
        assert_eq!(rt.registry().freshness_ms(Service::Gmail, 5_000), before);
    }

    #[test]
    fn on_demand_transport_failure_turns_service_amber() {
        let mut rt = runtime(vec![], Some("token expired".into()));
        rt.mark_connected(Service::Gmail, 1_000);
        let sink = CountingSink(RefCell::new(0));
        assert!(rt.fetch_on_demand(Service::Gmail, "q", &sink).is_err());
        assert!(rt.registry().state(Service::Gmail).is_amber());
    }

    #[test]
    fn transport_failure_turns_service_amber_only() {
        let mut rt = runtime(vec![], Some("token refresh failed".into()));
        rt.mark_connected(Service::Gmail, 100);
        rt.mark_connected(Service::GoogleCalendar, 100);
        let sink = CountingSink(RefCell::new(0));
        assert!(rt.sync_service(Service::Gmail, 200, &sink).is_err());
        assert!(rt.registry().state(Service::Gmail).is_amber());
        // isolation: calendar is untouched (FR-INT-06)
        assert!(!rt.registry().state(Service::GoogleCalendar).is_amber());
        assert_eq!(*sink.0.borrow(), 0, "nothing ingested on failure");
    }

    #[test]
    fn disconnected_service_is_denied_without_state_change() {
        let mut rt = runtime(vec![item("x")], None);
        // never connected
        let sink = CountingSink(RefCell::new(0));
        let err = rt.sync_service(Service::Gmail, 200, &sink).unwrap_err();
        assert!(matches!(err, SyncFailure::Denied(_)));
        assert_eq!(rt.registry().state(Service::Gmail), ConnState::Disconnected);
    }

    #[test]
    fn services_due_respects_release_connection_and_interval() {
        let mut rt = runtime(vec![], None);
        // Slack is Wave 2 — never due at Wave 1 even if we pretend it connected.
        rt.mark_connected(Service::Gmail, 1_000);
        rt.mark_connected(Service::GoogleCalendar, 1_000);
        // Within the interval of the last sync → not due.
        assert!(rt.services_due(2_000, DEFAULT_SYNC_INTERVAL_MS).is_empty());
        // After the interval elapses, both Google services are due; Slack (unreleased) is not.
        let due = rt.services_due(1_000 + DEFAULT_SYNC_INTERVAL_MS + 1, DEFAULT_SYNC_INTERVAL_MS);
        assert!(due.contains(&Service::Gmail) && due.contains(&Service::GoogleCalendar));
        assert!(!due.contains(&Service::Slack));
    }

    struct FakeExec(RefCell<Option<(Service, String)>>);
    impl WriteExecutor for FakeExec {
        fn execute(&self, service: Service, op_name: &str, _args: Value) -> Result<Value, String> {
            *self.0.borrow_mut() = Some((service, op_name.to_string()));
            Ok(serde_json::json!({ "ok": true }))
        }
    }

    #[test]
    fn execute_write_runs_authorized_op_and_refuses_composio_send() {
        let mut rt = runtime(vec![], None);
        rt.set_draft_stop(false); // so Gmail send resolves to Composio, not DraftStop
        rt.mark_connected(Service::Gmail, 1_000);
        rt.mark_connected(Service::GoogleCalendar, 1_000);
        let exec = FakeExec(RefCell::new(None));
        // An L3 calendar create is authorized → executes.
        rt.execute_write(Service::GoogleCalendar, "event_create", serde_json::json!({}), &exec).unwrap();
        // execute_write forwards the scope op name; the transport maps it to the MCP tool.
        assert_eq!(exec.0.borrow().as_ref().unwrap(), &(Service::GoogleCalendar, "event_create".to_string()));
        // Gmail send is Composio → refused at the first layer.
        let err = rt.execute_write(Service::Gmail, "send", serde_json::json!({}), &exec).unwrap_err();
        assert!(err.contains("Composio"));
    }
}
