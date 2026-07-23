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

/// The collectionBehavior the overlay wants, selected at setup (NSPanel mode = canJoinAllSpaces +
/// fullScreenAuxiliary = 257; plain-window fallback = moveToActiveSpace 274) and re-asserted by
/// every heal/reassert path. `stationary` (1<<4) was dropped: it is a suspect for the panel not
/// tracking Space switches on this machine, and the reference overlays run without it.
#[cfg(target_os = "macos")]
static PANEL_BEHAVIOR: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new((1 << 0) | (1 << 8));

/// NSFloatingWindowLevel (3) — the overlay spec's `.floating`: above every normal app window,
/// below system UI. (Earlier builds used Status/25.)
#[cfg(target_os = "macos")]
const OVERLAY_LEVEL: isize = 3;

/// True while the USER hid the overlay (toggle shortcut / Esc / tray). The auto-residency
/// machinery (watchers, heal, respawn) must respect this — a deliberately hidden panel stays
/// hidden until summoned again.
#[cfg(target_os = "macos")]
static USER_HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
        inline_source::mac::ui_log,
        shortcuts::get_shortcuts,
        shortcuts::set_shortcut,
        shortcuts::hide_panel,
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

    // Loud identity banner: if more than one SHOGUN is alive (e.g. a stale bundled "SHOGUN
    // Spike.app" left running from an earlier `open`), the visible panel may be the OLD process
    // while shortcuts hit the new one — which looks exactly like "quit button dead, drag dead".
    // The PID makes that unambiguous in the log.
    eprintln!("========================================================");
    eprintln!("[shell] SHOGUN starting — pid {} — build: plain-window/drag/quit", std::process::id());
    eprintln!("========================================================");

    // PROVEN by [panelstate]: a Regular app's plain window is REFUSED entry to other apps'
    // Spaces — onActiveSpace/drawn stayed false through hundreds of re-orders with both
    // canJoinAllSpaces (273) and moveToActiveSpace (274). That's an OS wall, not a flag problem.
    // The sanctioned way through it is the FULL overlay recipe: nonactivating NSPanel + Accessory
    // activation + canJoinAllSpaces/fsAux + hidesOnDeactivate=false. We never ran that combination
    // complete — the hidesOnDeactivate fix landed only after the NSPanel default was abandoned, so
    // every earlier NSPanel test self-hid on deactivate. Default = the full recipe;
    // SHOGUN_NO_NOTCH=1 keeps the plain window as a fallback.
    use std::sync::atomic::Ordering;
    if std::env::var("SHOGUN_NO_NOTCH").is_ok() {
        PANEL_BEHAVIOR.store((1 << 1) | (1 << 4) | (1 << 8), Ordering::Relaxed); // 274 move-to-active
        if let Some(win) = app.get_webview_window("notch") {
            let _ = win.show();
            float_on_all_spaces(&win);
            eprintln!("[shell] plain window fallback (SHOGUN_NO_NOTCH=1) — desktop-space only");
        }
    } else {
        PANEL_BEHAVIOR.store((1 << 0) | (1 << 8), Ordering::Relaxed); // 257 join-all + fsAux
        set_accessory_activation();
        match app.get_webview_window("notch") {
            Some(win) => {
                match panel::install(&win) {
                    Ok(()) => eprintln!("[shell] NSPanel installed — FULL overlay recipe (accessory + nonactivating + joinAll + hides=false)"),
                    Err(e) => eprintln!("[shell] panel install failed: {e} — try SHOGUN_NO_NOTCH=1"),
                }
                float_on_all_spaces(&win); // asserts joinAll/level 25/hidesOnDeactivate=false + orders front
            }
            None => eprintln!("[spike] no 'notch' window — panel not installed"),
        }
    }

    // Audit fixes: event-driven Space follow (re-show on every desktop/full-screen switch) and the
    // ground-truth [panelstate] diagnostics stream.
    watch_space_changes(app);
    spawn_panel_state_logger(app);

    // Menu-bar residency (overlay spec): a ⚔ tray item with Show/Hide + Quit. Combined with the
    // Accessory policy there is no Dock icon — SHOGUN lives in the menu bar like other overlays.
    {
        use tauri::menu::{Menu, MenuItem};
        use tauri::tray::TrayIconBuilder;
        let items = (
            MenuItem::with_id(app, "toggle", "Show / Hide", true, None::<&str>),
            MenuItem::with_id(app, "quit", "Quit SHOGUN", true, None::<&str>),
        );
        if let (Ok(toggle_i), Ok(quit_i)) = items {
            match Menu::with_items(app, &[&toggle_i, &quit_i]) {
                Ok(menu) => {
                    let mut b = TrayIconBuilder::with_id("shogun-tray").menu(&menu).title("⚔");
                    if let Some(icon) = app.default_window_icon() {
                        b = b.icon(icon.clone());
                    }
                    let built = b
                        .on_menu_event(|app, event| match event.id.as_ref() {
                            "toggle" => toggle_panel(app),
                            "quit" => {
                                eprintln!("[shell] tray quit — exiting");
                                std::process::exit(0);
                            }
                            _ => {}
                        })
                        .build(app);
                    match built {
                        Ok(_tray) => eprintln!("[shell] menu-bar tray installed (⚔)"),
                        Err(e) => eprintln!("[shell] tray install failed: {e}"),
                    }
                }
                Err(e) => eprintln!("[shell] tray menu build failed: {e}"),
            }
        } else {
            eprintln!("[shell] tray menu items failed to build");
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

    // Pin the panel INTO the notch band. Tauri's set_position is clamped below the menu bar
    // (observed: the top edge sat 39pt down — "under the notch" never actually happened), so set
    // the frame directly on the NSWindow: top-centre of its screen, top edge at the true screen
    // top. Level 25 draws over the menu-bar band, i.e. real notch residency.
    if let Some(win) = app.get_webview_window("notch") {
        if let Ok(p) = win.ns_window() {
            if !p.is_null() {
                // SAFETY: live NSWindow on the main thread (setup).
                unsafe { pin_top_centre(p as *mut objc2::runtime::AnyObject) };
                eprintln!("[shell] panel docked top-centre under the notch (visibleFrame top)");
            }
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

/// (main thread) Move the panel to the DISPLAY the mouse cursor is on, pinned top-centre. A window
/// physically lives on ONE display; with 2 displays the panel was stuck on the built-in one and
/// structurally invisible on the other (audit cause #3 — `origin` never changed in [panelstate]).
/// This is a pure reposition (no order in/out), so it never flickers or steals focus.
///
/// SAFETY: caller guarantees `ptr` is the live NSWindow and we're on the main thread.
#[cfg(target_os = "macos")]
unsafe fn reposition_to_cursor_screen(ptr: *mut objc2::runtime::AnyObject) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::{NSPoint, NSRect};
    let mouse: NSPoint = msg_send![class!(NSEvent), mouseLocation];
    let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
    let count: usize = if screens.is_null() { 0 } else { msg_send![screens, count] };
    for i in 0..count {
        let s: *mut AnyObject = msg_send![screens, objectAtIndex: i];
        if s.is_null() {
            continue;
        }
        let f: NSRect = msg_send![s, frame];
        let inside = mouse.x >= f.origin.x
            && mouse.x <= f.origin.x + f.size.width
            && mouse.y >= f.origin.y
            && mouse.y <= f.origin.y + f.size.height;
        if inside {
            // Dock at the TOP-CENTRE of the cursor's display, just under the menu bar/notch
            // (visibleFrame top). The product is a notch UI — the panel lives under the notch,
            // never overlapping it.
            let vf: NSRect = msg_send![s, visibleFrame];
            let w: NSRect = msg_send![ptr, frame];
            let x = vf.origin.x + ((vf.size.width - w.size.width) / 2.0).max(0.0);
            let y = vf.origin.y + (vf.size.height - w.size.height).max(0.0);
            let origin = NSPoint { x, y };
            let _: () = msg_send![ptr, setFrameOrigin: origin];
            break;
        }
    }
}

/// Toggle the overlay (spec: one shortcut/tray click shows it here if hidden or elsewhere, hides
/// it if it's visible on this Space). All NSWindow access on the main thread.
#[cfg(target_os = "macos")]
fn toggle_panel(handle: &tauri::AppHandle) {
    let h = handle.clone();
    let _ = handle.run_on_main_thread(move || {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use tauri::Manager;
        let visible_here = h
            .get_webview_window("notch")
            .and_then(|win| win.ns_window().ok())
            .map(|p| {
                if p.is_null() {
                    return false;
                }
                let ptr = p as *mut AnyObject;
                // SAFETY: main thread, live NSWindow, read-only getters.
                unsafe {
                    let v: bool = msg_send![ptr, isVisible];
                    let a: bool = msg_send![ptr, isOnActiveSpace];
                    v && a
                }
            })
            .unwrap_or(false);
        if visible_here {
            set_panel_hidden(&h);
        } else {
            summon_to_active_space(&h);
            eprintln!("[shell] toggle → shown");
        }
    });
}

/// Hide the overlay until the user summons it again (toggle shortcut / Esc / tray). Sets
/// USER_HIDDEN first so the residency watchers don't instantly re-show it.
#[cfg(target_os = "macos")]
fn set_panel_hidden(handle: &tauri::AppHandle) {
    USER_HIDDEN.store(true, std::sync::atomic::Ordering::Relaxed);
    let h = handle.clone();
    let _ = handle.run_on_main_thread(move || {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use tauri::Manager;
        if let Some(win) = h.get_webview_window("notch") {
            if let Ok(p) = win.ns_window() {
                if !p.is_null() {
                    let ptr = p as *mut AnyObject;
                    let nil: *mut AnyObject = std::ptr::null_mut();
                    // SAFETY: main thread, live NSWindow.
                    unsafe {
                        let _: () = msg_send![ptr, orderOut: nil];
                    }
                }
            }
        }
        eprintln!("[shell] toggle → hidden (summon shortcut or ⚔ tray to show)");
    });
}

/// (main thread) Dock the window at the TOP-CENTRE of ITS screen, just under the menu bar/notch
/// (visibleFrame top — never overlapping the notch).
///
/// SAFETY: caller guarantees `ptr` is the live NSWindow and we're on the main thread.
#[cfg(target_os = "macos")]
unsafe fn pin_top_centre(ptr: *mut objc2::runtime::AnyObject) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::{NSPoint, NSRect};
    let mut screen: *mut AnyObject = msg_send![ptr, screen];
    if screen.is_null() {
        screen = msg_send![class!(NSScreen), mainScreen];
    }
    if screen.is_null() {
        return;
    }
    let vf: NSRect = msg_send![screen, visibleFrame];
    let w: NSRect = msg_send![ptr, frame];
    let x = vf.origin.x + ((vf.size.width - w.size.width) / 2.0).max(0.0);
    let y = vf.origin.y + (vf.size.height - w.size.height).max(0.0);
    let origin = NSPoint { x, y };
    let _: () = msg_send![ptr, setFrameOrigin: origin];
}

/// Re-assert the overlay and re-show the panel on the active Space. Order matters: flags FIRST
/// (a tao demotion to behavior=1 excludes the window from full-screen Spaces, so re-ordering
/// before restoring flags silently fails — the earlier bug), then reposition + re-order only when
/// the panel isn't visible on the active Space (a visible panel is left alone, so a dragged
/// position survives).
/// Debounce for the re-show cycle. On-device evidence: one app switch fires app-activated +
/// space-changed + two deferreds — four orderOut→orderFront cycles back-to-back, each yanking the
/// panel across displays after it had ALREADY joined the new Space (drawn=true flipped back to
/// false every time). We were DoS-ing our own panel; the recipe itself works.
#[cfg(target_os = "macos")]
static REASSERT_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

#[cfg(target_os = "macos")]
fn reassert_panel(handle: &tauri::AppHandle, why: &'static str, defer: bool) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use tauri::Manager;
    // Overlay spec: a panel the USER hid (toggle / Esc / tray) stays hidden — residency must not
    // fight a deliberate hide.
    if USER_HIDDEN.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    // ON-DEVICE GROUND TRUTH: the ⌥J summon path (orderOut → orderFrontRegardless) re-attaches
    // the panel to the current Space 100% of the time, everywhere. The automatic recovery was
    // failing because it did LESS than summon (a bare orderFrontRegardless) and did it LATER
    // (450ms defer + a debounce that swallowed most events). So the automatic path now does
    // exactly what summon does, immediately — plus a `moveToActiveSpace` flip: canJoinAllSpaces
    // is provably not honored reliably for this window, so for the duration of the re-order the
    // collectionBehavior becomes moveToActiveSpace (1<<1) + fullScreenAuxiliary — the API whose
    // documented contract is "move this window to the active Space when ordered front" — then
    // flips back so the panel keeps floating over full-screen apps.
    let nspanel_mode = PANEL_BEHAVIOR.load(std::sync::atomic::Ordering::Relaxed) & 1 != 0;
    if let Some(win) = handle.get_webview_window("notch") {
        if let Ok(p) = win.ns_window() {
            if !p.is_null() {
                let ptr = p as *mut AnyObject;
                // SAFETY: all call sites run on the main thread (workspace notifications and the
                // state-logger's run_on_main_thread closure); live NSWindow; pure AppKit property
                // and ordering calls — no wry/tauri teardown involved.
                unsafe {
                    let want = PANEL_BEHAVIOR.load(std::sync::atomic::Ordering::Relaxed);
                    let _: () = msg_send![ptr, setCollectionBehavior: want];
                    let _: () = msg_send![ptr, setLevel: OVERLAY_LEVEL];
                    let visible: bool = msg_send![ptr, isVisible];
                    let on_active: bool = msg_send![ptr, isOnActiveSpace];
                    if visible && on_active {
                        return; // genuinely on screen here — leave it alone
                    }
                    // Debounce lightly: one app switch fires several notifications back-to-back;
                    // one re-order per burst is enough (and avoids fighting the Space animation).
                    {
                        let mut g = match REASSERT_AT.lock() {
                            Ok(g) => g,
                            Err(_) => return,
                        };
                        if let Some(t) = *g {
                            if t.elapsed() < std::time::Duration::from_millis(300) {
                                return;
                            }
                        }
                        *g = Some(std::time::Instant::now());
                    }
                    if nspanel_mode {
                        // EXACTLY the ⌥J summon sequence — the only recovery proven 100% on this
                        // machine. The two on-device lessons: (a) reposition to the CURSOR's
                        // display first — with two displays ("separate Spaces"), re-ordering on a
                        // display whose Space is inactive does nothing, and this was the entire
                        // difference between summon (works) and the old re-show (didn't);
                        // (b) NO collectionBehavior flip around the re-order — the window server
                        // applies property changes asynchronously, so flip→orderFront→restore in
                        // one tick nullifies the flip before it ever acts.
                        eprintln!("[shell] {why}: re-summoning panel to the cursor's display/space");
                        reposition_to_cursor_screen(ptr);
                        let nil: *mut AnyObject = std::ptr::null_mut();
                        let _: () = msg_send![ptr, orderOut: nil];
                        let _: () = msg_send![ptr, orderFrontRegardless];
                        return;
                    }
                }
            }
        }
    }
    if !nspanel_mode {
        // Plain-window fallback (SHOGUN_NO_NOTCH=1): the window is NOT class-swapped, so destroy
        // is safe, and a Regular plain window truly cannot join another Space — respawn it here.
        // (NEVER destroy/respawn the NSPanel-swapped window: tauri's destroy() throws an ObjC
        // exception through Rust — fatal abort, observed on-device.)
        let _ = defer;
        eprintln!("[shell] {why}: panel not on this space — respawning it here (plain-window mode)");
        respawn_panel(handle);
    }
}

/// Destroy the space-locked panel window and create a fresh one — which macOS guarantees is born
/// on the CURRENTLY ACTIVE Space. TWO separate main-thread ticks: destroying synchronously inside
/// the workspace-notification callback threw an ObjC exception through Rust (fatal abort), and the
/// "notch" label stays taken until the teardown settles — so destroy on one tick, poll until the
/// label frees, then build on a later tick.
#[cfg(target_os = "macos")]
fn respawn_panel(handle: &tauri::AppHandle) {
    use tauri::Manager;
    let h = handle.clone();
    std::thread::spawn(move || {
        // Tick 1: destroy the old window (its own main-loop turn, NOT the notification callback).
        let h2 = h.clone();
        let _ = h.run_on_main_thread(move || {
            if let Some(win) = h2.get_webview_window("notch") {
                let _ = win.destroy();
            }
        });
        // Wait for the label to actually free (teardown is asynchronous).
        for _ in 0..15 {
            std::thread::sleep(std::time::Duration::from_millis(120));
            let (tx, rx) = std::sync::mpsc::channel::<bool>();
            let h2 = h.clone();
            let posted = h.run_on_main_thread(move || {
                let _ = tx.send(h2.get_webview_window("notch").is_none());
            });
            if posted.is_err() {
                return; // app shutting down
            }
            if rx.recv_timeout(std::time::Duration::from_millis(500)).unwrap_or(false) {
                break;
            }
        }
        // Tick 2: build the fresh window on the (now-)active Space.
        let h2 = h.clone();
        let _ = h.run_on_main_thread(move || build_panel_window(&h2));
    });
}

/// (main thread) Build the fresh "notch" window and re-apply the overlay recipe. Skips politely if
/// the label is somehow still taken (next reassert retries).
#[cfg(target_os = "macos")]
fn build_panel_window(handle: &tauri::AppHandle) {
    use tauri::Manager;
    if handle.get_webview_window("notch").is_some() {
        eprintln!("[shell] respawn: old window still present — will retry on the next event");
        return;
    }
    let builder = tauri::WebviewWindowBuilder::new(handle, "notch", tauri::WebviewUrl::default())
        .title("SHOGUN")
        .transparent(true)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .shadow(true)
        .inner_size(640.0, 300.0)
        .visible(true)
        .focused(false);
    match builder.build() {
        Ok(win) => {
            if std::env::var("SHOGUN_NO_NOTCH").is_err() {
                if let Err(e) = panel::install(&win) {
                    eprintln!("[shell] respawn: panel install failed: {e}");
                }
            }
            float_on_all_spaces(&win);
            if let Ok(p) = win.ns_window() {
                if !p.is_null() {
                    // SAFETY: freshly built NSWindow, main thread.
                    unsafe { reposition_to_cursor_screen(p as *mut objc2::runtime::AnyObject) };
                }
            }
            eprintln!("[shell] respawned panel on the active space");
        }
        Err(e) => eprintln!("[shell] respawn failed: {e}"),
    }
}

/// Bring the panel to where the user actually is — the ⌃⌥N "summon" action. Reposition to the
/// cursor's display, then orderOut+orderFrontRegardless re-adds it to the current Space.
#[cfg(target_os = "macos")]
fn summon_to_active_space(app: &tauri::AppHandle) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use tauri::Manager;
    USER_HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
    let Some(win) = app.get_webview_window("notch") else { return };
    let _ = app.run_on_main_thread(move || {
        let Ok(p) = win.ns_window() else { return };
        if p.is_null() {
            return;
        }
        let ptr = p as *mut AnyObject;
        // SAFETY: live NSWindow, on the main thread.
        unsafe {
            reposition_to_cursor_screen(ptr);
            let nil: *mut AnyObject = std::ptr::null_mut();
            let _: () = msg_send![ptr, orderOut: nil];
            let _: () = msg_send![ptr, orderFrontRegardless];
        }
        eprintln!("[shell] ⌃⌥N — summoned panel to the cursor's screen/space");
    });
    // Also tell the webview to EXPAND (un-minimize): ⌃⌥N is the guaranteed re-open path after the
    // panel was collapsed to the handle.
    use tauri::Emitter;
    let _ = app.emit("summon", ());
}

/// Event-driven residency: re-assert the panel on BOTH desktop/full-screen switches
/// (`NSWorkspaceActiveSpaceDidChange`) AND app activations (`NSWorkspaceDidActivateApplication` —
/// clicking another app fires this WITHOUT a space change, and it was exactly the uncovered
/// moment where the panel vanished). Both paths run `reassert_panel`: flags first, then re-show.
#[cfg(target_os = "macos")]
fn watch_space_changes(app: &tauri::App) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;

    // SAFETY: main thread (setup). Observers and blocks are intentionally leaked (app lifetime).
    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            eprintln!("[shell] NSWorkspace nil — watchers not installed");
            return;
        }
        let nc: *mut AnyObject = msg_send![workspace, notificationCenter];
        if nc.is_null() {
            eprintln!("[shell] workspace notificationCenter nil — watchers not installed");
            return;
        }
        for (name_str, why) in [
            ("NSWorkspaceActiveSpaceDidChangeNotification", "space-changed"),
            ("NSWorkspaceDidActivateApplicationNotification", "app-activated"),
        ] {
            let handle = app.handle().clone();
            let name = NSString::from_str(name_str);
            let block = block2::RcBlock::new(move |_notif: *mut AnyObject| {
                reassert_panel(&handle, why, true);
            });
            let nil_obj: *mut AnyObject = std::ptr::null_mut();
            let _obs: *mut AnyObject =
                msg_send![nc, addObserverForName: &*name, object: nil_obj, queue: nil_obj, usingBlock: &*block];
            std::mem::forget(block);
        }
        eprintln!("[shell] space + app-activation watchers installed");
    }
}

/// Ground-truth diagnostics (1s, logs only on change): visibility, Space membership, behavior,
/// level, hidesOnDeactivate, and frame origin. `[panelstate]` lines make the next on-device run
/// tell us definitively WHY the panel isn't where the user expects — no more guessing.
#[cfg(target_os = "macos")]
fn spawn_panel_state_logger(app: &tauri::App) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSRect;
    use std::sync::{Arc, Mutex};
    use tauri::Manager;

    let handle = app.handle().clone();
    let last: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let stuck: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let h = handle.clone();
        let last = last.clone();
        let stuck = stuck.clone();
        let posted = handle.run_on_main_thread(move || {
            let Some(win) = h.get_webview_window("notch") else { return };
            let Ok(p) = win.ns_window() else { return };
            if p.is_null() {
                return;
            }
            let ptr = p as *mut AnyObject;
            // SAFETY: getters + conditional property writes on the live NSWindow, main thread.
            let (s, healthy) = unsafe {
                use objc2::class;
                let visible: bool = msg_send![ptr, isVisible];
                let on_active: bool = msg_send![ptr, isOnActiveSpace];
                let behavior: usize = msg_send![ptr, collectionBehavior];
                let level: isize = msg_send![ptr, level];
                let frame: NSRect = msg_send![ptr, frame];
                // The compositor's OWN verdict: bit 1 of occlusionState = actually drawn on screen.
                // visible=true + occluded=true is the smoking gun for "window fine, pixels absent".
                let occ: usize = msg_send![ptr, occlusionState];
                let drawn = occ & (1 << 1) != 0;
                let alpha: f64 = msg_send![ptr, alphaValue];
                let ns_app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                let app_active: bool =
                    if ns_app.is_null() { false } else { msg_send![ns_app, isActive] };
                // SELF-HEAL: tao re-applies its own stored window flags on focus/resize events,
                // silently demoting the overlay. Re-assert only when wrong; a pure property write
                // with no re-ordering, so it cannot flicker or steal focus.
                let want = PANEL_BEHAVIOR.load(std::sync::atomic::Ordering::Relaxed);
                if level != OVERLAY_LEVEL || behavior & want != want {
                    let _: () = msg_send![ptr, setCollectionBehavior: want];
                    let _: () = msg_send![ptr, setLevel: OVERLAY_LEVEL];
                    eprintln!("[panelstate] healed: level {level}→{OVERLAY_LEVEL} behavior {behavior}→{want}");
                }
                (
                    format!(
                        "visible={visible} drawn={drawn} onActiveSpace={on_active} behavior={behavior} level={level} alpha={alpha:.2} appActive={app_active} origin=({:.0},{:.0})",
                        frame.origin.x, frame.origin.y
                    ),
                    visible && on_active,
                )
            };
            if let Ok(mut g) = last.lock() {
                if *g != s {
                    eprintln!("[panelstate] {s}");
                    *g = s;
                }
            }
            // HEARTBEAT with EXPONENTIAL BACKOFF: a panel that stays off the active Space gets
            // the summon-strength recovery at ticks 2, 4, 8, 16… (capped: every 30s) of a stuck
            // streak — NOT every second. The e448c79 run proved a 1Hz orderOut/orderFront loop is
            // self-defeating (constant blinking, occlusion never settles). Healthy → counter
            // resets and the heartbeat stays silent.
            let fire = {
                let mut g = match stuck.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                if healthy || USER_HIDDEN.load(std::sync::atomic::Ordering::Relaxed) {
                    *g = 0;
                    false
                } else {
                    *g = g.saturating_add(1);
                    let n = *g;
                    n == 2 || n == 4 || n == 8 || n == 16 || (n > 16 && n % 30 == 0)
                }
            };
            if fire {
                reassert_panel(&h, "heartbeat", false);
            }
        });
        if posted.is_err() {
            break; // app shutting down
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

    // Behavior is mode-dependent (PANEL_BEHAVIOR): NSPanel overlay mode wants canJoinAllSpaces
    // (273); the plain-window fallback wants moveToActiveSpace (274) since a Regular window is
    // refused entry to other apps' Spaces anyway.
    let behavior: usize = PANEL_BEHAVIOR.load(std::sync::atomic::Ordering::Relaxed);
    let level: isize = OVERLAY_LEVEL;

    // SAFETY: `ptr` is the live NSWindow owned by Tauri; we message it synchronously on the main
    // thread. The setters take a scalar (NSUInteger / NSInteger) and return void; the getters
    // return the same scalar so we can confirm the value actually stuck (tao/wry may re-apply its
    // own collectionBehavior during startup and silently clobber ours).
    unsafe {
        let _: () = msg_send![ptr, setCollectionBehavior: behavior];
        let _: () = msg_send![ptr, setLevel: level];
        // ROOT-CAUSE FIX (audit): NSPanel defaults to hidesOnDeactivate=YES — the moment the app
        // deactivates (you click any other app / another screen), macOS orders the panel OUT.
        // That's why the panel "worked on this screen but never appeared anywhere else": it wasn't
        // that canJoinAllSpaces was ignored — the panel was being auto-hidden. Must be NO for an
        // always-visible overlay.
        let _: () = msg_send![ptr, setHidesOnDeactivate: false];
        // Belt-and-braces: never let the window server hide this window as part of app-hide.
        let _: () = msg_send![ptr, setCanHide: false];
        // Overlay spec: drag the panel by grabbing anywhere on its background.
        let _: () = msg_send![ptr, setMovableByWindowBackground: true];
        // Accessory (background) apps do NOT auto-show their windows — orderFrontRegardless forces
        // the window visible even while the app is inactive.
        let _: () = msg_send![ptr, orderFrontRegardless];
        let got: usize = msg_send![ptr, collectionBehavior];
        let lvl: isize = msg_send![ptr, level];
        let hides: bool = msg_send![ptr, hidesOnDeactivate];
        eprintln!(
            "[shell] NSWindow behavior set={behavior} readback={got} level={lvl} hidesOnDeactivate={hides}, ordered front"
        );
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

    // Product shortcuts (summon / draft / quit) are USER-REBINDABLE: load persisted bindings
    // (defaults: ⌃⌥N / ⌃⌥G / ⌃⌥Q) and register each; the Settings pane rebids them live via the
    // set_shortcut command.
    let handle = app.handle().clone();
    let binds = shortcuts::load(&handle);
    for (action, combo) in binds.iter() {
        match shortcuts::register_action(&handle, action, combo) {
            Ok(()) => eprintln!("[shell] shortcut {action} = {combo}"),
            Err(e) => eprintln!("[shell] shortcut {action} ({combo}) failed: {e}"),
        }
    }
    app.manage(shortcuts::Store(std::sync::Mutex::new(binds)));
}

/// User-rebindable global shortcuts. Bindings persist in app_data/shortcuts.json (combo strings
/// only — no secrets, Keychain not required) and are re-registered live on change. Combo format is
/// the plugin's string form, e.g. "Control+Alt+KeyN" (the key part is a `KeyboardEvent.code`).
#[cfg(target_os = "macos")]
mod shortcuts {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tauri::Manager;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    pub type Bindings = HashMap<String, String>;
    pub struct Store(pub Mutex<Bindings>);

    const ACTIONS: [&str; 3] = ["summon", "draft", "quit"];

    fn defaults() -> Bindings {
        let mut m = HashMap::new();
        m.insert("summon".into(), "Control+Alt+KeyN".into());
        m.insert("draft".into(), "Control+Alt+KeyG".into());
        m.insert("quit".into(), "Control+Alt+KeyQ".into());
        m
    }

    fn config_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("shortcuts.json"))
    }

    /// Load persisted bindings, filling any missing action with its default.
    pub fn load(app: &tauri::AppHandle) -> Bindings {
        let mut binds = defaults();
        if let Some(p) = config_path(app) {
            if let Ok(text) = std::fs::read_to_string(p) {
                if let Ok(saved) = serde_json::from_str::<Bindings>(&text) {
                    for (k, v) in saved {
                        if ACTIONS.contains(&k.as_str()) {
                            binds.insert(k, v);
                        }
                    }
                }
            }
        }
        binds
    }

    fn save(app: &tauri::AppHandle, binds: &Bindings) {
        let Some(p) = config_path(app) else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_string_pretty(binds) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&p, json) {
                    eprintln!("[shell] shortcuts save failed: {e}");
                }
            }
            Err(e) => eprintln!("[shell] shortcuts serialize failed: {e}"),
        }
    }

    /// Register `combo` for `action`. The combo string parses via the plugin (invalid combos and
    /// already-taken combos surface as Err — nothing changes in that case).
    pub fn register_action(app: &tauri::AppHandle, action: &str, combo: &str) -> Result<(), String> {
        let act = action.to_string();
        app.global_shortcut()
            .on_shortcut(combo, move |app, _sc, event| {
                if event.state() == ShortcutState::Pressed {
                    dispatch(app, &act);
                }
            })
            .map_err(|e| e.to_string())
    }

    fn dispatch(app: &tauri::AppHandle, action: &str) {
        match action {
            "summon" => crate::toggle_panel(app),
            "draft" => {
                if let Some(db) = app.try_state::<shogun_core::daemon::Db>() {
                    crate::inline_source::mac::run_inline_at_cursor(db.inner().clone());
                }
            }
            "quit" => {
                eprintln!("[shell] quit shortcut — exiting");
                std::process::exit(0);
            }
            _ => {}
        }
    }

    /// Current bindings for the Settings UI.
    #[tauri::command]
    pub fn get_shortcuts(store: tauri::State<'_, Store>) -> Bindings {
        store.0.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Hide the overlay (Esc in the webview). Stays hidden until summoned again.
    #[tauri::command]
    pub fn hide_panel(app: tauri::AppHandle) {
        crate::set_panel_hidden(&app);
    }

    /// Rebind `action` to `combo` live: register the new combo first (validates + detects
    /// conflicts), then unregister the old one and persist. On any error nothing changes.
    #[tauri::command]
    pub fn set_shortcut(
        action: String,
        combo: String,
        app: tauri::AppHandle,
        store: tauri::State<'_, Store>,
    ) -> Result<(), String> {
        if !ACTIONS.contains(&action.as_str()) {
            return Err(format!("unknown action: {action}"));
        }
        let old = store.0.lock().ok().and_then(|g| g.get(&action).cloned());
        if old.as_deref() == Some(combo.as_str()) {
            return Ok(());
        }
        register_action(&app, &action, &combo)?;
        if let Some(old) = old {
            if let Err(e) = app.global_shortcut().unregister(old.as_str()) {
                eprintln!("[shell] old shortcut unregister failed ({old}): {e}");
            }
        }
        if let Ok(mut g) = store.0.lock() {
            g.insert(action.clone(), combo.clone());
            save(&app, &g);
        }
        eprintln!("[shell] shortcut {action} → {combo}");
        Ok(())
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
