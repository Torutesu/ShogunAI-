//! SHOGUN Phase 0 notch-UI spike shell.
//!
//! Throwaway per spec §2.1 (only `spike-harness` is carried forward). Module boundaries
//! follow spec §3.11.1 and the one-way dependency rule `hover → statemachine → ipc/panel`;
//! `axcache` runs independently on focus events and must never be triggered by the state
//! machine (the "no collect-on-press" proof, spec §3.10.3).
//!
//! The macOS bodies (NSPanel, CGEventTap, AXUIElement) land on-device in T-05..T-12; the
//! modules below are typed stubs so the shell compiles and the wiring is reviewable first.

mod axcache;
mod display;
mod geometry;
mod hover;
mod ipc;
mod panel;
mod statemachine;

/// Tauri entry point. Registers the nspanel plugin, then in setup: reads the notch
/// geometry (T-06) and swaps the "notch" window into an NSPanel with the spec §3.1.2
/// attributes (T-05). Hover / axcache / display subsystems are wired in later increments.
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .setup(|_app| {
            #[cfg(target_os = "macos")]
            {
                use tauri::Manager;

                // T-06: read the real notch/pseudo geometry on the main thread and log it.
                if let Some(mtm) = objc2::MainThreadMarker::new() {
                    if let Some(g) = geometry::read_primary(mtm) {
                        eprintln!(
                            "[spike] geometry: notch={} notch_w={:.1} notch_h={:.1} menubar_h={:.1} screen={:.0}x{:.0}",
                            g.is_notch, g.notch_w, g.notch_h, g.menubar_h, g.screen.w, g.screen.h
                        );
                    }
                }

                // T-05: swap the notch window into an NSPanel with the spec §3.1.2 attributes.
                if let Some(win) = _app.get_webview_window("notch") {
                    if let Err(e) = panel::install(&win) {
                        eprintln!("[spike] panel install failed: {e}");
                    }
                }

                // T-07: install the listen-only mouse tap. The consumer drains raw samples;
                // on-device it normalises to NS and feeds HoverTracker → StateMachine.
                let (tx, rx) = std::sync::mpsc::channel::<hover::MouseSample>();
                hover::start(tx);
                std::thread::spawn(move || while rx.recv().is_ok() {});
            }
            // on-device (T-08+): state-machine timers driving the panel, axcache AXObserver,
            // display watch, harness JSONL writer thread.
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the SHOGUN spike shell");
}
