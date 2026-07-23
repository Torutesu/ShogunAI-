//! OAuth 2.1 Authorization Code + PKCE for first-layer connections (§6.9, FR-INT-02).
//!
//! Auth is **user → service directly** (no intermediary): the app opens the service's consent page
//! in the system browser, catches the redirect on a loopback URI, and exchanges the code for tokens
//! using a PKCE verifier — so no long-lived secret is required in the client and no code can be
//! replayed. Tokens land in the Keychain (invariant 7), never a file/DB/log.
//!
//! This module is the **pure** half: PKCE derivation, the authorize URL, the token-exchange /
//! refresh request forms, and token-response parsing with expiry. It has no network and no
//! randomness (entropy is passed in), so it is fully Linux-testable. The effectful half — generate
//! entropy, open the browser, run the loopback listener, POST the token endpoint, persist to
//! Keychain — is [`crate::oauth_flow`] behind the `live` feature.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Google's OAuth endpoints (the defaults for every first-layer Google Workspace service).
pub const GOOGLE_AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

/// The OAuth client configuration for one connection. `client_secret` is `None` for pure-PKCE
/// clients; Google "Desktop app" clients still send a (non-confidential) secret, so it is optional.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    /// The loopback redirect, e.g. `http://127.0.0.1:47821/callback`.
    pub redirect_uri: String,
    pub auth_endpoint: String,
    pub token_endpoint: String,
}

impl AuthConfig {
    /// A Google-endpoints config for a loopback `redirect_uri`.
    pub fn google(
        client_id: impl Into<String>,
        client_secret: Option<String>,
        redirect_uri: impl Into<String>,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret,
            redirect_uri: redirect_uri.into(),
            auth_endpoint: GOOGLE_AUTH_ENDPOINT.to_string(),
            token_endpoint: GOOGLE_TOKEN_ENDPOINT.to_string(),
        }
    }
}

/// A PKCE verifier/challenge pair (RFC 7636, method `S256`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pkce {
    /// The secret verifier — sent only in the token exchange, never in the browser URL.
    pub verifier: String,
    /// `base64url(sha256(verifier))` — sent in the authorize URL.
    pub challenge: String,
}

impl Pkce {
    /// Derive a PKCE pair from raw entropy (≥ 32 bytes recommended). Deterministic and pure — the
    /// caller supplies OS randomness (see [`crate::oauth_flow`]). The verifier is the base64url of
    /// the entropy (unreserved chars only, RFC 7636 §4.1); the challenge is its SHA-256.
    pub fn from_entropy(entropy: &[u8]) -> Self {
        let verifier = URL_SAFE_NO_PAD.encode(entropy);
        let digest = Sha256::digest(verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(digest);
        Self { verifier, challenge }
    }
}

/// Build the authorize URL to open in the system browser. `state` is an anti-CSRF nonce the caller
/// must verify on the redirect. `access_type=offline` + `prompt=consent` request a refresh token.
pub fn authorize_url(cfg: &AuthConfig, scopes: &[&str], pkce: &Pkce, state: &str) -> Result<String, String> {
    let mut url = url::Url::parse(&cfg.auth_endpoint).map_err(|e| format!("bad auth endpoint: {e}"))?;
    url.query_pairs_mut()
        .append_pair("client_id", &cfg.client_id)
        .append_pair("redirect_uri", &cfg.redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &scopes.join(" "))
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("state", state);
    Ok(url.into())
}

/// The form body for the authorization-code → token exchange.
pub fn token_exchange_form(cfg: &AuthConfig, code: &str, pkce: &Pkce) -> Vec<(String, String)> {
    let mut form = vec![
        ("grant_type".into(), "authorization_code".into()),
        ("code".into(), code.into()),
        ("redirect_uri".into(), cfg.redirect_uri.clone()),
        ("client_id".into(), cfg.client_id.clone()),
        ("code_verifier".into(), pkce.verifier.clone()),
    ];
    if let Some(secret) = &cfg.client_secret {
        form.push(("client_secret".into(), secret.clone()));
    }
    form
}

/// The form body for a refresh-token → access-token exchange.
pub fn refresh_form(cfg: &AuthConfig, refresh_token: &str) -> Vec<(String, String)> {
    let mut form = vec![
        ("grant_type".into(), "refresh_token".into()),
        ("refresh_token".into(), refresh_token.into()),
        ("client_id".into(), cfg.client_id.clone()),
    ];
    if let Some(secret) = &cfg.client_secret {
        form.push(("client_secret".into(), secret.clone()));
    }
    form
}

/// Extract `(code, state)` from a loopback redirect's HTTP request line, e.g.
/// `GET /callback?code=abc&state=xyz HTTP/1.1`. Pure, so the loopback handler stays testable. An
/// `error=...` param (user denied consent) becomes an `Err`.
pub fn parse_redirect(request_line: &str) -> Result<(String, String), String> {
    let target = request_line.split_whitespace().nth(1).ok_or("malformed request line")?;
    // Give the relative target a base so `url` can parse just the path+query.
    let parsed = url::Url::parse("http://127.0.0.1").and_then(|b| b.join(target)).map_err(|e| format!("bad redirect target: {e}"))?;
    let mut code = None;
    let mut state = None;
    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "error" => return Err(format!("authorization denied: {v}")),
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            _ => {}
        }
    }
    match (code, state) {
        (Some(c), Some(s)) => Ok((c, s)),
        _ => Err("redirect missing code or state".to_string()),
    }
}

#[derive(Debug, Deserialize)]
struct RawTokenResponse {
    access_token: String,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
}

/// The token-endpoint POST seam — a form POST returning the JSON body. The real impl is a `reqwest`
/// client ([`crate::oauth_flow::HttpTokenExchange`], feature `live`); tests and the token manager
/// inject a fake. Defined here (pure) so both the interactive flow and [`crate::token`] share it.
pub trait TokenExchange {
    /// POST `form` (application/x-www-form-urlencoded) to `token_endpoint`, returning the JSON body.
    fn post_form(&self, token_endpoint: &str, form: &[(String, String)]) -> Result<String, String>;
}

/// A parsed token set with an absolute expiry (unix ms). `refresh_token` may be absent on a refresh
/// response (the existing one stays valid).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_ms: i64,
}

impl TokenSet {
    /// Whether the access token is expired (or within `skew_ms` of expiring) at `now_ms`.
    pub fn is_expired(&self, now_ms: i64, skew_ms: i64) -> bool {
        now_ms.saturating_add(skew_ms) >= self.expires_at_ms
    }
}

/// Parse a token-endpoint JSON response into a [`TokenSet`], computing the absolute expiry from
/// `now_ms + expires_in`. `prior_refresh` carries forward a refresh token the response omitted.
pub fn parse_token_response(
    body: &str,
    now_ms: i64,
    prior_refresh: Option<String>,
) -> Result<TokenSet, String> {
    let raw: RawTokenResponse =
        serde_json::from_str(body).map_err(|_| "token response was not valid json".to_string())?;
    // Default to a conservative 60 min if the server omitted expires_in.
    let ttl_ms = raw.expires_in.unwrap_or(3600).saturating_mul(1000);
    Ok(TokenSet {
        access_token: raw.access_token,
        refresh_token: raw.refresh_token.or(prior_refresh),
        expires_at_ms: now_ms.saturating_add(ttl_ms),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AuthConfig {
        AuthConfig::google("client-123.apps.googleusercontent.com", Some("secret".into()), "http://127.0.0.1:47821/callback")
    }

    #[test]
    fn pkce_challenge_is_sha256_of_verifier_and_deterministic() {
        let entropy = [7u8; 32];
        let a = Pkce::from_entropy(&entropy);
        let b = Pkce::from_entropy(&entropy);
        assert_eq!(a, b, "same entropy → same pair");
        // challenge == base64url(sha256(verifier))
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(a.verifier.as_bytes()));
        assert_eq!(a.challenge, expected);
        // 32-byte SHA-256 → 43-char base64url (no padding).
        assert_eq!(a.challenge.len(), 43);
        // verifier is URL-safe (no '+', '/', '=').
        assert!(!a.verifier.contains(['+', '/', '=']));
    }

    #[test]
    fn authorize_url_has_pkce_and_offline_and_state() {
        let pkce = Pkce::from_entropy(&[1u8; 32]);
        let scopes = ["https://www.googleapis.com/auth/gmail.readonly", "https://www.googleapis.com/auth/gmail.compose"];
        let u = authorize_url(&cfg(), &scopes, &pkce, "nonce-xyz").unwrap();
        assert!(u.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(u.contains("code_challenge_method=S256"));
        assert!(u.contains(&format!("code_challenge={}", pkce.challenge)));
        assert!(u.contains("access_type=offline"));
        assert!(u.contains("state=nonce-xyz"));
        // scopes are space-joined then percent-encoded (space → %20 or +).
        assert!(u.contains("gmail.readonly"));
        // the verifier must NEVER appear in the browser URL.
        assert!(!u.contains(&pkce.verifier));
    }

    #[test]
    fn exchange_form_includes_verifier_and_secret() {
        let pkce = Pkce::from_entropy(&[2u8; 32]);
        let form = token_exchange_form(&cfg(), "auth-code", &pkce);
        assert!(form.contains(&("grant_type".into(), "authorization_code".into())));
        assert!(form.contains(&("code_verifier".into(), pkce.verifier.clone())));
        assert!(form.contains(&("client_secret".into(), "secret".into())));
    }

    #[test]
    fn refresh_form_omits_secret_when_none() {
        let mut c = cfg();
        c.client_secret = None;
        let form = refresh_form(&c, "refresh-abc");
        assert!(form.contains(&("grant_type".into(), "refresh_token".into())));
        assert!(!form.iter().any(|(k, _)| k == "client_secret"));
    }

    #[test]
    fn parse_redirect_extracts_code_and_state() {
        let (code, state) = parse_redirect("GET /callback?code=abc123&state=xyz HTTP/1.1").unwrap();
        assert_eq!(code, "abc123");
        assert_eq!(state, "xyz");
    }

    #[test]
    fn parse_redirect_surfaces_user_denial_and_missing_params() {
        assert!(parse_redirect("GET /callback?error=access_denied HTTP/1.1").unwrap_err().contains("denied"));
        assert!(parse_redirect("GET /callback?code=only HTTP/1.1").is_err());
        assert!(parse_redirect("garbage").is_err());
    }

    #[test]
    fn parse_token_response_computes_absolute_expiry() {
        let body = r#"{"access_token":"at","expires_in":3600,"refresh_token":"rt","token_type":"Bearer"}"#;
        let ts = parse_token_response(body, 1_000, None).unwrap();
        assert_eq!(ts.access_token, "at");
        assert_eq!(ts.refresh_token.as_deref(), Some("rt"));
        assert_eq!(ts.expires_at_ms, 1_000 + 3_600_000); // = 3_601_000
        assert!(!ts.is_expired(1_000, 0));
        assert!(!ts.is_expired(3_600_999, 0));
        assert!(ts.is_expired(3_601_000, 0));
        // skew makes it "expired" early
        assert!(ts.is_expired(3_000_000, 700_000));
    }

    #[test]
    fn refresh_response_without_refresh_token_carries_prior_forward() {
        let body = r#"{"access_token":"new","expires_in":3600}"#;
        let ts = parse_token_response(body, 0, Some("old-refresh".into())).unwrap();
        assert_eq!(ts.refresh_token.as_deref(), Some("old-refresh"));
    }
}
