//! Voice dialogue session: overlay window, settings, hold lifecycle, and context-aware chat (#44).

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::Mutex;

    use serde::Serialize;
    use shogun_core::daemon::Db;
    use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

    use crate::inline_source::mac::ChatAnswer;
    use crate::voice_lane::{self, TranscriptOutcome};

    const WINDOW_LABEL: &str = "voice";
    const RECORD_SIZE: (f64, f64) = (360.0, 88.0);
    const RESPONSE_SIZE: (f64, f64) = (480.0, 280.0);
    const MARGIN: f64 = 24.0;

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
    pub struct VoiceResponseEvent {
        pub text: String,
        pub transcript: String,
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
    }

    pub fn init(app: &AppHandle) {
        let settings = load_settings(app);
        let enabled_log = settings.enabled;
        match build_overlay(app) {
            Some(_) => eprintln!("[voice] overlay window ready (hidden)"),
            None => eprintln!("[voice] overlay window unavailable"),
        }
        if let Ok(mut lane) = LANE.lock() {
            *lane = Some(Lane { settings, audio: None });
        }
        eprintln!(
            "[voice] dialogue {}",
            if enabled_log { "enabled" } else { "off (beta default)" }
        );
    }

    pub fn on_hold_start(app: AppHandle) {
        let enabled = LANE
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|l| l.settings.enabled))
            .unwrap_or(false);
        if !enabled {
            return;
        }
        if crate::meeting::mac::is_recording() {
            emit_error(
                &app,
                "Voice is unavailable while meeting notes are recording.",
            );
            return;
        }
        let mut lane = match LANE.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let Some(lane) = lane.as_mut() else { return };
        if lane.audio.is_some() {
            return;
        }
        match voice_lane::start(&app) {
            Ok(handle) => {
                lane.audio = Some(handle);
                show_overlay(&app, RECORD_SIZE);
                emit_state(&app, "recording", None, None);
                eprintln!("[voice] hold start — mic open");
            }
            Err(e) => emit_error(&app, e),
        }
    }

    pub fn on_hold_end(app: AppHandle) {
        let audio = {
            let mut lane = match LANE.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(lane) = lane.as_mut() else { return };
            lane.audio.take()
        };
        let Some(audio) = audio else { return };

        emit_state(&app, "processing", None, None);
        resize_overlay(&app, RECORD_SIZE);
        eprintln!("[voice] hold end — transcribing");

        let transcript = match voice_lane::stop(audio) {
            TranscriptOutcome::Ok(t) => t,
            TranscriptOutcome::Empty => {
                hide_overlay(&app);
                emit_error(&app, "Didn't catch that — try again.");
                return;
            }
            TranscriptOutcome::Err(e) => {
                hide_overlay(&app);
                emit_error(&app, e);
                return;
            }
        };

        emit_state(&app, "processing", Some(transcript.clone()), None);

        let db = match app.try_state::<Db>() {
            Some(db) => db.inner().clone(),
            None => {
                hide_overlay(&app);
                emit_error(&app, "Memory isn't ready yet — try again in a moment.");
                return;
            }
        };

        let app_bg = app.clone();
        std::thread::spawn(move || {
            let answer: Result<ChatAnswer, String> =
                crate::inline_source::mac::voice_chat(&db, &transcript);
            match answer {
                Ok(a) => {
                    let _ = app_bg.emit(
                        "voice_response",
                        VoiceResponseEvent { text: a.text.clone(), transcript: transcript.clone() },
                    );
                    show_overlay(&app_bg, RESPONSE_SIZE);
                    emit_state(&app_bg, "response", Some(transcript), Some(a.text));
                }
                Err(e) => {
                    hide_overlay(&app_bg);
                    emit_error(&app_bg, e);
                }
            }
        });
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
        eprintln!("[voice] enabled={enabled}");
        Ok(())
    }

    #[tauri::command]
    pub fn voice_dismiss(app: AppHandle) {
        hide_overlay(&app);
        emit_state(&app, "idle", None, None);
    }

    fn build_overlay(app: &AppHandle) -> Option<WebviewWindow> {
        if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
            return Some(win);
        }
        let win = tauri::WebviewWindowBuilder::new(app, WINDOW_LABEL, tauri::WebviewUrl::default())
            .title("SHOGUN — voice")
            .transparent(true)
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .shadow(false)
            .skip_taskbar(true)
            .inner_size(RECORD_SIZE.0, RECORD_SIZE.1)
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

    fn park_bottom_center(win: &WebviewWindow, size: (f64, f64)) {
        let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize {
            width: size.0,
            height: size.1,
        }));
        let monitor = win.current_monitor().ok().flatten();
        let Some(monitor) = monitor else { return };
        let screen = monitor.size();
        let scale = monitor.scale_factor();
        let sw = screen.width as f64 / scale;
        let sh = screen.height as f64 / scale;
        let x = (sw - size.0) / 2.0;
        let y = sh - size.1 - MARGIN;
        let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
    }

    fn with_overlay<F>(app: &AppHandle, f: F)
    where
        F: FnOnce(&WebviewWindow) + Send + 'static,
    {
        let app = app.clone();
        let app_bg = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(win) = app_bg.get_webview_window(WINDOW_LABEL) {
                f(&win);
            }
        });
    }

    fn show_overlay(app: &AppHandle, size: (f64, f64)) {
        with_overlay(app, move |win| {
            park_bottom_center(win, size);
            let _ = win.show();
            let _ = win.set_focus();
        });
    }

    fn resize_overlay(app: &AppHandle, size: (f64, f64)) {
        with_overlay(app, move |win| {
            park_bottom_center(win, size);
        });
    }

    fn hide_overlay(app: &AppHandle) {
        with_overlay(app, |win| {
            let _ = win.hide();
        });
    }
}
