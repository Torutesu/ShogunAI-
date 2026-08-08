//! Deepgram Nova-3 utterance-batch ASR (meeting default, 2026-08-05).
//!
//! **Key security:** the company Deepgram key must never ship inside the desktop binary or a shared
//! Keychain secret. Auth goes through [`DeepgramAuth`]:
//! - production: ephemeral token from SHOGUN/Select backend (backend holds the key, or mints via
//!   Deepgram `POST /v1/auth/grant`);
//! - Keychain `deepgram-asr` (user pastes once in Settings);
//! - local debug only: `SHOGUN_DEEPGRAM_API_KEY` behind `#[cfg(debug_assertions)]`.
//!
//! **ponytail:** VAD-cut utterance → HTTP `/v1/listen` (linear16 @ 16 kHz). Continuous interim WS
//! streaming is a TODO once the live overlay needs sub-utterance partials.
//!
//! Always sends `mip_opt_out=true`. Waveform never written to disk by SHOGUN.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::super::{Segment, SAMPLE_RATE};
use super::Transcriber;
use crate::llm::traceability::{digest, Route, TraceRecord, TraceabilitySink};

const DEFAULT_LISTEN: &str = "https://api.deepgram.com/v1/listen";
const DEFAULT_MODEL: &str = "nova-3";

/// How the client obtains an Authorization header value (`Token …` or `Bearer …`).
pub trait DeepgramAuth: Send {
    fn authorization_header(&mut self) -> Result<String, String>;
}

/// Production path: fetch a short-lived JWT from the SHOGUN/Select backend.
///
/// Expected JSON: `{ "access_token": "<jwt>", "expires_in": <secs> }` (Deepgram grant shape).
/// Backend holds the company key and calls Deepgram `/v1/auth/grant` (or proxies listen).
pub struct EphemeralTokenAuth {
    token_url: String,
    client: reqwest::blocking::Client,
    cached: Option<(String, Instant, Duration)>,
}

impl EphemeralTokenAuth {
    pub fn new(token_url: impl Into<String>) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| format!("deepgram auth http client: {e}"))?;
        Ok(Self { token_url: token_url.into(), client, cached: None })
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
        let resp = self
            .client
            .post(&self.token_url)
            .header("Accept", "application/json")
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
#[derive(Default)]
pub struct KeychainKeyAuth {
    cached: Option<String>,
}

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
            return Ok(format!("Token {}", key.trim()));
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
            return Ok(Box::new(EphemeralTokenAuth::new(url)?));
        }
    }
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
pub struct Deepgram {
    config: DeepgramConfig,
    auth: Box<dyn DeepgramAuth>,
    client: reqwest::blocking::Client,
    trace: Option<Arc<dyn TraceabilitySink>>,
}

impl Deepgram {
    pub fn new(
        config: DeepgramConfig,
        auth: Box<dyn DeepgramAuth>,
        trace: Option<Arc<dyn TraceabilitySink>>,
    ) -> Result<Self, String> {
        if !config.mip_opt_out {
            return Err("Deepgram mip_opt_out must be true (company policy)".into());
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| format!("deepgram http client: {e}"))?;
        Ok(Self { config, auth, client, trace })
    }

    fn listen_url(&self) -> String {
        format!(
            "{}?model={}&language={}&mip_opt_out=true&encoding=linear16&sample_rate={}&channels=1",
            self.config.listen_endpoint,
            urlencoding_lite(&self.config.model),
            urlencoding_lite(&self.config.language),
            SAMPLE_RATE
        )
    }

    fn record_egress(&self, pcm_bytes: usize, duration_ms: u64) {
        let Some(sink) = &self.trace else { return };
        // Digest duration meta only — never the waveform (invariant 2 / G8).
        let meta = format!("duration_ms={duration_ms}");
        sink.record(TraceRecord {
            route: Route::Asr,
            purpose: self.config.purpose.clone(),
            destination: "api.deepgram.com".into(),
            chunk_bytes: pcm_bytes,
            chunk_xxh64: digest(&meta),
            third_party: true,
        });
    }
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
        let resp = self
            .client
            .post(&url)
            .header("Authorization", auth)
            .header("Content-Type", "audio/raw")
            .body(linear16.clone())
            .send()
            .map_err(|e| format!("deepgram listen request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("deepgram listen HTTP {}", resp.status()));
        }
        self.record_egress(linear16.len(), duration_ms);
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
        let mut cfg = DeepgramConfig::default();
        cfg.mip_opt_out = false;
        let err = match Deepgram::new(cfg, Box::new(DebugEnvKeyAuth), None) {
            Ok(_) => panic!("expected mip_opt_out rejection"),
            Err(e) => e,
        };
        assert!(err.contains("mip_opt_out"));
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
            Some(Arc::new(RecordingSink::new())),
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
}
