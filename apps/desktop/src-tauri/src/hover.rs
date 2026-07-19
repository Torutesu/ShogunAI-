//! Hover adapter (spec §3.4). Event-driven; polling forbidden.
//!
//! The judgement lives in `spike_core::hover::HoverTracker` (early-reject, 16ms coalesce,
//! velocity/fast-dwell, menu/drag suppression — unit-tested on Linux). This adapter runs a
//! listen-only CGEventTap (`kCGEventMouseMoved`, research item 2) on a dedicated CFRunLoop
//! thread and forwards raw pointer samples. on-device (T-07): the consumer normalises each
//! point to NS (`geometry::cg_to_ns`), feeds `HoverTracker`, and routes the emitted
//! `HoverSignal`s to `statemachine`. No allocation/log I/O in the tap callback (Q3 CPU).
//! Requires Accessibility permission (research item 3).
#![allow(dead_code, unused_imports)]

pub use spike_core::hover::{HoverParams, HoverSignal, HoverTracker};

#[cfg(target_os = "macos")]
pub use mac::{start, MouseSample};

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::mpsc::Sender;

    use objc2_core_foundation::{kCFRunLoopCommonModes, CFMachPort, CFRunLoop};
    use objc2_core_graphics::{
        CGEvent, CGEventGetLocation, CGEventMask, CGEventTapCreate, CGEventTapEnable,
        CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
    };

    /// A raw pointer sample from the tap, in CGEvent (top-left origin) coordinates.
    #[derive(Clone, Copy, Debug)]
    pub struct MouseSample {
        pub x: f64,
        pub y: f64,
    }

    // Called by the system on the tap thread. `user` is the leaked Sender pointer.
    unsafe extern "C-unwind" fn tap_cb(
        _proxy: CGEventTapProxy,
        _etype: CGEventType,
        event: NonNull<CGEvent>,
        user: *mut c_void,
    ) -> *mut CGEvent {
        if !user.is_null() {
            // SAFETY: `user` is the Box<Sender> pointer leaked in `start`, alive for the loop.
            let tx = unsafe { &*(user as *const Sender<MouseSample>) };
            let loc = unsafe { CGEventGetLocation(Some(event.as_ref())) };
            let _ = tx.send(MouseSample { x: loc.x, y: loc.y });
        }
        event.as_ptr()
    }

    /// Install a listen-only mouse-move CGEventTap on a dedicated CFRunLoop thread, forwarding
    /// raw samples to `tx`. Returns immediately; the thread runs the loop. If Accessibility
    /// permission is missing, tap creation returns None and the thread logs and exits.
    pub fn start(tx: Sender<MouseSample>) {
        std::thread::spawn(move || {
            // Leak a stable pointer to the Sender for user_info (lives for the run loop).
            let user = Box::into_raw(Box::new(tx)) as *mut c_void;
            // SAFETY: standard CGEventTap install sequence; pointers valid for the loop.
            unsafe {
                let mask: CGEventMask = 1u64 << (CGEventType::MouseMoved.0 as u64);
                let tap = match CGEventTapCreate(
                    CGEventTapLocation::HIDEventTap,
                    CGEventTapPlacement::HeadInsertEventTap,
                    CGEventTapOptions::ListenOnly,
                    mask,
                    Some(tap_cb),
                    user,
                ) {
                    Some(t) => t,
                    None => {
                        eprintln!("[spike] CGEventTap create failed — grant Accessibility permission");
                        return;
                    }
                };
                let Some(source) = CFMachPort::new_run_loop_source(None, Some(&tap), 0) else {
                    return;
                };
                let Some(rl) = CFRunLoop::current() else {
                    return;
                };
                rl.add_source(Some(&source), kCFRunLoopCommonModes);
                CGEventTapEnable(&tap, true);
                CFRunLoop::run();
            }
        });
    }
}
