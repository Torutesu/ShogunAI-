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
mod mark_launch;
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
mod splash;
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

mod bootstrap;
mod panel;

pub use bootstrap::run;
use panel::*;

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
        let db = db.inner();
        let directives = handle
            .try_state::<user_config_watch::UserConfigState>()
            .map(|s| s.directives_for_frontmost_app(db))
            .unwrap_or_default();
        inline_source::mac::run_inline_at_cursor(db.clone(), warm, handle.clone(), directives);
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
        .map(|state| state.directives_for_frontmost_app(db.inner()))
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
