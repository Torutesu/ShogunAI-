//! The REST listener for the Memory API (§6.11, FR-API-01 / NFR-SEC-03). Feature `server`.
//!
//! A thin axum adapter over the pure routing layer ([`crate::rest`]): it extracts method / path /
//! Bearer token from the socket, calls [`rest::respond`], and writes the `(status, json)` back.
//! All the policy — auth, path→tool, method validation — lives in `rest` and is unit-tested there;
//! this module is only the bind + adapt.
//!
//! Binding is **localhost-only** (`127.0.0.1`, NFR-SEC-03: never a public interface, CORS not
//! enabled). The default port is 7464; if it is busy the listener falls back to an ephemeral port
//! (FR-API-01) whose value the caller reads from [`tokio::net::TcpListener::local_addr`].

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::response::Response;
use axum::routing::any;
use axum::Router;
use tokio::net::TcpListener;

use crate::backend::MemoryBackend;
use crate::memory_api::TokenRegistry;
use crate::rest::{self, Method, RestRequest};

/// The default Memory API port (FR-API-01).
pub const DEFAULT_PORT: u16 = 7464;

/// Shared server state. Clone is cheap (all `Arc`), as axum requires.
#[derive(Clone)]
pub struct AppState {
    tokens: Arc<TokenRegistry>,
    backend: Arc<dyn MemoryBackend>,
}

impl AppState {
    /// Build state from a token registry and the data backend (the daemon injects a DB-backed one;
    /// tests can inject a stub).
    pub fn new(tokens: Arc<TokenRegistry>, backend: Arc<dyn MemoryBackend>) -> Self {
        Self { tokens, backend }
    }
}

/// Build the router: a single catch-all handler that defers to the pure routing layer.
pub fn build_router(state: AppState) -> Router {
    Router::new().fallback(any(handle)).with_state(state)
}

/// The one handler. Adapts an axum request into a [`RestRequest`], calls [`rest::respond`], and
/// renders the JSON response.
async fn handle(State(state): State<AppState>, req: Request) -> Response {
    let method = match *req.method() {
        axum::http::Method::GET => Some(Method::Get),
        axum::http::Method::POST => Some(Method::Post),
        _ => None,
    };
    let path = req.uri().path().to_string();
    let token = rest::bearer(req.headers().get(AUTHORIZATION).and_then(|v| v.to_str().ok()));
    // Parse the query string: `?include_low` (FR-API-06 opt-in) and `?q=<search>`.
    let raw_query = req.uri().query().unwrap_or("");
    let include_low = raw_query.split('&').any(|kv| kv == "include_low" || kv.starts_with("include_low="));
    let query = raw_query
        .split('&')
        .find_map(|kv| kv.strip_prefix("q="))
        .map(percent_decode);

    let (status, body) = match method {
        Some(method) => rest::respond_with(
            &RestRequest { method, path, token, include_low, query },
            &state.tokens,
            state.backend.as_ref(),
        ),
        // Any verb other than GET/POST is not used by the Memory API.
        None => (405, r#"{"error":"method_not_allowed"}"#.to_string()),
    };

    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap_or_default()
}

/// Minimal `application/x-www-form-urlencoded` value decode: `+` → space, `%XX` → byte. Unknown
/// escapes pass through. Enough for a search query param (no dep).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Bind the localhost listener on `port`, falling back to an ephemeral port if it is busy
/// (FR-API-01). Never binds a non-loopback interface (NFR-SEC-03).
pub async fn bind_local(port: u16) -> std::io::Result<TcpListener> {
    match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(listener) => Ok(listener),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            TcpListener::bind(("127.0.0.1", 0)).await
        }
        Err(e) => Err(e),
    }
}

/// Serve on an already-bound listener until the process ends. The caller reads the actual port from
/// the listener before handing it over (so it can report it via `shogun api status`).
pub async fn serve_on(listener: TcpListener, state: AppState) -> std::io::Result<()> {
    axum::serve(listener, build_router(state)).await
}

/// Convenience: bind the default (or fallback) port and serve. Returns only on error.
pub async fn serve(state: AppState, port: u16) -> std::io::Result<()> {
    let listener = bind_local(port).await?;
    serve_on(listener, state).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn raw_get(addr: std::net::SocketAddr, path: &str, auth: Option<&str>) -> String {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let auth_line = auth.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
        let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n{auth_line}Connection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        String::from_utf8_lossy(&buf).into_owned()
    }

    async fn spawn_server(tokens: TokenRegistry) -> std::net::SocketAddr {
        spawn_server_with(tokens, Arc::new(crate::backend::StubBackend)).await
    }

    async fn spawn_server_with(
        tokens: TokenRegistry,
        backend: Arc<dyn crate::backend::MemoryBackend>,
    ) -> std::net::SocketAddr {
        let listener = bind_local(0).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = AppState::new(Arc::new(tokens), backend);
        tokio::spawn(async move {
            let _ = serve_on(listener, state).await;
        });
        addr
    }

    #[tokio::test]
    async fn status_endpoint_serves_over_the_socket() {
        let addr = spawn_server(TokenRegistry::new()).await;
        let resp = raw_get(addr, "/v1/status", None).await;
        assert!(resp.contains("200"), "status line: {resp}");
        assert!(resp.contains("shogun-memory-api"));
        assert!(resp.contains("application/json"));
    }

    #[tokio::test]
    async fn read_without_token_is_401_over_the_socket() {
        let addr = spawn_server(TokenRegistry::new()).await;
        let resp = raw_get(addr, "/v1/memory/search", None).await;
        assert!(resp.contains("401"), "expected 401, got: {resp}");
        assert!(resp.contains("unauthorized"));
    }

    #[tokio::test]
    async fn read_with_valid_token_is_200_over_the_socket() {
        let mut tokens = TokenRegistry::new();
        tokens.issue("secret-token");
        let addr = spawn_server(tokens).await;
        let resp = raw_get(addr, "/v1/state/people", Some("secret-token")).await;
        assert!(resp.contains("200"), "expected 200, got: {resp}");
        assert!(resp.contains("state.people.list"));
    }

    #[tokio::test]
    async fn unknown_path_is_404_over_the_socket() {
        let addr = spawn_server(TokenRegistry::new()).await;
        let resp = raw_get(addr, "/v1/bogus", Some("t")).await;
        assert!(resp.contains("404"), "expected 404, got: {resp}");
    }

    #[tokio::test]
    async fn backend_data_flows_through_the_socket() {
        use crate::backend::{MemoryBackend, ReadItem};
        use crate::memory_api::Tool;

        struct Fake;
        impl MemoryBackend for Fake {
            fn read(&self, _tool: Tool, _params: &crate::backend::ReadParams) -> Vec<ReadItem> {
                vec![ReadItem::new("ship the report", 0.9)]
            }
        }
        let mut tokens = TokenRegistry::new();
        tokens.issue("t");
        let addr = spawn_server_with(tokens, Arc::new(Fake)).await;
        let resp = raw_get(addr, "/v1/state/commitments", Some("t")).await;
        assert!(resp.contains("200"), "got: {resp}");
        assert!(resp.contains("ship the report"), "real backend data missing: {resp}");
    }
}
