//! SHOGUN Windows / Linux windowed shell.
//!
//! Not the macOS Notch app. This process draws the Full-UI language in a real window, keeps a
//! tray presence, and talks to `shogun-platform` for paths and secrets. It does not capture the
//! screen, open sqlcipher, or send HTTP (FR-TR-03).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod commands;
mod tray;
mod view;

use tauri::Manager;

/// Windows taskbar grouping / pinning identity. Must run before the first window is created.
pub fn install_app_user_model_id() {
    #[cfg(windows)]
    aumid::install();
}

#[cfg(windows)]
mod aumid {
    /// Must match `tauri.conf.json` `identifier`.
    const ID: &str = "com.syogun.shogunai.pc";

    pub fn install() {
        use std::os::windows::ffi::OsStrExt;
        use std::ffi::OsStr;
        let wide: Vec<u16> = OsStr::new(ID).encode_wide().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` is a NUL-terminated UTF-16 string that lives for this call. Setting the
        // AppUserModelID before window creation is the documented requirement for taskbar pinning.
        let hr = unsafe { windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(wide.as_ptr()) };
        if hr < 0 {
            eprintln!("[shell] AppUserModelID was not set (HRESULT {hr})");
        }
    }
}

pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .setup(|app| {
            tray::install(app);
            if let Some(win) = app.get_webview_window("main") {
                let hidden = win.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = hidden.hide();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::shell_view,
            commands::autostart_get,
            commands::autostart_set,
            commands::open_app_data_dir,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        eprintln!("[shell] failed to start: {e}");
        std::process::exit(1);
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}
