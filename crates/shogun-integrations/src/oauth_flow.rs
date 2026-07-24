//! The effectful OAuth loopback flow (feature `live`): generate entropy, open the system browser,
//! catch the redirect on a loopback socket, and exchange the code for tokens.
//!
//! Only the orchestration lives here; every decision (URL, PKCE, forms, parsing) is the pure
//! [`crate::oauth`]. Cannot be exercised on Linux CI — it opens a browser and talks to Google — but
//! it compiles everywhere (the macOS-only browser-open is `#[cfg]`-gated). Confirm end to end on the
//! macOS build with a real Google OAuth "Desktop app" client (Developer Preview).

use std::io::{Read, Write};
use std::net::TcpListener;

use crate::oauth::{self, AuthConfig, Pkce, TokenExchange, TokenSet};

/// Fresh 32-byte CSPRNG entropy for a PKCE verifier.
fn entropy() -> Result<[u8; 32], String> {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).map_err(|e| format!("csprng failed: {e}"))?;
    Ok(buf)
}

/// A random-ish anti-CSRF state derived from entropy (base64url — no network, no extra dep).
fn state_from(entropy: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(entropy)
}

/// Open a URL in the system browser. macOS uses `open`; other targets are a no-op that returns the
/// URL to the caller to present (keeps the flow compilable off-macOS).
#[cfg(target_os = "macos")]
fn open_browser(url: &str) -> Result<(), String> {
    std::process::Command::new("open").arg(url).status().map_err(|e| format!("open failed: {e}"))?;
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn open_browser(_url: &str) -> Result<(), String> {
    Err("browser-open is only wired for macOS".to_string())
}

/// Run the full loopback Authorization-Code + PKCE flow and return a token set.
///
/// `listener` must be bound to the loopback port named in `cfg.redirect_uri` (the caller binds it so
/// it can put the real, OS-assigned port into the redirect URI first). `now_ms` stamps token expiry.
pub fn run_loopback_flow(
    cfg: &AuthConfig,
    scopes: &[&str],
    listener: &TcpListener,
    now_ms: i64,
    exchange: &dyn TokenExchange,
) -> Result<TokenSet, String> {
    let seed = entropy()?;
    let pkce = Pkce::from_entropy(&seed);
    let state = state_from(&seed);
    let url = oauth::authorize_url(cfg, scopes, &pkce, &state)?;
    open_browser(&url)?;

    let (code, got_state) = accept_redirect(listener)?;
    if got_state != state {
        return Err("oauth state mismatch (possible CSRF) — aborted".to_string());
    }
    let form = oauth::token_exchange_form(cfg, &code, &pkce);
    let body = exchange.post_form(&cfg.token_endpoint, &form)?;
    oauth::parse_token_response(&body, now_ms, None)
}

/// Accept one loopback connection, parse the redirect, and reply with a small "you can close this"
/// page. Returns `(code, state)`.
fn accept_redirect(listener: &TcpListener) -> Result<(String, String), String> {
    let (mut stream, _) = listener.accept().map_err(|e| format!("loopback accept failed: {e}"))?;
    let mut buf = [0u8; 2048];
    let n = stream.read(&mut buf).map_err(|e| format!("loopback read failed: {e}"))?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request.lines().next().unwrap_or_default();
    let parsed = oauth::parse_redirect(first_line);
    // Always answer the browser so the tab doesn't hang, regardless of parse result.
    let page = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body>SHOGUN is connected. You can close this tab.</body></html>";
    let _ = stream.write_all(page.as_bytes());
    parsed
}

// The concrete `reqwest` token exchange (HttpTokenExchange) lives in shogun-core — the single
// allowlisted HTTP-egress crate (FR-TR-03). This module stays HTTP-client-free: it only orchestrates
// the loopback flow over the injected `TokenExchange` seam.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_is_derived_deterministically_from_entropy() {
        assert_eq!(state_from(&[9u8; 32]), state_from(&[9u8; 32]));
        assert_ne!(state_from(&[1u8; 32]), state_from(&[2u8; 32]));
    }

    struct FakeExchange(String);
    impl TokenExchange for FakeExchange {
        fn post_form(&self, _e: &str, _f: &[(String, String)]) -> Result<String, String> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn fake_exchange_parses_into_token_set() {
        // Exercises the pure exchange→parse seam without touching the network or a browser.
        let ex = FakeExchange(r#"{"access_token":"at","expires_in":3600,"refresh_token":"rt"}"#.to_string());
        let cfg = AuthConfig::google("cid", None, "http://127.0.0.1:0/callback");
        let pkce = Pkce::from_entropy(&[3u8; 32]);
        let form = oauth::token_exchange_form(&cfg, "code", &pkce);
        let body = ex.post_form(&cfg.token_endpoint, &form).unwrap();
        let ts = oauth::parse_token_response(&body, 0, None).unwrap();
        assert_eq!(ts.access_token, "at");
        assert_eq!(ts.refresh_token.as_deref(), Some("rt"));
    }
}
