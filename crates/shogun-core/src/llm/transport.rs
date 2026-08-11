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

/// ストリーミング応答の結末。ステータスをボディより先に確定できるので、非2xxのときに
/// 「エラー本文をデルタとして画面に流してしまう」経路が構造的に存在しない。
#[derive(Debug)]
pub enum StreamOutcome {
    /// 2xx。ボディは到着順に `on_chunk` へ渡し終えた。
    Streamed { status: u16 },
    /// 非2xx。`on_chunk` は一度も呼ばず、エラー本文だけを持って返る。
    Failed { status: u16, body: String },
}

/// 増分でボディを受け取るトランスポート。
///
/// [`HttpTransport`] と分けてある。全実装に streaming を強いるとモックもBatch lane側も
/// 巻き添えになるが、増分が要るのはAgent laneのSSEだけで、しかもそこは「最初の一文字までの
/// 時間」がSLOになっている唯一の経路だから。
///
/// チャンクはコールバックで渡す。チャネルにすると送信側と受信側を同時に走らせる必要が生じ、
/// ライブラリ側にランタイムを持ち込むことになる — が、デコードは「バイトが届いた瞬間に
/// その場で」やれば済むので、同時実行そのものが要らない。
///
/// `on_chunk` が `false` を返したらそこで打ち切る。応答の途中でパネルを閉じるのは正常な
/// 操作であって、失敗ではない。
pub trait StreamingTransport: Send + Sync {
    /// `req` を送り、ボディを届いた順に `on_chunk` へ渡す。返るのは [`StreamOutcome`]。
    ///
    /// `on_chunk` を値渡しにしているのは、await をまたいで借用しないことでフューチャを `Send`
    /// に保つため。
    fn send_streaming<F>(
        &self,
        req: HttpRequest,
        on_chunk: F,
    ) -> impl Future<Output = Result<StreamOutcome, TransportError>> + Send
    where
        F: FnMut(&str) -> bool + Send;
}

/// 決められたチャンクを順に流すだけのテスト用トランスポート。ネットワーク無しで
/// ストリーミング経路を検証するための土台。
pub struct MockStreamingTransport {
    status: u16,
    chunks: std::sync::Mutex<std::collections::VecDeque<String>>,
}

impl MockStreamingTransport {
    pub fn new(status: u16, chunks: Vec<String>) -> Self {
        Self { status, chunks: std::sync::Mutex::new(chunks.into()) }
    }
}

impl StreamingTransport for MockStreamingTransport {
    fn send_streaming<F>(
        &self,
        _req: HttpRequest,
        mut on_chunk: F,
    ) -> impl Future<Output = Result<StreamOutcome, TransportError>> + Send
    where
        F: FnMut(&str) -> bool + Send,
    {
        let queued: Vec<String> = self
            .chunks
            .lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default();
        let status = self.status;
        async move {
            // 非2xxではチャンクを1つも渡さず、本文だけを組み立てて返す — 実トランスポートと
            // 同じ振る舞い。エラー本文が回答としてパネルに出る経路を塞ぐ。
            if !(200..300).contains(&status) {
                return Ok(StreamOutcome::Failed { status, body: queued.concat() });
            }
            for c in queued {
                // コールバックが false を返したら打ち切る。閉じたパネルに向かって流し続けない。
                if !on_chunk(&c) {
                    break;
                }
            }
            Ok(StreamOutcome::Streamed { status })
        }
    }
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
///
/// Cheap to clone — the inner `reqwest::Client` is a handle onto one connection pool, which is
/// the whole point of [`Self::shared`].
#[cfg(feature = "net")]
#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

/// One pool for the whole process. See [`ReqwestTransport::shared`].
#[cfg(feature = "net")]
static SHARED_CLIENT: std::sync::OnceLock<Result<reqwest::Client, String>> =
    std::sync::OnceLock::new();

#[cfg(feature = "net")]
fn build_client() -> Result<reqwest::Client, TransportError> {
    reqwest::Client::builder()
        .https_only(true)
        // Keep the connection alive between turns. Without this every chat turn, every ⌥-tap
        // draft and every recap re-pays DNS + TCP + TLS to the provider before the model has
        // seen a single byte — on a normal link that is most of the budget the "first token in
        // 1s" SLO has to fit inside. The pool is per-host, so an idle window longer than the
        // gap between two turns is what actually gets reused.
        .pool_idle_timeout(std::time::Duration::from_secs(300))
        .pool_max_idle_per_host(4)
        .tcp_keepalive(std::time::Duration::from_secs(60))
        // Connect-only, deliberately: a whole-request timeout would cut long streamed answers
        // off mid-sentence, and a stream that is still producing tokens is not a hung request.
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| TransportError::Io(e.to_string()))
}

#[cfg(feature = "net")]
impl ReqwestTransport {
    /// A transport over the process-wide connection pool. **Prefer this over [`Self::new`]** for
    /// anything a user waits on: a fresh client starts with an empty pool, so its first request
    /// always pays a full handshake no matter how recently another one finished.
    ///
    /// The build result is memoised, failure included — if TLS cannot be initialised at all,
    /// retrying per request would just re-fail slowly.
    pub fn shared() -> Result<Self, TransportError> {
        match SHARED_CLIENT.get_or_init(|| build_client().map_err(|e| e.to_string())) {
            Ok(client) => Ok(Self { client: client.clone() }),
            Err(e) => Err(TransportError::Io(e.clone())),
        }
    }

    /// A transport with its own private connection pool. Only for callers that genuinely want
    /// isolation; everything on a latency path wants [`Self::shared`].
    pub fn new() -> Result<Self, TransportError> {
        Ok(Self { client: build_client()? })
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

/// `carry` に溜まったバイトのうち、有効なUTF-8として確定した前半を切り出す。
///
/// 途中で切れた文字（`error_len() == None`）は `None` を返して次のチャンクを待つ。一方
/// **そもそも不正なバイト列は捨てる** — 待っても直らないので、残しておくと carry の先頭に
/// 居座り、以降のチャンクが永久に出力されなくなる（carry も無限に伸びる）。
#[cfg(feature = "net")]
fn take_valid_utf8(carry: &mut Vec<u8>) -> Option<String> {
    loop {
        let (valid_to, invalid_len) = match std::str::from_utf8(carry) {
            Ok(_) => (carry.len(), None),
            Err(e) => (e.valid_up_to(), e.error_len()),
        };
        if valid_to > 0 {
            // `carry[..valid_to]` は構築上つねに有効なUTF-8なので、ここで文字は失われない。
            let s = String::from_utf8_lossy(&carry[..valid_to]).into_owned();
            carry.drain(..valid_to);
            return Some(s);
        }
        // `None` は途中で切れた文字（または carry が空）— 次のチャンクを待つ。`Some(n)` は
        // 直らないバイト列。捨てて、その後ろに有効な文字が続いていないか見直す。
        let n = invalid_len?;
        carry.drain(..n);
    }
}

#[cfg(feature = "net")]
impl StreamingTransport for ReqwestTransport {
    fn send_streaming<F>(
        &self,
        req: HttpRequest,
        mut on_chunk: F,
    ) -> impl Future<Output = Result<StreamOutcome, TransportError>> + Send
    where
        F: FnMut(&str) -> bool + Send,
    {
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
            let mut resp = rb.send().await.map_err(|e| TransportError::Io(e.to_string()))?;
            let status = resp.status().as_u16();
            // ステータスはボディより先に確定する。非2xxならチャンクを1つも渡さず、本文を
            // 集め切って `Failed` で返す — エラー本文がSSEデルタとして画面に出る経路を塞ぐ。
            if !(200..300).contains(&status) {
                let body = resp.text().await.map_err(|e| TransportError::Io(e.to_string()))?;
                return Ok(StreamOutcome::Failed { status, body });
            }
            // マルチバイト文字はチャンク境界をまたぐ（日本語の応答ではほぼ毎回起きる）。
            // 到着したバイトをそのまま lossy 変換すると、境界にかかった1文字は置換文字に
            // なって二度と戻らない。`take_valid_utf8` で有効な前半だけを渡し、途中で切れた
            // 文字のバイトは次のチャンクの頭と繋ぐために持ち越す。
            let mut carry: Vec<u8> = Vec::new();
            while let Some(bytes) =
                resp.chunk().await.map_err(|e| TransportError::Io(e.to_string()))?
            {
                carry.extend_from_slice(&bytes);
                if let Some(s) = take_valid_utf8(&mut carry) {
                    // 届いたその場でコールバックへ。false なら打ち切る（パネルが閉じた）。
                    if !on_chunk(&s) {
                        break;
                    }
                }
                // take_valid_utf8 が None を返したときはまだ1文字も完成していない。
                // 次のチャンクを待つ。
            }
            // ストリームが閉じたとき carry に残っているバイトは捨てる。正常な SSE は必ず
            // 行末で終わるので、ここに文字の途中のバイトが来ることはない。
            Ok(StreamOutcome::Streamed { status })
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

    /// ストリーミング用のモックが、渡された順にチャンクをコールバックへ渡すこと。
    #[tokio::test]
    async fn mock_streaming_transport_delivers_chunks_in_order() {
        let t = MockStreamingTransport::new(200, vec!["one ".into(), "two".into()]);
        let req =
            HttpRequest::new(Method::Post, "https://api.anthropic.com/v1/messages", vec![], None)
                .unwrap();

        let mut got: Vec<String> = Vec::new();
        let outcome = t
            .send_streaming(req, |c| {
                got.push(c.to_string());
                true
            })
            .await
            .unwrap();

        assert!(matches!(outcome, StreamOutcome::Streamed { status: 200 }));
        assert_eq!(got, vec!["one ".to_string(), "two".to_string()]);
    }

    /// コールバックが `false` を返す（＝パネルが閉じた）と、その場でストリームを打ち切り、
    /// エラーにせず `Ok` で戻る。応答の途中で閉じるのは正常な操作で、失敗ではない。
    #[tokio::test]
    async fn a_callback_returning_false_stops_the_stream_early() {
        let t = MockStreamingTransport::new(200, vec!["one".into(), "two".into()]);
        let req =
            HttpRequest::new(Method::Post, "https://api.anthropic.com/v1/messages", vec![], None)
                .unwrap();

        let mut seen: Vec<String> = Vec::new();
        let outcome = t
            .send_streaming(req, |c| {
                seen.push(c.to_string());
                // 最初の1つだけ受け取って打ち切る。
                false
            })
            .await
            .unwrap();

        assert!(matches!(outcome, StreamOutcome::Streamed { status: 200 }));
        assert_eq!(seen, vec!["one".to_string()], "打ち切り後もチャンクを渡している");
    }

    /// 非2xxではデルタを1つも流さない。エラー本文が回答としてパネルに出る経路を塞ぐ。
    #[tokio::test]
    async fn a_failed_status_streams_no_chunks() {
        let t = MockStreamingTransport::new(401, vec!["{\"error\":\"bad key\"}".into()]);
        let req = HttpRequest::new(Method::Post, "https://api.anthropic.com/v1/messages", vec![], None).unwrap();
        let mut seen = 0usize;

        let outcome = t.send_streaming(req, |_| { seen += 1; true }).await.unwrap();

        assert_eq!(seen, 0, "エラー本文がチャンクとして流れた");
        assert!(matches!(outcome, StreamOutcome::Failed { status: 401, .. }));
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

    /// マルチバイト文字がチャンク境界にかかっても壊れない。日本語の応答では常に起きる。
    #[cfg(feature = "net")]
    #[test]
    fn a_multibyte_character_split_across_chunks_survives() {
        // "日本" = 6 bytes. 最初のチャンクが1文字目の途中で切れる。
        let full = "日本".as_bytes();
        let (head, tail) = full.split_at(4);

        let mut carry = head.to_vec();
        let first = take_valid_utf8(&mut carry).expect("1文字目は確定しているはず");
        assert_eq!(first, "日");
        assert_eq!(carry.len(), 1, "2文字目の途中のバイトが持ち越されていない");

        carry.extend_from_slice(tail);
        let second = take_valid_utf8(&mut carry).expect("2文字目が確定するはず");
        assert_eq!(second, "本");
        assert!(carry.is_empty());

        assert_eq!(format!("{first}{second}"), "日本", "文字が壊れた");
    }

    /// 1文字も完成していないチャンクでは何も出さず、バイトを捨てもしない。
    #[cfg(feature = "net")]
    #[test]
    fn an_incomplete_first_character_is_held_not_dropped() {
        let mut carry = "日".as_bytes()[..2].to_vec();
        assert!(take_valid_utf8(&mut carry).is_none());
        assert_eq!(carry.len(), 2, "確定前のバイトが捨てられた");
    }

    /// 直らない不正バイトは捨てて先へ進む。残すと carry の先頭に居座って、以降の応答が
    /// 永久に出なくなる（レビューで見つかった livelock）。
    #[cfg(feature = "net")]
    #[test]
    fn an_invalid_byte_is_dropped_rather_than_blocking_the_stream() {
        let mut carry = vec![0xFF];
        carry.extend_from_slice("ok".as_bytes());

        assert_eq!(take_valid_utf8(&mut carry).as_deref(), Some("ok"));
        assert!(carry.is_empty());
    }

    /// 不正バイトだけが届いた場合も詰まらない: 何も返さないが、バイトは溜め込まない。
    #[cfg(feature = "net")]
    #[test]
    fn a_lone_invalid_byte_does_not_accumulate() {
        let mut carry = vec![0xFF];
        assert!(take_valid_utf8(&mut carry).is_none());
        assert!(carry.is_empty(), "不正バイトが carry に残っている（livelockの条件）");

        // 次のチャンクは普通に読める。
        carry.extend_from_slice("hi".as_bytes());
        assert_eq!(take_valid_utf8(&mut carry).as_deref(), Some("hi"));
    }
}
