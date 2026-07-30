//! The MT3 audio lane: the desktop side of capture → VAD → ASR → transcript (FR-MT-13).
//!
//! The pipeline itself is pure logic in `shogun_core::audio` and is tested there. This file does
//! only the parts that cannot be pure: it opens the real macOS backends (mic via cpal, system tap
//! via Core Audio), owns the polling thread, and drops finished lines into the meeting DB.
//!
//! **Degradation is the rule, not the exception.** A missing whisper model, a denied microphone,
//! or a macOS without the system tap must never take the meeting down: the interval and the user's
//! notes still record. Every failure here logs `[meeting] … ; notes only` and returns `None`, and
//! the caller (`meeting.rs`) simply carries no audio handle.
//!
//! Invariant 2 holds by construction: the waveform lives only in the `Worker`'s buffers and is
//! dropped the moment a line is transcribed — this file writes text, never audio.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use shogun_core::audio::capture::{AudioSource, MultiSource};
use shogun_core::audio::worker::{SegmentSink, Worker};
use shogun_core::audio::{Speaker, Utterance};
use shogun_core::daemon::Db;
use shogun_core::meeting::settings::{AsrModel, MeetingLanguage};
use tauri::Manager;

/// A running audio lane. Dropping the handle without `stop` would leak the thread, so the machine
/// always takes it back through `stop` on `Effect::StopAudio`.
pub struct Handle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Persists finished transcript lines against one meeting interval. Owns a cloned `Db` (Arc-backed,
/// so the clone shares the same connection) and the interval it is writing to.
struct DbSink {
    db: Db,
    session_id: i64,
}

impl SegmentSink for DbSink {
    fn emit(&mut self, u: &Utterance, text: &str, confidence: f64) {
        // The capture source decides the speaker: mic input is me, the system tap is everyone else.
        // We never infer, so there is no `Unknown` on this path.
        let speaker = match u.speaker {
            Speaker::Me => shogun_memory::transcript_segments::Speaker::Me,
            Speaker::Other => shogun_memory::transcript_segments::Speaker::Other,
        };
        // Best-effort: `append_transcript` swallows write failures so a hiccup drops one line
        // rather than tearing down capture.
        self.db.append_transcript(self.session_id, u.started_at, speaker, text, confidence);
    }
}

/// Where the bundled whisper model lives. Mirrors the e5 embedding model resolution
/// (`embedding_model_paths` in lib.rs): a dev checkout points `SHOGUN_WHISPER_MODEL` at whatever
/// `scripts/fetch-whisper-model.sh` downloaded; a packaged app finds it in the resource dir.
/// Absence degrades to notes-only rather than erroring.
pub(crate) fn whisper_model_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    if let Ok(m) = std::env::var("SHOGUN_WHISPER_MODEL") {
        let p = std::path::PathBuf::from(m);
        return p.exists().then_some(p);
    }
    let p = app.path().resource_dir().ok()?.join("models/whisper-small.gguf");
    p.exists().then_some(p)
}

/// The whisper model to load for `model`, degrading toward the bundled small model. For `Turbo` we
/// try the fetched-once large-v3-turbo weights first (`model_fetch::ensure_turbo`), and fall back
/// to small whenever the fetch is unavailable (offline, hash mismatch) so a Turbo preference never
/// prevents transcription — it only asks for higher accuracy when it can be had.
pub(crate) fn select_model_path(app: &tauri::AppHandle, model: AsrModel) -> Option<std::path::PathBuf> {
    if model == AsrModel::Turbo {
        if let Some(turbo) = crate::model_fetch::ensure_turbo(app) {
            return Some(turbo);
        }
        eprintln!("[meeting] turbo model unavailable; using bundled small");
    }
    whisper_model_path(app)
}

/// Start listening for meeting `session_id` with the chosen ASR `model` and `language`. Returns
/// `None` (notes only) whenever any piece of the pipeline is unavailable — this is the degraded,
/// not the error, path (FR-MT-13, OPEN-07/08).
///
/// `language` fixes the transcription language for the whole session (English-primary policy, §8):
/// English/Japanese pin whisper; Auto detects once and locks (see whisper.rs). It is threaded in as
/// its own param, alongside `model`, so the meeting machine's settings decide it.
pub fn start(
    app: &tauri::AppHandle,
    session_id: i64,
    model: AsrModel,
    language: MeetingLanguage,
) -> Option<Handle> {
    // The database the sink writes into. Absent DB means nowhere to store the transcript, so there
    // is no point listening.
    let db = app.try_state::<Db>().map(|s| s.inner().clone());
    let Some(db) = db else {
        eprintln!("[meeting] no database for the audio lane; notes only");
        return None;
    };

    // The ASR model. Absent in a dev checkout without the fetch script; a real load failure is the
    // same outcome here — notes only — but is worth logging distinctly. A Turbo preference is
    // honoured when the fetched model is available, otherwise this resolves the bundled small model.
    let Some(model_path) = select_model_path(app, model) else {
        eprintln!("[meeting] no whisper model bundled; notes only");
        return None;
    };
    // The language is fixed for the session here: `whisper_code()` gives whisper `Some("en")`/
    // `Some("ja")` for a chosen language, or `None` for Auto (detect-once-then-lock inside whisper).
    let asr = match shogun_core::audio::asr::whisper::Whisper::load_with_language(
        &model_path.to_string_lossy(),
        language.whisper_code(),
    ) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[meeting] whisper model present but failed to load ({e}); notes only");
            return None;
        }
    };

    // The microphone (speaker = me). Denied permission or no input device → notes only.
    let mic = match shogun_core::audio::capture::mic::Mic::open() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[meeting] microphone unavailable ({e}); notes only");
            return None;
        }
    };

    // The system tap (speaker = other) is best-effort: `Ok(None)` on macOS < 14.4, and any error
    // is treated the same — mic-only capture rather than no capture. So a call whose participants'
    // audio we cannot tap still records the user's own side.
    let mut sources: Vec<Box<dyn AudioSource>> = vec![Box::new(mic)];
    match shogun_core::audio::capture::system_tap::SystemTap::open() {
        Ok(Some(tap)) => sources.push(Box::new(tap)),
        Ok(None) => eprintln!("[meeting] system audio tap unavailable (macOS < 14.4); mic only"),
        Err(e) => eprintln!("[meeting] system audio tap failed ({e}); mic only"),
    }

    let source = MultiSource::new(sources);
    let mut worker = Worker::new(source, asr);
    let mut sink = DbSink { db, session_id };

    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let join = std::thread::spawn(move || {
        // Poll-and-park: drain everything available, and only sleep when there was nothing, so a
        // busy meeting is transcribed promptly while an idle one costs almost no CPU.
        while !stop_flag.load(Ordering::Relaxed) {
            if worker.poll(now_ms(), &mut sink) == 0 {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        // Flush the final utterance on each speaker and release the devices before the buffers go.
        worker.stop(now_ms(), &mut sink);
    });

    eprintln!("[meeting] audio lane started for session {session_id}");
    Some(Handle { stop, join: Some(join) })
}

/// Stop the lane: signal the thread and wait for it to flush and release the devices. A missing
/// handle (audio never started, or already stopped) is a no-op.
pub fn stop(handle: Option<Handle>) {
    let Some(mut handle) = handle else { return };
    handle.stop.store(true, Ordering::Relaxed);
    if let Some(join) = handle.join.take() {
        // Ignore a poisoned/panicked capture thread: we are tearing down anyway, and a panic there
        // must not propagate into the meeting machine.
        let _ = join.join();
    }
    eprintln!("[meeting] audio lane stopped");
}
