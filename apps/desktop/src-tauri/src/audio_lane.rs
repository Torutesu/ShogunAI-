//! The MT3 audio lane: the desktop side of capture → ASR → transcript (FR-MT-13).
//!
//! The pipeline itself is pure logic in `shogun_core::audio` and is tested there. This file does
//! only the parts that cannot be pure: it opens the real macOS backends (mic via cpal, system tap
//! via Core Audio), owns the polling thread, and drops finished lines into the meeting DB.
//!
//! **Default ASR = Deepgram Nova-3 live WebSocket** (company-paid, 2026-08-05). Continuous PCM
//! stream per speaker with interim overlay updates; HTTP batch + local VAD is the fallback when
//! the WS handshake fails. Whisper remains an offline/dev fallback (`asr_backend: whisper` or
//! `SHOGUN_ASR_BACKEND=whisper`).
//!
//! **Degradation is the rule, not the exception.** Missing auth/model, a denied microphone, or a
//! macOS without the system tap must never take the meeting down: the interval and the user's notes
//! still record. Every failure here logs `[meeting] … ; notes only` and returns `None`.
//!
//! Waveform lives only in RAM and is dropped after transcription — this file writes text, never
//! audio. Deepgram egress is process-only (`mip_opt_out=true`).

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use shogun_core::audio::asr::deepgram::{
    self, Deepgram, DeepgramConfig, DeepgramLive, LiveMode, LiveResult, LIVE_CHUNK_SAMPLES,
};
use shogun_core::audio::asr::whisper::Whisper;
use shogun_core::audio::asr::Transcriber;
use shogun_core::audio::capture::{AudioSource, MultiSource};
use shogun_core::audio::worker::{SegmentSink, Worker};
use shogun_core::audio::{Speaker, Utterance};
use shogun_core::daemon::Db;
use shogun_core::meeting::settings::{AsrBackend, AsrModel, MeetingLanguage, MeetingMode, Settings};
use tauri::{Emitter, Manager};

/// A running audio lane. Dropping the handle without `stop` would leak the thread, so the machine
/// always takes it back through `stop` on `Effect::StopAudio`.
pub struct Handle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    last_audio_at: Arc<AtomicI64>,
}

impl Handle {
    /// Epoch ms when audio frames were last consumed — feeds the silence watchdog (FR-MT-11).
    pub fn last_audio_at(&self) -> i64 {
        self.last_audio_at.load(Ordering::Relaxed)
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Serialize, Clone)]
struct LiveLineEvent {
    ts: i64,
    speaker: Option<String>,
    text: String,
    translation: Option<String>,
    /// When true, overlay updates the in-progress line for this speaker instead of appending.
    #[serde(default)]
    interim: bool,
}

/// Persists finished transcript lines against one meeting interval. Owns a cloned `Db` (Arc-backed,
/// so the clone shares the same connection) and the interval it is writing to.
struct DbSink {
    db: Db,
    session_id: i64,
    app: tauri::AppHandle,
    settings: Arc<RwLock<Settings>>,
    /// When true, JA→EN is Select KK (Deepgram has no whisper translate). When false, whisper may
    /// own that lane in-process.
    select_kk_owns_en: bool,
}

impl DbSink {
    fn emit_line(
        &mut self,
        speaker: Speaker,
        ts: i64,
        text: &str,
        confidence: f64,
        translation: Option<&str>,
        interim: bool,
    ) {
        let speaker_mem = match speaker {
            Speaker::Me => shogun_memory::transcript_segments::Speaker::Me,
            Speaker::Other => shogun_memory::transcript_segments::Speaker::Other,
        };
        if !interim {
            self.db
                .append_transcript(self.session_id, ts, speaker_mem, text, confidence);
        }

        if !crate::meeting::mac::live_emit_allowed(self.session_id) {
            return;
        }
        let speaker_str = match speaker {
            Speaker::Me => Some("me".to_string()),
            Speaker::Other => Some("other".to_string()),
        };
        let event = LiveLineEvent {
            ts,
            speaker: speaker_str.clone(),
            text: text.to_string(),
            translation: translation.map(str::to_string),
            interim,
        };
        let _ = self.app.emit("meeting_live_line", event);

        if interim || translation.is_some() {
            return;
        }
        // Async Select KK fill-in when the ASR backend did not already supply a translation.
        if let Ok(s) = self.settings.read() {
            let speaker_me = speaker == Speaker::Me;
            let target = s.translation_target(speaker_me);
            let whisper_owns_en = !self.select_kk_owns_en
                && s.meeting_mode == MeetingMode::TwoWay
                && !speaker_me
                && s.my_lang == MeetingLanguage::English;
            let needs_kk = match target {
                Some(MeetingLanguage::Japanese) => !whisper_owns_en,
                Some(MeetingLanguage::English) => self.select_kk_owns_en,
                _ => false,
            };
            if needs_kk && crate::meeting_translate::should_translate_asr(text) {
                if let Some(lang) = target {
                    crate::meeting_translate::spawn_translation(
                        &self.app,
                        self.db.clone(),
                        self.session_id,
                        ts,
                        speaker_str,
                        text.to_string(),
                        lang,
                    );
                }
            }
        }
    }
}

impl SegmentSink for DbSink {
    fn emit(
        &mut self,
        u: &Utterance,
        text: &str,
        confidence: f64,
        translation: Option<&str>,
    ) {
        self.emit_line(u.speaker, u.started_at, text, confidence, translation, false);
    }
}

/// Env override wins over settings (`whisper` / `deepgram`).
fn resolve_asr_backend(settings: &Settings) -> AsrBackend {
    if let Ok(v) = std::env::var("SHOGUN_ASR_BACKEND") {
        let v = v.trim().to_ascii_lowercase();
        if v == "whisper" {
            return AsrBackend::Whisper;
        }
        if v == "deepgram" {
            return AsrBackend::Deepgram;
        }
    }
    settings.asr_backend
}

/// Where the bundled whisper model lives (fallback path only).
fn whisper_model_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    if let Ok(m) = std::env::var("SHOGUN_WHISPER_MODEL") {
        let p = std::path::PathBuf::from(m);
        if p.exists() {
            return Some(p);
        }
        eprintln!(
            "[meeting] SHOGUN_WHISPER_MODEL set but missing on disk: {}",
            p.display()
        );
    }
    for p in dev_whisper_candidates() {
        if p.exists() {
            eprintln!("[meeting] whisper model (dev checkout): {}", p.display());
            return Some(p);
        }
    }
    let p = app.path().resource_dir().ok()?.join("models/whisper-small.gguf");
    p.exists().then_some(p)
}

fn dev_whisper_candidates() -> [std::path::PathBuf; 2] {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    [
        base.join("../../../models/whisper/ggml-small.bin"),
        base.join("../../../models/whisper-small.gguf"),
    ]
}

fn select_model_path(app: &tauri::AppHandle, model: AsrModel) -> Option<std::path::PathBuf> {
    if model == AsrModel::Turbo {
        if let Some(turbo) = crate::model_fetch::ensure_turbo(app) {
            return Some(turbo);
        }
        eprintln!("[meeting] turbo model unavailable; using bundled small");
    }
    whisper_model_path(app)
}

enum MeetingAsr {
    Deepgram(Deepgram),
    Whisper(Whisper),
}

impl Transcriber for MeetingAsr {
    fn transcribe(&mut self, pcm: &[f32]) -> Vec<shogun_core::audio::Segment> {
        match self {
            MeetingAsr::Deepgram(d) => d.transcribe(pcm),
            MeetingAsr::Whisper(w) => w.transcribe(pcm),
        }
    }

    fn translate_to_english(&mut self, pcm: &[f32]) -> Vec<shogun_core::audio::Segment> {
        match self {
            MeetingAsr::Deepgram(d) => d.translate_to_english(pcm),
            MeetingAsr::Whisper(w) => w.translate_to_english(pcm),
        }
    }
}

fn meeting_config(language: MeetingLanguage) -> DeepgramConfig {
    DeepgramConfig::default().with_language(language.deepgram_code())
}

fn build_deepgram(db: &Db, language: MeetingLanguage) -> Result<Deepgram, String> {
    if !deepgram::deepgram_allowed_by_plan(true) {
        return Err("Deepgram ASR not allowed by plan".into());
    }
    let auth = deepgram::resolve_auth()?;
    let cfg = meeting_config(language);
    let trace = Arc::new(db.traceability_sink());
    Deepgram::new(cfg, auth, trace)
}

fn try_open_live_pair(
    db: &Db,
    language: MeetingLanguage,
    dual: bool,
) -> Result<(DeepgramLive, Option<DeepgramLive>), String> {
    if !deepgram::deepgram_allowed_by_plan(true) {
        return Err("Deepgram ASR not allowed by plan".into());
    }
    let cfg = meeting_config(language);
    let trace: Arc<dyn shogun_core::llm::traceability::TraceabilitySink> =
        Arc::new(db.traceability_sink());
    let mut auth = deepgram::resolve_auth()?;
    let me = DeepgramLive::connect(&cfg, auth.as_mut(), LiveMode::Meeting, trace.clone())?;
    let other = if dual {
        // Fresh auth header (cached inside the auth impl).
        let mut auth2 = deepgram::resolve_auth()?;
        Some(DeepgramLive::connect(&cfg, auth2.as_mut(), LiveMode::Meeting, trace)?)
    } else {
        None
    };
    Ok((me, other))
}

fn build_whisper(app: &tauri::AppHandle, model: AsrModel, language: MeetingLanguage) -> Option<Whisper> {
    let Some(model_path) = select_model_path(app, model) else {
        eprintln!("[meeting] no whisper model bundled; notes only");
        return None;
    };
    match Whisper::load_with_language(&model_path.to_string_lossy(), language.whisper_code()) {
        Ok(w) => Some(w),
        Err(e) => {
            eprintln!("[meeting] whisper model present but failed to load ({e}); notes only");
            None
        }
    }
}

struct StreamBuf {
    pending: Vec<f32>,
    /// Wall-clock ms when the current interim/final utterance started.
    utterance_ts: i64,
    line_seq: u32,
}

impl StreamBuf {
    fn new() -> Self {
        Self {
            pending: Vec::with_capacity(LIVE_CHUNK_SAMPLES),
            utterance_ts: 0,
            line_seq: 0,
        }
    }

    fn next_ts(&mut self, now: i64) -> i64 {
        if self.utterance_ts == 0 {
            self.utterance_ts = now + self.line_seq as i64;
            self.line_seq += 1;
        }
        self.utterance_ts
    }

    fn clear_utterance(&mut self) {
        self.utterance_ts = 0;
    }
}

fn push_stream_pcm(
    live: &mut DeepgramLive,
    buf: &mut StreamBuf,
    samples: &[f32],
) -> Result<(), String> {
    buf.pending.extend_from_slice(samples);
    while buf.pending.len() >= LIVE_CHUNK_SAMPLES {
        let chunk: Vec<f32> = buf.pending.drain(..LIVE_CHUNK_SAMPLES).collect();
        live.push_pcm(&chunk)?;
    }
    Ok(())
}

fn handle_live_results(
    speaker: Speaker,
    live: &DeepgramLive,
    buf: &mut StreamBuf,
    now: i64,
    sink: &mut DbSink,
) {
    for r in live.drain() {
        apply_live_result(speaker, &r, buf, now, sink);
    }
}

fn apply_live_result(
    speaker: Speaker,
    r: &LiveResult,
    buf: &mut StreamBuf,
    now: i64,
    sink: &mut DbSink,
) {
    let text = r.text.trim();
    if text.is_empty() {
        return;
    }
    let ts = buf.next_ts(now);
    if r.is_final {
        sink.emit_line(speaker, ts, text, r.confidence, None, false);
        buf.clear_utterance();
    } else {
        sink.emit_line(speaker, ts, text, r.confidence, None, true);
    }
}

fn run_live_loop(
    mut source: MultiSource,
    mut me: DeepgramLive,
    mut other: Option<DeepgramLive>,
    mut sink: DbSink,
    stop: Arc<AtomicBool>,
    last_audio_at: Arc<AtomicI64>,
) {
    let mut me_buf = StreamBuf::new();
    let mut other_buf = StreamBuf::new();

    while !stop.load(Ordering::Relaxed) {
        let now = now_ms();
        let mut got = false;
        while let Some(frame) = source.try_recv() {
            got = true;
            match frame.speaker {
                Speaker::Me => {
                    if let Err(e) = push_stream_pcm(&mut me, &mut me_buf, &frame.samples) {
                        eprintln!("[meeting] live Me push failed: {e}");
                    }
                }
                Speaker::Other => {
                    if let Some(ref mut live) = other {
                        if let Err(e) = push_stream_pcm(live, &mut other_buf, &frame.samples) {
                            eprintln!("[meeting] live Other push failed: {e}");
                        }
                    }
                }
            }
        }
        if got {
            last_audio_at.store(now, Ordering::Relaxed);
        } else {
            std::thread::sleep(Duration::from_millis(20));
        }

        handle_live_results(Speaker::Me, &me, &mut me_buf, now, &mut sink);
        if let Some(ref live) = other {
            handle_live_results(Speaker::Other, live, &mut other_buf, now, &mut sink);
        }
    }

    source.stop();
    // Flush trailing PCM then close streams for final transcripts.
    if !me_buf.pending.is_empty() {
        let _ = me.push_pcm(&me_buf.pending);
        me_buf.pending.clear();
    }
    if let Some(ref mut live) = other {
        if !other_buf.pending.is_empty() {
            let _ = live.push_pcm(&other_buf.pending);
            other_buf.pending.clear();
        }
    }

    let now = now_ms();
    match me.finish_finals() {
        Ok(finals) => {
            for t in finals {
                let ts = me_buf.next_ts(now);
                sink.emit_line(Speaker::Me, ts, &t, 0.9, None, false);
                me_buf.clear_utterance();
            }
        }
        Err(e) => eprintln!("[meeting] live Me finish: {e}"),
    }
    if let Some(live) = other.take() {
        match live.finish_finals() {
            Ok(finals) => {
                for t in finals {
                    let ts = other_buf.next_ts(now);
                    sink.emit_line(Speaker::Other, ts, &t, 0.9, None, false);
                    other_buf.clear_utterance();
                }
            }
            Err(e) => eprintln!("[meeting] live Other finish: {e}"),
        }
    }
}

/// Start listening for meeting `session_id` with live `settings`.
/// Returns `None` (notes only) whenever any piece of the pipeline is unavailable.
pub fn start(
    app: &tauri::AppHandle,
    session_id: i64,
    settings: Arc<RwLock<Settings>>,
) -> Option<Handle> {
    let db = app.try_state::<Db>().map(|s| s.inner().clone());
    let Some(db) = db else {
        eprintln!("[meeting] no database for the audio lane; notes only");
        return None;
    };

    let (backend, model, language) = {
        let s = settings.read().ok()?;
        (resolve_asr_backend(&s), s.asr_model, s.asr_language())
    };

    let mic = match shogun_core::audio::capture::mic::Mic::open() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[meeting] microphone unavailable ({e}); notes only");
            return None;
        }
    };

    let mut sources: Vec<Box<dyn AudioSource>> = vec![Box::new(mic)];
    let mut has_tap = false;
    match shogun_core::audio::capture::system_tap::SystemTap::open() {
        Ok(Some(tap)) => {
            has_tap = true;
            sources.push(Box::new(tap));
        }
        Ok(None) => eprintln!("[meeting] system audio tap unavailable (macOS < 14.4); mic only"),
        Err(e) => eprintln!("[meeting] system audio tap failed ({e}); mic only"),
    }

    let source = MultiSource::new(sources);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let last_audio_at = Arc::new(AtomicI64::new(now_ms()));
    let last_audio_flag = last_audio_at.clone();

    match backend {
        AsrBackend::Deepgram => {
            let sink = DbSink {
                db: db.clone(),
                session_id,
                app: app.clone(),
                settings: settings.clone(),
                select_kk_owns_en: true,
            };
            match try_open_live_pair(&db, language, has_tap) {
                Ok((me, other)) => {
                    eprintln!("[meeting] ASR backend=deepgram live WS");
                    let join = std::thread::spawn(move || {
                        run_live_loop(source, me, other, sink, stop_flag, last_audio_flag);
                    });
                    eprintln!("[meeting] audio lane started for session {session_id}");
                    Some(Handle {
                        stop,
                        join: Some(join),
                        last_audio_at,
                    })
                }
                Err(e) => {
                    eprintln!(
                        "[meeting] deepgram live WS unavailable ({e}); HTTP batch + VAD fallback"
                    );
                    let d = match build_deepgram(&db, language) {
                        Ok(d) => d,
                        Err(e2) => {
                            eprintln!("[meeting] deepgram unavailable ({e2}); notes only");
                            return None;
                        }
                    };
                    let asr = MeetingAsr::Deepgram(d);
                    let worker = Worker::new(source, asr).with_live_settings(settings.clone());
                    let mut sink = sink;
                    let join = std::thread::spawn(move || {
                        run_worker_loop(worker, &mut sink, stop_flag, last_audio_flag);
                    });
                    eprintln!("[meeting] audio lane started for session {session_id} (HTTP fallback)");
                    Some(Handle {
                        stop,
                        join: Some(join),
                        last_audio_at,
                    })
                }
            }
        }
        AsrBackend::Whisper => {
            let w = build_whisper(app, model, language)?;
            eprintln!("[meeting] ASR backend=whisper (offline/dev fallback)");
            let asr = MeetingAsr::Whisper(w);
            let worker = Worker::new(source, asr).with_live_settings(settings.clone());
            let mut sink = DbSink {
                db,
                session_id,
                app: app.clone(),
                settings,
                select_kk_owns_en: false,
            };
            let join = std::thread::spawn(move || {
                run_worker_loop(worker, &mut sink, stop_flag, last_audio_flag);
            });
            eprintln!("[meeting] audio lane started for session {session_id}");
            Some(Handle {
                stop,
                join: Some(join),
                last_audio_at,
            })
        }
    }
}

fn run_worker_loop<S: AudioSource, T: Transcriber>(
    mut worker: Worker<S, T>,
    sink: &mut DbSink,
    stop: Arc<AtomicBool>,
    last_audio_at: Arc<AtomicI64>,
) {
    while !stop.load(Ordering::Relaxed) {
        let now = now_ms();
        if worker.poll(now, sink) == 0 {
            std::thread::sleep(Duration::from_millis(20));
        } else {
            last_audio_at.store(now, Ordering::Relaxed);
        }
    }
    worker.stop(now_ms(), sink);
}

/// Stop the lane: signal the thread and wait for it to flush and release the devices. A missing
/// handle (audio never started, or already stopped) is a no-op.
pub fn stop(handle: Option<Handle>) {
    let Some(mut handle) = handle else { return };
    handle.stop.store(true, Ordering::Relaxed);
    if let Some(join) = handle.join.take() {
        let _ = join.join();
    }
    eprintln!("[meeting] audio lane stopped");
}
