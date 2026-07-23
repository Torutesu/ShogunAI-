//! NSPanel creation, attributes, and `ignoresMouseEvents` (spec §3.1).
//!
//! Swaps the Tauri "notch" WebviewWindow to an NSPanel via tauri-nspanel v2.1 and applies
//! the spec §3.1.2 attributes: styleMask `.borderless | .nonactivatingPanel`, level 25
//! (Status — above the menu bar; 101 blocks IME per tauri-nspanel #104, so keep 25 while the
//! search field is key, spec §3.5), collectionBehavior `.canJoinAllSpaces |
//! .fullScreenAuxiliary | .stationary | .ignoresCycle`. `ignoresMouseEvents` is toggled by
//! the state machine adapter (true in Idle/HoverIntent/Collapsing, false in Expanded).
#![allow(dead_code, unused_imports)]

#[cfg(target_os = "macos")]
pub use mac::install;

#[cfg(target_os = "macos")]
mod mac {
    // `Manager` is required in scope: the tauri_panel! expansion calls `window.app_handle()`.
    use tauri::{Manager, WebviewWindow};
    use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelLevel, StyleMask, WebviewWindowExt};

    // Declares an NSPanel subclass. Key-window behaviour is set here; level / collection
    // behaviour / style mask are applied at install time below.
    tauri_panel! {
        panel!(NotchPanel {
            config: {
                can_become_main_window: false,
                can_become_key_window: true,
                is_floating_panel: true
            }
        })
    }

    /// Convert `window` into an NSPanel and apply the spec §3.1.2 attributes.
    /// Concrete over the default (`Wry`) runtime — the macro impls `FromWindow<Wry>`.
    pub fn install(window: &WebviewWindow) -> tauri::Result<()> {
        let panel = window.to_panel::<NotchPanel>()?;
        panel.set_level(PanelLevel::Status.value());
        panel.set_collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()
                .stationary()
                .full_screen_auxiliary()
                .ignores_cycle()
                .value(),
        );
        panel.set_style_mask(StyleMask::new().nonactivating_panel().borderless().value());
        panel.set_becomes_key_only_if_needed(true);
        panel.set_floating_panel(true);
        // The panel hosts the always-visible product UI, so it must take clicks (chat, buttons).
        // Keep it interactive — the window resizes to fit (open vs handle), so it only covers a
        // small top-centre strip when collapsed rather than swallowing the whole screen.
        panel.set_ignores_mouse_events(false);
        panel.show();
        Ok(())
    }
}
