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

/// Tauri entry point. On-device (T-05) this: creates the WebviewWindow, swaps it to an
/// NSPanel (`tauri-nspanel` v2.1, research item 1), applies the spec §3.1.2 attributes,
/// and starts the hover / axcache / display subsystems. Kept minimal until then.
pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            // T-06: read the real notch/pseudo geometry on the main thread and log it.
            // on-device (T-05+): panel::install(app)?; hover::start(...); axcache::start(...);
            // display::start(...); harness JSONL writer thread.
            #[cfg(target_os = "macos")]
            if let Some(mtm) = objc2::MainThreadMarker::new() {
                if let Some(g) = geometry::read_primary(mtm) {
                    eprintln!(
                        "[spike] geometry: notch={} notch_w={:.1} notch_h={:.1} menubar_h={:.1} screen={:.0}x{:.0}",
                        g.is_notch, g.notch_w, g.notch_h, g.menubar_h, g.screen.w, g.screen.h
                    );
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the SHOGUN spike shell");
}
