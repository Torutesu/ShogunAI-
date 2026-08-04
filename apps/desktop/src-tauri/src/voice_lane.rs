//! Hold-to-talk audio lane: mic open only while the shortcut is held, ring buffer in RAM, on-device
//! Whisper on release. No disk, no DB — ephemeral transcripts for voice dialogue (#44).
//!
//! Whisper (~487MB) must never load on the NSEvent / AppKit thread. Load happens on a background
//! preload thread (when voice is enabled) or on the capture thread at release — never in `start`.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;
use shogun_core::audio::asr::Transcriber;
use shogun_core::audio::capture::mic::Mic;
use shogun_core::audio::capture::AudioSource;
use shogun_core::audio::ring::Ring;
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

static WHISPER: Mutex<Option<WhisperSlot>> = Mutex::new(None);

fn model_missing_hint() -> String {
    "Speech model missing (~465MB ggml-small). Run scripts/fetch-whisper-model.sh, or set SHOGUN_WHISPER_MODEL to a ggml-small.bin path.".into()
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

/// Warm the whisper model off the hot path (hold-to-talk must not load 500MB on an NSEvent thread).
pub fn preload_whisper(app: &AppHandle) -> Result<(), String> {
    load_whisper(app)
}

fn load_whisper(app: &AppHandle) -> Result<(), String> {
    let Some(path) = whisper_model_path(app) else {
        return Err(model_missing_hint());
    };
    let mut guard = WHISPER
        .lock()
        .map_err(|_| "whisper cache lock poisoned".to_string())?;
    if guard.as_ref().is_some_and(|s| s.path == path) {
        return Ok(());
    }
    // Drop prior model before loading a new path so we do not hold two ~487MB copies.
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
    *guard = Some(WhisperSlot { path, asr });
    Ok(())
}

fn transcribe_pcm(pcm: &[f32]) -> Result<String, String> {
    let mut guard = WHISPER
        .lock()
        .map_err(|_| "whisper cache lock poisoned".to_string())?;
    let slot = guard.as_mut().ok_or_else(|| {
        "Speech model not ready yet — wait a moment after enabling Voice, then try again.".to_string()
    })?;
    let segments = catch_unwind(AssertUnwindSafe(|| slot.asr.transcribe(pcm)));
    let segments = match segments {
        Ok(s) => s,
        Err(_) => {
            return Err(
                "Speech recognition crashed on this clip. Try a shorter hold.".into(),
            )
        }
    };
    let text: String = segments
        .iter()
        .map(|s| s.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() {
        Ok(String::new())
    } else {
        Ok(text)
    }
}

/// Open the mic and start filling a RAM ring buffer. Emits `voice_level` while active.
///
/// Does **not** load Whisper here — that belongs on preload / release so hold-start stays fast and
/// never blocks the voice-hold worker (or AppKit) on a 487MB model load.
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
        if let Err(e) = load_whisper(&app_handle) {
            return TranscriptOutcome::Err(e);
        }
        match transcribe_pcm(&pcm) {
            Ok(t) if t.trim().is_empty() => TranscriptOutcome::Empty,
            Ok(t) => TranscriptOutcome::Ok(t),
            Err(e) => TranscriptOutcome::Err(e),
        }
    });

    Ok(Handle {
        stop,
        join: Some(join),
    })
}

/// Signal the lane to stop, wait for Whisper, return the transcript outcome.
pub fn stop(mut handle: Handle) -> TranscriptOutcome {
    handle.stop.store(true, Ordering::Relaxed);
    match handle.join.take().and_then(|j| j.join().ok()) {
        Some(out) => out,
        None => TranscriptOutcome::Err("audio lane thread failed".into()),
    }
}
