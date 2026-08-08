//! Visual recall summon: pressing BOTH Command keys (left ⌘ + right ⌘) together opens the
//! screenshot-history browse window.
//!
//! A bare-modifier chord cannot be a global-shortcut-plugin combo (those need a real key), so
//! this uses NSEvent flagsChanged monitors — the same pattern as hold-to-talk
//! (voice_shortcut.rs). Left/right distinction comes from the DEVICE-DEPENDENT low bits of
//! `modifierFlags` (NX_DEVICELCMDKEYMASK / NX_DEVICERCMDKEYMASK): on a flagsChanged event
//! `keyCode` only says which key moved, while the device bits say which Command keys are held
//! right now — exactly what a two-key chord needs.
//!
//! Edge-triggered with re-arm: the chord fires once when the second ⌘ lands and cannot fire
//! again until at least one ⌘ is released, so holding the pair doesn't reopen/refocus in a loop.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;

/// Monitor registration mask (`addGlobalMonitorForEventsMatchingMask:`).
const MASK_FLAGS_CHANGED: usize = 1 << 12;

/// Device-INDEPENDENT modifier bits (shared with voice_shortcut.rs).
const FLAG_SHIFT: usize = 1 << 17;
const FLAG_CONTROL: usize = 1 << 18;
const FLAG_OPTION: usize = 1 << 19;
const FLAG_COMMAND: usize = 1 << 20;
const FLAG_FUNCTION: usize = 1 << 23;

/// Device-DEPENDENT bits in the low word of `modifierFlags` (IOKit NX_DEVICE*KEYMASK).
const DEVICE_LEFT_CMD: usize = 0x0008;
const DEVICE_RIGHT_CMD: usize = 0x0010;

static FIRED: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);

fn normalize_mods(flags: usize) -> usize {
    flags & (FLAG_SHIFT | FLAG_CONTROL | FLAG_OPTION | FLAG_COMMAND | FLAG_FUNCTION)
}

fn on_flags(app: &AppHandle, ev: *mut objc2::runtime::AnyObject) {
    use objc2::msg_send;
    // SAFETY: NSEvent pointer from AppKit monitor callback.
    let flags: usize = unsafe { msg_send![ev, modifierFlags] };
    let both = flags & DEVICE_LEFT_CMD != 0 && flags & DEVICE_RIGHT_CMD != 0;
    // Strict chord: ONLY Command among the standard modifiers. ⌘⌘ with Shift/Control/Option held
    // is some other gesture in flight, not a summon.
    if both && normalize_mods(flags) == FLAG_COMMAND {
        if !FIRED.swap(true, Ordering::SeqCst) {
            eprintln!("[recall] ⌘⌘ chord — opening visual recall");
            let app = app.clone();
            let handle = app.clone();
            // Monitors fire on the main thread, but route through run_on_main_thread anyway so a
            // future off-main caller cannot build a window from the wrong thread.
            let _ = handle.run_on_main_thread(move || crate::build_visual_recall_window(&app));
        }
    } else if !both {
        // Re-arm only when a Command key actually lifted — releasing Shift while both ⌘ stay
        // down must not queue a second fire.
        FIRED.store(false, Ordering::SeqCst);
    }
}

/// Install global + local NSEvent flagsChanged monitors for the ⌘⌘ chord.
pub fn install(app: &tauri::AppHandle) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    // Hot-reload / double setup must not stack monitors (see voice_shortcut.rs).
    if INSTALLED.swap(true, Ordering::SeqCst) {
        eprintln!("[recall] ⌘⌘ summon already installed — skip");
        return;
    }

    let handle_g = app.clone();
    let handle_l = app.clone();

    // SAFETY: main thread (setup); monitors intentionally leaked for app lifetime.
    unsafe {
        // Global monitor: void (NSEvent *) → void.
        let global = block2::RcBlock::new(move |ev: *mut AnyObject| {
            if ev.is_null() {
                return;
            }
            on_flags(&handle_g, ev);
        });
        // Local monitor: NSEvent * → NSEvent * (must return the event to pass it through —
        // a void block here is an ABI mismatch that crashes, see voice_shortcut.rs).
        let local = block2::RcBlock::new(move |ev: *mut AnyObject| -> *mut AnyObject {
            if !ev.is_null() {
                on_flags(&handle_l, ev);
            }
            ev
        });

        let g: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_FLAGS_CHANGED,
            handler: &*global
        ];
        let l: *mut AnyObject = msg_send![
            class!(NSEvent),
            addLocalMonitorForEventsMatchingMask: MASK_FLAGS_CHANGED,
            handler: &*local
        ];

        if g.is_null() {
            eprintln!("[recall] global monitor failed (accessibility?)");
        }
        if l.is_null() {
            eprintln!("[recall] local monitor unavailable");
        }

        std::mem::forget(global);
        std::mem::forget(local);
        eprintln!("[recall] ⌘⌘ summon installed (left+right Command)");
    }
}
