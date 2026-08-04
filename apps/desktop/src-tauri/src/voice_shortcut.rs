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

struct Combo {
    modifiers: usize,
    key_code: u16,
}

enum Cmd {
    Start(AppHandle),
    End(AppHandle),
}

static HOLDING: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);
static CMD_TX: Mutex<Option<Sender<Cmd>>> = Mutex::new(None);

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

fn current_modifier_flags() -> usize {
    use objc2::{class, msg_send};
    // SAFETY: NSEvent class method; readable from any thread on modern macOS.
    let flags: usize = unsafe { msg_send![class!(NSEvent), modifierFlags] };
    normalize_mods(flags)
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
            if crate::voice_session::mac::force_end_if_recording(app) {
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
                let started = crate::voice_session::mac::on_hold_start(app.clone());
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
                crate::voice_session::mac::on_hold_end(app.clone());
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
                            eprintln!(
                                "[voice] hold release (mod poll mods={})",
                                mods_label(mods)
                            );
                            hold_started = None;
                            let app = app.clone();
                            timeout_app = None;
                            crate::voice_session::mac::on_hold_end(app.clone());
                            schedule_end_failsafe(app);
                            continue;
                        }
                    }
                    if started.elapsed() >= MAX_HOLD && HOLDING.swap(false, Ordering::SeqCst) {
                        eprintln!("[voice] hold auto-end after {}s", MAX_HOLD.as_secs());
                        hold_started = None;
                        let app = app.clone();
                        timeout_app = None;
                        crate::voice_session::mac::on_hold_end(app.clone());
                        schedule_end_failsafe(app);
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Install global + local NSEvent monitors for the configured hold-to-talk combo.
pub fn install(app: &tauri::AppHandle) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    // Hot-reload / double setup must not stack monitors — that reintroduced the release SIGSEGV.
    if INSTALLED.swap(true, Ordering::SeqCst) {
        eprintln!("[voice] hold-to-talk already installed — skip");
        return;
    }

    let (tx, rx) = mpsc::channel::<Cmd>();
    if let Ok(mut slot) = CMD_TX.lock() {
        *slot = Some(tx);
    }
    std::thread::Builder::new()
        .name("voice-hold".into())
        .spawn(move || worker_loop(rx))
        .ok();

    let handle_g_start = app.clone();
    let handle_g_stop = app.clone();
    let handle_l_start = app.clone();
    let handle_l_stop = app.clone();

    // SAFETY: main thread (setup); monitors intentionally leaked for app lifetime.
    unsafe {
        // Global monitors: void (NSEvent *) → void
        let global_start = block2::RcBlock::new(move |ev: *mut AnyObject| {
            if ev.is_null() {
                return;
            }
            on_press(&handle_g_start, ev);
        });
        let global_stop = block2::RcBlock::new(move |ev: *mut AnyObject| {
            if ev.is_null() {
                return;
            }
            on_release(&handle_g_stop, ev);
        });

        // Local monitors: NSEvent * → NSEvent * (must return the event or nil to swallow).
        // Passing a void block here crashes on invoke — that was the release-time abort.
        let local_start = block2::RcBlock::new(move |ev: *mut AnyObject| -> *mut AnyObject {
            if !ev.is_null() {
                on_press(&handle_l_start, ev);
            }
            ev
        });
        let local_stop = block2::RcBlock::new(move |ev: *mut AnyObject| -> *mut AnyObject {
            if !ev.is_null() {
                on_release(&handle_l_stop, ev);
            }
            ev
        });

        let g_down: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_KEY_DOWN,
            handler: &*global_start
        ];
        let g_up: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_KEY_UP | MASK_FLAGS_CHANGED,
            handler: &*global_stop
        ];
        let l_down: *mut AnyObject = msg_send![
            class!(NSEvent),
            addLocalMonitorForEventsMatchingMask: MASK_KEY_DOWN,
            handler: &*local_start
        ];
        let l_up: *mut AnyObject = msg_send![
            class!(NSEvent),
            addLocalMonitorForEventsMatchingMask: MASK_KEY_UP | MASK_FLAGS_CHANGED,
            handler: &*local_stop
        ];

        if g_down.is_null() || g_up.is_null() {
            eprintln!("[voice] hold global monitor failed (accessibility?)");
        }
        if l_down.is_null() || l_up.is_null() {
            eprintln!("[voice] hold local monitor unavailable");
        }

        std::mem::forget(global_start);
        std::mem::forget(global_stop);
        std::mem::forget(local_start);
        std::mem::forget(local_stop);
        eprintln!("[voice] hold-to-talk installed (⌃⌥V default)");
    }
}
