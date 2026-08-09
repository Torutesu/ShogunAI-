//! The effectful OAuth loopback flow (feature `live`): generate entropy, open the system browser,
//! catch the redirect on a loopback socket, and exchange the code for tokens.
//!
//! Only the orchestration lives here; every decision (URL, PKCE, forms, parsing) is the pure
//! [`crate::oauth`], and the failure taxonomy is the pure [`crate::connect::ConnectError`] so the
//! desktop can map each outcome onto the FR-INT-06/07 state machine. Cannot be exercised end to
//! end on Linux CI — it opens a browser and talks to Google — but the redirect listener below IS
//! Linux-tested (a socket needs no browser). Confirm end to end on the macOS build with a real
//! Google OAuth "Desktop app" client (Developer Preview).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use crate::connect::{redirect_error_is_denial, ConnectError};
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
/// `timeout` bounds the wait for the browser redirect — an abandoned consent page ends in
/// [`ConnectError::Timeout`], never a command that hangs forever.
pub fn run_loopback_flow(
    cfg: &AuthConfig,
    scopes: &[&str],
    listener: &TcpListener,
    now_ms: i64,
    exchange: &dyn TokenExchange,
    timeout: Duration,
) -> Result<TokenSet, ConnectError> {
    let seed = entropy().map_err(ConnectError::Internal)?;
    let pkce = Pkce::from_entropy(&seed);
    let state = state_from(&seed);
    let url = oauth::authorize_url(cfg, scopes, &pkce, &state).map_err(ConnectError::Internal)?;
    open_browser(&url).map_err(ConnectError::BrowserOpen)?;

    let deadline = Instant::now() + timeout;
    let (code, got_state) = accept_redirect(listener, deadline)?;
    if got_state != state {
        return Err(ConnectError::BadRedirect("state mismatch (possible CSRF)".to_string()));
    }
    let form = oauth::token_exchange_form(cfg, &code, &pkce);
    let body = exchange.post_form(&cfg.token_endpoint, &form).map_err(ConnectError::Exchange)?;
    oauth::parse_token_response(&body, now_ms, None).map_err(ConnectError::Exchange)
}

/// The small "you can close this" page every loopback request gets, so no browser tab ever hangs.
const REPLY_PAGE: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body>SHOGUN is connected. You can close this tab.</body></html>";

/// Accept loopback connections until the OAuth redirect arrives or `deadline` passes.
///
/// Stray requests (a favicon probe, a health check) are answered and skipped — only a real
/// redirect resolves the wait: `code`+`state` → `Ok`, an `error=` param → [`ConnectError::Denied`].
/// No redirect by `deadline` → [`ConnectError::Timeout`].
fn accept_redirect(
    listener: &TcpListener,
    deadline: Instant,
) -> Result<(String, String), ConnectError> {
    listener
        .set_nonblocking(true)
        .map_err(|e| ConnectError::ListenerBind(format!("nonblocking accept unavailable: {e}")))?;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // Read the request line with a short blocking timeout so one stalled client
                // cannot eat the whole flow window.
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let first_line = request.lines().next().unwrap_or_default();
                let parsed = oauth::parse_redirect(first_line);
                // Always answer the browser so the tab doesn't hang, regardless of parse result.
                let _ = stream.write_all(REPLY_PAGE.as_bytes());
                match parsed {
                    Ok(code_state) => return Ok(code_state),
                    Err(e) if redirect_error_is_denial(&e) => return Err(ConnectError::Denied),
                    // A stray request — answered above; keep waiting for the real redirect.
                    Err(_) => {}
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(ConnectError::ListenerBind(format!("loopback accept failed: {e}"))),
        }
        if Instant::now() >= deadline {
            return Err(ConnectError::Timeout);
        }
    }
}

// The concrete `reqwest` token exchange (HttpTokenExchange) lives in shogun-core — the single
// allowlisted HTTP-egress crate (FR-TR-03). This module stays HTTP-client-free: it only orchestrates
// the loopback flow over the injected `TokenExchange` seam.

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

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

    fn bound_listener() -> (TcpListener, u16) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    fn send_request(port: u16, request_line: &str) {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(format!("{request_line}\r\nHost: x\r\n\r\n").as_bytes()).unwrap();
        // Read the reply so the listener's write succeeds before we drop the socket.
        let mut buf = [0u8; 256];
        let _ = s.read(&mut buf);
    }

    #[test]
    fn accept_redirect_times_out_without_a_redirect() {
        let (listener, _) = bound_listener();
        let err = accept_redirect(&listener, Instant::now() + Duration::from_millis(150))
            .unwrap_err();
        assert_eq!(err, ConnectError::Timeout);
    }

    #[test]
    fn accept_redirect_skips_stray_requests_then_returns_the_code() {
        let (listener, port) = bound_listener();
        let sender = std::thread::spawn(move || {
            // A browser favicon probe first, then the real redirect.
            send_request(port, "GET /favicon.ico HTTP/1.1");
            send_request(port, "GET /callback?code=abc123&state=xyz HTTP/1.1");
        });
        let (code, state) =
            accept_redirect(&listener, Instant::now() + Duration::from_secs(5)).unwrap();
        sender.join().unwrap();
        assert_eq!(code, "abc123");
        assert_eq!(state, "xyz");
    }

    #[test]
    fn accept_redirect_surfaces_user_denial() {
        let (listener, port) = bound_listener();
        let sender = std::thread::spawn(move || {
            send_request(port, "GET /callback?error=access_denied HTTP/1.1");
        });
        let err =
            accept_redirect(&listener, Instant::now() + Duration::from_secs(5)).unwrap_err();
        sender.join().unwrap();
        assert_eq!(err, ConnectError::Denied);
    }
}
