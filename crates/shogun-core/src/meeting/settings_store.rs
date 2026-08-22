//! Shared persistence for meeting settings.
//!
//! The desktop owns capture, but MCP/CLI/REST need to change the same user setting. Keeping the
//! file location and atomic write here prevents those faces from silently diverging.

use std::path::{Path, PathBuf};

use super::settings::Settings;

const DESKTOP_IDENTIFIER: &str = "com.syogun.shogunai";
const SETTINGS_FILE: &str = "meeting.json";

/// The desktop app-data file, with a debug/test override for isolated tests.
pub fn settings_path() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Ok(path) = std::env::var("SHOGUN_MEETING_SETTINGS_JSON") {
        if !path.trim().is_empty() {
            return Some(PathBuf::from(path));
        }
    }

    #[cfg(not(debug_assertions))]
    let _ = SETTINGS_FILE;

    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        return Some(
            PathBuf::from(home)
                .join("Library/Application Support")
                .join(DESKTOP_IDENTIFIER)
                .join(SETTINGS_FILE),
        );
    }
    None
}

/// Read a settings file. Bad or absent files are safe defaults: meeting notes remain off.
pub fn load() -> Settings {
    settings_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

/// Persist settings atomically so another API face never observes half-written JSON.
pub fn save(settings: &Settings) -> Result<(), String> {
    let path = settings_path().ok_or("meeting settings unavailable")?;
    save_at(&path, settings)
}

pub fn save_at(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("meeting settings unavailable: {e}"))?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, json).map_err(|e| format!("save failed: {e}"))?;
    std::fs::rename(&temporary, path).map_err(|e| format!("save failed: {e}"))
}
