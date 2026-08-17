//! Voice hold-to-talk session: overlay, settings, mic lifecycle, dictation output (#44).
//!
//! On release: Deepgram Nova-3 (when configured) or Whisper fallback → inject into focused field (AX), else clipboard → idle.
//! Chat response is deferred; this path is dictation-first per product ask.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use serde::Serialize;
    use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

    use crate::inline_source::mac::{self as inline_source, DictationTarget};
    use crate::voice_lane::{self, TranscriptOutcome};
    use shogun_core::daemon::Db;
    use shogun_core::llm::openai_compat::{
        OpenAiCompatAgentClient, OpenAiCompatConfig, GROQ_BASE_URL,
    };
    use shogun_core::llm::transport::ReqwestTransport;
    use shogun_core::llm::{ByokKey, Secret};
    use shogun_integrations::keychain_store;

    const WINDOW_LABEL: &str = "voice";
    const VOICE_EDIT_KEY_ACCOUNT: &str = "voice-edit-groq-byok";
    const LEGACY_GROQ_KEY_ACCOUNT: &str = "groq-byok";
    const VOICE_EDIT_MODEL: &str = "openai/gpt-oss-120b";
    /// Dictation must stay responsive even if the optional formatter is slow or unavailable.
    const VOICE_EDIT_TIMEOUT: Duration = Duration::from_secs(2);

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

    #[derive(Clone, serde::Serialize)]
    pub struct VoiceEditSettingsView {
        pub model: &'static str,
        pub has_key: bool,
    }

    fn voice_edit_key() -> Option<String> {
        if let Ok(bytes) = keychain_store::get_generic_secret(VOICE_EDIT_KEY_ACCOUNT) {
            if let Ok(key) = String::from_utf8(bytes) {
                return Some(key);
            }
        }
        let legacy = keychain_store::get_generic_secret(LEGACY_GROQ_KEY_ACCOUNT)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .filter(|key| !key.trim().is_empty());
        if let Some(ref key) = legacy {
            let _ = keychain_store::set_generic_secret(VOICE_EDIT_KEY_ACCOUNT, key.as_bytes());
        }
        legacy
    }

    /// Dictation is a latency-sensitive background path. Never let a Keychain authorization
    /// dialog block transcript delivery; settings remains the explicit interactive unlock path.
    fn voice_edit_key_non_interactive() -> Option<String> {
        keychain_store::get_generic_secret_non_interactive(VOICE_EDIT_KEY_ACCOUNT)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty())
    }

    #[tauri::command]
    pub fn get_voice_edit_settings() -> VoiceEditSettingsView {
        VoiceEditSettingsView {
            model: VOICE_EDIT_MODEL,
            has_key: voice_edit_key().is_some(),
        }
    }

    #[tauri::command]
    pub fn set_voice_edit_key(key: String) -> Result<(), String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("key is empty".into());
        }
        keychain_store::set_generic_secret(VOICE_EDIT_KEY_ACCOUNT, key.as_bytes())
            .map_err(|e| e.to_string())
    }

    #[tauri::command]
    pub fn clear_voice_edit_key() -> Result<(), String> {
        keychain_store::delete_generic_secret(VOICE_EDIT_KEY_ACCOUNT).map_err(|e| e.to_string())
    }

    fn valid_edit(raw: &str, edited: &str, protected_terms: &[String]) -> bool {
        let edited = edited.trim();
        !edited.is_empty()
            && edited.len() <= raw.len().saturating_mul(2).saturating_add(256)
            && preserves_source_words(raw, edited)
            && protected_terms
                .iter()
                .all(|term| raw.match_indices(term).count() == edited.match_indices(term).count())
    }

    fn normalized_words(text: &str) -> std::collections::HashMap<String, usize> {
        let mut words = std::collections::HashMap::new();
        for word in text.split_whitespace() {
            let normalized: String = word
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '\'')
                .flat_map(char::to_lowercase)
                .collect();
            if !normalized.is_empty() {
                *words.entry(normalized).or_insert(0) += 1;
            }
        }
        words
    }

    /// The formatter may add punctuation/capitalization, but it cannot silently drop spoken
    /// words. This is deliberately stricter than the prompt because model compliance is not a
    /// safety boundary.
    fn preserves_source_words(raw: &str, edited: &str) -> bool {
        let raw_words = normalized_words(raw);
        let edited_words = normalized_words(edited);
        raw_words
            .iter()
            .all(|(word, count)| edited_words.get(word).copied().unwrap_or(0) >= *count)
    }

    fn block_on_timeout<F>(
        runtime: &tokio::runtime::Runtime,
        duration: Duration,
        future: F,
    ) -> Result<F::Output, tokio::time::error::Elapsed>
    where
        F: std::future::Future,
    {
        runtime.block_on(async move { tokio::time::timeout(duration, future).await })
    }

    fn edit_dictation(
        transcript: &str,
        protected_terms: &[String],
        db: &shogun_core::daemon::Db,
    ) -> Option<String> {
        let Some(key) = voice_edit_key_non_interactive() else {
            return None;
        };
        let protected = if protected_terms.is_empty() {
            "(none)".to_string()
        } else {
            protected_terms.join("\n")
        };
        let prompt = format!(
            "Format this dictated transcript conservatively.\n\n\
             Do not delete, skip, summarize, reorder, or merge any spoken words.\n\
             Do not remove fillers or repeated words in this pass.\n\
             Only fix punctuation, capitalization, and an obvious spelling/ASR error when the intended replacement is certain.\n\
             Preserve technical terms, names, commands, paths, URLs, and the speaker's wording exactly.\n\
             If uncertain, keep the original words unchanged.\n\
             Do not add facts or explanations.\n\
             Return only the cleaned transcript.\n\n\
             Protected terms:\n<protected>\n{protected}\n</protected>\n\n\
             Transcript:\n<transcript>\n{transcript}\n</transcript>"
        );
        let Ok(transport) = ReqwestTransport::new() else {
            return None;
        };
        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return None;
        };
        let client = OpenAiCompatAgentClient::new(
            transport,
            db.traceability_sink(),
            ByokKey::new(Secret::new(key)),
            OpenAiCompatConfig::new(GROQ_BASE_URL, VOICE_EDIT_MODEL)
                .with_max_tokens(512)
                .with_reasoning_effort("low")
                .with_include_reasoning(false),
        );
        // Constructing `tokio::time::timeout` requires an entered Tokio reactor. The ASR worker is
        // a plain OS thread, so the timeout future must be created inside `block_on`.
        match block_on_timeout(&rt, VOICE_EDIT_TIMEOUT, client.complete(&prompt)) {
            Ok(Ok(edited)) if valid_edit(transcript, &edited, protected_terms) => {
                Some(edited.trim().to_string())
            }
            _ => None,
        }
    }

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
    fn deliver_dictation(
        app: &AppHandle,
        transcript: &str,
        target: Option<DictationTarget>,
        session: u64,
    ) {
        if !is_current_session(session) {
            return;
        }
        if let Some(target) = target {
            match inline_source::insert_dictation(&target, transcript) {
                Ok(()) => {
                    eprintln!("[voice] dictation inserted at captured caret");
                    if is_current_session(session) {
                        emit_state(app, "idle", Some(transcript.to_string()), None);
                    }
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
                if is_current_session(session) {
                    emit_state(app, "idle", Some(transcript.to_string()), None);
                }
            }
            Err(ce) => finish_with_error(app, session, format!("Could not paste or copy: {ce}")),
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

    /// The user has just turned Voice on in Settings. Prompt now, while the product is visibly
    /// asking for permission, instead of surprising them during the first hold-to-talk gesture.
    fn request_microphone_access_bg() {
        std::thread::spawn(|| match voice_lane::request_microphone_access() {
            Ok(()) => eprintln!("[voice] microphone access ready"),
            Err(e) => eprintln!("[voice] microphone access unavailable: {e}"),
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
        let pending_audio = Arc::new(Mutex::new(Some(audio)));
        let worker_audio = Arc::clone(&pending_audio);
        let worker_app = app.clone();
        let worker = std::thread::Builder::new()
            .name("voice-asr".into())
            .spawn(move || {
                let audio = match worker_audio.lock() {
                    Ok(mut audio) => audio.take(),
                    Err(_) => None,
                };
                let Some(audio) = audio else {
                    finish_with_error(
                        &worker_app,
                        session,
                        "Voice processing could not start. Try again.",
                    );
                    return;
                };
                let worker_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let transcript = match voice_lane::stop(audio) {
                        TranscriptOutcome::Ok(t) => t,
                        TranscriptOutcome::Empty => {
                            finish_with_error(
                                &worker_app,
                                session,
                                "Didn't catch that — try again.",
                            );
                            return;
                        }
                        TranscriptOutcome::Err(e) => {
                            finish_with_error(&worker_app, session, e);
                            return;
                        }
                    };

                    if !is_current_session(session) {
                        eprintln!("[voice] discard stale transcript (newer hold started)");
                        return;
                    }

                    let correction =
                        shogun_core::voice_dictionary::VoiceDictionary::with_defaults().correct(
                            &transcript,
                            &shogun_core::voice_dictionary::DictionaryContext::default(),
                        );
                    // Original ASR text is fallback for every optional edit failure. Local
                    // vocabulary correction is only sent as edit input; it cannot replace failed
                    // dictation.
                    let transcript = worker_app
                        .try_state::<Db>()
                        .and_then(|state| {
                            edit_dictation(
                                &correction.text,
                                &correction.protected_terms,
                                state.inner(),
                            )
                        })
                        .unwrap_or(transcript);
                    if !is_current_session(session) {
                        eprintln!(
                            "[voice] discard stale formatted transcript (newer hold started)"
                        );
                        return;
                    }
                    emit_state(&worker_app, "processing", Some(transcript.clone()), None);
                    deliver_dictation(&worker_app, &transcript, target, session);
                    release_processing(session);
                }));
                if worker_result.is_err() {
                    eprintln!("[voice] processing worker recovered from an internal failure");
                    finish_with_error(&worker_app, session, "Voice processing failed. Try again.");
                }
            });
        if worker.is_err() {
            if let Ok(mut audio) = pending_audio.lock() {
                if let Some(audio) = audio.take() {
                    let _ = voice_lane::stop(audio);
                }
            }
            finish_with_error(
                &app,
                session,
                "Voice processing could not start. Try again.",
            );
        }
    }

    fn should_apply_terminal_action(session: u64, current_session: u64) -> bool {
        session == current_session
    }

    fn should_release_processing(session: u64, current_session: u64, processing: bool) -> bool {
        processing && should_apply_terminal_action(session, current_session)
    }

    fn is_current_session(session: u64) -> bool {
        should_apply_terminal_action(session, SESSION.load(AtomicOrdering::SeqCst))
    }

    /// Report a terminal worker result only while it belongs to the active dictation session.
    fn finish_with_error(app: &AppHandle, session: u64, message: impl Into<String>) {
        if !is_current_session(session) {
            return;
        }
        emit_error(app, message);
        release_processing(session);
    }

    /// A cancelled or superseded worker must never unlock a newer dictation session.
    fn release_processing(session: u64) {
        if let Ok(mut lane) = LANE.lock() {
            if let Some(lane) = lane.as_mut() {
                // Read the generation while serializing lane mutation. A stale worker must not
                // observe an old generation, wait for a newer session, then clear its lock.
                let current_session = SESSION.load(AtomicOrdering::SeqCst);
                if should_release_processing(session, current_session, lane.processing) {
                    lane.processing = false;
                }
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
            request_microphone_access_bg();
        }
        eprintln!("[voice] enabled={enabled}");
        Ok(())
    }

    #[tauri::command]
    pub fn voice_dismiss(app: AppHandle) {
        // Invalidate any in-flight formatter before reopening Voice. It can still finish in the
        // background, but its transcript cannot be inserted or alter notch state.
        let _ = SESSION.fetch_add(1, AtomicOrdering::SeqCst);
        if let Ok(mut lane) = LANE.lock() {
            if let Some(lane) = lane.as_mut() {
                lane.processing = false;
            }
        }
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn edit_validation_rejects_dropped_spoken_words() {
            assert!(!valid_edit("deploy the api now", "Deploy the API.", &[]));
        }

        #[test]
        fn edit_validation_rejects_changed_protected_term() {
            let protected = ["ShogunAI".to_string()];
            assert!(!valid_edit(
                "open ShogunAI settings",
                "Open Shogun settings.",
                &protected
            ));
        }

        #[test]
        fn stale_worker_cannot_apply_terminal_action() {
            assert!(!should_apply_terminal_action(7, 8));
            assert!(should_apply_terminal_action(8, 8));
        }

        #[test]
        fn processing_release_requires_active_session_and_active_lock() {
            assert!(!should_release_processing(7, 8, true));
            assert!(!should_release_processing(8, 8, false));
            assert!(should_release_processing(8, 8, true));
        }

        #[test]
        fn formatter_timeout_enters_owned_tokio_runtime() {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime");

            let result = block_on_timeout(&runtime, Duration::from_millis(10), async { 7 });

            assert_eq!(result.ok(), Some(7));
        }
    }
}
