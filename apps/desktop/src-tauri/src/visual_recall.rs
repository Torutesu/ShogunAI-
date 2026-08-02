//! Visual recall settings + Tauri commands (issue #106/#107).
//!
//! Opt-in screen OCR (default off). Persisted to `visual_recall.json` under app data; the capture
//! poller reads the shared `RwLock` each tick so toggling applies immediately.
//!
//! OCR gate + text-region modules mirror Screenpipe's capture path (decision B, no image storage).

#[cfg(all(target_os = "macos", feature = "visual-recall-ocr"))]
pub mod ocr_gate;
#[cfg(all(target_os = "macos", feature = "visual-recall-ocr"))]
pub mod pipeline;
#[cfg(all(target_os = "macos", feature = "visual-recall-ocr"))]
pub mod text_regions;

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::{Mutex, RwLock};

    use shogun_core::capture::visual_recall::Settings;
    use tauri::Manager;

    pub type SharedSettings = std::sync::Arc<RwLock<Settings>>;

    static LANE: Mutex<Option<SharedSettings>> = Mutex::new(None);

    fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("visual_recall.json"))
    }

    /// Load persisted settings, publish the shared handle, return it for the capture poller.
    pub fn init(app: &tauri::AppHandle) -> SharedSettings {
        let mut settings = Settings::default();
        if let Some(p) = settings_path(app) {
            if let Ok(text) = std::fs::read_to_string(p) {
                if let Ok(saved) = serde_json::from_str::<Settings>(&text) {
                    settings = saved;
                }
            }
        }
        let shared = std::sync::Arc::new(RwLock::new(settings.clone()));
        if let Ok(mut g) = LANE.lock() {
            *g = Some(shared.clone());
        }
        eprintln!(
            "[visual_recall] screen OCR {}",
            if settings.enabled { "enabled" } else { "off (default)" }
        );
        shared
    }

    fn save(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
        let Some(p) = settings_path(app) else { return Ok(()) };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        std::fs::write(&p, json).map_err(|e| format!("save failed: {e}"))
    }

    #[tauri::command]
    pub fn get_visual_recall_settings() -> Settings {
        LANE.lock()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.read().ok().map(|v| v.clone())))
            .flatten()
            .unwrap_or_default()
    }

    /// Master switch for screen OCR (issue #107). Persist first, then apply.
    #[tauri::command]
    pub fn set_visual_recall_enabled(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
        let candidate = {
            let Ok(g) = LANE.lock() else { return Err("busy".into()) };
            let Some(shared) = g.as_ref() else { return Err("not ready".into()) };
            let current = shared.read().map_err(|_| "busy".to_string())?;
            Settings { enabled, ..current.clone() }
        };
        save(&app, &candidate)?;
        let Ok(g) = LANE.lock() else { return Err("busy".into()) };
        let Some(shared) = g.as_ref() else { return Err("not ready".into()) };
        let mut live = shared.write().map_err(|_| "busy".to_string())?;
        *live = candidate;
        eprintln!(
            "[visual_recall] screen OCR {}",
            if enabled { "enabled" } else { "off" }
        );
        Ok(())
    }
}
