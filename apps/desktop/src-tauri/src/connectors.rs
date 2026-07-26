//! First-layer connector management: the macOS adapter that lets the user connect / disconnect a
//! Google Workspace service and drives the 15-minute read-sync (§6.9, FR-INT-03/04/06/07).
//!
//! ROUGH / macOS-only: this is the "connect a service and have it sync" wiring the product needs,
//! built at all levels so the flow is exercisable. It cannot compile on Linux CI (Keychain +
//! network + browser), and the visual side is placeholder — polish is a later pass. The decision
//! logic it calls (gate, token refresh, mapping, normalization) is the Linux-tested
//! `shogun-integrations` crate; this file is only the effectful glue + Tauri commands.
//!
//! Prereqs (human): a Google OAuth "Desktop app" client (Developer Preview), its id/secret in the
//! env as `SHOGUN_GOOGLE_CLIENT_ID` / `SHOGUN_GOOGLE_CLIENT_SECRET`.
#![allow(dead_code)]

#[cfg(target_os = "macos")]
pub mod mac {
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use shogun_core::daemon::Db;
    // The reqwest clients live in shogun-core (the single allowlisted HTTP egress, FR-TR-03).
    use shogun_core::mcp_http::HttpTokenExchange;
    use shogun_integrations::keychain::KeychainTokenStore;
    use shogun_integrations::oauth::AuthConfig;
    use shogun_integrations::oauth_flow::run_loopback_flow;
    use shogun_integrations::runtime::{ConnectorRuntime, DEFAULT_SYNC_INTERVAL_MS};
    use shogun_integrations::token::{ManagedTokenProvider, TokenStore};
    use shogun_integrations::RemoteMcpTransport;
    use shogun_mcp::scope::{from_source, Service, Wave};

    /// Same Keychain "service" field as the BYOK key (inline_source.rs) — one SHOGUN namespace.
    const KEYCHAIN_SERVICE: &str = "com.selectkk.shogun";

    /// The concrete transport stack: HTTPS MCP calls whose token is auto-refreshed from the Keychain.
    pub type Provider = ManagedTokenProvider<HttpTokenExchange, KeychainTokenStore>;
    // 公式 MCP (Developer Preview) に依存せず、Gmail REST を直接叩く（設計 §2）。
    pub type Transport = RemoteMcpTransport<shogun_core::gmail_rest::GmailRestRpc<Provider>>;
    /// The concrete connector runtime (shared by the poller, the connector commands, and the
    /// approval-queue send executor).
    pub type Runtime = ConnectorRuntime<Transport>;
    /// The runtime owned by the app (behind a Mutex; the poller and the commands share it).
    pub struct ConnectorState(pub Arc<Mutex<Runtime>>);

    /// The OAuth client for one service, from the env — Google and Slack are different vendors
    /// with different endpoints and different registered apps, so the config MUST be per-service
    /// (a Slack refresh posting to Google's token endpoint would fail confusingly). `redirect_uri`
    /// is filled per-connect (loopback port); refresh does not use it.
    fn auth_config_for(svc: Service, redirect_uri: &str) -> Result<AuthConfig, String> {
        match svc {
            Service::Gmail | Service::GoogleCalendar | Service::GoogleDrive => {
                let id = std::env::var("SHOGUN_GOOGLE_CLIENT_ID")
                    .map_err(|_| "SHOGUN_GOOGLE_CLIENT_ID not set".to_string())?;
                let secret = std::env::var("SHOGUN_GOOGLE_CLIENT_SECRET").ok();
                Ok(AuthConfig::google(id, secret, redirect_uri))
            }
            Service::Slack => {
                let id = std::env::var("SHOGUN_SLACK_CLIENT_ID")
                    .map_err(|_| "SHOGUN_SLACK_CLIENT_ID not set".to_string())?;
                let secret = std::env::var("SHOGUN_SLACK_CLIENT_SECRET")
                    .map_err(|_| "SHOGUN_SLACK_CLIENT_SECRET not set".to_string())?;
                Ok(AuthConfig::slack(id, secret, redirect_uri))
            }
            other => Err(format!("{} has no OAuth client configured (Wave 3)", other.source_str())),
        }
    }

    /// Build the runtime with a Keychain-backed, auto-refreshing transport. Wave::One is the current
    /// rollout (Gmail + Calendar + Drive); `draft_stop` comes from settings (default on). The token
    /// provider selects the refresh config per service, so a future Wave-2 Slack token refreshes
    /// against Slack's endpoint, never Google's.
    pub fn build_runtime(draft_stop: bool) -> Result<ConnectorRuntime<Transport>, String> {
        let selector: shogun_integrations::token::ConfigSelector =
            Box::new(|svc| auth_config_for(svc, "http://127.0.0.1/callback").ok());
        let provider = ManagedTokenProvider::with_config_selector(
            selector,
            HttpTokenExchange::new()?,
            KeychainTokenStore::new(KEYCHAIN_SERVICE),
        );
        let transport = RemoteMcpTransport::new(
            shogun_core::gmail_rest::GmailRestRpc::new(provider)?,
        );
        Ok(ConnectorRuntime::new(transport, Wave::One, draft_stop))
    }

    /// The 15-minute read-sync poller (FR-INT-04). Owns clones of the runtime + Db, syncs every due
    /// service, and lets each service fail independently to amber (FR-INT-06).
    pub fn spawn_sync_poller(state: Arc<Mutex<ConnectorRuntime<Transport>>>, db: Db) {
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(15 * 60));
            let now = db.now_ms();
            if let Ok(mut rt) = state.lock() {
                for (svc, res) in rt.poll_tick(now, DEFAULT_SYNC_INTERVAL_MS, &db) {
                    match res {
                        Ok(rep) => eprintln!("[connectors] {} synced (+{} new)", svc.source_str(), rep.inserted),
                        Err(e) => eprintln!("[connectors] {} sync failed: {e:?}", svc.source_str()),
                    }
                }
            }
        });
    }

    // ------------------------------------------------------------- commands

    /// List every service's connection status for the connections screen. The return type derives
    /// `Serialize` in shogun-integrations, so Tauri serializes it directly (no serde_json here).
    #[tauri::command]
    pub fn connectors_list(
        state: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
    ) -> Result<Vec<shogun_integrations::ServiceStatus>, String> {
        let now = db.now_ms();
        let rt = state.0.lock().map_err(|_| "runtime lock poisoned".to_string())?;
        Ok(rt.statuses(now))
    }

    /// Connect a service: run the loopback OAuth+PKCE flow, persist the token to the Keychain, and
    /// mark it connected. `service` is the source id (`gmail` / `gcal` / `gdrive`).
    ///
    /// Async: the blocking flow (which waits for the user to finish consent in the browser) runs on a
    /// blocking thread via `spawn_blocking`, so the UI stays responsive. `State` isn't `Send`, so the
    /// `Arc`/`Db` handles are cloned out before the work moves onto that thread.
    #[tauri::command]
    pub async fn connect_service(
        service: String,
        state: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
    ) -> Result<(), String> {
        let svc = from_source(&service).ok_or_else(|| format!("unknown service: {service}"))?;
        let runtime = state.0.clone();
        let db = (*db).clone();
        tauri::async_runtime::spawn_blocking(move || connect_blocking(svc, &service, &runtime, &db))
            .await
            .map_err(|e| format!("connect task failed: {e}"))?
    }

    /// The blocking half of [`connect_service`]: bind the loopback listener, run the OAuth flow, save
    /// the token, and mark the service connected. Runs off the UI thread.
    fn connect_blocking(
        svc: Service,
        service: &str,
        runtime: &Arc<Mutex<ConnectorRuntime<Transport>>>,
        db: &Db,
    ) -> Result<(), String> {
        let scopes = shogun_integrations::endpoints::endpoint(svc)
            .ok_or_else(|| format!("{service} has no first-layer MCP endpoint"))?
            .scopes;

        // Bind the loopback listener first so the real port goes into the redirect URI.
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("loopback bind failed: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let redirect = format!("http://127.0.0.1:{port}/callback");
        let cfg = auth_config_for(svc, &redirect)?;

        // Diagnostic: the client_id is a PUBLIC value (it appears in the browser's authorize URL),
        // never a secret — logging it lets the user verify the env var has no typo/whitespace when
        // Google returns "invalid_client / OAuth client was not found". The client SECRET is never
        // logged (invariant 7).
        eprintln!(
            "[connectors] {service}: authorizing with client_id=[{}] (len {}), redirect={redirect}",
            cfg.client_id,
            cfg.client_id.len()
        );
        if !cfg.client_id.ends_with(".apps.googleusercontent.com") {
            eprintln!(
                "[connectors] ⚠️ client_id does not end with .apps.googleusercontent.com — check SHOGUN_GOOGLE_CLIENT_ID (wrong value or Client Secret pasted by mistake?)"
            );
        }

        let exchange = HttpTokenExchange::new()?;
        let tokens = run_loopback_flow(&cfg, scopes, &listener, db.now_ms(), &exchange)?;

        // Persist to the Keychain (invariant 7) and flip the connection state to Connected.
        KeychainTokenStore::new(KEYCHAIN_SERVICE).save(svc, &tokens)?;
        runtime.lock().map_err(|_| "runtime lock poisoned".to_string())?.mark_connected(svc, db.now_ms());
        eprintln!("[connectors] connected {service}");
        Ok(())
    }

    /// On-demand read of a specific item (§6.9 read_on_demand, L2): fetch it now and ingest into
    /// memory — e.g. the Gmail thread the user just opened. `query` is the item id/search string.
    /// The neutral primitive: the caller (a focus watcher, or a UI button) decides when to invoke
    /// it. Returns how many new items were ingested.
    #[tauri::command]
    pub fn fetch_on_demand(
        service: String,
        query: String,
        state: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
    ) -> Result<u64, String> {
        let svc = from_source(&service).ok_or_else(|| format!("unknown service: {service}"))?;
        let mut rt = state.0.lock().map_err(|_| "runtime lock poisoned".to_string())?;
        match rt.fetch_on_demand(svc, &query, &*db) {
            Ok(report) => Ok(report.inserted as u64),
            Err(e) => Err(format!("{e:?}")),
        }
    }

    /// Disconnect a service (FR-INT-07): delete the Keychain token and stop syncing. Ingested events
    /// are kept by default (the user can wipe them from the memory settings).
    #[tauri::command]
    pub fn disconnect_service(
        service: String,
        state: tauri::State<'_, ConnectorState>,
    ) -> Result<(), String> {
        let svc = from_source(&service).ok_or_else(|| format!("unknown service: {service}"))?;
        // Ignore a "not found" delete — disconnect must be idempotent.
        let _ = KeychainTokenStore::new(KEYCHAIN_SERVICE).delete(svc);
        state.0.lock().map_err(|_| "runtime lock poisoned".to_string())?.disconnect(svc, false);
        eprintln!("[connectors] disconnected {service}");
        Ok(())
    }
}
