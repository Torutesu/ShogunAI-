//! First-layer connector management: the macOS adapter that lets the user connect / disconnect a
//! service and drives the 15-minute read-sync (§6.9, FR-INT-03/04/06/07).
//!
//! ROUGH / macOS-only: this is the "connect a service and have it sync" wiring the product needs,
//! built at all levels so the flow is exercisable. It cannot compile on Linux CI (Keychain +
//! network + browser), and the visual side is placeholder — polish is a later pass. The decision
//! logic it calls (gate, token refresh, mapping, normalization) is the Linux-tested
//! `shogun-integrations` crate; this file is only the effectful glue + Tauri commands.
//!
//! **Transport**: Gmail reads and drafts are now routed through Composio (`ComposioReadRpc` backed
//! by `HttpComposioApi`), replacing the former direct Gmail REST path. Credentials (API key +
//! user_id) are loaded from the Keychain / `composio.json` on each `build_runtime` call.
#![allow(dead_code)]

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tauri::Manager;
    use shogun_core::composio_read::ComposioReadRpc;
    use shogun_core::composio_send::HttpComposioApi;
    use shogun_core::daemon::Db;
    use shogun_core::llm::traceability::{Route, TraceRecord, TraceabilitySink};
    use shogun_integrations::runtime::{ConnectorRuntime, DEFAULT_SYNC_INTERVAL_MS};
    use shogun_integrations::RemoteMcpTransport;
    use shogun_mcp::scope::{from_source, Wave};

    use crate::approvals::mac::{composio_api_key, load_composio_policy};

    /// Same Keychain "service" field used across all SHOGUN secrets.
    const KEYCHAIN_SERVICE: &str = "com.selectkk.shogun";

    /// The concrete transport stack: Composio-backed Gmail read+draft transport.
    pub type Transport = RemoteMcpTransport<ComposioReadRpc<HttpComposioApi>>;
    /// The concrete connector runtime (shared by the poller, the connector commands, and the
    /// approval-queue send executor).
    pub type Runtime = ConnectorRuntime<Transport>;
    /// The runtime owned by the app (behind a Mutex; the poller and the commands share it).
    pub struct ConnectorState(pub Arc<Mutex<Runtime>>);

    /// Build the runtime from current Composio credentials. Reads the API key from the Keychain
    /// and the user_id from `composio.json`. If either is absent the runtime still starts — reads
    /// will fail gracefully (content-free error) until the user configures the credentials.
    ///
    /// `app` is needed to locate `composio.json` via `app_data_dir`.
    pub fn build_runtime(
        app: &tauri::AppHandle,
        draft_stop: bool,
    ) -> Result<ConnectorRuntime<Transport>, String> {
        let api_key = composio_api_key().unwrap_or_default();
        let user_id = {
            let p = load_composio_policy(app);
            if !p.user_id.trim().is_empty() {
                p.user_id.clone()
            } else {
                std::env::var("SHOGUN_COMPOSIO_USER_ID").unwrap_or_default()
            }
        };
        let api = HttpComposioApi::new(api_key)?;
        let rpc = ComposioReadRpc::new(api, user_id);
        let transport = RemoteMcpTransport::new(rpc);
        Ok(ConnectorRuntime::new(transport, Wave::One, draft_stop))
    }

    /// Rebuild the connector runtime from the current credentials and swap it under the lock.
    ///
    /// Called after `set_composio_key` or `set_composio_user_id` successfully persist new creds so
    /// the live transport immediately picks them up without restarting the app.
    pub(crate) fn rebuild_gmail_runtime(
        state: &ConnectorState,
        app: &tauri::AppHandle,
        draft_stop: bool,
    ) -> Result<(), String> {
        let new_runtime = build_runtime(app, draft_stop)?;
        let mut rt = state.0.lock().map_err(|_| "runtime lock poisoned".to_string())?;
        *rt = new_runtime;
        eprintln!("[connectors] gmail runtime rebuilt with updated credentials");
        Ok(())
    }

    /// The 15-minute read-sync poller (FR-INT-04). Owns clones of the runtime + Db, syncs every due
    /// service, and lets each service fail independently to amber (FR-INT-06).
    ///
    /// Skips the sync when `consent_acknowledged` is false in the Composio policy — no data leaves
    /// to a third party without the user's explicit opt-in. Records a traceability entry on each
    /// successful sync to mark the third-party read boundary (FR-TR-03).
    pub fn spawn_sync_poller(state: Arc<Mutex<ConnectorRuntime<Transport>>>, db: Db, app: tauri::AppHandle) {
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(15 * 60));

            // Gate: skip the sync if the user has not granted Composio consent.
            let policy = load_composio_policy(&app);
            if !policy.consent_acknowledged {
                eprintln!("[connectors] gmail sync skipped — Composio consent not granted");
                continue;
            }

            let now = db.now_ms();
            if let Ok(mut rt) = state.lock() {
                // Plan gate (issue #97): refresh the entitlements before each tick so the service
                // gate sees the current plan — an expired trial stops the read-sync (first-layer
                // reads are Standard-and-up; expired has no active plan).
                rt.set_plan(crate::entitlement::mac::current(&app));
                for (svc, res) in rt.poll_tick(now, DEFAULT_SYNC_INTERVAL_MS, &db) {
                    match res {
                        Ok(rep) => {
                            eprintln!(
                                "[connectors] {} synced (+{} new)",
                                svc.source_str(),
                                rep.inserted
                            );
                            // Record that a third-party (Composio) read happened for this service.
                            // We record THAT it happened, not what was fetched — body text never
                            // reaches storage (invariant 3 / FR-TR-03). Empty chunk: zero bytes,
                            // deterministic digest of the empty string.
                            db.traceability_sink().record(TraceRecord::for_chunk(
                                Route::Composio,
                                "gmail_read",
                                "gmail",
                                "",
                                true,
                            ));
                            // context_updated（#61）: read-sync 完了を匿名計測。
                            if let Some(analytics) = app.try_state::<crate::analytics::Analytics>() {
                                analytics.capture(
                                    "context_updated",
                                    crate::analytics::context_updated_props(svc.source_str(), rep.inserted as u64),
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("[connectors] {} sync failed: {e:?}", svc.source_str());
                        }
                    }
                }
            }
        });
    }

    // ------------------------------------------------------------- commands

    /// Whether the wired transport can actually serve this service. The live transport is
    /// `ComposioReadRpc`, which serves **Gmail only** — letting Calendar/Drive "connect" would put
    /// them on the 15-minute poller, where every tick errors and flips them to amber
    /// ("Needs reauth" for a service the user never mis-authed). Until a real Calendar/Drive
    /// transport exists they are presented as coming soon.
    fn transport_serves(svc: shogun_mcp::scope::Service) -> bool {
        matches!(svc, shogun_mcp::scope::Service::Gmail)
    }

    /// List every service's connection status for the connections screen.
    #[tauri::command]
    pub fn connectors_list(
        state: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
    ) -> Result<Vec<shogun_integrations::ServiceStatus>, String> {
        let now = db.now_ms();
        let rt = state.0.lock().map_err(|_| "runtime lock poisoned".to_string())?;
        let mut rows = rt.statuses(now);
        for row in &mut rows {
            // Released by wave, but not reachable through the wired transport → "Coming soon",
            // not a Connect button that can only end in a false amber.
            if !transport_serves(row.service) {
                row.state = shogun_integrations::ConnUi::ComingSoon;
            }
        }
        Ok(rows)
    }

    /// Connect a service. For the Composio-backed Gmail transport "connect" means the user has
    /// configured the API key and user_id (via `set_composio_key` / `set_composio_user_id`) and the
    /// runtime is rebuilt from those creds. For Wave-2/3 services (Slack, Notion, GitHub, Linear)
    /// the loopback OAuth flow is still the right path — but those are not yet live (Wave 1 = Gmail
    /// + Calendar + Drive via Composio for Gmail; Calendar/Drive are future work).
    ///
    /// For now we mark the service connected in the runtime state to allow the poller and UI to
    /// reflect it.
    #[tauri::command]
    pub async fn connect_service(
        service: String,
        state: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
    ) -> Result<(), String> {
        let svc = from_source(&service).ok_or_else(|| format!("unknown service: {service}"))?;
        if !transport_serves(svc) {
            return Err(format!("{service} is not available yet"));
        }
        let now = db.now_ms();
        state
            .0
            .lock()
            .map_err(|_| "runtime lock poisoned".to_string())?
            .mark_connected(svc, now);
        eprintln!("[connectors] connected {service} (Composio transport)");
        Ok(())
    }

    /// On-demand read of a specific item (§6.9 read_on_demand, L2): fetch it now and ingest into
    /// memory. Returns how many new items were ingested.
    #[tauri::command]
    pub fn fetch_on_demand(
        service: String,
        query: String,
        app: tauri::AppHandle,
        state: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
    ) -> Result<u64, String> {
        let svc = from_source(&service).ok_or_else(|| format!("unknown service: {service}"))?;
        if !transport_serves(svc) {
            return Err(format!("{service} is not available yet"));
        }
        // The same opt-in gate the sync poller applies (CLAUDE.md 連携実装ルール): an on-demand
        // fetch sends the user_id + query to Composio exactly like a poll does, so it must be
        // impossible without the user's explicit Composio consent.
        if !load_composio_policy(&app).consent_acknowledged {
            return Err("Composio consent has not been granted".into());
        }
        let mut rt = state.0.lock().map_err(|_| "runtime lock poisoned".to_string())?;
        // Plan gate (issue #97): refresh entitlements so the service gate sees the current plan
        // (reads are Standard-and-up; an expired trial is denied).
        rt.set_plan(crate::entitlement::mac::current(&app));
        match rt.fetch_on_demand(svc, &query, &*db) {
            Ok(report) => {
                // Same third-party (Composio) read boundary the sync poller records — an on-demand
                // fetch sends the user_id + query to Composio just as the poller does, so it must be
                // just as visible in the traceability screen. We record THAT a read happened, never
                // the query or fetched body (invariant 3 / FR-TR-03; empty chunk = zero bytes).
                db.traceability_sink().record(TraceRecord::for_chunk(
                    Route::Composio,
                    "gmail_read",
                    svc.source_str(),
                    "",
                    true,
                ));
                Ok(report.inserted as u64)
            }
            Err(e) => Err(format!("{e:?}")),
        }
    }

    /// Disconnect a service (FR-INT-07): stop syncing. Ingested events are kept by default.
    #[tauri::command]
    pub fn disconnect_service(
        service: String,
        state: tauri::State<'_, ConnectorState>,
    ) -> Result<(), String> {
        let svc = from_source(&service).ok_or_else(|| format!("unknown service: {service}"))?;
        state.0.lock().map_err(|_| "runtime lock poisoned".to_string())?.disconnect(svc, false);
        eprintln!("[connectors] disconnected {service}");
        Ok(())
    }
}
