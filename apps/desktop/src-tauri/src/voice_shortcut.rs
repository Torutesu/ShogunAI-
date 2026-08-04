//! Hold-to-talk shortcut via NSEvent global + local monitors (#44).
//!
//! Global shortcuts only fire on press; hold/release needs NSEvent (same pattern as ⌥-tap and
//! Wispr/Hush). The combo is persisted in shortcuts.json (`voice` action) and read live.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;

const MASK_KEY_DOWN: usize = 1 << 10;
const MASK_KEY_UP: usize = 1 << 11;
const MASK_FLAGS_CHANGED: usize = 1 << 12;

const FLAG_SHIFT: usize = 1 << 17;
const FLAG_CONTROL: usize = 1 << 18;
const FLAG_OPTION: usize = 1 << 19;
const FLAG_COMMAND: usize = 1 << 20;
const FLAG_FUNCTION: usize = 1 << 23;

struct Combo {
    modifiers: usize,
    key_code: u16,
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
    Some(Combo { modifiers: mods, key_code: key_code? })
}

fn normalize_mods(flags: usize) -> usize {
    flags & (FLAG_SHIFT | FLAG_CONTROL | FLAG_OPTION | FLAG_COMMAND | FLAG_FUNCTION)
}

fn combo_matches(flags: usize, key: u16, combo: &Combo) -> bool {
    normalize_mods(flags) == combo.modifiers && key == combo.key_code
}

fn current_voice_combo(app: &AppHandle) -> Combo {
    let fallback = parse_combo("Control+Alt+KeyV").expect("default voice combo");
    let combo = crate::shortcuts::binding(app, "voice").unwrap_or_else(|| "Control+Alt+KeyV".into());
    parse_combo(&combo).unwrap_or(fallback)
}

static HOLDING: AtomicBool = AtomicBool::new(false);

/// Install global + local NSEvent monitors for the configured hold-to-talk combo.
pub fn install(app: &tauri::AppHandle) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    let handle_start = app.clone();
    let handle_stop = app.clone();

    // SAFETY: main thread (setup); monitors intentionally leaked for app lifetime.
    unsafe {
        let start_block = block2::RcBlock::new(move |ev: *mut AnyObject| {
            if ev.is_null() || HOLDING.load(Ordering::SeqCst) {
                return;
            }
            let combo = current_voice_combo(&handle_start);
            let flags: usize = msg_send![ev, modifierFlags];
            let key: u16 = msg_send![ev, keyCode];
            if combo_matches(flags, key, &combo) {
                HOLDING.store(true, Ordering::SeqCst);
                // NSEvent monitor thread — never block here (whisper load / mic / transcribe).
                let app = handle_start.clone();
                std::thread::spawn(move || crate::voice_session::mac::on_hold_start(app));
            }
        });

        let stop_block = block2::RcBlock::new(move |ev: *mut AnyObject| {
            if ev.is_null() || !HOLDING.load(Ordering::SeqCst) {
                return;
            }
            let combo = current_voice_combo(&handle_stop);
            let flags: usize = msg_send![ev, modifierFlags];
            let ty: usize = msg_send![ev, type];
            let release = if ty == MASK_KEY_UP {
                let key: u16 = msg_send![ev, keyCode];
                key == combo.key_code
            } else if ty == MASK_FLAGS_CHANGED {
                normalize_mods(flags) != combo.modifiers
            } else {
                false
            };
            if release {
                HOLDING.store(false, Ordering::SeqCst);
                let app = handle_stop.clone();
                std::thread::spawn(move || crate::voice_session::mac::on_hold_end(app));
            }
        });

        for (mask, block) in [
            (MASK_KEY_DOWN, &*start_block),
            (MASK_KEY_UP | MASK_FLAGS_CHANGED, &*stop_block),
        ] {
            let global: *mut AnyObject = msg_send![
                class!(NSEvent),
                addGlobalMonitorForEventsMatchingMask: mask,
                handler: block
            ];
            let local: *mut AnyObject = msg_send![
                class!(NSEvent),
                addLocalMonitorForEventsMatchingMask: mask,
                handler: block
            ];
            if global.is_null() {
                eprintln!("[voice] hold monitor mask={mask} failed (accessibility?)");
            }
            if local.is_null() {
                eprintln!("[voice] local hold monitor mask={mask} unavailable");
            }
        }
        std::mem::forget(start_block);
        std::mem::forget(stop_block);
        eprintln!("[voice] hold-to-talk installed (⌃⌥V default)");
    }
}
