//! Hold-to-talk shortcut via NSEvent global + local monitors (#44).
//!
//! Global shortcuts only fire on press; hold/release needs NSEvent (same pattern as ⌥-tap and
//! Wispr/Hush). The combo is persisted in shortcuts.json (`voice` action) and read live.
//!
//! Local monitors require a block that *returns* the NSEvent (pass-through). Reusing a void
//! global-style block for local monitors is an ABI mismatch and crashes on key-up when the app
//! is key — that was the release-time crash. Start/end also run on one worker so a fast tap
//! cannot process End before Start finishes (stuck "recording").
//!
//! Critical: `-[NSEvent type]` returns NSEventType (10/11/12), NOT the monitor mask bit
//! (`1 << type`). Comparing type to MASK_* made `is_release` always false → stuck recording.

use std::cell::RefCell;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::AppHandle;

/// Monitor registration masks (`addGlobalMonitorForEventsMatchingMask:`).
const MASK_KEY_DOWN: usize = 1 << 10;
const MASK_KEY_UP: usize = 1 << 11;
const MASK_FLAGS_CHANGED: usize = 1 << 12;

/// `-[NSEvent type]` / NSEventType values — must NOT be confused with MASK_* above.
const TYPE_KEY_DOWN: usize = 10;
const TYPE_KEY_UP: usize = 11;
const TYPE_FLAGS_CHANGED: usize = 12;

const FLAG_SHIFT: usize = 1 << 17;
const FLAG_CONTROL: usize = 1 << 18;
const FLAG_OPTION: usize = 1 << 19;
const FLAG_COMMAND: usize = 1 << 20;
const FLAG_FUNCTION: usize = 1 << 23;

/// Auto-end a forgotten hold so the notch cannot stick in "recording".
const MAX_HOLD: Duration = Duration::from_secs(30);
/// After End, re-check once so a missed stop cannot leave the mic/UI live.
const END_FAILSAFE: Duration = Duration::from_millis(500);
/// Retry global shortcut registration after Accessibility changes without blocking AppKit.
const GLOBAL_MONITOR_RETRY: Duration = Duration::from_secs(3);

struct Combo {
    modifiers: usize,
    key_code: u16,
}

enum Cmd {
    Start(AppHandle),
    End(AppHandle),
}

static HOLDING: AtomicBool = AtomicBool::new(false);
static INSTALL_STARTED: AtomicBool = AtomicBool::new(false);
static LOCAL_DOWN_INSTALLED: AtomicBool = AtomicBool::new(false);
static LOCAL_UP_INSTALLED: AtomicBool = AtomicBool::new(false);
static GLOBAL_TRUST_WATCHER_RUNNING: AtomicBool = AtomicBool::new(false);
static CMD_TX: Mutex<Option<Sender<Cmd>>> = Mutex::new(None);

type GlobalMonitorHandler = block2::RcBlock<dyn Fn(NonNull<objc2_app_kit::NSEvent>)>;

/// Both token and callback must survive for as long as the AppKit global monitor is installed.
/// This state is main-thread-local because AppKit monitor registration is main-thread-only.
#[derive(Default)]
struct GlobalMonitorOwnership {
    down: Option<(
        objc2::rc::Retained<objc2::runtime::AnyObject>,
        GlobalMonitorHandler,
    )>,
    up: Option<(
        objc2::rc::Retained<objc2::runtime::AnyObject>,
        GlobalMonitorHandler,
    )>,
}

thread_local! {
    static GLOBAL_MONITORS: RefCell<GlobalMonitorOwnership> = RefCell::new(GlobalMonitorOwnership::default());
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GlobalMonitorState {
    down_installed: bool,
    up_installed: bool,
}

impl GlobalMonitorState {
    fn needs_recovery(self) -> bool {
        !self.down_installed || !self.up_installed
    }

    fn has_any_monitor(self) -> bool {
        self.down_installed || self.up_installed
    }

    fn after_attempt(self, down_installed: bool, up_installed: bool) -> Self {
        Self {
            down_installed: self.down_installed || down_installed,
            up_installed: self.up_installed || up_installed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlobalMonitorAction {
    Keep,
    InstallMissing,
    RemoveAll,
}

fn global_monitor_action(trusted: bool, state: GlobalMonitorState) -> GlobalMonitorAction {
    match (trusted, state.has_any_monitor(), state.needs_recovery()) {
        (false, false, _) => GlobalMonitorAction::Keep,
        (false, true, _) => GlobalMonitorAction::RemoveAll,
        (true, _, true) => GlobalMonitorAction::InstallMissing,
        (true, _, false) => GlobalMonitorAction::Keep,
    }
}

fn key_code_for_token(token: &str) -> Option<u16> {
    if let Some(letter) = token.strip_prefix("Key") {
        return mac_key_code(letter);
    }
    if let Some(d) = token.strip_prefix("Digit") {
        return match d {
            "0" => Some(29),
            "1" => Some(18),
            "2" => Some(19),
            "3" => Some(20),
            "4" => Some(21),
            "5" => Some(23),
            "6" => Some(22),
            "7" => Some(26),
            "8" => Some(28),
            "9" => Some(25),
            _ => None,
        };
    }
    None
}

/// US keyboard virtual key codes (macOS).
fn mac_key_code(letter: &str) -> Option<u16> {
    match letter {
        "A" => Some(0),
        "B" => Some(11),
        "C" => Some(8),
        "D" => Some(2),
        "E" => Some(14),
        "F" => Some(3),
        "G" => Some(5),
        "H" => Some(4),
        "I" => Some(34),
        "J" => Some(38),
        "K" => Some(40),
        "L" => Some(37),
        "M" => Some(46),
        "N" => Some(45),
        "O" => Some(31),
        "P" => Some(35),
        "Q" => Some(12),
        "R" => Some(15),
        "S" => Some(1),
        "T" => Some(17),
        "U" => Some(32),
        "V" => Some(9),
        "W" => Some(13),
        "X" => Some(7),
        "Y" => Some(16),
        "Z" => Some(6),
        _ => None,
    }
}

fn parse_combo(combo: &str) -> Option<Combo> {
    let mut mods = 0usize;
    let mut key_code = None;
    for part in combo.split('+') {
        match part {
            "Control" => mods |= FLAG_CONTROL,
            "Alt" => mods |= FLAG_OPTION,
            "Shift" => mods |= FLAG_SHIFT,
            "Super" => mods |= FLAG_COMMAND,
            "Fn" | "Function" => mods |= FLAG_FUNCTION,
            other => {
                if let Some(k) = key_code_for_token(other) {
                    key_code = Some(k);
                }
            }
        }
    }
    Some(Combo {
        modifiers: mods,
        key_code: key_code?,
    })
}

fn default_combo() -> Combo {
    // Hardcoded Control+Alt+V — parse cannot fail.
    Combo {
        modifiers: FLAG_CONTROL | FLAG_OPTION,
        key_code: 9,
    }
}

fn normalize_mods(flags: usize) -> usize {
    flags & (FLAG_SHIFT | FLAG_CONTROL | FLAG_OPTION | FLAG_COMMAND | FLAG_FUNCTION)
}

fn combo_matches(flags: usize, key: u16, combo: &Combo) -> bool {
    normalize_mods(flags) == combo.modifiers && key == combo.key_code
}

fn type_name(ty: usize) -> &'static str {
    match ty {
        TYPE_KEY_DOWN => "keyDown",
        TYPE_KEY_UP => "keyUp",
        TYPE_FLAGS_CHANGED => "flagsChanged",
        _ => "other",
    }
}

fn mods_label(mods: usize) -> String {
    let mut parts = Vec::new();
    if mods & FLAG_CONTROL != 0 {
        parts.push("⌃");
    }
    if mods & FLAG_OPTION != 0 {
        parts.push("⌥");
    }
    if mods & FLAG_SHIFT != 0 {
        parts.push("⇧");
    }
    if mods & FLAG_COMMAND != 0 {
        parts.push("⌘");
    }
    if mods & FLAG_FUNCTION != 0 {
        parts.push("Fn");
    }
    if parts.is_empty() {
        "none".into()
    } else {
        parts.join("")
    }
}

/// True when this event touches the voice chord (letter key or a required modifier change).
fn touches_combo(ty: usize, flags: usize, key: u16, combo: &Combo) -> bool {
    if key == combo.key_code {
        return true;
    }
    if ty == TYPE_FLAGS_CHANGED {
        let before_relevant = HOLDING.load(Ordering::SeqCst);
        let mods = normalize_mods(flags);
        return before_relevant || (mods & combo.modifiers) != 0;
    }
    false
}

/// Release when the letter key goes up, OR any required modifier of the chord drops.
/// ⌃⌥V: releasing Control OR Option OR V all end the hold (order does not matter).
fn is_release(ty: usize, flags: usize, key: u16, combo: &Combo) -> bool {
    if ty == TYPE_KEY_UP {
        return key == combo.key_code;
    }
    if ty == TYPE_FLAGS_CHANGED {
        let mods = normalize_mods(flags);
        return (mods & combo.modifiers) != combo.modifiers;
    }
    false
}

fn current_voice_combo(app: &AppHandle) -> Combo {
    let combo =
        crate::shortcuts::binding(app, "voice").unwrap_or_else(|| "Control+Alt+KeyV".into());
    parse_combo(&combo).unwrap_or_else(default_combo)
}

fn send_cmd(cmd: Cmd) {
    if let Ok(guard) = CMD_TX.lock() {
        if let Some(tx) = guard.as_ref() {
            let _ = tx.send(cmd);
        }
    }
}

fn shortcut_modifier_flags_from_cg(flags: u64) -> usize {
    normalize_mods(flags as usize)
}

fn current_modifier_flags() -> usize {
    use objc2_core_graphics::{CGEventSource, CGEventSourceStateID};

    // The worker is not AppKit's main thread. CoreGraphics' combined session state is safe here
    // and shares the modifier-bit layout used by NSEvent for the shortcut modifiers.
    shortcut_modifier_flags_from_cg(
        CGEventSource::flags_state(CGEventSourceStateID::CombinedSessionState).0,
    )
}

fn on_press(app: &AppHandle, ev: *mut objc2::runtime::AnyObject) {
    use objc2::msg_send;
    let combo = current_voice_combo(app);
    // SAFETY: NSEvent pointer from AppKit monitor callback.
    let flags: usize = unsafe { msg_send![ev, modifierFlags] };
    let key: u16 = unsafe { msg_send![ev, keyCode] };
    let ty: usize = unsafe { msg_send![ev, type] };
    if touches_combo(ty, flags, key, &combo) {
        eprintln!(
            "[voice] {} key={} mods={} holding={}",
            type_name(ty),
            key,
            mods_label(normalize_mods(flags)),
            HOLDING.load(Ordering::SeqCst)
        );
    }
    if HOLDING.load(Ordering::SeqCst) {
        return;
    }
    if combo_matches(flags, key, &combo) {
        HOLDING.store(true, Ordering::SeqCst);
        eprintln!(
            "[voice] hold start chord key={} mods={}",
            key,
            mods_label(normalize_mods(flags))
        );
        send_cmd(Cmd::Start(app.clone()));
    }
}

fn on_release(app: &AppHandle, ev: *mut objc2::runtime::AnyObject) {
    use objc2::msg_send;
    let combo = current_voice_combo(app);
    // SAFETY: NSEvent pointer from AppKit monitor callback.
    let flags: usize = unsafe { msg_send![ev, modifierFlags] };
    let ty: usize = unsafe { msg_send![ev, type] };
    let key: u16 = unsafe { msg_send![ev, keyCode] };
    if touches_combo(ty, flags, key, &combo) {
        eprintln!(
            "[voice] {} key={} mods={} holding={}",
            type_name(ty),
            key,
            mods_label(normalize_mods(flags)),
            HOLDING.load(Ordering::SeqCst)
        );
    }
    if !HOLDING.load(Ordering::SeqCst) {
        return;
    }
    if is_release(ty, flags, key, &combo) {
        HOLDING.store(false, Ordering::SeqCst);
        eprintln!(
            "[voice] hold release ({} key={} mods={})",
            type_name(ty),
            key,
            mods_label(normalize_mods(flags))
        );
        send_cmd(Cmd::End(app.clone()));
    }
}

fn schedule_end_failsafe(app: AppHandle) {
    std::thread::Builder::new()
        .name("voice-end-failsafe".into())
        .spawn(move || {
            std::thread::sleep(END_FAILSAFE);
            if crate::voice_session::mac::lifecycle::force_end_if_recording(app) {
                eprintln!(
                    "[voice] end failsafe — still recording {}ms after release",
                    END_FAILSAFE.as_millis()
                );
            }
        })
        .ok();
}

fn worker_loop(rx: Receiver<Cmd>) {
    let mut hold_started: Option<Instant> = None;
    let mut timeout_app: Option<AppHandle> = None;
    loop {
        let wait = if hold_started.is_some() {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(3600)
        };
        match rx.recv_timeout(wait) {
            Ok(Cmd::Start(app)) => {
                let started = crate::voice_session::mac::lifecycle::on_hold_start(app.clone());
                if started {
                    hold_started = Some(Instant::now());
                    timeout_app = Some(app);
                } else {
                    // Start failed / disabled — clear sticky HOLDING so the next press works.
                    HOLDING.store(false, Ordering::SeqCst);
                    hold_started = None;
                    timeout_app = None;
                }
            }
            Ok(Cmd::End(app)) => {
                hold_started = None;
                timeout_app = None;
                crate::voice_session::mac::lifecycle::on_hold_end(app.clone());
                schedule_end_failsafe(app);
            }
            Err(RecvTimeoutError::Timeout) => {
                if let (Some(started), Some(app)) = (hold_started, timeout_app.as_ref()) {
                    // Poll modifier flags while holding — catches a missed flagsChanged (common
                    // when Control/Option release order races with the letter key on ⌃⌥V).
                    if HOLDING.load(Ordering::SeqCst) {
                        let combo = current_voice_combo(app);
                        let mods = current_modifier_flags();
                        if (mods & combo.modifiers) != combo.modifiers {
                            HOLDING.store(false, Ordering::SeqCst);
                            eprintln!("[voice] hold release (mod poll mods={})", mods_label(mods));
                            hold_started = None;
                            let app = app.clone();
                            timeout_app = None;
                            crate::voice_session::mac::lifecycle::on_hold_end(app.clone());
                            schedule_end_failsafe(app);
                            continue;
                        }
                    }
                    if started.elapsed() >= MAX_HOLD && HOLDING.swap(false, Ordering::SeqCst) {
                        eprintln!("[voice] hold auto-end after {}s", MAX_HOLD.as_secs());
                        hold_started = None;
                        let app = app.clone();
                        timeout_app = None;
                        crate::voice_session::mac::lifecycle::on_hold_end(app.clone());
                        schedule_end_failsafe(app);
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn global_monitor_state() -> GlobalMonitorState {
    GLOBAL_MONITORS.with(|monitors| {
        let monitors = monitors.borrow();
        GlobalMonitorState {
            down_installed: monitors.down.is_some(),
            up_installed: monitors.up.is_some(),
        }
    })
}

/// Remove every retained global monitor token. AppKit requires this on its main thread.
fn remove_global_monitors_main(app: &tauri::AppHandle) {
    use objc2_app_kit::NSEvent;

    if objc2::MainThreadMarker::new().is_none() {
        eprintln!("[voice] global monitor removal skipped outside AppKit main thread");
        return;
    }

    let (down, up) = GLOBAL_MONITORS.with(|monitors| {
        let mut monitors = monitors.borrow_mut();
        (monitors.down.take(), monitors.up.take())
    });
    let removed = down.is_some() || up.is_some();
    // SAFETY: tokens are returned by NSEvent and each is removed at most once.
    unsafe {
        if let Some((token, _handler)) = down {
            NSEvent::removeMonitor(&token);
        }
        if let Some((token, _handler)) = up {
            NSEvent::removeMonitor(&token);
        }
    }
    if removed {
        HOLDING.store(false, Ordering::SeqCst);
        send_cmd(Cmd::End(app.clone()));
        eprintln!("[voice] global monitors removed after Accessibility change");
    }
}

/// Local fallback remains available when SHOGUN is focused, even without Accessibility access.
fn install_local_monitors_main(app: &tauri::AppHandle) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    if objc2::MainThreadMarker::new().is_none() {
        eprintln!("[voice] local monitor install skipped outside AppKit main thread");
        return;
    }

    // SAFETY: AppKit main thread. Local monitor blocks pass events through unchanged.
    unsafe {
        if !LOCAL_DOWN_INSTALLED.load(Ordering::SeqCst) {
            let handle = app.clone();
            let local_start = block2::RcBlock::new(move |ev: *mut AnyObject| -> *mut AnyObject {
                if !ev.is_null() {
                    on_press(&handle, ev);
                }
                ev
            });
            let local: *mut AnyObject = msg_send![
                class!(NSEvent),
                addLocalMonitorForEventsMatchingMask: MASK_KEY_DOWN,
                handler: &*local_start
            ];
            if local.is_null() {
                eprintln!("[voice] hold local key-down monitor unavailable");
            } else {
                LOCAL_DOWN_INSTALLED.store(true, Ordering::SeqCst);
                std::mem::forget(local_start);
            }
        }

        if !LOCAL_UP_INSTALLED.load(Ordering::SeqCst) {
            let handle = app.clone();
            let local_stop = block2::RcBlock::new(move |ev: *mut AnyObject| -> *mut AnyObject {
                if !ev.is_null() {
                    on_release(&handle, ev);
                }
                ev
            });
            let local: *mut AnyObject = msg_send![
                class!(NSEvent),
                addLocalMonitorForEventsMatchingMask: MASK_KEY_UP | MASK_FLAGS_CHANGED,
                handler: &*local_stop
            ];
            if local.is_null() {
                eprintln!("[voice] hold local release monitor unavailable");
            } else {
                LOCAL_UP_INSTALLED.store(true, Ordering::SeqCst);
                std::mem::forget(local_stop);
            }
        }
    }
}

/// Install only the global handlers that are missing after an earlier partial registration.
fn install_global_monitors_main(app: &tauri::AppHandle) {
    use objc2_app_kit::{NSEvent, NSEventMask};

    if objc2::MainThreadMarker::new().is_none() {
        eprintln!("[voice] global monitor install skipped outside AppKit main thread");
        return;
    }

    let before = global_monitor_state();
    if !before.needs_recovery() {
        return;
    }

    let mut installed_down = false;
    let mut installed_up = false;

    if !before.down_installed {
        let handle = app.clone();
        let handler: GlobalMonitorHandler =
            block2::RcBlock::new(move |event: NonNull<objc2_app_kit::NSEvent>| {
                on_press(&handle, event.as_ptr().cast());
            });
        if let Some(token) =
            NSEvent::addGlobalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &handler)
        {
            GLOBAL_MONITORS.with(|monitors| monitors.borrow_mut().down = Some((token, handler)));
            installed_down = true;
        } else {
            eprintln!("[voice] global key-down monitor unavailable; retry scheduled");
        }
    }

    if !before.up_installed {
        let handle = app.clone();
        let handler: GlobalMonitorHandler =
            block2::RcBlock::new(move |event: NonNull<objc2_app_kit::NSEvent>| {
                on_release(&handle, event.as_ptr().cast());
            });
        if let Some(token) = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::KeyUp | NSEventMask::FlagsChanged,
            &handler,
        ) {
            GLOBAL_MONITORS.with(|monitors| monitors.borrow_mut().up = Some((token, handler)));
            installed_up = true;
        } else {
            eprintln!("[voice] global release monitor unavailable; retry scheduled");
        }
    }

    if !before
        .after_attempt(installed_down, installed_up)
        .needs_recovery()
    {
        eprintln!("[voice] global hold-to-talk monitors installed");
    }
}

fn reconcile_global_monitors_main(app: &tauri::AppHandle) {
    if objc2::MainThreadMarker::new().is_none() {
        eprintln!("[voice] global monitor reconcile skipped outside AppKit main thread");
        return;
    }

    match global_monitor_action(crate::axcache::ax_trusted_silent(), global_monitor_state()) {
        GlobalMonitorAction::Keep => {}
        GlobalMonitorAction::InstallMissing => install_global_monitors_main(app),
        GlobalMonitorAction::RemoveAll => remove_global_monitors_main(app),
    }
}

fn start_global_trust_watcher(app: tauri::AppHandle) {
    if GLOBAL_TRUST_WATCHER_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    let spawned = std::thread::Builder::new()
        .name("voice-monitor-trust".into())
        .spawn(move || loop {
            std::thread::sleep(GLOBAL_MONITOR_RETRY);
            let handle = app.clone();
            if app
                .run_on_main_thread(move || reconcile_global_monitors_main(&handle))
                .is_err()
            {
                eprintln!("[voice] global trust watcher could not reach AppKit main thread");
            }
        });
    if spawned.is_err() {
        GLOBAL_TRUST_WATCHER_RUNNING.store(false, Ordering::SeqCst);
        eprintln!("[voice] global trust watcher thread unavailable");
    }
}

/// Install global and local NSEvent monitors for the configured hold-to-talk combo.
pub fn install(app: &tauri::AppHandle) {
    // Hot-reload / double setup must not stack monitors — that reintroduced the release SIGSEGV.
    if INSTALL_STARTED.swap(true, Ordering::SeqCst) {
        eprintln!("[voice] hold-to-talk already installed — skip");
        return;
    }

    let (tx, rx) = mpsc::channel::<Cmd>();
    if let Ok(mut slot) = CMD_TX.lock() {
        *slot = Some(tx);
    }
    let _ = std::thread::Builder::new()
        .name("voice-hold".into())
        .spawn(move || worker_loop(rx));

    if objc2::MainThreadMarker::new().is_some() {
        install_local_monitors_main(app);
        reconcile_global_monitors_main(app);
    } else {
        let handle = app.clone();
        if app
            .run_on_main_thread(move || {
                install_local_monitors_main(&handle);
                reconcile_global_monitors_main(&handle);
            })
            .is_err()
        {
            eprintln!("[voice] initial monitor install could not reach AppKit main thread");
        }
    }
    start_global_trust_watcher(app.clone());

    eprintln!("[voice] hold-to-talk monitor setup scheduled (⌃⌥V default)");
}

#[cfg(test)]
mod tests {
    use super::{
        global_monitor_action, shortcut_modifier_flags_from_cg, GlobalMonitorAction,
        GlobalMonitorState, FLAG_COMMAND, FLAG_CONTROL, FLAG_FUNCTION, FLAG_OPTION, FLAG_SHIFT,
    };

    #[test]
    fn core_graphics_modifier_bits_preserve_shift_option_and_function() {
        let flags = (FLAG_SHIFT | FLAG_OPTION | FLAG_FUNCTION | (1 << 16) | (1 << 8)) as u64;

        assert_eq!(
            shortcut_modifier_flags_from_cg(flags),
            FLAG_SHIFT | FLAG_OPTION | FLAG_FUNCTION
        );
    }

    #[test]
    fn core_graphics_modifier_bits_preserve_control_and_command() {
        assert_eq!(
            shortcut_modifier_flags_from_cg((FLAG_CONTROL | FLAG_COMMAND) as u64),
            FLAG_CONTROL | FLAG_COMMAND
        );
    }

    #[test]
    fn denied_startup_keeps_empty_global_monitor_ownership() {
        assert_eq!(
            global_monitor_action(false, GlobalMonitorState::default()),
            GlobalMonitorAction::Keep
        );
    }

    #[test]
    fn trusted_partial_registration_installs_only_missing_handler() {
        let state = GlobalMonitorState::default().after_attempt(true, false);

        assert_eq!(
            global_monitor_action(true, state),
            GlobalMonitorAction::InstallMissing
        );
    }

    #[test]
    fn trusted_complete_registration_stays_steady() {
        let state = GlobalMonitorState::default().after_attempt(true, true);

        assert_eq!(
            global_monitor_action(true, state),
            GlobalMonitorAction::Keep
        );
    }

    #[test]
    fn revoked_accessibility_removes_retained_global_tokens() {
        let state = GlobalMonitorState::default().after_attempt(true, true);

        assert_eq!(
            global_monitor_action(false, state),
            GlobalMonitorAction::RemoveAll
        );
    }

    #[test]
    fn regranted_accessibility_reinstalls_cleared_global_tokens() {
        assert_eq!(
            global_monitor_action(true, GlobalMonitorState::default()),
            GlobalMonitorAction::InstallMissing
        );
    }
}
