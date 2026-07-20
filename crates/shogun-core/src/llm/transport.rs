//! The HTTP transport seam (WP3.1 network layer).
//!
//! The Anthropic clients are written against this trait, not a concrete HTTP library, so their
//! request construction, traceability recording, and response parsing are all exercised on Linux
//! with a [`MockTransport`] and no socket. The real socket implementation
//! ([`ReqwestTransport`], feature `net`) is a thin adapter.
//!
//! Two safety properties live here:
//! - **HTTPS only** (NFR-SEC-04): [`HttpRequest::new`] rejects any non-`https://` URL, and the
//!   real transport never disables certificate verification.
//! - **Secrets never print** (G7): [`HttpRequest`]'s `Debug` redacts the `x-api-key` /
//!   `authorization` headers, so a request captured by a mock or dumped in a test cannot leak the
//!   API key.

use std::fmt;
use std::future::Future;

/// HTTP method — the two verbs the Anthropic REST surface uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
        }
    }
}

/// An outbound HTTPS request. Construct with [`HttpRequest::new`], which enforces the `https://`
/// scheme; headers carrying secrets are redacted under `Debug`.
#[derive(Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl HttpRequest {
    /// Build a request, rejecting any non-HTTPS URL (NFR-SEC-04). Callers never construct the
    /// struct directly, so plaintext can never slip through.
    pub fn new(
        method: Method,
        url: impl Into<String>,
        headers: Vec<(String, String)>,
        body: Option<String>,
    ) -> Result<Self, TransportError> {
        let url = url.into();
        if !url.starts_with("https://") {
            return Err(TransportError::InsecureUrl(url));
        }
        Ok(Self { method, url, headers, body })
    }
}

/// Header names whose values are secrets and must never be printed.
fn is_secret_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("x-api-key") || name.eq_ignore_ascii_case("authorization")
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<(&str, &str)> = self
            .headers
            .iter()
            .map(|(k, v)| (k.as_str(), if is_secret_header(k) { "***redacted***" } else { v.as_str() }))
            .collect();
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &headers)
            .field("body", &self.body)
            .finish()
    }
}

/// An HTTP response: status code and full body. The transport reads the entire body (Anthropic
/// returns SSE as the body too); incremental token streaming is a later, streaming-transport
/// concern (see [`super::anthropic`]).
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Transport failures. Provider-level (non-2xx) errors are surfaced by the clients, not here;
/// this is only the wire layer.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("insecure url (https required): {0}")]
    InsecureUrl(String),
    #[error("transport error: {0}")]
    Io(String),
}

/// The transport seam. Static-dispatch only (the clients are generic over `T: HttpTransport`), so
/// the future is `Send` without `async-trait`.
pub trait HttpTransport: Send + Sync {
    fn send(
        &self,
        req: HttpRequest,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send;
}

// ---- test double ---------------------------------------------------------------------------

/// A transport that records every request and replays canned responses in order — the offline
/// stand-in that makes the Anthropic clients Linux-testable. Public so downstream crates can use
/// it in their own tests. `pub` items don't trip `dead_code`, so it carries no warning in
/// production builds.
pub struct MockTransport {
    responses: std::sync::Mutex<std::collections::VecDeque<HttpResponse>>,
    sent: std::sync::Mutex<Vec<HttpRequest>>,
}

impl MockTransport {
    /// Queue the responses the transport will return, in call order.
    pub fn new(responses: impl IntoIterator<Item = HttpResponse>) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses.into_iter().collect()),
            sent: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Convenience: a single 200 response with `body`.
    pub fn ok(body: impl Into<String>) -> Self {
        Self::new([HttpResponse { status: 200, body: body.into() }])
    }

    /// Every request the transport has seen, in order.
    pub fn sent(&self) -> Vec<HttpRequest> {
        self.sent.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

impl HttpTransport for MockTransport {
    fn send(
        &self,
        req: HttpRequest,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send {
        // Capture the request and pop the next canned response synchronously; the returned future
        // is trivially ready (and Send).
        let result = {
            if let Ok(mut s) = self.sent.lock() {
                s.push(req);
            }
            match self.responses.lock() {
                Ok(mut q) => q
                    .pop_front()
                    .ok_or_else(|| TransportError::Io("MockTransport: no queued response".into())),
                Err(_) => Err(TransportError::Io("MockTransport: poisoned".into())),
            }
        };
        std::future::ready(result)
    }
}

// ---- real transport (feature `net`) --------------------------------------------------------

/// The production transport: a `reqwest` client pinned to HTTPS with rustls. Certificate
/// verification is never disabled (NFR-SEC-04) and `https_only(true)` is a second guard beyond
/// [`HttpRequest::new`]'s scheme check.
#[cfg(feature = "net")]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

#[cfg(feature = "net")]
impl ReqwestTransport {
    pub fn new() -> Result<Self, TransportError> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .build()
            .map_err(|e| TransportError::Io(e.to_string()))?;
        Ok(Self { client })
    }
}

#[cfg(feature = "net")]
impl HttpTransport for ReqwestTransport {
    fn send(
        &self,
        req: HttpRequest,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send {
        let client = self.client.clone();
        async move {
            let method = match req.method {
                Method::Get => reqwest::Method::GET,
                Method::Post => reqwest::Method::POST,
            };
            let mut rb = client.request(method, &req.url);
            for (k, v) in &req.headers {
                rb = rb.header(k, v);
            }
            if let Some(body) = req.body {
                rb = rb.body(body);
            }
            let resp = rb.send().await.map_err(|e| TransportError::Io(e.to_string()))?;
            let status = resp.status().as_u16();
            let body = resp.text().await.map_err(|e| TransportError::Io(e.to_string()))?;
            Ok(HttpResponse { status, body })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_plaintext_url() {
        let err = HttpRequest::new(Method::Get, "http://api.anthropic.com/v1/models", vec![], None);
        assert!(matches!(err, Err(TransportError::InsecureUrl(_))));
    }

    #[test]
    fn accepts_https_url() {
        let ok = HttpRequest::new(Method::Get, "https://api.anthropic.com/v1/models", vec![], None);
        assert!(ok.is_ok());
    }

    #[test]
    fn debug_redacts_api_key_header() {
        let req = HttpRequest::new(
            Method::Post,
            "https://api.anthropic.com/v1/messages",
            vec![
                ("x-api-key".into(), "sk-secret-abcdef".into()),
                ("anthropic-version".into(), "2023-06-01".into()),
            ],
            Some("{}".into()),
        )
        .unwrap();
        let dumped = format!("{req:?}");
        assert!(!dumped.contains("sk-secret-abcdef"), "api key must not appear in Debug");
        assert!(dumped.contains("***redacted***"));
        // Non-secret headers are still visible for debugging.
        assert!(dumped.contains("2023-06-01"));
    }

    #[tokio::test]
    async fn mock_records_requests_and_replays_responses() {
        let t = MockTransport::new([
            HttpResponse { status: 200, body: "first".into() },
            HttpResponse { status: 500, body: "second".into() },
        ]);
        let r1 = t
            .send(HttpRequest::new(Method::Get, "https://x/a", vec![], None).unwrap())
            .await
            .unwrap();
        let r2 = t
            .send(HttpRequest::new(Method::Get, "https://x/b", vec![], None).unwrap())
            .await
            .unwrap();
        assert_eq!(r1.body, "first");
        assert_eq!(r2.status, 500);
        assert!(!r2.is_success());
        assert_eq!(t.sent().len(), 2);
        assert_eq!(t.sent()[1].url, "https://x/b");
    }
}
