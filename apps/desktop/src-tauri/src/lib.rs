//! SHOGUN Phase 0 notch-UI spike shell.
//!
//! Throwaway per spec §2.1 (only the harness/core crates are carried forward). Module
//! boundaries follow spec §3.11.1; decision logic lives in `shogun_core` (tested on Linux),
//! measurement plumbing in `spike_harness`, and this crate is the macOS adapter layer.
//! `axcache` runs on focus events and must never be triggered by the state machine
//! (the "no collect-on-press" proof, spec §3.10.3).

mod axcache;
mod display;
mod geometry;
mod hover;
mod integrate;
mod panel;

/// Tauri entry point. Registers the nspanel plugin and the webview→Rust command half of
/// the closed IPC contract (spec §3.11.2), then in setup: NSPanel swap (T-05), geometry
/// read (T-06), mouse tap (T-07), and the integrated engine + measurement streams (T-08+).
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init()).invoke_handler(tauri::generate_handler![
        integrate::mac::painted,
        integrate::mac::interact,
        integrate::mac::promote,
        integrate::mac::hotkey,
        integrate::mac::open_full_ui,
        integrate::mac::anim_done,
        integrate::mac::collapse_request,
        integrate::mac::clock_sync_ack,
        integrate::mac::focus_field,
    ]);

    // NOTE: do NOT add .on_page_load here — with the NSPanel-swapped window it trips a
    // wry 0.55.1 unwrap-None panic (wkwebview/mod.rs:1349) and kills the app at startup
    // (observed on the smoke runner). Diagnostics use the delayed eval probe instead.
    builder
        .setup(|_app| {
            #[cfg(target_os = "macos")]
            setup_macos(_app);
            #[cfg(target_os = "macos")]
            {
                // KNOWN WRY PITFALL (runs #4/#5): calling WebviewWindow::eval() or url()
                // on the NSPanel-swapped window panics wry 0.55.1 (wkwebview/mod.rs:1349
                // unwrap-None) and kills the main thread ~immediately. Do not probe the
                // webview from Rust; webview liveness is checked via the boot-ping
                // command from JS instead (cmd interact kind=boot in the log).
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            // Startup failure: report and exit without a panic (CLAUDE.md: no
            // unwrap/expect outside tests; errors must not take the process down
            // via panic paths).
            eprintln!("[spike] fatal: failed to run the shell: {e}");
            std::process::exit(1);
        });
}

/// macOS wiring, flat with early returns (each step logs its own failure — a missing
/// optional must not silently skip later subsystems).
#[cfg(target_os = "macos")]
fn setup_macos(app: &tauri::App) {
    use tauri::Manager;

    // T-05: swap the notch window into an NSPanel with the spec §3.1.2 attributes.
    // SPIKE_NO_PANEL=1 skips the swap — diagnostic flag to isolate whether the swap
    // itself breaks the webview (silent-webview investigation, smoke runs #2/#3).
    if std::env::var("SPIKE_NO_PANEL").is_ok() {
        eprintln!("[spike] SPIKE_NO_PANEL set — NSPanel swap skipped");
    } else {
        match app.get_webview_window("notch") {
            Some(win) => {
                if let Err(e) = panel::install(&win) {
                    eprintln!("[spike] panel install failed: {e}");
                }
            }
            None => eprintln!("[spike] no 'notch' window — panel not installed"),
        }
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

    // Pin the panel to the top-centre of the primary display so the idle shell sits flush
    // under the notch and the Expanded panel drops straight down from it. An un-positioned
    // Tauri window is centred on screen (observed: the panel appeared mid-screen), which
    // contradicts the notch-anchored layout (spec §3.1.3). x is centred on the screen
    // width; y=0 puts the window top at the screen top (the panel level is Status, so it
    // draws over the menu-bar band).
    if let Some(win) = app.get_webview_window("notch") {
        let scale = win.scale_factor().unwrap_or(1.0);
        let win_w = win.outer_size().map(|s| s.width as f64 / scale).unwrap_or(432.0);
        let x = ((g.screen.w - win_w) / 2.0).max(0.0);
        match win.set_position(tauri::LogicalPosition::new(x, 0.0)) {
            Ok(()) => eprintln!("[spike] panel positioned at top-centre x={x:.0} y=0"),
            Err(e) => eprintln!("[spike] set_position failed: {e}"),
        }
    }

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
