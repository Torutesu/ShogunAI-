//! SHOGUN Phase 0 notch-UI spike shell.
//!
//! Throwaway per spec §2.1 (only the harness/core crates are carried forward). Module
//! boundaries follow spec §3.11.1; decision logic lives in `spike_core` (tested on Linux),
//! measurement plumbing in `spike_harness`, and this crate is the macOS adapter layer.
//! `axcache` runs on focus events and must never be triggered by the state machine
//! (the "no collect-on-press" proof, spec §3.10.3).

mod axcache;
mod display;
mod geometry;
mod hover;
mod integrate;
mod ipc;
mod panel;
mod statemachine;

/// Tauri entry point. Registers the nspanel plugin and the webview→Rust command half of
/// the closed IPC contract (spec §3.11.2), then in setup: NSPanel swap (T-05), geometry
/// read (T-06), mouse tap (T-07), and the integrated engine + measurement streams (T-08+).
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init()).invoke_handler(tauri::generate_handler![
        integrate::painted,
        integrate::interact,
        integrate::anim_done,
        integrate::collapse_request,
        integrate::clock_sync_ack,
        integrate::focus_field,
    ]);

    builder
        .setup(|_app| {
            #[cfg(target_os = "macos")]
            setup_macos(_app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the SHOGUN spike shell");
}

/// macOS wiring, flat with early returns (each step logs its own failure — a missing
/// optional must not silently skip later subsystems).
#[cfg(target_os = "macos")]
fn setup_macos(app: &tauri::App) {
    use tauri::Manager;

    // T-05: swap the notch window into an NSPanel with the spec §3.1.2 attributes.
    match app.get_webview_window("notch") {
        Some(win) => {
            if let Err(e) = panel::install(&win) {
                eprintln!("[spike] panel install failed: {e}");
            }
        }
        None => eprintln!("[spike] no 'notch' window — panel not installed"),
    }

    // T-06: geometry (panel screen + CG conversion constants).
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        eprintln!("[spike] setup not on main thread — engine not started");
        return;
    };
    let Some(g) = geometry::read_primary(mtm) else {
        eprintln!("[spike] no screen — engine not started");
        return;
    };
    eprintln!(
        "[spike] geometry: notch={} notch_w={:.1} notch_h={:.1} menubar_h={:.1} screen={:.0}x{:.0} primary_h={:.0} displays={}",
        g.is_notch, g.notch_w, g.notch_h, g.menubar_h, g.screen.w, g.screen.h, g.primary_height, g.display_count
    );

    // T-07/T-08: mouse tap → integrated engine + measurement streams.
    let (tx, rx) = std::sync::mpsc::channel::<hover::TapEvent>();
    hover::start(tx);
    let menubar_min_y = g.screen.max_y() - g.menubar_h;
    let shared = integrate::start(
        app.handle().clone(),
        integrate::StartGeometry {
            regions: g.regions,
            menubar_min_y,
            primary_height: g.primary_height,
            is_notch: g.is_notch,
            display_count: g.display_count,
        },
        rx,
    );
    app.manage(shared);

    // T-11/T-12 sanity: Accessibility trust + one focused-window walk through the tested
    // policy. Event-driven focus subscription is on-device work (runbook D-03/D-05).
    eprintln!("[spike] accessibility trusted: {}", axcache::ax_trusted());
    if let Some(pid) = display::frontmost_pid() {
        if let Some(r) = axcache::snapshot(pid, 250) {
            eprintln!(
                "[spike] ax snapshot: {} bytes, {} elements, depth {}, partial={}",
                r.text_bytes, r.elements_visited, r.depth_reached, r.partial
            );
        }
    }
}
