//! Hover adapter (spec §3.4). Event-driven; polling forbidden.
//!
//! The judgement lives in `shogun_core::notch::hover::HoverTracker` (unit-tested on Linux). This
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

pub use shogun_core::notch::hover::{HoverParams, HoverSignal, HoverTracker};

#[cfg(target_os = "macos")]
pub use mac::{set_hover_band_cg, start, TapEvent};

#[cfg(target_os = "macos")]
mod mac {
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
    use std::sync::mpsc::Sender;
    use std::time::Duration;

    use objc2_core_foundation::{kCFRunLoopCommonModes, CFMachPort, CFRunLoop};
    use objc2_core_graphics::{
        CGEvent, CGEventMask, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
        CGEventTapProxy, CGEventType,
    };

    /// Default Idle early-reject zone before geometry arrives (spec §3.4.1 silhouette ≈180×40).
    /// Grown to live panel size while open so moves into the Expanded body still reach HoverTracker.
    const DEFAULT_BAND_H_CG: f64 = 40.0;
    const DEFAULT_BAND_W_CG: f64 = 184.0; // ~180 notch + 2pt pad each side
    // f64::to_bits is not const on the workspace MSRV (1.80) — these are the IEEE-754 bit
    // patterns of DEFAULT_BAND_H_CG (40.0) and DEFAULT_BAND_W_CG (184.0).
    static HOVER_BAND_H_BITS: AtomicU64 = AtomicU64::new(0x4044_0000_0000_0000);
    static HOVER_BAND_W_BITS: AtomicU64 = AtomicU64::new(0x4067_0000_0000_0000);

    /// Set the CGEventTap early-reject zone: `height` points down from each display's top,
    /// `width` centred on that display (Idle = notch silhouette; open = panel + grace).
    /// No full-menu-bar strip — X must stay under the visible notch/panel.
    pub fn set_hover_band_cg(height: f64, width: f64) {
        let h = if height.is_finite() && height > 0.0 {
            height
        } else {
            DEFAULT_BAND_H_CG
        };
        let w = if width.is_finite() && width > 0.0 {
            width
        } else {
            DEFAULT_BAND_W_CG
        };
        HOVER_BAND_H_BITS.store(h.to_bits(), Ordering::Relaxed);
        HOVER_BAND_W_BITS.store(w.to_bits(), Ordering::Relaxed);
    }

    fn hover_band_h_cg() -> f64 {
        f64::from_bits(HOVER_BAND_H_BITS.load(Ordering::Relaxed))
    }

    fn hover_band_w_cg() -> f64 {
        f64::from_bits(HOVER_BAND_W_BITS.load(Ordering::Relaxed))
    }

    /// The top edge, in CG coordinates, of the display containing `x, y`, plus that display's
    /// horizontal centre (for notch-centred X early-reject).
    ///
    /// Uses CoreGraphics rather than NSScreen because this runs on the tap's own CFRunLoop thread,
    /// where AppKit is not safe to touch. Falls back to (0.0, x) when the point is on no known
    /// display, which keeps prior Y behaviour rather than dropping the event.
    fn display_top_and_mid_cg(x: f64, y: f64) -> (f64, f64) {
        use objc2_core_graphics::{CGDisplayBounds, CGGetDisplaysWithPoint};
        let point = objc2_core_foundation::CGPoint { x, y };
        let mut ids: [u32; 8] = [0; 8];
        let mut count: u32 = 0;
        // SAFETY: both are plain C calls; the buffer is sized by `ids.len()` and `count` receives
        // however many were written, which is what the loop below reads.
        let ok = unsafe {
            CGGetDisplaysWithPoint(point, ids.len() as u32, ids.as_mut_ptr(), &mut count)
        };
        if ok != objc2_core_graphics::CGError::Success || count == 0 {
            return (0.0, x);
        }
        let bounds = CGDisplayBounds(ids[0]);
        let mid_x = bounds.origin.x + bounds.size.width / 2.0;
        (bounds.origin.y, mid_x)
    }

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
            let (top_y, mid_x) = display_top_and_mid_cg(loc.x, loc.y);
            let half_w = hover_band_w_cg() / 2.0;
            let inside = (loc.y - top_y) <= hover_band_h_cg()
                && (loc.x - mid_x).abs() <= half_w;
            // Early reject: outside the notch/panel silhouette AND already known-outside → zero
            // further work. One edge sample passes on band exit so HoverTracker sees the leave.
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
