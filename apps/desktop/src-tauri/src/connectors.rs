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
    use shogun_core::mcp_http::{HttpMcpRpc, HttpTokenExchange};
    use shogun_integrations::keychain::KeychainTokenStore;
    use shogun_integrations::oauth::AuthConfig;
    use shogun_integrations::oauth_flow::run_loopback_flow;
    use shogun_integrations::runtime::{ConnectorRuntime, DEFAULT_SYNC_INTERVAL_MS};
    use shogun_integrations::token::{ManagedTokenProvider, TokenStore};
    use shogun_integrations::RemoteMcpTransport;
    use shogun_mcp::scope::{from_source, Wave};

    /// Same Keychain "service" field as the BYOK key (inline_source.rs) — one SHOGUN namespace.
    const KEYCHAIN_SERVICE: &str = "com.selectkk.shogun";

    /// The concrete transport stack: HTTPS MCP calls whose token is auto-refreshed from the Keychain.
    type Provider = ManagedTokenProvider<HttpTokenExchange, KeychainTokenStore>;
    type Transport = RemoteMcpTransport<HttpMcpRpc<Provider>>;
    /// The runtime owned by the app (behind a Mutex; the poller and the commands share it).
    pub struct ConnectorState(pub Arc<Mutex<ConnectorRuntime<Transport>>>);

    /// Read the Google OAuth client from the env. `redirect_uri` is filled per-connect (loopback
    /// port); refresh does not use it, so a placeholder is fine for the long-lived provider config.
    fn auth_config(redirect_uri: &str) -> Result<AuthConfig, String> {
        let client_id = std::env::var("SHOGUN_GOOGLE_CLIENT_ID")
            .map_err(|_| "SHOGUN_GOOGLE_CLIENT_ID not set".to_string())?;
        let client_secret = std::env::var("SHOGUN_GOOGLE_CLIENT_SECRET").ok();
        Ok(AuthConfig::google(client_id, client_secret, redirect_uri))
    }

    /// Build the runtime with a Keychain-backed, auto-refreshing transport. Wave::One is the current
    /// rollout (Gmail + Calendar + Drive); `draft_stop` comes from settings (default on).
    pub fn build_runtime(draft_stop: bool) -> Result<ConnectorRuntime<Transport>, String> {
        let cfg = auth_config("http://127.0.0.1/callback")?; // placeholder; refresh ignores it
        let provider = ManagedTokenProvider::new(cfg, HttpTokenExchange::new()?, KeychainTokenStore::new(KEYCHAIN_SERVICE));
        let transport = RemoteMcpTransport::new(HttpMcpRpc::new(provider)?);
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
    /// NOTE (rough): this is a synchronous command, so it blocks its thread while the user completes
    /// consent in the browser. For production, make it `async` and run the flow via
    /// `tauri::async_runtime::spawn_blocking` (cloning the `Arc`/`Db` out of `State` first) so the UI
    /// stays responsive during the wait.
    #[tauri::command]
    pub fn connect_service(
        service: String,
        state: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
    ) -> Result<(), String> {
        let svc = from_source(&service).ok_or_else(|| format!("unknown service: {service}"))?;
        let scopes = shogun_integrations::endpoints::endpoint(svc)
            .ok_or_else(|| format!("{service} has no first-layer MCP endpoint"))?
            .scopes;

        // Bind the loopback listener first so the real port goes into the redirect URI.
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("loopback bind failed: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let redirect = format!("http://127.0.0.1:{port}/callback");
        let cfg = auth_config(&redirect)?;

        let exchange = HttpTokenExchange::new()?;
        let tokens = run_loopback_flow(&cfg, scopes, &listener, db.now_ms(), &exchange)?;

        // Persist to the Keychain (invariant 7) and flip the connection state to Connected.
        KeychainTokenStore::new(KEYCHAIN_SERVICE).save(svc, &tokens)?;
        state.0.lock().map_err(|_| "runtime lock poisoned".to_string())?.mark_connected(svc, db.now_ms());
        eprintln!("[connectors] connected {service}");
        Ok(())
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
