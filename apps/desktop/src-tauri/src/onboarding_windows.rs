//! Generation-owned native onboarding window session.

use std::time::Duration;

use serde::Serialize;

use crate::geometry::{Point, Rect};

pub const INTRO_DURATION: Duration = Duration::from_secs(5);
const INTERACTIVE_WIDTH: f64 = 1120.0;
const INTERACTIVE_HEIGHT: f64 = 720.0;
const INTERACTIVE_MIN_WIDTH: f64 = 900.0;
const INTERACTIVE_MIN_HEIGHT: f64 = 620.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingSurfaceKind {
    Main,
    Ambient,
    Interactive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalPermissionKind {
    Accessibility,
    Microphone,
    ScreenRecording,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplaySnapshot {
    pub display_id: u32,
    pub appkit_frame: Rect,
    pub cg_frame: Rect,
    pub scale_factor: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OnboardingSurface {
    pub surface: OnboardingSurfaceKind,
    pub generation: u64,
    pub display_id: u32,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionPhase {
    Intro,
    Interactive,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowLevelPolicy {
    Overlay,
    Normal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowPolicy {
    pub level: WindowLevelPolicy,
    pub ignores_mouse_events: bool,
    pub can_become_key_or_main: bool,
    pub current_space_only: bool,
}

trait ExternalPermissionWindowOps {
    fn close_intro_windows(&mut self) -> Result<(), String>;
    fn lower_interactive_window(&mut self) -> Result<(), String>;
}

fn enforce_external_permission_barrier(
    operations: &mut impl ExternalPermissionWindowOps,
) -> Result<(), String> {
    operations.close_intro_windows()?;
    operations.lower_interactive_window()
}

fn configure_intro_with_rollback(
    configure: impl FnOnce() -> Result<(), String>,
    rollback: impl FnOnce(),
) -> Result<(), String> {
    if let Err(error) = configure() {
        rollback();
        return Err(error);
    }
    Ok(())
}

fn surface_close_should_cleanup(
    model: &WindowSessionModel,
    generation: u64,
    label: &str,
    external_permission_ui: bool,
) -> bool {
    model.generation() == generation
        && model
            .surfaces()
            .iter()
            .find(|surface| surface.label == label)
            .is_some_and(|surface| {
                !external_permission_ui || surface.surface == OnboardingSurfaceKind::Interactive
            })
}

#[derive(Clone)]
pub struct WindowSessionModel {
    generation: u64,
    launch_display_id: u32,
    displays: Vec<DisplaySnapshot>,
    phase: SessionPhase,
    intro_transitioned: bool,
    owned: Vec<OnboardingSurface>,
}

pub fn select_cursor_display(
    displays: &[DisplaySnapshot],
    cursor: Point,
    main_display_id: Option<u32>,
) -> Option<u32> {
    displays
        .iter()
        .find(|display| display.appkit_frame.contains(cursor))
        .map(|display| display.display_id)
        .or_else(|| {
            main_display_id.filter(|main_id| {
                displays
                    .iter()
                    .any(|display| display.display_id == *main_id)
            })
        })
        .or_else(|| displays.first().map(|display| display.display_id))
}

pub fn window_policy(surface: OnboardingSurfaceKind) -> WindowPolicy {
    match surface {
        OnboardingSurfaceKind::Main => WindowPolicy {
            level: WindowLevelPolicy::Overlay,
            ignores_mouse_events: false,
            can_become_key_or_main: true,
            current_space_only: false,
        },
        OnboardingSurfaceKind::Ambient => WindowPolicy {
            level: WindowLevelPolicy::Overlay,
            ignores_mouse_events: true,
            can_become_key_or_main: false,
            current_space_only: false,
        },
        OnboardingSurfaceKind::Interactive => WindowPolicy {
            level: WindowLevelPolicy::Normal,
            ignores_mouse_events: false,
            can_become_key_or_main: true,
            current_space_only: true,
        },
    }
}

pub fn full_display_appkit_frame(display: DisplaySnapshot) -> Rect {
    display.appkit_frame
}

pub fn interactive_appkit_frame(display: DisplaySnapshot) -> Rect {
    let frame = display.appkit_frame;
    Rect::new(
        (frame.x + (frame.w - INTERACTIVE_WIDTH) / 2.0).max(frame.x),
        (frame.y + (frame.h - INTERACTIVE_HEIGHT) / 2.0).max(frame.y),
        INTERACTIVE_WIDTH.min(frame.w),
        INTERACTIVE_HEIGHT.min(frame.h),
    )
}

impl WindowSessionModel {
    pub fn intro(
        generation: u64,
        displays: Vec<DisplaySnapshot>,
        cursor: Point,
        main_display_id: Option<u32>,
    ) -> Option<Self> {
        let launch_display_id = select_cursor_display(&displays, cursor, main_display_id)?;
        let owned = intro_surfaces(generation, launch_display_id, &displays);
        Some(Self {
            generation,
            launch_display_id,
            displays,
            phase: SessionPhase::Intro,
            intro_transitioned: false,
            owned,
        })
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn launch_display_id(&self) -> u32 {
        self.launch_display_id
    }

    pub fn intro_active(&self) -> bool {
        self.phase == SessionPhase::Intro
    }

    pub fn surfaces(&self) -> &[OnboardingSurface] {
        &self.owned
    }

    pub fn surface_for_label(
        &self,
        label: &str,
        expected_generation: u64,
    ) -> Result<OnboardingSurface, String> {
        if expected_generation != self.generation {
            return Err("stale onboarding window generation".to_owned());
        }
        self.owned
            .iter()
            .find(|surface| surface.label == label && surface.generation == expected_generation)
            .cloned()
            .ok_or_else(|| "onboarding surface does not belong to this session".to_owned())
    }

    pub fn deadline_elapsed(&mut self, callback_generation: u64, elapsed: Duration) -> bool {
        if self.phase != SessionPhase::Intro
            || self.intro_transitioned
            || callback_generation != self.generation
            || elapsed < INTRO_DURATION
        {
            return false;
        }
        self.intro_transitioned = true;
        true
    }

    pub fn finish_intro(&mut self) {
        if self.phase != SessionPhase::Intro {
            return;
        }
        self.phase = SessionPhase::Interactive;
        self.owned.clear();
        self.owned.push(OnboardingSurface {
            surface: OnboardingSurfaceKind::Interactive,
            generation: self.generation,
            display_id: self.launch_display_id,
            label: crate::onboarding::mac::ONBOARDING_LABEL.to_owned(),
        });
    }

    pub fn reconcile_displays(
        &mut self,
        callback_generation: u64,
        displays: Vec<DisplaySnapshot>,
        cursor: Point,
        main_display_id: Option<u32>,
    ) -> Option<Vec<String>> {
        if callback_generation != self.generation || self.phase == SessionPhase::Closed {
            return None;
        }
        match self.phase {
            SessionPhase::Intro => {
                let old_labels = self
                    .owned
                    .iter()
                    .map(|surface| surface.label.clone())
                    .collect::<Vec<_>>();
                if displays.is_empty() {
                    self.generation = self.generation.wrapping_add(1);
                    self.phase = SessionPhase::Closed;
                    self.owned.clear();
                    return Some(old_labels);
                }
                if !displays
                    .iter()
                    .any(|display| display.display_id == self.launch_display_id)
                {
                    self.launch_display_id =
                        select_cursor_display(&displays, cursor, main_display_id)?;
                }
                self.generation = self.generation.wrapping_add(1);
                self.displays = displays;
                self.owned =
                    intro_surfaces(self.generation, self.launch_display_id, &self.displays);
                Some(old_labels)
            }
            SessionPhase::Interactive => {
                if displays.is_empty() {
                    return Some(self.cleanup());
                }
                if !displays
                    .iter()
                    .any(|display| display.display_id == self.launch_display_id)
                {
                    self.launch_display_id =
                        select_cursor_display(&displays, cursor, main_display_id)?;
                    if let Some(surface) = self.owned.first_mut() {
                        surface.display_id = self.launch_display_id;
                    }
                }
                self.displays = displays;
                None
            }
            SessionPhase::Closed => None,
        }
    }

    pub fn prepare_for_external_ui(&mut self) -> Vec<String> {
        let mut closed = Vec::new();
        self.owned.retain(|surface| {
            let intro = matches!(
                surface.surface,
                OnboardingSurfaceKind::Main | OnboardingSurfaceKind::Ambient
            );
            if intro {
                closed.push(surface.label.clone());
            }
            !intro
        });
        closed
    }

    pub fn cleanup(&mut self) -> Vec<String> {
        if self.phase == SessionPhase::Closed {
            return Vec::new();
        }
        self.phase = SessionPhase::Closed;
        self.generation = self.generation.wrapping_add(1);
        self.owned.drain(..).map(|surface| surface.label).collect()
    }
}

fn intro_surfaces(
    generation: u64,
    launch_display_id: u32,
    displays: &[DisplaySnapshot],
) -> Vec<OnboardingSurface> {
    displays
        .iter()
        .map(|display| {
            let surface = if display.display_id == launch_display_id {
                OnboardingSurfaceKind::Main
            } else {
                OnboardingSurfaceKind::Ambient
            };
            let label = match surface {
                OnboardingSurfaceKind::Main => format!("onboarding-main-{generation}"),
                OnboardingSurfaceKind::Ambient => {
                    format!("onboarding-ambient-{generation}-{}", display.display_id)
                }
                OnboardingSurfaceKind::Interactive => unreachable!(),
            };
            OnboardingSurface {
                surface,
                generation,
                display_id: display.display_id,
                label,
            }
        })
        .collect()
}

#[cfg(target_os = "macos")]
pub mod mac {
    use std::collections::HashMap;
    use std::sync::{mpsc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEvent};
    use tauri::{AppHandle, Manager};

    use super::{
        configure_intro_with_rollback, enforce_external_permission_barrier,
        full_display_appkit_frame, interactive_appkit_frame, surface_close_should_cleanup,
        window_policy, DisplaySnapshot, ExternalPermissionKind, ExternalPermissionWindowOps,
        OnboardingSurface, OnboardingSurfaceKind, WindowLevelPolicy, WindowSessionModel,
        INTERACTIVE_HEIGHT, INTERACTIVE_MIN_HEIGHT, INTERACTIVE_MIN_WIDTH, INTERACTIVE_WIDTH,
        INTRO_DURATION,
    };
    use crate::geometry::Point;

    const ALL_SPACES_BEHAVIOR: usize = (1 << 0) | (1 << 8);
    const CURRENT_SPACE_BEHAVIOR: usize = 0;
    const NORMAL_WINDOW_LEVEL: isize = 0;
    const NONACTIVATING_PANEL_STYLE: usize = 1 << 7;

    fn ambient_panels() -> &'static Mutex<HashMap<String, usize>> {
        static PANELS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
        PANELS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn ambient_panel_class() -> Result<&'static objc2::runtime::AnyClass, String> {
        use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
        use objc2::{class, sel};

        static CLASS: OnceLock<Option<&'static AnyClass>> = OnceLock::new();
        CLASS
            .get_or_init(|| {
                extern "C" fn no(_this: &AnyObject, _selector: Sel) -> Bool {
                    Bool::NO
                }
                let mut builder =
                    ClassBuilder::new(c"ShogunOnboardingAmbientPanel", class!(NSPanel))?;
                // SAFETY: both methods match AppKit's `BOOL method(void)` declarations.
                unsafe {
                    builder.add_method(sel!(canBecomeKeyWindow), no as extern "C" fn(_, _) -> _);
                    builder.add_method(sel!(canBecomeMainWindow), no as extern "C" fn(_, _) -> _);
                }
                Some(builder.register())
            })
            .ok_or_else(|| "ambient NSPanel subclass registration failed".to_owned())
    }

    struct NativeSession {
        model: WindowSessionModel,
        started_at: Option<Instant>,
        state_revision: u64,
        permission_generation: Option<u64>,
        external_permission_ui: Option<ExternalPermissionKind>,
    }

    #[derive(Default)]
    struct RuntimeState {
        next_generation: u64,
        session: Option<NativeSession>,
        display_observer: Option<usize>,
    }

    #[derive(Default)]
    pub struct OnboardingWindowRuntime(Mutex<RuntimeState>);

    fn read_displays(mtm: MainThreadMarker) -> Vec<DisplaySnapshot> {
        crate::geometry::mac::read_all(mtm)
            .into_iter()
            .map(|geometry| DisplaySnapshot {
                display_id: geometry.display_id,
                appkit_frame: geometry.screen,
                cg_frame: geometry.cg_screen,
                scale_factor: geometry.scale_factor,
            })
            .collect()
    }

    fn cursor_location() -> Point {
        let cursor = NSEvent::mouseLocation();
        Point::new(cursor.x, cursor.y)
    }

    fn display_for<'a>(
        displays: &'a [DisplaySnapshot],
        surface: &OnboardingSurface,
    ) -> Result<&'a DisplaySnapshot, String> {
        displays
            .iter()
            .find(|display| display.display_id == surface.display_id)
            .ok_or_else(|| format!("display {} disconnected", surface.display_id))
    }

    fn surface_route(surface: &OnboardingSurface) -> String {
        let kind = match surface.surface {
            OnboardingSurfaceKind::Main => "main",
            OnboardingSurfaceKind::Ambient => "ambient",
            OnboardingSurfaceKind::Interactive => "interactive",
        };
        format!(
            "onboarding.html?surface={kind}&generation={}",
            surface.generation
        )
    }

    fn build_intro_window(
        app: &AppHandle,
        surface: &OnboardingSurface,
        display: &DisplaySnapshot,
    ) -> Result<tauri::WebviewWindow, String> {
        let frame = full_display_appkit_frame(*display);
        let window = tauri::WebviewWindowBuilder::new(
            app,
            &surface.label,
            tauri::WebviewUrl::App(surface_route(surface).into()),
        )
        .title("ShogunAI")
        .visible(false)
        .focused(false)
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .inner_size(frame.w, frame.h)
        .build()
        .map_err(|error| format!("{} build failed: {error}", surface.label))?;
        configure_intro_with_rollback(
            || configure_intro_window(&window, surface, display),
            || {
                dismantle_ambient_panel(&window);
                let _ = window.destroy();
            },
        )?;
        Ok(window)
    }

    fn build_intro_windows(
        app: &AppHandle,
        surfaces: &[OnboardingSurface],
        displays: &[DisplaySnapshot],
    ) -> Result<Vec<tauri::WebviewWindow>, String> {
        let mut windows = Vec::new();
        for surface in surfaces {
            let result = display_for(displays, surface)
                .and_then(|display| build_intro_window(app, surface, display));
            match result {
                Ok(window) => windows.push(window),
                Err(error) => {
                    destroy_windows(&windows);
                    return Err(error);
                }
            }
        }
        Ok(windows)
    }

    fn adopt_ambient_panel(
        window: &tauri::WebviewWindow,
        label: &str,
        display: &DisplaySnapshot,
    ) -> Result<*mut objc2::runtime::AnyObject, String> {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};

        let tao = window
            .ns_window()
            .map_err(|error| format!("ambient host window unavailable: {error}"))?
            as *mut AnyObject;
        if tao.is_null() {
            return Err("ambient host window unavailable".to_owned());
        }
        let mut panels = ambient_panels()
            .lock()
            .map_err(|_| "ambient NSPanel registry unavailable".to_owned())?;
        // SAFETY: called on AppKit main thread. Content view is retained across reparenting; panel
        // is released by `dismantle_ambient_panel` before Tauri host destruction.
        let panel = unsafe {
            let frame = ns_rect(full_display_appkit_frame(*display));
            let alloc: *mut AnyObject = msg_send![ambient_panel_class()?, alloc];
            let style = (1 << 4) | NONACTIVATING_PANEL_STYLE;
            let panel: *mut AnyObject = msg_send![alloc, initWithContentRect: frame, styleMask: style, backing: 2usize, defer: false];
            if panel.is_null() {
                return Err("ambient NSPanel creation failed".to_owned());
            }
            let _: () = msg_send![panel, setReleasedWhenClosed: false];
            let _: () = msg_send![panel, setIgnoresMouseEvents: true];
            let _: () = msg_send![panel, setCollectionBehavior: ALL_SPACES_BEHAVIOR];
            let _: () = msg_send![panel, setHidesOnDeactivate: false];
            let _: () = msg_send![panel, setLevel: crate::OVERLAY_LEVEL];
            let can_key: bool = msg_send![panel, canBecomeKeyWindow];
            let can_main: bool = msg_send![panel, canBecomeMainWindow];
            if can_key || can_main {
                let _: () = msg_send![panel, close];
                let _: () = msg_send![panel, release];
                return Err("ambient NSPanel accepted key/main status".to_owned());
            }
            let content: *mut AnyObject = msg_send![tao, contentView];
            let _: () = msg_send![content, retain];
            let placeholder: *mut AnyObject = msg_send![class!(NSView), new];
            let _: () = msg_send![tao, setContentView: placeholder];
            let _: () = msg_send![placeholder, release];
            let _: () = msg_send![panel, setContentView: content];
            let _: () = msg_send![content, release];
            panel
        };
        panels.insert(label.to_owned(), panel as usize);
        Ok(panel)
    }

    fn ambient_panel(label: &str) -> Option<*mut objc2::runtime::AnyObject> {
        ambient_panels()
            .lock()
            .ok()
            .and_then(|panels| panels.get(label).copied())
            .map(|panel| panel as *mut objc2::runtime::AnyObject)
    }

    fn dismantle_ambient_panel(window: &tauri::WebviewWindow) {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};

        let panel = ambient_panels()
            .lock()
            .ok()
            .and_then(|mut panels| panels.remove(window.label()))
            .map(|panel| panel as *mut AnyObject);
        let Some(panel) = panel else { return };
        let tao = window.ns_window().ok().map(|ptr| ptr as *mut AnyObject);
        // SAFETY: main thread, live panel. Reparent content before panel release when host survives.
        unsafe {
            if let Some(tao) = tao.filter(|ptr| !ptr.is_null()) {
                let content: *mut AnyObject = msg_send![panel, contentView];
                let _: () = msg_send![content, retain];
                let placeholder: *mut AnyObject = msg_send![class!(NSView), new];
                let _: () = msg_send![panel, setContentView: placeholder];
                let _: () = msg_send![placeholder, release];
                let _: () = msg_send![tao, setContentView: content];
                let _: () = msg_send![content, release];
            }
            let _: () = msg_send![panel, orderOut: std::ptr::null_mut::<AnyObject>()];
            let _: () = msg_send![panel, close];
            let _: () = msg_send![panel, release];
        }
    }

    fn configure_intro_window(
        window: &tauri::WebviewWindow,
        surface: &OnboardingSurface,
        display: &DisplaySnapshot,
    ) -> Result<(), String> {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;

        let ptr = window
            .ns_window()
            .map_err(|error| format!("native intro window unavailable: {error}"))?
            as *mut AnyObject;
        if ptr.is_null() {
            return Err("native intro window unavailable".to_owned());
        }
        if surface.surface == OnboardingSurfaceKind::Ambient {
            let _ = adopt_ambient_panel(window, &surface.label, display)?;
            return Ok(());
        }
        let policy = window_policy(surface.surface);
        let level = match policy.level {
            WindowLevelPolicy::Overlay => crate::OVERLAY_LEVEL,
            WindowLevelPolicy::Normal => NORMAL_WINDOW_LEVEL,
        };
        // SAFETY: setup and display callbacks invoke this on AppKit's main thread. `ptr` belongs
        // to the live Tauri window; selectors take scalar AppKit values.
        unsafe {
            let frame = ns_rect(full_display_appkit_frame(*display));
            let _: () = msg_send![ptr, setFrame: frame, display: false];
            let _: () = msg_send![ptr, setIgnoresMouseEvents: policy.ignores_mouse_events];
            let _: () = msg_send![ptr, setLevel: level];
            let _: () = msg_send![ptr, setCollectionBehavior: ALL_SPACES_BEHAVIOR];
            let _: () = msg_send![ptr, setHidesOnDeactivate: false];
        }
        Ok(())
    }

    fn ns_rect(rect: crate::geometry::Rect) -> objc2_foundation::NSRect {
        objc2_foundation::NSRect {
            origin: objc2_foundation::NSPoint {
                x: rect.x,
                y: rect.y,
            },
            size: objc2_foundation::NSSize {
                width: rect.w,
                height: rect.h,
            },
        }
    }

    fn build_interactive_window(
        app: &AppHandle,
        surface: &OnboardingSurface,
        display: &DisplaySnapshot,
    ) -> Result<tauri::WebviewWindow, String> {
        let window = tauri::WebviewWindowBuilder::new(
            app,
            &surface.label,
            tauri::WebviewUrl::App(surface_route(surface).into()),
        )
        .title("ShogunAI")
        .visible(false)
        .focused(false)
        .resizable(true)
        .always_on_top(false)
        .inner_size(INTERACTIVE_WIDTH, INTERACTIVE_HEIGHT)
        .min_inner_size(INTERACTIVE_MIN_WIDTH, INTERACTIVE_MIN_HEIGHT)
        .build()
        .map_err(|error| format!("interactive onboarding build failed: {error}"))?;
        configure_interactive_window(&window, Some(display))?;
        Ok(window)
    }

    fn configure_interactive_window(
        window: &tauri::WebviewWindow,
        display: Option<&DisplaySnapshot>,
    ) -> Result<(), String> {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;

        window
            .set_always_on_top(false)
            .map_err(|error| format!("interactive level reset failed: {error}"))?;
        let ptr = window
            .ns_window()
            .map_err(|error| format!("native interactive window unavailable: {error}"))?
            as *mut AnyObject;
        if ptr.is_null() {
            return Err("native interactive window unavailable".to_owned());
        }
        let policy = window_policy(OnboardingSurfaceKind::Interactive);
        let level = match policy.level {
            WindowLevelPolicy::Overlay => crate::OVERLAY_LEVEL,
            WindowLevelPolicy::Normal => NORMAL_WINDOW_LEVEL,
        };
        let collection_behavior = if policy.current_space_only {
            CURRENT_SPACE_BEHAVIOR
        } else {
            ALL_SPACES_BEHAVIOR
        };
        // SAFETY: caller runs on AppKit's main thread with Tauri's live NSWindow.
        unsafe {
            if let Some(display) = display {
                let frame = ns_rect(interactive_appkit_frame(*display));
                let _: () = msg_send![ptr, setFrame: frame, display: false];
            }
            let _: () = msg_send![ptr, setIgnoresMouseEvents: policy.ignores_mouse_events];
            let _: () = msg_send![ptr, setLevel: level];
            let _: () = msg_send![ptr, setCollectionBehavior: collection_behavior];
            let _: () = msg_send![ptr, setHidesOnDeactivate: false];
        }
        Ok(())
    }

    fn show_intro_windows(windows: &[tauri::WebviewWindow]) -> Result<(), String> {
        use objc2::msg_send;

        for window in windows {
            if let Some(panel) = ambient_panel(window.label()) {
                // SAFETY: main thread; registry entry remains live until session teardown.
                unsafe {
                    let _: () = msg_send![panel, orderFrontRegardless];
                }
                continue;
            }
            window
                .show()
                .map_err(|error| format!("intro window show failed: {error}"))?;
        }
        Ok(())
    }

    fn activate_interactive(
        window: &tauri::WebviewWindow,
        mtm: MainThreadMarker,
    ) -> Result<(), String> {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;

        window
            .show()
            .map_err(|error| format!("interactive onboarding show failed: {error}"))?;
        NSApplication::sharedApplication(mtm).activate();
        let ptr = window
            .ns_window()
            .map_err(|error| format!("native interactive window unavailable: {error}"))?
            as *mut AnyObject;
        if ptr.is_null() {
            return Err("native interactive window unavailable".to_owned());
        }
        let nil: *mut AnyObject = std::ptr::null_mut();
        // SAFETY: caller runs on AppKit's main thread with Tauri's live NSWindow.
        unsafe {
            let _: () = msg_send![ptr, makeKeyAndOrderFront: nil];
        }
        window
            .set_focus()
            .map_err(|error| format!("interactive onboarding focus failed: {error}"))
    }

    fn destroy_labels(app: &AppHandle, labels: &[String]) {
        for label in labels {
            if let Some(window) = app.get_webview_window(label) {
                dismantle_ambient_panel(&window);
                let _ = window.hide();
                let _ = window.destroy();
            }
        }
    }

    fn destroy_windows(windows: &[tauri::WebviewWindow]) {
        for window in windows {
            dismantle_ambient_panel(window);
            let _ = window.hide();
            let _ = window.destroy();
        }
    }

    fn schedule_deadline(app: AppHandle, generation: u64, started_at: Instant) {
        let remaining = INTRO_DURATION.saturating_sub(started_at.elapsed());
        std::thread::spawn(move || {
            std::thread::sleep(remaining);
            let callback_app = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Err(error) = transition_intro(&callback_app, generation) {
                    eprintln!("[onboarding] intro transition failed: {error}");
                    cleanup(&callback_app);
                }
            });
        });
    }

    fn attach_surface_cleanup(
        app: &AppHandle,
        window: &tauri::WebviewWindow,
        surface: &OnboardingSurface,
    ) {
        let callback_app = app.clone();
        let generation = surface.generation;
        let label = surface.label.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                cleanup_generation(&callback_app, generation, &label);
            }
        });
    }

    pub fn start(app: &AppHandle) -> Result<(), String> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "onboarding window session must start on main thread".to_owned())?;
        let runtime = app
            .try_state::<OnboardingWindowRuntime>()
            .ok_or_else(|| "onboarding window runtime unavailable".to_owned())?;
        {
            let state = runtime
                .0
                .lock()
                .map_err(|_| "onboarding window runtime unavailable".to_owned())?;
            if let Some(session) = state.session.as_ref() {
                if let Some(surface) = session
                    .model
                    .surfaces()
                    .iter()
                    .find(|surface| surface.surface == OnboardingSurfaceKind::Interactive)
                {
                    if let Some(window) = app.get_webview_window(&surface.label) {
                        activate_interactive(&window, mtm)?;
                    }
                }
                return Ok(());
            }
        }

        let displays = read_displays(mtm);
        if displays.is_empty() {
            return Err("no attached display available".to_owned());
        }
        let cursor = cursor_location();
        let main_display_id = crate::geometry::mac::read_main_display_id(mtm);
        let store = app
            .try_state::<crate::onboarding::mac::Store>()
            .ok_or_else(|| "onboarding state unavailable".to_owned())?;
        let onboarding_state = store.snapshot()?;
        let permission_generation = crate::onboarding::mac::start_watcher(app.clone());

        let (generation, model, started_at) = {
            let mut state = runtime
                .0
                .lock()
                .map_err(|_| "onboarding window runtime unavailable".to_owned())?;
            state.next_generation = state.next_generation.wrapping_add(1);
            let generation = state.next_generation;
            let mut model =
                WindowSessionModel::intro(generation, displays.clone(), cursor, main_display_id)
                    .ok_or_else(|| "no launch display available".to_owned())?;
            let started_at = (!onboarding_state.intro_complete).then(Instant::now);
            if onboarding_state.intro_complete {
                model.finish_intro();
            }
            (generation, model, started_at)
        };

        let surfaces = model.surfaces().to_vec();
        let mut built = Vec::new();
        for surface in &surfaces {
            let display = match display_for(&displays, surface) {
                Ok(display) => display,
                Err(error) => {
                    destroy_windows(&built);
                    if let Some(generation) = permission_generation {
                        crate::permissions::mac::stop(app, generation);
                    }
                    return Err(error);
                }
            };
            let window_result = match surface.surface {
                OnboardingSurfaceKind::Main | OnboardingSurfaceKind::Ambient => {
                    build_intro_window(app, surface, display)
                }
                OnboardingSurfaceKind::Interactive => {
                    build_interactive_window(app, surface, display)
                }
            };
            match window_result {
                Ok(window) => built.push(window),
                Err(error) => {
                    destroy_windows(&built);
                    if let Some(generation) = permission_generation {
                        crate::permissions::mac::stop(app, generation);
                    }
                    return Err(error);
                }
            }
        }

        {
            let mut state = runtime
                .0
                .lock()
                .map_err(|_| "onboarding window runtime unavailable".to_owned())?;
            state.session = Some(NativeSession {
                model,
                started_at,
                state_revision: onboarding_state.revision,
                permission_generation,
                external_permission_ui: None,
            });
        }

        if let Some(started_at) = started_at {
            for (surface, window) in surfaces.iter().zip(&built) {
                if surface.surface == OnboardingSurfaceKind::Main {
                    attach_surface_cleanup(app, window, surface);
                }
            }
            if let Err(error) = show_intro_windows(&built) {
                destroy_windows(&built);
                cleanup(app);
                return Err(error);
            }
            schedule_deadline(app.clone(), generation, started_at);
        } else {
            let window = built
                .first()
                .ok_or_else(|| "interactive onboarding window missing".to_owned())?;
            attach_surface_cleanup(app, window, &surfaces[0]);
            activate_interactive(window, mtm)?;
            crate::permission_drag::install_monitor(app);
        }
        install_display_observer(app);
        crate::onboarding_music::mac::start(app, onboarding_state.music_muted);
        Ok(())
    }

    fn transition_intro(app: &AppHandle, generation: u64) -> Result<(), String> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "intro transition must run on main thread".to_owned())?;
        let runtime = app
            .try_state::<OnboardingWindowRuntime>()
            .ok_or_else(|| "onboarding window runtime unavailable".to_owned())?;
        let (expected_revision, started_at) = {
            let mut state = runtime
                .0
                .lock()
                .map_err(|_| "onboarding window runtime unavailable".to_owned())?;
            let session = state
                .session
                .as_mut()
                .ok_or_else(|| "onboarding window session unavailable".to_owned())?;
            let started_at = session
                .started_at
                .ok_or_else(|| "onboarding intro is not active".to_owned())?;
            if !session
                .model
                .deadline_elapsed(generation, started_at.elapsed())
            {
                return Ok(());
            }
            (session.state_revision, started_at)
        };

        if started_at.elapsed() < INTRO_DURATION {
            return Ok(());
        }
        let store = app
            .try_state::<crate::onboarding::mac::Store>()
            .ok_or_else(|| "onboarding state unavailable".to_owned())?;
        let saved = store.mark_intro_complete(expected_revision)?;

        let (intro_labels, surface, displays) = {
            let mut state = runtime
                .0
                .lock()
                .map_err(|_| "onboarding window runtime unavailable".to_owned())?;
            let session = state
                .session
                .as_mut()
                .ok_or_else(|| "onboarding window session unavailable".to_owned())?;
            if session.model.generation() != generation {
                return Ok(());
            }
            let labels = session.model.prepare_for_external_ui();
            session.model.finish_intro();
            session.started_at = None;
            session.state_revision = saved.revision;
            let surface = session
                .model
                .surfaces()
                .first()
                .cloned()
                .ok_or_else(|| "interactive onboarding surface missing".to_owned())?;
            (labels, surface, session.model.displays.clone())
        };

        destroy_labels(app, &intro_labels);
        let display = display_for(&displays, &surface)?;
        let window = build_interactive_window(app, &surface, display)?;
        attach_surface_cleanup(app, &window, &surface);
        activate_interactive(&window, mtm)?;
        crate::permission_drag::install_monitor(app);
        Ok(())
    }

    fn cleanup_generation(app: &AppHandle, generation: u64, label: &str) {
        let Some(runtime) = app.try_state::<OnboardingWindowRuntime>() else {
            return;
        };
        let should_cleanup = runtime
            .0
            .lock()
            .ok()
            .and_then(|state| {
                state.session.as_ref().map(|session| {
                    surface_close_should_cleanup(
                        &session.model,
                        generation,
                        label,
                        session.external_permission_ui.is_some(),
                    )
                })
            })
            .unwrap_or(false);
        if should_cleanup {
            cleanup(app);
        }
    }

    fn cleanup_main(app: &AppHandle) {
        let Some(runtime) = app.try_state::<OnboardingWindowRuntime>() else {
            return;
        };
        let (labels, permission_generation, display_observer) = {
            let Ok(mut state) = runtime.0.lock() else {
                return;
            };
            state.next_generation = state.next_generation.wrapping_add(1);
            let (labels, permission_generation) = state
                .session
                .take()
                .map(|mut session| (session.model.cleanup(), session.permission_generation))
                .unwrap_or_default();
            (labels, permission_generation, state.display_observer.take())
        };
        destroy_labels(app, &labels);
        if let Some(generation) = permission_generation {
            crate::permissions::mac::stop(app, generation);
        }
        if let Some(observer) = display_observer {
            remove_display_observer(observer);
        }
        crate::onboarding_music::mac::stop(app);
    }

    pub fn cleanup(app: &AppHandle) {
        if MainThreadMarker::new().is_some() {
            cleanup_main(app);
            return;
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let callback_app = app.clone();
        if app
            .run_on_main_thread(move || {
                cleanup_main(&callback_app);
                let _ = sender.send(());
            })
            .is_ok()
        {
            let _ = receiver.recv();
        }
    }

    fn prepare_for_external_permission_ui_main(
        app: &AppHandle,
        kind: ExternalPermissionKind,
    ) -> Result<(), String> {
        let runtime = app
            .try_state::<OnboardingWindowRuntime>()
            .ok_or_else(|| "onboarding window runtime unavailable".to_owned())?;
        let (generation, intro_labels, interactive_label) = {
            let mut state = runtime
                .0
                .lock()
                .map_err(|_| "onboarding window runtime unavailable".to_owned())?;
            let session = state
                .session
                .as_mut()
                .ok_or_else(|| "onboarding window session unavailable".to_owned())?;
            let labels = session
                .model
                .surfaces()
                .iter()
                .filter(|surface| {
                    matches!(
                        surface.surface,
                        OnboardingSurfaceKind::Main | OnboardingSurfaceKind::Ambient
                    )
                })
                .map(|surface| surface.label.clone())
                .collect::<Vec<_>>();
            let interactive = session
                .model
                .surfaces()
                .iter()
                .find(|surface| surface.surface == OnboardingSurfaceKind::Interactive)
                .map(|surface| surface.label.clone());
            session.external_permission_ui = Some(kind);
            (session.model.generation(), labels, interactive)
        };
        let mut operations = NativeExternalPermissionOperations {
            app,
            intro_labels: &intro_labels,
            interactive_label: interactive_label.as_deref(),
        };
        if let Err(error) = enforce_external_permission_barrier(&mut operations) {
            if let Ok(mut state) = runtime.0.lock() {
                if let Some(session) = state.session.as_mut() {
                    if session.model.generation() == generation {
                        session.external_permission_ui = None;
                    }
                }
            }
            return Err(error);
        }
        let mut state = runtime
            .0
            .lock()
            .map_err(|_| "onboarding window runtime unavailable".to_owned())?;
        let session = state
            .session
            .as_mut()
            .ok_or_else(|| "onboarding window session unavailable".to_owned())?;
        if session.model.generation() != generation {
            return Err("onboarding window session changed during permission barrier".to_owned());
        }
        let _ = session.model.prepare_for_external_ui();
        Ok(())
    }

    struct NativeExternalPermissionOperations<'a> {
        app: &'a AppHandle,
        intro_labels: &'a [String],
        interactive_label: Option<&'a str>,
    }

    impl ExternalPermissionWindowOps for NativeExternalPermissionOperations<'_> {
        fn close_intro_windows(&mut self) -> Result<(), String> {
            for label in self.intro_labels {
                let Some(window) = self.app.get_webview_window(label) else {
                    continue;
                };
                dismantle_ambient_panel(&window);
                window
                    .hide()
                    .map_err(|error| format!("intro window hide failed: {error}"))?;
                window
                    .destroy()
                    .map_err(|error| format!("intro window destroy failed: {error}"))?;
            }
            Ok(())
        }

        fn lower_interactive_window(&mut self) -> Result<(), String> {
            let Some(label) = self.interactive_label else {
                return Ok(());
            };
            let window = self
                .app
                .get_webview_window(label)
                .ok_or_else(|| "interactive onboarding window unavailable".to_owned())?;
            configure_interactive_window(&window, None)
        }
    }

    pub fn prepare_for_external_permission_ui(
        app: &AppHandle,
        kind: ExternalPermissionKind,
    ) -> Result<(), String> {
        if MainThreadMarker::new().is_some() {
            return prepare_for_external_permission_ui_main(app, kind);
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let callback_app = app.clone();
        app.run_on_main_thread(move || {
            let _ = sender.send(prepare_for_external_permission_ui_main(&callback_app, kind));
        })
        .map_err(|error| format!("external permission barrier dispatch failed: {error}"))?;
        receiver
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "external permission barrier timed out".to_owned())?
    }

    #[tauri::command]
    pub fn onboarding_window_surface(
        expected_generation: u64,
        window: tauri::WebviewWindow,
        app: AppHandle,
    ) -> Result<OnboardingSurface, String> {
        let runtime = app
            .try_state::<OnboardingWindowRuntime>()
            .ok_or_else(|| "onboarding window runtime unavailable".to_owned())?;
        let state = runtime
            .0
            .lock()
            .map_err(|_| "onboarding window runtime unavailable".to_owned())?;
        state
            .session
            .as_ref()
            .ok_or_else(|| "onboarding window session unavailable".to_owned())?
            .model
            .surface_for_label(window.label(), expected_generation)
    }

    fn install_display_observer(app: &AppHandle) {
        let Some(runtime) = app.try_state::<OnboardingWindowRuntime>() else {
            return;
        };
        if runtime
            .0
            .lock()
            .ok()
            .is_none_or(|state| state.display_observer.is_some())
        {
            return;
        }
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        use objc2_foundation::NSString;

        let callback_app = app.clone();
        unsafe {
            let center: *mut AnyObject = msg_send![class!(NSNotificationCenter), defaultCenter];
            if center.is_null() {
                return;
            }
            let name = NSString::from_str("NSApplicationDidChangeScreenParametersNotification");
            let block = block2::RcBlock::new(move |_notification: *mut AnyObject| {
                if let Err(error) = reconcile_displays(&callback_app) {
                    eprintln!("[onboarding] display reconciliation failed: {error}");
                }
            });
            let nil: *mut AnyObject = std::ptr::null_mut();
            let observer: *mut AnyObject = msg_send![center, addObserverForName: &*name, object: nil, queue: nil, usingBlock: &*block];
            if !observer.is_null() {
                if let Ok(mut state) = runtime.0.lock() {
                    state.display_observer = Some(observer as usize);
                }
            }
        }
    }

    fn remove_display_observer(observer: usize) {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};

        // SAFETY: cleanup runs on main thread. Notification center owns token until removal.
        unsafe {
            let center: *mut AnyObject = msg_send![class!(NSNotificationCenter), defaultCenter];
            if !center.is_null() {
                let token = observer as *mut AnyObject;
                let _: () = msg_send![center, removeObserver: token];
            }
        }
    }

    fn reconcile_displays(app: &AppHandle) -> Result<(), String> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "display reconciliation must run on main thread".to_owned())?;
        let displays = read_displays(mtm);
        let cursor = cursor_location();
        let main_display_id = crate::geometry::mac::read_main_display_id(mtm);
        let runtime = app
            .try_state::<OnboardingWindowRuntime>()
            .ok_or_else(|| "onboarding window runtime unavailable".to_owned())?;

        let (
            old_generation,
            old_launch,
            phase_has_intro,
            started_at,
            external_permission_ui,
            candidate,
        ) = {
            let state = runtime
                .0
                .lock()
                .map_err(|_| "onboarding window runtime unavailable".to_owned())?;
            let Some(session) = state.session.as_ref() else {
                return Ok(());
            };
            (
                session.model.generation(),
                session.model.launch_display_id(),
                session.model.intro_active(),
                session.started_at,
                session.external_permission_ui.is_some(),
                session.model.clone(),
            )
        };

        let mut candidate = candidate;
        let old_labels =
            candidate.reconcile_displays(old_generation, displays.clone(), cursor, main_display_id);
        if displays.is_empty() {
            cleanup(app);
            return Ok(());
        }

        if phase_has_intro {
            let Some(old_labels) = old_labels else {
                return Ok(());
            };
            if external_permission_ui {
                let _ = candidate.prepare_for_external_ui();
                let new_generation = candidate.generation();
                {
                    let mut state = runtime
                        .0
                        .lock()
                        .map_err(|_| "onboarding window runtime unavailable".to_owned())?;
                    let Some(session) = state.session.as_mut() else {
                        return Ok(());
                    };
                    if session.model.generation() != old_generation {
                        return Ok(());
                    }
                    session.model = candidate;
                }
                destroy_labels(app, &old_labels);
                if let Some(started_at) = started_at {
                    schedule_deadline(app.clone(), new_generation, started_at);
                }
                return Ok(());
            }
            let windows = build_intro_windows(app, candidate.surfaces(), &displays)?;
            let new_generation = candidate.generation();
            if let Some((surface, window)) = candidate
                .surfaces()
                .iter()
                .zip(&windows)
                .find(|(surface, _)| surface.surface == OnboardingSurfaceKind::Main)
            {
                attach_surface_cleanup(app, window, surface);
            }
            {
                let mut state = runtime
                    .0
                    .lock()
                    .map_err(|_| "onboarding window runtime unavailable".to_owned())?;
                let Some(session) = state.session.as_mut() else {
                    destroy_windows(&windows);
                    return Ok(());
                };
                if session.model.generation() != old_generation {
                    destroy_windows(&windows);
                    return Ok(());
                }
                session.model = candidate;
            }
            destroy_labels(app, &old_labels);
            show_intro_windows(&windows)?;
            if let Some(started_at) = started_at {
                schedule_deadline(app.clone(), new_generation, started_at);
            }
            return Ok(());
        }

        if candidate.launch_display_id() != old_launch {
            let surface = candidate
                .surfaces()
                .first()
                .ok_or_else(|| "interactive onboarding surface missing".to_owned())?;
            let display = display_for(&displays, surface)?;
            if let Some(window) = app.get_webview_window(&surface.label) {
                configure_interactive_window(&window, Some(display))?;
            }
        }
        let mut state = runtime
            .0
            .lock()
            .map_err(|_| "onboarding window runtime unavailable".to_owned())?;
        if let Some(session) = state.session.as_mut() {
            if session.model.generation() == old_generation {
                session.model = candidate;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(
        display_id: u32,
        appkit_frame: Rect,
        cg_frame: Rect,
        scale_factor: f64,
    ) -> DisplaySnapshot {
        DisplaySnapshot {
            display_id,
            appkit_frame,
            cg_frame,
            scale_factor,
        }
    }

    fn displays() -> Vec<DisplaySnapshot> {
        vec![
            display(
                1,
                Rect::new(0.0, 0.0, 1512.0, 982.0),
                Rect::new(0.0, 0.0, 1512.0, 982.0),
                2.0,
            ),
            display(
                2,
                Rect::new(-1920.0, 0.0, 1920.0, 1080.0),
                Rect::new(-1920.0, -98.0, 1920.0, 1080.0),
                1.0,
            ),
            display(
                3,
                Rect::new(0.0, 982.0, 1280.0, 720.0),
                Rect::new(0.0, -720.0, 1280.0, 720.0),
                1.25,
            ),
            display(
                4,
                Rect::new(1512.0, -900.0, 1440.0, 900.0),
                Rect::new(1512.0, 982.0, 1440.0, 900.0),
                1.5,
            ),
        ]
    }

    #[test]
    fn cursor_selection_handles_negative_and_offset_displays() {
        assert_eq!(
            select_cursor_display(&displays(), Point::new(-1.0, 500.0), Some(1)),
            Some(2)
        );
    }

    #[test]
    fn cursor_selection_handles_displays_above_and_below() {
        let screens = displays();
        assert_eq!(
            (
                select_cursor_display(&screens, Point::new(20.0, 1200.0), Some(1)),
                select_cursor_display(&screens, Point::new(1600.0, -200.0), Some(1)),
            ),
            (Some(3), Some(4))
        );
    }

    #[test]
    fn cursor_selection_uses_half_open_edges() {
        let screens = displays();
        assert_eq!(
            select_cursor_display(&screens, Point::new(0.0, 500.0), Some(2)),
            Some(1)
        );
    }

    #[test]
    fn cursor_selection_falls_back_to_main_then_first() {
        let screens = displays();
        assert_eq!(
            (
                select_cursor_display(&screens, Point::new(9000.0, 9000.0), Some(3)),
                select_cursor_display(&screens, Point::new(9000.0, 9000.0), Some(99)),
            ),
            (Some(3), Some(1))
        );
    }

    #[test]
    fn cursor_selection_uses_appkit_frames_with_mixed_scale_pairs() {
        let screens = displays();
        assert_eq!(
            select_cursor_display(&screens, Point::new(1400.0, 800.0), None),
            Some(1)
        );
    }

    #[test]
    fn appkit_window_frames_preserve_negative_and_above_display_geometry() {
        let screens = displays();
        assert_eq!(
            full_display_appkit_frame(screens[1]),
            screens[1].appkit_frame
        );
        assert_eq!(
            interactive_appkit_frame(screens[2]),
            Rect::new(80.0, 982.0, 1120.0, 720.0)
        );
    }

    #[test]
    fn stale_generation_cannot_transition_intro() {
        let mut session = WindowSessionModel::intro(7, displays(), Point::new(50.0, 50.0), Some(1))
            .expect("session");
        assert!(!session.deadline_elapsed(6, INTRO_DURATION));
    }

    #[test]
    fn five_second_transition_occurs_exactly_once() {
        let mut session = WindowSessionModel::intro(7, displays(), Point::new(50.0, 50.0), Some(1))
            .expect("session");
        assert!(!session.deadline_elapsed(7, INTRO_DURATION - Duration::from_nanos(1)));
        assert!(session.deadline_elapsed(7, INTRO_DURATION));
        assert!(!session.deadline_elapsed(7, INTRO_DURATION + Duration::from_secs(1)));
    }

    #[test]
    fn intro_finish_replaces_all_cinematic_surfaces_with_one_interactive_surface() {
        let mut session = WindowSessionModel::intro(7, displays(), Point::new(50.0, 50.0), Some(1))
            .expect("session");
        assert!(session.deadline_elapsed(7, INTRO_DURATION));
        session.finish_intro();
        assert_eq!(
            session.surfaces(),
            &[OnboardingSurface {
                surface: OnboardingSurfaceKind::Interactive,
                generation: 7,
                display_id: 1,
                label: crate::onboarding::mac::ONBOARDING_LABEL.to_owned(),
            }]
        );
    }

    #[test]
    fn cleanup_is_idempotent_and_invalidates_callbacks() {
        let mut session = WindowSessionModel::intro(9, displays(), Point::new(50.0, 50.0), Some(1))
            .expect("session");
        let labels = session.cleanup();
        assert_eq!(labels.len(), 4);
        assert!(session.cleanup().is_empty());
        assert!(!session.deadline_elapsed(9, INTRO_DURATION));
    }

    #[test]
    fn intro_disconnect_rebuild_keeps_main_or_uses_cursor_fallback() {
        let mut session = WindowSessionModel::intro(2, displays(), Point::new(50.0, 50.0), Some(1))
            .expect("session");
        let old = session
            .reconcile_displays(
                2,
                displays()[1..].to_vec(),
                Point::new(-10.0, 20.0),
                Some(3),
            )
            .expect("rebuild");
        assert_eq!(
            (old.len(), session.generation(), session.launch_display_id()),
            (4, 3, 2)
        );
    }

    #[test]
    fn interactive_disconnect_moves_only_when_launch_display_disappears() {
        let screens = displays();
        let mut session =
            WindowSessionModel::intro(3, screens.clone(), Point::new(1.0, 1.0), Some(1))
                .expect("session");
        session.finish_intro();
        assert!(session
            .reconcile_displays(3, screens.clone(), Point::new(-1.0, 2.0), Some(1))
            .is_none());
        let _ =
            session.reconcile_displays(3, screens[1..].to_vec(), Point::new(-1.0, 2.0), Some(3));
        assert_eq!(session.launch_display_id(), 2);
    }

    #[test]
    fn interactive_and_ambient_policies_are_safe() {
        assert_eq!(
            (
                window_policy(OnboardingSurfaceKind::Interactive),
                window_policy(OnboardingSurfaceKind::Ambient),
            ),
            (
                WindowPolicy {
                    level: WindowLevelPolicy::Normal,
                    ignores_mouse_events: false,
                    can_become_key_or_main: true,
                    current_space_only: true,
                },
                WindowPolicy {
                    level: WindowLevelPolicy::Overlay,
                    ignores_mouse_events: true,
                    can_become_key_or_main: false,
                    current_space_only: false,
                },
            )
        );
    }

    #[test]
    fn external_permission_barrier_removes_intro_before_permission_work() {
        let mut session = WindowSessionModel::intro(4, displays(), Point::new(50.0, 50.0), Some(1))
            .expect("session");
        let closed = session.prepare_for_external_ui();
        assert_eq!(closed.len(), 4);
        assert!(session.surfaces().is_empty());
    }

    #[test]
    fn display_change_after_intro_barrier_keeps_deadline_live() {
        let mut session = WindowSessionModel::intro(4, displays(), Point::new(50.0, 50.0), Some(1))
            .expect("session");
        let _ = session.prepare_for_external_ui();
        let old = session
            .reconcile_displays(4, displays()[1..].to_vec(), Point::new(-1.0, 1.0), Some(3))
            .expect("reconcile");
        assert!(old.is_empty());
        let _ = session.prepare_for_external_ui();
        assert!(session.deadline_elapsed(5, INTRO_DURATION));
    }

    #[test]
    fn external_ui_suppresses_intro_teardown_but_not_interactive_close() {
        let mut session = WindowSessionModel::intro(8, displays(), Point::new(50.0, 50.0), Some(1))
            .expect("session");
        let main = session.surfaces()[0].label.clone();
        assert!(!surface_close_should_cleanup(&session, 8, &main, true));
        session.finish_intro();
        assert!(surface_close_should_cleanup(
            &session,
            8,
            crate::onboarding::mac::ONBOARDING_LABEL,
            true
        ));
    }

    #[test]
    fn surface_lookup_rejects_stale_generation() {
        let session = WindowSessionModel::intro(12, displays(), Point::new(50.0, 50.0), Some(1))
            .expect("session");
        let label = &session.surfaces()[0].label;
        assert!(session.surface_for_label(label, 11).is_err());
        assert_eq!(
            session
                .surface_for_label(label, 12)
                .expect("surface")
                .generation,
            12
        );
    }

    #[test]
    fn external_permission_barrier_orders_close_before_lower_and_permission() {
        struct Operations(Vec<&'static str>);

        impl ExternalPermissionWindowOps for Operations {
            fn close_intro_windows(&mut self) -> Result<(), String> {
                self.0.push("close_intro");
                Ok(())
            }

            fn lower_interactive_window(&mut self) -> Result<(), String> {
                self.0.push("lower_interactive");
                Ok(())
            }
        }

        let mut operations = Operations(Vec::new());
        enforce_external_permission_barrier(&mut operations).expect("barrier");
        operations.0.push("permission_request");
        assert_eq!(
            operations.0,
            vec!["close_intro", "lower_interactive", "permission_request"]
        );
    }

    #[test]
    fn intro_configuration_failure_rolls_back_current_host() {
        let rolled_back = std::cell::Cell::new(false);
        let result = configure_intro_with_rollback(
            || Err("injected configuration failure".to_owned()),
            || rolled_back.set(true),
        );
        assert!(result.is_err());
        assert!(rolled_back.get());
    }
}
