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
    use std::sync::{Arc, Mutex, TryLockError};
    use std::time::Duration;

    use tauri::Manager;
    use shogun_core::composio_read::ComposioReadRpc;
    use shogun_core::composio_send::HttpComposioApi;
    use shogun_core::daemon::Db;
    use shogun_core::mcp_http::{HttpMcpRpc, HttpTokenExchange};
    use shogun_core::llm::traceability::{Route, TraceRecord, TraceabilitySink};
    use shogun_integrations::oauth::AuthConfig;
    use shogun_integrations::runtime::{ConnectorRuntime, DEFAULT_SYNC_INTERVAL_MS};
    use shogun_integrations::token::ManagedTokenProvider;
    use shogun_integrations::keychain_store::SERVICE as KEYCHAIN_SERVICE_NAME;
    use shogun_integrations::KeychainTokenStore;
    use shogun_integrations::RemoteMcpTransport;
    use shogun_integrations::DispatchRpc;
    use shogun_mcp::scope::{from_source, Wave};

    use crate::approvals::mac::{composio_api_key, load_composio_policy};

    /// Same Keychain "service" field used across all SHOGUN secrets.

    /// The concrete transport stack: Composio-backed Gmail read+draft transport.
    type OfficialProvider = ManagedTokenProvider<HttpTokenExchange, KeychainTokenStore>;
    type OfficialRpc = HttpMcpRpc<OfficialProvider>;
    type Rpc = DispatchRpc<ComposioReadRpc<HttpComposioApi>, OfficialRpc>;
    pub type Transport = RemoteMcpTransport<Rpc>;
    /// The concrete connector runtime (shared by the poller, the connector commands, and the
    /// approval-queue send executor).
    pub type Runtime = ConnectorRuntime<Transport>;
    /// The runtime owned by the app (behind a Mutex; the poller and the commands share it).
    pub struct ConnectorState(pub Arc<Mutex<Runtime>>);

    fn with_runtime<R>(state: &Arc<Mutex<Runtime>>, f: impl FnOnce(&mut Runtime) -> R) -> Result<R, String> {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match state.try_lock() {
                Ok(mut guard) => return Ok(f(&mut guard)),
                Err(TryLockError::Poisoned(_)) => return Err("runtime lock poisoned".into()),
                Err(TryLockError::WouldBlock) if std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                Err(TryLockError::WouldBlock) => return Err("runtime busy; retry connector operation".into()),
            }
        }
    }

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
            } else { String::new() }
        };
        let api = HttpComposioApi::new(api_key)?;
        let composio = ComposioReadRpc::new(api, user_id);
        let exchange = HttpTokenExchange::new()?;
        let provider = ManagedTokenProvider::with_config_selector(
            Box::new(|service| google_oauth_config(service, None)),
            exchange,
            KeychainTokenStore::new(KEYCHAIN_SERVICE_NAME),
        );
        let official = HttpMcpRpc::new(provider)?;
        let transport = RemoteMcpTransport::new(DispatchRpc::new(composio, official));
        Ok(ConnectorRuntime::new(transport, Wave::One, draft_stop))
    }

    fn google_oauth_config(
        service: shogun_mcp::scope::Service,
        redirect_uri: Option<String>,
    ) -> Option<AuthConfig> {
        if !matches!(service, shogun_mcp::scope::Service::GoogleCalendar | shogun_mcp::scope::Service::GoogleDrive) {
            return None;
        }
        let client_id = shogun_integrations::keychain_store::get_generic_secret(
            shogun_integrations::keychain_store::GOOGLE_OAUTH_CLIENT_ID_ACCOUNT,
        ).ok().and_then(|bytes| String::from_utf8(bytes).ok())?;
        if client_id.trim().is_empty() { return None; }
        let client_secret = shogun_integrations::keychain_store::get_generic_secret(
            shogun_integrations::keychain_store::GOOGLE_OAUTH_CLIENT_SECRET_ACCOUNT,
        ).ok().and_then(|bytes| String::from_utf8(bytes).ok()).filter(|s| !s.trim().is_empty());
        let redirect = redirect_uri.unwrap_or_else(|| "http://127.0.0.1:0/callback".to_string());
        Some(AuthConfig::google(client_id.trim(), client_secret, redirect))
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
            let now = db.now_ms();
            let prepared = if let Ok(rt) = state.try_lock() {
                let due = rt.services_due(now, DEFAULT_SYNC_INTERVAL_MS).into_iter().filter(|svc| {
                    *svc != shogun_mcp::scope::Service::Gmail || policy.consent_acknowledged
                }).collect::<Vec<_>>();
                due.into_iter().map(|svc| (svc, rt.prepare_sync(svc))).collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            for (svc, prepared) in prepared {
                let res = prepared.and_then(|ticket| ticket.run(&db));
                if let Ok(mut rt) = state.try_lock() {
                    rt.apply_sync_result(svc, now, &res);
                }
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
                            let third_party = svc == shogun_mcp::scope::Service::Gmail;
                            db.traceability_sink().record(TraceRecord::for_chunk(
                                if third_party { Route::Composio } else { Route::Mcp },
                                if third_party { "gmail_read" } else { "integration_read" },
                                svc.source_str(), "", third_party,
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
        });
    }

    // ------------------------------------------------------------- commands

    /// List every service's connection status for the connections screen.
    #[tauri::command]
    pub fn connectors_list(
        state: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
    ) -> Result<Vec<shogun_integrations::ServiceStatus>, String> {
        let now = db.now_ms();
        let rt = state.0.lock().map_err(|_| "runtime lock poisoned".to_string())?;
        Ok(rt.statuses(now))
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
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        let svc = from_source(&service).ok_or_else(|| format!("unknown service: {service}"))?;
        let now = db.now_ms();
        let runtime = state.0.clone();
        tokio::task::spawn_blocking(move || connect_service_blocking(svc, runtime, now, app))
            .await
            .map_err(|_| "connector connect task failed".to_string())??;
        Ok(())
    }

    fn connect_service_blocking(
        service: shogun_mcp::scope::Service,
        runtime: Arc<Mutex<Runtime>>,
        now_ms: i64,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        if !service.is_released(Wave::One) {
            return Err(format!("{} is unreleased at Wave One", service.source_str()));
        }
        if service == shogun_mcp::scope::Service::Gmail {
            let policy = load_composio_policy(&app);
            if !policy.consent_acknowledged {
                return Err("Gmail Composio consent is required".to_string());
            }
            if policy.user_id.trim().is_empty() {
                return Err("Gmail Composio user ID is not configured".to_string());
            }
            if composio_api_key().filter(|key| !key.trim().is_empty()).is_none() {
                return Err("Gmail Composio API key is not configured".to_string());
            }
            runtime
                .lock()
                .map_err(|_| "runtime lock poisoned".to_string())?
                .mark_connected(service, now_ms);
        } else {
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
                .map_err(|_| "OAuth loopback bind failed".to_string())?;
            let port = listener.local_addr().map_err(|_| "OAuth loopback address failed".to_string())?.port();
            let cfg = google_oauth_config(service, Some(format!("http://127.0.0.1:{port}/callback")))
                .ok_or_else(|| "Google OAuth client configuration is missing".to_string())?;
            let endpoint = shogun_integrations::endpoints::endpoint(service)
                .ok_or_else(|| "official MCP endpoint is unavailable".to_string())?;
            let tokens = shogun_integrations::oauth_flow::run_loopback_flow(
                &cfg, endpoint.scopes, &listener, shogun_integrations::token::system_now_ms(), &HttpTokenExchange::new()?
            )?;
            let store = KeychainTokenStore::new(KEYCHAIN_SERVICE_NAME);
            shogun_integrations::TokenStore::save(&store, service, &tokens)?;
            let capability_transport = match runtime.lock() {
                Ok(rt) => rt,
                Err(_) => {
                    let _ = shogun_integrations::TokenStore::delete(&store, service);
                    return Err("runtime lock poisoned".to_string());
                }
            }.capability_transport(service);
            if let Err(error) = capability_transport.validate_capabilities(service) {
                let _ = shogun_integrations::TokenStore::delete(&store, service);
                return Err(error);
            }
            runtime.lock().map_err(|_| "runtime lock poisoned".to_string())?.mark_connected(service, now_ms);
        }
        Ok(())
    }

    /// On-demand read of a specific item (§6.9 read_on_demand, L2): fetch it now and ingest into
    /// memory. Returns how many new items were ingested.
    #[tauri::command]
    pub fn fetch_on_demand(
        service: String,
        query: String,
        state: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
        app: tauri::AppHandle,
    ) -> Result<u64, String> {
        let svc = from_source(&service).ok_or_else(|| format!("unknown service: {service}"))?;
        if svc == shogun_mcp::scope::Service::Gmail && !load_composio_policy(&app).consent_acknowledged {
            return Err("Gmail Composio consent is required".into());
        }
        let prepared = with_runtime(&state.0, |rt| rt.prepare_on_demand(svc, query.clone()))?
            .map_err(|error| format!("{error:?}"))?;
        let result = prepared.run(&*db);
        if let Ok(mut rt) = state.0.lock() {
            rt.apply_on_demand_result(svc, &result);
        }
        match result {
            Ok(report) => {
                // Same third-party (Composio) read boundary the sync poller records — an on-demand
                // fetch sends the user_id + query to Composio just as the poller does, so it must be
                // just as visible in the traceability screen. We record THAT a read happened, never
                // the query or fetched body (invariant 3 / FR-TR-03; empty chunk = zero bytes).
                let third_party = svc == shogun_mcp::scope::Service::Gmail;
                db.traceability_sink().record(TraceRecord::for_chunk(
                    if third_party { Route::Composio } else { Route::Mcp },
                    if third_party { "gmail_read" } else { "integration_read" },
                    svc.source_str(), "", third_party,
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
        let store = KeychainTokenStore::new(KEYCHAIN_SERVICE_NAME);
        shogun_integrations::TokenStore::delete(&store, svc)?;
        with_runtime(&state.0, |rt| rt.disconnect(svc, false))?;
        eprintln!("[connectors] disconnected {service}");
        Ok(())
    }
}
