//! SHOGUN Phase 0 notch-UI spike shell.
//!
//! Throwaway per spec §2.1 (only the harness/core crates are carried forward). Module
//! boundaries follow spec §3.11.1; decision logic lives in `shogun_core` (tested on Linux),
//! measurement plumbing in `spike_harness`, and this crate is the macOS adapter layer.
//! `axcache` runs on focus events and must never be triggered by the state machine
//! (the "no collect-on-press" proof, spec §3.10.3).

mod axcache;
mod capture_source;
mod display;
mod geometry;
mod hover;
mod inline_source;
mod integrate;
mod notch_actions;
mod notch_exec;
mod panel;

/// Tauri entry point. Registers the nspanel plugin and the webview→Rust command half of
/// the closed IPC contract (spec §3.11.2), then in setup: NSPanel swap (T-05), geometry
/// read (T-06), mouse tap (T-07), and the integrated engine + measurement streams (T-08+).
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder
        .plugin(tauri_nspanel::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
        integrate::mac::painted,
        integrate::mac::interact,
        integrate::mac::promote,
        integrate::mac::hotkey,
        integrate::mac::open_full_ui,
        integrate::mac::anim_done,
        integrate::mac::collapse_request,
        integrate::mac::clock_sync_ack,
        integrate::mac::focus_field,
        notch_actions::mac::notch_actions,
        notch_exec::mac::run_notch_action,
        notch_exec::mac::confirm_notch_action,
        inline_source::mac::inline_at_cursor,
        inline_source::mac::shogun_status,
        inline_source::mac::shogun_state,
        inline_source::mac::shogun_chat,
        inline_source::mac::quit_app,
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

    // Make SHOGUN an accessory app FIRST. collectionBehavior=canJoinAllSpaces is set and reads
    // back correctly, yet the window still doesn't follow Spaces — because a Regular activation
    // policy (Dock icon, normal app) has its windows swept away on Space switch even with
    // canJoinAllSpaces. Accessory (LSUIElement) is the recipe every notch/menu-bar overlay uses:
    // no Dock icon, windows behave like system UI and actually join every Space + sit over
    // full-screen apps. Global shortcuts, capture, and the webview are unaffected.
    set_accessory_activation();

    // ALL-SPACES REALITY (on-device): a plain activating window ignores canJoinAllSpaces even when
    // it's set and reads back correctly (273) — it stays on its launch Space, off other desktops,
    // under full-screen apps. The only combination that actually follows every Space is a
    // NONACTIVATING NSPanel (canJoinAllSpaces + fullScreenAuxiliary, Status level) under the
    // Accessory activation policy set above. The panel renders the webview fine here (proven on
    // device). So the NSPanel is the DEFAULT; `SHOGUN_NO_NOTCH=1` falls back to the plain window
    // (renders + interactive, but does NOT follow Spaces) for debugging.
    if std::env::var("SHOGUN_NO_NOTCH").is_ok() {
        eprintln!("[shell] plain window mode (SHOGUN_NO_NOTCH=1) — renders but won't follow Spaces");
        if let Some(win) = app.get_webview_window("notch") {
            let _ = win.show();
            float_on_all_spaces(&win);
        }
    } else if let Some(win) = app.get_webview_window("notch") {
        match panel::install(&win) {
            Ok(()) => eprintln!("[shell] NSPanel installed (nonactivating, all-spaces overlay)"),
            Err(e) => eprintln!("[shell] panel install failed: {e} — try SHOGUN_NO_NOTCH=1"),
        }
        // Explicitly (re)assert collectionBehavior + level + orderFront and log the readback, so we
        // can confirm the values actually landed on the panel window (not just trust panel.rs).
        float_on_all_spaces(&win);
        // canJoinAllSpaces isn't honored on device even when set — so ACTIVELY keep the panel on
        // whatever Space becomes active: orderFrontRegardless pulls the window onto the current
        // Space. A light ticker re-asserts it so switching desktops / going full-screen shows the
        // panel within a fraction of a second regardless of whether the OS honors canJoinAllSpaces.
        keep_on_active_space(app, win.clone());
    } else {
        eprintln!("[spike] no 'notch' window — panel not installed");
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
    // Pin the panel to the top-centre of the primary display so it hangs from the notch.
    if let Some(win) = app.get_webview_window("notch") {
        let scale = win.scale_factor().unwrap_or(1.0);
        let win_w = win.outer_size().map(|s| s.width as f64 / scale).unwrap_or(400.0);
        let x = ((g.screen.w - win_w) / 2.0).max(0.0);
        match win.set_position(tauri::LogicalPosition::new(x, 0.0)) {
            Ok(()) => eprintln!("[shell] panel pinned top-centre x={x:.0} y=0"),
            Err(e) => eprintln!("[shell] set_position failed: {e}"),
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

    // ⌘⇧Space: open the panel directly (statemachine §3.3 Hotkey→Expanded) without depending on
    // hover. Registered here so a flaky CGEventTap can't leave the panel unreachable.
    register_expand_shortcut(app);

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

    // WP2.2: start the memory capture source. Open the on-device DB under the app-data dir and
    // poll the focus into memory (exclusion → walk → collapse → extract). AX text only (invariant
    // 2). If the DB can't be opened the daemon simply doesn't capture — the shell keeps running.
    match memory_db(app) {
        Ok(db) => {
            // Share the handle: Tauri state (notch_actions / execution) + the capture poller.
            app.manage(db.clone());
            app.manage(notch_exec::mac::new_engine(db.clone()));
            let policy = shogun_core::capture::exclusion::ExclusionPolicy::new();
            let _ = capture_source::spawn_capture_poller(db, policy, None);
            eprintln!("[spike] capture source started (poll {}ms)", capture_source::DEFAULT_POLL_MS);
        }
        Err(e) => eprintln!("[spike] memory DB unavailable — capture source not started: {e}"),
    }
}

/// Set the app's activation policy to Accessory (NSApplicationActivationPolicyAccessory = 1): no
/// Dock icon, background overlay. Required for the notch window to actually follow every Space and
/// draw over full-screen apps — a Regular-policy app ignores that even with canJoinAllSpaces set.
/// Runs on the main thread (setup).
#[cfg(target_os = "macos")]
fn set_accessory_activation() {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    // SAFETY: standard AppKit calls on the shared NSApplication, on the main thread.
    unsafe {
        let ns_app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if ns_app.is_null() {
            eprintln!("[shell] NSApplication nil — activation policy unchanged");
            return;
        }
        let ok: bool = msg_send![ns_app, setActivationPolicy: 1isize];
        eprintln!("[shell] activation policy = Accessory (no Dock icon, all-spaces overlay) ok={ok}");
    }
}

/// Keep the panel on whatever Space is active. canJoinAllSpaces isn't honored on device (confirmed
/// set but ignored — even bundled, likely a window manager), so a 400ms ticker checks
/// `isOnActiveSpace` and, only when the panel is NOT on the current Space, re-assigns it there
/// (orderOut clears the old assignment, orderFrontRegardless re-adds it to the active Space).
/// Skipping the no-op case means no flicker while you use it; it only moves when you switch
/// desktops / go full-screen. Nonactivating orderFront doesn't steal focus.
#[cfg(target_os = "macos")]
fn keep_on_active_space(app: &tauri::App, win: tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    let handle = app.handle().clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(400));
        let w = win.clone();
        let posted = handle.run_on_main_thread(move || {
            if let Ok(p) = w.ns_window() {
                if !p.is_null() {
                    let ptr = p as *mut AnyObject;
                    // SAFETY: live NSWindow, messaged on the main thread.
                    unsafe {
                        // Only act when the panel is NOT on the Space the user is looking at. Asking
                        // the OS to keep it everywhere (canJoinAllSpaces) is ignored on this machine,
                        // so instead we detect "not here" and pull it here: orderOut clears the stale
                        // Space assignment, orderFrontRegardless re-adds the window to the CURRENTLY
                        // active Space. Because we skip this when it's already on the active Space,
                        // there's no flicker while you're using it — it only moves when you switch
                        // desktops / go full-screen. Works regardless of window managers.
                        let on_active: bool = msg_send![ptr, isOnActiveSpace];
                        if !on_active {
                            let nil: *mut AnyObject = std::ptr::null_mut();
                            let _: () = msg_send![ptr, orderOut: nil];
                            let _: () = msg_send![ptr, orderFrontRegardless];
                        }
                    }
                }
            }
        });
        if posted.is_err() {
            break; // app is shutting down
        }
    });
}

/// Make the plain (rendering) window float on every Space and over full-screen apps by setting its
/// NSWindow `collectionBehavior` + level directly — the same recipe the NSPanel uses, minus the
/// window-class swap that blanks the wry webview on device. Runs on the main thread (setup).
#[cfg(target_os = "macos")]
fn float_on_all_spaces(win: &tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let ptr = match win.ns_window() {
        Ok(p) if !p.is_null() => p as *mut AnyObject,
        Ok(_) => {
            eprintln!("[shell] ns_window null — cannot set all-spaces behavior");
            return;
        }
        Err(e) => {
            eprintln!("[shell] ns_window unavailable: {e} — cannot set all-spaces behavior");
            return;
        }
    };

    // NSWindowCollectionBehavior bits: CanJoinAllSpaces (1<<0) | Stationary (1<<4) |
    // FullScreenAuxiliary (1<<8) = 273. CanJoinAllSpaces keeps the window on EVERY Space without
    // needing activation — the right choice for a nonactivating panel. (MoveToActiveSpace was wrong:
    // it only relocates on window *activation*, which a nonactivating panel never does.) The dev
    // tests where CanJoinAllSpaces "didn't work" were all UNBUNDLED; the bundled accessory app is
    // the first real test of this recipe.
    let behavior: usize = (1 << 0) | (1 << 4) | (1 << 8);
    // NSStatusWindowLevel (25): floats above ordinary and full-screen windows. Matches panel.rs;
    // 25 is IME-safe (101 is the level that blocks input methods, tauri-nspanel #104).
    let level: isize = 25;

    // SAFETY: `ptr` is the live NSWindow owned by Tauri; we message it synchronously on the main
    // thread. The setters take a scalar (NSUInteger / NSInteger) and return void; the getters
    // return the same scalar so we can confirm the value actually stuck (tao/wry may re-apply its
    // own collectionBehavior during startup and silently clobber ours).
    unsafe {
        let _: () = msg_send![ptr, setCollectionBehavior: behavior];
        let _: () = msg_send![ptr, setLevel: level];
        // Accessory (background) apps do NOT auto-show their windows — the panel vanished the
        // moment we set the Accessory policy. orderFrontRegardless forces the window visible even
        // while the app is inactive; combined with canJoinAllSpaces it's the standard always-on
        // overlay recipe. Re-run on each re-apply so it stays put.
        let _: () = msg_send![ptr, orderFrontRegardless];
        let got: usize = msg_send![ptr, collectionBehavior];
        let lvl: isize = msg_send![ptr, level];
        eprintln!("[shell] NSWindow collectionBehavior set={behavior} readback={got} level={lvl}, ordered front");
    }
}

/// Register the ⌘⇧Space global shortcut → feed a Hotkey input to the engine (Idle→Expanded direct,
/// statemachine §3.3). Errors are logged, not fatal — the app still runs (hover remains available).
#[cfg(target_os = "macos")]
fn register_expand_shortcut(app: &tauri::App) {
    use std::sync::Arc;
    use tauri::Manager;
    use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

    // ⌘⇧J: ⌘⇧Space collides with the input-method source switcher on JP keyboards, so the OS
    // consumes it before the handler runs. J is uncontended.
    let expand = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyJ);
    let res = app.global_shortcut().on_shortcut(expand, move |app, _sc, event| {
        // Diagnostic: log every delivery so we can tell the handler fired even if state differs.
        eprintln!("[spike] shortcut fired: {:?}", event.state());
        if event.state() == ShortcutState::Pressed {
            if let Some(shared) = app.try_state::<Arc<integrate::mac::Shared>>() {
                shared.trigger_hotkey();
            }
            // Also run the UI-independent core self-test so the product path is verifiable even if
            // the spike's webview panel doesn't render.
            notch_exec::mac::self_test(app);
        }
    });
    match res {
        Ok(()) => eprintln!("[spike] ⌘⇧J registered — press it to open the panel"),
        Err(e) => eprintln!("[spike] global shortcut registration failed: {e}"),
    }

    // ⌃⌥G → draft at the cursor (inline). The product trigger is a bare Option tap (rebindable in
    // Settings), which needs a CGEventTap on flagsChanged — an on-device refinement; a concrete
    // combo is used here so the read→generate→insert loop is testable now.
    use shogun_core::daemon::Db;
    let draft = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyG);
    let res = app.global_shortcut().on_shortcut(draft, move |app, _sc, event| {
        if event.state() == ShortcutState::Pressed {
            if let Some(db) = app.try_state::<Db>() {
                inline_source::mac::run_inline_at_cursor(db.inner().clone());
            }
        }
    });
    match res {
        Ok(()) => eprintln!("[spike] ⌃⌥G registered — press it to draft at the cursor"),
        Err(e) => eprintln!("[spike] inline shortcut registration failed: {e}"),
    }
}

/// Open (creating if needed) the on-device memory DB under the app-data dir, with a real
/// wall-clock. macOS-only; the DB is owned by the Rust core (CLAUDE.md invariant 1).
#[cfg(target_os = "macos")]
fn memory_db(app: &tauri::App) -> Result<shogun_core::daemon::Db, String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("memory.db");
    eprintln!("[spike] memory DB: {}", path.display());
    let clock = std::sync::Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    });
    shogun_core::daemon::Db::open(path, clock).map_err(|e| e.to_string())
}
