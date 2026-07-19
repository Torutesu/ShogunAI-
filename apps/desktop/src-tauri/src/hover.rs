//! Hover adapter (spec §3.4). Event-driven; polling forbidden.
//!
//! The judgement lives in `spike_core::hover::HoverTracker` (unit-tested on Linux). This
//! adapter runs a listen-only CGEventTap on a dedicated CFRunLoop thread and forwards
//! [`TapEvent`]s. Design points (review findings #1/#3/#8 + spec §3.4.1):
//! - Mask covers MouseMoved AND LeftMouseDown/Up/Dragged so drag/menu suppression and the
//!   ButtonDown cancel actually receive their inputs.
//! - Early reject happens IN the tap callback: moves outside the top band are dropped
//!   (with one edge sample sent on band exit so the tracker sees the leave), keeping the
//!   per-mouse-event cost near zero during ordinary use (Q3 CPU budget).
//! - `TapDisabledByTimeout`/`ByUserInput` are handled: the tap is re-enabled and the
//!   incident is surfaced as `TapEvent::Status` (a dead tap must never be silent).
//! - Missing Accessibility permission retries every 3s instead of parking forever, so
//!   granting permission recovers without an app restart.
#![allow(dead_code, unused_imports)]

pub use spike_core::hover::{HoverParams, HoverSignal, HoverTracker};

#[cfg(target_os = "macos")]
pub use mac::{start, TapEvent};

#[cfg(target_os = "macos")]
mod mac {
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicPtr, Ordering};
    use std::sync::mpsc::Sender;
    use std::time::Duration;

    use objc2_core_foundation::{kCFRunLoopCommonModes, CFMachPort, CFRunLoop};
    use objc2_core_graphics::{
        CGEvent, CGEventMask, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
        CGEventTapProxy, CGEventType,
    };

    /// Top band in CG coordinates (y grows downward from the primary display's top edge).
    /// Mirrors spike_core's 40pt early-reject band (spec §3.4.1).
    const TOP_BAND_CG: f64 = 40.0;

    /// Events forwarded from the tap, in CGEvent (top-left origin) coordinates.
    #[derive(Clone, Copy, Debug)]
    pub enum TapEvent {
        /// Pointer move (or drag) inside the top band — plus one edge sample on band exit.
        Moved { x: f64, y: f64, buttons: u32 },
        /// Left button down (anywhere — menubar suppression needs global downs).
        Down { x: f64, y: f64 },
        /// Left button up.
        Up,
        /// Tap lifecycle: false = system disabled it (recorded, then re-enabled), true =
        /// tap (re-)armed. Also false while Accessibility permission is missing.
        Status { active: bool },
    }

    /// Callback context. Single-threaded (the tap's run loop), so Cells suffice; the tap
    /// port is stashed post-creation for re-enabling from inside the callback.
    struct TapCtx {
        tx: Sender<TapEvent>,
        tap_port: AtomicPtr<c_void>,
        in_band: Cell<bool>,
        buttons: Cell<u32>,
    }

    unsafe extern "C-unwind" fn tap_cb(
        _proxy: CGEventTapProxy,
        etype: CGEventType,
        event: NonNull<CGEvent>,
        user: *mut c_void,
    ) -> *mut CGEvent {
        if user.is_null() {
            return event.as_ptr();
        }
        // SAFETY: `user` is the Box<TapCtx> leaked in `start`, alive for the run loop.
        let ctx = unsafe { &*(user as *const TapCtx) };

        // System disabled the tap (slow callback / user input): surface + re-enable.
        if etype == CGEventType::TapDisabledByTimeout || etype == CGEventType::TapDisabledByUserInput {
            let _ = ctx.tx.send(TapEvent::Status { active: false });
            let port = ctx.tap_port.load(Ordering::Acquire) as *const CFMachPort;
            if !port.is_null() {
                // SAFETY: port is the CFMachPort stored right after creation, retained for
                // the lifetime of the run loop below.
                unsafe { CGEvent::tap_enable(&*port, true) };
                let _ = ctx.tx.send(TapEvent::Status { active: true });
            }
            return event.as_ptr();
        }

        if etype == CGEventType::LeftMouseDown {
            let loc = unsafe { CGEvent::location(Some(event.as_ref())) };
            ctx.buttons.set(1);
            let _ = ctx.tx.send(TapEvent::Down { x: loc.x, y: loc.y });
        } else if etype == CGEventType::LeftMouseUp {
            ctx.buttons.set(0);
            let _ = ctx.tx.send(TapEvent::Up);
        } else if etype == CGEventType::MouseMoved || etype == CGEventType::LeftMouseDragged {
            let loc = unsafe { CGEvent::location(Some(event.as_ref())) };
            let inside = loc.y <= TOP_BAND_CG;
            // Early reject: below the band AND already known-outside → zero further work.
            // One edge sample passes on band exit so HoverTracker sees the leave.
            if inside || ctx.in_band.get() {
                ctx.in_band.set(inside);
                let _ = ctx.tx.send(TapEvent::Moved { x: loc.x, y: loc.y, buttons: ctx.buttons.get() });
            }
        }
        event.as_ptr()
    }

    /// Install the tap on a dedicated CFRunLoop thread, forwarding events to `tx`.
    /// Retries every 3s while Accessibility permission is missing (recovers without a
    /// restart once granted). Returns immediately.
    pub fn start(tx: Sender<TapEvent>) {
        std::thread::spawn(move || {
            let ctx = Box::into_raw(Box::new(TapCtx {
                tx,
                tap_port: AtomicPtr::new(std::ptr::null_mut()),
                in_band: Cell::new(false),
                buttons: Cell::new(0),
            }));
            // SAFETY: standard CGEventTap install; ctx outlives the run loop (leaked).
            unsafe {
                let mask: CGEventMask = (1u64 << (CGEventType::MouseMoved.0 as u64))
                    | (1u64 << (CGEventType::LeftMouseDown.0 as u64))
                    | (1u64 << (CGEventType::LeftMouseUp.0 as u64))
                    | (1u64 << (CGEventType::LeftMouseDragged.0 as u64));
                let mut warned = false;
                let tap = loop {
                    match CGEvent::tap_create(
                        CGEventTapLocation::HIDEventTap,
                        CGEventTapPlacement::HeadInsertEventTap,
                        CGEventTapOptions::ListenOnly,
                        mask,
                        Some(tap_cb),
                        ctx as *mut c_void,
                    ) {
                        Some(t) => break t,
                        None => {
                            if !warned {
                                eprintln!("[spike] CGEventTap create failed — grant Accessibility permission (retrying every 3s)");
                                let _ = (*ctx).tx.send(TapEvent::Status { active: false });
                                warned = true;
                            }
                            std::thread::sleep(Duration::from_secs(3));
                        }
                    }
                };
                // Stash the port for in-callback re-enabling. `tap` (CFRetained) stays
                // alive on this stack frame for the whole run loop.
                (*ctx).tap_port.store(&*tap as *const CFMachPort as *mut c_void, Ordering::Release);

                let Some(source) = CFMachPort::new_run_loop_source(None, Some(&tap), 0) else {
                    return;
                };
                let Some(rl) = CFRunLoop::current() else {
                    return;
                };
                rl.add_source(Some(&source), kCFRunLoopCommonModes);
                CGEvent::tap_enable(&tap, true);
                let _ = (*ctx).tx.send(TapEvent::Status { active: true });
                CFRunLoop::run();
            }
        });
    }
}
