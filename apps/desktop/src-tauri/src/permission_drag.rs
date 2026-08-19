//! Native `.app` drag source for the onboarding permission center.
//!
//! Interaction adapted from PermissionFlow v2.11.2 (MIT), commit
//! `2f2a4b76b1eb2ff7ab815b977be8229853f10bf8`: System Settings receives a real
//! file URL from an AppKit `NSDraggingSession`, not browser drag data. The webview reveals a
//! nonactivating helper; its native view owns input and carries the running ShogunAI.app bundle.

const DRAG_THRESHOLD_POINTS: f64 = 4.0;

pub(crate) const PASTEBOARD_TYPES: [&str; 5] = [
    "public.file-url",
    "public.url",
    "NSFilenamesPboardType",
    "com.apple.pasteboard.promised-file-url",
    "public.utf8-plain-text",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PayloadKind {
    Url,
    Filenames,
    Path,
}

fn payload_kind(pasteboard_type: &str) -> Option<PayloadKind> {
    match pasteboard_type {
        "public.file-url" | "public.url" | "com.apple.pasteboard.promised-file-url" => {
            Some(PayloadKind::Url)
        }
        "NSFilenamesPboardType" => Some(PayloadKind::Filenames),
        "public.utf8-plain-text" => Some(PayloadKind::Path),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct DragCandidate {
    origin: Option<(f64, f64)>,
    started: bool,
}

impl DragCandidate {
    fn mouse_down(&mut self, x: f64, y: f64) {
        self.origin = Some((x, y));
        self.started = false;
    }

    fn mouse_dragged(&mut self, x: f64, y: f64) -> bool {
        let Some((origin_x, origin_y)) = self.origin else {
            return false;
        };
        if self.started {
            return false;
        }
        if (x - origin_x).hypot(y - origin_y) <= DRAG_THRESHOLD_POINTS {
            return false;
        }
        self.started = true;
        true
    }

    fn mouse_up(&mut self) {
        self.origin = None;
        self.started = false;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PermissionHelperKind {
    Accessibility,
    Microphone,
    ScreenRecording,
}

impl PermissionHelperKind {
    pub(crate) fn supports_drag(self) -> bool {
        matches!(self, Self::Accessibility | Self::ScreenRecording)
    }
}

impl From<crate::onboarding_windows::ExternalPermissionKind> for PermissionHelperKind {
    fn from(value: crate::onboarding_windows::ExternalPermissionKind) -> Self {
        match value {
            crate::onboarding_windows::ExternalPermissionKind::Accessibility => Self::Accessibility,
            crate::onboarding_windows::ExternalPermissionKind::Microphone => Self::Microphone,
            crate::onboarding_windows::ExternalPermissionKind::ScreenRecording => {
                Self::ScreenRecording
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DragHelperPhase {
    Visible,
    Dragging,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveHelper {
    generation: u64,
    kind: PermissionHelperKind,
    phase: DragHelperPhase,
}

#[derive(Debug, Default)]
struct DragHelperModel {
    next_generation: u64,
    active: Option<ActiveHelper>,
}

impl DragHelperModel {
    fn begin(&mut self, kind: PermissionHelperKind) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        let generation = self.next_generation;
        self.active = Some(ActiveHelper {
            generation,
            kind,
            phase: DragHelperPhase::Visible,
        });
        generation
    }

    fn active_generation(&self) -> Option<u64> {
        self.active.map(|active| active.generation)
    }

    fn phase(&self) -> Option<DragHelperPhase> {
        self.active.map(|active| active.phase)
    }

    fn drag_began(&mut self, generation: u64) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        if active.generation != generation || !active.kind.supports_drag() {
            return false;
        }
        active.phase = DragHelperPhase::Dragging;
        true
    }

    fn drag_ended(&mut self, generation: u64) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        if active.generation != generation || active.phase != DragHelperPhase::Dragging {
            return false;
        }
        active.phase = DragHelperPhase::Visible;
        true
    }

    fn settings_closed(&mut self, generation: u64) -> bool {
        self.cleanup(generation)
    }

    fn cleanup(&mut self, generation: u64) -> bool {
        if self.active_generation() != Some(generation) {
            return false;
        }
        self.active = None;
        true
    }
}

fn execute_external_action_with(
    barrier: impl FnOnce() -> Result<(), String>,
    prepare_helper: impl FnOnce() -> Result<(), String>,
    request: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    barrier()?;
    prepare_helper()?;
    request()
}

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_graphics::geometry::CGRect;
use core_graphics::window::{
    kCGNullWindowID, kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
    CGWindowListCopyWindowInfo,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSBackingStoreType, NSColor, NSDragOperation,
    NSDraggingContext, NSDraggingFormation, NSDraggingItem, NSDraggingSession, NSDraggingSource,
    NSEvent, NSFloatingWindowLevel, NSFont, NSImageView, NSPanel, NSPasteboard, NSPasteboardType,
    NSPasteboardTypeFileURL, NSPasteboardTypeString, NSPasteboardTypeURL, NSPasteboardWriting,
    NSRunningApplication, NSTextField, NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
    NSWorkspace,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
};
use tauri::{AppHandle, Manager};

use crate::onboarding::mac::ONBOARDING_LABEL;

const HELPER_WIDTH: f64 = 390.0;
const HELPER_HEIGHT: f64 = 108.0;
const HELPER_GAP: f64 = 10.0;
const TRACK_INTERVAL: Duration = Duration::from_millis(100);
const SETTINGS_MISSING_POLL_LIMIT: u8 = 12;
const SYSTEM_SETTINGS_BUNDLE_ID: &str = "com.apple.systempreferences";

#[derive(Debug)]
struct PasteboardWriterIvars {
    url: Retained<NSURL>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; macro drops retained NSURL ivar.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = PasteboardWriterIvars]
    struct AppBundlePasteboardWriter;

    // SAFETY: NSObjectProtocol adds no implementation requirements.
    unsafe impl NSObjectProtocol for AppBundlePasteboardWriter {}

    // SAFETY: Selectors and return ownership match NSPasteboardWriting.
    unsafe impl NSPasteboardWriting for AppBundlePasteboardWriter {
        #[unsafe(method_id(writableTypesForPasteboard:))]
        fn writable_types(
            &self,
            _pasteboard: &NSPasteboard,
        ) -> Retained<NSArray<NSPasteboardType>> {
            let promised = NSString::from_str(PASTEBOARD_TYPES[3]);
            NSArray::from_slice(&[
                unsafe { NSPasteboardTypeFileURL },
                unsafe { NSPasteboardTypeURL },
                #[allow(deprecated)]
                unsafe {
                    objc2_app_kit::NSFilenamesPboardType
                },
                &promised,
                unsafe { NSPasteboardTypeString },
            ])
        }

        #[unsafe(method_id(pasteboardPropertyListForType:))]
        fn property_list(&self, pasteboard_type: &NSPasteboardType) -> Option<Retained<AnyObject>> {
            match payload_kind(&pasteboard_type.to_string()) {
                Some(PayloadKind::Url) => self.ivars().url.absoluteString().map(Into::into),
                Some(PayloadKind::Filenames) => self
                    .ivars()
                    .url
                    .path()
                    .map(|path| NSArray::from_slice(&[&*path]).into()),
                Some(PayloadKind::Path) => self.ivars().url.path().map(Into::into),
                None => None,
            }
        }
    }
);

impl AppBundlePasteboardWriter {
    fn new(mtm: MainThreadMarker, url: Retained<NSURL>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PasteboardWriterIvars { url });
        // SAFETY: freshly allocated NSObject subclass with initialized ivars.
        unsafe { msg_send![super(this), init] }
    }
}

#[derive(Debug)]
struct DragViewIvars {
    generation: u64,
    bundle_url: Retained<NSURL>,
    candidate: Cell<DragCandidate>,
}

define_class!(
    // SAFETY: NSView has no extra subclassing requirements; macro drops retained ivars.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = DragViewIvars]
    struct PermissionDragView;

    // SAFETY: NSObjectProtocol adds no implementation requirements.
    unsafe impl NSObjectProtocol for PermissionDragView {}

    impl PermissionDragView {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let point = self.convertPoint_fromView(event.locationInWindow(), None);
            let mut candidate = self.ivars().candidate.get();
            candidate.mouse_down(point.x, point.y);
            self.ivars().candidate.set(candidate);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let point = self.convertPoint_fromView(event.locationInWindow(), None);
            let mut candidate = self.ivars().candidate.get();
            let should_start = candidate.mouse_dragged(point.x, point.y);
            self.ivars().candidate.set(candidate);
            if should_start {
                self.begin_app_drag(event);
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            let mut candidate = self.ivars().candidate.get();
            candidate.mouse_up();
            self.ivars().candidate.set(candidate);
        }
    }

    // SAFETY: selectors and ABI match NSDraggingSource.
    unsafe impl NSDraggingSource for PermissionDragView {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn source_operation_mask(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy
        }

        #[unsafe(method(ignoreModifierKeysForDraggingSession:))]
        fn ignore_modifier_keys(&self, _session: &NSDraggingSession) -> bool {
            true
        }

        #[unsafe(method(draggingSession:willBeginAtPoint:))]
        fn drag_will_begin(&self, _session: &NSDraggingSession, _point: NSPoint) {
            drag_state_changed(self.ivars().generation, true);
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn drag_ended(
            &self,
            _session: &NSDraggingSession,
            _point: NSPoint,
            _operation: NSDragOperation,
        ) {
            let mut candidate = self.ivars().candidate.get();
            candidate.mouse_up();
            self.ivars().candidate.set(candidate);
            drag_state_changed(self.ivars().generation, false);
        }
    }
);

impl PermissionDragView {
    fn new(mtm: MainThreadMarker, generation: u64, bundle_path: &Path) -> Retained<Self> {
        let path = NSString::from_str(&bundle_path.to_string_lossy());
        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(HELPER_WIDTH, HELPER_HEIGHT),
        );
        let this = Self::alloc(mtm).set_ivars(DragViewIvars {
            generation,
            bundle_url: NSURL::fileURLWithPath(&path),
            candidate: Cell::new(DragCandidate::default()),
        });
        // SAFETY: freshly allocated NSView subclass with initialized ivars.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        view.install_contents(mtm, bundle_path);
        view
    }

    fn install_contents(&self, mtm: MainThreadMarker, bundle_path: &Path) {
        let path = NSString::from_str(&bundle_path.to_string_lossy());
        let icon = NSWorkspace::sharedWorkspace().iconForFile(&path);
        icon.setSize(NSSize::new(52.0, 52.0));
        let image = NSImageView::imageViewWithImage(&icon, mtm);
        image.setFrame(NSRect::new(
            NSPoint::new(18.0, 28.0),
            NSSize::new(52.0, 52.0),
        ));
        let title = NSTextField::labelWithString(
            &NSString::from_str("Drag ShogunAI to the permission list"),
            mtm,
        );
        title.setFrame(NSRect::new(
            NSPoint::new(88.0, 50.0),
            NSSize::new(280.0, 24.0),
        ));
        title.setFont(Some(&NSFont::systemFontOfSize(15.0)));
        let hint =
            NSTextField::labelWithString(&NSString::from_str("Release over System Settings"), mtm);
        hint.setFrame(NSRect::new(
            NSPoint::new(88.0, 29.0),
            NSSize::new(280.0, 20.0),
        ));
        hint.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        self.addSubview(&image);
        self.addSubview(&title);
        self.addSubview(&hint);
    }

    fn begin_app_drag(&self, event: &NSEvent) {
        let writer = AppBundlePasteboardWriter::new(self.mtm(), self.ivars().bundle_url.clone());
        let item = NSDraggingItem::initWithPasteboardWriter(
            NSDraggingItem::alloc(),
            ProtocolObject::from_ref(&*writer),
        );
        let path = self
            .ivars()
            .bundle_url
            .path()
            .unwrap_or_else(|| NSString::from_str(""));
        let icon = NSWorkspace::sharedWorkspace().iconForFile(&path);
        icon.setSize(NSSize::new(56.0, 56.0));
        let point = self.convertPoint_fromView(event.locationInWindow(), None);
        let frame = NSRect::new(
            NSPoint::new(point.x - 28.0, point.y - 28.0),
            NSSize::new(56.0, 56.0),
        );
        // SAFETY: NSImage is valid drag-frame content and AppKit retains it for the call.
        unsafe { item.setDraggingFrame_contents(frame, Some(&icon)) };
        let items = NSArray::from_retained_slice(&[item]);
        let session = self.beginDraggingSessionWithItems_event_source(
            &items,
            event,
            ProtocolObject::from_ref(self),
        );
        session.setAnimatesToStartingPositionsOnCancelOrFail(true);
        session.setDraggingFormation(NSDraggingFormation::None);
    }
}

define_class!(
    // SAFETY: NSPanel supports normal subclassing; class stores no ivars.
    #[unsafe(super = NSPanel)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ()]
    struct PermissionHelperPanel;

    // SAFETY: NSObjectProtocol adds no implementation requirements.
    unsafe impl NSObjectProtocol for PermissionHelperPanel {}

    impl PermissionHelperPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool { false }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool { false }
    }
);

impl PermissionHelperPanel {
    fn new(mtm: MainThreadMarker, content: &PermissionDragView) -> Retained<Self> {
        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(HELPER_WIDTH, HELPER_HEIGHT),
        );
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: standard NSPanel designated initializer on initialized subclass.
        let panel: Retained<Self> = unsafe {
            msg_send![
                super(this),
                initWithContentRect: frame,
                styleMask: NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ]
        };
        // SAFETY: retained Rust owner keeps panel alive after close.
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setContentView(Some(content));
        panel.setLevel(NSFloatingWindowLevel);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        panel.setHidesOnDeactivate(false);
        panel.setOpaque(true);
        panel.setHasShadow(true);
        panel.setBackgroundColor(Some(&NSColor::windowBackgroundColor()));
        panel
    }
}

#[derive(Debug)]
struct PreviousApplication {
    pid: i32,
    bundle_id: Option<String>,
}

struct NativeSession {
    generation: u64,
    kind: PermissionHelperKind,
    bundle_path: Result<PathBuf, String>,
    panel: Option<Retained<PermissionHelperPanel>>,
    tracker_cancel: Arc<AtomicBool>,
    previous_application: Option<PreviousApplication>,
    settings_seen: bool,
    missing_polls: u8,
}

#[derive(Default)]
struct NativeController {
    model: DragHelperModel,
    session: Option<NativeSession>,
}

impl NativeController {
    fn begin(&mut self, app: &AppHandle, kind: PermissionHelperKind) -> u64 {
        self.cleanup(true);
        let generation = self.model.begin(kind);
        let bundle_path = if kind.supports_drag() {
            crate::onboarding::mac::runtime_bundle_identity(app).map(|identity| identity.app_bundle)
        } else {
            Err("microphone permission does not use app dragging".to_owned())
        };
        let tracker_cancel = Arc::new(AtomicBool::new(false));
        self.session = Some(NativeSession {
            generation,
            kind,
            bundle_path,
            panel: None,
            tracker_cancel: Arc::clone(&tracker_cancel),
            previous_application: capture_frontmost_application(),
            settings_seen: false,
            missing_polls: 0,
        });
        start_settings_tracker(app.clone(), generation, tracker_cancel);
        generation
    }

    fn show_helper(&mut self, mtm: MainThreadMarker) -> Result<u64, String> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| "permission action is not active".to_owned())?;
        if !session.kind.supports_drag() {
            return Err("microphone permission does not use app dragging".to_owned());
        }
        if session.panel.is_none() {
            let bundle_path = session.bundle_path.as_ref().map_err(Clone::clone)?;
            let view = PermissionDragView::new(mtm, session.generation, bundle_path);
            let panel = PermissionHelperPanel::new(mtm, &view);
            if panel.canBecomeKeyWindow() || panel.canBecomeMainWindow() {
                return Err("permission helper must remain nonactivating".to_owned());
            }
            session.panel = Some(panel);
        }
        let generation = session.generation;
        self.track_settings(generation);
        Ok(generation)
    }

    fn drag_state_changed(&mut self, generation: u64, began: bool) {
        let changed = if began {
            self.model.drag_began(generation)
        } else {
            self.model.drag_ended(generation)
        };
        if !changed {
            return;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Some(panel) = session.panel.as_ref() else {
            return;
        };
        panel.setIgnoresMouseEvents(began);
        panel.setAlphaValue(if began { 0.72 } else { 1.0 });
        if began {
            panel.orderBack(None);
        } else if session.settings_seen {
            panel.orderFrontRegardless();
        }
    }

    fn track_settings(&mut self, generation: u64) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.generation != generation {
            return;
        }
        let settings_frame = system_settings_window_frame();
        match settings_frame {
            Some(frame) => {
                session.settings_seen = true;
                session.missing_polls = 0;
                if let Some(panel) = session.panel.as_ref() {
                    panel.setFrame_display(helper_frame(frame), true);
                    if self.model.phase() == Some(DragHelperPhase::Visible) {
                        panel.orderFrontRegardless();
                    }
                }
            }
            None if session.settings_seen => {
                session.missing_polls = session.missing_polls.saturating_add(1);
                if session.missing_polls >= SETTINGS_MISSING_POLL_LIMIT {
                    self.model.settings_closed(generation);
                    self.cleanup(true);
                }
            }
            None => {}
        }
    }

    fn cleanup(&mut self, restore_previous: bool) {
        let Some(session) = self.session.take() else {
            return;
        };
        session.tracker_cancel.store(true, Ordering::Release);
        if let Some(panel) = session.panel {
            panel.orderOut(None);
            panel.close();
        }
        self.model.cleanup(session.generation);
        if restore_previous {
            restore_application(session.previous_application);
        }
    }
}

thread_local! {
    static CONTROLLER: RefCell<NativeController> = RefCell::new(NativeController::default());
}

/// Starts one permission action after the shared external-window barrier. The OS request remains
/// last, so onboarding cannot overlap its old webview with System Settings or a native prompt.
pub(crate) fn perform_permission_action(
    app: &AppHandle,
    kind: crate::onboarding_windows::ExternalPermissionKind,
    request: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let helper_kind = PermissionHelperKind::from(kind);
    let callback_app = app.clone();
    execute_external_action_with(
        || crate::onboarding_windows::mac::prepare_for_external_permission_ui(app, kind),
        || {
            let controller_app = callback_app.clone();
            dispatch_main(&callback_app, move || {
                CONTROLLER.with(|controller| {
                    controller.borrow_mut().begin(&controller_app, helper_kind);
                });
                Ok(())
            })
        },
        request,
    )
    .inspect_err(|_| cleanup(app, true))
}

/// Compatibility entry point used by the web card. It only reveals the native helper; its AppKit
/// view owns mouse-down, threshold detection, and the actual drag session.
#[tauri::command]
pub fn arm_permission_app_drag(app: AppHandle) -> Result<(), String> {
    show_permission_app_drag_helper(app)
}

/// Pointer-leave occurs while the pointer moves from the web card to the native panel. Therefore
/// this compatibility command intentionally does not destroy the active helper.
#[tauri::command]
pub fn disarm_permission_app_drag() {}

#[tauri::command]
pub fn show_permission_app_drag_helper(app: AppHandle) -> Result<(), String> {
    dispatch_main(&app, || {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "permission helper requested outside AppKit main thread".to_owned())?;
        CONTROLLER.with(|controller| controller.borrow_mut().show_helper(mtm))?;
        Ok(())
    })
}

#[tauri::command]
pub fn close_permission_app_drag_helper(app: AppHandle) -> Result<(), String> {
    dispatch_main(&app, move || {
        CONTROLLER.with(|controller| controller.borrow_mut().cleanup(true));
        Ok(())
    })
}

pub(crate) fn cleanup(app: &AppHandle, restore_previous: bool) {
    if let Err(error) = app.run_on_main_thread(move || {
        CONTROLLER.with(|controller| controller.borrow_mut().cleanup(restore_previous));
    }) {
        eprintln!("[onboarding] permission helper cleanup failed: {error}");
    }
}

/// Retained for setup compatibility. Native helper views now own their input directly.
pub fn install_monitor(_app: &AppHandle) {}

fn dispatch_main<T: Send + 'static>(
    app: &AppHandle,
    action: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    if MainThreadMarker::new().is_some() {
        return action();
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let _ = sender.send(action());
    })
    .map_err(|error| format!("could not schedule AppKit action: {error}"))?;
    receiver
        .recv()
        .map_err(|error| format!("AppKit action did not complete: {error}"))?
}

fn start_settings_tracker(app: AppHandle, generation: u64, cancel: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        while !cancel.load(Ordering::Acquire) {
            std::thread::sleep(TRACK_INTERVAL);
            if cancel.load(Ordering::Acquire) {
                break;
            }
            let handle = app.clone();
            if app
                .run_on_main_thread(move || {
                    if handle.get_webview_window(ONBOARDING_LABEL).is_none() {
                        CONTROLLER.with(|controller| controller.borrow_mut().cleanup(true));
                    } else {
                        CONTROLLER
                            .with(|controller| controller.borrow_mut().track_settings(generation));
                    }
                })
                .is_err()
            {
                break;
            }
        }
    });
}

fn drag_state_changed(generation: u64, began: bool) {
    CONTROLLER.with(|controller| {
        controller
            .borrow_mut()
            .drag_state_changed(generation, began)
    });
}

fn capture_frontmost_application() -> Option<PreviousApplication> {
    let application = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    Some(PreviousApplication {
        pid: application.processIdentifier(),
        bundle_id: application
            .bundleIdentifier()
            .map(|value| value.to_string()),
    })
}

fn restore_application(previous: Option<PreviousApplication>) {
    let Some(previous) = previous else {
        return;
    };
    let application = NSRunningApplication::runningApplicationWithProcessIdentifier(previous.pid)
        .or_else(|| {
            let identifier = NSString::from_str(previous.bundle_id.as_deref()?);
            NSRunningApplication::runningApplicationsWithBundleIdentifier(&identifier).firstObject()
        });
    if let Some(application) = application {
        #[allow(deprecated)]
        application.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    }
}

fn system_settings_window_frame() -> Option<CGRect> {
    // SAFETY: create-rule CoreGraphics function; CFArray assumes ownership of returned object.
    let raw = unsafe {
        CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        )
    };
    if raw.is_null() {
        return None;
    }
    // SAFETY: checked non-null create-rule CFArray.
    let windows: CFArray<CFDictionary<CFString, *const std::ffi::c_void>> =
        unsafe { TCFType::wrap_under_create_rule(raw) };
    let mut best: Option<CGRect> = None;
    for window in windows.iter() {
        let owner_pid_key = CFString::from_static_string("kCGWindowOwnerPID");
        let layer_key = CFString::from_static_string("kCGWindowLayer");
        let bounds_key = CFString::from_static_string("kCGWindowBounds");
        let Some(owner_pid) = window
            .find(&owner_pid_key)
            .and_then(|value| cf_number_i32(&value))
        else {
            continue;
        };
        let Some(application) =
            NSRunningApplication::runningApplicationWithProcessIdentifier(owner_pid)
        else {
            continue;
        };
        if application
            .bundleIdentifier()
            .as_deref()
            .map(NSString::to_string)
            .as_deref()
            != Some(SYSTEM_SETTINGS_BUNDLE_ID)
            || window
                .find(&layer_key)
                .and_then(|value| cf_number_i32(&value))
                != Some(0)
        {
            continue;
        }
        let Some(bounds) = window.find(&bounds_key) else {
            continue;
        };
        // SAFETY: kCGWindowBounds is a CFDictionary in every CGWindow info record.
        let bounds = unsafe { CFDictionary::wrap_under_get_rule(*bounds as CFDictionaryRef) };
        if let Some(frame) = CGRect::from_dict_representation(&bounds) {
            let is_larger = best.as_ref().is_none_or(|current| {
                frame.size.width * frame.size.height > current.size.width * current.size.height
            });
            if is_larger {
                best = Some(frame);
            }
        }
    }
    best
}

fn cf_number_i32(value: &*const std::ffi::c_void) -> Option<i32> {
    if value.is_null() {
        return None;
    }
    // SAFETY: CGWindow owner/layer values are CFNumbers.
    unsafe { CFNumber::wrap_under_get_rule(*value as _) }.to_i32()
}

fn helper_frame(settings: CGRect) -> NSRect {
    let Some(mtm) = MainThreadMarker::new() else {
        return NSRect::new(
            NSPoint::new(settings.origin.x, settings.origin.y),
            NSSize::new(HELPER_WIDTH, HELPER_HEIGHT),
        );
    };
    let screens = crate::geometry::mac::read_all(mtm);
    let screen = screens
        .iter()
        .find(|screen| {
            settings.origin.x >= screen.cg_screen.x
                && settings.origin.x < screen.cg_screen.x + screen.cg_screen.w
                && settings.origin.y >= screen.cg_screen.y
                && settings.origin.y < screen.cg_screen.y + screen.cg_screen.h
        })
        .or_else(|| screens.first());
    let Some(screen) = screen else {
        return NSRect::new(
            NSPoint::new(settings.origin.x, settings.origin.y),
            NSSize::new(HELPER_WIDTH, HELPER_HEIGHT),
        );
    };
    let settings_x = screen.screen.x + settings.origin.x - screen.cg_screen.x;
    let settings_y = screen.screen.y + screen.screen.h
        - (settings.origin.y - screen.cg_screen.y)
        - settings.size.height;
    let below_y = settings_y - HELPER_HEIGHT - HELPER_GAP;
    let (x, y) = if below_y >= screen.screen.y {
        (settings_x, below_y)
    } else {
        let right_x = settings_x + settings.size.width + HELPER_GAP;
        if right_x + HELPER_WIDTH <= screen.screen.x + screen.screen.w {
            (right_x, settings_y)
        } else {
            (settings_x - HELPER_WIDTH - HELPER_GAP, settings_y)
        }
    };
    NSRect::new(NSPoint::new(x, y), NSSize::new(HELPER_WIDTH, HELPER_HEIGHT))
}

#[cfg(test)]
fn app_bundle_from_executable(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    if macos.file_name()? != "MacOS" {
        return None;
    }
    let contents = macos.parent()?;
    if contents.file_name()? != "Contents" {
        return None;
    }
    let bundle = contents.parent()?;
    (bundle
        .extension()
        .is_some_and(|extension| extension == "app"))
    .then(|| bundle.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::{
        app_bundle_from_executable, execute_external_action_with, payload_kind, DragCandidate,
        DragHelperModel, DragHelperPhase, PayloadKind, PermissionHelperKind, PASTEBOARD_TYPES,
    };
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    #[test]
    fn finds_packaged_app_ancestor() {
        let executable = Path::new("/Applications/ShogunAI.app/Contents/MacOS/shogun-desktop");
        assert_eq!(
            app_bundle_from_executable(executable),
            Some(PathBuf::from("/Applications/ShogunAI.app"))
        );
    }

    #[test]
    fn rejects_unbundled_dev_executable() {
        assert_eq!(
            app_bundle_from_executable(Path::new("/workspace/target/debug/shogun-desktop")),
            None
        );
    }

    #[test]
    fn rejects_nested_tool_inside_app_that_is_not_bundle_executable() {
        assert_eq!(
            app_bundle_from_executable(Path::new(
                "/Applications/ShogunAI.app/Contents/Resources/debug/tool"
            )),
            None
        );
    }

    #[test]
    fn drag_starts_only_after_movement_exceeds_four_points() {
        let mut candidate = DragCandidate::default();
        candidate.mouse_down(10.0, 10.0);

        assert!(!candidate.mouse_dragged(14.0, 10.0));
        assert!(candidate.mouse_dragged(14.1, 10.0));
    }

    #[test]
    fn diagonal_drag_uses_euclidean_distance() {
        let mut candidate = DragCandidate::default();
        candidate.mouse_down(0.0, 0.0);

        assert!(candidate.mouse_dragged(3.0, 3.0));
    }

    #[test]
    fn mouse_up_discards_candidate_so_unrelated_drag_cannot_be_consumed() {
        let mut candidate = DragCandidate::default();
        candidate.mouse_down(0.0, 0.0);
        candidate.mouse_up();

        assert!(!candidate.mouse_dragged(20.0, 20.0));
    }

    #[test]
    fn payload_matrix_matches_permission_flow() {
        assert_eq!(
            PASTEBOARD_TYPES,
            [
                "public.file-url",
                "public.url",
                "NSFilenamesPboardType",
                "com.apple.pasteboard.promised-file-url",
                "public.utf8-plain-text",
            ]
        );
    }

    #[test]
    fn payload_values_match_type_contract() {
        assert_eq!(payload_kind(PASTEBOARD_TYPES[0]), Some(PayloadKind::Url));
        assert_eq!(
            payload_kind(PASTEBOARD_TYPES[2]),
            Some(PayloadKind::Filenames)
        );
        assert_eq!(payload_kind(PASTEBOARD_TYPES[4]), Some(PayloadKind::Path));
    }

    #[test]
    fn controller_keeps_exactly_one_helper() {
        let mut model = DragHelperModel::default();
        let first = model.begin(PermissionHelperKind::Accessibility);
        let second = model.begin(PermissionHelperKind::ScreenRecording);

        assert_ne!(first, second);
        assert_eq!(model.active_generation(), Some(second));
    }

    #[test]
    fn microphone_never_supports_drag_helper() {
        assert!(!PermissionHelperKind::Microphone.supports_drag());
    }

    #[test]
    fn begin_and_end_restore_visible_helper_state() {
        let mut model = DragHelperModel::default();
        let generation = model.begin(PermissionHelperKind::Accessibility);

        assert!(model.drag_began(generation));
        assert_eq!(model.phase(), Some(DragHelperPhase::Dragging));
        assert!(model.drag_ended(generation));
        assert_eq!(model.phase(), Some(DragHelperPhase::Visible));
    }

    #[test]
    fn settings_close_cleans_active_helper() {
        let mut model = DragHelperModel::default();
        let generation = model.begin(PermissionHelperKind::Accessibility);

        assert!(model.settings_closed(generation));
        assert_eq!(model.active_generation(), None);
    }

    #[test]
    fn stale_generation_callbacks_do_not_restore_new_helper() {
        let mut model = DragHelperModel::default();
        let stale = model.begin(PermissionHelperKind::Accessibility);
        let current = model.begin(PermissionHelperKind::ScreenRecording);

        assert!(!model.drag_ended(stale));
        assert_eq!(model.active_generation(), Some(current));
    }

    #[test]
    fn cleanup_is_idempotent() {
        let mut model = DragHelperModel::default();
        let generation = model.begin(PermissionHelperKind::Accessibility);

        assert!(model.cleanup(generation));
        assert!(!model.cleanup(generation));
    }

    #[test]
    fn external_permission_barrier_runs_before_helper_and_request() {
        let order = RefCell::new(Vec::new());
        execute_external_action_with(
            || {
                order.borrow_mut().push("barrier");
                Ok(())
            },
            || {
                order.borrow_mut().push("helper");
                Ok(())
            },
            || {
                order.borrow_mut().push("request");
                Ok(())
            },
        )
        .expect("ordered action");

        assert_eq!(*order.borrow(), ["barrier", "helper", "request"]);
    }
}
