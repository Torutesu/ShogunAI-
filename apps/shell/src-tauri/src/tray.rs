//! Tray icon. Close-to-tray is a Windows professional default; Quit is explicit.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::App;

pub fn install(app: &mut App) {
    let show = match MenuItem::with_id(app, "show", "Open ShogunAI", true, None::<&str>) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[shell] tray menu item failed: {e}");
            return;
        }
    };
    let quit = match MenuItem::with_id(app, "quit", "Quit ShogunAI", true, None::<&str>) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[shell] tray menu item failed: {e}");
            return;
        }
    };
    let menu = match Menu::with_items(app, &[&show, &quit]) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[shell] tray menu failed: {e}");
            return;
        }
    };

    let icon = match tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png")) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[shell] tray icon unusable ({e}) — no tray this run");
            return;
        }
    };

    let built = TrayIconBuilder::with_id("shogun-tray")
        .menu(&menu)
        .tooltip("ShogunAI")
        .icon(icon)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => crate::show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::show_main_window(tray.app_handle());
            }
        })
        .build(app);

    if let Err(e) = built {
        eprintln!("[shell] tray install failed: {e}");
    }
}
