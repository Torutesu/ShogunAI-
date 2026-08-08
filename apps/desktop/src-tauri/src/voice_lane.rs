//! Hold-to-talk audio lane: mic open only while the shortcut is held, ring buffer in RAM,
//! Nova-3 (Deepgram) on release when auth is configured; otherwise on-device Whisper fallback.
//! No disk, no DB for audio — ephemeral transcripts for voice dictation (#44).
//!
//! Whisper (~487MB) must never load on the NSEvent / AppKit thread. Deepgram auth is warmed on a
//! background preload thread when voice is enabled; Whisper loads only when Deepgram auth is absent.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;
use shogun_core::audio::asr::deepgram::{self, Deepgram, DeepgramConfig};
use shogun_core::audio::asr::Transcriber;
use shogun_core::audio::capture::mic::Mic;
use shogun_core::audio::capture::AudioSource;
use shogun_core::audio::ring::Ring;
use shogun_core::daemon::Db;
use tauri::{AppHandle, Emitter, Manager};

/// Outcome of a single hold-to-talk capture.
pub enum TranscriptOutcome {
    Ok(String),
    Empty,
    Err(String),
}

pub struct Handle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<TranscriptOutcome>>,
}

#[derive(Serialize, Clone)]
struct LevelEvent {
    rms: f32,
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

struct WhisperSlot {
    path: std::path::PathBuf,
    asr: shogun_core::audio::asr::whisper::Whisper,
}

enum AsrCache {
    Deepgram(Deepgram),
    Whisper(WhisperSlot),
}

static ASR: Mutex<Option<AsrCache>> = Mutex::new(None);

fn model_missing_hint() -> String {
    "Speech model missing (~465MB ggml-small). Run scripts/fetch-whisper-model.sh, or set SHOGUN_WHISPER_MODEL to a ggml-small.bin path, or add a Deepgram key in Settings.".into()
}

fn whisper_model_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    if let Ok(m) = std::env::var("SHOGUN_WHISPER_MODEL") {
        let p = std::path::PathBuf::from(m);
        if p.exists() {
            return Some(p);
        }
        eprintln!(
            "[voice] SHOGUN_WHISPER_MODEL set but missing on disk: {}",
            p.display()
        );
    }
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for p in [
        base.join("../../../models/whisper/ggml-small.bin"),
        base.join("../../../models/whisper-small.gguf"),
    ] {
        if p.exists() {
            return Some(p);
        }
    }
    let p = app
        .path()
        .resource_dir()
        .ok()?
        .join("models/whisper-small.gguf");
    p.exists().then_some(p)
}

fn trace_sink(app: &AppHandle) -> Option<Arc<dyn shogun_core::llm::traceability::TraceabilitySink>> {
    app.try_state::<Db>()
        .map(|s| Arc::new(s.inner().clone().traceability_sink()) as Arc<dyn shogun_core::llm::traceability::TraceabilitySink>)
}

fn build_deepgram(app: &AppHandle) -> Result<Deepgram, String> {
    let auth = deepgram::resolve_auth()?;
    let cfg = DeepgramConfig::default().with_purpose("voice_dictation");
    let trace = trace_sink(app);
    Deepgram::new(cfg, auth, trace)
}

fn deepgram_configured() -> bool {
    deepgram::resolve_auth().is_ok()
}

/// Warm ASR off the hot path (hold-to-talk must not load 500MB on an NSEvent thread).
pub fn preload_asr(app: &AppHandle) -> Result<(), String> {
    if deepgram_configured() {
        let d = build_deepgram(app)?;
        let mut guard = ASR
            .lock()
            .map_err(|_| "asr cache lock poisoned".to_string())?;
        *guard = Some(AsrCache::Deepgram(d));
        eprintln!("[voice] ASR backend=deepgram (nova-3, multi)");
        return Ok(());
    }
    eprintln!("[voice] no Deepgram auth — will use whisper fallback when you dictate");
    preload_whisper(app)
}

/// Back-compat name for callers that only warmed Whisper.
pub fn preload_whisper(app: &AppHandle) -> Result<(), String> {
    load_whisper(app)
}

fn load_whisper(app: &AppHandle) -> Result<(), String> {
    let Some(path) = whisper_model_path(app) else {
        return Err(model_missing_hint());
    };
    let mut guard = ASR
        .lock()
        .map_err(|_| "asr cache lock poisoned".to_string())?;
    if let Some(AsrCache::Whisper(slot)) = guard.as_ref() {
        if slot.path == path {
            return Ok(());
        }
    }
    *guard = None;
    let path_str = path.to_string_lossy().to_string();
    let loaded = catch_unwind(AssertUnwindSafe(|| {
        shogun_core::audio::asr::whisper::Whisper::load_with_language(&path_str, None)
    }));
    let asr = match loaded {
        Ok(Ok(w)) => w,
        Ok(Err(e)) => return Err(format!("whisper load failed: {e}")),
        Err(_) => {
            return Err(
                "Speech model crashed while loading. Try again, or keep SHOGUN_WHISPER_GPU unset (CPU)."
                    .into(),
            )
        }
    };
    *guard = Some(AsrCache::Whisper(WhisperSlot { path, asr }));
    eprintln!("[voice] ASR backend=whisper (offline fallback)");
    Ok(())
}

fn ensure_deepgram(app: &AppHandle) -> Result<(), String> {
    let mut guard = ASR
        .lock()
        .map_err(|_| "asr cache lock poisoned".to_string())?;
    if matches!(guard.as_ref(), Some(AsrCache::Deepgram(_))) {
        return Ok(());
    }
    let d = build_deepgram(app)?;
    *guard = Some(AsrCache::Deepgram(d));
    Ok(())
}

fn transcribe_pcm_whisper(pcm: &[f32]) -> Result<String, String> {
    let mut guard = ASR
        .lock()
        .map_err(|_| "asr cache lock poisoned".to_string())?;
    let slot = match guard.as_mut() {
        Some(AsrCache::Whisper(w)) => w,
        _ => {
            return Err(
                "Speech model not ready yet — wait a moment after enabling Voice, then try again."
                    .to_string(),
            )
        }
    };
    let segments = catch_unwind(AssertUnwindSafe(|| slot.asr.transcribe(pcm)));
    let segments = match segments {
        Ok(s) => s,
        Err(_) => {
            return Err("Speech recognition crashed on this clip. Try a shorter hold.".into())
        }
    };
    join_segments(&segments)
}

fn transcribe_pcm_deepgram(pcm: &[f32]) -> Result<String, String> {
    let mut guard = ASR
        .lock()
        .map_err(|_| "asr cache lock poisoned".to_string())?;
    let d = match guard.as_mut() {
        Some(AsrCache::Deepgram(d)) => d,
        _ => return Err("Deepgram client not ready — try again in a moment.".into()),
    };
    d.transcribe_utterance(pcm)
}

fn join_segments(segments: &[shogun_core::audio::Segment]) -> Result<String, String> {
    let text: String = segments
        .iter()
        .map(|s| s.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Ok(text)
}

fn transcribe_clip(app: &AppHandle, pcm: &[f32]) -> TranscriptOutcome {
    if deepgram_configured() {
        if let Err(e) = ensure_deepgram(app) {
            return TranscriptOutcome::Err(e);
        }
        match transcribe_pcm_deepgram(pcm) {
            Ok(t) if t.trim().is_empty() => TranscriptOutcome::Empty,
            Ok(t) => TranscriptOutcome::Ok(t),
            Err(e) => TranscriptOutcome::Err(e),
        }
    } else {
        if let Err(e) = load_whisper(app) {
            return TranscriptOutcome::Err(e);
        }
        match transcribe_pcm_whisper(pcm) {
            Ok(t) if t.trim().is_empty() => TranscriptOutcome::Empty,
            Ok(t) => TranscriptOutcome::Ok(t),
            Err(e) => TranscriptOutcome::Err(e),
        }
    }
}

/// Open the mic and start filling a RAM ring buffer. Emits `voice_level` while active.
///
/// Does **not** load ASR models here — preload / release keeps hold-start fast.
pub fn start(app: &AppHandle) -> Result<Handle, String> {
    let mic = Mic::open().map_err(|e| format!("microphone unavailable: {e}"))?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let app_handle = app.clone();

    let join = std::thread::spawn(move || {
        let mut mic = mic;
        let mut ring = Ring::new();
        while !stop_flag.load(Ordering::Relaxed) {
            if let Some(frame) = mic.try_recv() {
                let level = rms(&frame.samples);
                let _ = app_handle.emit("voice_level", LevelEvent { rms: level });
                ring.push(&frame.samples);
            } else {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        mic.stop();
        let pcm = ring.drain();
        if pcm.is_empty() {
            return TranscriptOutcome::Empty;
        }
        transcribe_clip(&app_handle, &pcm)
    });

    Ok(Handle {
        stop,
        join: Some(join),
    })
}

/// Signal the lane to stop, wait for ASR, return the transcript outcome.
pub fn stop(mut handle: Handle) -> TranscriptOutcome {
    handle.stop.store(true, Ordering::Relaxed);
    match handle.join.take().and_then(|j| j.join().ok()) {
        Some(out) => out,
        None => TranscriptOutcome::Err("audio lane thread failed".into()),
    }
}
