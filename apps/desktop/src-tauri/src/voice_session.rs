//! Voice hold-to-talk session: overlay, settings, mic lifecycle, dictation output (#44).
//!
//! On release: Deepgram Nova-3 (when configured) or Whisper fallback → inject into focused field (AX), else clipboard → idle.
//! Chat response is deferred; this path is dictation-first per product ask.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::sync::Mutex;

    use serde::Serialize;
    use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

    use crate::inline_source::mac::{self as inline_source, DictationTarget};
    use crate::voice_lane::{self, TranscriptOutcome};

    const WINDOW_LABEL: &str = "voice";

    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    pub struct Settings {
        #[serde(default)]
        pub enabled: bool,
    }

    impl Default for Settings {
        fn default() -> Self {
            Self { enabled: false }
        }
    }

    struct Lane {
        settings: Settings,
        audio: Option<voice_lane::Handle>,
        /// True between successful hold-start and hold-end — used to idle-out a stuck UI if
        /// release arrives with no audio handle (should be rare after the ordered worker).
        ui_recording: bool,
        /// True while the released clip is being transcribed and delivered. A second hold is
        /// ignored until this clears so sessions cannot race or surface a misleading ASR error.
        processing: bool,
        /// Caret captured before recording starts. Dictation inserts here without rewriting
        /// surrounding text, even if focus changes while ASR processes.
        target: Option<DictationTarget>,
    }

    static LANE: Mutex<Option<Lane>> = Mutex::new(None);

    #[derive(Clone, Serialize)]
    pub struct VoiceStateEvent {
        pub phase: &'static str,
        pub transcript: Option<String>,
        pub response: Option<String>,
    }

    #[derive(Clone, Serialize)]
    pub struct VoiceErrorEvent {
        pub message: String,
    }

    /// Monotonic session id so a late ASR thread cannot clobber a newer hold.
    static SESSION: AtomicU64 = AtomicU64::new(0);

    fn settings_path(app: &AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("voice.json"))
    }

    fn load_settings(app: &AppHandle) -> Settings {
        settings_path(app)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save_settings(app: &AppHandle, settings: &Settings) {
        let Some(p) = settings_path(app) else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(settings) {
            if let Err(e) = std::fs::write(&p, json) {
                eprintln!("[voice] settings save failed: {e}");
            }
        }
    }

    fn emit_state(
        app: &AppHandle,
        phase: &'static str,
        transcript: Option<String>,
        response: Option<String>,
    ) {
        let _ = app.emit(
            "voice_state",
            VoiceStateEvent {
                phase,
                transcript,
                response,
            },
        );
    }

    fn emit_error(app: &AppHandle, message: impl Into<String>) {
        let msg = message.into();
        let _ = app.emit(
            "voice_error",
            VoiceErrorEvent {
                message: msg.clone(),
            },
        );
        emit_state(app, "error", None, Some(msg));
    }

    /// Leave transcript on the general pasteboard (no restore — user wants the text).
    fn copy_to_clipboard(text: &str) -> Result<(), String> {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        use objc2_foundation::NSString;

        let pb: *mut AnyObject = unsafe { msg_send![class!(NSPasteboard), generalPasteboard] };
        if pb.is_null() {
            return Err("no pasteboard".into());
        }
        let utf8 = NSString::from_str("public.utf8-plain-text");
        let ours = NSString::from_str(text);
        let _: isize = unsafe { msg_send![pb, clearContents] };
        let ok: bool = unsafe { msg_send![pb, setString: &*ours, forType: &*utf8] };
        if ok {
            Ok(())
        } else {
            Err("could not write the pasteboard".into())
        }
    }

    /// Dictation output: captured caret/selection, never paragraph rewrite; else clipboard.
    fn deliver_dictation(app: &AppHandle, transcript: &str, target: Option<DictationTarget>) {
        if let Some(target) = target {
            match inline_source::insert_dictation(&target, transcript) {
                Ok(()) => {
                    eprintln!("[voice] dictation inserted at captured caret");
                    emit_state(app, "idle", Some(transcript.to_string()), None);
                    return;
                }
                Err(e) => {
                    eprintln!("[voice] dictation inject failed ({e}) — clipboard");
                }
            }
        } else {
            eprintln!("[voice] no injectable field — clipboard");
        }

        match copy_to_clipboard(transcript) {
            Ok(()) => {
                emit_state(app, "idle", Some(transcript.to_string()), None);
            }
            Err(ce) => emit_error(app, format!("Could not paste or copy: {ce}")),
        }
    }

    fn preload_asr_bg(app: &AppHandle) {
        let app = app.clone();
        std::thread::spawn(move || {
            if let Err(e) = voice_lane::preload_asr(&app) {
                eprintln!("[voice] asr preload failed: {e}");
            } else {
                eprintln!("[voice] dictation ASR ready");
            }
        });
    }

    pub fn init(app: &AppHandle) {
        let settings = load_settings(app);
        let enabled_log = settings.enabled;
        let _ = build_overlay(app);
        if let Ok(mut lane) = LANE.lock() {
            *lane = Some(Lane {
                settings: settings.clone(),
                audio: None,
                ui_recording: false,
                processing: false,
                target: None,
            });
        }
        if settings.enabled {
            preload_asr_bg(app);
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

    /// Begin hold-to-talk capture. Returns `true` when the mic lane is live (UI shows recording).
    pub fn on_hold_start(app: AppHandle) -> bool {
        let enabled = LANE
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|l| l.settings.enabled))
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
        // Do not hold LANE across mic open — keep the lock short so settings IPC cannot stall.
        {
            let mut lane = match LANE.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            let Some(lane) = lane.as_mut() else {
                return false;
            };
            if lane.processing {
                eprintln!("[voice] hold ignored — previous dictation still processing");
                return false;
            }
            if lane.audio.is_some() {
                // Already live — treat as success so the release path still runs.
                lane.ui_recording = true;
                return true;
            }
        }

        let target = inline_source::capture_dictation_target();
        let handle = match voice_lane::start(&app) {
            Ok(h) => h,
            Err(e) => {
                emit_error(&app, e);
                return false;
            }
        };

        let mut lane = match LANE.lock() {
            Ok(g) => g,
            Err(_) => {
                // Lane gone — stop the mic we just opened.
                let _ = voice_lane::stop(handle);
                return false;
            }
        };
        let Some(lane) = lane.as_mut() else {
            let _ = voice_lane::stop(handle);
            return false;
        };
        if lane.audio.is_some() {
            // Race: another start won. Drop this handle.
            let _ = voice_lane::stop(handle);
            lane.ui_recording = true;
            return true;
        }
        lane.audio = Some(handle);
        lane.target = target;
        lane.ui_recording = true;
        let _ = SESSION.fetch_add(1, AtomicOrdering::SeqCst);
        emit_state(&app, "recording", None, None);
        eprintln!("[voice] hold start — mic open");
        true
    }

    /// True when a hold is still live (mic handle or UI recording flag). Used by the release failsafe.
    pub fn is_ui_recording() -> bool {
        LANE.lock()
            .ok()
            .and_then(|g| g.as_ref().map(|l| l.ui_recording || l.audio.is_some()))
            .unwrap_or(false)
    }

    /// If still recording 500ms after a release signal, force `on_hold_end` again. Returns true when
    /// it had to act (stuck path).
    pub fn force_end_if_recording(app: AppHandle) -> bool {
        if !is_ui_recording() {
            return false;
        }
        eprintln!("[voice] force_end_if_recording — ending stuck hold");
        on_hold_end(app);
        true
    }

    /// End hold: stop mic → Deepgram or Whisper → dictation inject/clipboard → idle.
    ///
    /// ASR runs on a dedicated thread so the voice-hold worker is never blocked.
    pub fn on_hold_end(app: AppHandle) {
        let (audio, target) = {
            let mut lane = match LANE.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(lane) = lane.as_mut() else {
                return;
            };
            let was_recording = lane.ui_recording || lane.audio.is_some();
            lane.ui_recording = false;
            match lane.audio.take() {
                Some(handle) => {
                    lane.processing = true;
                    (handle, lane.target.take())
                }
                None => {
                    lane.target = None;
                    if was_recording {
                        emit_state(&app, "idle", None, None);
                    }
                    return;
                }
            }
        };

        // Signal release to the frontend before ASR so a stuck recording chrome can failsafe.
        let _ = app.emit("voice_hold_released", ());
        // Leave recording immediately so the notch meter cannot stick while ASR runs.
        emit_state(&app, "processing", None, None);
        eprintln!("[voice] hold end — transcribing (dictation)");

        let session = SESSION.load(AtomicOrdering::SeqCst);
        std::thread::Builder::new()
            .name("voice-asr".into())
            .spawn(move || {
                let transcript = match voice_lane::stop(audio) {
                    TranscriptOutcome::Ok(t) => t,
                    TranscriptOutcome::Empty => {
                        emit_error(&app, "Didn't catch that — try again.");
                        set_processing(false);
                        return;
                    }
                    TranscriptOutcome::Err(e) => {
                        emit_error(&app, e);
                        set_processing(false);
                        return;
                    }
                };

                if SESSION.load(AtomicOrdering::SeqCst) != session {
                    set_processing(false);
                    eprintln!("[voice] discard stale transcript (newer hold started)");
                    return;
                }

                emit_state(&app, "processing", Some(transcript.clone()), None);
                // Dictation-first: no chat call on this path.
                deliver_dictation(&app, &transcript, target);
                set_processing(false);
            })
            .ok();
    }

    fn set_processing(processing: bool) {
        if let Ok(mut lane) = LANE.lock() {
            if let Some(lane) = lane.as_mut() {
                lane.processing = processing;
            }
        }
    }

    #[tauri::command]
    pub fn get_voice_settings() -> Settings {
        LANE.lock()
            .ok()
            .and_then(|g| g.as_ref().map(|l| l.settings.clone()))
            .unwrap_or_default()
    }

    #[tauri::command]
    pub fn set_voice_enabled(enabled: bool, app: AppHandle) -> Result<(), String> {
        let mut lane = LANE
            .lock()
            .map_err(|_| "voice lane lock poisoned".to_string())?;
        let settings = lane.as_mut().ok_or("voice not initialized")?;
        settings.settings.enabled = enabled;
        save_settings(&app, &settings.settings);
        if enabled {
            preload_asr_bg(&app);
        }
        eprintln!("[voice] enabled={enabled}");
        Ok(())
    }

    #[tauri::command]
    pub fn voice_dismiss(app: AppHandle) {
        emit_state(&app, "idle", None, None);
    }

    /// Frontend failsafe: force-end a hold that stayed in recording after release.
    #[tauri::command]
    pub fn voice_force_end(app: AppHandle) {
        let _ = force_end_if_recording(app);
    }

    fn build_overlay(app: &AppHandle) -> Option<WebviewWindow> {
        if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
            return Some(win);
        }
        let win = tauri::WebviewWindowBuilder::new(app, WINDOW_LABEL, tauri::WebviewUrl::default())
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
            .map_err(|e| eprintln!("[voice] overlay build failed: {e}"))
            .ok()?;
        configure_overlay(&win);
        Some(win)
    }

    fn configure_overlay(win: &WebviewWindow) {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use std::sync::atomic::Ordering;

        let ptr = match win.ns_window() {
            Ok(p) if !p.is_null() => p as *mut AnyObject,
            _ => return,
        };
        let behavior = crate::PANEL_BEHAVIOR.load(Ordering::Relaxed);
        let level = crate::OVERLAY_LEVEL;
        // SAFETY: live NSWindow on main thread (setup).
        unsafe {
            let _: () = msg_send![ptr, setCollectionBehavior: behavior];
            let _: () = msg_send![ptr, setLevel: level];
            let _: () = msg_send![ptr, setHidesOnDeactivate: false];
            let _: () = msg_send![ptr, setCanHide: true];
            let _: () = msg_send![ptr, setMovableByWindowBackground: false];
            let _: () = msg_send![ptr, setIgnoresMouseEvents: false];
        }
    }
}
