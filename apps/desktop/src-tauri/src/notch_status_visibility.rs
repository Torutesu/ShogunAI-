//! Idle notch chin visibility — "reading …" / due counts vs quiet welded hide.

use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// When true, Idle shows reading/app/due chrome. When false, welded hide until hover.
    #[serde(default = "default_visible")]
    pub visible: bool,
}

fn default_visible() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            visible: default_visible(),
        }
    }
}

pub fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("notch_status_visibility.json"))
}

pub fn load_settings(app: &tauri::AppHandle) -> Settings {
    let Some(path) = settings_path(app) else {
        return Settings::default();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_settings(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app).ok_or("app data dir unavailable")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create settings dir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("write settings: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("commit settings: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn get_notch_status_visible(app: tauri::AppHandle) -> bool {
    load_settings(&app).visible
}

#[tauri::command]
pub fn set_notch_status_visible(visible: bool, app: tauri::AppHandle) -> Result<(), String> {
    save_settings(&app, &Settings { visible })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn default_is_visible() {
        assert!(Settings::default().visible);
    }

    #[test]
    fn roundtrip_json() {
        let s = Settings { visible: false };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn missing_field_defaults_visible() {
        let back: Settings = serde_json::from_str("{}").unwrap();
        assert!(back.visible);
    }
}
