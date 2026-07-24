//! Token lifecycle: store, load, and **auto-refresh** OAuth tokens so a first-layer MCP call always
//! has a valid access token (§6.9). This is the working half of "the connection actually stays
//! connected" — a background sync 50 minutes after connecting must not fail just because the access
//! token expired; [`TokenManager`] refreshes it transparently using the stored refresh token.
//!
//! Pure and Linux-testable over two seams: [`TokenStore`] (Keychain on macOS — [`crate::live`] —
//! or [`MemoryTokenStore`] in tests) and [`crate::oauth::TokenExchange`] (reqwest live, fake in
//! tests). Secrets live only in the store (Keychain in production); nothing here logs a token.

use serde::{Deserialize, Serialize};
use shogun_mcp::scope::Service;

use crate::oauth::{self, AuthConfig, TokenExchange, TokenSet};

/// Refresh this many ms before the access token's hard expiry, so an in-flight call never races the
/// boundary.
pub const DEFAULT_REFRESH_SKEW_MS: i64 = 60_000;

/// Persists a service's [`TokenSet`]. The macOS impl is a Keychain entry (invariant 7); tests use
/// [`MemoryTokenStore`]. Keyed per service so disconnecting one leaves the others intact.
pub trait TokenStore {
    fn load(&self, service: Service) -> Option<TokenSet>;
    fn save(&self, service: Service, tokens: &TokenSet) -> Result<(), String>;
    fn delete(&self, service: Service) -> Result<(), String>;
}

/// Serializable mirror of [`TokenSet`] for whatever the store persists (a Keychain blob is a
/// string). JSON keeps it forward-compatible if fields are added.
#[derive(Serialize, Deserialize)]
struct StoredToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_at_ms: i64,
}

/// Serialize a token set for storage.
pub fn serialize(tokens: &TokenSet) -> Result<String, String> {
    serde_json::to_string(&StoredToken {
        access_token: tokens.access_token.clone(),
        refresh_token: tokens.refresh_token.clone(),
        expires_at_ms: tokens.expires_at_ms,
    })
    .map_err(|_| "failed to serialize token".to_string())
}

/// Parse a stored token blob back into a [`TokenSet`].
pub fn deserialize(blob: &str) -> Result<TokenSet, String> {
    let s: StoredToken = serde_json::from_str(blob).map_err(|_| "failed to parse stored token".to_string())?;
    Ok(TokenSet {
        access_token: s.access_token,
        refresh_token: s.refresh_token,
        expires_at_ms: s.expires_at_ms,
    })
}

/// Why a valid access token could not be produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    /// No stored token for the service — it is not connected.
    NotConnected,
    /// The access token is expired and there is no refresh token — the user must reconnect.
    NeedsReauth,
    /// The refresh network call / store failed. Carries a short, content-free reason.
    Refresh(String),
}

/// Produces a valid access token for a service, refreshing on demand. Borrows the config, the token
/// exchange (network), and the store.
pub struct TokenManager<'a, X: TokenExchange, S: TokenStore> {
    cfg: &'a AuthConfig,
    exchange: &'a X,
    store: &'a S,
}

impl<'a, X: TokenExchange, S: TokenStore> TokenManager<'a, X, S> {
    pub fn new(cfg: &'a AuthConfig, exchange: &'a X, store: &'a S) -> Self {
        Self { cfg, exchange, store }
    }

    /// A currently-valid access token for `service`. If the stored one is within
    /// [`DEFAULT_REFRESH_SKEW_MS`] of expiry, refresh it (persisting the new one) before returning.
    pub fn valid_access_token(&self, service: Service, now_ms: i64) -> Result<String, TokenError> {
        let tokens = self.store.load(service).ok_or(TokenError::NotConnected)?;
        if !tokens.is_expired(now_ms, DEFAULT_REFRESH_SKEW_MS) {
            return Ok(tokens.access_token);
        }
        let refresh = tokens.refresh_token.clone().ok_or(TokenError::NeedsReauth)?;
        let form = oauth::refresh_form(self.cfg, &refresh);
        let body = self
            .exchange
            .post_form(&self.cfg.token_endpoint, &form)
            .map_err(TokenError::Refresh)?;
        // A refresh response often omits the refresh token — carry the existing one forward.
        let refreshed = oauth::parse_token_response(&body, now_ms, Some(refresh))
            .map_err(TokenError::Refresh)?;
        self.store.save(service, &refreshed).map_err(TokenError::Refresh)?;
        Ok(refreshed.access_token)
    }
}

/// Wall-clock unix-ms — the default clock for [`ManagedTokenProvider`].
pub fn system_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Picks the [`AuthConfig`] (OAuth client + endpoints) used to refresh a service's token. Vendors
/// differ — Google services share one config, Slack has its own token endpoint — so refresh must
/// never cross vendors. `None` = no client registered for that service (its refresh fails cleanly).
pub type ConfigSelector = Box<dyn Fn(Service) -> Option<AuthConfig> + Send + Sync>;

/// A [`crate::rpc::TokenProvider`] that always returns a *valid* access token, refreshing via
/// [`TokenManager`] when the stored one is near expiry. This is what the live MCP client
/// (`shogun-core::mcp_http::HttpMcpRpc`) uses, so a first-layer call an hour after connecting still
/// succeeds. Pure (no network of its own) — the refresh network call goes through the injected
/// [`TokenExchange`].
pub struct ManagedTokenProvider<X: TokenExchange, S: TokenStore> {
    config_for: ConfigSelector,
    exchange: X,
    store: S,
    clock: fn() -> i64,
}

impl<X: TokenExchange, S: TokenStore> ManagedTokenProvider<X, S> {
    /// One config for every service (fine while only Google services are released — they share
    /// endpoints and one OAuth client). Uses the wall clock.
    pub fn new(cfg: AuthConfig, exchange: X, store: S) -> Self {
        Self::with_config_selector(Box::new(move |_| Some(cfg.clone())), exchange, store)
    }

    /// Per-service config selection — required once services from more than one vendor are
    /// connectable (e.g. Google + Slack), so a Slack refresh never posts to Google's endpoint.
    pub fn with_config_selector(config_for: ConfigSelector, exchange: X, store: S) -> Self {
        Self { config_for, exchange, store, clock: system_now_ms }
    }

    /// Override the clock (tests).
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.clock = clock;
        self
    }
}

impl<X: TokenExchange, S: TokenStore> crate::rpc::TokenProvider for ManagedTokenProvider<X, S> {
    fn access_token(&self, service: Service) -> Result<String, String> {
        let cfg = (self.config_for)(service)
            .ok_or_else(|| format!("no oauth client configured for {}", service.source_str()))?;
        let mgr = TokenManager::new(&cfg, &self.exchange, &self.store);
        mgr.valid_access_token(service, (self.clock)()).map_err(|e| format!("{e:?}"))
    }
}

/// An in-memory token store for tests and dev (never used in production — production is the Keychain).
#[derive(Default)]
pub struct MemoryTokenStore {
    tokens: std::sync::Mutex<Vec<(Service, TokenSet)>>,
}

impl MemoryTokenStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStore for MemoryTokenStore {
    fn load(&self, service: Service) -> Option<TokenSet> {
        let g = self.tokens.lock().ok()?;
        g.iter().find(|(s, _)| *s == service).map(|(_, t)| t.clone())
    }
    fn save(&self, service: Service, tokens: &TokenSet) -> Result<(), String> {
        let mut g = self.tokens.lock().map_err(|_| "store poisoned".to_string())?;
        g.retain(|(s, _)| *s != service);
        g.push((service, tokens.clone()));
        Ok(())
    }
    fn delete(&self, service: Service) -> Result<(), String> {
        let mut g = self.tokens.lock().map_err(|_| "store poisoned".to_string())?;
        g.retain(|(s, _)| *s != service);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn cfg() -> AuthConfig {
        AuthConfig::google("cid", Some("secret".into()), "http://127.0.0.1:0/callback")
    }

    /// Records how many times a refresh was attempted and returns a canned refresh response.
    struct CountingExchange {
        calls: RefCell<u32>,
        reply: Result<String, String>,
    }
    impl TokenExchange for CountingExchange {
        fn post_form(&self, _e: &str, _f: &[(String, String)]) -> Result<String, String> {
            *self.calls.borrow_mut() += 1;
            self.reply.clone()
        }
    }

    fn token(access: &str, refresh: Option<&str>, expires_at_ms: i64) -> TokenSet {
        TokenSet { access_token: access.into(), refresh_token: refresh.map(str::to_string), expires_at_ms }
    }

    #[test]
    fn serialize_round_trips() {
        let t = token("at", Some("rt"), 12345);
        let blob = serialize(&t).unwrap();
        assert_eq!(deserialize(&blob).unwrap(), t);
    }

    #[test]
    fn valid_token_is_returned_without_refresh() {
        let store = MemoryTokenStore::new();
        store.save(Service::Gmail, &token("live-at", Some("rt"), 1_000_000)).unwrap();
        let ex = CountingExchange { calls: RefCell::new(0), reply: Ok(String::new()) };
        let cfg = cfg();
        let mgr = TokenManager::new(&cfg, &ex, &store);
        // now well before expiry (minus skew) → no refresh
        assert_eq!(mgr.valid_access_token(Service::Gmail, 100_000).unwrap(), "live-at");
        assert_eq!(*ex.calls.borrow(), 0, "must not refresh a still-valid token");
    }

    #[test]
    fn expired_token_is_refreshed_and_persisted() {
        let store = MemoryTokenStore::new();
        store.save(Service::Gmail, &token("old-at", Some("rt-1"), 1_000)).unwrap();
        let ex = CountingExchange {
            calls: RefCell::new(0),
            reply: Ok(r#"{"access_token":"new-at","expires_in":3600}"#.to_string()),
        };
        let cfg = cfg();
        let mgr = TokenManager::new(&cfg, &ex, &store);
        // now is past expiry → refresh happens
        let at = mgr.valid_access_token(Service::Gmail, 2_000).unwrap();
        assert_eq!(at, "new-at");
        assert_eq!(*ex.calls.borrow(), 1);
        // the refreshed token was persisted, carrying the prior refresh token forward
        let stored = store.load(Service::Gmail).unwrap();
        assert_eq!(stored.access_token, "new-at");
        assert_eq!(stored.refresh_token.as_deref(), Some("rt-1"));
        assert_eq!(stored.expires_at_ms, 2_000 + 3_600_000);
    }

    #[test]
    fn expired_without_refresh_token_needs_reauth() {
        let store = MemoryTokenStore::new();
        store.save(Service::Gmail, &token("old-at", None, 1_000)).unwrap();
        let ex = CountingExchange { calls: RefCell::new(0), reply: Ok(String::new()) };
        let cfg = cfg();
        let mgr = TokenManager::new(&cfg, &ex, &store);
        assert_eq!(mgr.valid_access_token(Service::Gmail, 2_000), Err(TokenError::NeedsReauth));
        assert_eq!(*ex.calls.borrow(), 0);
    }

    #[test]
    fn unconnected_service_errors() {
        let store = MemoryTokenStore::new();
        let ex = CountingExchange { calls: RefCell::new(0), reply: Ok(String::new()) };
        let cfg = cfg();
        let mgr = TokenManager::new(&cfg, &ex, &store);
        assert_eq!(mgr.valid_access_token(Service::Gmail, 0), Err(TokenError::NotConnected));
    }

    fn clock_2000() -> i64 {
        2_000
    }

    #[test]
    fn managed_provider_serves_a_valid_token_via_the_provider_trait() {
        use crate::rpc::TokenProvider;
        let store = MemoryTokenStore::new();
        // stored token is expired at now=2000 → the provider refreshes transparently
        store.save(Service::Gmail, &token("old", Some("rt"), 1_000)).unwrap();
        let ex = CountingExchange {
            calls: RefCell::new(0),
            reply: Ok(r#"{"access_token":"fresh","expires_in":3600}"#.to_string()),
        };
        let provider = ManagedTokenProvider::new(cfg(), ex, store).with_clock(clock_2000);
        assert_eq!(provider.access_token(Service::Gmail).unwrap(), "fresh");
        // an unconnected service surfaces an error string
        assert!(provider.access_token(Service::GoogleCalendar).is_err());
    }

    #[test]
    fn config_selector_scopes_refresh_to_the_right_vendor() {
        use crate::rpc::TokenProvider;
        let store = MemoryTokenStore::new();
        store.save(Service::Gmail, &token("g", Some("r"), 9_999_999)).unwrap();
        store.save(Service::Slack, &token("s", Some("r"), 9_999_999)).unwrap();
        let ex = CountingExchange { calls: RefCell::new(0), reply: Ok(String::new()) };
        // Only Google services have a config; Slack has none registered.
        let selector: super::ConfigSelector = Box::new(|svc| match svc {
            Service::Gmail | Service::GoogleCalendar | Service::GoogleDrive => {
                Some(AuthConfig::google("cid", None, "http://127.0.0.1:0/cb"))
            }
            _ => None,
        });
        let provider =
            ManagedTokenProvider::with_config_selector(selector, ex, store).with_clock(clock_2000);
        // Gmail's token is valid → served.
        assert_eq!(provider.access_token(Service::Gmail).unwrap(), "g");
        // Slack has a stored token but no oauth client → a clean, explicit error (not a
        // cross-vendor refresh against Google's endpoint).
        let err = provider.access_token(Service::Slack).unwrap_err();
        assert!(err.contains("no oauth client"), "got: {err}");
    }

    #[test]
    fn delete_removes_only_that_service() {
        let store = MemoryTokenStore::new();
        store.save(Service::Gmail, &token("g", Some("r"), 9)).unwrap();
        store.save(Service::GoogleCalendar, &token("c", Some("r"), 9)).unwrap();
        store.delete(Service::Gmail).unwrap();
        assert!(store.load(Service::Gmail).is_none());
        assert!(store.load(Service::GoogleCalendar).is_some());
    }
}
