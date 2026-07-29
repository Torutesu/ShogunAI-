//! SHOGUN Phase 0 notch-UI spike shell.
//!
//! Throwaway per spec §2.1 (only the harness/core crates are carried forward). Module
//! boundaries follow spec §3.11.1; decision logic lives in `shogun_core` (tested on Linux),
//! measurement plumbing in `spike_harness`, and this crate is the macOS adapter layer.
//! `axcache` runs on focus events and must never be triggered by the state machine
//! (the "no collect-on-press" proof, spec §3.10.3).

mod ai_sessions;
mod approvals;
mod axcache;
mod capture_source;
mod connectors;
mod display;
mod dream;
mod exclusions;
mod geometry;
mod hover;
mod inline_source;
mod integrate;
mod notch_actions;
mod notch_exec;

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

/// The NATIVE NSPanel that actually hosts the webview content on screen. The tauri/tao window
/// stays alive but permanently hidden (it owns the wry webview plumbing); its contentView is
/// reparented into this panel at startup. Every ordering/visibility/space operation targets THIS
/// pointer. Set once on the main thread during setup; never freed (app lifetime).
#[cfg(target_os = "macos")]
static NATIVE_PANEL: std::sync::atomic::AtomicPtr<objc2::runtime::AnyObject> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// The NSWindow every overlay operation should target: the native NSPanel when it exists,
/// otherwise the tao window (plain-window fallback / pre-adoption).
#[cfg(target_os = "macos")]
pub(crate) fn overlay_ptr(handle: &tauri::AppHandle) -> Option<*mut objc2::runtime::AnyObject> {
    use tauri::Manager;
    let p = NATIVE_PANEL.load(std::sync::atomic::Ordering::Acquire);
    if !p.is_null() {
        return Some(p);
    }
    let win = handle.get_webview_window("notch")?;
    match win.ns_window() {
        Ok(p) if !p.is_null() => Some(p as *mut objc2::runtime::AnyObject),
        _ => None,
    }
}

/// Tauri entry point. Registers the webview→Rust command half of the closed IPC contract
/// (spec §3.11.2), then in setup: the native overlay NSPanel, geometry read, mouse tap, and the
/// integrated engine + measurement streams.
pub fn run() {
    // ROOT-CAUSE FIX (overlay): become an Accessory (background) app BEFORE AppKit creates any
    // window. The reference overlays that float over every app/Space are Accessory/LSUIElement
    // from process start; SHOGUN used to create its window as a Regular app and flip the policy
    // afterwards — and a window born under Regular policy keeps its original Space binding, which
    // is why canJoinAllSpaces read back correctly yet was never honored. The window itself is no
    // longer declared in tauri.conf.json: it is built in setup, after this line has run.
    #[cfg(target_os = "macos")]
    if std::env::var("SHOGUN_NO_NOTCH").is_err() {
        set_accessory_activation();
    }
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder
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
        inline_source::mac::set_byok_key,
        inline_source::mac::clear_byok_key,
        inline_source::mac::get_llm_settings,
        inline_source::mac::set_llm_settings,
        inline_source::mac::resolve_state_item,
        inline_source::mac::clear_memory,
        shortcuts::get_shortcuts,
        shortcuts::set_shortcut,
        shortcuts::hide_panel,
        onboarding::onboarding_state,
        onboarding::set_onboarding_state,
        ax_permission,
        request_ax_permission,
        exclusions::mac::exclusion_categories,
        connectors::mac::get_draft_stop,
        connectors::mac::set_draft_stop,
        set_panel_size,
        start_panel_drag,
        // First-layer connectors + the L3 send/approval queue, both rendered as sections of the
        // in-panel Settings view (there is no separate settings window).
        connectors::mac::connectors_list,
        connectors::mac::connect_service,
        connectors::mac::disconnect_service,
        connectors::mac::fetch_on_demand,
        approvals::mac::submit_send,
        approvals::mac::draft_reply,
        approvals::mac::list_approvals,
        approvals::mac::confirm_send,
        approvals::mac::reject_send,
        ai_sessions::mac::get_ai_session_import,
        ai_sessions::mac::set_ai_session_import,
        dream::mac::dream_status,
        dream::mac::run_dream_now,
    ]);

    // NOTE: the visible surface is a NATIVE NSPanel hosting the webview's content view
    // (adopt_native_panel). Do not drive the webview from Rust via eval()/on_page_load — wry
    // 0.55.1 panics on the reparented setup; the webview talks to Rust via commands instead.
    builder
        .setup(|_app| {
            #[cfg(target_os = "macos")]
            setup_macos(_app);
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
        eprintln!("[shell] plain window fallback (SHOGUN_NO_NOTCH=1) — desktop-space only");
    } else {
        PANEL_BEHAVIOR.store((1 << 0) | (1 << 8), Ordering::Relaxed); // 257 join-all + fsAux
    }
    // The window is NOT declared in tauri.conf.json — it is built HERE, after the process became
    // an Accessory app at the very top of run(). A window created while the app was still Regular
    // keeps its original Space binding forever (canJoinAllSpaces reads back but is ignored) —
    // the last structural difference between SHOGUN and overlays that float everywhere.
    match app.get_webview_window("notch") {
        Some(win) => {
            // Safety net: if a config-declared window ever reappears, adopt it the same way.
            if std::env::var("SHOGUN_NO_NOTCH").is_err() {
                adopt_native_panel(&win);
            } else {
                let _ = win.show();
                float_on_all_spaces(&win);
            }
        }
        None => build_panel_window(app.handle()),
    }

    // Agent-lane provider settings (provider + model; key stays in the Keychain). MUST load
    // before any fallible early-return below (geometry etc.) — a skipped load silently reverts
    // every chat/draft to the default provider.
    inline_source::mac::init_llm_settings(app.handle());

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
    if let Some(ptr) = overlay_ptr(app.handle()) {
        // SAFETY: live NSWindow/NSPanel on the main thread (setup).
        unsafe { pin_top_centre(ptr) };
        eprintln!("[shell] panel docked top-centre, under the notch on the menu-bar display");
    }

    // What SHOGUN is allowed to read (FR-CAP-05/06). Built here, before the first thread that
    // reads a window: both the capture poller and the AX cache warmer consult it, and the warmer
    // starts inside `integrate::start` below. `exclusions::is_excluded` fails closed until this
    // runs, so an early thread is blind rather than reading a password manager.
    let exclusion_policy: exclusions::mac::SharedPolicy =
        std::sync::Arc::new(std::sync::Mutex::new(exclusions::mac::load(app.handle())));
    exclusions::mac::install(exclusion_policy.clone());

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

    // The product's signature trigger: TAP the Option key alone → draft at the cursor. A bare
    // modifier can't be a global shortcut, so this is an NSEvent global monitor.
    watch_option_tap(app);

    // T-11/T-12 sanity: Accessibility trust + one focused-window walk through the tested
    // policy. Event-driven focus subscription is on-device work (runbook D-03/D-05).
    eprintln!("[spike] accessibility trusted: {}", axcache::ax_trusted());
    // Whatever happens to be focused when SHOGUN launches gets no special exemption — launching
    // while a password manager is frontmost must not read it.
    if let Some(front) = display::frontmost_app() {
        let pid = front.pid;
        let title = axcache::focused_window(pid).and_then(|w| w.title());
        if exclusions::mac::is_excluded(&front.bundle_id, title.as_deref()) {
            eprintln!("[spike] ax snapshot skipped — {} is excluded from reading", front.bundle_id);
        } else if let Some(r) = axcache::snapshot(pid, 250) {
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
            // The reply-context cache is filled by the capture poller (focus path) and read by
            // the draft command, so a press never collects context.
            let reply_cache = shogun_core::daemon::ReplyContextCache::new();
            app.manage(reply_cache.clone());
            let _ = capture_source::spawn_capture_poller(
                db.clone(),
                exclusion_policy,
                None,
                Some(reply_cache),
            );
            eprintln!("[spike] capture source started (poll {}ms)", capture_source::DEFAULT_POLL_MS);

            // AI coding-tool transcripts (opt-in): a large share of the work happens there, and
            // the tools' own session logs carry role/time/session id that screen capture cannot.
            ai_sessions::mac::spawn_importer(app.handle().clone(), db.clone());

            // Embed the backlog in the background (FR-MEM-22: never on the write path, so a slow
            // model cannot delay a capture). A no-op when no model is loaded.
            spawn_embed_job(db.clone());

            // Local state maintenance (the model-free half of the Dream Cycle).
            spawn_maintenance_job(db.clone());

            // The Dream Cycle itself (§6.7): the nightly gate + consolidation. Everything it
            // decides is in shogun-core; this starts the driver that reads idle/power/clock and
            // actually ticks it. Without a Select KK key it runs the local-rule lane — no network.
            let _ = dream::mac::spawn_dream_driver(db.clone());
            // Say it started. The driver is silent by design once running — the gate skips all
            // day without logging — so without this line "working" and "never spawned" look
            // identical for the twenty-odd hours before the window opens.
            eprintln!(
                "[dream] nightly driver started (window {:02}:00–{:02}:00 local, idle + power gated)",
                shogun_core::dreamcycle::schedule::DEFAULT_WINDOW_START_HOUR,
                shogun_core::dreamcycle::schedule::DEFAULT_WINDOW_END_HOUR,
            );

            // First-layer connectors (§6.9). Build the auto-refreshing runtime and start the
            // 15-min read-sync poller. Missing Google creds (env) is not fatal — the app runs
            // without connectors until the user sets them up.
            match connectors::mac::build_runtime(connectors::mac::draft_stop_enabled(app.handle())) {
                Ok(rt) => {
                    let shared = std::sync::Arc::new(std::sync::Mutex::new(rt));
                    connectors::mac::spawn_sync_poller(shared.clone(), db.clone());
                    app.manage(connectors::mac::ConnectorState(shared));
                    // The shared L3 approval queue (producers enqueue sends; the UI confirms them).
                    app.manage(approvals::mac::ApprovalQueueState::default());
                    eprintln!("[spike] connector runtime started (read-sync poller live)");
                }
                Err(e) => eprintln!("[spike] connectors not started: {e}"),
            }
        }
        Err(e) => eprintln!("[spike] memory DB unavailable — capture source not started: {e}"),
    }

    // Last line of setup, and outside the DB branch: whether the panel is on screen has nothing to
    // do with whether memory opened, and a failed DB must not swallow the answer.
    report_panel_health(app.handle());
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
            // (visibleFrame top). The product is a notch UI — the panel hangs from the notch,
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
        let visible_here = overlay_ptr(&h)
            .map(|ptr| {
                // SAFETY: main thread, live NSWindow/NSPanel, read-only getters.
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
        if let Some(ptr) = overlay_ptr(&h) {
            let nil: *mut AnyObject = std::ptr::null_mut();
            // SAFETY: main thread, live NSWindow/NSPanel.
            unsafe {
                let _: () = msg_send![ptr, orderOut: nil];
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

    // Dock to the MENU-BAR display, not whichever one the pointer happened to be on at launch.
    //
    // `NSScreen.screens[0]` is the display that owns the menu bar, and it is the same screen
    // `geometry::read_primary` measures the notch, the hover bands and the idle rect from. Placing
    // the panel anywhere else meant the geometry described one display while the panel sat on
    // another — and, from the outside, meant the overlay appeared on a different screen depending
    // on where the mouse was when the app started. Looking at the wrong monitor is indistinguishable
    // from the UI never coming up.
    //
    // Following the cursor is still the right behaviour for the deliberate "come here" action; that
    // is what ⌥J / `summon_to_active_space` does.
    let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
    let count: usize = if screens.is_null() { 0 } else { msg_send![screens, count] };
    let mut screen: *mut AnyObject = if count > 0 {
        msg_send![screens, objectAtIndex: 0usize]
    } else {
        std::ptr::null_mut()
    };
    if screen.is_null() {
        screen = msg_send![ptr, screen];
    }
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
    // Where it actually landed, and on which screen. With more than one display "I can't see it"
    // is usually "it is on the other one", and that is not answerable without the coordinates.
    eprintln!(
        "[shell] panel docked at {:.0},{:.0} ({:.0}x{:.0}) on the menu-bar display {:.0},{:.0} {:.0}x{:.0}",
        x, y, w.size.width, w.size.height,
        vf.origin.x, vf.origin.y, vf.size.width, vf.size.height
    );
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
fn reassert_panel(handle: &tauri::AppHandle, why: &'static str) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    // Overlay spec: a panel the USER hid (toggle / Esc / tray) stays hidden — residency must not
    // fight a deliberate hide.
    if USER_HIDDEN.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let Some(ptr) = overlay_ptr(handle) else { return };
    // SAFETY: all call sites run on the main thread (workspace notifications and the
    // state-logger's run_on_main_thread closure); live NSWindow/NSPanel; pure AppKit
    // property and ordering calls.
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
        // The proven summon sequence: reposition to the cursor's display, then a full
        // re-order. With the native panel this rarely fires at all.
        eprintln!("[shell] {why}: re-summoning panel to the cursor's display/space");
        reposition_to_cursor_screen(ptr);
        let nil: *mut AnyObject = std::ptr::null_mut();
        let _: () = msg_send![ptr, orderOut: nil];
        let _: () = msg_send![ptr, orderFrontRegardless];
    }
}

/// (main thread) Build the "notch" window and adopt it into the native overlay panel. Skips
/// politely if the label is somehow already taken.
#[cfg(target_os = "macos")]
fn build_panel_window(handle: &tauri::AppHandle) {
    use tauri::Manager;
    if handle.get_webview_window("notch").is_some() {
        eprintln!("[shell] build: window already present — skipping");
        return;
    }
    // ORDER MATTERS: the window is built HIDDEN, converted to an NSPanel, and only THEN shown.
    // The window server classifies a window on its FIRST show — a window first shown as a
    // regular window keeps regular-window Space behavior even after the NSPanel class swap
    // (observed: even a fresh Accessory-born window stayed off the active Space when it was
    // shown before the swap). Panels shown as panels from the start are what the reference
    // overlays do.
    let builder = tauri::WebviewWindowBuilder::new(handle, "notch", tauri::WebviewUrl::default())
        .title("SHOGUN")
        .transparent(true)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .shadow(true)
        .inner_size(640.0, 300.0)
        .visible(false)
        .focused(false);
    match builder.build() {
        Ok(win) => {
            if std::env::var("SHOGUN_NO_NOTCH").is_err() {
                // The REAL overlay architecture: a genuine NSPanel created as a panel from
                // birth, hosting the webview's content view. No class swap, no post-hoc
                // styleMask — the window server sees a true nonactivating panel, the same
                // structure the overlays that work on this machine use.
                adopt_native_panel(&win);
            } else {
                let _ = win.show();
                float_on_all_spaces(&win);
            }
            if let Some(ptr) = overlay_ptr(handle) {
                // SAFETY: main thread (setup / respawn tick), live window/panel.
                unsafe { reposition_to_cursor_screen(ptr) };
            }
            eprintln!("[shell] panel window built on the active space");
        }
        Err(e) => eprintln!("[shell] panel window build failed: {e}"),
    }
}

/// The overlay's NSPanel subclass: `canBecomeKeyWindow` → YES. A borderless window's default is
/// NO, which silently made every text field in the overlay untypeable (chat, shortcut recording,
/// key entry) — clicks landed but the panel never took keystrokes. Registered once; falls back to
/// plain NSPanel if registration fails (typing degraded, overlay still shows).
#[cfg(target_os = "macos")]
fn overlay_panel_class() -> &'static objc2::runtime::AnyClass {
    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
    use objc2::{class, sel};
    use std::sync::OnceLock;
    static CLS: OnceLock<&'static AnyClass> = OnceLock::new();
    CLS.get_or_init(|| {
        extern "C" fn yes(_this: &AnyObject, _sel: Sel) -> Bool {
            Bool::YES
        }
        match ClassBuilder::new(c"ShogunOverlayPanel", class!(NSPanel)) {
            Some(mut b) => {
                // SAFETY: the method signature matches the ObjC declaration (BOOL, no args).
                // The `fn(_, _) -> _` cast lets the compiler pick the concrete lifetime objc2's
                // MethodImplementation impl needs (a fully spelled-out cast is "not general
                // enough" — objc2's documented workaround).
                unsafe {
                    b.add_method(sel!(canBecomeKeyWindow), yes as extern "C" fn(_, _) -> _);
                }
                b.register()
            }
            None => {
                eprintln!("[shell] overlay panel class registration failed — typing may not work");
                class!(NSPanel)
            }
        }
    })
}

/// (main thread) Create a genuine NSPanel (`nonactivatingPanel` styleMask from init) and move the
/// tao window's contentView — which contains the WKWebView — into it. The tao window stays alive
/// and hidden (it owns the wry/IPC plumbing); the panel is what the user sees. This is the
/// structural fix after every flag/ordering/class-swap approach failed on this machine: the
/// window server classifies a window at creation, so only a window BORN as a panel gets true
/// panel Space behavior (all Spaces, over full-screen apps, no focus steal).
/// One shot, a couple of seconds after launch: does the panel actually put pixels on screen?
///
/// "The UI doesn't appear" has half a dozen causes that look identical from outside the process —
/// ordered out, zero alpha, on another Space, hosting an empty view, covered by something — and
/// AppKit will answer all of them in three getters. Printed unconditionally, because a diagnostic
/// behind an environment variable is a diagnostic that does not get run when it is needed.
#[cfg(target_os = "macos")]
fn report_panel_health(app: &tauri::AppHandle) {
    let h = app.clone();
    std::thread::spawn(move || {
        // Late enough that the webview has attached and the compositor has settled.
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let h2 = h.clone();
        let _ = h.run_on_main_thread(move || {
            use objc2::msg_send;
            use objc2::runtime::AnyObject;
            use objc2_foundation::NSRect;
            let Some(ptr) = overlay_ptr(&h2) else {
                eprintln!("[shell] health: no overlay window");
                return;
            };
            // SAFETY: main thread, live window; getters only.
            unsafe {
                let visible: bool = msg_send![ptr, isVisible];
                // Bit 1 of occlusionState is the compositor's own verdict: actually drawn.
                let occ: usize = msg_send![ptr, occlusionState];
                let drawn = occ & (1 << 1) != 0;
                let alpha: f64 = msg_send![ptr, alphaValue];
                let on_active: bool = msg_send![ptr, isOnActiveSpace];
                let frame: NSRect = msg_send![ptr, frame];
                let cv: *mut AnyObject = msg_send![ptr, contentView];
                let subviews: usize = if cv.is_null() {
                    0
                } else {
                    let subs: *mut AnyObject = msg_send![cv, subviews];
                    if subs.is_null() { 0 } else { msg_send![subs, count] }
                };
                eprintln!(
                    "[shell] health: visible={visible} drawn={drawn} alpha={alpha:.2} \
                     onActiveSpace={on_active} frame={:.0},{:.0} {:.0}x{:.0} subviews={subviews}",
                    frame.origin.x, frame.origin.y, frame.size.width, frame.size.height
                );
                // Say what it means, so the next line of the log is the diagnosis rather than data.
                if !visible {
                    eprintln!("[shell] health: ordered out — nothing is on screen. ⌥J summons it.");
                } else if subviews == 0 {
                    eprintln!(
                        "[shell] health: on screen but hosting an empty view — the webview never \
                         moved into the panel, so there is nothing to draw."
                    );
                } else if alpha < 0.05 {
                    eprintln!("[shell] health: transparent (alpha {alpha:.2}).");
                } else if !on_active {
                    eprintln!("[shell] health: on a different Space — ⌥J brings it to this one.");
                } else if !drawn {
                    eprintln!(
                        "[shell] health: on screen with content, but the compositor is not drawing \
                         it — something is covering it."
                    );
                } else {
                    eprintln!("[shell] health: drawing normally at the frame above.");
                }
            }
        });
    });
}

#[cfg(target_os = "macos")]
fn adopt_native_panel(win: &tauri::WebviewWindow) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSRect;

    let tao = match win.ns_window() {
        Ok(p) if !p.is_null() => p as *mut AnyObject,
        _ => {
            eprintln!("[shell] adopt: tao ns_window unavailable — falling back to plain window");
            let _ = win.show();
            float_on_all_spaces(win);
            return;
        }
    };
    // SAFETY: main thread; `tao` is the live (hidden) NSWindow owned by tauri. The panel is
    // created here and intentionally leaked (app lifetime). Manual retain/release pairs keep the
    // content view alive across the reparent.
    unsafe {
        let frame: NSRect = msg_send![tao, frame];
        let alloc: *mut AnyObject = msg_send![overlay_panel_class(), alloc];
        // styleMask: borderless (0) | nonactivatingPanel (1<<7); backing: NSBackingStoreBuffered.
        let style: usize = 1 << 7;
        let panel: *mut AnyObject =
            msg_send![alloc, initWithContentRect: frame, styleMask: style, backing: 2usize, defer: false];
        if panel.is_null() {
            eprintln!("[shell] adopt: NSPanel init failed — falling back to plain window");
            let _ = win.show();
            float_on_all_spaces(win);
            return;
        }
        // Move the webview: retain the content view, give tao an empty placeholder so two
        // windows never share a view, then hand it to the panel.
        let cv: *mut AnyObject = msg_send![tao, contentView];
        let _: () = msg_send![cv, retain];
        let placeholder: *mut AnyObject = msg_send![class!(NSView), new];
        let _: () = msg_send![tao, setContentView: placeholder];
        let _: () = msg_send![placeholder, release];
        let _: () = msg_send![panel, setContentView: cv];
        let _: () = msg_send![cv, release];

        // Did the webview actually come with it? The panel can be perfectly placed, sized, ordered
        // front and still show nothing if the view it hosts is empty — and the webview keeps
        // running either way, so JS-side signals like `interact kind=boot` prove nothing about
        // whether any pixels exist. Unconditional: this is the one fact worth a line at every
        // launch, and it costs two message sends.
        let cv_frame: NSRect = msg_send![cv, frame];
        let subs: *mut AnyObject = msg_send![cv, subviews];
        let n: usize = if subs.is_null() { 0 } else { msg_send![subs, count] };
        eprintln!(
            "[shell] adopt: panel content view {:.0}x{:.0} with {n} subview(s){}",
            cv_frame.size.width,
            cv_frame.size.height,
            if n == 0 { " — EMPTY, nothing will be drawn" } else { "" }
        );

        let _: () = msg_send![panel, setReleasedWhenClosed: false];
        let _: () = msg_send![panel, setOpaque: false];
        let clear: *mut AnyObject = msg_send![class!(NSColor), clearColor];
        let _: () = msg_send![panel, setBackgroundColor: clear];
        let _: () = msg_send![panel, setHasShadow: true];
        let _: () = msg_send![panel, setLevel: OVERLAY_LEVEL];
        let want = PANEL_BEHAVIOR.load(std::sync::atomic::Ordering::Relaxed);
        let _: () = msg_send![panel, setCollectionBehavior: want];
        let _: () = msg_send![panel, setHidesOnDeactivate: false];
        let _: () = msg_send![panel, setCanHide: false];
        let _: () = msg_send![panel, setMovableByWindowBackground: true];
        let _: () = msg_send![panel, setFloatingPanel: true];
        let _: () = msg_send![panel, setBecomesKeyOnlyIfNeeded: true];
        let _: () = msg_send![panel, setWorksWhenModal: true];
        let _: () = msg_send![panel, setAcceptsMouseMovedEvents: true];

        NATIVE_PANEL.store(panel, std::sync::atomic::Ordering::Release);
        reposition_to_cursor_screen(panel);
        let _: () = msg_send![panel, orderFrontRegardless];

        let got: usize = msg_send![panel, collectionBehavior];
        let lvl: isize = msg_send![panel, level];
        let mask: usize = msg_send![panel, styleMask];
        eprintln!(
            "[shell] NATIVE NSPanel hosting the webview — behavior={got} level={lvl} styleMask={mask} (born a panel, no swap)"
        );
    }
}

/// Resize the visible overlay (native panel or fallback window) keeping the TOP edge anchored —
/// Whether SHOGUN is trusted for Accessibility, WITHOUT prompting — the onboarding permission step
/// polls this (a prompting check would reopen the system dialog on every poll). See `axcache`.
#[cfg(target_os = "macos")]
#[tauri::command]
fn ax_permission() -> bool {
    axcache::ax_trusted_silent()
}

/// Ask for Accessibility once from the onboarding button: fire the one-time system prompt and open
/// System Settings at the Accessibility pane (the only route back after the prompt is answered).
#[cfg(target_os = "macos")]
#[tauri::command]
fn request_ax_permission() {
    axcache::request_ax_permission();
}

/// the webview's minimize/expand control. AppKit frames are bottom-left origin, so the y origin
/// shifts by the height delta.
#[cfg(target_os = "macos")]
#[tauri::command]
fn set_panel_size(app: tauri::AppHandle, width: f64, height: f64, anchor: Option<String>) {
    // "left" keeps the top-left corner put (the bottom-right resize grip); anything else — and the
    // default — keeps the panel's centre, which is what a notch-hung panel needs.
    let keep_left = anchor.as_deref() == Some("left");
    let anchor_label = if keep_left { "left" } else { "center" };
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        use objc2::msg_send;
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        use objc2::runtime::AnyObject;
        let Some(ptr) = overlay_ptr(&h) else { return };
        // SAFETY: main thread, live NSWindow/NSPanel.
        unsafe {
            let f: NSRect = msg_send![ptr, frame];
            // Keep the panel where it *looks* like it is. Anchoring the left edge moves the panel
            // sideways by half of every size change: the window is born 640 wide and centred under
            // the notch, then the webview collapses it to the ~260pt pill — which left the pill
            // 190pt to the left of the notch it is supposed to hang from, far enough that it read
            // as "the UI never appeared". The notch is the screen's centre, and a dragged panel's
            // centre is where the user put it, so the centre is the thing that has to hold.
            let mut x = if keep_left {
                f.origin.x
            } else {
                f.origin.x + f.size.width / 2.0 - width / 2.0
            };
            // An expansion near a screen edge slides inward rather than hanging off it.
            let screen: *mut AnyObject = msg_send![ptr, screen];
            if !screen.is_null() {
                let vf: NSRect = msg_send![screen, visibleFrame];
                let max_x = vf.origin.x + (vf.size.width - width).max(0.0);
                x = x.clamp(vf.origin.x, max_x);
            }
            let r = NSRect {
                // Top edge anchored: the panel hangs from the notch, so it grows downward.
                origin: NSPoint { x, y: f.origin.y + f.size.height - height },
                size: NSSize { width, height },
            };
            let _: () = msg_send![ptr, setFrame: r, display: true];
            eprintln!(
                "[shell] panel resized to {:.0}x{:.0} at {:.0},{:.0} (anchor {})",
                width, height, r.origin.x, r.origin.y, anchor_label
            );
        }
    });
}

/// Begin a native window drag of the overlay from the webview's header mouse-down. The tao
/// `startDragging` targets the hidden tao window, so the webview calls this instead — it hands
/// the in-flight mouse event to the native panel.
#[cfg(target_os = "macos")]
#[tauri::command]
fn start_panel_drag(app: tauri::AppHandle) {
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        let Some(ptr) = overlay_ptr(&h) else { return };
        // SAFETY: main thread; standard AppKit calls.
        unsafe {
            let ns_app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            if ns_app.is_null() {
                return;
            }
            let ev: *mut AnyObject = msg_send![ns_app, currentEvent];
            if !ev.is_null() {
                let _: () = msg_send![ptr, performWindowDragWithEvent: ev];
            }
        }
    });
}

/// Bring the panel to where the user actually is — the ⌃⌥N "summon" action. Reposition to the
/// cursor's display, then orderOut+orderFrontRegardless re-adds it to the current Space.
#[cfg(target_os = "macos")]
fn summon_to_active_space(app: &tauri::AppHandle) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    USER_HIDDEN.store(false, std::sync::atomic::Ordering::Relaxed);
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(ptr) = overlay_ptr(&h) else { return };
        // SAFETY: live NSWindow/NSPanel, on the main thread.
        unsafe {
            reposition_to_cursor_screen(ptr);
            let nil: *mut AnyObject = std::ptr::null_mut();
            let _: () = msg_send![ptr, orderOut: nil];
            let _: () = msg_send![ptr, orderFrontRegardless];
        }
        eprintln!("[shell] summon — panel to the cursor's screen/space");
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
                reassert_panel(&handle, why);
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

    let handle = app.handle().clone();
    let last: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let stuck: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    // Diagnostics are opt-in now that the overlay works — the 1s loop itself stays (it drives the
    // self-heal and the heartbeat recovery), but state lines print only with SHOGUN_DEBUG_PANEL=1.
    let debug = std::env::var("SHOGUN_DEBUG_PANEL").is_ok();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let h = handle.clone();
        let last = last.clone();
        let stuck = stuck.clone();
        let posted = handle.run_on_main_thread(move || {
            let Some(ptr) = overlay_ptr(&h) else { return };
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
            if debug {
                if let Ok(mut g) = last.lock() {
                    if *g != s {
                        eprintln!("[panelstate] {s}");
                        *g = s;
                    }
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
                reassert_panel(&h, "heartbeat");
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
        // Diagnostics: styleMask bit 7 (1<<7=128) = nonactivatingPanel; the class must be the
        // swapped NotchPanel for the window server to treat this as a true panel.
        let mask: usize = msg_send![ptr, styleMask];
        let cls: *const objc2::runtime::AnyClass = msg_send![ptr, class];
        let cls_name = if cls.is_null() { "?" } else { (*cls).name().to_str().unwrap_or("?") };
        eprintln!(
            "[shell] NSWindow behavior set={behavior} readback={got} level={lvl} hidesOnDeactivate={hides} styleMask={mask} class={cls_name}, ordered front"
        );
    }
}

/// TAP ⌥ (Option) alone → draft at the cursor. Semantics of a "tap": Option goes down with no
/// other modifier, no other key is pressed while it is held, and it is released within 500ms.
/// That keeps every normal Option use intact — ⌥J summon, ⌥-arrow word nav, ⌥+letter special
/// characters — because any keyDown while Option is held disarms the tap. Uses NSEvent GLOBAL
/// monitors (Accessibility permission, already required for capture); global monitors only see
/// other apps' events, which is exactly the draft target (the focused field over there).
#[cfg(target_os = "macos")]
fn watch_option_tap(app: &tauri::App) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::Instant;

    // State machine for a clean "Option tap":
    //   ARMED   — Option is held after a genuine up→down with no other input.
    //   POISONED— some other key/modifier/mouse happened during THIS hold; the tap is dead until
    //             Option is fully released (so releasing the disqualifier can't re-arm).
    //   OPT_PREV— was Option already down last flagsChanged (to arm only on the real down edge).
    //   DOWN_AT — monotonic start of the hold (Instant, immune to wall-clock steps).
    static ARMED: AtomicBool = AtomicBool::new(false);
    static POISONED: AtomicBool = AtomicBool::new(false);
    static OPT_PREV: AtomicBool = AtomicBool::new(false);
    static DOWN_AT: Mutex<Option<Instant>> = Mutex::new(None);

    const MASK_KEY_DOWN: usize = 1 << 10; // NSEventMaskKeyDown
    const MASK_FLAGS_CHANGED: usize = 1 << 12; // NSEventMaskFlagsChanged
    // Any mouse/scroll/gesture during the hold also disqualifies (⌥-click, ⌥-drag, ⌥-scroll).
    const MASK_MOUSE: usize = (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5) | (1 << 6)
        | (1 << 22) | (1 << 25) | (1 << 26) | (1 << 27) | (1 << 29) | (1 << 30) | (1 << 31);
    const FLAG_OPTION: usize = 1 << 19; // NSEventModifierFlagOption
    // shift | control | command | fn — any of these joining the chord disqualifies the tap.
    const FLAG_OTHERS: usize = (1 << 17) | (1 << 18) | (1 << 20) | (1 << 23);
    const MAX_TAP_MS: u128 = 500;

    /// Any non-Option input during the hold kills the tap until Option is released.
    fn poison() {
        POISONED.store(true, Ordering::Relaxed);
        ARMED.store(false, Ordering::Relaxed);
    }

    // SAFETY: main thread (setup); monitors and blocks are intentionally leaked (app lifetime).
    unsafe {
        let disarm_block = block2::RcBlock::new(move |_ev: *mut AnyObject| poison());
        let key_mon: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_KEY_DOWN,
            handler: &*disarm_block
        ];
        let mouse_mon: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_MOUSE,
            handler: &*disarm_block
        ];
        std::mem::forget(disarm_block);

        let handle = app.handle().clone();
        let flags_block = block2::RcBlock::new(move |ev: *mut AnyObject| {
            if ev.is_null() {
                return;
            }
            let flags: usize = msg_send![ev, modifierFlags];
            let option_down = flags & FLAG_OPTION != 0;
            let others_down = flags & FLAG_OTHERS != 0;
            let opt_prev = OPT_PREV.swap(option_down, Ordering::Relaxed);

            if others_down {
                // A second modifier is part of this chord — poison for the rest of the hold.
                poison();
                return;
            }
            if option_down && !opt_prev {
                // Genuine Option DOWN edge with nothing else held: start a fresh, clean hold.
                POISONED.store(false, Ordering::Relaxed);
                ARMED.store(true, Ordering::Relaxed);
                if let Ok(mut g) = DOWN_AT.lock() {
                    *g = Some(Instant::now());
                }
            } else if !option_down && opt_prev {
                // Option UP edge — fire only on a clean, short, un-poisoned tap.
                let armed = ARMED.swap(false, Ordering::Relaxed);
                let poisoned = POISONED.swap(false, Ordering::Relaxed);
                let held = DOWN_AT.lock().ok().and_then(|g| *g).map(|t| t.elapsed().as_millis());
                if armed && !poisoned && held.is_some_and(|h| h <= MAX_TAP_MS) {
                    eprintln!("[shell] ⌥ tap — draft at cursor");
                    use tauri::Manager;
                    if let Some(db) = handle.try_state::<shogun_core::daemon::Db>() {
                        // The ⌥-tap is the fastest path in the product: read the pack the focus
                        // path already built rather than assembling anything now.
                        let warm = handle
                            .try_state::<shogun_core::daemon::ReplyContextCache>()
                            .and_then(|c| c.current());
                        inline_source::mac::run_inline_at_cursor(db.inner().clone(), warm);
                    }
                }
            }
        });
        let flags_mon: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_FLAGS_CHANGED,
            handler: &*flags_block
        ];
        std::mem::forget(flags_block);

        if key_mon.is_null() || mouse_mon.is_null() || flags_mon.is_null() {
            eprintln!("[shell] ⌥-tap monitor failed to install (accessibility permission?)");
        } else {
            eprintln!("[shell] ⌥ tap-to-draft installed (tap Option alone, <0.5s, no other input)");
        }
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

    // Product shortcuts (summon / quit) are USER-REBINDABLE: load persisted bindings
    // (defaults: ⌃⌥N / ⌃⌥Q) and register each; the Settings pane rebinds them live via the
    // set_shortcut command. Draft is not here — it fires on a bare ⌥ tap (watch_option_tap).
    let handle = app.handle().clone();
    let binds = shortcuts::load(&handle);
    for (action, combo) in binds.iter() {
        match shortcuts::register_action(&handle, action, combo) {
            Ok(()) => eprintln!("[shell] shortcut {action} = {combo}"),
            Err(e) => eprintln!("[shell] shortcut {action} ({combo}) failed: {e}"),
        }
    }
    app.manage(shortcuts::Store(std::sync::Mutex::new(binds)));

    // Onboarding state (issue #6): Rust owns "how far this device got set up" (invariant 1),
    // persisted to app_data/onboarding.json. Load once here so the read command answers from the
    // managed copy without hitting disk on every panel launch.
    let onboarding = onboarding::load(&handle);
    app.manage(onboarding::Store(std::sync::Mutex::new(onboarding)));
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

    // Draft is intentionally NOT here: its ONLY trigger is tapping ⌥ alone (watch_option_tap). A
    // bare modifier can't be a global shortcut, and the previous ⌃⌥G "alternative" only confused
    // (it showed as rerebindable in Settings but the real trigger was the ⌥ tap). Summon and quit
    // stay user-rebindable.
    const ACTIONS: [&str; 2] = ["summon", "quit"];

    fn defaults() -> Bindings {
        let mut m = HashMap::new();
        m.insert("summon".into(), "Control+Alt+KeyN".into());
        m.insert("quit".into(), "Control+Alt+KeyQ".into());
        m
    }

    fn config_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("shortcuts.json"))
    }

    /// On-disk format. `version` lets a default-change migration run ONCE instead of forever —
    /// version 1 was the bare Bindings map (still parsed for backward compat).
    #[derive(serde::Serialize, serde::Deserialize, Default)]
    struct ShortcutsFile {
        #[serde(default)]
        version: u32,
        #[serde(default)]
        binds: Bindings,
    }

    /// The current on-disk version. v2 = the (short-lived) ⌥G draft default; v3 = draft back to
    /// ⌃⌥G; v4 = draft removed entirely (the ⌥ tap is the sole trigger). A v4 load drops any
    /// persisted "draft" binding from disk (ACTIONS no longer contains it, so it's ignored anyway).
    const SHORTCUTS_VERSION: u32 = 4;

    /// Load persisted bindings, filling any missing action with its default.
    pub fn load(app: &tauri::AppHandle) -> Bindings {
        let mut binds = defaults();
        let mut version = 0u32;
        if let Some(p) = config_path(app) {
            if let Ok(text) = std::fs::read_to_string(p) {
                let mut saved: Option<Bindings> = None;
                if let Ok(file) = serde_json::from_str::<ShortcutsFile>(&text) {
                    if !file.binds.is_empty() {
                        version = file.version;
                        saved = Some(file.binds);
                    }
                }
                if saved.is_none() {
                    // Legacy v1: a bare Bindings map.
                    if let Ok(flat) = serde_json::from_str::<Bindings>(&text) {
                        version = 1;
                        saved = Some(flat);
                    }
                }
                if let Some(saved) = saved {
                    for (k, v) in saved {
                        if ACTIONS.contains(&k.as_str()) {
                            binds.insert(k, v);
                        }
                    }
                }
            }
        }
        // One-shot version bump: save() below stamps the current version and writes only ACTIONS
        // entries, so any legacy "draft" binding on disk is dropped (draft is now ⌥-tap only).
        if version < SHORTCUTS_VERSION {
            save(app, &binds);
        }
        binds
    }

    fn save(app: &tauri::AppHandle, binds: &Bindings) {
        let Some(p) = config_path(app) else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let file = ShortcutsFile { version: SHORTCUTS_VERSION, binds: binds.clone() };
        match serde_json::to_string_pretty(&file) {
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

/// First-run onboarding state, owned by Rust (invariant 1) and persisted to
/// `app_data/onboarding.json` — the same JSON-settings shape as `mod shortcuts`, not the DB, since
/// this is a one-time "how far did this device get set up" value, not year-scale memory.
///
/// The flow's single exit is `set_onboarding_state(completed = true)`. A build whose core cannot
/// answer `onboarding_state` reports COMPLETED (see `getOnboardingState` in ipc.ts): showing the
/// flow without being able to record its end would trap the user in it on every launch. The state
/// is exposed to the webview AND, symmetrically (invariant 6), to the agent side via the Memory
/// API — an agent needs to know how far this device is configured.
mod onboarding {
    use std::sync::Mutex;
    use tauri::Manager;

    /// The six steps, in order. Kept in lockstep with `StepId` in
    /// `apps/desktop/src/onboarding/ipc.ts` — that file is the contract's single list.
    const STEPS: [&str; 6] = ["welcome", "reads", "permission", "plan", "connect", "ready"];

    fn first_step() -> String {
        "welcome".into()
    }

    /// In-memory copy of the persisted state, managed so the read command answers without touching
    /// disk on every launch of the panel.
    pub struct Store(pub Mutex<OnboardingState>);

    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct OnboardingState {
        /// True once the user finished (or explicitly skipped to the end).
        #[serde(default)]
        pub completed: bool,
        /// Furthest step reached, so a quit mid-flow resumes there.
        #[serde(default = "first_step")]
        pub step: String,
        /// Which plan the user said they wanted. Billing is a separate flow; this only records the
        /// intent (plan gating itself lives in the Rust core, not here).
        #[serde(default)]
        pub plan: Option<String>,
        /// Unix seconds when the 7-day trial started. Per issue #6 the trial begins at onboarding
        /// COMPLETION, not first launch — so this is stamped the first time `completed` becomes
        /// true and never moved again. Re-running onboarding from Settings sets `completed = false`
        /// but must not restart the clock. Local-only, not a secret, so no Keychain.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub trial_started_at: Option<i64>,
    }

    impl Default for OnboardingState {
        fn default() -> Self {
            Self { completed: false, step: first_step(), plan: None, trial_started_at: None }
        }
    }

    /// Fold a whole-record write from the flow into the next persisted state. Pure so the
    /// trial-start stamp is testable without a real clock — `now_unix` is injected.
    ///
    /// The flow has exactly one writer and always sends the whole record, so there is no partial
    /// update to reconcile; the only derived field is `trial_started_at`, which the caller never
    /// sends.
    pub fn apply(
        prev: &OnboardingState,
        step: String,
        plan: Option<String>,
        completed: bool,
        now_unix: i64,
    ) -> OnboardingState {
        // Once the trial has started it never restarts (reopening from Settings sends
        // completed=false, and losing the stamp would hand a fresh 7 days); otherwise the first
        // write that completes onboarding stamps it.
        let trial_started_at = prev.trial_started_at.or(if completed { Some(now_unix) } else { None });
        let step = if STEPS.contains(&step.as_str()) { step } else { first_step() };
        OnboardingState { completed, step, plan, trial_started_at }
    }

    fn config_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("onboarding.json"))
    }

    /// On-disk format, versioned like `shortcuts.json` so a future default change can migrate once.
    #[derive(serde::Serialize, serde::Deserialize, Default)]
    struct OnboardingFile {
        #[serde(default)]
        version: u32,
        #[serde(default)]
        state: OnboardingState,
    }

    const ONBOARDING_VERSION: u32 = 1;

    /// Load persisted state, defaulting to first-run when the file is absent or unreadable. A
    /// malformed step from disk is normalised back to `welcome` by `apply` on the next write; on
    /// read we tolerate it rather than reset, so a resumed session lands as close as possible.
    pub fn load(app: &tauri::AppHandle) -> OnboardingState {
        let Some(p) = config_path(app) else { return OnboardingState::default() };
        let Ok(text) = std::fs::read_to_string(p) else { return OnboardingState::default() };
        serde_json::from_str::<OnboardingFile>(&text).map(|f| f.state).unwrap_or_default()
    }

    fn save(app: &tauri::AppHandle, state: &OnboardingState) -> Result<(), String> {
        let p = config_path(app).ok_or_else(|| "no app data dir".to_string())?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let file = OnboardingFile { version: ONBOARDING_VERSION, state: state.clone() };
        let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        std::fs::write(&p, json).map_err(|e| format!("onboarding save failed: {e}"))
    }

    fn now_unix() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// Current onboarding state for the flow (invariant 1: Rust owns it). Reads the managed copy,
    /// so the panel does not hit disk on every launch.
    #[tauri::command]
    pub fn onboarding_state(store: tauri::State<'_, Store>) -> OnboardingState {
        store.0.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Whole-record write — the flow has one writer, so a partial update would let a resumed
    /// session disagree with itself. Folds in the derived `trial_started_at`, persists, updates the
    /// managed copy, and mirrors to the Memory API so the agent side sees the same state
    /// (invariant 6).
    #[tauri::command]
    pub fn set_onboarding_state(
        step: String,
        plan: Option<String>,
        completed: bool,
        app: tauri::AppHandle,
        store: tauri::State<'_, Store>,
    ) -> Result<(), String> {
        let prev = store.0.lock().map(|g| g.clone()).unwrap_or_default();
        let next = apply(&prev, step, plan, completed, now_unix());
        save(&app, &next)?;
        if let Ok(mut g) = store.0.lock() {
            *g = next;
        }
        // Symmetry to the agent side (invariant 6) is wired in a follow-up step; the persisted
        // state above is the single source both surfaces read.
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn done(trial: Option<i64>) -> OnboardingState {
            OnboardingState { completed: true, step: "ready".into(), plan: None, trial_started_at: trial }
        }

        #[test]
        fn stamps_trial_at_first_completion() {
            let prev = OnboardingState::default();
            let next = apply(&prev, "ready".into(), Some("pro".into()), true, 1000);
            assert_eq!(next.trial_started_at, Some(1000));
        }

        #[test]
        fn no_trial_before_completion() {
            let prev = OnboardingState::default();
            let next = apply(&prev, "plan".into(), None, false, 1000);
            assert_eq!(next.trial_started_at, None);
        }

        #[test]
        fn completion_is_idempotent() {
            // A second write with completed=true must not re-stamp a later time.
            let next = apply(&done(Some(1000)), "ready".into(), None, true, 2000);
            assert_eq!(next.trial_started_at, Some(1000));
        }

        #[test]
        fn reopening_from_settings_keeps_the_trial() {
            // Settings re-runs onboarding by setting completed=false; the clock must not restart.
            let next = apply(&done(Some(1000)), "welcome".into(), None, false, 2000);
            assert_eq!(next.trial_started_at, Some(1000));
        }

        #[test]
        fn unknown_step_falls_back_to_welcome() {
            let next = apply(&OnboardingState::default(), "bogus".into(), None, false, 0);
            assert_eq!(next.step, "welcome");
        }
    }
}

/// The Keychain account holding the database encryption key (service is the shared SHOGUN one).
#[cfg(target_os = "macos")]
const DB_KEY_ACCOUNT: &str = "memory-db-key";

/// Read the database key from the Keychain, generating and storing one on first run.
///
/// The key lives in the Keychain and nowhere else (invariant 7) — never a file, never a log, and
/// it is not derived from anything guessable. If the Keychain hands back something malformed we
/// refuse rather than silently minting a new key, because a new key would make the existing
/// memory permanently unreadable.
#[cfg(target_os = "macos")]
fn db_key() -> Result<shogun_memory::DbKey, String> {
    const SERVICE: &str = "com.selectkk.shogun";
    match security_framework::passwords::get_generic_password(SERVICE, DB_KEY_ACCOUNT) {
        Ok(bytes) => {
            let hex = String::from_utf8(bytes).map_err(|_| "db key is not valid text".to_string())?;
            shogun_memory::DbKey::from_hex(&hex)
                .ok_or_else(|| "db key in the Keychain is malformed — refusing to replace it".into())
        }
        Err(_) => {
            // First run: mint a key from the OS CSPRNG.
            let mut raw = [0u8; 32];
            getrandom::getrandom(&mut raw).map_err(|e| format!("key generation failed: {e}"))?;
            let key = shogun_memory::DbKey::new(raw);
            security_framework::passwords::set_generic_password(
                SERVICE,
                DB_KEY_ACCOUNT,
                key.to_hex().as_bytes(),
            )
            .map_err(|e| format!("could not store the db key: {e}"))?;
            eprintln!("[spike] memory DB key created and stored in the Keychain");
            Ok(key)
        }
    }
}

/// Open (creating if needed) the on-device memory DB under the app-data dir, with a real
/// wall-clock. macOS-only; the DB is owned by the Rust core (CLAUDE.md invariant 1).
///
/// The database is encrypted at rest. An install that predates encryption still has a plaintext
/// file, so it is converted in place first: the converted copy is written alongside, and only
/// swapped in once it is complete — a failure mid-way leaves the original memory intact.
#[cfg(target_os = "macos")]
fn memory_db(app: &tauri::App) -> Result<shogun_core::daemon::Db, String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("memory.db");
    eprintln!("[spike] memory DB: {}", path.display());
    let key = db_key()?;

    if shogun_memory::is_plaintext_db(&path) {
        eprintln!("[spike] existing plaintext memory DB found — encrypting it");
        let converted = dir.join("memory.db.encrypting");
        let _ = std::fs::remove_file(&converted);
        shogun_memory::encrypt_existing(&path, &converted, &key)
            .map_err(|e| format!("encrypting the existing DB failed: {e}"))?;
        // Keep the plaintext original until the swap succeeds, then remove it.
        let backup = dir.join("memory.db.plaintext-backup");
        std::fs::rename(&path, &backup).map_err(|e| format!("backup failed: {e}"))?;
        match std::fs::rename(&converted, &path) {
            Ok(()) => {
                let _ = std::fs::remove_file(&backup);
                eprintln!("[spike] memory DB encrypted in place");
            }
            Err(e) => {
                // Put the original back — the user's memory must survive a failed upgrade.
                let _ = std::fs::rename(&backup, &path);
                return Err(format!("swapping in the encrypted DB failed: {e}"));
            }
        }
    }

    let clock = std::sync::Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    });
    let db = shogun_core::daemon::Db::open_encrypted(path, &key, clock).map_err(|e| e.to_string())?;
    ensure_ort_dylib(app);
    Ok(attach_embedder(db, embedding_model_paths(app)))
}

/// Run the model-free state maintenance periodically.
///
/// The full Dream Cycle is nightly and gated on idle/power (§6.7); this is the subset that costs
/// almost nothing and that state rots without — decay, corroboration, and overdue/staleness. Run
/// hourly rather than nightly so a commitment does not sit invisible for a day after the evidence
/// that would corroborate it arrives, and once shortly after launch so a fresh start is current.
#[cfg(target_os = "macos")]
fn spawn_maintenance_job(db: shogun_core::daemon::Db) {
    /// Confidence half-life: a month of silence halves a record's confidence (FR-ST-21).
    const HALF_LIFE_MS: i64 = 30 * 24 * 60 * 60 * 1000;
    std::thread::spawn(move || {
        // Let the app finish starting before touching the DB.
        std::thread::sleep(std::time::Duration::from_secs(30));
        loop {
            let now = db.now_ms();
            let r = db.run_local_maintenance(now, HALF_LIFE_MS);
            if r.corroborated > 0 || r.overdue > 0 {
                eprintln!(
                    "[maintenance] {} corroborated, {} newly overdue, {} loops aged, {} decayed",
                    r.corroborated, r.overdue, r.stale, r.decayed
                );
            }
            std::thread::sleep(std::time::Duration::from_secs(60 * 60));
        }
    });
}

/// Drain the embedding backlog on a slow loop.
///
/// Deliberately unhurried and small-batched: this competes with the capture daemon and the UI for
/// cores, and the idle-CPU budget is 5%. Falling behind is fine — an un-embedded event is still
/// found by full-text search (FR-MEM-22), it just isn't matchable by paraphrase yet.
#[cfg(target_os = "macos")]
fn spawn_embed_job(db: shogun_core::daemon::Db) {
    std::thread::spawn(move || loop {
        let n = db.embed_pending(32);
        if n > 0 {
            eprintln!("[embed] embedded {n} event(s)");
            // More waiting: come back promptly but still yield between batches.
            std::thread::sleep(std::time::Duration::from_secs(2));
        } else {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
}

/// Prefer the ONNX Runtime shipped inside the `.app` over anything installed on the machine.
///
/// This is the one location `shogun_memory::embed_onnx` cannot know about — it searches the system
/// prefixes (including `/opt/homebrew/lib`, which a bare `dlopen` misses on Apple Silicon) and
/// fails cleanly when there is nothing there. Setting the variable here just means a packaged app
/// uses its own copy rather than whatever a developer happens to have installed. An explicit
/// `ORT_DYLIB_PATH` still wins, since neither this nor the library overwrites one that is set.
#[cfg(target_os = "macos")]
fn ensure_ort_dylib(app: &tauri::App) {
    use tauri::Manager;
    if std::env::var_os("ORT_DYLIB_PATH").is_some() {
        return;
    }
    let Ok(res) = app.path().resource_dir() else { return };
    // Contents/Frameworks is a sibling of Contents/Resources.
    let bundled = [
        res.parent().map(|c| c.join("Frameworks/libonnxruntime.dylib")),
        Some(res.join("libonnxruntime.dylib")),
    ];
    if let Some(p) = bundled.into_iter().flatten().find(|p| p.exists()) {
        // Set on the setup thread, before the embed job or any model load reads it; `ort` resolves
        // the variable once, at first dlopen.
        std::env::set_var("ORT_DYLIB_PATH", &p);
        eprintln!("[embed] onnx runtime (bundled): {}", p.display());
    }
}

/// Where the bundled embedding model lives. Inside the packaged app it sits in the resource
/// directory; in a dev checkout the env vars point at whatever
/// `scripts/fetch-embedding-model.sh` downloaded.
#[cfg(target_os = "macos")]
fn embedding_model_paths(app: &tauri::App) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    use tauri::Manager;
    if let (Ok(m), Ok(t)) =
        (std::env::var("SHOGUN_EMBED_MODEL"), std::env::var("SHOGUN_EMBED_TOKENIZER"))
    {
        return Some((m.into(), t.into()));
    }
    let dir = app.path().resource_dir().ok()?.join("models/multilingual-e5-small");
    let (m, t) = (dir.join("model.onnx"), dir.join("tokenizer.json"));
    (m.exists() && t.exists()).then_some((m, t))
}

/// Attach the local embedding model if it is present, turning search from lexical into hybrid.
///
/// Absence is normal, not an error: the model is fetched separately and the product is fully
/// usable without it — every result still comes back through full-text search, it just cannot
/// match a paraphrase. A load FAILURE is different and is logged loudly, because that means the
/// model is there but unusable.
#[cfg(target_os = "macos")]
fn attach_embedder(
    db: shogun_core::daemon::Db,
    paths: Option<(std::path::PathBuf, std::path::PathBuf)>,
) -> shogun_core::daemon::Db {
    let Some((model, tokenizer)) = paths else {
        eprintln!("[embed] no local model — search stays lexical (hybrid needs the bundled model)");
        return db;
    };
    match shogun_memory::embed_onnx::OnnxEmbedder::load(&model, &tokenizer) {
        Ok(e) => {
            eprintln!("[embed] local model loaded — hybrid search enabled");
            db.with_embedder(std::sync::Arc::new(e))
        }
        Err(e) => {
            eprintln!("[embed] model present but failed to load ({e}) — search stays lexical");
            db
        }
    }
}

