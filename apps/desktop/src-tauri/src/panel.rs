//! Native panel state, window management, and residency recovery.

use crate::*;

/// The collectionBehavior the overlay wants, selected at setup (NSPanel mode = canJoinAllSpaces +
/// fullScreenAuxiliary = 257; plain-window fallback = moveToActiveSpace 274) and re-asserted by
/// every heal/reassert path. `stationary` (1<<4) was dropped: it is a suspect for the panel not
/// tracking Space switches on this machine, and the reference overlays run without it.
#[cfg(target_os = "macos")]
pub(crate) static PANEL_BEHAVIOR: std::sync::atomic::AtomicUsize =
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
pub(crate) static CASTLE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The current Castle Position read from the atomic (default `Notch`).
#[cfg(target_os = "macos")]
pub(crate) fn current_castle() -> shogun_core::notch::geometry::CastlePosition {
    shogun_core::notch::geometry::CastlePosition::from_u8(
        CASTLE.load(std::sync::atomic::Ordering::Relaxed),
    )
}

/// Legacy user-dragged override. New builds never populate it; `castle::init` clears old persisted
/// values so every placement path resolves to the selected Castle Position.
#[cfg(target_os = "macos")]
pub(crate) static DRAG_OVERRIDE: std::sync::Mutex<
    Option<shogun_core::notch::geometry::DragOffset>,
> = std::sync::Mutex::new(None);

/// True while our code is moving the panel. Retained as the single movement boundary for the
/// dock/resize paths even though user-driven panel movement is disabled.
#[cfg(target_os = "macos")]
static PROGRAMMATIC_MOVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The drag override, if any (poisoned lock reads as "no override" — never panic in the shell).
#[cfg(target_os = "macos")]
pub(crate) fn current_drag_override() -> Option<shogun_core::notch::geometry::DragOffset> {
    DRAG_OVERRIDE.lock().ok().and_then(|g| *g)
}

/// (main thread) Where the panel rests on `screen`: the user's dragged spot when one exists
/// (issue #21), else the Castle Position dock (issue #20; Notch welds under the hardware notch).
/// Both paths clamp on-screen, so a display change can only pull the panel back on screen.
///
/// SAFETY: `screen` must be a live `NSScreen*`; called on the main thread.
#[cfg(target_os = "macos")]
pub(crate) unsafe fn resting_dock_origin(
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
pub(crate) fn with_programmatic_move<R>(f: impl FnOnce() -> R) -> R {
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
pub(crate) fn toggle_panel(handle: &tauri::AppHandle) {
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
pub(crate) fn set_panel_hidden(handle: &tauri::AppHandle) {
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
pub(crate) unsafe fn pin_top_centre(ptr: *mut objc2::runtime::AnyObject) {
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
        const { assert!(!PANEL_MOVABLE) };
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
pub(crate) fn build_panel_window(handle: &tauri::AppHandle) {
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
pub(crate) fn build_scribe_window(
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
pub(crate) fn report_panel_health(app: &tauri::AppHandle) {
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
pub(crate) fn adopt_native_panel(win: &tauri::WebviewWindow) {
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
pub(crate) fn set_panel_size(
    app: tauri::AppHandle,
    width: f64,
    height: f64,
    anchor: Option<String>,
) {
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
pub(crate) fn summon_to_active_space(app: &tauri::AppHandle) {
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
pub(crate) fn watch_space_changes(app: &tauri::App) {
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
pub(crate) fn spawn_panel_state_logger(app: &tauri::App) {
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
