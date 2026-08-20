//! Floating meeting overlay windows and their screen geometry.

use serde::Serialize;
use shogun_core::meeting::settings::Settings;
use shogun_core::meeting::statemachine::{Input, State};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};

use super::state::{emit, now_ms, step, LANE};

const WINDOW_LABEL: &str = "meeting";
const WIN_CC: &str = "meeting-cc";
const WIN_CANVAS: &str = "meeting-canvas";
const WIN_CHAT: &str = "meeting-chat";
/// Offered: white horizontal pill (Meeting detected). Room for title stack + Take Notes.
const OFFER_SIZE: (f64, f64) = (440.0, 96.0);
/// Idle / hidden fallback size.
const BAR_SIZE: (f64, f64) = (400.0, 88.0);
/// In-meeting black control capsule only (content panels are separate windows).
/// Height includes top inset for bar-slot tooltips (they sit above the 52px bar).
const PILL_SIZE: (f64, f64) = (320.0, 100.0);
/// Host pill enlarged so `.ov__modemenu--bar` (+ lang row) fits above the capsule.
/// Rough: tip pad 36 + menu (~3×36 + lang row + pad) + gap 8 + pill 52 + bottom pad 8 ≈ 280.
const PILL_WITH_MENU_SIZE: (f64, f64) = (320.0, 280.0);
/// Host bar mode-menu open — `recording_overlay_size` must grow or sync fights the FE.
static HOST_MENU_OPEN: AtomicBool = AtomicBool::new(false);
/// Live captions window default (own NSWindow — not stacked in the host).
const LIVE_SIZE: (f64, f64) = (520.0, 300.0);
/// AI Canvas window default.
const CANVAS_SIZE: (f64, f64) = (380.0, 320.0);
/// AI Chat window default (shorter than CC so three panels fit typical MacBook heights).
const CHAT_SIZE: (f64, f64) = (320.0, 380.0);
const RECAP_SIZE: (f64, f64) = (420.0, 520.0);
/// Whether the live captions panel window is open.
static OVERLAY_PANEL_OPEN: AtomicBool = AtomicBool::new(true);
/// Whether the AI Canvas panel window is open.
static OVERLAY_CANVAS_OPEN: AtomicBool = AtomicBool::new(false);
/// Whether the AI Chat panel window is open.
static OVERLAY_CHAT_OPEN: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, Default)]
struct PanelSizes {
    cc: Option<(f64, f64)>,
    canvas: Option<(f64, f64)>,
    chat: Option<(f64, f64)>,
}

#[derive(Debug, Clone, Copy, Default)]
struct PanelParked {
    cc: bool,
    canvas: bool,
    chat: bool,
}

/// Per-panel user resize (logical px). Independent windows — never one shared desk size.
static PANEL_CUSTOM_SIZE: std::sync::Mutex<PanelSizes> = std::sync::Mutex::new(PanelSizes {
    cc: None,
    canvas: None,
    chat: None,
});
static PANEL_PARKED: std::sync::Mutex<PanelParked> = std::sync::Mutex::new(PanelParked {
    cc: false,
    canvas: false,
    chat: false,
});
const OVERLAY_SIZE_MIN: (f64, f64) = (280.0, 180.0);
const OVERLAY_SIZE_MAX: (f64, f64) = (720.0, 900.0);
/// Distance from screen edges, in logical pixels.
const MARGIN: f64 = 16.0;
/// Menu-bar height to clear for top-right parking.
const MENUBAR_H: f64 = 28.0;
/// Distance from the bottom of the visible screen — clears Meet/Zoom mic bar.
const BOTTOM_MARGIN: f64 = 136.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParkMode {
    TopRight,
    BottomCenter,
}

fn park_mode_for_state(state: State) -> ParkMode {
    match state {
        State::Recording => ParkMode::BottomCenter,
        // Offer card and Recap are notification-style surfaces, not in-meeting controls.
        State::Offered | State::Wrapping | State::Idle => ParkMode::TopRight,
    }
}

/// Build host + independent panel windows (hidden). **Setup only — main thread.**
pub(super) fn build_overlay(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    // Every meeting surface is its own tight transparent window (glass chrome in CSS).
    // Never one shared transparent desk that blocks clicks in empty space.
    let host = build_one_overlay(app, WINDOW_LABEL, BAR_SIZE, "ShogunAI — meeting", false)?;
    let _ = build_one_overlay(app, WIN_CC, LIVE_SIZE, "ShogunAI — captions", false);
    let _ = build_one_overlay(app, WIN_CANVAS, CANVAS_SIZE, "ShogunAI — canvas", false);
    let _ = build_one_overlay(app, WIN_CHAT, CHAT_SIZE, "ShogunAI — chat", false);
    Some(host)
}

fn build_one_overlay(
    app: &tauri::AppHandle,
    label: &str,
    size: (f64, f64),
    title: &str,
    opaque: bool,
) -> Option<tauri::WebviewWindow> {
    if let Some(win) = app.get_webview_window(label) {
        return Some(win);
    }
    let win = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::default())
        .title(title)
        .transparent(!opaque)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .shadow(false)
        .skip_taskbar(true)
        .inner_size(size.0, size.1)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|e| eprintln!("[meeting] overlay `{label}` build failed: {e}"))
        .ok()?;
    configure_overlay_window(&win, opaque);
    eprintln!(
        "[meeting] overlay `{label}` url = {:?} opaque={opaque}",
        win.url().map(|u| u.to_string())
    );
    Some(win)
}

/// One-time NSWindow setup for a meeting overlay. Deliberately NOT `float_on_all_spaces`:
/// that helper orders the window front and sets `canHide=false` / `movableByWindowBackground`,
/// which left a transparent full-window hit target blocking the desktop even when the lane
/// was Idle and `hide()` had been called.
fn configure_overlay_window(win: &tauri::WebviewWindow, opaque: bool) {
    use objc2::class;
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use std::sync::atomic::Ordering;

    let ptr = match win.ns_window() {
        Ok(p) if !p.is_null() => p as *mut AnyObject,
        _ => {
            eprintln!("[meeting] ns_window unavailable — overlay may not float correctly");
            return;
        }
    };
    let behavior = crate::PANEL_BEHAVIOR.load(Ordering::Relaxed);
    let level = crate::OVERLAY_LEVEL;
    // SAFETY: live NSWindow on the main thread (setup).
    unsafe {
        if opaque {
            let _: () = msg_send![ptr, setOpaque: true];
            let black: *mut AnyObject = msg_send![class!(NSColor), blackColor];
            let _: () = msg_send![ptr, setBackgroundColor: black];
        } else {
            // Transparent frame: clear NSWindow backing so CSS glass + border-radius show
            // Meet behind. Window stays sized to chrome only — no desk padding.
            let _: () = msg_send![ptr, setOpaque: false];
            let clear: *mut AnyObject = msg_send![class!(NSColor), clearColor];
            let _: () = msg_send![ptr, setBackgroundColor: clear];
        }
        let _: () = msg_send![ptr, setHasShadow: false];
        let _: () = msg_send![ptr, setCollectionBehavior: behavior];
        let _: () = msg_send![ptr, setLevel: level];
        let _: () = msg_send![ptr, setHidesOnDeactivate: false];
        // Must stay hideable — the notch overlay sets canHide=false for residency, but this
        // window must disappear entirely when the lane is Idle.
        let _: () = msg_send![ptr, setCanHide: true];
        // Drag is via the grip strip and glass headers (`-webkit-app-region: drag` in CSS, or
        // `meeting_drag` → start_dragging). A movable background turns the whole frame into
        // an invisible click-catcher — transparent padding and rounded corners still block
        // clicks behind the window at the AppKit layer.
        let _: () = msg_send![ptr, setMovableByWindowBackground: false];
        // Start click-through until sync_window / show_content_panel shows real UI.
        let _: () = msg_send![ptr, setIgnoresMouseEvents: true];
    }
    eprintln!("[meeting] overlay window configured (hidden, click-through, opaque={opaque})");
}

fn overlay_ns_window(win: &tauri::WebviewWindow) -> Option<*mut objc2::runtime::AnyObject> {
    match win.ns_window() {
        Ok(p) if !p.is_null() => Some(p as *mut objc2::runtime::AnyObject),
        _ => None,
    }
}

/// Webview-owned desire: capture mouse on the glass card (`ignoresMouseEvents=false`).
static OVERLAY_WANTS_INTERACTIVE: AtomicBool = AtomicBool::new(false);

/// AppKit-only click-through toggle. Never orderOut here — visibility is sync_window's job.
fn set_overlay_ignores_mouse(win: &tauri::WebviewWindow, ignores: bool) {
    use objc2::msg_send;
    let Some(ptr) = overlay_ns_window(win) else {
        return;
    };
    // SAFETY: live NSWindow; called from the main thread via sync_window / setup.
    unsafe {
        let _: () = msg_send![ptr, setIgnoresMouseEvents: ignores];
    }
}

fn apply_overlay_interactive(win: &tauri::WebviewWindow) {
    let interactive = OVERLAY_WANTS_INTERACTIVE.load(Ordering::SeqCst);
    set_overlay_ignores_mouse(win, !interactive);
}

fn overlay_monitor(win: &tauri::WebviewWindow) -> Option<tauri::Monitor> {
    match win.current_monitor() {
        Ok(Some(m)) => Some(m),
        _ => match win.primary_monitor() {
            Ok(Some(m)) => Some(m),
            _ => {
                eprintln!("[meeting] no monitor to park the overlay on");
                None
            }
        },
    }
}

/// Park the overlay at the top-right of the screen the cursor is on (offer / recap).
///
/// Computed and set entirely in **physical** pixels — see `park_bottom_center`.
fn park_top_right(win: &tauri::WebviewWindow, size: (f64, f64)) {
    let Some(monitor) = overlay_monitor(win) else {
        return;
    };
    let scale = monitor.scale_factor();
    let screen = monitor.size();
    let origin = monitor.position();
    let w = (size.0 * scale).round() as i32;
    let margin = (MARGIN * scale).round() as i32;
    // Below the menu bar, so the overlay never fights the notch for the same pixels.
    let top = ((MARGIN + MENUBAR_H) * scale).round() as i32;

    let x = origin.x + screen.width as i32 - w - margin;
    let y = origin.y + top;
    eprintln!(
        "[meeting] park top-right ({x},{y}) physical — screen {}x{} at ({},{}) scale {scale}",
        screen.width, screen.height, origin.x, origin.y
    );
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Park the overlay bottom-center on the screen the cursor is on — above the Meet mic bar.
///
/// Re-parked when the window size changes (pill ↔ live panel) so the anchor stays centered.
/// After that the user may drag it; we do not move it until hide, resize, or offer → live.
///
/// Computed and set entirely in **physical** pixels. Mixing the two coordinate systems is
/// how the panel ended up in the middle of the screen: the monitor answers in physical
/// pixels, the window size is given in logical ones, and subtracting one from the other on a
/// Retina display is off by exactly the scale factor.
fn park_bottom_center(win: &tauri::WebviewWindow, size: (f64, f64)) {
    let Some(monitor) = overlay_monitor(win) else {
        return;
    };
    let scale = monitor.scale_factor();
    let screen = monitor.size();
    let origin = monitor.position();
    let w = (size.0 * scale).round() as i32;
    let h = (size.1 * scale).round() as i32;
    let bottom = (BOTTOM_MARGIN * scale).round() as i32;

    let x = origin.x + (screen.width as i32 - w) / 2;
    let y = origin.y + screen.height as i32 - h - bottom;
    eprintln!(
        "[meeting] park bottom-center ({x},{y}) physical — screen {}x{} at ({},{}) scale {scale}",
        screen.width, screen.height, origin.x, origin.y
    );
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
}

fn park_overlay(win: &tauri::WebviewWindow, mode: ParkMode, size: (f64, f64)) {
    match mode {
        ParkMode::TopRight => park_top_right(win, size),
        ParkMode::BottomCenter => park_bottom_center(win, size),
    }
}

/// Host window size only — content panels are separate windows.
fn recording_overlay_size() -> (f64, f64) {
    if HOST_MENU_OPEN.load(Ordering::SeqCst) {
        PILL_WITH_MENU_SIZE
    } else {
        PILL_SIZE
    }
}

/// Grow/shrink the host in place, keeping the bottom edge fixed (menu opens upward).
fn resize_host_keeping_bottom(win: &tauri::WebviewWindow, size: (f64, f64)) {
    let prev_pos = win.outer_position().ok();
    let prev_size = win.outer_size().ok();
    let scale = overlay_monitor(win)
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    let _ = win.set_size(tauri::LogicalSize::new(size.0, size.1));
    if let (Some(pos), Some(prev)) = (prev_pos, prev_size) {
        let prev_h = (prev.height as f64) / scale;
        let dy = ((size.1 - prev_h) * scale).round() as i32;
        if dy != 0 {
            let _ = win.set_position(tauri::PhysicalPosition::new(pos.x, pos.y - dy));
        }
    }
    let _ = win.set_size(tauri::LogicalSize::new(size.0, size.1));
}

fn panel_default_size(label: &str) -> (f64, f64) {
    match label {
        WIN_CC => LIVE_SIZE,
        WIN_CANVAS => CANVAS_SIZE,
        WIN_CHAT => CHAT_SIZE,
        _ => PILL_SIZE,
    }
}

fn panel_size(label: &'static str) -> (f64, f64) {
    if let Ok(guard) = PANEL_CUSTOM_SIZE.lock() {
        let custom = match label {
            WIN_CC => guard.cc,
            WIN_CANVAS => guard.canvas,
            WIN_CHAT => guard.chat,
            _ => None,
        };
        if let Some(size) = custom {
            return size;
        }
    }
    panel_default_size(label)
}

fn clear_custom_size_if_idle() {
    use std::sync::atomic::Ordering;
    if OVERLAY_PANEL_OPEN.load(Ordering::SeqCst)
        || OVERLAY_CANVAS_OPEN.load(Ordering::SeqCst)
        || OVERLAY_CHAT_OPEN.load(Ordering::SeqCst)
    {
        return;
    }
    if let Ok(mut g) = PANEL_CUSTOM_SIZE.lock() {
        *g = PanelSizes::default();
    }
    if let Ok(mut g) = PANEL_PARKED.lock() {
        *g = PanelParked::default();
    }
}

fn clamp_overlay_size(width: f64, height: f64) -> (f64, f64) {
    (
        width.clamp(OVERLAY_SIZE_MIN.0, OVERLAY_SIZE_MAX.0).round(),
        height.clamp(OVERLAY_SIZE_MIN.1, OVERLAY_SIZE_MAX.1).round(),
    )
}

fn hide_content_panels(app: &tauri::AppHandle) {
    for label in [WIN_CC, WIN_CANVAS, WIN_CHAT] {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.hide();
        }
    }
    if let Ok(mut g) = PANEL_PARKED.lock() {
        *g = PanelParked::default();
    }
}

#[derive(Debug, Clone, Copy)]
struct PanelPlacement {
    label: &'static str,
    x: i32,
    y: i32,
    size: (f64, f64),
}

/// Bottom-align above the recording pill; clamp top so tall chat stays below the menu bar.
fn content_panel_y(origin_y: i32, screen_h: i32, h: i32, bottom: i32, top_clear: i32) -> i32 {
    let y = origin_y + screen_h - h - bottom;
    y.max(origin_y + top_clear)
}

/// CC center, canvas left, chat right — x from actual panel widths, group-shifted to fit.
fn compute_content_panel_layout(monitor: &tauri::Monitor) -> Vec<PanelPlacement> {
    use std::sync::atomic::Ordering;

    let scale = monitor.scale_factor();
    let screen = monitor.size();
    let origin = monitor.position();
    let gap = (12.0 * scale).round() as i32;
    let margin = (MARGIN * scale).round() as i32;
    let bottom = ((BOTTOM_MARGIN + PILL_SIZE.1 + 16.0) * scale).round() as i32;
    let top_clear = ((MARGIN + MENUBAR_H) * scale).round() as i32;
    let cx = origin.x + (screen.width as i32) / 2;

    let cc_open = OVERLAY_PANEL_OPEN.load(Ordering::SeqCst);
    let canvas_open = OVERLAY_CANVAS_OPEN.load(Ordering::SeqCst);
    let chat_open = OVERLAY_CHAT_OPEN.load(Ordering::SeqCst);

    let cc_size = panel_size(WIN_CC);
    let cc_w = (cc_size.0 * scale).round() as i32;
    let cc_left = cx - cc_w / 2;
    let cc_right = cc_left + cc_w;

    let mut out = Vec::new();

    if cc_open {
        let cc_h = (cc_size.1 * scale).round() as i32;
        out.push(PanelPlacement {
            label: WIN_CC,
            x: cc_left,
            y: content_panel_y(origin.y, screen.height as i32, cc_h, bottom, top_clear),
            size: cc_size,
        });
    }

    // With captions closed there is no centre anchor to flank, so the survivors are centred as
    // a group. Centring them individually puts both at `cx - w/2` and lands canvas and chat on
    // top of each other whenever both are open without captions.
    let canvas_size = panel_size(WIN_CANVAS);
    let chat_size = panel_size(WIN_CHAT);
    let canvas_w = (canvas_size.0 * scale).round() as i32;
    let chat_w = (chat_size.0 * scale).round() as i32;
    let (loose_canvas_x, loose_chat_x) = if canvas_open && chat_open {
        let left = cx - (canvas_w + gap + chat_w) / 2;
        (left, left + canvas_w + gap)
    } else {
        (cx - canvas_w / 2, cx - chat_w / 2)
    };

    if canvas_open {
        let h = (canvas_size.1 * scale).round() as i32;
        let x = if cc_open {
            cc_left - gap - canvas_w
        } else {
            loose_canvas_x
        };
        out.push(PanelPlacement {
            label: WIN_CANVAS,
            x,
            y: content_panel_y(origin.y, screen.height as i32, h, bottom, top_clear),
            size: canvas_size,
        });
    }

    if chat_open {
        let h = (chat_size.1 * scale).round() as i32;
        let x = if cc_open {
            cc_right + gap
        } else {
            loose_chat_x
        };
        out.push(PanelPlacement {
            label: WIN_CHAT,
            x,
            y: content_panel_y(origin.y, screen.height as i32, h, bottom, top_clear),
            size: chat_size,
        });
    }

    if out.is_empty() {
        return out;
    }

    let avail_left = origin.x + margin;
    let avail_right = origin.x + screen.width as i32 - margin;
    let leftmost = out.iter().map(|p| p.x).min().unwrap_or(avail_left);
    let rightmost = out
        .iter()
        .map(|p| p.x + (p.size.0 * scale).round() as i32)
        .max()
        .unwrap_or(avail_right);
    let mut shift = 0;
    if rightmost > avail_right {
        shift = avail_right - rightmost;
    }
    if leftmost + shift < avail_left {
        shift += avail_left - (leftmost + shift);
    }
    if shift != 0 {
        for p in &mut out {
            p.x += shift;
        }
    }

    for p in &mut out {
        let w = (p.size.0 * scale).round() as i32;
        let min_x = avail_left;
        let max_x = (avail_right - w).max(min_x);
        p.x = p.x.clamp(min_x, max_x);
    }

    out
}

fn apply_panel_placement(win: &tauri::WebviewWindow, placement: PanelPlacement, scale: f64) {
    let label = placement.label;
    let size = placement.size;
    let x = placement.x;
    let y = placement.y;
    eprintln!(
        "[meeting] park panel `{label}` ({x},{y}) size {}x{}",
        size.0, size.1
    );
    let _ = win.set_size(tauri::LogicalSize::new(size.0, size.1));
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    let _ = win.set_size(tauri::LogicalSize::new(size.0, size.1));
    if let Ok(actual) = win.outer_size() {
        let aw = (actual.width as f64) / scale;
        let ah = (actual.height as f64) / scale;
        if (aw - size.0).abs() > 8.0 || (ah - size.1).abs() > 8.0 {
            eprintln!(
                "[meeting] WARN panel `{label}` size desync want={}x{} got={aw:.0}x{ah:.0}",
                size.0, size.1
            );
        }
    }
}

/// Park open content panels as one row (re-layouts all open panels when any needs park).
fn park_content_panels(app: &tauri::AppHandle, monitor: &tauri::Monitor) {
    let scale = monitor.scale_factor();
    for placement in compute_content_panel_layout(monitor) {
        let Some(win) = app.get_webview_window(placement.label) else {
            continue;
        };
        apply_panel_placement(&win, placement, scale);
    }
}

fn show_content_panel(app: &tauri::AppHandle, label: &'static str, open: bool) {
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    if !open {
        let _ = win.hide();
        // Drop this panel's parked bit and re-fit the survivors. Leaving the bit set kept the
        // closed panel's slot as a gap in the row and suppressed the re-park on reopen.
        if let Ok(mut g) = PANEL_PARKED.lock() {
            match label {
                WIN_CC => g.cc = false,
                WIN_CANVAS => g.canvas = false,
                WIN_CHAT => g.chat = false,
                _ => {}
            }
        }
        if let Some(monitor) = overlay_monitor(&win) {
            park_content_panels(app, &monitor);
        }
        return;
    }
    let size = panel_size(label);
    let needs_park = PANEL_PARKED
        .lock()
        .ok()
        .map(|g| !match label {
            WIN_CC => g.cc,
            WIN_CANVAS => g.canvas,
            WIN_CHAT => g.chat,
            _ => false,
        })
        .unwrap_or(true);
    let _ = win.set_size(tauri::LogicalSize::new(size.0, size.1));
    if needs_park {
        if let Some(monitor) = overlay_monitor(&win) {
            park_content_panels(app, &monitor);
        }
        if let Ok(mut g) = PANEL_PARKED.lock() {
            use std::sync::atomic::Ordering;
            if OVERLAY_PANEL_OPEN.load(Ordering::SeqCst) {
                g.cc = true;
            }
            if OVERLAY_CANVAS_OPEN.load(Ordering::SeqCst) {
                g.canvas = true;
            }
            if OVERLAY_CHAT_OPEN.load(Ordering::SeqCst) {
                g.chat = true;
            }
        }
    }
    // Content panel windows capture clicks while visible (tight chrome, no desk padding).
    set_overlay_ignores_mouse(&win, false);
    let _ = win.show();
    let _ = win.set_always_on_top(true);
    if let Some(ptr) = overlay_ns_window(&win) {
        use objc2::msg_send;
        // SAFETY: live NSWindow on main thread.
        unsafe {
            let _: () = msg_send![ptr, orderFrontRegardless];
        }
    }
    eprintln!(
        "[meeting] panel `{label}` show pos={:?} size={:?}",
        win.outer_position().ok(),
        (size.0, size.1),
    );
}

#[derive(Debug, Clone, Copy, Serialize)]
struct OverlayPanelFlags {
    panel: bool,
    canvas: bool,
    chat: bool,
}

fn panel_flags() -> OverlayPanelFlags {
    use std::sync::atomic::Ordering;
    OverlayPanelFlags {
        panel: OVERLAY_PANEL_OPEN.load(Ordering::SeqCst),
        canvas: OVERLAY_CANVAS_OPEN.load(Ordering::SeqCst),
        chat: OVERLAY_CHAT_OPEN.load(Ordering::SeqCst),
    }
}

fn emit_panel_flags(app: &tauri::AppHandle) {
    let _ = app.emit("meeting_overlay_panels", panel_flags());
}

/// Broadcast a settings change to every meeting window. `MeetingView` carries no settings and
/// each panel window fetches its own copy once at mount, so without this a mode or language
/// switch in the bar never reaches captions — the core translates while the panel keeps
/// rendering the transcription layout.
pub(super) fn emit_settings(app: &tauri::AppHandle, settings: &Settings) {
    let _ = app.emit("meeting_settings", settings.clone());
}

fn sync_content_panels(app: &tauri::AppHandle, recording_visible: bool) {
    use std::sync::atomic::Ordering;
    if !recording_visible {
        hide_content_panels(app);
        return;
    }
    show_content_panel(app, WIN_CC, OVERLAY_PANEL_OPEN.load(Ordering::SeqCst));
    show_content_panel(app, WIN_CANVAS, OVERLAY_CANVAS_OPEN.load(Ordering::SeqCst));
    show_content_panel(app, WIN_CHAT, OVERLAY_CHAT_OPEN.load(Ordering::SeqCst));
}

/// Toggle open flags change host size/visibility? No — only panel windows. Bypass
/// `sync_window_main`'s LAST short-circuit so show/hide still runs.
fn sync_content_panels_for_recording(app: &tauri::AppHandle) {
    let recording_visible = LANE
        .lock()
        .ok()
        .and_then(|g| {
            g.as_ref().map(|l| {
                l.settings.enabled && l.machine.state() == State::Recording && !l.overlay_dismissed
            })
        })
        .unwrap_or(false);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        sync_content_panels(&handle, recording_visible);
    });
    emit_panel_flags(app);
}

/// Show, hide and resize the overlay to match the lane's state.
pub(super) fn sync_window(
    app: &tauri::AppHandle,
    state: State,
    enabled: bool,
    overlay_dismissed: bool,
) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        sync_window_main(&handle, state, enabled, overlay_dismissed);
    });
}

fn sync_window_main(app: &tauri::AppHandle, state: State, enabled: bool, overlay_dismissed: bool) {
    // `PARKED` records only whether the overlay has been placed yet — the user may drag it
    // afterwards and it must not jump back. Showing is attempted on *every* tick it should
    // be visible: `show()` is idempotent, and treating "we showed it once" as "it is on
    // screen" is what left an invisible window in the one state that has to be seen.
    static PARKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    static LAST_PARK_MODE: std::sync::Mutex<Option<ParkMode>> = std::sync::Mutex::new(None);
    use std::sync::atomic::Ordering;

    let visible = enabled
        && !matches!(state, State::Idle)
        && !(state == State::Recording && overlay_dismissed);
    let size = match state {
        State::Wrapping => RECAP_SIZE,
        State::Recording => recording_overlay_size(),
        State::Offered => OFFER_SIZE,
        State::Idle => BAR_SIZE,
    };
    // Skip redundant AppKit work: emit() runs every second while a meeting is active, but the
    // overlay only needs to change on visibility/state/size transitions. Hammering set_size /
    // orderFront every tick races teardown and can destabilize the webview.
    // (visible, state, overlay_dismissed, w, h) — the last emitted overlay geometry.
    type LastEmit = (bool, State, bool, f64, f64);
    static LAST: std::sync::Mutex<Option<LastEmit>> = std::sync::Mutex::new(None);
    // ONE acquisition: reading `prev_size` under a separate lock let a concurrent emit() land
    // between the two, so the size we compared against was not the size we then overwrote.
    let size_changed = {
        let Ok(mut last) = LAST.lock() else { return };
        let changed = last
            .as_ref()
            .map(|(_, _, _, w, h)| *w != size.0 || *h != size.1)
            .unwrap_or(true);
        if last.as_ref().is_some_and(|(v, s, dismissed, w, h)| {
            *v == visible
                && *dismissed == overlay_dismissed
                && (!visible || (*s == state && *w == size.0 && *h == size.1))
        }) {
            return;
        }
        *last = Some((visible, state, overlay_dismissed, size.0, size.1));
        changed
    };
    // Never builds: the window exists from launch (see `build_overlay`). If it is missing,
    // something failed at setup and the right answer is to do nothing rather than to try
    // creating an AppKit window from this thread.
    let Some(win) = app.get_webview_window(WINDOW_LABEL) else {
        return;
    };
    if !visible {
        OVERLAY_WANTS_INTERACTIVE.store(false, Ordering::SeqCst);
        HOST_MENU_OPEN.store(false, Ordering::SeqCst);
        set_overlay_ignores_mouse(&win, true);
        let _ = win.hide();
        hide_content_panels(app);
        PARKED.store(false, Ordering::SeqCst);
        if let Ok(mut last_mode) = LAST_PARK_MODE.lock() {
            *last_mode = None;
        }
        return;
    }
    if state != State::Recording {
        HOST_MENU_OPEN.store(false, Ordering::SeqCst);
    }
    let park_mode = park_mode_for_state(state);
    let park_mode_changed = LAST_PARK_MODE
        .lock()
        .ok()
        .and_then(|m| m.as_ref().map(|prev| *prev != park_mode))
        .unwrap_or(true);
    // Menu expand/collapse: keep bottom edge (and user drag x). Full re-park would snap
    // a dragged pill back to bottom-center every time the mode menu opens.
    if state == State::Recording
        && size_changed
        && PARKED.load(Ordering::SeqCst)
        && !park_mode_changed
    {
        resize_host_keeping_bottom(&win, size);
    } else {
        let _ = win.set_size(tauri::LogicalSize::new(size.0, size.1));
        if !PARKED.load(Ordering::SeqCst) || park_mode_changed || size_changed {
            park_overlay(&win, park_mode, size);
            PARKED.store(true, Ordering::SeqCst);
            if let Ok(mut last_mode) = LAST_PARK_MODE.lock() {
                *last_mode = Some(park_mode);
            }
        }
    }
    // Whole window captures clicks while the host surface is visible.
    OVERLAY_WANTS_INTERACTIVE.store(true, Ordering::SeqCst);
    apply_overlay_interactive(&win);
    let shown = win.show();
    let _ = win.set_always_on_top(true);
    if let Some(ptr) = overlay_ns_window(&win) {
        use objc2::msg_send;
        // SAFETY: live NSWindow on the main thread.
        unsafe {
            let _: () = msg_send![ptr, orderFrontRegardless];
        }
    }
    let recording_visible = state == State::Recording;
    sync_content_panels(app, recording_visible);
    if recording_visible {
        emit_panel_flags(app);
    }
    let _ = app.emit("meeting_overlay_surface", ());
    eprintln!(
        "[meeting] overlay show ok={} pos={:?} size={:?} interactive={} panels=cc:{} canvas:{} chat:{}",
        shown.is_ok(),
        win.outer_position().ok(),
        (size.0, size.1),
        OVERLAY_WANTS_INTERACTIVE.load(Ordering::SeqCst),
        OVERLAY_PANEL_OPEN.load(Ordering::SeqCst),
        OVERLAY_CANVAS_OPEN.load(Ordering::SeqCst),
        OVERLAY_CHAT_OPEN.load(Ordering::SeqCst),
    );
}

/// Dismiss the Recap and return the lane to Idle.
pub(super) fn meeting_wrapped(app: tauri::AppHandle) {
    let wrapping = LANE
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|l| l.machine.state() == State::Wrapping))
        .unwrap_or(false);
    if !wrapping {
        return;
    }
    step(&app, Input::Wrapped);
}

/// Let the user move the overlay (Issue #7: draggable).
///
/// `start_dragging` alone is unreliable on borderless WKWebView windows — hand the in-flight
/// mouse event to AppKit like the notch panel's `start_panel_drag`.
pub(super) fn meeting_drag(app: tauri::AppHandle, label: Option<String>) {
    let label = label.unwrap_or_else(|| WINDOW_LABEL.to_string());
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        let Some(win) = handle.get_webview_window(&label) else {
            return;
        };
        let Some(ptr) = overlay_ns_window(&win) else {
            return;
        };
        // SAFETY: main thread; standard AppKit calls on a live NSWindow.
        unsafe {
            let ns_app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            if !ns_app.is_null() {
                let ev: *mut AnyObject = msg_send![ns_app, currentEvent];
                if !ev.is_null() {
                    let _: () = msg_send![ptr, performWindowDragWithEvent: ev];
                    return;
                }
            }
        }
        let _ = win.start_dragging();
    });
}

/// The pill's current contents (FR-MT-09). Also the webview's first read at boot.
pub(super) fn meeting_overlay_dismiss(app: tauri::AppHandle) {
    let now = now_ms();
    let Ok(mut g) = LANE.lock() else { return };
    let Some(lane) = g.as_mut() else { return };
    if lane.machine.state() == State::Recording {
        lane.overlay_dismissed = true;
        emit(&app, lane, now);
    }
}

/// Expand/collapse the live captions panel window.
pub(super) fn meeting_set_overlay_panel(app: tauri::AppHandle, open: bool) {
    use std::sync::atomic::Ordering;
    OVERLAY_PANEL_OPEN.store(open, Ordering::SeqCst);
    clear_custom_size_if_idle();
    sync_content_panels_for_recording(&app);
}

/// Expand/collapse the AI Canvas panel window.
pub(super) fn meeting_set_overlay_canvas(app: tauri::AppHandle, open: bool) {
    use std::sync::atomic::Ordering;
    OVERLAY_CANVAS_OPEN.store(open, Ordering::SeqCst);
    clear_custom_size_if_idle();
    sync_content_panels_for_recording(&app);
}

/// Expand/collapse the AI Chat panel window.
pub(super) fn meeting_set_overlay_chat(app: tauri::AppHandle, open: bool) {
    use std::sync::atomic::Ordering;
    OVERLAY_CHAT_OPEN.store(open, Ordering::SeqCst);
    clear_custom_size_if_idle();
    sync_content_panels_for_recording(&app);
}

/// Live corner-resize of one content panel window (keeps top-left fixed),
/// or host pill expand/collapse for the bar mode menu (keeps bottom edge fixed).
pub(super) fn meeting_set_overlay_size(
    app: tauri::AppHandle,
    width: f64,
    height: f64,
    label: Option<String>,
) {
    let label_owned = label.unwrap_or_else(|| WIN_CC.to_string());
    if label_owned == WINDOW_LABEL {
        // Host: FE passes PILL_SIZE / PILL_WITH_MENU_SIZE — not panel clamp mins.
        let open = height + 0.5 >= PILL_WITH_MENU_SIZE.1;
        HOST_MENU_OPEN.store(open, Ordering::SeqCst);
        let size = recording_overlay_size();
        let _ = width; // host width fixed to pill constants
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some(win) = handle.get_webview_window(WINDOW_LABEL) else {
                return;
            };
            resize_host_keeping_bottom(&win, size);
        });
        return;
    }
    let key: &'static str = match label_owned.as_str() {
        WIN_CC => WIN_CC,
        WIN_CANVAS => WIN_CANVAS,
        WIN_CHAT => WIN_CHAT,
        _ => return,
    };
    let size = clamp_overlay_size(width, height);
    if let Ok(mut g) = PANEL_CUSTOM_SIZE.lock() {
        match key {
            WIN_CC => g.cc = Some(size),
            WIN_CANVAS => g.canvas = Some(size),
            WIN_CHAT => g.chat = Some(size),
            _ => {}
        }
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(win) = handle.get_webview_window(key) else {
            return;
        };
        let prev = win.outer_position().ok();
        let _ = win.set_size(tauri::LogicalSize::new(size.0, size.1));
        if let Some(p) = prev {
            let _ = win.set_position(p);
        }
        let _ = win.set_size(tauri::LogicalSize::new(size.0, size.1));
    });
}
