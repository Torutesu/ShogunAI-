//! The launch window: the mark folding itself together while the core comes up.
//!
//! The shell had no splash at all — `tauri.conf.json` declares no windows and every one of them is
//! built from Rust when something needs it — so the first thing a launch drew was whatever opened
//! first. This is a small transparent window that shows the arrival and then goes away.
//!
//! It is built hidden and shown only once its page has finished loading, so a webview that is slow
//! or broken produces *no splash* rather than an empty rectangle over the desktop. The close is a
//! timer this module owns and the webview cannot influence: a splash that fails to close is worse
//! than no splash, so nothing about closing depends on the page having run.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    const LABEL: &str = "splash";

    /// The mark's own arrival is 760ms; the rest is time to read it as finished rather than cut.
    const ON_SCREEN_MS: u64 = 1_150;

    /// Big enough for a 112px mark to breathe, small enough not to read as a window.
    const SIZE: f64 = 280.0;

    static SHOWN: AtomicBool = AtomicBool::new(false);

    /// Put the launch window up, and take it down again. Returns immediately.
    pub fn init(app: &tauri::App) {
        if SHOWN.swap(true, Ordering::SeqCst) {
            return; // one launch, one arrival
        }
        let handle = app.handle().clone();

        let builder = WebviewWindowBuilder::new(&handle, LABEL, WebviewUrl::App("splash.html".into()))
            .title("ShogunAI")
            .inner_size(SIZE, SIZE)
            .resizable(false)
            .decorations(false)
            // `macos-private-api` is already on for the notch panel; the launch window uses it for
            // the same reason — the mark should fold against the desktop, not against a panel.
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            // Never take focus. A launch that steals the keyboard for a second is worse than a
            // launch with no animation at all.
            .focused(false)
            .center()
            .visible(false)
            .on_page_load(|window, payload| {
                if payload.event() == tauri::webview::PageLoadEvent::Finished {
                    let _ = window.show();
                }
            });

        match builder.build() {
            Ok(_) => eprintln!("[splash] launch window built"),
            Err(e) => {
                // Survivable: the app opens exactly as it did before this existed.
                eprintln!("[splash] launch window unavailable ({e}) — starting without it");
                return;
            }
        }

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(ON_SCREEN_MS));
            let closer = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                if let Some(window) = closer.get_webview_window(LABEL) {
                    let _ = window.close();
                }
            });
        });
    }
}

#[cfg(not(target_os = "macos"))]
pub mod mac {
    pub fn init(_app: &tauri::App) {}
}
