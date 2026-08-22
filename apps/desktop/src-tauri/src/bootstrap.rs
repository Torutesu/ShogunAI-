//! Tauri startup and webview command registration.

use crate::*;

/// Tauri entry point. Registers the webview→Rust command half of the closed IPC contract
/// (spec §3.11.2), then in setup: the native overlay NSPanel, geometry read, mouse tap, and the
/// integrated engine + measurement streams.
pub fn run() {
    // Activation policy (Regular vs Accessory) is applied in setup_macos from dock_visibility.json
    // before the overlay window is built — see dock_visibility::mac::init.
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder
        .plugin(
            tauri_plugin_autostart::Builder::new()
                // AppleScript registers in System Settings → Login Items (LaunchAgent plist is invisible there).
                .macos_launcher(tauri_plugin_autostart::MacosLauncher::AppleScript)
                .build(),
        )
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
            meeting::mac::meeting_status,
            meeting::mac::meeting_start,
            meeting::mac::meeting_not_now,
            meeting::mac::meeting_stop,
            meeting::mac::meeting_toggle_pause,
            meeting::mac::meeting_save_note,
            meeting::mac::meeting_recap,
            meeting::mac::meeting_recap_minutes,
            meeting::mac::meeting_select_kk_configured,
            #[cfg(target_os = "macos")]
            meeting_live_summary::meeting_request_live_summary,
            meeting::mac::get_deepgram_key_status,
            meeting::mac::set_deepgram_key,
            meeting::mac::clear_deepgram_key,
            meeting::mac::get_meeting_transcript,
            meeting::mac::meeting_exclude_app,
            meeting::mac::meeting_include_app,
            meeting::mac::meeting_drag,
            meeting::mac::meeting_wrapped,
            meeting::mac::get_meeting_settings,
            meeting::mac::set_meeting_enabled,
            meeting::mac::set_meeting_allow_mic_only,
            meeting::mac::get_meeting_microphones,
            meeting::mac::set_meeting_microphone,
            meeting::mac::set_meeting_mode,
            meeting::mac::set_meeting_langs,
            meeting::mac::meeting_overlay_dismiss,
            meeting::mac::meeting_set_overlay_panel,
            meeting::mac::meeting_set_overlay_canvas,
            meeting::mac::meeting_set_overlay_chat,
            meeting::mac::meeting_set_overlay_size,
            meeting::mac::meeting_set_overlay_stealth,
            visual_recall::mac::get_visual_recall_settings,
            visual_recall::mac::set_visual_recall_enabled,
            visual_recall::mac::set_visual_recall_retention,
            visual_recall::mac::get_visual_recall_status,
            visual_recall::mac::list_screen_frames,
            visual_recall::mac::get_screen_frame_image,
            visual_recall::mac::delete_screen_frame,
            visual_recall::mac::open_visual_recall,
            memory_api_settings::mac::memory_api_settings,
            memory_api_settings::mac::set_memory_api_enabled,
            memory_api_settings::mac::set_memory_api_profile,
            memory_api_settings::mac::issue_memory_api_token,
            memory_api_settings::mac::revoke_memory_api_token,
            notch_actions::mac::notch_actions,
            notch_exec::mac::run_notch_action,
            notch_exec::mac::confirm_notch_action,
            search_ui::mac::search_memory,
            metrics::record_ui_slo,
            inline_source::mac::inline_at_cursor,
            scribe::mac::scribe_open,
            scribe::mac::scribe_submit,
            scribe::mac::scribe_status,
            scribe::mac::scribe_close,
            scribe::mac::scribe_cancel,
            inline_source::mac::shogun_status,
            inline_source::mac::shogun_state,
            inline_source::mac::shogun_chat,
            inline_source::mac::shogun_chat_stream,
            inline_source::mac::shogun_chat_cancel,
            inline_source::mac::quit_app,
            inline_source::mac::ui_log,
            inline_source::mac::set_byok_key,
            inline_source::mac::clear_byok_key,
            inline_source::mac::byok_key_last4,
            inline_source::mac::get_llm_settings,
            inline_source::mac::set_llm_settings,
            inline_source::mac::subscription_delegates,
            inline_source::mac::verify_subscription_delegate,
            inline_source::mac::resolve_state_item,
            inline_source::mac::clear_memory,
            inline_source::mac::delete_data_since,
            inline_source::mac::delete_all_and_account,
            shortcuts::get_shortcuts,
            shortcuts::set_shortcut,
            shortcuts::hide_panel,
            castle::get_castle_position,
            castle::set_castle_position,
            set_panel_size,
            // First-layer connectors + the L3 send/approval queue, both rendered as sections of the
            // in-panel Settings view (there is no separate settings window).
            fullui::mac::full_ui_view,
            connectors::mac::connectors_list,
            connect_offer::mac::connect_offer_status,
            connect_offer::mac::connect_offer_not_now,
            connect_offer::mac::connect_offer_never,
            connectors::mac::connect_service,
            connectors::mac::disconnect_service,
            connectors::mac::fetch_on_demand,
            connectors::mac::google_oauth_settings,
            connectors::mac::set_google_oauth_client,
            connectors::mac::clear_google_oauth_client,
            approvals::mac::submit_send,
            approvals::mac::draft_reply,
            approvals::mac::list_approvals,
            approvals::mac::confirm_send,
            approvals::mac::reject_send,
            approvals::mac::set_composio_key,
            approvals::mac::clear_composio_key,
            approvals::mac::set_composio_policy,
            approvals::mac::set_composio_user_id,
            approvals::mac::composio_settings,
            ai_sessions::mac::get_ai_session_import,
            ai_sessions::mac::set_ai_session_import,
            dream::mac::dream_status,
            dream::mac::run_dream_now,
            dream::mac::select_kk_configured,
            dream::mac::set_select_kk_key,
            dream::mac::clear_select_kk_key,
            // First-run onboarding flow (issue #6, superseding the #46 AX guide). Own webview
            // (onboarding.html), opened by setup_macos until the flow has been completed once.
            // State is Rust-owned (invariant 1); the AX check split (silent poll / prompting button)
            // and the accessibility-changed watcher are the #46 assets, kept.
            entitlement::mac::entitlement_status,
            // Stripe billing (issue #8). Status/activation are local; checkout and the portal open
            // Stripe-hosted pages in the system browser — no card UI in this app (FR-BIL-07).
            billing::mac::billing_status,
            billing::mac::billing_activate,
            billing::mac::billing_refresh,
            billing::mac::billing_deactivate,
            billing::mac::billing_open_checkout,
            billing::mac::billing_open_portal,
            onboarding::mac::permission_status,
            onboarding::mac::onboarding_state,
            onboarding::mac::set_onboarding_state,
            onboarding::mac::open_accessibility_settings,
            onboarding::mac::request_microphone_permission,
            onboarding::mac::request_screen_recording_permission,
            permission_drag::arm_permission_app_drag,
            permission_drag::disarm_permission_app_drag,
            startup_health::mac::startup_health,
            onboarding::mac::onboarding_event,
            exclusions::mac::exclusion_categories,
            analytics::analytics_get_opt_out,
            analytics::analytics_set_opt_out,
            sound::mac::get_sound_settings,
            sound::mac::set_sound_pref,
            sound::mac::set_sound_startup,
            sound::mac::set_sound_quiet_hours,
            sound::mac::preview_sound_cue,
            launch_at_login::mac::get_launch_at_login_settings,
            launch_at_login::mac::set_launch_at_login_enabled,
            dock_visibility::mac::get_dock_visible,
            dock_visibility::mac::set_dock_visible,
            notch_status_visibility::get_notch_status_visible,
            notch_status_visibility::set_notch_status_visible,
            voice_session::mac::get_voice_settings,
            voice_session::mac::set_voice_enabled,
            voice_session::mac::set_voice_dictionary_egress_consent,
            voice_session::mac::get_voice_microphones,
            voice_session::mac::set_voice_microphone,
            voice_session::mac::voice_dismiss,
            voice_session::mac::voice_force_end,
            voice_session::mac::get_voice_edit_settings,
            voice_session::mac::set_voice_edit_key,
            voice_session::mac::clear_voice_edit_key,
            voice_session::mac::list_voice_dictionary_terms,
            voice_session::mac::create_voice_dictionary_term,
            voice_session::mac::update_voice_dictionary_term,
            voice_session::mac::delete_voice_dictionary_term,
            // Daily summaries (issue #10): delivery judgement + seen-state + the Evening card data.
            daily_summaries::mac::summary_state,
            daily_summaries::mac::mark_summary_seen,
            daily_summaries::mac::get_daily_summary_settings,
            daily_summaries::mac::set_daily_summary_settings,
            daily_summaries::mac::evening_wrap,
            daily_summaries::mac::morning_card,
            daily_summaries::mac::open_summary_source,
            user_config_watch::get_user_config_status,
            user_config_watch::open_shougun_md,
            user_config_watch::regenerate_shougun_md,
            user_config_watch::list_learned_lessons,
            user_config_watch::set_learned_lesson_active,
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

    // Loud identity banner: if more than one SHOGUN is alive (e.g. a stale bundled "ShogunAI.app"
    // left running from an earlier `open`), the visible panel may be the OLD process
    // while shortcuts hit the new one — which looks exactly like "quit button dead, drag dead".
    // The PID makes that unambiguous in the log.
    eprintln!("========================================================");
    eprintln!(
        "[shell] SHOGUN starting — pid {} — build: plain-window/drag/quit",
        std::process::id()
    );
    eprintln!("========================================================");

    // First thing on screen: the mark folding itself together while everything below this line
    // gets going. Built hidden and closed on its own timer, so a launch never waits on it.
    splash::mac::init(app);

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
    // Castle Position (issue #20): load the user's chosen resting place BEFORE the panel is first
    // adopted/docked, so it lands at the right spot on launch instead of flashing at the notch and
    // jumping. Default (Notch) reproduces the historical top-centre dock.
    castle::init(app.handle());
    // Dock vs menu-bar-only: must run before the overlay window is first built.
    dock_visibility::mac::init(app);

    // The window is NOT declared in tauri.conf.json — it is built HERE, after activation policy
    // is set from the saved preference. A window created while the app was still Regular keeps
    // its original Space binding forever (canJoinAllSpaces reads back but is ignored) —
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

    // One Keychain pass for secrets read during boot (DB, Dream, Composio). BYOK keys load lazily
    // when the user picks a provider — warming them here caused extra prompts for unused keys.
    //
    // Deliberately AFTER the window exists. Nothing before this point reads a secret, and these
    // are four round trips to securityd on the main thread: running them first meant the user
    // watched an empty menu bar for their duration. Now the webview is already loading while they
    // happen, and they still land before the first consumer (`memory_db`, a few lines down).
    shogun_integrations::keychain_store::warm_startup_keychain(&[
        "memory-db-key",
        "select-kk-batch",
        "composio-api-key",
        "deepgram-asr",
        "google-oauth-client-id",
        "google-oauth-client-secret",
    ]);

    // Agent-lane provider settings (provider + model; key stays in the Keychain). MUST load
    // before any fallible early-return below (geometry etc.) — a skipped load silently reverts
    // every chat/draft to the default provider.
    inline_source::mac::init_llm_settings(app.handle());

    launch_at_login::mac::init(app);
    // ~/Shougun.md: load on startup (creating a sample if missing) and watch for changes.
    // The parsed config is held in shared state and feeds directives into the inline generation call.
    let user_cfg = user_config_watch::UserConfigState::default();
    user_config_watch::spawn_user_config_watch(user_cfg.clone());
    app.manage(user_cfg);

    // Audit fixes: event-driven Space follow (re-show on every desktop/full-screen switch) and the
    // ground-truth [panelstate] diagnostics stream.
    watch_space_changes(app);
    spawn_panel_state_logger(app);

    // Menu-bar residency: S-mark tray icon (template silhouette) with Show/Hide + Quit.
    {
        use tauri::menu::{Menu, MenuItem};
        use tauri::tray::TrayIconBuilder;
        let items = (
            MenuItem::with_id(app, "toggle", "Show / Hide", true, None::<&str>),
            MenuItem::with_id(app, "quit", "Quit ShogunAI", true, None::<&str>),
        );
        if let (Ok(toggle_i), Ok(quit_i)) = items {
            match Menu::with_items(app, &[&toggle_i, &quit_i]) {
                Ok(menu) => {
                    // A compiled-in PNG cannot realistically fail to decode, but the shell has
                    // exactly one rule — never panic — and "the menu-bar icon is missing" is a
                    // survivable degradation. Note the failure only skips the TRAY: everything
                    // below in setup (the panel, capture, the DB) still has to run.
                    match tauri::image::Image::from_bytes(include_bytes!(
                        "../icons/tray-icon@2x.png"
                    )) {
                        Ok(tray_icon) => {
                            let built = TrayIconBuilder::with_id("shogun-tray")
                                .menu(&menu)
                                .tooltip("ShogunAI")
                                .icon(tray_icon)
                                .icon_as_template(true)
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
                                Ok(_tray) => eprintln!("[shell] menu-bar tray installed (S mark)"),
                                Err(e) => eprintln!("[shell] tray install failed: {e}"),
                            }
                        }
                        Err(e) => eprintln!("[shell] tray icon unusable ({e}) — no tray this run"),
                    }
                }
                Err(e) => eprintln!("[shell] tray menu build failed: {e}"),
            }
        } else {
            eprintln!("[shell] tray menu items failed to build");
        }
    }

    // Both icons AppKit draws for us fold themselves in, once, and then hand the real artwork
    // back. After the tray block on purpose: the menu-bar half has nothing to animate until the
    // tray it belongs to exists.
    mark_launch::mac::init(app);

    // T-06: geometry (panel screen + CG conversion constants).
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        eprintln!("[spike] setup not on main thread — engine not started");
        return;
    };
    let display_geometries = geometry::mac::read_all(mtm);
    let Some(g) = display_geometries.first() else {
        eprintln!("[spike] no screen — engine not started");
        return;
    };
    eprintln!(
        "[spike] geometry: notch={} notch_w={:.1} notch_h={:.1} menubar_h={:.1} screen={:.0}x{:.0} displays={}",
        g.is_notch, g.notch_w, g.notch_h, g.menubar_h, g.screen.w, g.screen.h, g.display_count
    );

    // Pin the panel INTO the notch band. Tauri's set_position is clamped below the menu bar
    // (observed: the top edge sat 39pt down — "under the notch" never actually happened), so set
    // the frame directly on the NSWindow: top-centre of its screen, top edge at the true screen
    // top. Level mainMenu+3 (27) draws over the menu-bar band — floating (3) does not.
    if let Some(ptr) = overlay_ptr(app.handle()) {
        // SAFETY: live NSWindow/NSPanel on the main thread (setup).
        unsafe { crate::panel::pin_top_centre(ptr) };
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
    // Idle early-reject zone = notch silhouette + 2pt pad (not a full menu-bar strip).
    {
        use geometry::GeometryParams;
        let p = GeometryParams::default();
        hover::set_hover_band_cg(g.idle.h + p.enter_bottom, g.idle.w + 2.0 * p.enter_lr);
        eprintln!(
            "[spike] hover band Idle: {:.0}×{:.0} (visible idle {:.0}×{:.0})",
            g.idle.w + 2.0 * p.enter_lr,
            g.idle.h + p.enter_bottom,
            g.idle.w,
            g.idle.h
        );
    }
    let menubar_min_y = g.screen.max_y() - g.menubar_h;
    let shared = integrate::start(
        app.handle().clone(),
        integrate::StartGeometry {
            regions: g.regions,
            menubar_min_y,
            coordinate_space: shogun_core::notch::engine::DisplayCoordinateSpace::new(
                g.cg_screen,
                g.screen,
            ),
            is_notch: g.is_notch,
            display_count: g.display_count,
            screen: g.screen,
            idle_hit: g.activation,
            // Every display's own notch geometry, so hovering the notch on a second monitor is
            // hit-tested against that monitor rather than against the primary's coordinates.
            per_display: display_geometries
                .into_iter()
                .map(|d| integrate::DisplayGeometry {
                    screen: d.screen,
                    regions: d.regions,
                    menubar_min_y: d.screen.max_y() - d.menubar_h,
                    idle_hit: d.activation,
                    coordinate_space: shogun_core::notch::engine::DisplayCoordinateSpace::new(
                        d.cg_screen,
                        d.screen,
                    ),
                })
                .collect(),
        },
        rx,
    );
    app.manage(shared);
    // Live SLO/grounding samples for the Full UI health pane (in-memory, this run only).
    app.manage(metrics::SloRegister::default());

    // ⌘⇧Space: open the panel directly (statemachine §3.3 Hotkey→Expanded) without depending on
    // hover. Registered here so a flaky CGEventTap can't leave the panel unreachable.
    register_expand_shortcut(app);

    // The product's signature trigger: TAP the Option key alone → draft at the cursor. A bare
    // modifier can't be a global shortcut, so this is an NSEvent global monitor.
    watch_option_tap(app);

    // T-11/T-12 sanity: Accessibility trust + one focused-window walk through the tested
    // policy. Event-driven focus subscription is on-device work (runbook D-03/D-05).
    eprintln!("[spike] accessibility trusted: {}", axcache::ax_trusted());
    // Issue #6: first-run onboarding, Rust-owned state (invariant 1). The managed copy is loaded
    // once here (migrating any legacy #46 disposition file in place) so the read command answers
    // without hitting disk. The flow shows until it has been completed once; a quit mid-flow
    // resumes at the persisted step, and a legacy completed/skipped device is never re-trapped.
    app.manage(onboarding::mac::Store(std::sync::Mutex::new(
        onboarding::mac::load(app.handle()),
    )));
    if onboarding::mac::should_show_onboarding(app.handle()) {
        onboarding::mac::build_onboarding_window(app.handle());
    }
    // Whatever happens to be focused when SHOGUN launches gets no special exemption — launching
    // while a password manager is frontmost must not read it.
    //
    // Off the main thread: this is a diagnostic (it prints a line and drops the result), but it
    // walks a foreign app's AX tree with a 250ms budget, and an unresponsive frontmost app spends
    // all of it. Nothing below waits on the line, so boot shouldn't either. The capture poller
    // reads the same focus a moment later through the same exclusion policy — which is installed
    // above, so this thread cannot outrun it and read something it shouldn't.
    std::thread::spawn(|| {
        let Some(front) = display::frontmost_app() else {
            return;
        };
        let pid = front.pid;
        let title = axcache::focused_window(pid).and_then(|w| w.title());
        if exclusions::mac::is_excluded(&front.bundle_id, title.as_deref()) {
            eprintln!(
                "[spike] ax snapshot skipped — {} is excluded from reading",
                front.bundle_id
            );
        } else if let Some(r) = axcache::snapshot(pid, 250) {
            eprintln!(
                "[spike] ax snapshot: {} bytes, {} elements, depth {}, partial={}",
                r.text_bytes, r.elements_visited, r.depth_reached, r.partial
            );
        }
    });

    // Meeting notes (§6.16). Independent of the memory DB: settings, the offer overlay, and Meet
    // detection must work even when capture cannot start — a corrupt DB must not make the toggle
    // return "not ready" or leave LANE unset (FR-MT-01/02a).
    meeting::mac::init(&app.handle().clone());
    meeting::mac::spawn_meeting_driver(app.handle().clone());

    voice_session::mac::lifecycle::init(app.handle());
    voice_shortcut::install(app.handle());
    // Visual recall summon: left+right of one modifier together (default ⌘⌘). Rebindable via
    // the "recall" binding; bare-modifier gestures can't go through the global-shortcut plugin,
    // so the monitor reads the binding live.
    recall_shortcut::install(app.handle());

    // Cue playback (#49). Before the DB branch below: the one cue that can fire during setup is
    // the capture-stopped failure, and it needs the players already loaded.
    sound::mac::init(app.handle());

    // Licence verification: once at launch, then every 24h (FR-BIL-08). Off the setup thread —
    // a slow or unreachable licence API must never delay the panel coming up.
    billing::mac::spawn_verification_loop(app.handle().clone());

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
            let visual_recall = visual_recall::mac::init(app.handle());
            let _ = capture_source::spawn_capture_poller(
                db.clone(),
                exclusion_policy,
                visual_recall,
                None,
                Some(reply_cache),
            );
            eprintln!(
                "[spike] capture source started (poll {}ms)",
                capture_source::DEFAULT_POLL_MS
            );

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

            install_connectors(app.handle(), Some(db));
        }
        Err(e) => {
            eprintln!("[spike] memory DB unavailable — capture source not started: {e}");
            // Without this the app runs on looking healthy while capture, search and ⌥-tap are
            // all dead, and the only trace is this stderr line nobody sees in a shipped build.
            startup_health::mac::set_memory_db_error(e);
            // …and say it out loud. Capture silently not running is the failure that costs the
            // user a day of memory, which is exactly what the Fail cue is for (#49).
            sound::mac::play(shogun_core::sound::Cue::CaptureStopped);
            // ConnectorState + ApprovalQueueState must exist before any settings command runs.
            // The read-sync poller needs a DB; listing/connecting still works without one.
            install_connectors(app.handle(), None);
        }
    }

    // "Connect this app" offer (#86): state only — ticks ride the meeting driver's 1s frontmost
    // poll, which degrades to a no-op until this state (and ConnectorState, managed above in
    // either branch) exists.
    connect_offer::mac::install(app.handle());

    // --- 匿名プロダクト分析（PostHog, #61）---
    let analytics_handle = app.handle();
    let analytics = crate::analytics::init(analytics_handle);
    {
        let mut p = shogun_core::analytics::Props::new();
        p.insert("cold_start".into(), serde_json::Value::Bool(true));
        analytics.capture("app_opened", p);
    }
    app.manage(analytics);

    // Silent unless the user explicitly asked for a startup sound (#49 D1): SHOGUN is a login
    // item, so launching is something the Mac did, not something the user did.
    sound::mac::play(shogun_core::sound::Cue::AppLaunched);

    // Last line of setup, and outside the DB branch: whether the panel is on screen has nothing to
    // do with whether memory opened, and a failed DB must not swallow the answer.
    report_panel_health(app.handle());
}
