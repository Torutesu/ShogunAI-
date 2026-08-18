//! SHOGUN Phase 0 notch-UI spike shell.
//!
//! Throwaway per spec §2.1 (only the harness/core crates are carried forward). Module
//! boundaries follow spec §3.11.1; decision logic lives in `shogun_core` (tested on Linux),
//! measurement plumbing in `spike_harness`, and this crate is the macOS adapter layer.
//! `axcache` runs on focus events and must never be triggered by the state machine
//! (the "no collect-on-press" proof, spec §3.10.3).

mod ai_sessions;
mod approvals;
mod audio_lane;
pub mod axcache;
// Turbo model fetch resolves the on-device path and delegates the HTTPS download to shogun-core's
// traced egress (model_asset). On-device only, matching where the audio lane lives.
mod analytics;
mod billing;
mod capture_source;
mod connectors;
mod daily_summaries;
pub mod display;
mod dock_visibility;
mod dream;
mod entitlement;
mod exclusions;
mod fullui;
mod geometry;
mod hover;
mod inline_source;
mod integrate;
mod launch_at_login;
pub mod meeting;
#[cfg(target_os = "macos")]
mod meeting_live_summary;
mod meeting_recap;
#[cfg(target_os = "macos")]
mod meeting_translate;
#[cfg(target_os = "macos")]
mod memory_api_settings;
mod metrics;
mod mic;
#[cfg(target_os = "macos")]
mod model_fetch;
mod net_lane;
mod notch_actions;
mod notch_exec;
mod notch_status_visibility;
mod onboarding;
#[cfg(target_os = "macos")]
mod permission_drag;
#[cfg(target_os = "macos")]
mod recall_shortcut;
#[cfg(all(target_os = "macos", feature = "visual-recall-ocr"))]
mod screen_ocr;
mod scribe;
mod search_ui;
/// UI cue playback and the silence rules around it (#49, docs/sound-design.md).
#[cfg(target_os = "macos")]
mod sound;
mod startup_health;
mod user_config_watch;
mod visual_recall;
mod voice_editor;
#[cfg(target_os = "macos")]
mod voice_lane;
#[cfg(target_os = "macos")]
mod voice_session;
#[cfg(target_os = "macos")]
mod voice_shortcut;

/// The collectionBehavior the overlay wants, selected at setup (NSPanel mode = canJoinAllSpaces +
/// fullScreenAuxiliary = 257; plain-window fallback = moveToActiveSpace 274) and re-asserted by
/// every heal/reassert path. `stationary` (1<<4) was dropped: it is a suspect for the panel not
/// tracking Space switches on this machine, and the reference overlays run without it.
#[cfg(target_os = "macos")]
static PANEL_BEHAVIOR: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new((1 << 0) | (1 << 8));

/// The notch is a docked surface, not a floating utility window. Keep this shared so adoption and
/// recovery cannot accidentally disagree and re-enable background dragging.
#[cfg(target_os = "macos")]
const PANEL_MOVABLE: bool = false;

/// NSMainMenuWindowLevel (24) + 3 — same as boring.notch (`level = .mainMenu + 3`).
///
/// Floating (3) sits UNDER the menu bar. Frame can be flush to `screen.frame.maxY` and Idle
/// still paints as a gap under the notch because the menu bar occludes the top band. Level 27
/// draws in the notch/menu-bar region so the welded chin is actually visible there.
#[cfg(target_os = "macos")]
pub(crate) const OVERLAY_LEVEL: isize = 24 + 3;
/// Window label for the Full UI (spec §D). Shared by the builder and the open path so the
/// "already open → focus it" check can't drift from the label the window was built with.
pub(crate) const FULL_UI_LABEL: &str = "fullui";
/// Window label for the Visual recall browse UI (saved screen timeline).
pub(crate) const VISUAL_RECALL_LABEL: &str = "visual-recall";
#[cfg(target_os = "macos")]
const SCRIBE_LABEL: &str = "scribe";
#[cfg(target_os = "macos")]
const SCRIBE_MIN_W: f64 = 320.0;
#[cfg(target_os = "macos")]
const SCRIBE_MAX_W: f64 = 760.0;
#[cfg(target_os = "macos")]
const SCRIBE_H: f64 = 56.0;
/// Full UI window size, in LOGICAL points. The minimum is the spec §D floor — below it the
/// sidebar plus a three-card health row stops fitting.
const FULL_UI_W: f64 = 1200.0;
const FULL_UI_H: f64 = 820.0;
const FULL_UI_MIN_W: f64 = 1040.0;
const FULL_UI_MIN_H: f64 = 720.0;
const VISUAL_RECALL_W: f64 = 720.0;
const VISUAL_RECALL_H: f64 = 640.0;
const VISUAL_RECALL_MIN_W: f64 = 480.0;
const VISUAL_RECALL_MIN_H: f64 = 400.0;
/// Open notch panel resize ceiling — must match `PANEL_MAX_SCREEN_FRAC` in App.tsx.
const PANEL_MAX_SCREEN_FRAC: f64 = 0.75;

/// True while the USER hid the overlay (toggle shortcut / Esc / tray). The auto-residency
/// machinery (watchers, heal, respawn) must respect this — a deliberately hidden panel stays
/// hidden until summoned again.
#[cfg(target_os = "macos")]
static USER_HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The user's chosen Castle Position (issue #20) — where the panel resides and expands from —
/// encoded as `CastlePosition::to_u8`. Kept as a lock-free atomic because every placement path
/// (`reposition_to_cursor_screen`, `pin_top_centre`, `set_panel_size`) reads it on the main thread
/// and must not block. Persisted separately to `castle.json` via the `castle` module; the default
/// (0 = Notch) matches the spike's original top-centre dock.
#[cfg(target_os = "macos")]
static CASTLE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The current Castle Position read from the atomic (default `Notch`).
#[cfg(target_os = "macos")]
fn current_castle() -> shogun_core::notch::geometry::CastlePosition {
    shogun_core::notch::geometry::CastlePosition::from_u8(
        CASTLE.load(std::sync::atomic::Ordering::Relaxed),
    )
}

/// Legacy user-dragged override. New builds never populate it; `castle::init` clears old persisted
/// values so every placement path resolves to the selected Castle Position.
#[cfg(target_os = "macos")]
static DRAG_OVERRIDE: std::sync::Mutex<Option<shogun_core::notch::geometry::DragOffset>> =
    std::sync::Mutex::new(None);

/// True while our code is moving the panel. Retained as the single movement boundary for the
/// dock/resize paths even though user-driven panel movement is disabled.
#[cfg(target_os = "macos")]
static PROGRAMMATIC_MOVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The drag override, if any (poisoned lock reads as "no override" — never panic in the shell).
#[cfg(target_os = "macos")]
fn current_drag_override() -> Option<shogun_core::notch::geometry::DragOffset> {
    DRAG_OVERRIDE.lock().ok().and_then(|g| *g)
}

/// (main thread) Where the panel rests on `screen`: the user's dragged spot when one exists
/// (issue #21), else the Castle Position dock (issue #20; Notch welds under the hardware notch).
/// Both paths clamp on-screen, so a display change can only pull the panel back on screen.
///
/// SAFETY: `screen` must be a live `NSScreen*`; called on the main thread.
#[cfg(target_os = "macos")]
unsafe fn resting_dock_origin(
    screen: *mut objc2::runtime::AnyObject,
    width: f64,
    height: f64,
) -> objc2_foundation::NSPoint {
    use objc2::msg_send;
    use objc2_foundation::{NSPoint, NSRect};
    use shogun_core::notch::geometry::{drag_origin, Rect as GRect};
    match current_drag_override() {
        Some(off) => {
            // SAFETY: same contract as the enclosing fn — live NSScreen*, main thread.
            let vf: NSRect = unsafe { msg_send![screen, visibleFrame] };
            let vis = GRect::new(vf.origin.x, vf.origin.y, vf.size.width, vf.size.height);
            let o = drag_origin(vis, width, height, off);
            NSPoint { x: o.x, y: o.y }
        }
        // SAFETY: same contract as the enclosing fn — live NSScreen*, main thread.
        None => unsafe { castle_dock_origin(screen, width, height, current_castle()) },
    }
}

/// Run `f` inside the programmatic-movement boundary. Main thread only.
///
/// The flag is restored through a `Drop` guard that puts back the PREVIOUS value, which buys two
/// things a plain store-true/store-false could not: a panic inside `f` cannot leave the flag
/// latched, and a nested call cannot clear it on the inner exit while the outer move is active.
#[cfg(target_os = "macos")]
fn with_programmatic_move<R>(f: impl FnOnce() -> R) -> R {
    use std::sync::atomic::Ordering;

    struct Guard(bool);
    impl Drop for Guard {
        fn drop(&mut self) {
            PROGRAMMATIC_MOVE.store(self.0, Ordering::Relaxed);
        }
    }

    let _guard = Guard(PROGRAMMATIC_MOVE.swap(true, Ordering::Relaxed));
    f()
}

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
            meeting::mac::set_meeting_mode,
            meeting::mac::set_meeting_langs,
            meeting::mac::meeting_overlay_dismiss,
            meeting::mac::meeting_set_overlay_panel,
            meeting::mac::meeting_set_overlay_canvas,
            meeting::mac::meeting_set_overlay_chat,
            meeting::mac::meeting_set_overlay_size,
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
            voice_session::mac::voice_dismiss,
            voice_session::mac::voice_force_end,
            voice_session::mac::get_voice_edit_settings,
            voice_session::mac::set_voice_edit_key,
            voice_session::mac::clear_voice_edit_key,
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

    voice_session::mac::init(app.handle());
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

/// (main thread) Dock origin for castle placement. Notch welds to the physical screen top
/// (full `frame`); other castles use `visibleFrame` so they clear the menu bar / Dock.
///
/// SAFETY: `screen` must be a live `NSScreen*`; called on the main thread.
#[cfg(target_os = "macos")]
unsafe fn castle_dock_origin(
    screen: *mut objc2::runtime::AnyObject,
    width: f64,
    height: f64,
    pos: shogun_core::notch::geometry::CastlePosition,
) -> objc2_foundation::NSPoint {
    use objc2::msg_send;
    use objc2_foundation::{NSPoint, NSRect};
    use shogun_core::notch::geometry::{castle_dock_frame, castle_origin, Rect as GRect};
    let f: NSRect = msg_send![screen, frame];
    let vf: NSRect = msg_send![screen, visibleFrame];
    let screen_r = GRect::new(f.origin.x, f.origin.y, f.size.width, f.size.height);
    let vis_r = GRect::new(vf.origin.x, vf.origin.y, vf.size.width, vf.size.height);
    let dock = castle_dock_frame(screen_r, vis_r, pos);
    let o = castle_origin(dock, width, height, pos);
    NSPoint { x: o.x, y: o.y }
}

/// Clamp a proposed frame into the castle dock rect (full screen for Notch, visible otherwise).
///
/// SAFETY: `screen` must be a live `NSScreen*`; called on the main thread.
#[cfg(target_os = "macos")]
unsafe fn clamp_to_castle_dock(
    screen: *mut objc2::runtime::AnyObject,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    pos: shogun_core::notch::geometry::CastlePosition,
) -> (f64, f64) {
    use objc2::msg_send;
    use objc2_foundation::NSRect;
    use shogun_core::notch::geometry::{castle_dock_frame, Rect as GRect};
    let f: NSRect = msg_send![screen, frame];
    let vf: NSRect = msg_send![screen, visibleFrame];
    let screen_r = GRect::new(f.origin.x, f.origin.y, f.size.width, f.size.height);
    let vis_r = GRect::new(vf.origin.x, vf.origin.y, vf.size.width, vf.size.height);
    let dock = castle_dock_frame(screen_r, vis_r, pos);
    let max_x = dock.x + (dock.w - width).max(0.0);
    let max_y = dock.y + (dock.h - height).max(0.0);
    (x.clamp(dock.x, max_x), y.clamp(dock.y, max_y))
}

/// Clamp a proposed frame into `screen`'s visible frame — the bound for a user-dragged position,
/// which belongs to the screen rather than to any castle's dock band.
///
/// SAFETY: `screen` must be a live `NSScreen*`; called on the main thread.
#[cfg(target_os = "macos")]
unsafe fn clamp_to_visible_frame(
    screen: *mut objc2::runtime::AnyObject,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> (f64, f64) {
    use objc2::msg_send;
    use objc2_foundation::NSRect;
    let vf: NSRect = msg_send![screen, visibleFrame];
    let max_x = vf.origin.x + (vf.size.width - width).max(0.0);
    let max_y = vf.origin.y + (vf.size.height - height).max(0.0);
    (x.clamp(vf.origin.x, max_x), y.clamp(vf.origin.y, max_y))
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
    let count: usize = if screens.is_null() {
        0
    } else {
        msg_send![screens, count]
    };
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
            // Dock at the user's resting place on the cursor's display: the dragged spot when one
            // exists (issue #21), else the Castle Position (issue #20). Notch welds to the
            // physical screen top; edge/corner positions rest on the visible frame.
            let w: NSRect = msg_send![ptr, frame];
            let origin = unsafe { resting_dock_origin(s, w.size.width, w.size.height) };
            with_programmatic_move(|| {
                // SAFETY: same contract as the enclosing fn (main thread, live panel); the
                // closure needs its own block because it doesn't inherit the unsafe context.
                unsafe {
                    let _: () = msg_send![ptr, setFrameOrigin: origin];
                }
            });
            break;
        }
    }
}

/// True when the panel is currently sitting on the display the cursor is on.
///
/// # Safety
/// `ptr` must be a live `NSWindow`/`NSPanel`, called on the main thread.
#[cfg(target_os = "macos")]
unsafe fn panel_is_on_cursor_screen(ptr: *mut objc2::runtime::AnyObject) -> bool {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::{NSPoint, NSRect};
    let mouse: NSPoint = msg_send![class!(NSEvent), mouseLocation];
    let w: NSRect = msg_send![ptr, frame];
    // Compare by the panel's own centre: it hangs from the top of one display, so its midpoint is
    // unambiguously on that display even when displays are adjacent.
    let cx = w.origin.x + w.size.width / 2.0;
    let cy = w.origin.y + w.size.height / 2.0;
    let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
    let count: usize = if screens.is_null() {
        0
    } else {
        msg_send![screens, count]
    };
    for i in 0..count {
        let sc: *mut AnyObject = msg_send![screens, objectAtIndex: i];
        if sc.is_null() {
            continue;
        }
        let f: NSRect = msg_send![sc, frame];
        let has = |x: f64, y: f64| {
            x >= f.origin.x
                && x <= f.origin.x + f.size.width
                && y >= f.origin.y
                && y <= f.origin.y + f.size.height
        };
        if has(mouse.x, mouse.y) {
            return has(cx, cy);
        }
    }
    // Cursor on no known screen — treat as "same" so the toggle still hides rather than doing
    // nothing visible.
    true
}

/// Move the panel to the display the cursor is on, if it isn't there already.
///
/// Cheap and idempotent: called on every hover-open, so it checks before it moves rather than
/// re-laying-out the window on each transition.
#[cfg(target_os = "macos")]
pub(crate) fn move_panel_to_cursor_screen(app: &tauri::AppHandle) {
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(ptr) = overlay_ptr(&h) else { return };
        // SAFETY: main thread, live NSWindow/NSPanel.
        unsafe {
            if panel_is_on_cursor_screen(ptr) {
                return;
            }
            reposition_to_cursor_screen(ptr);
        }
        eprintln!("[shell] panel followed the cursor to another display");
    });
}

/// Toggle the overlay: hide it when it is already in front of you, otherwise bring it here.
///
/// "In front of you" means the cursor's DISPLAY, not merely the active Space. With two monitors
/// the panel on display 1 is still on the active Space while you are working on display 2, so an
/// active-Space test made the shortcut hide the panel instead of moving it — the panel could never
/// be summoned to the second screen. All NSWindow access on the main thread.
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
                    v && a && panel_is_on_cursor_screen(ptr)
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
        eprintln!("[shell] toggle → hidden (summon shortcut or menu-bar tray to show)");
    });
}

/// (main thread) Dock the window at the user's resting place on the MENU-BAR display: the dragged
/// spot when one exists (issue #21), else the Castle Position (issue #20). Notch welds to the
/// physical screen top (full frame — behind/under the hardware notch); edge and corner castles
/// rest on the visible frame so they clear the menu bar / Dock.
///
/// SAFETY: caller guarantees `ptr` is the live NSWindow and we're on the main thread.
#[cfg(target_os = "macos")]
unsafe fn pin_top_centre(ptr: *mut objc2::runtime::AnyObject) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSRect;

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
    let count: usize = if screens.is_null() {
        0
    } else {
        msg_send![screens, count]
    };
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
    let w: NSRect = msg_send![ptr, frame];
    let origin = unsafe { resting_dock_origin(screen, w.size.width, w.size.height) };
    with_programmatic_move(|| {
        // SAFETY: same contract as the enclosing fn (main thread, live panel); the closure
        // needs its own block because it doesn't inherit the unsafe context.
        unsafe {
            let _: () = msg_send![ptr, setFrameOrigin: origin];
        }
    });
    // Which anchor, where it actually landed, and on which screen. With more than one display
    // "I can't see it" is usually "it is on the other one", and that is not answerable without
    // the coordinates.
    let anchor = if current_drag_override().is_some() {
        "drag"
    } else {
        current_castle().key()
    };
    let f: NSRect = msg_send![screen, frame];
    eprintln!(
        "[shell] panel docked ({}) at {:.0},{:.0} ({:.0}x{:.0}) on the menu-bar display {:.0},{:.0} {:.0}x{:.0}",
        anchor, origin.x, origin.y, w.size.width, w.size.height,
        f.origin.x, f.origin.y, f.size.width, f.size.height
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

/// AppKit's visibility and Space membership do not prove that the WindowServer is rendering the
/// panel. A cold launch can report both while its compositor surface is absent; only a panel that
/// is visible, on the active Space, and drawn is safe to leave unreordered.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct PanelPresentation {
    visible: bool,
    on_active_space: bool,
    drawn: bool,
}

#[cfg(target_os = "macos")]
fn panel_needs_reorder(presentation: PanelPresentation) -> bool {
    !presentation.visible || !presentation.on_active_space || !presentation.drawn
}

#[cfg(all(test, target_os = "macos"))]
mod panel_recovery_tests {
    use super::{panel_needs_reorder, PanelPresentation, PANEL_MOVABLE};

    #[test]
    fn notch_panel_is_never_user_movable() {
        assert!(!PANEL_MOVABLE);
    }

    #[test]
    fn undrawn_panel_needs_reorder_even_when_visible_on_active_space() {
        assert!(panel_needs_reorder(PanelPresentation {
            visible: true,
            on_active_space: true,
            drawn: false,
        }));
    }

    #[test]
    fn invisible_panel_needs_reorder() {
        assert!(panel_needs_reorder(PanelPresentation {
            visible: false,
            on_active_space: true,
            drawn: true,
        }));
    }

    #[test]
    fn panel_off_active_space_needs_reorder() {
        assert!(panel_needs_reorder(PanelPresentation {
            visible: true,
            on_active_space: false,
            drawn: true,
        }));
    }

    #[test]
    fn drawn_visible_panel_on_active_space_is_healthy() {
        assert!(!panel_needs_reorder(PanelPresentation {
            visible: true,
            on_active_space: true,
            drawn: true,
        }));
    }
}

#[cfg(target_os = "macos")]
fn reassert_panel(handle: &tauri::AppHandle, why: &'static str) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    // Overlay spec: a panel the USER hid (toggle / Esc / tray) stays hidden — residency must not
    // fight a deliberate hide.
    if USER_HIDDEN.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let Some(ptr) = overlay_ptr(handle) else {
        return;
    };
    // SAFETY: all call sites marshal through `run_on_main_thread`; live NSWindow/NSPanel; pure
    // AppKit property and ordering calls.
    unsafe {
        let want = PANEL_BEHAVIOR.load(std::sync::atomic::Ordering::Relaxed);
        let behavior: usize = msg_send![ptr, collectionBehavior];
        let level: isize = msg_send![ptr, level];
        let hides_on_deactivate: bool = msg_send![ptr, hidesOnDeactivate];
        let can_hide: bool = msg_send![ptr, canHide];
        if behavior != want {
            let _: () = msg_send![ptr, setCollectionBehavior: want];
        }
        if level != OVERLAY_LEVEL {
            let _: () = msg_send![ptr, setLevel: OVERLAY_LEVEL];
        }
        if hides_on_deactivate {
            let _: () = msg_send![ptr, setHidesOnDeactivate: false];
        }
        if can_hide {
            let _: () = msg_send![ptr, setCanHide: false];
        }
        let _: () = msg_send![ptr, setMovable: PANEL_MOVABLE];
        let _: () = msg_send![ptr, setMovableByWindowBackground: PANEL_MOVABLE];
        let visible: bool = msg_send![ptr, isVisible];
        let on_active_space: bool = msg_send![ptr, isOnActiveSpace];
        let occlusion: usize = msg_send![ptr, occlusionState];
        let drawn = occlusion & (1 << 1) != 0;
        if !panel_needs_reorder(PanelPresentation {
            visible,
            on_active_space,
            drawn,
        }) {
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
        .title("ShogunAI")
        .transparent(true)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        // Native shadow under the bezel reads as a floating gap (boring.notch: hasShadow=false).
        .shadow(false)
        .inner_size(640.0, 300.0)
        .visible(false)
        .focused(false)
        .title_bar_style(tauri::TitleBarStyle::Overlay);
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

/// Build Scribe beside the AX field captured before Shogun takes focus. This is a regular focused
/// window, never the notch's non-activating NSPanel, so the two lifecycles remain independent.
#[cfg(target_os = "macos")]
fn build_scribe_window(
    handle: &tauri::AppHandle,
    opened: scribe::mac::ScribeOpenResult,
) -> Result<(), String> {
    use tauri::Manager;

    if let Some(existing) = handle.get_webview_window(SCRIBE_LABEL) {
        let _ = existing.close();
    }
    let width = opened
        .anchor
        .map(|anchor| anchor.width.clamp(SCRIBE_MIN_W, SCRIBE_MAX_W))
        .unwrap_or(520.0);
    let url = format!("index.html?view=scribe&session={}", opened.session_id);
    let mut builder =
        tauri::WebviewWindowBuilder::new(handle, SCRIBE_LABEL, tauri::WebviewUrl::App(url.into()))
            .title("ShogunAI Scribe")
            .transparent(true)
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .shadow(false)
            .skip_taskbar(true)
            .inner_size(width, SCRIBE_H)
            .visible(false)
            .focused(true);

    if let Some(anchor) = opened.anchor {
        let x = anchor.x + (anchor.width - width) / 2.0;
        let above = anchor.y - SCRIBE_H - 8.0;
        let y = if above >= 4.0 {
            above
        } else {
            anchor.y + anchor.height + 8.0
        };
        builder = builder.position(x.max(4.0), y.max(4.0));
    }

    let window = builder.build().map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    eprintln!("[shell] Scribe field overlay opened");
    Ok(())
}

/// The Full UI window (spec §D) — Today, Context Health, Sources, Memory, Activity, Traceability.
///
/// Deliberately an ORDINARY window, not the overlay: the notch panel is a nonactivating NSPanel
/// that floats over everyone's Spaces and must never steal focus, whereas this is a place you sit
/// and read, so it wants a title bar, focus, resizing, and normal window management. It therefore
/// skips `adopt_native_panel` entirely and loads its own document (`fullui.html`).
///
/// Already open → focus it rather than building a second one.
pub(crate) fn build_full_ui_window(handle: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(win) = handle.get_webview_window(FULL_UI_LABEL) {
        // Re-opening from the panel should surface the window you already have.
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        eprintln!("[shell] full UI already open — focused");
        return;
    }
    let builder = tauri::WebviewWindowBuilder::new(
        handle,
        FULL_UI_LABEL,
        tauri::WebviewUrl::App("fullui.html".into()),
    )
    .title("ShogunAI")
    .resizable(true)
    // Spec §D floor. Below this the sidebar plus a three-card health row stops fitting.
    .min_inner_size(FULL_UI_MIN_W, FULL_UI_MIN_H)
    .inner_size(FULL_UI_W, FULL_UI_H)
    // The window surface is transparent, but the content is NOT glass: `.full` paints an opaque
    // ground (styles.css) so nothing shows through — this is a window you look AT (dense tables,
    // small type), where transparency would just be noise. Transparency stays on so the Overlay
    // title bar and the window's rounded corners composite cleanly over `.full`, not for a blur.
    .transparent(true)
    // Overlay, NOT Transparent. `Transparent` took the traffic lights with it and left the window
    // with no way to close — the content simply drew over where they had been. `Overlay` keeps
    // them floating above the title bar; the pane reserves room for them so nothing sits underneath.
    .title_bar_style(tauri::TitleBarStyle::Overlay)
    .focused(true);
    match builder.build() {
        Ok(win) => {
            center_on_cursor_screen(&win);
            eprintln!("[shell] full UI window built");
        }
        Err(e) => eprintln!("[shell] full UI window build failed: {e}"),
    }
}

/// Put the window on the display the cursor is on, centred.
///
/// Tauri places a new window on the primary monitor, which on a multi-display desk means the Full
/// UI opens somewhere you aren't looking. The panel already follows the cursor's screen; the
/// window should too, for the same reason — you asked for it from wherever you were working.
///
/// Best-effort: if the cursor or monitor can't be resolved we leave the window where Tauri put it
/// rather than guessing a position.
fn center_on_cursor_screen(win: &tauri::WebviewWindow) {
    let Ok(cursor) = win.cursor_position() else {
        return;
    };
    let Ok(Some(mon)) = win.monitor_from_point(cursor.x, cursor.y) else {
        return;
    };

    // Work in LOGICAL points, and use the size we asked for rather than reading it back.
    // `outer_size()` right after build returns the pre-scaling size on a Retina display, so
    // centring against it placed the window half a window-width too far right — far enough to
    // hang off the edge of the screen.
    let scale = mon.scale_factor();
    let mp = mon.position().to_logical::<f64>(scale);
    let ms = mon.size().to_logical::<f64>(scale);

    // `max(mp.*)` keeps a window larger than the display pinned to the top-left corner instead of
    // drifting off-screen to the left.
    let x = (mp.x + (ms.width - FULL_UI_W) / 2.0).max(mp.x);
    let y = (mp.y + (ms.height - FULL_UI_H) / 2.0).max(mp.y);
    let _ = win.set_position(tauri::LogicalPosition::new(x, y));
}

/// Visual recall browse window — timeline scrubber, image preview, OCR text, delete.
///
/// Ordinary window like Full UI: user sits and browses saved screens. Already open → focus it.
pub(crate) fn build_visual_recall_window(handle: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(win) = handle.get_webview_window(VISUAL_RECALL_LABEL) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        eprintln!("[shell] visual recall window already open — focused");
        return;
    }
    let builder = tauri::WebviewWindowBuilder::new(
        handle,
        VISUAL_RECALL_LABEL,
        tauri::WebviewUrl::App("visual-recall.html".into()),
    )
    .title("ShogunAI — Visual recall")
    .resizable(true)
    .min_inner_size(VISUAL_RECALL_MIN_W, VISUAL_RECALL_MIN_H)
    .inner_size(VISUAL_RECALL_W, VISUAL_RECALL_H)
    .transparent(true)
    .title_bar_style(tauri::TitleBarStyle::Overlay)
    .focused(true);
    match builder.build() {
        Ok(win) => {
            center_visual_recall_on_cursor_screen(&win);
            eprintln!("[shell] visual recall window built");
        }
        Err(e) => eprintln!("[shell] visual recall window build failed: {e}"),
    }
}

fn center_visual_recall_on_cursor_screen(win: &tauri::WebviewWindow) {
    let Ok(cursor) = win.cursor_position() else {
        return;
    };
    let Ok(Some(mon)) = win.monitor_from_point(cursor.x, cursor.y) else {
        return;
    };
    let scale = mon.scale_factor();
    let mp = mon.position().to_logical::<f64>(scale);
    let ms = mon.size().to_logical::<f64>(scale);
    let x = (mp.x + (ms.width - VISUAL_RECALL_W) / 2.0).max(mp.x);
    let y = (mp.y + (ms.height - VISUAL_RECALL_H) / 2.0).max(mp.y);
    let _ = win.set_position(tauri::LogicalPosition::new(x, y));
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
            let presentation = unsafe {
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
                    if subs.is_null() {
                        0
                    } else {
                        msg_send![subs, count]
                    }
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
                PanelPresentation {
                    visible,
                    on_active_space: on_active,
                    drawn,
                }
            };
            if panel_needs_reorder(presentation) {
                reassert_panel(&h2, "cold-launch-health");
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
        // Match boring.notch: borderless | utilityWindow | nonactivatingPanel | hudWindow.
        // Earlier: nonactivatingPanel alone (128) — fine for Space behavior, but utility+hud
        // matches the reference panel chrome that actually lives in the notch band.
        let style: usize = (1 << 4) | (1 << 7) | (1 << 13); // utility | nonactivating | hud
        let panel: *mut AnyObject = msg_send![alloc, initWithContentRect: frame, styleMask: style, backing: 2usize, defer: false];
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

        // Paint from (0,0) of the panel — no titlebar/safe-area inset pushing the WKWebView down.
        // Flexible width/height so setFrame on the panel keeps the webview flush-top.
        let content: NSRect = msg_send![panel, contentRectForFrameRect: frame];
        let flush = NSRect {
            origin: objc2_foundation::NSPoint { x: 0.0, y: 0.0 },
            size: content.size,
        };
        let _: () = msg_send![cv, setFrame: flush];
        // NSViewWidthSizable (2) | NSViewHeightSizable (16)
        let _: () = msg_send![cv, setAutoresizingMask: (2usize | 16usize)];

        // No NSVisualEffectView here on purpose. Vibrancy frosts what is behind the window —
        // it obscures your work rather than revealing it, which is the opposite of what this
        // overlay wants. The panel is plain alpha over a transparent NSPanel so the window you
        // were reading stays legible underneath.

        // Did the webview actually come with it? The panel can be perfectly placed, sized, ordered
        // front and still show nothing if the view it hosts is empty — and the webview keeps
        // running either way, so JS-side signals like `interact kind=boot` prove nothing about
        // whether any pixels exist. Unconditional: this is the one fact worth a line at every
        // launch, and it costs two message sends.
        let cv_frame: NSRect = msg_send![cv, frame];
        let subs: *mut AnyObject = msg_send![cv, subviews];
        let n: usize = if subs.is_null() {
            0
        } else {
            msg_send![subs, count]
        };
        eprintln!(
            "[shell] adopt: panel content view {:.0}x{:.0} origin=({:.0},{:.0}) with {n} subview(s){}",
            cv_frame.size.width,
            cv_frame.size.height,
            cv_frame.origin.x,
            cv_frame.origin.y,
            if n == 0 { " — EMPTY, nothing will be drawn" } else { "" }
        );

        let _: () = msg_send![panel, setReleasedWhenClosed: false];
        let _: () = msg_send![panel, setOpaque: false];
        let clear: *mut AnyObject = msg_send![class!(NSColor), clearColor];
        let _: () = msg_send![panel, setBackgroundColor: clear];
        // Weld into the hardware notch: native window shadow reads as a floating gap under the
        // bezel (boring.notch uses hasShadow=false; lift lives in CSS only on Expanded body).
        let _: () = msg_send![panel, setHasShadow: false];
        // Kill AppKit window animations that fight the CSS Idle↔Expanded morph.
        let _: () = msg_send![panel, setAnimationBehavior: 2isize]; // NSWindowAnimationBehaviorNone
        let want = PANEL_BEHAVIOR.load(std::sync::atomic::Ordering::Relaxed);
        let _: () = msg_send![panel, setCollectionBehavior: want];
        let _: () = msg_send![panel, setHidesOnDeactivate: false];
        let _: () = msg_send![panel, setCanHide: false];
        let _: () = msg_send![panel, setMovable: PANEL_MOVABLE];
        let _: () = msg_send![panel, setMovableByWindowBackground: PANEL_MOVABLE];
        // ORDER: setFloatingPanel BEFORE setLevel. AppKit's floating-panel path resets level to
        // NSFloatingWindowLevel (3); boring.notch sets isFloatingPanel first, then mainMenu+3.
        let _: () = msg_send![panel, setFloatingPanel: true];
        let _: () = msg_send![panel, setBecomesKeyOnlyIfNeeded: true];
        let _: () = msg_send![panel, setWorksWhenModal: true];
        let _: () = msg_send![panel, setAcceptsMouseMovedEvents: true];
        let _: () = msg_send![panel, setLevel: OVERLAY_LEVEL];

        NATIVE_PANEL.store(panel, std::sync::atomic::Ordering::Release);
        reposition_to_cursor_screen(panel);
        let _: () = msg_send![panel, orderFrontRegardless];

        let got: usize = msg_send![panel, collectionBehavior];
        let lvl: isize = msg_send![panel, level];
        let mask: usize = msg_send![panel, styleMask];
        let pf: NSRect = msg_send![panel, frame];
        eprintln!(
            "[shell] NATIVE NSPanel hosting the webview — behavior={got} level={lvl} styleMask={mask} \
             frame={:.0},{:.0} {:.0}x{:.0} (want level {OVERLAY_LEVEL}=mainMenu+3)",
            pf.origin.x, pf.origin.y, pf.size.width, pf.size.height
        );

    }
}

/// Resize the visible overlay (native panel or fallback window) — the webview's minimize/expand
/// control. AppKit frames are bottom-left origin. The default path re-docks the panel at its
/// resting place with the new size — the dragged spot when one exists (issue #21), else the
/// Castle Position (issue #20) — so it grows AWAY from its anchor (down from the notch, up from
/// the bottom, right from the left edge …; Notch stays welded under the hardware notch).
/// `anchor: "left"` is the manual corner-grip drag: it keeps the top-left corner put so the panel
/// grows down/right under the pointer, wherever the panel currently sits.
#[cfg(target_os = "macos")]
#[tauri::command]
fn set_panel_size(app: tauri::AppHandle, width: f64, height: f64, anchor: Option<String>) {
    use tauri::Manager;
    let keep_left = anchor.as_deref() == Some("left");
    let anchor_label = if keep_left { "left" } else { "rest" };
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        let Some(ptr) = overlay_ptr(&h) else { return };
        // SAFETY: main thread, live NSWindow/NSPanel.
        unsafe {
            let mut width = width;
            let mut height = height;
            let screen: *mut AnyObject = msg_send![ptr, screen];
            if !screen.is_null() {
                let f: NSRect = msg_send![screen, frame];
                let max_w = (f.size.width * PANEL_MAX_SCREEN_FRAC).floor();
                let max_h = (f.size.height * PANEL_MAX_SCREEN_FRAC).floor();
                width = width.min(max_w);
                height = height.min(max_h);
            }
            let f: NSRect = msg_send![ptr, frame];
            let pos = current_castle();
            let (mut x, mut y) = if keep_left {
                // Manual corner-grip resize (bottom-right grip): the top-left has to stay put or the
                // panel walks out from under the pointer.
                (f.origin.x, f.origin.y + f.size.height - height)
            } else if !screen.is_null() {
                // View switch (handle ↔ chat ↔ settings): re-dock at the resting place so a size
                // change grows away from the anchored edge — the dragged spot when one exists
                // (issue #21), else the Castle Position. Notch uses the full screen frame so the
                // top edge stays welded under the hardware notch.
                let o = resting_dock_origin(screen, width, height);
                (o.x, o.y)
            } else {
                // No screen (rare): fall back to holding the panel's centre and top edge.
                (
                    f.origin.x + f.size.width / 2.0 - width / 2.0,
                    f.origin.y + f.size.height - height,
                )
            };
            // Whichever path we took, never hang off the dock frame. A dragged panel is clamped
            // to the VISIBLE frame instead of the castle's dock rect: the castle rect is only a
            // superset today (Notch = full screen), so the difference is invisible — but the
            // moment a position narrows its dock band, clamping a dragged spot through the castle
            // would yank the panel home on every view switch.
            if !screen.is_null() {
                let clamped = if current_drag_override().is_some() {
                    clamp_to_visible_frame(screen, x, y, width, height)
                } else {
                    clamp_to_castle_dock(screen, x, y, width, height, pos)
                };
                x = clamped.0;
                y = clamped.1;
            }
            let r = NSRect {
                origin: NSPoint { x, y },
                size: NSSize { width, height },
            };
            // Bracketed: a resize repositions the frame origin, which posts the same did-move
            // notification a user drag does — without the flag every collapse/expand would be
            // recorded as a drag override.
            with_programmatic_move(|| {
                // Main thread, live NSWindow/NSPanel; the closure runs inside the enclosing
                // unsafe block, so no inner one is needed.
                let _: () = msg_send![ptr, setFrame: r, display: true];
                // Keep the hosted webview flush to the panel content origin after every resize —
                // otherwise a stale content-view origin reintroduces the under-notch gap.
                let cv: *mut AnyObject = msg_send![ptr, contentView];
                if !cv.is_null() {
                    let content: NSRect = msg_send![ptr, contentRectForFrameRect: r];
                    let flush = NSRect {
                        origin: NSPoint { x: 0.0, y: 0.0 },
                        size: content.size,
                    };
                    let _: () = msg_send![cv, setFrame: flush];
                }
            });
            // The window is transparent, so macOS derives its shadow from the rendered alpha mask
            // — and caches it. Without this the shadow keeps the shape the panel had at its
            // previous size, which shows up as a hard edge sitting away from the glass (most
            // visible collapsing the tall panel back to the pill).
            let _: () = msg_send![ptr, invalidateShadow];
            let _: () = msg_send![ptr, setHasShadow: false];
            let _: () = msg_send![ptr, setLevel: OVERLAY_LEVEL];
            let lvl: isize = msg_send![ptr, level];
            eprintln!(
                "[shell] panel resized to {:.0}x{:.0} at {:.0},{:.0} (anchor {}) level={}",
                width, height, r.origin.x, r.origin.y, anchor_label, lvl
            );
        }
        // Hover R_exp + CGEventTap band must match the live frame or move-into-panel collapses.
        if let Some(shared) = h.try_state::<std::sync::Arc<integrate::mac::Shared>>() {
            shared.set_panel_hit_size(width, height);
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
    sound::mac::play(shogun_core::sound::Cue::Summon);
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
            (
                "NSWorkspaceActiveSpaceDidChangeNotification",
                "space-changed",
            ),
            (
                "NSWorkspaceDidActivateApplicationNotification",
                "app-activated",
            ),
        ] {
            let handle = app.handle().clone();
            let name = NSString::from_str(name_str);
            let block = block2::RcBlock::new(move |_notif: *mut AnyObject| {
                let main_handle = handle.clone();
                if handle
                    .run_on_main_thread(move || reassert_panel(&main_handle, why))
                    .is_err()
                {
                    eprintln!("[shell] {why}: could not schedule panel reassert on main thread");
                }
            });
            let nil_obj: *mut AnyObject = std::ptr::null_mut();
            let _obs: *mut AnyObject = msg_send![nc, addObserverForName: &*name, object: nil_obj, queue: nil_obj, usingBlock: &*block];
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
                let hides_on_deactivate: bool = msg_send![ptr, hidesOnDeactivate];
                let can_hide: bool = msg_send![ptr, canHide];
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
                if behavior != want {
                    let _: () = msg_send![ptr, setCollectionBehavior: want];
                }
                if level != OVERLAY_LEVEL {
                    let _: () = msg_send![ptr, setLevel: OVERLAY_LEVEL];
                }
                if hides_on_deactivate {
                    let _: () = msg_send![ptr, setHidesOnDeactivate: false];
                }
                if can_hide {
                    let _: () = msg_send![ptr, setCanHide: false];
                }
                if behavior != want
                    || level != OVERLAY_LEVEL
                    || hides_on_deactivate
                    || can_hide
                {
                    eprintln!(
                        "[panelstate] healed: level {level}→{OVERLAY_LEVEL} behavior {behavior}→{want} \
                         hidesOnDeactivate {hides_on_deactivate}→false canHide {can_hide}→false"
                    );
                }
                (
                    format!(
                        "visible={visible} drawn={drawn} onActiveSpace={on_active} behavior={behavior} level={level} alpha={alpha:.2} appActive={app_active} origin=({:.0},{:.0})",
                        frame.origin.x, frame.origin.y
                    ),
                    !panel_needs_reorder(PanelPresentation {
                        visible,
                        on_active_space: on_active,
                        drawn,
                    }),
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
pub(crate) fn float_on_all_spaces(win: &tauri::WebviewWindow) {
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
        // The overlay is anchored to its Castle Position; background mouse-down must not move it.
        let _: () = msg_send![ptr, setMovable: PANEL_MOVABLE];
        let _: () = msg_send![ptr, setMovableByWindowBackground: PANEL_MOVABLE];
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
        let cls_name = if cls.is_null() {
            "?"
        } else {
            (*cls).name().to_str().unwrap_or("?")
        };
        eprintln!(
            "[shell] NSWindow behavior set={behavior} readback={got} level={lvl} hidesOnDeactivate={hides} styleMask={mask} class={cls_name}, ordered front"
        );
    }
}

/// Run the inline draft at the cursor — the ⌥-tap's action, also dispatched when "draft" is
/// rebound to a normal chord through the global-shortcut plugin.
#[cfg(target_os = "macos")]
pub(crate) fn run_inline_draft(handle: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(db) = handle.try_state::<shogun_core::daemon::Db>() {
        // The draft trigger is the fastest path in the product: read the pack the focus path
        // already built rather than assembling anything now.
        let warm = handle
            .try_state::<shogun_core::daemon::ReplyContextCache>()
            .and_then(|c| c.current());
        let directives = handle
            .try_state::<user_config_watch::UserConfigState>()
            .map(|s| s.directives())
            .unwrap_or_default();
        inline_source::mac::run_inline_at_cursor(
            db.inner().clone(),
            warm,
            handle.clone(),
            directives,
        );
    }
}

#[cfg(target_os = "macos")]
mod right_option_tap {
    use std::time::{Duration, Instant};

    pub const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(300);
    pub const DISPATCH_GRACE: Duration = Duration::from_millis(20);
    pub const FLAG_ALT: usize = 1 << 19;
    pub const RIGHT_OPTION_KEY_CODE: u16 = 61;
    pub const POISON_EVENT_MASK: usize = (1 << 1)
        | (1 << 2)
        | (1 << 3)
        | (1 << 4)
        | (1 << 5)
        | (1 << 6)
        | (1 << 7)
        | (1 << 8)
        | (1 << 9)
        | (1 << 18) // rotate
        | (1 << 19) // begin gesture
        | (1 << 20) // end gesture
        | (1 << 22) // scroll wheel
        | (1 << 23) // tablet point
        | (1 << 24) // tablet proximity
        | (1 << 25)
        | (1 << 26)
        | (1 << 27)
        | (1 << 29) // gesture
        | (1 << 30) // magnify
        | (1 << 31) // swipe
        | (1usize << 32) // smart magnify
        | (1usize << 34) // pressure
        | (1usize << 37); // direct touch

    pub fn tap_flag(combo: &str) -> Option<usize> {
        match combo.strip_prefix("Tap+")? {
            "Shift" => Some(1 << 17),
            "Control" => Some(1 << 18),
            "Alt" => Some(FLAG_ALT),
            "Super" => Some(1 << 20),
            "Fn" => Some(1 << 23),
            _ => None,
        }
    }

    pub fn correct_modifier_key(target: usize, key_code: u16) -> bool {
        target != FLAG_ALT || key_code == RIGHT_OPTION_KEY_CODE
    }

    pub fn clean_release(armed: bool, poisoned: bool, correct_key: bool, held_ms: u128) -> bool {
        armed && !poisoned && correct_key && held_ms <= 500
    }

    #[derive(Debug, PartialEq, Eq)]
    pub enum CleanTapAction {
        QueueDraft {
            generation: u64,
            superseded_draft: Option<u64>,
        },
        StartScribe,
    }

    #[derive(Default)]
    pub struct State {
        pending_draft: Option<(Instant, u64)>,
        next_generation: u64,
    }

    impl State {
        pub fn clean_tap(&mut self, now: Instant) -> CleanTapAction {
            if let Some((first, _)) = self.pending_draft {
                if now.saturating_duration_since(first) <= DOUBLE_TAP_WINDOW {
                    self.pending_draft = None;
                    return CleanTapAction::StartScribe;
                }
            }
            let superseded_draft = self.pending_draft.take().map(|(_, generation)| generation);
            self.next_generation = self.next_generation.wrapping_add(1);
            let generation = self.next_generation;
            self.pending_draft = Some((now, generation));
            CleanTapAction::QueueDraft {
                generation,
                superseded_draft,
            }
        }

        pub fn take_due_draft(&mut self, now: Instant) -> Option<u64> {
            let due = self.pending_draft.filter(|(started, _)| {
                now.saturating_duration_since(*started) >= DOUBLE_TAP_WINDOW
            });
            if due.is_some() {
                self.pending_draft = None;
            }
            due.map(|(_, generation)| generation)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn second_tap_at_exact_window_starts_scribe() {
            let start = Instant::now();
            let mut state = State::default();
            assert_eq!(
                state.clean_tap(start),
                CleanTapAction::QueueDraft {
                    generation: 1,
                    superseded_draft: None
                }
            );
            assert_eq!(
                state.clean_tap(start + DOUBLE_TAP_WINDOW),
                CleanTapAction::StartScribe
            );
            assert_eq!(state.take_due_draft(start + DOUBLE_TAP_WINDOW), None);
        }

        #[test]
        fn first_tap_is_due_only_after_window() {
            let start = Instant::now();
            let mut state = State::default();
            state.clean_tap(start);
            assert_eq!(
                state.take_due_draft(start + Duration::from_millis(299)),
                None
            );
            assert_eq!(state.take_due_draft(start + DOUBLE_TAP_WINDOW), Some(1));
        }

        #[test]
        fn late_second_tap_preserves_first_draft() {
            let start = Instant::now();
            let mut state = State::default();
            state.clean_tap(start);
            assert_eq!(
                state.clean_tap(start + DOUBLE_TAP_WINDOW + Duration::from_nanos(1)),
                CleanTapAction::QueueDraft {
                    generation: 2,
                    superseded_draft: Some(1)
                }
            );
        }

        #[test]
        fn only_right_option_is_allowed_for_alt_binding() {
            assert!(correct_modifier_key(FLAG_ALT, RIGHT_OPTION_KEY_CODE));
            assert!(!correct_modifier_key(FLAG_ALT, 58));
            assert!(correct_modifier_key(1 << 17, 56));
        }

        #[test]
        fn other_tap_modifiers_work_and_normal_chords_are_inert() {
            assert_eq!(tap_flag("Tap+Shift"), Some(1 << 17));
            assert_eq!(tap_flag("Tap+Control"), Some(1 << 18));
            assert_eq!(tap_flag("Control+Alt+KeyD"), None);
        }

        #[test]
        fn poisoned_or_long_hold_never_fires() {
            assert!(!clean_release(true, true, true, 100));
            assert!(!clean_release(true, false, true, 501));
            assert!(!clean_release(true, false, false, 100));
            assert!(clean_release(true, false, true, 500));
        }

        #[test]
        fn every_pointer_scroll_and_gesture_family_poisons_hold() {
            for bit in [
                1usize, 2, 3, 4, 5, 6, 7, 8, 9, 18, 19, 20, 22, 23, 24, 25, 26, 27, 29, 30, 31, 32,
                34, 37,
            ] {
                assert_ne!(
                    POISON_EVENT_MASK & (1usize << bit),
                    0,
                    "missing event bit {bit}"
                );
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn start_scribe(handle: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(db) = handle.try_state::<shogun_core::daemon::Db>() else {
        eprintln!("[shell] Scribe open skipped: database unavailable");
        return;
    };
    let warm = handle
        .try_state::<shogun_core::daemon::ReplyContextCache>()
        .and_then(|cache| cache.current());
    let directives = handle
        .try_state::<user_config_watch::UserConfigState>()
        .map(|state| state.directives())
        .unwrap_or_default();
    match scribe::mac::open_scribe(db.inner().clone(), warm, directives, handle.clone()) {
        Ok(opened) => {
            let session_id = opened.session_id;
            if let Err(error) = build_scribe_window(handle, opened) {
                let _ = scribe::mac::scribe_cancel(session_id, handle.clone());
                eprintln!("[shell] Scribe overlay failed: {error}");
            }
        }
        Err(error) => eprintln!("[shell] Scribe open failed: {error}"),
    }
}

/// TAP one modifier alone → draft at the cursor (default ⌥; the "draft" binding's "Tap+X" combo
/// picks the modifier, read live so a rebind applies without reinstalling). Semantics of a "tap":
/// the modifier goes down with no other modifier, no other key is pressed while it is held, and
/// it is released within 500ms. That keeps every normal modifier use intact — ⌥-arrow word nav,
/// ⌥+letter special characters — because any keyDown during the hold disarms the tap. Uses
/// NSEvent GLOBAL monitors (Accessibility permission, already required for capture); global
/// monitors only see other apps' events, which is exactly the draft target (the focused field
/// over there). Inert while "draft" is bound to a normal chord (the plugin handles those).
#[cfg(target_os = "macos")]
fn watch_option_tap(app: &tauri::App) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
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
    const MASK_MOUSE: usize = right_option_tap::POISON_EVENT_MASK;
    // shift | control | option | command | fn — the full standard-modifier set; whichever one the
    // binding targets, all the OTHERS joining the chord disqualifies the tap.
    const FLAG_ALL_MODS: usize = (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 23);
    /// The NSEvent modifier flag the "draft" binding targets, or None when draft is bound to a
    /// normal chord (the tap watcher is then inert; the plugin dispatches instead).
    fn tap_flag(handle: &tauri::AppHandle) -> Option<usize> {
        let combo = crate::shortcuts::binding(handle, "draft").unwrap_or_else(|| "Tap+Alt".into());
        right_option_tap::tap_flag(&combo)
    }

    /// Any non-Option input during the hold kills the tap until Option is released.
    fn poison() {
        POISONED.store(true, Ordering::Relaxed);
        ARMED.store(false, Ordering::Relaxed);
    }

    fn queue_draft(
        handle: tauri::AppHandle,
        state: Arc<Mutex<right_option_tap::State>>,
        generation: u64,
    ) {
        std::thread::spawn(move || {
            std::thread::sleep(
                right_option_tap::DOUBLE_TAP_WINDOW + right_option_tap::DISPATCH_GRACE,
            );
            let due = state
                .lock()
                .ok()
                .and_then(|mut state| state.take_due_draft(Instant::now()))
                == Some(generation);
            if due {
                eprintln!("[shell] right ⌥ tap — draft at cursor");
                crate::run_inline_draft(&handle);
            }
        });
    }

    let tap_state = Arc::new(Mutex::new(right_option_tap::State::default()));

    // SAFETY: main thread (setup); monitors and blocks are intentionally leaked (app lifetime).
    unsafe {
        let disarm_block = block2::RcBlock::new(move |_ev: *mut AnyObject| {
            // Any global key/mouse event doubles as the "user is here" stamp the daily-summary
            // cue gates on (issue #10) — global monitors never see our own panel's events, so
            // `interact` stamps those separately.
            crate::daily_summaries::note_global_input(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
            );
            poison();
        });
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
        let state_for_flags = tap_state.clone();
        let flags_block = block2::RcBlock::new(move |ev: *mut AnyObject| {
            if ev.is_null() {
                return;
            }
            let Some(target) = tap_flag(&handle) else {
                // Draft is bound to a normal chord right now — nothing for the tap watcher.
                return;
            };
            let flags: usize = msg_send![ev, modifierFlags];
            let key_code: u16 = msg_send![ev, keyCode];
            let target_down = flags & target != 0;
            let others_down = flags & (FLAG_ALL_MODS & !target) != 0;
            let was_down = OPT_PREV.swap(target_down, Ordering::Relaxed);

            if others_down {
                // A second modifier is part of this chord — poison for the rest of the hold.
                poison();
                return;
            }
            if target_down && !was_down {
                // The default gesture belongs to right Option only. Left Option remains ordinary
                // keyboard input and can never draft or open Scribe.
                if target == right_option_tap::FLAG_ALT
                    && !right_option_tap::correct_modifier_key(target, key_code)
                {
                    poison();
                    if let Ok(mut down) = DOWN_AT.lock() {
                        *down = None;
                    }
                    return;
                }
                // Genuine DOWN edge with nothing else held: start a fresh, clean hold.
                POISONED.store(false, Ordering::Relaxed);
                ARMED.store(true, Ordering::Relaxed);
                if let Ok(mut g) = DOWN_AT.lock() {
                    *g = Some(Instant::now());
                }
            } else if !target_down && was_down {
                // UP edge — fire only on a clean, short, un-poisoned tap.
                let armed = ARMED.swap(false, Ordering::Relaxed);
                let poisoned = POISONED.swap(false, Ordering::Relaxed);
                let held = DOWN_AT
                    .lock()
                    .ok()
                    .and_then(|g| *g)
                    .map(|t| t.elapsed().as_millis());
                let correct_key = right_option_tap::correct_modifier_key(target, key_code);
                if held.is_some_and(|milliseconds| {
                    right_option_tap::clean_release(armed, poisoned, correct_key, milliseconds)
                }) {
                    if target != right_option_tap::FLAG_ALT {
                        eprintln!("[shell] modifier tap — draft at cursor");
                        crate::run_inline_draft(&handle);
                        return;
                    }
                    let action = state_for_flags
                        .lock()
                        .ok()
                        .map(|mut state| state.clean_tap(Instant::now()));
                    match action {
                        Some(right_option_tap::CleanTapAction::StartScribe) => {
                            eprintln!("[shell] right ⌥ double-tap — Scribe");
                            start_scribe(&handle);
                        }
                        Some(right_option_tap::CleanTapAction::QueueDraft {
                            generation,
                            superseded_draft,
                        }) => {
                            if superseded_draft.is_some() {
                                crate::run_inline_draft(&handle);
                            }
                            queue_draft(handle.clone(), state_for_flags.clone(), generation);
                        }
                        None => {}
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
            eprintln!("[shell] tap-to-draft monitor failed to install (accessibility permission?)");
        } else {
            eprintln!("[shell] right ⌥ tap-to-draft/double-tap Scribe installed");
        }
    }
}

/// Register the ⌘⇧Space global shortcut → feed a Hotkey input to the engine (Idle→Expanded direct,
/// statemachine §3.3). Errors are logged, not fatal — the app still runs (hover remains available).
#[cfg(target_os = "macos")]
fn register_expand_shortcut(app: &tauri::App) {
    use std::sync::Arc;
    use tauri::Manager;
    use tauri_plugin_global_shortcut::{
        Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
    };

    // ⌘⇧J: ⌘⇧Space collides with the input-method source switcher on JP keyboards, so the OS
    // consumes it before the handler runs. J is uncontended.
    let expand = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyJ);
    let res = app
        .global_shortcut()
        .on_shortcut(expand, move |app, _sc, event| {
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

    // Draft and recall are rebindable like everything else (2026-08 decision), but their default
    // triggers are bare-modifier GESTURES, which the global-shortcut plugin cannot express. Those
    // use an extended combo grammar handled by NSEvent monitors instead:
    //   "Tap+Alt"    — a solo tap of one modifier (draft's ⌥ tap; watch_option_tap)
    //   "Dual+Super" — left+right of the same modifier together (recall's ⌘⌘; recall_shortcut)
    // Either action may also be bound to a normal chord ("Control+Alt+KeyD"), which then goes
    // through the plugin and `dispatch` like summon/quit.
    const ACTIONS: [&str; 5] = ["summon", "quit", "voice", "draft", "recall"];

    fn defaults() -> Bindings {
        let mut m = HashMap::new();
        m.insert("summon".into(), "Control+Alt+KeyN".into());
        m.insert("quit".into(), "Control+Alt+KeyQ".into());
        m.insert("voice".into(), "Control+Alt+KeyV".into());
        m.insert("draft".into(), "Tap+Alt".into());
        m.insert("recall".into(), "Dual+Super".into());
        m
    }

    /// Gesture combos ("Tap+X" / "Dual+X") live outside the global-shortcut plugin — the NSEvent
    /// monitors read the binding live on every event, so no (un)registration is needed for them.
    pub(crate) fn is_gesture(combo: &str) -> bool {
        combo.starts_with("Tap+") || combo.starts_with("Dual+")
    }

    /// A gesture combo must name a real modifier, and only draft/recall accept gestures at all
    /// (a summon you can trigger by tapping ⇧ alone would fire constantly while typing).
    fn validate_gesture(action: &str, combo: &str) -> Result<(), String> {
        if !matches!(action, "draft" | "recall") {
            return Err(format!(
                "{action} needs a key chord, not a modifier gesture"
            ));
        }
        let ok = match combo.split_once('+') {
            Some(("Tap", m)) => matches!(m, "Alt" | "Control" | "Shift" | "Super" | "Fn"),
            Some(("Dual", m)) => matches!(m, "Alt" | "Control" | "Shift" | "Super"),
            _ => false,
        };
        if ok {
            Ok(())
        } else {
            Err(format!("unknown gesture combo: {combo}"))
        }
    }

    fn config_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        app.path()
            .app_data_dir()
            .ok()
            .map(|d| d.join("shortcuts.json"))
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
    /// v5 = voice hold shortcut (⌃⌥V default). v6 = draft and recall become rebindable with the
    /// gesture combo grammar ("Tap+Alt" / "Dual+Super" defaults).
    const SHORTCUTS_VERSION: u32 = 6;

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
        let file = ShortcutsFile {
            version: SHORTCUTS_VERSION,
            binds: binds.clone(),
        };
        match serde_json::to_string_pretty(&file) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&p, json) {
                    eprintln!("[shell] shortcuts save failed: {e}");
                }
            }
            Err(e) => eprintln!("[shell] shortcuts serialize failed: {e}"),
        }
    }

    /// Read one persisted binding (used by hold-to-talk monitors).
    pub(crate) fn binding(app: &tauri::AppHandle, action: &str) -> Option<String> {
        app.try_state::<Store>()?
            .0
            .lock()
            .ok()?
            .get(action)
            .cloned()
    }

    /// Register `combo` for `action`. The combo string parses via the plugin (invalid combos and
    /// already-taken combos surface as Err — nothing changes in that case).
    pub fn register_action(
        app: &tauri::AppHandle,
        action: &str,
        combo: &str,
    ) -> Result<(), String> {
        if action == "voice" {
            // Hold-to-talk is wired through NSEvent monitors, not the global-shortcut plugin.
            return Ok(());
        }
        if is_gesture(combo) {
            // Gesture combos are watched by NSEvent monitors that read the binding live —
            // validate the token and there is nothing to register.
            return validate_gesture(action, combo);
        }
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
            "draft" => crate::run_inline_draft(app),
            "recall" => crate::build_visual_recall_window(app),
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
        if action != "voice" {
            register_action(&app, &action, &combo)?;
            // Only plugin-registered chords need unregistering — gesture combos were never
            // registered (their NSEvent monitors read the stored binding live).
            if let Some(old) = old.filter(|o| !is_gesture(o)) {
                if let Err(e) = app.global_shortcut().unregister(old.as_str()) {
                    eprintln!("[shell] old shortcut unregister failed ({old}): {e}");
                }
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

/// Castle Position (issue #20): where the panel resides on screen and expands from. Six choices,
/// notch (top-centre) by default. The value is Rust-owned (invariant 1), persisted to
/// `app_data/castle.json`, mirrored into the `CASTLE` atomic for the placement fns, and exposed to
/// both the UI and any future API symmetrically (invariant 6) through `get`/`set` commands.
///
/// The same file also carries the drag override (issue #21) — the user-dragged resting place that
/// takes precedence over the castle until a castle is picked again (see `DRAG_OVERRIDE` and
/// `docs/fixes/2026-07-30-pill-drag-port-design.md`).
#[cfg(target_os = "macos")]
mod castle {
    use super::{current_castle, current_drag_override, redock_to_castle, CASTLE, DRAG_OVERRIDE};
    use shogun_core::notch::geometry::CastlePosition;
    use std::sync::atomic::Ordering;
    use std::sync::OnceLock;
    use tauri::Manager;

    /// Resolved once in `init` so the did-move observer can persist a dragged spot without an
    /// `AppHandle` (notification blocks only get the NSNotification).
    static CONFIG_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();

    fn config_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        app.path()
            .app_data_dir()
            .ok()
            .map(|d| d.join("castle.json"))
    }

    /// On-disk form of the drag override: visible-frame offsets (`geometry::DragOffset`).
    #[derive(serde::Serialize, serde::Deserialize, Clone, Copy)]
    struct DragFile {
        dx: f64,
        dy: f64,
    }

    /// On-disk form: the stable wire key ("notch", "left_edge", …). A string, not the raw byte, so
    /// the file stays readable and survives any future re-encoding of the atomic. `drag` is the
    /// user-dragged resting place (issue #21); absent = docked at `position`. Optional and skipped
    /// when `None`, so files from before the drag port load unchanged and files without an
    /// override stay byte-identical to the old format.
    #[derive(serde::Serialize, serde::Deserialize, Default)]
    struct CastleFile {
        #[serde(default)]
        position: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        drag: Option<DragFile>,
    }

    /// Load the persisted position into runtime state. Legacy drag overrides are deliberately
    /// discarded and removed from disk: the notch is a docked surface now.
    pub fn init(app: &tauri::AppHandle) {
        if let Some(p) = config_path(app) {
            let _ = CONFIG_PATH.set(p);
        }
        let file = CONFIG_PATH
            .get()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<CastleFile>(&t).ok())
            .unwrap_or_default();
        let pos = CastlePosition::from_key(&file.position).unwrap_or_default();
        CASTLE.store(pos.to_u8(), Ordering::Relaxed);
        if let Ok(mut g) = DRAG_OVERRIDE.lock() {
            *g = None;
        }
        if file.drag.is_some() {
            if let Err(e) = save_now() {
                eprintln!("[shell] legacy drag override cleanup failed: {e}");
            }
            eprintln!("[shell] legacy drag override cleared");
        }
        eprintln!("[shell] castle position {}", pos.key());
    }

    /// Write the current position + drag override to `castle.json`. Path-less (pre-init /
    /// portless environments) degrades to a no-op rather than an error.
    fn save_now() -> Result<(), String> {
        let Some(p) = CONFIG_PATH.get() else {
            return Ok(());
        };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let file = CastleFile {
            position: current_castle().key().into(),
            drag: current_drag_override().map(|o| DragFile { dx: o.dx, dy: o.dy }),
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        std::fs::write(p, json).map_err(|e| format!("save failed: {e}"))
    }

    /// The current Castle Position as its wire key, for the Settings UI to preselect.
    #[tauri::command]
    pub fn get_castle_position() -> String {
        current_castle().key().into()
    }

    /// Move SHOGUN's castle. Persists the choice, updates the atomic, and re-docks the live panel
    /// immediately so the move is visible the moment it's picked. Picking a castle is also the
    /// explicit "go home" gesture: it CLEARS any drag override (issue #21) — the one way, from UI
    /// and API alike (invariant 6), to return a dragged panel to a named resting place.
    #[tauri::command]
    pub fn set_castle_position(app: tauri::AppHandle, position: String) -> Result<(), String> {
        let pos = CastlePosition::from_key(&position)
            .ok_or_else(|| format!("unknown castle position: {position}"))?;
        if let Ok(mut g) = DRAG_OVERRIDE.lock() {
            *g = None;
        }
        CASTLE.store(pos.to_u8(), Ordering::Relaxed);
        save_now()?;
        redock_to_castle(&app);
        eprintln!(
            "[shell] castle position → {} (drag override cleared)",
            pos.key()
        );
        Ok(())
    }
}

/// Re-dock the live overlay to the current Castle Position on its present screen. Used when the
/// user changes the position from Settings so the panel jumps to its new home at once.
#[cfg(target_os = "macos")]
fn redock_to_castle(handle: &tauri::AppHandle) {
    let h = handle.clone();
    let _ = handle.run_on_main_thread(move || {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use objc2_foundation::NSRect;
        let Some(ptr) = overlay_ptr(&h) else { return };
        // SAFETY: main thread, live NSWindow/NSPanel.
        unsafe {
            let screen: *mut AnyObject = msg_send![ptr, screen];
            if screen.is_null() {
                return;
            }
            let w: NSRect = msg_send![ptr, frame];
            // `resting_dock_origin`, not `castle_dock_origin`: the caller (set_castle_position)
            // clears the drag override first, so this resolves to the castle — and any future
            // caller that redocks WITHOUT clearing keeps respecting a dragged spot.
            let origin = resting_dock_origin(screen, w.size.width, w.size.height);
            with_programmatic_move(|| {
                // Main thread, live NSWindow/NSPanel; the closure runs inside the enclosing
                // unsafe block, so no inner one is needed.
                let _: () = msg_send![ptr, setFrameOrigin: origin];
            });
        }
    });
}

/// The Keychain account holding the database encryption key (service is the shared SHOGUN one).
#[cfg(target_os = "macos")]
const DB_KEY_ACCOUNT: &str = "memory-db-key";

/// Register connector runtime state for Tauri commands. Always called — `connectors_list` and
/// meeting settings must not fail with "state not managed" when the memory DB is down.
#[cfg(target_os = "macos")]
fn install_connectors(app: &tauri::AppHandle, db: Option<shogun_core::daemon::Db>) {
    use tauri::Manager;
    // The ONE shared L3 approval queue (B-3 / E-08): created unconditionally at startup, before
    // the connector runtime, so every producer and the confirm UI always resolve the same managed
    // queue — even when the connector runtime fails to start.
    let approval_path = app
        .path()
        .app_data_dir()
        .map(memory_data_dir)
        .map(|dir| dir.join(shogun_mcp::approval_store::STORE_FILE))
        .unwrap_or_else(|_| std::path::PathBuf::from(shogun_mcp::approval_store::STORE_FILE));
    let heartbeat_path = approval_path.with_file_name(shogun_mcp::desktop_heartbeat::FILE_NAME);
    // Only the desktop executes confirmed sends. Recover rows left in flight by its previous
    // process once at startup; ordinary queue transactions must never fail live work mid-send.
    match shogun_mcp::approval_store::recover_in_flight(
        &approval_path,
        shogun_mcp::approval_store::now_ms(),
    ) {
        Ok(recovered) if !recovered.is_empty() => eprintln!(
            "[approvals] recovered {} interrupted send(s) as failed",
            recovered.len()
        ),
        Ok(_) => {}
        Err(error) => eprintln!("[approvals] startup recovery failed: {error}"),
    }
    app.manage(approvals::mac::ApprovalQueueState::at(approval_path));
    // Headless MCP/REST sends require this fresh writer; a stopped desktop naturally fails closed.
    std::thread::spawn(move || loop {
        let _ = shogun_mcp::desktop_heartbeat::write(
            &heartbeat_path,
            shogun_mcp::desktop_heartbeat::now_ms(),
        );
        std::thread::sleep(std::time::Duration::from_secs(1));
    });
    // Draft-stop is seeded from the persisted ComposioPolicy (composio.json) — the single source
    // the settings/onboarding toggle and the L3 send gate read. Absent/unreadable policy defaults
    // to draft_stop = true (invariant 4 fail-safe, see ComposioPolicy).
    match connectors::mac::build_runtime(app, approvals::mac::load_composio_policy(app).draft_stop)
    {
        Ok(rt) => {
            let shared = std::sync::Arc::new(std::sync::Mutex::new(rt));
            if let Some(db) = db {
                connectors::mac::spawn_sync_poller(shared.clone(), db, app.clone());
                eprintln!("[spike] connector runtime started (read-sync poller live)");
            } else {
                eprintln!("[spike] connector runtime started (read-sync poller skipped — no DB)");
            }
            app.manage(connectors::mac::ConnectorState(shared));
        }
        Err(e) => eprintln!("[spike] connectors not started: {e}"),
    }
}

/// Whether an open failure looks like a corrupt or wrong-key encrypted file rather than a
/// transient I/O error.
#[cfg(target_os = "macos")]
fn is_unreadable_db_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("not a database")
        || lower.contains("file is encrypted or is not a database")
        || lower.contains("hmac check failed")
        || lower.contains("malformed database schema")
}

/// Open the encrypted DB, backing up and recreating when the on-disk file cannot be read.
#[cfg(target_os = "macos")]
fn open_encrypted_db(
    path: &std::path::Path,
    key: &shogun_memory::DbKey,
    clock: shogun_core::daemon::Clock,
    key_just_minted: bool,
) -> Result<shogun_core::daemon::Db, String> {
    match shogun_core::daemon::Db::open_encrypted(path, key, clock.clone()) {
        Ok(db) => Ok(db),
        Err(e) => {
            let msg = e.to_string();
            if path.exists() && is_unreadable_db_error(&msg) && !key_just_minted {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let backup = path.with_file_name(format!("memory.db.unreadable-{stamp}"));
                if std::fs::rename(path, &backup).is_ok() {
                    eprintln!(
                        "[spike] unreadable memory DB moved to {} — creating a fresh store",
                        backup.display()
                    );
                    return shogun_core::daemon::Db::open_encrypted(path, key, clock)
                        .map_err(|e| e.to_string());
                }
            }
            Err(msg)
        }
    }
}

/// Read the database key from the Keychain, generating and storing one on first run.
///
/// The key lives in the Keychain and nowhere else (invariant 7) — never a file, never a log, and
/// it is not derived from anything guessable. If the Keychain hands back something malformed we
/// refuse rather than silently minting a new key, because a new key would make the existing
/// memory permanently unreadable.
/// Whether the DB encryption key was freshly minted this launch (vs loaded from Keychain).
#[cfg(target_os = "macos")]
struct DbKeyLoad {
    key: shogun_memory::DbKey,
    minted: bool,
}

/// `errSecItemNotFound` — the only Keychain error where minting a new key is safe.
#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25_300;

#[cfg(target_os = "macos")]
fn db_key() -> Result<DbKeyLoad, String> {
    use shogun_integrations::keychain_store;
    match keychain_store::get_generic_secret(DB_KEY_ACCOUNT) {
        Ok(bytes) => {
            let hex = String::from_utf8(bytes).map_err(|_| "db key is not valid text".to_string())?;
            let key = shogun_memory::DbKey::from_hex(&hex)
                .ok_or_else(|| "db key in the Keychain is malformed — refusing to replace it".to_string())?;
            Ok(DbKeyLoad { key, minted: false })
        }
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => {
            // First run: mint a key from the OS CSPRNG.
            let mut raw = [0u8; 32];
            getrandom::getrandom(&mut raw).map_err(|e| format!("key generation failed: {e}"))?;
            let key = shogun_memory::DbKey::new(raw);
            keychain_store::set_generic_secret(DB_KEY_ACCOUNT, key.to_hex().as_bytes())
                .map_err(|e| format!("could not store the db key: {e}"))?;
            eprintln!("[spike] memory DB key created and stored in the Keychain");
            Ok(DbKeyLoad { key, minted: true })
        }
        Err(e) => Err(format!(
            "could not read the memory DB key from the Keychain (status {}): unlock Keychain access and relaunch",
            e.code()
        )),
    }
}

/// Resolve the memory store directory (co-locates `memory.db` and `visual_recall.json`).
#[cfg(target_os = "macos")]
pub(crate) fn memory_data_dir(base: std::path::PathBuf) -> std::path::PathBuf {
    let mut dir = base;
    if let Ok(suffix) = std::env::var("SHOGUN_DATA_SUFFIX") {
        let suffix = suffix.trim();
        if !suffix.is_empty() {
            let safe: String = suffix
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !safe.is_empty() {
                dir = dir.join(format!("dev-{safe}"));
            }
        }
    }
    dir
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
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let dir = memory_data_dir(base);
    if let Ok(suffix) = std::env::var("SHOGUN_DATA_SUFFIX") {
        let suffix = suffix.trim();
        if !suffix.is_empty() {
            let safe: String = suffix
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !safe.is_empty() {
                eprintln!("[spike] SHOGUN_DATA_SUFFIX set — using an isolated store: {safe}");
            }
        }
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("memory.db");
    eprintln!("[spike] memory DB: {}", path.display());
    let DbKeyLoad {
        key,
        minted: key_minted,
    } = db_key()?;

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

    let clock: shogun_core::daemon::Clock = std::sync::Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    });
    let db = open_encrypted_db(&path, &key, clock, key_minted)?;
    ensure_ort_dylib(app);
    let db = attach_embedder(db, embedding_model_paths(app));
    // 圧縮は段階展開: 既定 off。ヘビーユーザー/AB は SHOGUN_COMPRESSION=1 で有効化（設定 UI は次周）。
    let db = if std::env::var("SHOGUN_COMPRESSION")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("on"))
        .unwrap_or(false)
    {
        let budget = std::env::var("SHOGUN_COMPRESSION_BUDGET")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(2000);
        db.with_compression_config(shogun_fusion::compress::CompressionConfig {
            enabled: true,
            budget_tokens: budget,
            ..Default::default()
        })
    } else {
        db
    };
    Ok(db)
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
        // C-3: the effector that shows the overdue notifications (B-2's real ShowNotification).
        let effector = crate::notch_exec::mac::NotchEffector::new(db.clone());
        loop {
            let now = db.now_ms();
            let r = db.run_local_maintenance(now, HALF_LIFE_MS);
            if r.corroborated > 0 || r.overdue > 0 {
                eprintln!(
                    "[maintenance] {} corroborated, {} newly overdue, {} loops aged, {} decayed",
                    r.corroborated, r.overdue, r.stale, r.decayed
                );
            }
            // C-3: one notification per newly-overdue commitment. `newly_overdue` holds only the
            // rows THIS pass flipped open→overdue (the flip is the dedup watermark — core-tested),
            // so nothing here can re-notify. The actions are ShowNotification: non-egress and
            // L1-permitted (pinned by `overdue_notifications_are_l1_non_sends` in shogun-core),
            // which is why they may run directly through the effector.
            for action in shogun_core::daemon::overdue_notifications(&r.newly_overdue) {
                debug_assert!(action.is_l1_eligible() && !action.is_external_send());
                if let Err(e) = shogun_agents::engine::LocalEffector::run(&effector, &action) {
                    // The reason only — never the notification text (state summaries stay out
                    // of logs).
                    eprintln!("[maintenance] overdue notification failed: {e}");
                }
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
    let Ok(res) = app.path().resource_dir() else {
        return;
    };
    // Contents/Frameworks is a sibling of Contents/Resources.
    let bundled = [
        res.parent()
            .map(|c| c.join("Frameworks/libonnxruntime.dylib")),
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
    if let (Ok(m), Ok(t)) = (
        std::env::var("SHOGUN_EMBED_MODEL"),
        std::env::var("SHOGUN_EMBED_TOKENIZER"),
    ) {
        return Some((m.into(), t.into()));
    }
    let dir = app
        .path()
        .resource_dir()
        .ok()?
        .join("models/multilingual-e5-small");
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
        startup_health::mac::set_embedding_model(false);
        return db;
    };
    match shogun_memory::embed_onnx::OnnxEmbedder::load(&model, &tokenizer) {
        Ok(e) => {
            eprintln!("[embed] local model loaded — hybrid search enabled");
            startup_health::mac::set_embedding_model(true);
            db.with_embedder(std::sync::Arc::new(e))
        }
        Err(e) => {
            eprintln!("[embed] model present but failed to load ({e}) — search stays lexical");
            // Present-but-broken is the same outcome for the user as absent: lexical search.
            startup_health::mac::set_embedding_model(false);
            db
        }
    }
}
