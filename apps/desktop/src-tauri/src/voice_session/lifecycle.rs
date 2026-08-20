//! Hold-to-talk session ownership, cancellation, overlay, and terminal transitions.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use super::asr;
use super::consent::{load_settings, Settings};
use super::dictionary;
use super::editor;
use super::insertion::{
    cancel_delivery_fence, capture_dictation_target, deliver_dictation, DeliveryFence,
    DeliveryOutcome, DictationTarget, DELIVERY_READY,
};
use super::{emit_error, emit_state};

const WINDOW_LABEL: &str = "voice";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SessionPhase {
    Opening,
    Recording,
    Processing,
    Finishing,
}

pub(super) struct ActiveSession {
    id: u64,
    phase: SessionPhase,
    audio: Option<crate::voice_lane::Handle>,
    target: Option<Arc<DictationTarget>>,
    delivery: Arc<DeliveryFence>,
}

pub(super) struct Lane {
    pub(super) settings: Settings,
    active: Option<ActiveSession>,
}

pub(super) static LANE: Mutex<Option<Lane>> = Mutex::new(None);
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

struct SessionCleanup {
    session: u64,
}

impl Drop for SessionCleanup {
    fn drop(&mut self) {
        abandon_session(self.session);
    }
}

pub(super) fn session_is_processing(session: u64) -> bool {
    let Ok(lane) = LANE.lock() else {
        return false;
    };
    lane.as_ref()
        .and_then(|lane| lane.active.as_ref())
        .is_some_and(|active| active.id == session && active.phase == SessionPhase::Processing)
}

fn claim_terminal(session: u64, expected: SessionPhase) -> bool {
    let Ok(mut lane) = LANE.lock() else {
        return false;
    };
    let Some(lane) = lane.as_mut() else {
        return false;
    };
    if lane
        .active
        .as_ref()
        .is_some_and(|active| active.id == session && active.phase == expected)
    {
        if let Some(active) = lane.active.as_mut() {
            active.phase = SessionPhase::Finishing;
        }
        true
    } else {
        false
    }
}

fn clear_terminal(session: u64) {
    let Ok(mut lane) = LANE.lock() else {
        return;
    };
    let Some(lane) = lane.as_mut() else {
        return;
    };
    if lane
        .active
        .as_ref()
        .is_some_and(|active| active.id == session && active.phase == SessionPhase::Finishing)
    {
        lane.active = None;
    }
}

fn abandon_session(session: u64) {
    let Ok(mut lane) = LANE.lock() else {
        return;
    };
    let Some(lane) = lane.as_mut() else {
        return;
    };
    if lane
        .active
        .as_ref()
        .is_some_and(|active| active.id == session)
    {
        lane.active = None;
    }
}

fn complete_terminal<F>(session: u64, expected: SessionPhase, emit: F) -> bool
where
    F: FnOnce(),
{
    if !claim_terminal(session, expected) {
        return false;
    }
    emit();
    clear_terminal(session);
    true
}

pub(super) fn cancel_active_session() -> Option<crate::voice_lane::Handle> {
    let (audio, delivery) = {
        let Ok(mut lane) = LANE.lock() else {
            return None;
        };
        let lane = lane.as_mut()?;
        let mut active = lane.active.take()?;
        (active.audio.take(), Arc::clone(&active.delivery))
    };
    cancel_delivery_fence(&delivery);
    audio
}

pub(super) fn stop_cancelled_audio(audio: crate::voice_lane::Handle) {
    let retained = Arc::new(Mutex::new(Some(audio)));
    let worker = Arc::clone(&retained);
    let spawned = std::thread::Builder::new()
        .name("voice-cancel".into())
        .spawn(move || {
            if let Some(audio) = worker.lock().ok().and_then(|mut audio| audio.take()) {
                let _ = crate::voice_lane::stop(audio);
            }
        });
    if spawned.is_err() {
        if let Some(audio) = retained.lock().ok().and_then(|mut audio| audio.take()) {
            let _ = crate::voice_lane::stop(audio);
        }
    }
}

pub fn init(app: &AppHandle) {
    let settings = load_settings(app);
    let enabled_log = settings.enabled;
    let _ = build_overlay(app);
    if let Ok(mut lane) = LANE.lock() {
        *lane = Some(Lane {
            settings: settings.clone(),
            active: None,
        });
    }
    if settings.enabled {
        asr::preload_asr_bg(app);
    }
    eprintln!(
        "[voice] dialogue {}",
        if enabled_log {
            "enabled"
        } else {
            "off (beta default)"
        }
    );
}

/// Begins hold-to-talk capture. Returns true only when mic lane is live.
pub fn on_hold_start(app: AppHandle) -> bool {
    let enabled = LANE
        .lock()
        .ok()
        .and_then(|lane| lane.as_ref().map(|lane| lane.settings.enabled))
        .unwrap_or(false);
    if !enabled {
        return false;
    }
    if crate::meeting::mac::is_recording() {
        emit_error(
            &app,
            "Voice is unavailable while meeting notes are recording.",
        );
        return false;
    }
    let existing_phase = LANE.lock().ok().and_then(|lane| {
        lane.as_ref()
            .and_then(|lane| lane.active.as_ref().map(|active| active.phase))
    });
    match existing_phase {
        Some(SessionPhase::Recording | SessionPhase::Opening) => return true,
        Some(SessionPhase::Processing | SessionPhase::Finishing) => return false,
        None => {}
    }
    // No target still records safely: transcript goes to clipboard. Selected text is never replaced.
    let target = match capture_dictation_target() {
        Ok(target) => Some(Arc::new(target)),
        Err(reason) => {
            eprintln!("[voice] no safe insertion target: {reason}");
            None
        }
    };
    let context = dictionary::dictionary_context(target.as_deref());
    let (session, allow_personal_dictionary_keyterms) = {
        let mut lane = match LANE.lock() {
            Ok(lane) => lane,
            Err(_) => return false,
        };
        let Some(lane) = lane.as_mut() else {
            return false;
        };
        match lane.active.as_ref().map(|active| active.phase) {
            Some(SessionPhase::Recording | SessionPhase::Opening) => return true,
            Some(SessionPhase::Processing | SessionPhase::Finishing) => return false,
            None => {}
        }
        let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        lane.active = Some(ActiveSession {
            id,
            phase: SessionPhase::Opening,
            audio: None,
            target,
            delivery: Arc::new(DeliveryFence {
                state: std::sync::atomic::AtomicU8::new(DELIVERY_READY),
                operation: Mutex::new(()),
            }),
        });
        (
            id,
            lane.settings.share_personal_dictionary_with_speech_provider,
        )
    };
    // Before mic opens: own capture cannot hear cue, and meeting recording blocks this path.
    crate::sound::mac::play(shogun_core::sound::Cue::VoiceStart);
    let handle = match crate::voice_lane::start(&app, context, allow_personal_dictionary_keyterms) {
        Ok(handle) => handle,
        Err(error) => {
            let _ = complete_terminal(session, SessionPhase::Opening, || emit_error(&app, error));
            return false;
        }
    };
    let mut lane = match LANE.lock() {
        Ok(lane) => lane,
        Err(_) => {
            let _ = crate::voice_lane::stop(handle);
            return false;
        }
    };
    let Some(lane) = lane.as_mut() else {
        let _ = crate::voice_lane::stop(handle);
        return false;
    };
    let Some(active) = lane.active.as_mut() else {
        let _ = crate::voice_lane::stop(handle);
        return false;
    };
    if active.id != session || active.phase != SessionPhase::Opening {
        let _ = crate::voice_lane::stop(handle);
        return false;
    }
    active.audio = Some(handle);
    active.phase = SessionPhase::Recording;
    emit_state(&app, "recording", None, None);
    eprintln!("[voice] hold start — mic open");
    true
}

pub fn is_ui_recording() -> bool {
    let Ok(lane) = LANE.lock() else {
        return false;
    };
    lane.as_ref()
        .and_then(|lane| lane.active.as_ref())
        .is_some_and(|active| active.phase == SessionPhase::Recording)
}

pub fn force_end_if_recording(app: AppHandle) -> bool {
    if !is_ui_recording() {
        return false;
    }
    eprintln!("[voice] force_end_if_recording — ending stuck hold");
    on_hold_end(app);
    true
}

/// Ends hold: stop mic, transcribe, locally correct, optionally edit, insert or copy, then idle.
pub fn on_hold_end(app: AppHandle) {
    let (session, audio, target, delivery) = {
        let mut lane = match LANE.lock() {
            Ok(lane) => lane,
            Err(_) => return,
        };
        let Some(lane) = lane.as_mut() else {
            return;
        };
        let Some(active) = lane.active.as_mut() else {
            return;
        };
        if active.phase != SessionPhase::Recording {
            return;
        }
        let Some(audio) = active.audio.take() else {
            lane.active = None;
            emit_state(&app, "idle", None, None);
            return;
        };
        active.phase = SessionPhase::Processing;
        (
            active.id,
            audio,
            active.target.clone(),
            Arc::clone(&active.delivery),
        )
    };
    let _ = app.emit("voice_hold_released", ());
    emit_state(&app, "processing", None, None);
    eprintln!("[voice] hold end — transcribing (dictation)");
    let shared_audio = Arc::new(Mutex::new(Some(audio)));
    let worker_audio = Arc::clone(&shared_audio);
    let worker_app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("voice-asr".into())
        .spawn(move || {
            let _cleanup = SessionCleanup { session };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> bool {
                let Some(audio) = worker_audio.lock().ok().and_then(|mut audio| audio.take())
                else {
                    return false;
                };
                let transcript = match crate::voice_lane::stop(audio) {
                    crate::voice_lane::TranscriptOutcome::Ok(transcript) => transcript,
                    crate::voice_lane::TranscriptOutcome::Empty => {
                        let _ = complete_terminal(session, SessionPhase::Processing, || {
                            emit_error(&worker_app, "Didn't catch that — try again.")
                        });
                        return true;
                    }
                    crate::voice_lane::TranscriptOutcome::Err(error) => {
                        let _ = complete_terminal(session, SessionPhase::Processing, || {
                            emit_error(&worker_app, format!("Voice transcription failed: {error}"))
                        });
                        return true;
                    }
                };
                if !session_is_processing(session) {
                    return true;
                }
                crate::sound::mac::play(shogun_core::sound::Cue::VoiceEnd);
                let context = dictionary::dictionary_context(target.as_deref());
                let dictionary = worker_app
                    .try_state::<shogun_core::daemon::Db>()
                    .map(|db| db.inner().voice_dictionary())
                    .unwrap_or_else(shogun_core::voice_dictionary::VoiceDictionary::with_defaults);
                let correction =
                    dictionary::dictionary_edit_candidate(&transcript, &dictionary, &context);
                let transcript = worker_app
                    .try_state::<shogun_core::daemon::Db>()
                    .and_then(|db| {
                        editor::edit_dictation(
                            &correction.text,
                            &correction.protected_terms,
                            db.inner(),
                        )
                    })
                    .unwrap_or(correction.text);
                if !session_is_processing(session) {
                    return true;
                }
                let Some(outcome) =
                    deliver_dictation(session, target.as_deref(), &delivery, &transcript)
                else {
                    return true;
                };
                match outcome {
                    DeliveryOutcome::Inserted | DeliveryOutcome::Copied => {
                        let _ = complete_terminal(session, SessionPhase::Processing, || {
                            emit_state(&worker_app, "idle", Some(transcript), None)
                        });
                    }
                    DeliveryOutcome::CopyFailed(error) => {
                        let _ = complete_terminal(session, SessionPhase::Processing, || {
                            emit_error(&worker_app, format!("Could not copy dictation: {error}"))
                        });
                    }
                }
                true
            }));
            if !matches!(result, Ok(true)) {
                let _ = complete_terminal(session, SessionPhase::Processing, || {
                    emit_error(&worker_app, "Voice transcription failed.")
                });
            }
        });
    if spawned.is_err() {
        let audio = shared_audio.lock().ok().and_then(|mut audio| audio.take());
        if let Some(audio) = audio {
            let _ = crate::voice_lane::stop(audio);
        }
        let _ = complete_terminal(session, SessionPhase::Processing, || {
            emit_error(&app, "Voice transcription could not start.")
        });
    }
}

pub(super) fn voice_dismiss(app: AppHandle) {
    if let Some(audio) = cancel_active_session() {
        stop_cancelled_audio(audio);
    }
    emit_state(&app, "idle", None, None);
}

pub(super) fn voice_force_end(app: AppHandle) {
    let _ = force_end_if_recording(app);
}

fn build_overlay(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        return Some(window);
    }
    let window = tauri::WebviewWindowBuilder::new(app, WINDOW_LABEL, tauri::WebviewUrl::default())
        .title("ShogunAI — voice")
        .transparent(true)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .shadow(false)
        .skip_taskbar(true)
        .inner_size(1.0, 1.0)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|error| eprintln!("[voice] overlay build failed: {error}"))
        .ok()?;
    configure_overlay(&window);
    Some(window)
}

fn configure_overlay(window: &WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    let pointer = match window.ns_window() {
        Ok(pointer) if !pointer.is_null() => pointer as *mut AnyObject,
        _ => return,
    };
    let behavior = crate::PANEL_BEHAVIOR.load(Ordering::Relaxed);
    let level = crate::OVERLAY_LEVEL;
    // SAFETY: live NSWindow on main thread during setup.
    unsafe {
        let _: () = msg_send![pointer, setCollectionBehavior: behavior];
        let _: () = msg_send![pointer, setLevel: level];
        let _: () = msg_send![pointer, setHidesOnDeactivate: false];
        let _: () = msg_send![pointer, setCanHide: true];
        let _: () = msg_send![pointer, setMovableByWindowBackground: false];
        let _: () = msg_send![pointer, setIgnoresMouseEvents: false];
    }
}
