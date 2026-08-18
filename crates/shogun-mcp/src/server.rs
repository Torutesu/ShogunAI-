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

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::response::Response;
use axum::routing::any;
use axum::Router;
use shogun_agents::approval::ApprovalQueue;
use shogun_agents::entitlement::Entitlements;
use tokio::net::TcpListener;

use crate::backend::MemoryBackend;
use crate::memory_api::TokenRegistry;
use crate::rest::{self, Method, RestRequest, Routed};

/// The default Memory API port (FR-API-01).
pub const DEFAULT_PORT: u16 = 7464;

/// An injected millisecond clock (unix ms) — deterministic under test.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The plan entitlement provider (issue #97). A closure, not a snapshot: a trial can expire while
/// the server runs, so it is consulted on every request. The default (see [`AppState::new`]) is
/// trial-not-started — full access until a trial stamp / billing state is wired in.
pub type EntitlementProvider = Arc<dyn Fn() -> Entitlements + Send + Sync>;

/// Live in-product SLO metrics source (NFR-SLO-00). The daemon implements it over its `SloRegistry`;
/// the server serves its JSON at `GET /v1/metrics` (`shogun metrics` / the Advanced UI).
pub trait MetricsSource: Send + Sync {
    /// The current SLO snapshot as JSON (the `{"metrics":[...]}` shape).
    fn snapshot_json(&self) -> String;
}

/// Shared server state. Clone is cheap (all `Arc`), as axum requires. The approval queue is the
/// **same one the Notch UI drains** — an API-requested L3 send and a human L3 are one flow
/// (invariant 6 / FR-API-04).
#[derive(Clone)]
pub struct AppState {
    tokens: Arc<TokenRegistry>,
    backend: Arc<dyn MemoryBackend>,
    approvals: Arc<Mutex<ApprovalQueue>>,
    clock: Clock,
    metrics: Option<Arc<dyn MetricsSource>>,
    entitlements: EntitlementProvider,
    surface: rest::ApprovalSurface,
    approvals_path: Option<PathBuf>,
    desktop_running: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl AppState {
    /// Build state from the token registry, the data backend, the shared approval queue, and a
    /// clock. The daemon injects a DB-backed backend + its real approval queue; tests inject stubs.
    /// The plan gate defaults to trial-not-started (full access — the documented pre-onboarding
    /// default); the composition root attaches the real provider via [`AppState::with_entitlements`].
    pub fn new(
        tokens: Arc<TokenRegistry>,
        backend: Arc<dyn MemoryBackend>,
        approvals: Arc<Mutex<ApprovalQueue>>,
        clock: Clock,
    ) -> Self {
        Self {
            tokens,
            backend,
            approvals,
            clock,
            metrics: None,
            entitlements: Arc::new(Entitlements::trial_not_started),
            // Headless by default — see `McpServer::new`. The composition root that owns a
            // confirm UI opts in with `with_approval_surface`.
            surface: rest::ApprovalSurface::Absent,
            approvals_path: None,
            desktop_running: Arc::new(|| false),
        }
    }

    /// Declare that this process runs a confirm UI, so L3 sends may be enqueued for it.
    #[must_use]
    pub fn with_approval_surface(mut self, surface: rest::ApprovalSurface) -> Self {
        self.surface = surface;
        self
    }

    #[must_use]
    pub fn with_approvals_path(mut self, path: PathBuf) -> Self {
        self.approvals_path = Some(path);
        self
    }

    #[must_use]
    pub fn with_desktop_running_check(
        mut self,
        check: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Self {
        self.desktop_running = Arc::new(check);
        self
    }

    /// Attach the live SLO metrics source served at `GET /v1/metrics`. Without it, the endpoint
    /// returns an empty (all-unmeasured) snapshot.
    pub fn with_metrics(mut self, metrics: Arc<dyn MetricsSource>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Attach the plan entitlement provider (issue #97) — consulted on every request, so trial
    /// expiry takes effect without a restart.
    pub fn with_entitlements(mut self, entitlements: EntitlementProvider) -> Self {
        self.entitlements = entitlements;
        self
    }
}

/// Build the router: a single catch-all handler that defers to the pure routing layer.
pub fn build_router(state: AppState) -> Router {
    Router::new().fallback(any(handle)).with_state(state)
}

/// The one handler. Adapts an axum request into a [`RestRequest`], calls [`rest::respond`], and
/// renders the JSON response.
/// Is this request addressed to us as localhost?
///
/// The listener is loopback-only and every tool endpoint needs a Bearer token, so memory data was
/// never exposed. But `/v1/status` and `/v1/metrics` are deliberately open, and a page on
/// `attacker.com` whose DNS rebinds to 127.0.0.1 is SAME-ORIGIN with this server — CORS never
/// applies, and the live SLO snapshot (device activity timing) becomes readable by an arbitrary
/// website. A Host check is the standard answer: a rebound page still carries its own hostname.
fn host_is_loopback(host: Option<&str>) -> bool {
    let Some(host) = host else { return false };
    let name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    let name = name.trim_start_matches('[').trim_end_matches(']');
    matches!(name, "127.0.0.1" | "localhost" | "::1" | "0.0.0.0")
}

async fn handle(State(state): State<AppState>, req: Request) -> Response {
    if !host_is_loopback(
        req.headers()
            .get(axum::http::header::HOST)
            .and_then(|v| v.to_str().ok()),
    ) {
        return render(403, r#"{"error":"forbidden_host"}"#.to_string());
    }
    let method = match *req.method() {
        axum::http::Method::GET => Some(Method::Get),
        axum::http::Method::POST => Some(Method::Post),
        _ => None,
    };
    let path = req.uri().path().to_string();
    let token = rest::bearer(
        req.headers()
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
    );
    // Parse the query string: `?include_low` (FR-API-06 opt-in), `?q=<search>`, visual-recall window.
    let raw_query = req.uri().query().unwrap_or("");
    let include_low = raw_query
        .split('&')
        .any(|kv| kv == "include_low" || kv.starts_with("include_low="));
    let query = raw_query
        .split('&')
        .find_map(|kv| kv.strip_prefix("q="))
        .map(percent_decode);
    let from_ms = raw_query
        .split('&')
        .find_map(|kv| kv.strip_prefix("from_ms="))
        .and_then(|v| v.parse::<i64>().ok());
    let to_ms = raw_query
        .split('&')
        .find_map(|kv| kv.strip_prefix("to_ms="))
        .and_then(|v| v.parse::<i64>().ok());

    // Read the request body (POST writes / actions). Bounded to 256 KiB; empty on read failure.
    let body = axum::body::to_bytes(req.into_body(), 256 * 1024)
        .await
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .filter(|s| !s.is_empty());

    let (status, resp_body) = match method {
        None => (405, r#"{"error":"method_not_allowed"}"#.to_string()),
        Some(method) => {
            let rreq = RestRequest {
                method,
                path,
                token,
                include_low,
                query,
                body,
                from_ms,
                to_ms,
            };
            // Resolve the plan once per request (issue #97) — the provider re-reads its source, so
            // a trial expiring while the server runs locks the next request.
            let ent = (state.entitlements)();
            match rest::route(&rreq, &state.tokens, &ent) {
                // actions.execute needs the shared approval queue (L3 sends enqueue there).
                Routed::Action => {
                    if let Some(path) = &state.approvals_path {
                        if !(state.desktop_running)() {
                            (503, r#"{"error":"desktop_unavailable"}"#.to_string())
                        } else {
                            match crate::approval_store::with_queue(path, |queue| {
                                rest::act(
                                    rreq.body.as_deref(),
                                    (state.clock)(),
                                    queue,
                                    shogun_agents::approval::ApprovalOrigin::Api,
                                    rest::ApprovalSurface::Present,
                                )
                            }) {
                                Ok(result) => result,
                                Err(_) => (500, r#"{"error":"approval_store"}"#.to_string()),
                            }
                        }
                    } else {
                        match state.approvals.lock() {
                            Ok(mut queue) => rest::act(
                                rreq.body.as_deref(),
                                (state.clock)(),
                                &mut queue,
                                shogun_agents::approval::ApprovalOrigin::Api,
                                state.surface,
                            ),
                            Err(_) => (500, r#"{"error":"internal"}"#.to_string()),
                        }
                    }
                }
                Routed::ApprovalStatus { id } => {
                    if let Some(path) = &state.approvals_path {
                        match crate::approval_store::with_queue(path, |queue| {
                            rest::poll_approval(id, queue, (state.clock)())
                        }) {
                            Ok(body) => (200, body),
                            Err(_) => (500, r#"{"error":"approval_store"}"#.to_string()),
                        }
                    } else {
                        match state.approvals.lock() {
                            Ok(mut queue) => {
                                (200, rest::poll_approval(id, &mut queue, (state.clock)()))
                            }
                            Err(_) => (500, r#"{"error":"internal"}"#.to_string()),
                        }
                    }
                }
                // metrics come from the injected live source (empty snapshot if none).
                Routed::Metrics => (
                    200,
                    state
                        .metrics
                        .as_ref()
                        .map(|m| m.snapshot_json())
                        .unwrap_or_else(|| r#"{"metrics":[]}"#.to_string()),
                ),
                // reads/writes/status/errors go through the backend renderer.
                _ => rest::respond_with(&rreq, &state.tokens, &ent, state.backend.as_ref()),
            }
        }
    };

    render(status, resp_body)
}

/// The one place a JSON response is built, so every exit from `handle` is shaped the same.
fn render(status: u16, body: String) -> Response {
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
        let auth_line = auth
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: localhost\r\n{auth_line}Connection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// GET with an arbitrary `Host:` header — what a DNS-rebound page's request looks like.
    async fn raw_get_with_host(addr: std::net::SocketAddr, path: &str, host: &str) -> String {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        String::from_utf8_lossy(&buf).into_owned()
    }

    async fn raw_post(
        addr: std::net::SocketAddr,
        path: &str,
        auth: Option<&str>,
        body: &str,
    ) -> String {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let auth_line = auth
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\n{auth_line}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
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
        let approvals = Arc::new(Mutex::new(ApprovalQueue::new()));
        // The tests stand in for a process that HAS a confirm UI (the desktop app hosting this
        // face); the headless default is exercised by `send_is_refused_without_an_approval_surface`.
        let state = AppState::new(Arc::new(tokens), backend, approvals, Arc::new(|| 0))
            .with_approval_surface(rest::ApprovalSurface::Present);
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
    async fn locked_plan_is_403_over_the_socket() {
        // Issue #97: a Standard-plan provider turns every tool endpoint into 403 plan_required,
        // valid token included; /v1/status stays open.
        use shogun_agents::entitlement::{entitlements, Plan};
        let mut tokens = TokenRegistry::new();
        tokens.issue("t");
        let listener = bind_local(0).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let approvals = Arc::new(Mutex::new(ApprovalQueue::new()));
        let state = AppState::new(
            Arc::new(tokens),
            Arc::new(crate::backend::StubBackend),
            approvals,
            Arc::new(|| 0),
        )
        .with_entitlements(Arc::new(|| entitlements(Plan::Standard, 0)));
        tokio::spawn(async move {
            let _ = serve_on(listener, state).await;
        });
        let resp = raw_get(addr, "/v1/state/people", Some("t")).await;
        assert!(resp.contains("403"), "expected 403, got: {resp}");
        assert!(resp.contains("plan_required"));
        let resp = raw_get(addr, "/v1/status", None).await;
        assert!(resp.contains("200"), "status must stay open: {resp}");
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
        assert!(
            resp.contains("ship the report"),
            "real backend data missing: {resp}"
        );
    }

    #[tokio::test]
    async fn post_note_reaches_the_backend_write_over_the_socket() {
        use crate::backend::{MemoryBackend, ReadItem, ReadParams, WriteResult};
        use crate::memory_api::Tool;
        use std::sync::Mutex;

        // a backend that records the note body it was asked to write
        struct Recorder {
            last: Mutex<Option<String>>,
        }
        impl MemoryBackend for Recorder {
            fn read(&self, _t: Tool, _p: &ReadParams) -> Vec<ReadItem> {
                Vec::new()
            }
            fn write(&self, tool: Tool, body: &str) -> WriteResult {
                if tool == Tool::MemoryAppendNote {
                    *self.last.lock().unwrap() = Some(body.to_string());
                    Ok(Some(7))
                } else {
                    Ok(None)
                }
            }
        }
        let mut tokens = TokenRegistry::new();
        tokens.issue("t");
        let backend = Arc::new(Recorder {
            last: Mutex::new(None),
        });
        let addr = spawn_server_with(tokens, backend.clone()).await;

        let resp = raw_post(addr, "/v1/memory/notes", Some("t"), "buy milk").await;
        assert!(resp.contains("202"), "got: {resp}");
        assert!(resp.contains("\"id\":7"));
        assert_eq!(backend.last.lock().unwrap().as_deref(), Some("buy milk"));

        // no token → 401, and the backend is not touched
        let resp = raw_post(addr, "/v1/memory/notes", None, "sneaky").await;
        assert!(resp.contains("401"), "got: {resp}");
    }

    #[tokio::test]
    async fn a_rebound_hostname_cannot_read_the_open_endpoints() {
        // /v1/metrics has no token by design, and the listener is loopback-only — but a page on
        // attacker.com whose DNS answers 127.0.0.1 is same-origin with this server, so CORS never
        // applies. The Host header is what still distinguishes it.
        let mut tokens = TokenRegistry::new();
        tokens.issue("t");
        let addr = spawn_server(tokens).await;

        let rebound = raw_get_with_host(addr, "/v1/metrics", "attacker.example").await;
        assert!(rebound.contains("403"), "got: {rebound}");
        assert!(rebound.contains("forbidden_host"), "got: {rebound}");

        // The real local caller is unaffected, including with an explicit port.
        let ok =
            raw_get_with_host(addr, "/v1/metrics", &format!("127.0.0.1:{}", addr.port())).await;
        assert!(ok.contains("200"), "got: {ok}");
        let ok_name = raw_get_with_host(addr, "/v1/status", "localhost").await;
        assert!(ok_name.contains("200"), "got: {ok_name}");
    }

    #[test]
    fn host_header_matching_is_name_only() {
        assert!(host_is_loopback(Some("127.0.0.1")));
        assert!(host_is_loopback(Some("127.0.0.1:7464")));
        assert!(host_is_loopback(Some("localhost:7464")));
        assert!(host_is_loopback(Some("[::1]:7464")));
        assert!(!host_is_loopback(Some("attacker.example")));
        assert!(!host_is_loopback(Some("localhost.attacker.example")));
        // A missing Host is HTTP/1.1-invalid and is not something a real local caller sends.
        assert!(!host_is_loopback(None));
    }

    #[tokio::test]
    async fn actions_execute_local_and_send_over_the_socket() {
        let mut tokens = TokenRegistry::new();
        tokens.issue("t");
        let addr = spawn_server(tokens).await;

        // a local action is authorized immediately (200)
        let resp = raw_post(
            addr,
            "/v1/actions/execute",
            Some("t"),
            r#"{"kind":"local_search","query":"x"}"#,
        )
        .await;
        assert!(resp.contains("200"), "got: {resp}");
        assert!(resp.contains("\"executed\":\"local\""));

        // an external send is pending L3 approval (202 + approval id) — not executed
        let resp = raw_post(
            addr,
            "/v1/actions/execute",
            Some("t"),
            r#"{"kind":"send_email","to":"a@b.com","subject":"s","body":"b"}"#,
        )
        .await;
        assert!(resp.contains("202"), "got: {resp}");
        assert!(resp.contains("\"pending\":true"));
        assert!(resp.contains("\"approval_id\":"));

        // no token → 401
        let resp = raw_post(
            addr,
            "/v1/actions/execute",
            None,
            r#"{"kind":"local_search","query":"x"}"#,
        )
        .await;
        assert!(resp.contains("401"), "got: {resp}");
    }
}
