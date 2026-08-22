//! IPC. Presentation-only commands: the view, autostart, and opening the data folder.
//! Secrets are not get/set from the webview in this slice.

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::view::{self, ShellView};

#[tauri::command]
pub fn shell_view() -> ShellView {
    view::assemble()
}

#[tauri::command]
pub fn autostart_get(app: AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn autostart_set(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let launch = app.autolaunch();
    if enabled {
        launch.enable().map_err(|e| e.to_string())?;
    } else {
        launch.disable().map_err(|e| e.to_string())?;
    }
    launch.is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_app_data_dir() -> Result<String, String> {
    let path = shogun_platform::ensure_app_data_dir().map_err(|e| e.to_string())?;
    spawn_folder_open(&path)?;
    Ok(path.display().to_string())
}

fn spawn_folder_open(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = path;
        Err("opening the data folder is only implemented on Windows and Linux".to_string())
    }
}
