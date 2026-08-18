//! Deepgram Nova-3 ASR (meeting default, 2026-08-05).
//!
//! **Key security:** the company Deepgram key must never ship inside the desktop binary or a shared
//! Keychain secret. Auth goes through [`DeepgramAuth`]:
//! - production: ephemeral token from SHOGUN/Select backend (backend holds the key, or mints via
//!   Deepgram `POST /v1/auth/grant`);
//! - Keychain `deepgram-asr` (user pastes once in Settings);
//! - local debug only: `SHOGUN_DEEPGRAM_API_KEY` behind `#[cfg(debug_assertions)]`.
//!
//! **Primary path:** live WebSocket (`wss://…/v1/listen`) — stream linear16 PCM while speaking,
//! finalize on `CloseStream` / utterance end (Wispr/Willow-style). HTTP `/v1/listen` remains the
//! fallback when the WS handshake fails.
//!
//! Always sends `mip_opt_out=true`. Waveform never written to disk by SHOGUN.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::super::{Segment, SAMPLE_RATE};
use super::Transcriber;
use crate::llm::traceability::{digest, Route, TraceRecord, TraceabilitySink};

const DEFAULT_LISTEN: &str = "https://api.deepgram.com/v1/listen";
const DEFAULT_MODEL: &str = "nova-3";
/// Deepgram endpointing silence (ms) — matches local VAD hangover.
const LIVE_ENDPOINTING_MS: u32 = 300;
/// ~100 ms of 16 kHz mono before flushing a WS binary frame.
pub const LIVE_CHUNK_SAMPLES: usize = (SAMPLE_RATE as usize) / 10;
/// Most linear16 bytes allowed to sit queued for the WS thread: 30 seconds of 16 kHz mono
/// (2 bytes/sample) — the same wall [`crate::audio::ring::MAX_SECONDS`] puts on captured PCM.
/// Without it a stalled socket turns "keep what the network hasn't caught up on" into "keep
/// everything" (~32 KB/s of RAM, unbounded); once full, new audio is dropped and counted.
pub const LIVE_QUEUE_MAX_BYTES: usize = 30 * 2 * (SAMPLE_RATE as usize);

/// How the client obtains an Authorization header value (`Token …` or `Bearer …`).
pub trait DeepgramAuth: Send {
    fn authorization_header(&mut self) -> Result<String, String>;
}

/// Production path: fetch a short-lived JWT from the SHOGUN/Select backend.
///
/// Expected JSON: `{ "access_token": "<jwt>", "expires_in": <secs> }` (Deepgram grant shape).
/// Backend holds the company key and calls Deepgram `/v1/auth/grant` (or proxies listen).
///
/// The mint request is **authenticated with this device's licence token** (FR-BIL-08): the
/// endpoint spends the company Deepgram key, so an unauthenticated mint would let anyone who
/// learns the URL issue themselves tokens against it. The backend verifies the same
/// `v1.<payload>.<sig>` token the Batch relay does (apps/api/README-asr-proxy.md).
pub struct EphemeralTokenAuth {
    token_url: String,
    /// Resolved per mint, not once: the token expires every ~24h and is re-verified on a timer,
    /// so a long-running app must pick up the refreshed one.
    license: fn() -> Option<String>,
    client: reqwest::blocking::Client,
    cached: Option<(String, Instant, Duration)>,
}

impl EphemeralTokenAuth {
    /// Mint against `token_url`, presenting the device's cached licence token.
    pub fn new(token_url: impl Into<String>) -> Result<Self, String> {
        Self::with_license_source(token_url, crate::license_client::cached_license_token)
    }

    /// Same, with the licence source injected — the seam the tests use to drive the "no licence"
    /// and "licence attached" paths without a Keychain or a billing.json.
    pub fn with_license_source(
        token_url: impl Into<String>,
        license: fn() -> Option<String>,
    ) -> Result<Self, String> {
        let token_url = token_url.into();
        // The mint carries a bearer credential; plain HTTP would put it (and the returned
        // Deepgram token) on the wire in the clear.
        if !token_url.starts_with("https://") {
            return Err("SHOGUN_ASR_TOKEN_URL must be an https:// URL".into());
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| format!("deepgram auth http client: {e}"))?;
        Ok(Self { token_url, license, client, cached: None })
    }
}

impl DeepgramAuth for EphemeralTokenAuth {
    fn authorization_header(&mut self) -> Result<String, String> {
        if let Some((ref token, at, ttl)) = self.cached {
            // Refresh 5s before expiry.
            if at.elapsed() + Duration::from_secs(5) < ttl {
                return Ok(format!("Bearer {token}"));
            }
        }
        // No entitled licence → no mint. Degrading here is correct: the alternative is an
        // anonymous request against a spend-bearing endpoint.
        let license = (self.license)().ok_or_else(|| {
            "no active licence on this device — meeting transcription needs a verified \
             subscription"
                .to_string()
        })?;
        let resp = self
            .client
            .post(&self.token_url)
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {license}"))
            .send()
            .map_err(|e| format!("asr token fetch failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("asr token fetch HTTP {}", resp.status()));
        }
        let body: serde_json::Value =
            resp.json().map_err(|e| format!("asr token JSON: {e}"))?;
        let token = body
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "asr token response missing access_token".to_string())?
            .to_string();
        let expires_in = body
            .get("expires_in")
            .and_then(|v| v.as_u64())
            .unwrap_or(30)
            .max(5);
        let ttl = Duration::from_secs(expires_in);
        self.cached = Some((token.clone(), Instant::now(), ttl));
        Ok(format!("Bearer {token}"))
    }
}

/// Keychain-backed API key (`com.selectkk.shogun` / `deepgram-asr`). Cached per session.
/// macOS-only: the Keychain store does not exist on other targets (Linux CI builds `net`).
#[cfg(target_os = "macos")]
#[derive(Default)]
pub struct KeychainKeyAuth {
    cached: Option<String>,
}

#[cfg(target_os = "macos")]
impl DeepgramAuth for KeychainKeyAuth {
    fn authorization_header(&mut self) -> Result<String, String> {
        if let Some(ref key) = self.cached {
            return Ok(format!("Token {key}"));
        }
        let key = shogun_integrations::keychain_store::get_deepgram_asr_key().ok_or_else(|| {
            "Deepgram API key not in Keychain — add it in Settings → Meeting notes".to_string()
        })?;
        self.cached = Some(key.clone());
        Ok(format!("Token {key}"))
    }
}

/// Debug-only long-lived key from env. **Never compiled into release auth resolution.**
#[derive(Default)]
pub struct DebugEnvKeyAuth;

impl DeepgramAuth for DebugEnvKeyAuth {
    fn authorization_header(&mut self) -> Result<String, String> {
        #[cfg(debug_assertions)]
        {
            let key = std::env::var("SHOGUN_DEEPGRAM_API_KEY").map_err(|_| {
                "set SHOGUN_DEEPGRAM_API_KEY for debug Deepgram ASR (never ships in release)"
                    .to_string()
            })?;
            if key.trim().is_empty() {
                return Err("SHOGUN_DEEPGRAM_API_KEY is empty".into());
            }
            Ok(format!("Token {}", key.trim()))
        }
        #[cfg(not(debug_assertions))]
        {
            Err("Deepgram debug API key is unavailable in release builds".into())
        }
    }
}

/// Wire config. Auth is separate so paid-plan gating can live in Rust core later without
/// baking secrets into the binary.
#[derive(Debug, Clone)]
pub struct DeepgramConfig {
    pub listen_endpoint: String,
    pub model: String,
    /// Deepgram `language` query param (`multi`, `en`, `ja`, …).
    pub language: String,
    /// Always true in production construction.
    pub mip_opt_out: bool,
    /// Traceability `purpose` (e.g. `meeting_asr`, `voice_dictation`).
    pub purpose: String,
}

impl Default for DeepgramConfig {
    fn default() -> Self {
        Self {
            listen_endpoint: DEFAULT_LISTEN.into(),
            model: DEFAULT_MODEL.into(),
            language: "multi".into(),
            mip_opt_out: true,
            purpose: "meeting_asr".into(),
        }
    }
}

impl DeepgramConfig {
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    pub fn with_purpose(mut self, purpose: impl Into<String>) -> Self {
        self.purpose = purpose.into();
        self
    }
}

/// Resolve auth for the meeting lane: token URL → Keychain → debug env key → error.
///
/// `SHOGUN_ASR_TOKEN_URL` points at the Select/SHOGUN mint endpoint (production). Keychain holds
/// the user's pasted Deepgram key. `SHOGUN_DEEPGRAM_API_KEY` is a debug-only override.
pub fn resolve_auth() -> Result<Box<dyn DeepgramAuth>, String> {
    if let Ok(url) = std::env::var("SHOGUN_ASR_TOKEN_URL") {
        let url = url.trim().to_string();
        if !url.is_empty() {
            // `EphemeralTokenAuth::new` enforces https and attaches the licence token.
            return Ok(Box::new(EphemeralTokenAuth::new(url)?));
        }
    }
    #[cfg(target_os = "macos")]
    if shogun_integrations::keychain_store::deepgram_asr_configured() {
        return Ok(Box::new(KeychainKeyAuth::default()));
    }
    #[cfg(debug_assertions)]
    {
        if std::env::var("SHOGUN_DEEPGRAM_API_KEY").is_ok() {
            return Ok(Box::new(DebugEnvKeyAuth));
        }
    }
    Err(
        "Deepgram ASR needs a speech provider key in Settings, SHOGUN_ASR_TOKEN_URL \
         (production ephemeral token), or SHOGUN_DEEPGRAM_API_KEY (debug builds only)"
            .into(),
    )
}

/// Utterance-batch Deepgram listen client implementing [`Transcriber`].
///
/// The traceability sink is **required**, not optional (invariant 3): audio leaving the device
/// for a third party is exactly the egress the ledger exists to show, and a client that could be
/// built without a sink made "sent, unrecorded" a reachable state whenever the DB was not in
/// Tauri state. A caller that cannot supply a sink cannot start ASR.
pub struct Deepgram {
    config: DeepgramConfig,
    auth: Box<dyn DeepgramAuth>,
    client: reqwest::blocking::Client,
    trace: Arc<dyn TraceabilitySink>,
}

impl Deepgram {
    pub fn new(
        config: DeepgramConfig,
        auth: Box<dyn DeepgramAuth>,
        trace: Arc<dyn TraceabilitySink>,
    ) -> Result<Self, String> {
        if !config.mip_opt_out {
            return Err("Deepgram mip_opt_out must be true (company policy)".into());
        }
        check_listen_endpoint(&config.listen_endpoint)?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| format!("deepgram http client: {e}"))?;
        Ok(Self { config, auth, client, trace })
    }

    fn listen_url(&self) -> String {
        http_listen_url(&self.config)
    }

    fn record_egress(&self, pcm_bytes: usize, duration_ms: u64) {
        record_asr_egress(
            self.trace.as_ref(),
            &self.config.purpose,
            pcm_bytes,
            duration_ms,
        );
    }
}

/// Meeting vs hold-to-talk live query params.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveMode {
    /// Interim partials + endpointing for the meeting overlay.
    Meeting,
    /// Dictation finalize-on-release; interims off (UI hides them anyway).
    Voice,
}

/// One transcript event from a live WebSocket session.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveResult {
    pub text: String,
    pub is_final: bool,
    pub speech_final: bool,
    pub confidence: f64,
}

enum LiveCmd {
    Audio(Vec<u8>),
    Close,
}

/// Non-blocking handle to a Deepgram live listen session (dedicated WS thread).
///
/// Push PCM from the audio poll thread; drain [`LiveResult`]s without waiting on the network.
pub struct DeepgramLive {
    cmd_tx: Sender<LiveCmd>,
    result_rx: Receiver<LiveResult>,
    join: Option<JoinHandle<()>>,
    pcm_bytes: usize,
    /// Linear16 bytes currently queued for the WS thread (incremented in [`Self::push_pcm`],
    /// decremented by the thread as it consumes) — the [`LIVE_QUEUE_MAX_BYTES`] backpressure gauge.
    queued_bytes: Arc<AtomicUsize>,
    /// Bytes dropped in the current full-queue burst; nonzero means the "queue full" line has
    /// already been logged for this burst (one line per burst, not per chunk).
    dropped_bytes: usize,
    purpose: String,
    /// Required, for the same reason as [`Deepgram::trace`].
    trace: Arc<dyn TraceabilitySink>,
    /// Whether the session's egress has been recorded ([`Self::finish_finals`] does it on the
    /// normal path; [`Drop`] covers abnormal teardown so no streamed audio goes undisclosed).
    traced: bool,
}

impl DeepgramLive {
    /// Open `wss://…/v1/listen`. Fails fast so callers can fall back to HTTP batch.
    pub fn connect(
        config: &DeepgramConfig,
        auth: &mut dyn DeepgramAuth,
        mode: LiveMode,
        trace: Arc<dyn TraceabilitySink>,
    ) -> Result<Self, String> {
        if !config.mip_opt_out {
            return Err("Deepgram mip_opt_out must be true (company policy)".into());
        }
        check_listen_endpoint(&config.listen_endpoint)?;
        let url = live_listen_url(config, mode);
        let authorization = auth
            .authorization_header()
            .map_err(|e| format!("deepgram auth failed: {e}"))?;

        let (cmd_tx, cmd_rx) = mpsc::channel::<LiveCmd>();
        let (result_tx, result_rx) = mpsc::channel::<LiveResult>();
        let purpose = config.purpose.clone();
        let queued_bytes = Arc::new(AtomicUsize::new(0));
        let thread_queued = queued_bytes.clone();

        let join = thread::Builder::new()
            .name("deepgram-live".into())
            .spawn(move || {
                if let Err(e) = live_session_loop(&url, &authorization, cmd_rx, result_tx, &thread_queued) {
                    eprintln!("[asr] deepgram live session ended: {e}");
                }
            })
            .map_err(|e| format!("deepgram live thread: {e}"))?;

        Ok(Self {
            cmd_tx,
            result_rx,
            join: Some(join),
            pcm_bytes: 0,
            queued_bytes,
            dropped_bytes: 0,
            purpose,
            trace,
            traced: false,
        })
    }

    /// Queue linear16 PCM (non-blocking for the caller beyond a short channel send).
    ///
    /// Bounded like the capture [`Ring`](crate::audio::ring::Ring): once [`LIVE_QUEUE_MAX_BYTES`]
    /// (30 s) sit unconsumed — a stalled socket, a wedged WS thread — new audio is dropped rather
    /// than queued. A gap in the transcript is honest; unbounded RAM growth is not. Dropped bytes
    /// never count toward `pcm_bytes`, so the egress record stays what actually left the device.
    pub fn push_pcm(&mut self, pcm: &[f32]) -> Result<(), String> {
        if pcm.is_empty() {
            return Ok(());
        }
        let bytes = f32_to_linear16(pcm);
        if self
            .queued_bytes
            .load(Ordering::Acquire)
            .saturating_add(bytes.len())
            > LIVE_QUEUE_MAX_BYTES
        {
            if self.dropped_bytes == 0 {
                eprintln!(
                    "[asr] deepgram live queue full ({}s of audio unsent) — dropping new audio until the socket drains",
                    LIVE_QUEUE_MAX_BYTES / (2 * SAMPLE_RATE as usize)
                );
            }
            self.dropped_bytes = self.dropped_bytes.saturating_add(bytes.len());
            return Ok(());
        }
        if self.dropped_bytes > 0 {
            eprintln!(
                "[asr] deepgram live queue drained — dropped {} byte(s) of audio during the stall",
                self.dropped_bytes
            );
            self.dropped_bytes = 0;
        }
        let len = bytes.len();
        self.queued_bytes.fetch_add(len, Ordering::AcqRel);
        match self.cmd_tx.send(LiveCmd::Audio(bytes)) {
            Ok(()) => {
                self.pcm_bytes = self.pcm_bytes.saturating_add(len);
                Ok(())
            }
            Err(_) => {
                // Never entered the queue (and never streamed): keep the gauge and the egress
                // byte count in step with what the thread actually consumes.
                self.queued_bytes.fetch_sub(len, Ordering::AcqRel);
                Err("deepgram live session closed".to_string())
            }
        }
    }

    /// Non-blocking drain of one transcript event.
    pub fn try_recv(&self) -> Option<LiveResult> {
        self.result_rx.try_recv().ok()
    }

    /// Drain all currently available results.
    pub fn drain(&self) -> Vec<LiveResult> {
        let mut out = Vec::new();
        while let Some(r) = self.try_recv() {
            out.push(r);
        }
        out
    }

    /// Send `CloseStream`, wait for remaining finals (bounded), join the WS thread.
    /// Returns each final transcript segment (voice joins these; meetings emit one-by-one).
    pub fn finish_finals(mut self) -> Result<Vec<String>, String> {
        let _ = self.cmd_tx.send(LiveCmd::Close);
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut finals = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match self.result_rx.recv_timeout(remaining.min(Duration::from_millis(200))) {
                Ok(r) => {
                    if r.is_final && !r.text.trim().is_empty() {
                        finals.push(r.text.trim().to_string());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.join.as_ref().is_some_and(|j| j.is_finished()) {
                        break;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        for r in self.drain() {
            if r.is_final && !r.text.trim().is_empty() {
                finals.push(r.text.trim().to_string());
            }
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        let duration_ms = ((self.pcm_bytes as u64 / 2) * 1000) / u64::from(SAMPLE_RATE);
        self.traced = true;
        record_asr_egress(
            self.trace.as_ref(),
            &self.purpose,
            self.pcm_bytes,
            duration_ms,
        );
        Ok(finals)
    }

    /// Send `CloseStream`, wait for remaining finals (bounded), join the WS thread.
    /// Returns concatenated final transcripts (for voice dictation).
    pub fn finish(self) -> Result<String, String> {
        Ok(self.finish_finals()?.join(" "))
    }
}

impl Drop for DeepgramLive {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(LiveCmd::Close);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        // Abnormal teardown (handle dropped without `finish_finals` — caller error path, aborted
        // meeting, unwinding): the audio already streamed to Deepgram regardless, so the session
        // must still leave its egress record (invariant 3 / 2026-08-05 ASR exception).
        if !self.traced && self.pcm_bytes > 0 {
            self.traced = true;
            let duration_ms = ((self.pcm_bytes as u64 / 2) * 1000) / u64::from(SAMPLE_RATE);
            record_asr_egress(self.trace.as_ref(), &self.purpose, self.pcm_bytes, duration_ms);
        }
    }
}

fn live_session_loop(
    url: &str,
    authorization: &str,
    cmd_rx: Receiver<LiveCmd>,
    result_tx: Sender<LiveResult>,
    queued_bytes: &AtomicUsize,
) -> Result<(), String> {
    use tungstenite::client::IntoClientRequest;
    use tungstenite::http::header::{AUTHORIZATION, HeaderValue};
    use tungstenite::{connect, Error as WsError, Message};

    let mut request = url
        .into_client_request()
        .map_err(|e| format!("deepgram live request: {e}"))?;
    let auth_val = HeaderValue::from_str(authorization)
        .map_err(|e| format!("deepgram live auth header: {e}"))?;
    request.headers_mut().insert(AUTHORIZATION, auth_val);

    let (mut socket, _resp) = connect(request).map_err(|e| format!("deepgram live connect: {e}"))?;
    set_live_read_timeout(socket.get_mut(), Duration::from_millis(20));

    let mut closing = false;
    let mut close_since: Option<Instant> = None;

    loop {
        // Outbound audio / CloseStream first so we never starve sends behind reads.
        match cmd_rx.try_recv() {
            Ok(LiveCmd::Audio(bytes)) => {
                // Consumed either way (sent or discarded while closing) — the push side's
                // backpressure gauge tracks what still sits in the channel.
                queued_bytes.fetch_sub(bytes.len(), std::sync::atomic::Ordering::AcqRel);
                if !closing {
                    socket
                        .send(Message::Binary(bytes))
                        .map_err(|e| format!("deepgram live send audio: {e}"))?;
                }
            }
            Ok(LiveCmd::Close) => {
                if !closing {
                    closing = true;
                    close_since = Some(Instant::now());
                    let _ = socket.send(Message::Text(r#"{"type":"CloseStream"}"#.into()));
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                if !closing {
                    closing = true;
                    close_since = Some(Instant::now());
                    let _ = socket.send(Message::Text(r#"{"type":"CloseStream"}"#.into()));
                }
            }
        }

        match socket.read() {
            Ok(Message::Text(text)) => {
                if let Some(r) = parse_live_message(&text) {
                    if result_tx.send(r).is_err() {
                        break;
                    }
                }
            }
            Ok(Message::Binary(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => {}
            Ok(Message::Ping(p)) => {
                let _ = socket.send(Message::Pong(p));
            }
            Ok(Message::Close(_)) => break,
            Err(WsError::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                if closing {
                    let timed_out = close_since
                        .is_some_and(|t| t.elapsed() > Duration::from_secs(5));
                    if timed_out || matches!(cmd_rx.try_recv(), Err(TryRecvError::Disconnected)) {
                        // Grace period after CloseStream — peer should have flushed finals.
                        if timed_out {
                            break;
                        }
                    }
                } else {
                    // Idle: wait briefly for the next PCM chunk instead of busy-spinning.
                    match cmd_rx.recv_timeout(Duration::from_millis(5)) {
                        Ok(LiveCmd::Audio(bytes)) => {
                            queued_bytes
                                .fetch_sub(bytes.len(), std::sync::atomic::Ordering::AcqRel);
                            socket
                                .send(Message::Binary(bytes))
                                .map_err(|e| format!("deepgram live send audio: {e}"))?;
                        }
                        Ok(LiveCmd::Close) => {
                            closing = true;
                            close_since = Some(Instant::now());
                            let _ = socket.send(Message::Text(r#"{"type":"CloseStream"}"#.into()));
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => {
                            closing = true;
                            close_since = Some(Instant::now());
                            let _ = socket.send(Message::Text(r#"{"type":"CloseStream"}"#.into()));
                        }
                    }
                }
            }
            Err(WsError::ConnectionClosed) | Err(WsError::AlreadyClosed) => break,
            Err(e) => {
                if closing {
                    break;
                }
                return Err(format!("deepgram live read: {e}"));
            }
        }
    }

    let _ = socket.close(None);
    Ok(())
}

fn set_live_read_timeout(stream: &mut tungstenite::stream::MaybeTlsStream<std::net::TcpStream>, dur: Duration) {
    use tungstenite::stream::MaybeTlsStream;
    match stream {
        MaybeTlsStream::Plain(tcp) => {
            let _ = tcp.set_read_timeout(Some(dur));
        }
        MaybeTlsStream::Rustls(s) => {
            let _ = s.sock.set_read_timeout(Some(dur));
        }
        _ => {}
    }
}

fn parse_live_message(text: &str) -> Option<LiveResult> {
    let body: serde_json::Value = serde_json::from_str(text).ok()?;
    let ty = body.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if ty != "Results" {
        return None;
    }
    let alt = body.pointer("/channel/alternatives/0")?;
    let transcript = alt
        .get("transcript")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if transcript.is_empty() {
        return None;
    }
    let confidence = alt
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.9)
        .clamp(0.0, 1.0);
    Some(LiveResult {
        text: transcript.to_string(),
        is_final: body.get("is_final").and_then(|v| v.as_bool()).unwrap_or(false),
        speech_final: body
            .get("speech_final")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        confidence,
    })
}

fn record_asr_egress(
    sink: &dyn TraceabilitySink,
    purpose: &str,
    pcm_bytes: usize,
    duration_ms: u64,
) {
    // Digest duration meta only — never the waveform (invariant 2 / G8).
    let meta = format!("duration_ms={duration_ms}");
    sink.record(TraceRecord {
        route: Route::Asr,
        purpose: purpose.to_string(),
        destination: "api.deepgram.com".into(),
        chunk_bytes: pcm_bytes,
        chunk_xxh64: digest(&meta),
        third_party: true,
    });
}

/// HTTPS batch listen URL (VAD-cut fallback).
pub fn http_listen_url(config: &DeepgramConfig) -> String {
    let mut url = format!(
        "{}?model={}&language={}&mip_opt_out=true&smart_format=true&encoding=linear16&sample_rate={}&channels=1",
        config.listen_endpoint,
        urlencoding_lite(&config.model),
        urlencoding_lite(&config.language),
        SAMPLE_RATE
    );
    if is_dictation_purpose(&config.purpose) {
        url.push_str("&dictation=true");
    }
    url
}

/// Live WebSocket listen URL (`wss://` + streaming query params).
pub fn live_listen_url(config: &DeepgramConfig, mode: LiveMode) -> String {
    let ws_base = https_to_wss(&config.listen_endpoint);
    let mut url = format!(
        "{}?model={}&language={}&mip_opt_out=true&smart_format=true&encoding=linear16&sample_rate={}&channels=1",
        ws_base,
        urlencoding_lite(&config.model),
        urlencoding_lite(&config.language),
        SAMPLE_RATE
    );
    match mode {
        LiveMode::Meeting => {
            // Endpointing matches local VAD hangover; interims feed the overlay.
            url.push_str(&format!(
                "&interim_results=true&endpointing={LIVE_ENDPOINTING_MS}"
            ));
        }
        LiveMode::Voice => {
            // Finalize only on CloseStream (key release) — do not endpoint mid-hold.
            url.push_str("&dictation=true&interim_results=false&endpointing=false");
        }
    }
    url
}

/// Refuse a listen endpoint that would carry meeting audio and the Authorization header in
/// cleartext. Same rule as the mint URL check in [`EphemeralTokenAuth::with_license_source`]:
/// TLS (`https://` / `wss://`) anywhere; plain `http://` / `ws://` only to the local host (mock
/// servers in tests and local dev never leave the machine). A schemeless endpoint is fine —
/// [`https_to_wss`] defaults it to `wss://`.
fn check_listen_endpoint(endpoint: &str) -> Result<(), String> {
    if endpoint.starts_with("http://") || endpoint.starts_with("ws://") {
        let rest = endpoint.split("://").nth(1).unwrap_or("");
        if !is_local_host(rest) {
            return Err(format!(
                "Deepgram listen endpoint must be https:// or wss:// (plain {} is allowed only \
                 for localhost)",
                endpoint.split("://").next().unwrap_or("http")
            ));
        }
    }
    Ok(())
}

/// Whether the authority at the head of `rest` (scheme already stripped) is the local host.
fn is_local_host(rest: &str) -> bool {
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

fn https_to_wss(endpoint: &str) -> String {
    if let Some(rest) = endpoint.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = endpoint.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if endpoint.starts_with("wss://") || endpoint.starts_with("ws://") {
        endpoint.to_string()
    } else {
        format!("wss://{endpoint}")
    }
}

fn is_dictation_purpose(purpose: &str) -> bool {
    // voice_lane uses `voice_dictation`; match any purpose containing "dictation".
    purpose.contains("dictation")
}

impl Deepgram {
    /// Batch transcribe one utterance; surfaces auth/HTTP errors to callers (voice dictation).
    pub fn transcribe_utterance(&mut self, pcm: &[f32]) -> Result<String, String> {
        let segs = self.listen(pcm)?;
        let text: String = segs
            .iter()
            .map(|s| s.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        Ok(text)
    }

    fn listen(&mut self, pcm: &[f32]) -> Result<Vec<Segment>, String> {
        if pcm.is_empty() {
            return Ok(Vec::new());
        }
        let linear16 = f32_to_linear16(pcm);
        let duration_ms = (pcm.len() as u64 * 1000) / u64::from(SAMPLE_RATE);
        let auth = self
            .auth
            .authorization_header()
            .map_err(|e| format!("deepgram auth failed: {e}"))?;
        let url = self.listen_url();
        // Record the egress BEFORE the request: the audio has crossed the Deepgram boundary the
        // moment the body is transmitted, and a 4xx/5xx answer does not un-send it. Tracing only
        // after the 2xx check would leave a failed call's audio undisclosed — the exact gap the
        // 2026-08-05 ASR exception requires this log to close (invariant 3).
        self.record_egress(linear16.len(), duration_ms);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", auth)
            .header("Content-Type", "audio/raw")
            .body(linear16)
            .send()
            .map_err(|e| format!("deepgram listen request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("deepgram listen HTTP {}", resp.status()));
        }
        parse_listen_response(resp)
    }
}

impl Transcriber for Deepgram {
    fn transcribe(&mut self, pcm: &[f32]) -> Vec<Segment> {
        match self.listen(pcm) {
            Ok(segs) => segs,
            Err(e) => {
                eprintln!("[asr] {e}");
                Vec::new()
            }
        }
    }

    // Deepgram has no whisper-style translate task. JA→EN uses Select KK in the desktop sink.
    fn translate_to_english(&mut self, _pcm: &[f32]) -> Vec<Segment> {
        Vec::new()
    }
}

fn f32_to_linear16(pcm: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pcm.len() * 2);
    for &s in pcm {
        let clamped = s.clamp(-1.0, 1.0);
        let i = (clamped * 32767.0) as i16;
        out.extend_from_slice(&i.to_le_bytes());
    }
    out
}

fn parse_listen_response(resp: reqwest::blocking::Response) -> Result<Vec<Segment>, String> {
    let body: serde_json::Value =
        resp.json().map_err(|e| format!("deepgram JSON: {e}"))?;
    let alt = body
        .pointer("/results/channels/0/alternatives/0")
        .ok_or_else(|| "deepgram response missing alternatives".to_string())?;
    let text = alt
        .get("transcript")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let confidence = alt.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.9);
    Ok(vec![Segment { text, confidence: confidence.clamp(0.0, 1.0) }])
}

/// Minimal query escaping without a new dependency.
fn urlencoding_lite(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Stub for paid-plan gating in Rust core (not webview-only). Always true until billing wires it.
pub fn deepgram_allowed_by_plan(_plan_allows_meeting_asr: bool) -> bool {
    // ponytail: gating lives here later; meetings already ship on all plans (FR-MT).
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::traceability::RecordingSink;

    #[test]
    fn rejects_mip_opt_in() {
        let cfg = DeepgramConfig { mip_opt_out: false, ..DeepgramConfig::default() };
        let err = match Deepgram::new(cfg, Box::new(DebugEnvKeyAuth), Arc::new(RecordingSink::new())) {
            Ok(_) => panic!("expected mip_opt_out rejection"),
            Err(e) => e,
        };
        assert!(err.contains("mip_opt_out"));
    }

    #[test]
    fn the_mint_url_must_be_https() {
        // A bearer credential goes out on this request; plain HTTP would publish it.
        // Not `unwrap_err()`: the struct deliberately has no Debug impl, because it caches the
        // minted Deepgram token and a derived Debug would print it.
        let err = match EphemeralTokenAuth::new("http://mint.example/asr/token") {
            Ok(_) => panic!("plain-http mint URL should be refused"),
            Err(e) => e,
        };
        assert!(err.contains("https"), "{err}");
        assert!(EphemeralTokenAuth::new("https://mint.example/asr/token").is_ok());
    }

    #[test]
    fn the_mint_refuses_to_send_without_an_entitled_licence() {
        // The endpoint spends the company Deepgram key. No licence → no request at all, rather
        // than an anonymous one that a leaked URL could replay.
        let mut auth =
            EphemeralTokenAuth::with_license_source("https://mint.example/asr/token", || None)
                .unwrap();
        let err = auth.authorization_header().unwrap_err();
        assert!(err.contains("licence"), "{err}");
    }

    #[test]
    fn linear16_packs_little_endian() {
        let bytes = f32_to_linear16(&[0.0, 1.0, -1.0]);
        assert_eq!(bytes.len(), 6);
        assert_eq!(&bytes[0..2], &0i16.to_le_bytes());
        assert_eq!(&bytes[2..4], &32767i16.to_le_bytes());
        assert_eq!(&bytes[4..6], &(-32767i16).to_le_bytes());
    }

    #[test]
    fn empty_pcm_yields_no_segments_without_network() {
        let mut d = Deepgram::new(
            DeepgramConfig::default(),
            Box::new(DebugEnvKeyAuth),
            Arc::new(RecordingSink::new()),
        )
        .expect("build");
        assert!(d.transcribe(&[]).is_empty());
    }

    #[test]
    fn duration_digest_has_no_waveform_bytes() {
        let meta = "duration_ms=250";
        let d = digest(meta);
        assert_eq!(d.len(), 16);
        assert!(!d.is_empty());
    }

    #[test]
    fn meeting_listen_url_has_smart_format_not_dictation() {
        let d = Deepgram::new(DeepgramConfig::default(), Box::new(DebugEnvKeyAuth), Arc::new(RecordingSink::new()))
            .expect("build");
        let url = d.listen_url();
        assert!(url.contains("mip_opt_out=true"));
        assert!(url.contains("smart_format=true"));
        assert!(!url.contains("dictation=true"));
        assert!(!url.contains("paragraphs="));
        assert!(!url.contains("diarize="));
        assert!(!url.contains("sentiment="));
    }

    #[test]
    fn voice_listen_url_adds_dictation() {
        let d = Deepgram::new(
            DeepgramConfig::default().with_purpose("voice_dictation"),
            Box::new(DebugEnvKeyAuth),
            Arc::new(RecordingSink::new()),
        )
        .expect("build");
        let url = d.listen_url();
        assert!(url.contains("smart_format=true"));
        assert!(url.contains("dictation=true"));
        assert!(url.contains("mip_opt_out=true"));
    }

    #[test]
    fn meeting_live_url_has_interim_and_endpointing() {
        let cfg = DeepgramConfig::default();
        let url = live_listen_url(&cfg, LiveMode::Meeting);
        assert!(url.starts_with("wss://"));
        assert!(url.contains("mip_opt_out=true"));
        assert!(url.contains("smart_format=true"));
        assert!(url.contains("interim_results=true"));
        assert!(url.contains("endpointing=300"));
        assert!(!url.contains("dictation=true"));
        assert!(url.contains("encoding=linear16"));
        assert!(url.contains(&format!("sample_rate={SAMPLE_RATE}")));
    }

    #[test]
    fn voice_live_url_has_dictation_no_interim() {
        let cfg = DeepgramConfig::default().with_purpose("voice_dictation");
        let url = live_listen_url(&cfg, LiveMode::Voice);
        assert!(url.starts_with("wss://"));
        assert!(url.contains("dictation=true"));
        assert!(url.contains("interim_results=false"));
        assert!(url.contains("endpointing=false"));
        assert!(!url.contains("endpointing=300"));
        assert!(url.contains("mip_opt_out=true"));
        assert!(url.contains("smart_format=true"));
    }

    #[test]
    fn cleartext_listen_endpoints_are_refused_except_localhost() {
        // Meeting audio + the Authorization header ride this endpoint; a plain-http override
        // (misconfig, hostile env) would put both on the wire in the clear.
        for bad in ["http://mock.example/v1/listen", "ws://mock.example/v1/listen"] {
            let cfg = DeepgramConfig { listen_endpoint: bad.into(), ..DeepgramConfig::default() };
            let err = match Deepgram::new(cfg, Box::new(DebugEnvKeyAuth), Arc::new(RecordingSink::new())) {
                Ok(_) => panic!("cleartext endpoint should be refused: {bad}"),
                Err(e) => e,
            };
            assert!(err.contains("https"), "{err}");
        }
        // Local mock servers (tests, dev) never leave the machine and stay usable.
        for ok in [
            "http://localhost:8080/v1/listen",
            "http://127.0.0.1:9999/v1/listen",
            "ws://[::1]:9999/v1/listen",
            "https://api.deepgram.com/v1/listen",
            "wss://api.deepgram.com/v1/listen",
        ] {
            let cfg = DeepgramConfig { listen_endpoint: ok.into(), ..DeepgramConfig::default() };
            assert!(
                Deepgram::new(cfg, Box::new(DebugEnvKeyAuth), Arc::new(RecordingSink::new())).is_ok(),
                "{ok} should be accepted"
            );
        }
    }

    #[test]
    fn live_connect_refuses_cleartext_remote_endpoints() {
        // Checked before auth is even consulted — the header must never be minted for a
        // connection that would leak it.
        let cfg = DeepgramConfig {
            listen_endpoint: "http://mock.example/v1/listen".into(),
            ..DeepgramConfig::default()
        };
        let mut auth = DebugEnvKeyAuth;
        let err = match DeepgramLive::connect(&cfg, &mut auth, LiveMode::Meeting, Arc::new(RecordingSink::new())) {
            Ok(_) => panic!("cleartext live endpoint should be refused"),
            Err(e) => e,
        };
        assert!(err.contains("https"), "{err}");
    }

    #[test]
    fn a_stalled_live_queue_drops_new_audio_at_the_30s_wall() {
        // A live handle wired to channels nobody drains — the "stalled WS thread" case. The
        // queue must stop at the Ring-style 30-second wall instead of growing ~32 KB/s forever.
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (_result_tx, result_rx) = mpsc::channel();
        let mut live = DeepgramLive {
            cmd_tx,
            result_rx,
            join: None,
            pcm_bytes: 0,
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            dropped_bytes: 0,
            purpose: "meeting_asr".into(),
            trace: Arc::new(RecordingSink::new()),
            traced: true, // synthetic session: nothing streamed, no egress record on drop
        };
        // 31 one-second chunks against a 30-second cap: the 31st hits the wall and is dropped.
        let second = vec![0.0f32; SAMPLE_RATE as usize];
        for _ in 0..31 {
            live.push_pcm(&second).expect("push");
        }
        assert_eq!(live.queued_bytes.load(Ordering::Acquire), LIVE_QUEUE_MAX_BYTES);
        assert_eq!(
            live.pcm_bytes, LIVE_QUEUE_MAX_BYTES,
            "dropped audio never counts as egressed"
        );
        assert!(live.dropped_bytes > 0, "the overflow chunk was counted, not queued");
        // The WS thread consuming a chunk makes room, and recovery resets the burst counter.
        match cmd_rx.try_recv() {
            Ok(LiveCmd::Audio(bytes)) => {
                live.queued_bytes.fetch_sub(bytes.len(), Ordering::AcqRel);
            }
            _ => panic!("a queued audio chunk was expected"),
        }
        live.push_pcm(&second).expect("push after drain");
        assert_eq!(live.dropped_bytes, 0, "recovery ends the drop burst");
    }

    #[test]
    fn https_to_wss_rewrites_scheme() {
        assert_eq!(
            https_to_wss("https://api.deepgram.com/v1/listen"),
            "wss://api.deepgram.com/v1/listen"
        );
        assert_eq!(
            https_to_wss("wss://api.deepgram.com/v1/listen"),
            "wss://api.deepgram.com/v1/listen"
        );
    }

    #[test]
    fn parse_live_results_interim_and_final() {
        let interim = r#"{"type":"Results","is_final":false,"speech_final":false,"channel":{"alternatives":[{"transcript":"hel","confidence":0.5}]}}"#;
        let r = parse_live_message(interim).expect("interim");
        assert_eq!(r.text, "hel");
        assert!(!r.is_final);
        assert!(!r.speech_final);

        let fin = r#"{"type":"Results","is_final":true,"speech_final":true,"channel":{"alternatives":[{"transcript":"hello","confidence":0.98}]}}"#;
        let r = parse_live_message(fin).expect("final");
        assert_eq!(r.text, "hello");
        assert!(r.is_final);
        assert!(r.speech_final);
        assert!((r.confidence - 0.98).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_live_skips_metadata_and_empty() {
        assert!(parse_live_message(r#"{"type":"Metadata"}"#).is_none());
        assert!(parse_live_message(
            r#"{"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"  "}]}}"#
        )
        .is_none());
    }
}
