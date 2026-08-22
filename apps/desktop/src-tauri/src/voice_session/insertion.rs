//! Captured Accessibility target validation and guarded dictation delivery.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

use accessibility_sys::{
    kAXEnabledAttribute, kAXErrorSuccess, kAXFocusedUIElementAttribute, kAXIsEditableAttribute,
    kAXRoleAttribute, kAXSelectedTextAttribute, kAXSelectedTextRangeAttribute, kAXValueAttribute,
    kAXValueTypeCFRange, AXUIElementCopyAttributeValue, AXUIElementCreateSystemWide,
    AXUIElementGetPid, AXUIElementIsAttributeSettable, AXUIElementRef,
    AXUIElementSetAttributeValue, AXUIElementSetMessagingTimeout, AXValueCreate, AXValueGetTypeID,
    AXValueGetValue,
};
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation_sys::base::{CFEqual, CFGetTypeID, CFRange, CFRelease, CFTypeRef};
use core_foundation_sys::number::{CFBooleanGetTypeID, CFBooleanGetValue, CFBooleanRef};
use core_foundation_sys::string::{CFStringGetTypeID, CFStringRef};

use super::lifecycle::session_is_processing;

pub(super) const DELIVERY_READY: u8 = 0;
pub(super) const DELIVERY_WRITING: u8 = 1;
pub(super) const DELIVERY_CANCELLED: u8 = 2;
pub(super) const DELIVERY_DONE: u8 = 3;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreateKeyboardEvent(
        source: *const std::ffi::c_void,
        keycode: u16,
        key_down: bool,
    ) -> *mut std::ffi::c_void;
    fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
}

pub(super) struct DictationTarget {
    element: AXUIElementRef,
    pid: i32,
    pub(super) bundle_id: Option<String>,
    role: String,
    original_value: String,
    caret: CFRange,
    direct_ax_writable: bool,
    operation: Mutex<()>,
}

// SAFETY: retained AX element is accessed only while `operation` is held. Atomic state linearizes
// cancellation against first AX mutation.
unsafe impl Send for DictationTarget {}
unsafe impl Sync for DictationTarget {}

impl Drop for DictationTarget {
    fn drop(&mut self) {
        if !self.element.is_null() {
            unsafe { CFRelease(self.element.cast()) };
        }
    }
}

pub(super) struct DeliveryFence {
    pub(super) state: AtomicU8,
    pub(super) operation: Mutex<()>,
}

pub(super) struct DeliveryGuard<'a> {
    state: &'a AtomicU8,
}

impl Drop for DeliveryGuard<'_> {
    fn drop(&mut self) {
        self.state.store(DELIVERY_DONE, Ordering::Release);
    }
}

/// Leave transcript on general pasteboard; no restore because user wants this text.
fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;

    let pasteboard: *mut AnyObject = unsafe { msg_send![class!(NSPasteboard), generalPasteboard] };
    if pasteboard.is_null() {
        return Err("no pasteboard".into());
    }
    let utf8 = NSString::from_str("public.utf8-plain-text");
    let ours = NSString::from_str(text);
    let _: isize = unsafe { msg_send![pasteboard, clearContents] };
    let wrote: bool = unsafe { msg_send![pasteboard, setString: &*ours, forType: &*utf8] };
    wrote
        .then_some(())
        .ok_or_else(|| "could not write the pasteboard".into())
}

fn editable_role(role: &str) -> bool {
    matches!(
        role,
        "AXTextArea" | "AXTextField" | "AXSearchField" | "AXComboBox"
    )
}

fn secure_role(role: &str) -> bool {
    role == "AXSecureTextField"
}

unsafe fn copy_string(element: AXUIElementRef, name: &str) -> Option<String> {
    let attribute = CFString::new(name);
    let mut value: CFTypeRef = std::ptr::null();
    let error = unsafe {
        AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
    };
    if error != kAXErrorSuccess || value.is_null() {
        return None;
    }
    unsafe {
        if CFGetTypeID(value) == CFStringGetTypeID() {
            Some(CFString::wrap_under_create_rule(value as CFStringRef).to_string())
        } else {
            CFRelease(value);
            None
        }
    }
}

unsafe fn copy_bool(element: AXUIElementRef, name: &str) -> Option<bool> {
    let attribute = CFString::new(name);
    let mut value: CFTypeRef = std::ptr::null();
    let error = unsafe {
        AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
    };
    if error != kAXErrorSuccess || value.is_null() {
        return None;
    }
    let result = if unsafe { CFGetTypeID(value) == CFBooleanGetTypeID() } {
        Some(unsafe { CFBooleanGetValue(value as CFBooleanRef) })
    } else {
        None
    };
    unsafe { CFRelease(value) };
    result
}

unsafe fn selected_text_is_settable(element: AXUIElementRef) -> bool {
    let attribute = CFString::new(kAXSelectedTextAttribute);
    let mut settable = 0u8;
    let result = unsafe {
        AXUIElementIsAttributeSettable(element, attribute.as_concrete_TypeRef(), &mut settable)
    };
    result == kAXErrorSuccess && settable != 0
}

unsafe fn target_is_writable(element: AXUIElementRef) -> bool {
    let enabled = unsafe { copy_bool(element, kAXEnabledAttribute) };
    let editable = unsafe { copy_bool(element, kAXIsEditableAttribute) };
    writable_attributes(enabled, editable, unsafe {
        selected_text_is_settable(element)
    })
}

pub(super) fn writable_attributes(
    enabled: Option<bool>,
    editable: Option<bool>,
    settable: bool,
) -> bool {
    enabled == Some(true) && editable == Some(true) && settable
}

/// Web editors can omit AX editable/settability despite a stable value and caret. Keep a guarded
/// paste target, while explicit disabled/non-editable attributes fail closed.
pub(super) fn paste_target_attributes(
    role: &str,
    enabled: Option<bool>,
    editable: Option<bool>,
) -> bool {
    editable_role(role) && !secure_role(role) && enabled != Some(false) && editable != Some(false)
}

unsafe fn copy_range(element: AXUIElementRef) -> Option<CFRange> {
    let attribute = CFString::new(kAXSelectedTextRangeAttribute);
    let mut value: CFTypeRef = std::ptr::null();
    let error = unsafe {
        AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
    };
    if error != kAXErrorSuccess || value.is_null() {
        return None;
    }
    let mut range = CFRange::init(0, 0);
    let valid = unsafe { CFGetTypeID(value) == AXValueGetTypeID() }
        && unsafe {
            AXValueGetValue(
                value.cast_mut().cast(),
                kAXValueTypeCFRange,
                (&mut range as *mut CFRange).cast(),
            )
        };
    unsafe { CFRelease(value) };
    valid.then_some(range)
}

unsafe fn focused_element() -> Option<AXUIElementRef> {
    let system = unsafe { AXUIElementCreateSystemWide() };
    if system.is_null() {
        return None;
    }
    unsafe { AXUIElementSetMessagingTimeout(system, 0.25) };
    let attribute = CFString::new(kAXFocusedUIElementAttribute);
    let mut value: CFTypeRef = std::ptr::null();
    let error = unsafe {
        AXUIElementCopyAttributeValue(system, attribute.as_concrete_TypeRef(), &mut value)
    };
    unsafe { CFRelease(system.cast()) };
    if error != kAXErrorSuccess || value.is_null() {
        return None;
    }
    let element = value.cast_mut().cast();
    unsafe { AXUIElementSetMessagingTimeout(element, 0.25) };
    Some(element)
}

pub(super) fn valid_collapsed_caret(value: &str, range: CFRange) -> bool {
    let Ok(location) = usize::try_from(range.location) else {
        return false;
    };
    range.length == 0 && location <= value.encode_utf16().count()
}

pub(super) fn capture_dictation_target() -> Result<DictationTarget, &'static str> {
    let element = unsafe { focused_element() }.ok_or("no editable text field is focused")?;
    let role = unsafe { copy_string(element, kAXRoleAttribute) }.unwrap_or_default();
    let enabled = unsafe { copy_bool(element, kAXEnabledAttribute) };
    let editable = unsafe { copy_bool(element, kAXIsEditableAttribute) };
    if !paste_target_attributes(&role, enabled, editable) {
        unsafe { CFRelease(element.cast()) };
        return Err("place the caret in an editable text field before dictating");
    }
    let Some(value) = (unsafe { copy_string(element, kAXValueAttribute) }) else {
        unsafe { CFRelease(element.cast()) };
        return Err("the focused text field cannot be verified");
    };
    let Some(caret) = (unsafe { copy_range(element) }) else {
        unsafe { CFRelease(element.cast()) };
        return Err("place a single caret before dictating");
    };
    if !valid_collapsed_caret(&value, caret) {
        unsafe { CFRelease(element.cast()) };
        return Err("dictation only inserts at a single caret; clear the selection first");
    }
    let mut pid = 0i32;
    if unsafe { AXUIElementGetPid(element, &mut pid) } != kAXErrorSuccess || pid <= 0 {
        unsafe { CFRelease(element.cast()) };
        return Err("the focused text field identity cannot be verified");
    }
    let frontmost = crate::display::frontmost_app();
    if frontmost.as_ref().map(|app| app.pid) != Some(pid) {
        unsafe { CFRelease(element.cast()) };
        return Err("the focused application changed before dictation started");
    }
    Ok(DictationTarget {
        element,
        pid,
        bundle_id: frontmost
            .and_then(|app| (!app.bundle_id.trim().is_empty()).then_some(app.bundle_id)),
        role,
        original_value: value,
        caret,
        direct_ax_writable: writable_attributes(enabled, editable, unsafe {
            selected_text_is_settable(element)
        }),
        operation: Mutex::new(()),
    })
}

unsafe fn set_caret(element: AXUIElementRef, caret: CFRange) -> bool {
    let value = unsafe { AXValueCreate(kAXValueTypeCFRange, (&caret as *const CFRange).cast()) };
    if value.is_null() {
        return false;
    }
    let attribute = CFString::new(kAXSelectedTextRangeAttribute);
    let result = unsafe {
        AXUIElementSetAttributeValue(element, attribute.as_concrete_TypeRef(), value.cast())
    };
    unsafe { CFRelease(value.cast()) };
    result == kAXErrorSuccess
}

fn same_focused_element(target: &DictationTarget) -> bool {
    let Some(focused) = (unsafe { focused_element() }) else {
        return false;
    };
    let same = unsafe { CFEqual(focused.cast(), target.element.cast()) };
    unsafe { CFRelease(focused.cast()) };
    same != 0
}

pub(super) fn expected_insert(value: &str, caret: CFRange, transcript: &str) -> Option<String> {
    let units: Vec<u16> = value.encode_utf16().collect();
    let location = usize::try_from(caret.location).ok()?;
    if caret.length != 0 || location > units.len() {
        return None;
    }
    let transcript_units: Vec<u16> = transcript.encode_utf16().collect();
    let mut inserted = Vec::with_capacity(units.len() + transcript_units.len());
    inserted.extend_from_slice(&units[..location]);
    inserted.extend_from_slice(&transcript_units);
    inserted.extend_from_slice(&units[location..]);
    String::from_utf16(&inserted).ok()
}

unsafe fn value_matches(element: AXUIElementRef, expected: &str) -> bool {
    for _ in 0..4 {
        if unsafe { copy_string(element, kAXValueAttribute) }.as_deref() == Some(expected) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    false
}

fn paste_target_state_matches(
    target: &DictationTarget,
    role: Option<&str>,
    value: Option<&str>,
    caret: Option<CFRange>,
    focused: bool,
    enabled: Option<bool>,
    editable: Option<bool>,
) -> bool {
    focused
        && role == Some(target.role.as_str())
        && paste_target_attributes(&target.role, enabled, editable)
        && value == Some(target.original_value.as_str())
        && caret == Some(target.caret)
}

unsafe fn paste_text_at_target(
    target: &DictationTarget,
    transcript: &str,
    expected: &str,
) -> Result<(), String> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;

    const KVK_ANSI_V: u16 = 0x09;
    const FLAG_COMMAND: u64 = 1 << 20;
    const HID_EVENT_TAP: u32 = 0;

    let pasteboard: *mut AnyObject = unsafe { msg_send![class!(NSPasteboard), generalPasteboard] };
    if pasteboard.is_null() {
        return Err("no pasteboard".into());
    }
    let utf8 = NSString::from_str("public.utf8-plain-text");
    let saved: *mut AnyObject = unsafe { msg_send![pasteboard, stringForType: &*utf8] };
    let saved = if saved.is_null() {
        None
    } else {
        Some(unsafe { &*saved.cast::<NSString>() }.to_string())
    };
    let restore = || unsafe {
        let _: isize = msg_send![pasteboard, clearContents];
        if let Some(previous) = &saved {
            let previous = NSString::from_str(previous);
            let _: bool = msg_send![pasteboard, setString: &*previous, forType: &*utf8];
        }
    };
    let ours = NSString::from_str(transcript);
    let _: isize = unsafe { msg_send![pasteboard, clearContents] };
    let wrote: bool = unsafe { msg_send![pasteboard, setString: &*ours, forType: &*utf8] };
    if !wrote {
        restore();
        return Err("could not write the pasteboard".into());
    }
    for key_down in [true, false] {
        let event = unsafe { CGEventCreateKeyboardEvent(std::ptr::null(), KVK_ANSI_V, key_down) };
        if event.is_null() {
            restore();
            return Err("could not synthesise the paste".into());
        }
        unsafe {
            CGEventSetFlags(event, FLAG_COMMAND);
            CGEventPost(HID_EVENT_TAP, event);
            CFRelease(event.cast());
        }
    }
    let landed = unsafe { value_matches(target.element, expected) };
    restore();
    landed
        .then_some(())
        .ok_or_else(|| "the paste did not change the captured field".into())
}

pub(super) fn target_state_matches(
    target: &DictationTarget,
    role: Option<&str>,
    value: Option<&str>,
    caret: Option<CFRange>,
    focused: bool,
    writable: bool,
) -> bool {
    focused
        && role == Some(target.role.as_str())
        && editable_role(&target.role)
        && !secure_role(&target.role)
        && writable
        && value == Some(target.original_value.as_str())
        && caret == Some(target.caret)
}

enum InsertAttempt {
    Cancelled,
    UnsafeBeforeClaim,
    UnsafeAfterClaim,
    Inserted,
}
enum PasteAttempt {
    Cancelled,
    UnsafeBeforeClaim,
    FailedAfterClaim(String),
    Inserted,
}

fn paste_at_captured_caret(
    session: u64,
    target: &DictationTarget,
    delivery_state: &AtomicU8,
    transcript: &str,
) -> PasteAttempt {
    let Ok(_operation) = target.operation.lock() else {
        return PasteAttempt::UnsafeBeforeClaim;
    };
    let mut pid = 0i32;
    if unsafe { AXUIElementGetPid(target.element, &mut pid) } != kAXErrorSuccess
        || pid != target.pid
        || crate::display::frontmost_app().map(|app| app.pid) != Some(target.pid)
        || !paste_target_state_matches(
            target,
            unsafe { copy_string(target.element, kAXRoleAttribute) }.as_deref(),
            unsafe { copy_string(target.element, kAXValueAttribute) }.as_deref(),
            unsafe { copy_range(target.element) },
            same_focused_element(target),
            unsafe { copy_bool(target.element, kAXEnabledAttribute) },
            unsafe { copy_bool(target.element, kAXIsEditableAttribute) },
        )
    {
        return PasteAttempt::UnsafeBeforeClaim;
    }
    let Some(expected) = expected_insert(&target.original_value, target.caret, transcript) else {
        return PasteAttempt::UnsafeBeforeClaim;
    };
    if !session_is_processing(session) {
        return PasteAttempt::Cancelled;
    }
    if delivery_state
        .compare_exchange(
            DELIVERY_READY,
            DELIVERY_WRITING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return PasteAttempt::Cancelled;
    }
    match unsafe { paste_text_at_target(target, transcript, &expected) } {
        Ok(()) => PasteAttempt::Inserted,
        Err(error) => PasteAttempt::FailedAfterClaim(error),
    }
}

fn insert_at_captured_caret(
    session: u64,
    target: &DictationTarget,
    delivery_state: &AtomicU8,
    transcript: &str,
) -> InsertAttempt {
    let Ok(_operation) = target.operation.lock() else {
        return InsertAttempt::UnsafeBeforeClaim;
    };
    let mut pid = 0i32;
    if unsafe { AXUIElementGetPid(target.element, &mut pid) } != kAXErrorSuccess
        || pid != target.pid
    {
        return InsertAttempt::UnsafeBeforeClaim;
    }
    if !target_state_matches(
        target,
        unsafe { copy_string(target.element, kAXRoleAttribute) }.as_deref(),
        unsafe { copy_string(target.element, kAXValueAttribute) }.as_deref(),
        unsafe { copy_range(target.element) },
        same_focused_element(target),
        unsafe { target_is_writable(target.element) },
    ) {
        return InsertAttempt::UnsafeBeforeClaim;
    }
    if !session_is_processing(session) {
        return InsertAttempt::Cancelled;
    }
    if delivery_state
        .compare_exchange(
            DELIVERY_READY,
            DELIVERY_WRITING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return InsertAttempt::Cancelled;
    }
    if !unsafe { set_caret(target.element, target.caret) }
        || unsafe { copy_range(target.element) } != Some(target.caret)
    {
        return InsertAttempt::UnsafeAfterClaim;
    }
    let Some(expected) = expected_insert(&target.original_value, target.caret, transcript) else {
        return InsertAttempt::UnsafeAfterClaim;
    };
    let attribute = CFString::new(kAXSelectedTextAttribute);
    let value = CFString::new(transcript);
    let _result = unsafe {
        AXUIElementSetAttributeValue(
            target.element,
            attribute.as_concrete_TypeRef(),
            value.as_concrete_TypeRef().cast(),
        )
    };
    if !unsafe { value_matches(target.element, &expected) } {
        return InsertAttempt::UnsafeAfterClaim;
    }
    InsertAttempt::Inserted
}

pub(super) fn cancel_delivery_fence(delivery: &DeliveryFence) {
    if delivery.state.load(Ordering::Acquire) == DELIVERY_READY {
        let _ = delivery.state.compare_exchange(
            DELIVERY_READY,
            DELIVERY_CANCELLED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
    // Waits for AX writes and clipboard fallback. If cancellation wins, queued worker sees CANCELLED.
    drop(delivery.operation.lock());
}

pub(super) enum DeliveryOutcome {
    Inserted,
    Copied,
    CopyFailed(String),
}

fn claim_clipboard_delivery(session: u64, delivery: &DeliveryFence) -> Option<DeliveryGuard<'_>> {
    if !session_is_processing(session) {
        return None;
    }
    if delivery
        .state
        .compare_exchange(
            DELIVERY_READY,
            DELIVERY_WRITING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return None;
    }
    Some(DeliveryGuard {
        state: &delivery.state,
    })
}

fn copy_claimed(transcript: &str) -> DeliveryOutcome {
    match copy_to_clipboard(transcript) {
        Ok(()) => DeliveryOutcome::Copied,
        Err(error) => DeliveryOutcome::CopyFailed(error),
    }
}

pub(super) fn deliver_dictation(
    session: u64,
    target: Option<&DictationTarget>,
    delivery: &DeliveryFence,
    transcript: &str,
) -> Option<DeliveryOutcome> {
    let _delivery_operation = delivery.operation.lock().ok()?;
    match target {
        Some(target) if !target.direct_ax_writable => {
            match paste_at_captured_caret(session, target, &delivery.state, transcript) {
                PasteAttempt::Inserted => {
                    let _delivery = DeliveryGuard {
                        state: &delivery.state,
                    };
                    Some(DeliveryOutcome::Inserted)
                }
                PasteAttempt::FailedAfterClaim(error) => {
                    eprintln!(
                        "[voice] guarded paste failed: {error}; keeping transcript on clipboard"
                    );
                    let _delivery = DeliveryGuard {
                        state: &delivery.state,
                    };
                    Some(copy_claimed(transcript))
                }
                PasteAttempt::UnsafeBeforeClaim => {
                    let _delivery = claim_clipboard_delivery(session, delivery)?;
                    Some(copy_claimed(transcript))
                }
                PasteAttempt::Cancelled => None,
            }
        }
        Some(target) => {
            match insert_at_captured_caret(session, target, &delivery.state, transcript) {
                InsertAttempt::Inserted => {
                    let _delivery = DeliveryGuard {
                        state: &delivery.state,
                    };
                    Some(DeliveryOutcome::Inserted)
                }
                InsertAttempt::UnsafeAfterClaim => {
                    let _delivery = DeliveryGuard {
                        state: &delivery.state,
                    };
                    Some(copy_claimed(transcript))
                }
                InsertAttempt::UnsafeBeforeClaim => {
                    let _delivery = claim_clipboard_delivery(session, delivery)?;
                    Some(copy_claimed(transcript))
                }
                InsertAttempt::Cancelled => None,
            }
        }
        None => {
            let _delivery = claim_clipboard_delivery(session, delivery)?;
            Some(copy_claimed(transcript))
        }
    }
}
