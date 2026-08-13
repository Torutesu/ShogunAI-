//! Dock icon visibility — NSApplication activation policy (Regular vs Accessory).

use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// When true, NSApplicationActivationPolicyRegular (Dock icon visible).
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
        .map(|d| d.join("dock_visibility.json"))
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

#[cfg(target_os = "macos")]
pub fn apply_activation_policy(visible: bool) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    // NSApplicationActivationPolicyRegular = 0, Accessory = 1 (no Dock icon; menu-bar tray OK).
    let policy: isize = if visible { 0 } else { 1 };
    let label = if visible {
        "Regular (Dock icon + menu-bar tray)"
    } else {
        "Accessory (menu-bar tray only)"
    };
    // SAFETY: standard AppKit calls on the shared NSApplication, on the main thread.
    unsafe {
        let ns_app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if ns_app.is_null() {
            eprintln!("[shell] NSApplication nil — activation policy unchanged");
            return;
        }
        let ok: bool = msg_send![ns_app, setActivationPolicy: policy];
        eprintln!("[shell] activation policy = {label} ok={ok}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn apply_activation_policy(_visible: bool) {}

#[cfg(target_os = "macos")]
pub mod mac {
    use super::{apply_activation_policy, load_settings, save_settings, Settings};
    use tauri::App;

    pub fn init(app: &App) {
        let settings = load_settings(app.handle());
        apply_activation_policy(settings.visible);
        eprintln!(
            "[dock] effective={} (activation policy applied)",
            if settings.visible { "visible" } else { "hidden" }
        );
    }

    #[tauri::command]
    pub fn get_dock_visible(app: tauri::AppHandle) -> bool {
        load_settings(&app).visible
    }

    #[tauri::command]
    pub fn set_dock_visible(visible: bool, app: tauri::AppHandle) -> Result<(), String> {
        save_settings(&app, &Settings { visible })?;
        apply_activation_policy(visible);
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
pub mod mac {
    pub fn init(_app: &tauri::App) {}

    #[tauri::command]
    pub fn get_dock_visible(_app: tauri::AppHandle) -> bool {
        true
    }

    #[tauri::command]
    pub fn set_dock_visible(_visible: bool, _app: tauri::AppHandle) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
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
