//! Native `.app` drag source for the onboarding permission center.
//!
//! Interaction adapted from PermissionFlow v2.11.2 (MIT), commit
//! `2f2a4b76b1eb2ff7ab815b977be8229853f10bf8`: System Settings receives a real
//! file URL from an AppKit `NSDraggingSession`, not browser drag data. The webview only arms this
//! adapter; the next AppKit drag event carries the running ShogunAI.app bundle.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, MainThreadOnly};
use objc2_app_kit::{
    NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource,
    NSEvent, NSEventType, NSWindow, NSWorkspace,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
};
use tauri::{AppHandle, Manager};

use crate::onboarding::mac::ONBOARDING_LABEL;

static ARMED: AtomicBool = AtomicBool::new(false);
static MONITOR_INSTALLED: AtomicBool = AtomicBool::new(false);

define_class!(
    // SAFETY: NSObject has no subclassing requirements and this class has no ivars or Drop impl.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ()]
    struct PermissionDragSource;

    // SAFETY: NSObjectProtocol adds no implementation requirements.
    unsafe impl NSObjectProtocol for PermissionDragSource {}

    // SAFETY: The selector and ABI match AppKit's NSDraggingSource protocol.
    unsafe impl NSDraggingSource for PermissionDragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn source_operation_mask(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy
        }
    }
);

impl PermissionDragSource {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's init signature is correct and `this` is freshly allocated.
        unsafe { msg_send![super(this), init] }
    }
}

thread_local! {
    static SOURCE: std::cell::OnceCell<Retained<PermissionDragSource>> = const {
        std::cell::OnceCell::new()
    };
    /// A fast drag event can beat pointer-down IPC. Keep it until the main-thread arm arrives.
    static PENDING_EVENT: std::cell::RefCell<Option<Retained<NSEvent>>> = const {
        std::cell::RefCell::new(None)
    };
}

/// Arm the next left-mouse drag from the permission card. Tauri commands may run off-main, so all
/// AppKit work stays in the local event monitor or the explicitly scheduled main-thread closure.
#[tauri::command]
pub fn arm_permission_app_drag(app: AppHandle) -> Result<(), String> {
    current_app_bundle().map(|_| ())?;
    ARMED.store(true, Ordering::Release);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let pending = PENDING_EVENT.with(|event| event.borrow_mut().take());
        if let Some(event) = pending {
            if ARMED.swap(false, Ordering::AcqRel) {
                if let Err(error) = begin_drag(&handle, &event) {
                    eprintln!("[onboarding] pending permission app drag failed: {error}");
                }
            }
        }
    })
    .map_err(|error| format!("could not arm drag on AppKit main thread: {error}"))?;
    Ok(())
}

#[tauri::command]
pub fn disarm_permission_app_drag() {
    ARMED.store(false, Ordering::Release);
}

/// Install one app-lifetime local event monitor. Must be called from Tauri setup on AppKit main.
pub fn install_monitor(app: &AppHandle) {
    use objc2::{class, msg_send};

    if MONITOR_INSTALLED.swap(true, Ordering::AcqRel) {
        return;
    }
    if MainThreadMarker::new().is_none() {
        MONITOR_INSTALLED.store(false, Ordering::Release);
        eprintln!("[onboarding] permission drag monitor skipped outside AppKit main thread");
        return;
    }

    let handle = app.clone();
    let monitor = block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
        if event.is_null() {
            return event;
        }

        // SAFETY: NSEvent supplied this live object synchronously on the AppKit main thread.
        let event_ref = unsafe { &*(event.cast::<NSEvent>()) };
        match event_ref.r#type() {
            NSEventType::LeftMouseUp => {
                ARMED.store(false, Ordering::Release);
                PENDING_EVENT.with(|event| *event.borrow_mut() = None);
            }
            NSEventType::LeftMouseDragged => {
                if ARMED.swap(false, Ordering::AcqRel) {
                    PENDING_EVENT.with(|event| *event.borrow_mut() = None);
                    if let Err(error) = begin_drag(&handle, event_ref) {
                        eprintln!("[onboarding] permission app drag failed: {error}");
                    }
                } else {
                    // SAFETY: retain the callback event until pointer-down IPC reaches main.
                    let retained =
                        unsafe { Retained::retain(event_ref as *const NSEvent as *mut NSEvent) };
                    PENDING_EVENT.with(|event| *event.borrow_mut() = retained);
                }
            }
            _ => {}
        }
        event
    });

    // SAFETY: AppKit main thread; the block passes every event through unchanged. The monitor and
    // block intentionally live for the process lifetime.
    let token: *mut AnyObject = unsafe {
        msg_send![
            class!(NSEvent),
            addLocalMonitorForEventsMatchingMask: (1_u64 << NSEventType::LeftMouseUp.0)
                | (1_u64 << NSEventType::LeftMouseDragged.0),
            handler: &*monitor
        ]
    };
    if token.is_null() {
        MONITOR_INSTALLED.store(false, Ordering::Release);
        eprintln!("[onboarding] permission drag monitor unavailable");
    } else {
        std::mem::forget(monitor);
    }
}

fn begin_drag(app: &AppHandle, event: &NSEvent) -> Result<(), String> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| "drag session requested outside AppKit main thread".to_owned())?;
    let bundle = current_app_bundle()?;
    let bundle_string = NSString::from_str(&bundle.to_string_lossy());
    let file_url = NSURL::fileURLWithPath(&bundle_string);
    let writer = ProtocolObject::from_ref(&*file_url);
    let item = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), writer);

    let icon = NSWorkspace::sharedWorkspace().iconForFile(&bundle_string);
    icon.setSize(NSSize::new(56.0, 56.0));
    let point = event.locationInWindow();
    let frame = NSRect::new(
        NSPoint::new(point.x - 28.0, point.y - 28.0),
        NSSize::new(56.0, 56.0),
    );
    // SAFETY: NSImage is valid dragging-frame content and outlives this call.
    unsafe { item.setDraggingFrame_contents(frame, Some(&icon)) };
    let items = NSArray::from_retained_slice(&[item]);

    let window = app
        .get_webview_window(ONBOARDING_LABEL)
        .ok_or_else(|| "onboarding window is no longer open".to_owned())?;
    let raw_window = window
        .ns_window()
        .map_err(|error| format!("onboarding NSWindow unavailable: {error}"))?;
    if raw_window.is_null() {
        return Err("onboarding NSWindow is null".to_owned());
    }
    // SAFETY: Tauri owns this live window and the local monitor calls us synchronously on main.
    let native_window = unsafe { &*(raw_window.cast::<NSWindow>()) };
    SOURCE.with(|cell| {
        let source = cell.get_or_init(|| PermissionDragSource::new(mtm));
        let source = ProtocolObject::from_ref(&**source);
        let session =
            native_window.beginDraggingSessionWithItems_event_source(&items, event, source);
        session.setAnimatesToStartingPositionsOnCancelOrFail(true);
    });
    Ok(())
}

fn current_app_bundle() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate running executable: {error}"))?;
    app_bundle_from_executable(&executable).ok_or_else(|| {
        "running executable is not inside a packaged .app; build the app to test native drag"
            .to_owned()
    })
}

fn app_bundle_from_executable(executable: &Path) -> Option<PathBuf> {
    executable
        .ancestors()
        .find(|candidate| {
            candidate
                .extension()
                .is_some_and(|extension| extension == "app")
        })
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::app_bundle_from_executable;
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
}
