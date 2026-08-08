//! Launch-at-login preference (issue #109).
//!
//! Uses the OS login-item API only — **never** a watchdog that relaunches after Quit.
//! Quit (`std::process::exit`) stays quit until the next user login when the preference is on.

use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// User preference: start Shogun when this user signs in to the Mac.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: default_enabled() }
    }
}

pub fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("launch_at_login.json"))
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
pub mod mac {
    use super::{load_settings, save_settings, Settings};
    use tauri::App;
    use tauri_plugin_autostart::ManagerExt;

    /// Older builds used LaunchAgent (plist in ~/Library/LaunchAgents). That autostarts but does
    /// not appear in System Settings → Login Items, and would double-launch after we switch modes.
    fn remove_legacy_launch_agent_plist() {
        let Ok(home) = std::env::var("HOME") else {
            return;
        };
        let legacy = std::path::PathBuf::from(home).join("Library/LaunchAgents/SHOGUN Spike.plist");
        if legacy.exists() {
            if let Err(e) = std::fs::remove_file(&legacy) {
                eprintln!("[launch] legacy LaunchAgent plist remove failed: {e}");
            } else {
                eprintln!("[launch] removed legacy LaunchAgent plist");
            }
        }
    }

    /// Sync the saved preference to the OS login item. Login items launch on sign-in only —
    /// they do not respawn a process the user just quit.
    pub fn apply_os_state(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
        let mgr = app.autolaunch();
        let os_enabled = mgr.is_enabled().map_err(|e| format!("autostart status: {e}"))?;
        if enabled && !os_enabled {
            mgr.enable().map_err(|e| format!("enable launch at login: {e}"))?;
            eprintln!("[launch] login item enabled");
        } else if !enabled && os_enabled {
            mgr.disable().map_err(|e| format!("disable launch at login: {e}"))?;
            eprintln!("[launch] login item disabled");
        }
        Ok(())
    }

    /// Read OS login-item truth, repair mismatches, and persist effective state when needed.
    fn reconcile_with_os(app: &tauri::AppHandle) -> Settings {
        let prefs = load_settings(app);
        let mgr = app.autolaunch();

        let os_enabled = match mgr.is_enabled() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[launch] reconcile: could not read OS login-item state: {e}");
                return prefs;
            }
        };

        if prefs.enabled == os_enabled {
            return Settings { enabled: os_enabled };
        }

        eprintln!(
            "[launch] reconcile: pref={} os={}",
            if prefs.enabled { "on" } else { "off" },
            if os_enabled { "on" } else { "off" }
        );

        if prefs.enabled && !os_enabled {
            if let Err(e) = mgr.enable() {
                eprintln!("[launch] reconcile: retry enable failed: {e}");
            }
        } else if !prefs.enabled && os_enabled {
            if let Err(e) = mgr.disable() {
                eprintln!("[launch] reconcile: disable stale login item failed: {e}");
            }
        }

        let effective = mgr.is_enabled().unwrap_or(os_enabled);

        if effective != prefs.enabled {
            let reconciled = Settings { enabled: effective };
            if let Err(e) = save_settings(app, &reconciled) {
                eprintln!("[launch] reconcile: persist effective state failed: {e}");
            }
        }

        Settings { enabled: effective }
    }

    pub fn init(app: &App) {
        remove_legacy_launch_agent_plist();
        let settings = reconcile_with_os(app.handle());
        eprintln!(
            "[launch] effective={} (login-item reconcile ok)",
            if settings.enabled { "on" } else { "off" }
        );
    }

    #[tauri::command]
    pub fn get_launch_at_login_settings(app: tauri::AppHandle) -> Settings {
        reconcile_with_os(&app)
    }

    /// Persist first, then touch the OS login item (same order as meeting settings).
    #[tauri::command]
    pub fn set_launch_at_login_enabled(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
        let next = Settings { enabled };
        save_settings(&app, &next)?;
        apply_os_state(&app, enabled)?;
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
pub mod mac {
    use super::Settings;

    pub fn init(_app: &tauri::App) {}

    #[tauri::command]
    pub fn get_launch_at_login_settings(_app: tauri::AppHandle) -> Settings {
        Settings { enabled: false }
    }

    #[tauri::command]
    pub fn set_launch_at_login_enabled(_enabled: bool, _app: tauri::AppHandle) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn default_is_enabled() {
        assert!(Settings::default().enabled);
    }

    #[test]
    fn roundtrip_json() {
        let s = Settings { enabled: false };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn missing_field_defaults_enabled() {
        let back: Settings = serde_json::from_str("{}").unwrap();
        assert!(back.enabled);
    }
}
