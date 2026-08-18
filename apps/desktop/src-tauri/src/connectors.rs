//! First-layer connector management: the macOS adapter that lets the user connect / disconnect a
//! service and drives the 15-minute read-sync (§6.9, FR-INT-03/04/06/07).
//!
//! macOS-only: this is the "connect a service and have it sync" wiring. It cannot compile on Linux
//! CI (Keychain + network + browser). The decision logic it calls (gate, token refresh, mapping,
//! normalization, OAuth flow, failure taxonomy) is the Linux-tested `shogun-integrations` crate;
//! this file is only the effectful glue + Tauri commands.
//!
//! **Transport routing** (plan B-4):
//! - Gmail reads and drafts go through Composio (`ComposioReadRpc` backed by `HttpComposioApi`) —
//!   the 2026-07 product decision. Its "connect" is credential+consent verification, no OAuth.
//! - Google Calendar / Drive go through the official first-layer MCP servers (`HttpMcpRpc` with a
//!   Keychain-backed auto-refreshing token provider). Their "connect" is the real OAuth 2.1 PKCE
//!   loopback flow (`oauth_flow::run_loopback_flow`): browser consent → loopback redirect → token
//!   exchange → Keychain persist (invariant 7) → only then `mark_connected`. They stay "Coming
//!   soon" unless `SHOGUN_ENABLE_WAVE1_READ` opts them in (live-verification switch, no rebuild).
#![allow(dead_code)]

#[cfg(target_os = "macos")]
pub mod mac {
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use serde_json::Value;
    use shogun_core::composio_read::ComposioReadRpc;
    use shogun_core::composio_send::HttpComposioApi;
    use shogun_core::daemon::Db;
    use shogun_core::llm::traceability::{Route, TraceRecord, TraceabilitySink};
    use shogun_core::mcp_http::{HttpMcpRpc, HttpTokenExchange};
    use shogun_integrations::connect::{self, ConnectError};
    use shogun_integrations::keychain_store;
    use shogun_integrations::oauth_flow;
    use shogun_integrations::rpc::McpRpc;
    use shogun_integrations::runtime::{ConnectorRuntime, DEFAULT_SYNC_INTERVAL_MS};
    use shogun_integrations::token::{ConfigSelector, ManagedTokenProvider, TokenStore};
    use shogun_integrations::{AuthConfig, KeychainTokenStore, RemoteMcpTransport};
    use shogun_mcp::scope::{from_source, Service, Wave};
    use tauri::Manager;

    use crate::approvals::mac::{composio_api_key, load_composio_policy};

    /// Same Keychain "service" field used across all SHOGUN secrets.
    const KEYCHAIN_SERVICE: &str = "com.selectkk.shogun";
    /// Keychain accounts for the user-provided Google Desktop OAuth client. The client secret is
    /// optional for PKCE, but when supplied it is still a secret and must never leave Keychain.
    const GOOGLE_OAUTH_CLIENT_ID_ACCOUNT: &str = "google-oauth-client-id";
    const GOOGLE_OAUTH_CLIENT_SECRET_ACCOUNT: &str = "google-oauth-client-secret";

    /// How long the interactive OAuth connect waits for the browser redirect before giving up.
    const CONNECT_TIMEOUT: Duration = Duration::from_secs(300);

    /// The Keychain-backed, auto-refreshing token provider for first-layer MCP calls.
    type FirstLayerTokens = ManagedTokenProvider<HttpTokenExchange, KeychainTokenStore>;

    /// Routes each service to the transport that actually serves it: Gmail → Composio (the 2026-07
    /// decision), everything else → the official first-layer MCP client (present only when a
    /// Google OAuth client is configured in the environment).
    pub struct RoutedReadRpc {
        composio: ComposioReadRpc<HttpComposioApi>,
        first_layer: Option<HttpMcpRpc<FirstLayerTokens>>,
    }

    impl McpRpc for RoutedReadRpc {
        fn call_tool(
            &self,
            service: Service,
            tool: &str,
            arguments: Value,
        ) -> Result<Value, String> {
            if service == Service::Gmail {
                return self.composio.call_tool(service, tool, arguments);
            }
            match &self.first_layer {
                Some(rpc) => rpc.call_tool(service, tool, arguments),
                None => Err(format!(
                    "no first-layer MCP client for {} (Google OAuth client not configured)",
                    service.source_str()
                )),
            }
        }
    }

    /// The concrete transport stack: Composio-backed Gmail + first-layer MCP for the rest.
    pub type Transport = RemoteMcpTransport<RoutedReadRpc>;
    /// The concrete connector runtime (shared by the poller, the connector commands, and the
    /// approval-queue send executor).
    pub type Runtime = ConnectorRuntime<Transport>;
    /// The runtime owned by the app (behind a Mutex; the poller and the commands share it).
    pub struct ConnectorState(pub Arc<Mutex<Runtime>>);

    fn nonempty(value: String) -> Option<String> {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    }

    /// Prefer one complete Keychain client over the developer-only environment fallback. Never mix
    /// the two sources: rotating the saved client must not accidentally pair it with an old shell
    /// secret. This is pure so the precedence rule stays testable without macOS Keychain access.
    fn select_google_client(
        keychain_id: Option<String>,
        keychain_secret: Option<String>,
        env_id: Option<String>,
        env_secret: Option<String>,
    ) -> Option<(String, Option<String>)> {
        keychain_id
            .map(|id| (id, keychain_secret))
            .or_else(|| env_id.map(|id| (id, env_secret)))
    }

    /// Google OAuth config from Keychain first, then the development environment fallback. Values
    /// are intentionally never logged or returned to the webview.
    fn google_client_config(service: Service) -> Result<(String, Option<String>), ConnectError> {
        let keychain_id = keychain_store::get_generic_secret(GOOGLE_OAUTH_CLIENT_ID_ACCOUNT)
            .ok()
            .and_then(|value| String::from_utf8(value).ok())
            .and_then(nonempty);
        let keychain_secret =
            keychain_store::get_generic_secret(GOOGLE_OAUTH_CLIENT_SECRET_ACCOUNT)
                .ok()
                .and_then(|value| String::from_utf8(value).ok())
                .and_then(nonempty);
        let env_id = std::env::var(connect::GOOGLE_CLIENT_ID_ENV)
            .ok()
            .and_then(nonempty);
        let env_secret = std::env::var(connect::GOOGLE_CLIENT_SECRET_ENV)
            .ok()
            .and_then(nonempty);
        select_google_client(keychain_id, keychain_secret, env_id, env_secret).ok_or_else(|| {
            ConnectError::MissingClientConfig {
                service: service.source_str(),
            }
        })
    }

    /// Build the first-layer MCP client, if a Google OAuth client is configured. Tokens come from
    /// the Keychain and auto-refresh via the Google token endpoint; the redirect URI placeholder is
    /// never used by a refresh (only the interactive connect binds a real loopback port).
    fn build_first_layer_rpc() -> Option<HttpMcpRpc<FirstLayerTokens>> {
        let (id, secret) = google_client_config(Service::GoogleCalendar).ok()?;
        let cfg = AuthConfig::google(id, secret, "http://127.0.0.1/callback");
        let selector: ConfigSelector = Box::new(move |svc| match svc {
            // Google services share the one Google OAuth client; refresh never crosses vendors.
            Service::Gmail | Service::GoogleCalendar | Service::GoogleDrive => Some(cfg.clone()),
            _ => None,
        });
        let exchange = match HttpTokenExchange::new() {
            Ok(x) => x,
            Err(e) => {
                eprintln!("[connectors] first-layer token exchange unavailable: {e}");
                return None;
            }
        };
        let provider = ManagedTokenProvider::with_config_selector(
            selector,
            exchange,
            KeychainTokenStore::new(KEYCHAIN_SERVICE),
        );
        match HttpMcpRpc::new(provider) {
            Ok(rpc) => Some(rpc),
            Err(e) => {
                eprintln!("[connectors] first-layer MCP client unavailable: {e}");
                None
            }
        }
    }

    /// Build the runtime from current credentials. Gmail: Composio API key from the Keychain +
    /// user_id from `composio.json`. Calendar/Drive: the first-layer MCP client when a Google OAuth
    /// client is configured. If credentials are absent the runtime still starts — reads fail
    /// gracefully (content-free error) until the user configures them.
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
        let rpc = RoutedReadRpc {
            composio: ComposioReadRpc::new(api, user_id),
            first_layer: build_first_layer_rpc(),
        };
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
        let mut rt = state
            .0
            .lock()
            .map_err(|_| "runtime lock poisoned".to_string())?;
        *rt = new_runtime;
        eprintln!("[connectors] gmail runtime rebuilt with updated credentials");
        Ok(())
    }

    fn read_keychain_secret(account: &str) -> Option<Vec<u8>> {
        keychain_store::get_generic_secret(account).ok()
    }

    fn delete_keychain_secret_if_present(account: &str) -> Result<(), String> {
        match keychain_store::delete_generic_secret(account) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == -25300 /* errSecItemNotFound */ => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }

    /// Restore one Keychain entry after a later write/delete failed. Values stay opaque bytes so
    /// this helper can never accidentally decode or log an OAuth secret or token.
    fn restore_keychain_secret(account: &str, previous: Option<&[u8]>) -> Result<(), String> {
        match previous {
            Some(value) => keychain_store::set_generic_secret(account, value)
                .map_err(|error| error.to_string()),
            None => delete_keychain_secret_if_present(account),
        }
    }

    fn google_token_account(service: Service) -> String {
        format!("{}-tokenset", service.source_str())
    }

    /// Credential rotation invalidates Calendar and Drive grants. Deleting the token entries and
    /// replacing the runtime happens together while its lock is held, so no request can refresh a
    /// token under a now-invalid OAuth client. If a Keychain delete fails, restore all token entries
    /// changed so far and leave the old runtime in place.
    fn rebuild_for_google_oauth_change(
        state: &ConnectorState,
        app: &tauri::AppHandle,
    ) -> Result<(), String> {
        let draft_stop = load_composio_policy(app).draft_stop;
        let replacement = build_runtime(app, draft_stop)?;
        let services = [Service::GoogleCalendar, Service::GoogleDrive];
        let snapshots = services.map(|service| {
            let account = google_token_account(service);
            let value = read_keychain_secret(&account);
            (account, value)
        });

        let mut runtime = state
            .0
            .lock()
            .map_err(|_| "runtime lock poisoned".to_string())?;
        for (account, _) in &snapshots {
            if let Err(error) = delete_keychain_secret_if_present(account) {
                for (changed_account, previous) in &snapshots {
                    if changed_account == account {
                        break;
                    }
                    let _ = restore_keychain_secret(changed_account, previous.as_deref());
                }
                return Err(format!("could not clear old Google authorization: {error}"));
            }
        }
        *runtime = replacement;
        eprintln!("[connectors] Google OAuth client changed; Calendar and Drive disconnected");
        Ok(())
    }

    /// Presence-only view for Settings. Client values are never sent across IPC, including the
    /// nominally-public client id, because it identifies the user's configured OAuth application.
    #[derive(serde::Serialize)]
    pub struct GoogleOAuthSettingsView {
        pub has_client_id: bool,
        pub has_client_secret: bool,
    }

    #[tauri::command]
    pub fn google_oauth_settings() -> GoogleOAuthSettingsView {
        GoogleOAuthSettingsView {
            has_client_id: read_keychain_secret(GOOGLE_OAUTH_CLIENT_ID_ACCOUNT)
                .and_then(|value| String::from_utf8(value).ok())
                .and_then(nonempty)
                .is_some(),
            has_client_secret: read_keychain_secret(GOOGLE_OAUTH_CLIENT_SECRET_ACCOUNT)
                .and_then(|value| String::from_utf8(value).ok())
                .and_then(nonempty)
                .is_some(),
        }
    }

    /// Save a Google Desktop OAuth client in Keychain. A blank secret preserves an existing
    /// optional secret; use Clear to remove the complete client. The paired writes are transactional:
    /// if either one fails, both accounts return to their previous values.
    #[tauri::command]
    pub fn set_google_oauth_client(
        client_id: String,
        client_secret: String,
        state: tauri::State<'_, ConnectorState>,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        let client_id =
            nonempty(client_id).ok_or_else(|| "Google OAuth client ID is required".to_string())?;
        let client_secret = nonempty(client_secret);
        let previous_id = read_keychain_secret(GOOGLE_OAUTH_CLIENT_ID_ACCOUNT);
        let previous_secret = read_keychain_secret(GOOGLE_OAUTH_CLIENT_SECRET_ACCOUNT);
        let changed = previous_id.as_deref() != Some(client_id.as_bytes())
            || client_secret
                .as_deref()
                .is_some_and(|secret| previous_secret.as_deref() != Some(secret.as_bytes()));

        keychain_store::set_generic_secret(GOOGLE_OAUTH_CLIENT_ID_ACCOUNT, client_id.as_bytes())
            .map_err(|error| error.to_string())?;
        if let Some(secret) = &client_secret {
            if let Err(error) = keychain_store::set_generic_secret(
                GOOGLE_OAUTH_CLIENT_SECRET_ACCOUNT,
                secret.as_bytes(),
            ) {
                let _ =
                    restore_keychain_secret(GOOGLE_OAUTH_CLIENT_ID_ACCOUNT, previous_id.as_deref());
                return Err(error.to_string());
            }
        }

        if !changed {
            return Ok(());
        }
        if let Err(error) = rebuild_for_google_oauth_change(&state, &app) {
            let _ = restore_keychain_secret(GOOGLE_OAUTH_CLIENT_ID_ACCOUNT, previous_id.as_deref());
            let _ = restore_keychain_secret(
                GOOGLE_OAUTH_CLIENT_SECRET_ACCOUNT,
                previous_secret.as_deref(),
            );
            return Err(error);
        }
        Ok(())
    }

    /// Remove the Keychain-backed Google client. Environment variables remain the documented
    /// developer fallback, but Calendar and Drive grants are always revoked because their client
    /// selection may have changed.
    #[tauri::command]
    pub fn clear_google_oauth_client(
        state: tauri::State<'_, ConnectorState>,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        let previous_id = read_keychain_secret(GOOGLE_OAUTH_CLIENT_ID_ACCOUNT);
        let previous_secret = read_keychain_secret(GOOGLE_OAUTH_CLIENT_SECRET_ACCOUNT);
        delete_keychain_secret_if_present(GOOGLE_OAUTH_CLIENT_ID_ACCOUNT)?;
        if let Err(error) = delete_keychain_secret_if_present(GOOGLE_OAUTH_CLIENT_SECRET_ACCOUNT) {
            let _ = restore_keychain_secret(GOOGLE_OAUTH_CLIENT_ID_ACCOUNT, previous_id.as_deref());
            return Err(error);
        }
        if previous_id.is_none() && previous_secret.is_none() {
            return Ok(());
        }
        if let Err(error) = rebuild_for_google_oauth_change(&state, &app) {
            let _ = restore_keychain_secret(GOOGLE_OAUTH_CLIENT_ID_ACCOUNT, previous_id.as_deref());
            let _ = restore_keychain_secret(
                GOOGLE_OAUTH_CLIENT_SECRET_ACCOUNT,
                previous_secret.as_deref(),
            );
            return Err(error);
        }
        Ok(())
    }

    /// Record the read-egress boundary for one service (invariant 3 / FR-TR-03): Gmail reads cross
    /// a third party (Composio); Calendar/Drive reads go direct to the vendor's official MCP.
    /// Either way we record THAT a read happened, never what was fetched — empty chunk, zero bytes.
    fn record_read_trace(db: &Db, svc: Service) {
        let (route, purpose, third_party) = if svc == Service::Gmail {
            (Route::Composio, "gmail_read", true)
        } else {
            (Route::Mcp, "first_layer_read", false)
        };
        db.traceability_sink().record(TraceRecord::for_chunk(
            route,
            purpose,
            svc.source_str(),
            "",
            third_party,
        ));
    }

    /// The 15-minute read-sync poller (FR-INT-04). Owns clones of the runtime + Db, syncs every due
    /// service, and lets each service fail independently to amber (FR-INT-06).
    ///
    /// The Composio consent gate applies to **Gmail only** — no data leaves to a third party
    /// without the user's explicit opt-in, while first-layer direct reads (Calendar/Drive) are not
    /// third-party and keep syncing. Records a traceability entry on each successful sync
    /// (FR-TR-03).
    pub fn spawn_sync_poller(
        state: Arc<Mutex<ConnectorRuntime<Transport>>>,
        db: Db,
        app: tauri::AppHandle,
    ) {
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(15 * 60));

                let consent = load_composio_policy(&app).consent_acknowledged;
                let now = db.now_ms();
                if let Ok(mut rt) = state.lock() {
                    // Plan gate (issue #97): refresh the entitlements before each tick so the service
                    // gate sees the current plan — an expired trial stops the read-sync (first-layer
                    // reads are Standard-and-up; expired has no active plan).
                    rt.set_plan(crate::entitlement::mac::current(&app));
                    for svc in rt.services_due(now, DEFAULT_SYNC_INTERVAL_MS) {
                        // Gate: a Gmail sync sends the user_id to Composio (third party) — skip it
                        // until the user has granted Composio consent. Other services are direct.
                        if svc == Service::Gmail && !consent {
                            eprintln!(
                                "[connectors] gmail sync skipped — Composio consent not granted"
                            );
                            continue;
                        }
                        match rt.sync_service(svc, now, &db) {
                            Ok(rep) => {
                                eprintln!(
                                    "[connectors] {} synced (+{} new)",
                                    svc.source_str(),
                                    rep.inserted
                                );
                                record_read_trace(&db, svc);
                                // context_updated（#61）: read-sync 完了を匿名計測。
                                if let Some(analytics) =
                                    app.try_state::<crate::analytics::Analytics>()
                                {
                                    analytics.capture(
                                        "context_updated",
                                        crate::analytics::context_updated_props(
                                            svc.source_str(),
                                            rep.inserted as u64,
                                        ),
                                    );
                                }
                            }
                            Err(e) => {
                                eprintln!("[connectors] {} sync failed: {e:?}", svc.source_str());
                            }
                        }
                    }
                }
            }
        });
    }

    // ------------------------------------------------------------- commands

    /// Whether the wired transport can actually serve this service. Gmail always (Composio);
    /// Calendar/Drive only when `SHOGUN_ENABLE_WAVE1_READ` opts them in (the on-device live-
    /// verification switch — docs/connector-summary-and-live-checklist.md §4). Everything else is
    /// "Coming soon" rather than a Connect button that can only end in a false amber.
    fn transport_serves(svc: Service) -> bool {
        let extra =
            connect::parse_wave1_read_optin(std::env::var(connect::WAVE1_READ_ENV).ok().as_deref());
        connect::transport_serves(svc, &extra)
    }

    /// List every service's connection status for the connections screen.
    #[tauri::command]
    pub fn connectors_list(
        state: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
    ) -> Result<Vec<shogun_integrations::ServiceStatus>, String> {
        let now = db.now_ms();
        let rt = state
            .0
            .lock()
            .map_err(|_| "runtime lock poisoned".to_string())?;
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

    /// Verify Gmail's Composio prerequisites (API key + user id + consent). Gmail's "connect" is
    /// credential verification, not OAuth — the 2026-07 decision routes all Gmail I/O through
    /// Composio, so there is no Google token to obtain. Each missing piece is an actionable error.
    fn verify_gmail_composio(app: &tauri::AppHandle) -> Result<(), String> {
        if composio_api_key()
            .map(|k| k.trim().is_empty())
            .unwrap_or(true)
        {
            return Err(
                "Composio API key is not configured — add it in Settings, then retry Connect."
                    .to_string(),
            );
        }
        let policy = load_composio_policy(app);
        let user_id = if policy.user_id.trim().is_empty() {
            std::env::var("SHOGUN_COMPOSIO_USER_ID").unwrap_or_default()
        } else {
            policy.user_id.clone()
        };
        if user_id.trim().is_empty() {
            return Err(
                "Composio user id is not configured — set it in Settings, then retry Connect."
                    .to_string(),
            );
        }
        if !policy.consent_acknowledged {
            return Err(
                "Gmail runs through a third party (Composio) and needs your consent first — grant it in Settings, then retry Connect."
                    .to_string(),
            );
        }
        Ok(())
    }

    /// The blocking half of a Google first-layer connect: env config → loopback listener → browser
    /// PKCE flow → Keychain persist. Pure sequencing over the shogun-integrations machinery; every
    /// failure is a typed [`ConnectError`]. Nothing is marked connected here — the caller does that
    /// only after this returns Ok, so there is never a half-connected state.
    fn run_google_connect(svc: Service, now_ms: i64) -> Result<(), ConnectError> {
        let (client_id, client_secret) = google_client_config(svc)?;
        let endpoint = shogun_integrations::endpoints::endpoint(svc).ok_or_else(|| {
            ConnectError::Internal(format!("{} has no MCP endpoint", svc.source_str()))
        })?;
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|e| ConnectError::ListenerBind(e.to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|e| ConnectError::ListenerBind(e.to_string()))?
            .port();
        let cfg = AuthConfig::google(
            client_id,
            client_secret,
            format!("http://127.0.0.1:{port}/callback"),
        );
        let exchange = HttpTokenExchange::new().map_err(ConnectError::Internal)?;
        let tokens = oauth_flow::run_loopback_flow(
            &cfg,
            endpoint.scopes,
            &listener,
            now_ms,
            &exchange,
            CONNECT_TIMEOUT,
        )?;
        // Persist to the Keychain (invariant 7) — the only place the token set ever lands.
        KeychainTokenStore::new(KEYCHAIN_SERVICE)
            .save(svc, &tokens)
            .map_err(ConnectError::Persist)
    }

    /// Connect a service.
    ///
    /// - **Gmail** (Composio transport): verify API key + user id + consent, then mark connected.
    /// - **Google Calendar / Drive** (first-layer MCP, env-opt-in): run the real OAuth 2.1 PKCE
    ///   loopback flow — browser consent → redirect → token exchange → Keychain — and mark
    ///   connected only on success. A failed attempt (denied / timeout / exchange / persist) turns
    ///   the service amber per FR-INT-06 (`ConnectFailed`); a precondition problem (missing OAuth
    ///   client env) leaves it Disconnected so the Connect button stays as the retry affordance.
    ///   Either way the actionable reason is returned to the UI.
    #[tauri::command]
    pub async fn connect_service(
        service: String,
        app: tauri::AppHandle,
        state: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
    ) -> Result<(), String> {
        let svc = from_source(&service).ok_or_else(|| format!("unknown service: {service}"))?;
        if !transport_serves(svc) {
            return Err(format!("{service} is not available yet"));
        }
        let now = db.now_ms();
        let rt = state.0.clone();

        match svc {
            Service::Gmail => {
                verify_gmail_composio(&app)?;
                rt.lock()
                    .map_err(|_| "runtime lock poisoned".to_string())?
                    .mark_connected(svc, now);
                eprintln!("[connectors] connected gmail (Composio transport)");
                // The landing point when the user comes back from a browser and is looking
                // elsewhere (#49).
                crate::sound::mac::play(shogun_core::sound::Cue::ConnectorLinked);
                Ok(())
            }
            Service::GoogleCalendar | Service::GoogleDrive => {
                // The loopback flow blocks on the browser redirect — keep it off the async
                // runtime and off the runtime lock (the poller must not stall behind a consent
                // page left open).
                let outcome = tokio::task::spawn_blocking(move || run_google_connect(svc, now))
                    .await
                    .map_err(|e| format!("connect task failed: {e}"))?;
                match outcome {
                    Ok(()) => {
                        rt.lock()
                            .map_err(|_| "runtime lock poisoned".to_string())?
                            .mark_connected(svc, now);
                        eprintln!("[connectors] connected {service} (first-layer OAuth)");
                        // The landing point when the user comes back from a browser and is
                        // looking elsewhere (#49).
                        crate::sound::mac::play(shogun_core::sound::Cue::ConnectorLinked);
                        Ok(())
                    }
                    Err(e) => {
                        // FR-INT-06: a failed attempt goes amber (reauth affordance); a
                        // precondition problem stays Disconnected (Connect = retry affordance).
                        if e.marks_amber() {
                            let _ = rt.lock().map(|mut guard| guard.mark_connect_failed(svc));
                        }
                        eprintln!("[connectors] {service} connect failed: {e:?}");
                        Err(e.to_string())
                    }
                }
            }
            other => Err(format!("{} is not available yet", other.source_str())),
        }
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
        // The same opt-in gate the sync poller applies (CLAUDE.md 連携実装ルール): a Gmail
        // on-demand fetch sends the user_id + query to Composio (third party) exactly like a poll
        // does, so it must be impossible without the user's explicit Composio consent. First-layer
        // direct reads (Calendar/Drive) cross no third party and need no Composio consent.
        if svc == Service::Gmail && !load_composio_policy(&app).consent_acknowledged {
            return Err("Composio consent has not been granted".into());
        }
        let mut rt = state
            .0
            .lock()
            .map_err(|_| "runtime lock poisoned".to_string())?;
        // Plan gate (issue #97): refresh entitlements so the service gate sees the current plan
        // (reads are Standard-and-up; an expired trial is denied).
        rt.set_plan(crate::entitlement::mac::current(&app));
        match rt.fetch_on_demand(svc, &query, &*db) {
            Ok(report) => {
                // The same read-egress boundary the sync poller records — an on-demand fetch must
                // be just as visible in the traceability screen. We record THAT a read happened,
                // never the query or fetched body (invariant 3 / FR-TR-03; empty chunk).
                record_read_trace(&db, svc);
                Ok(report.inserted as u64)
            }
            Err(e) => Err(format!("{e:?}")),
        }
    }

    /// Disconnect a service (FR-INT-07): delete the stored token set from the Keychain, then stop
    /// syncing. Ingested events are kept by default. A missing token entry is fine (Gmail's
    /// Composio path stores no per-service token set) — the delete is best-effort and the state
    /// reset always happens.
    #[tauri::command]
    pub fn disconnect_service(
        service: String,
        state: tauri::State<'_, ConnectorState>,
    ) -> Result<(), String> {
        let svc = from_source(&service).ok_or_else(|| format!("unknown service: {service}"))?;
        if let Err(e) = KeychainTokenStore::new(KEYCHAIN_SERVICE).delete(svc) {
            // Not-found is the common case for services that never stored a token set.
            eprintln!("[connectors] {service} token delete skipped: {e}");
        }
        state
            .0
            .lock()
            .map_err(|_| "runtime lock poisoned".to_string())?
            .disconnect(svc, false);
        eprintln!("[connectors] disconnected {service}");
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::{nonempty, select_google_client};

        #[test]
        fn google_client_prefers_complete_keychain_source() {
            let selected = select_google_client(
                Some("saved-id".to_string()),
                Some("saved-secret".to_string()),
                Some("env-id".to_string()),
                Some("env-secret".to_string()),
            );
            assert_eq!(
                selected,
                Some(("saved-id".to_string(), Some("saved-secret".to_string())))
            );
        }

        #[test]
        fn google_client_uses_environment_only_when_keychain_id_is_absent() {
            let selected = select_google_client(
                None,
                Some("orphaned-secret".to_string()),
                Some("env-id".to_string()),
                Some("env-secret".to_string()),
            );
            assert_eq!(
                selected,
                Some(("env-id".to_string(), Some("env-secret".to_string())))
            );
        }

        #[test]
        fn blank_google_oauth_values_are_not_credentials() {
            assert_eq!(nonempty("  \n\t ".to_string()), None);
            assert_eq!(
                nonempty("  saved-id  ".to_string()),
                Some("saved-id".to_string())
            );
        }
    }
}
