//! Voice hold-to-talk session: overlay, settings, mic lifecycle, dictation output (#44).
//!
//! On release: Deepgram Nova-3 (when configured) or Whisper fallback → inject into focused field (AX), else clipboard → idle.
//! Chat response is deferred; this path is dictation-first per product ask.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    use serde::Serialize;
    use shogun_core::inline::TextInserter;
    use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

    use crate::inline_source::mac::AxTextInserter;
    use crate::voice_lane::{self, TranscriptOutcome};

    const WINDOW_LABEL: &str = "voice";

    #[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
    pub struct Settings {
        #[serde(default)]
        pub enabled: bool,
    }

    struct Lane {
        settings: Settings,
        audio: Option<voice_lane::Handle>,
        /// True between successful hold-start and hold-end — used to idle-out a stuck UI if
        /// release arrives with no audio handle (should be rare after the ordered worker).
        ui_recording: bool,
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

    #[derive(Clone, Serialize)]
    pub struct VoiceToastEvent {
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

    fn emit_state(app: &AppHandle, phase: &'static str, transcript: Option<String>, response: Option<String>) {
        let _ = app.emit(
            "voice_state",
            VoiceStateEvent { phase, transcript, response },
        );
    }

    fn emit_error(app: &AppHandle, message: impl Into<String>) {
        let msg = message.into();
        let _ = app.emit("voice_error", VoiceErrorEvent { message: msg.clone() });
        emit_state(app, "error", None, Some(msg));
        // Push-to-talk failing quietly is the worst outcome: the user held a key, said something,
        // and nothing happened (#49, push-to-talk design §5).
        crate::sound::mac::play(shogun_core::sound::Cue::VoiceFailed);
    }

    fn emit_toast(app: &AppHandle, message: impl Into<String>) {
        let message = message.into();
        let _ = app.emit("voice_toast", VoiceToastEvent { message });
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

    /// Dictation output: focused field via AX (+ ⌘V fallback inside AxTextInserter), else clipboard.
    fn deliver_dictation(app: &AppHandle, transcript: &str) {
        use crate::inline_source::mac::AxCursorReader;
        use shogun_core::inline::CursorReader;

        // Only inject when an editable text-carrying field is focused (same signal as inline draft).
        let has_field = AxCursorReader.read().is_some();
        if has_field {
            match AxTextInserter.insert(transcript) {
                Ok(()) => {
                    eprintln!("[voice] dictation pasted into focused field");
                    emit_toast(app, "Pasted");
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
                emit_toast(app, "Copied to clipboard");
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
            });
        }
        if settings.enabled {
            preload_asr_bg(app);
        }
        eprintln!(
            "[voice] dialogue {}",
            if enabled_log { "enabled" } else { "off (beta default)" }
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
            if lane.audio.is_some() {
                // Already live — treat as success so the release path still runs.
                lane.ui_recording = true;
                return true;
            }
        }

        // BEFORE the mic opens, deliberately (#49 §5). Our own capture cannot pick up a cue that
        // has already played, and meeting recording blocks this path entirely — so the only thing
        // left that could hear it is another app's live call, which the hot-mic rule catches.
        crate::sound::mac::play(shogun_core::sound::Cue::VoiceStart);

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
        let audio = {
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
                Some(handle) => handle,
                None => {
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
                // Cue after `stop`, so our own mic is already closed and cannot hear its own end
                // cue — and only on success: a failure plays its own sound from `emit_error`, and
                // two cues back to back would say less than either one alone (#49).
                let transcript = match voice_lane::stop(audio) {
                    TranscriptOutcome::Ok(t) => {
                        crate::sound::mac::play(shogun_core::sound::Cue::VoiceEnd);
                        t
                    }
                    TranscriptOutcome::Empty => {
                        emit_error(&app, "Didn't catch that — try again.");
                        return;
                    }
                    TranscriptOutcome::Err(e) => {
                        emit_error(&app, e);
                        return;
                    }
                };

                if SESSION.load(AtomicOrdering::SeqCst) != session {
                    eprintln!("[voice] discard stale transcript (newer hold started)");
                    return;
                }

                emit_state(&app, "processing", Some(transcript.clone()), None);
                // Dictation-first: no chat call on this path.
                deliver_dictation(&app, &transcript);
            })
            .ok();
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
        let mut lane = LANE.lock().map_err(|_| "voice lane lock poisoned".to_string())?;
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
