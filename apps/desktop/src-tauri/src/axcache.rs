//! Context-cache adapter (spec §3.10). AX calls are confined to THIS module.
//!
//! The walk policy lives in `shogun_core::capture::walk_policy` (`walk`, `Limits`, `Role`, `AxNode`,
//! `ContextCache` — depth ≤8/≤300/≤32KB/SecureTextField-skip, unit-tested on Linux). This
//! adapter implements `AxNode` for a retained AXUIElement (value→title→description;
//! `AXUIElementSetMessagingTimeout` 100ms; 250ms timebox via `should_stop`). on-device
//! (T-11) it also subscribes to NSWorkspace/AXObserver focus events and updates the
//! `RwLock<ContextCache>`. The state machine may only READ the cache — never trigger a walk
//! (spec §3.10.3).
#![allow(dead_code, unused_imports)]

pub use shogun_core::capture::walk_policy::{walk, AxNode, ContextCache, Limits, Role, WalkResult};

#[cfg(target_os = "macos")]
pub use mac::{ax_call_count, ax_trusted, browser_url, focused_window, snapshot, AxElement};

#[cfg(target_os = "macos")]
mod mac {
    use accessibility_sys::{
        kAXChildrenAttribute, kAXDescriptionAttribute, kAXDocumentAttribute, kAXErrorSuccess,
        kAXFocusedWindowAttribute, kAXRoleAttribute, kAXTitleAttribute, kAXTrustedCheckOptionPrompt,
        kAXURLAttribute, kAXValueAttribute,
        AXIsProcessTrustedWithOptions, AXUIElementCopyAttributeValue, AXUIElementCreateApplication,
        AXUIElementRef, AXUIElementSetMessagingTimeout,
    };
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;
    use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex, CFArrayRef};
    use core_foundation_sys::base::{CFGetTypeID, CFRelease, CFRetain, CFTypeRef};
    use core_foundation_sys::string::{CFStringGetTypeID, CFStringRef};
    use core_foundation_sys::url::{CFURLGetTypeID, CFURLRef};

    use shogun_core::capture::walk_policy::{walk, AxNode, Limits, Role, WalkResult};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Total AX attribute-copy calls since launch. The harness heartbeat records this and
    /// the "no collect-on-press" proof asserts it doesn't grow during Expanded spans
    /// except via focus events (spec §3.10.3).
    static AX_CALLS: AtomicU64 = AtomicU64::new(0);

    /// Read the cumulative AX call counter.
    pub fn ax_call_count() -> u64 {
        AX_CALLS.load(Ordering::Relaxed)
    }

    /// A retained AXUIElement. Clone = CFRetain, Drop = CFRelease.
    pub struct AxElement(AXUIElementRef);

    impl Clone for AxElement {
        fn clone(&self) -> Self {
            // SAFETY: self.0 is a live retained AXUIElement (a CFType).
            unsafe { CFRetain(self.0 as CFTypeRef) };
            AxElement(self.0)
        }
    }

    impl Drop for AxElement {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: balances the +1 this wrapper owns.
                unsafe { CFRelease(self.0 as CFTypeRef) };
            }
        }
    }

    /// Copy a string attribute (None if absent or not a CFString).
    unsafe fn copy_string(el: AXUIElementRef, name: &str) -> Option<String> {
        AX_CALLS.fetch_add(1, Ordering::Relaxed);
        let cf_name = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: valid element + attribute name; out-pointer is a CFTypeRef slot.
        let err = unsafe { AXUIElementCopyAttributeValue(el, cf_name.as_concrete_TypeRef(), &mut value) };
        if err != kAXErrorSuccess || value.is_null() {
            return None;
        }
        // SAFETY: value is a +1 CFType (create rule); we consume/release it here.
        unsafe {
            if CFGetTypeID(value) == CFStringGetTypeID() {
                Some(CFString::wrap_under_create_rule(value as CFStringRef).to_string())
            } else {
                CFRelease(value);
                None
            }
        }
    }

    /// Copy an element-valued attribute (create rule → owned AxElement).
    /// The returned element gets the 100ms messaging timeout — the timeout is
    /// per-element, so setting it only on the app element leaves children at the ~6s
    /// default and one hung node can blow the 300ms cache budget (review #7).
    unsafe fn copy_element(el: AXUIElementRef, name: &str) -> Option<AxElement> {
        AX_CALLS.fetch_add(1, Ordering::Relaxed);
        let cf_name = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: as above.
        let err = unsafe { AXUIElementCopyAttributeValue(el, cf_name.as_concrete_TypeRef(), &mut value) };
        if err != kAXErrorSuccess || value.is_null() {
            return None;
        }
        let el = value as AXUIElementRef;
        // SAFETY: valid element; 0.1s per-message timeout (spec §3.10.2).
        unsafe { AXUIElementSetMessagingTimeout(el, 0.1) };
        Some(AxElement(el))
    }

    /// Copy the children array as owned AxElements (each retained, each with the 100ms
    /// messaging timeout — see copy_element).
    unsafe fn copy_children(el: AXUIElementRef) -> Vec<AxElement> {
        AX_CALLS.fetch_add(1, Ordering::Relaxed);
        let cf_name = CFString::new(kAXChildrenAttribute);
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: as above.
        let err = unsafe { AXUIElementCopyAttributeValue(el, cf_name.as_concrete_TypeRef(), &mut value) };
        if err != kAXErrorSuccess || value.is_null() {
            return Vec::new();
        }
        // SAFETY: value is a +1 CFType; released at the end of this block.
        unsafe {
            if CFGetTypeID(value) != CFArrayGetTypeID() {
                CFRelease(value);
                return Vec::new();
            }
            let arr = value as CFArrayRef;
            let n = CFArrayGetCount(arr);
            let mut out = Vec::with_capacity(n.max(0) as usize);
            for i in 0..n {
                let child = CFArrayGetValueAtIndex(arr, i) as AXUIElementRef;
                if !child.is_null() {
                    // Array holds borrowed refs (get rule) — retain to own.
                    CFRetain(child as CFTypeRef);
                    AXUIElementSetMessagingTimeout(child, 0.1);
                    out.push(AxElement(child));
                }
            }
            CFRelease(value);
            out
        }
    }

    fn role_of(name: &str) -> Role {
        match name {
            "AXStaticText" => Role::StaticText,
            "AXTextArea" => Role::TextArea,
            "AXTextField" => Role::TextField,
            "AXHeading" => Role::Heading,
            "AXLink" => Role::Link,
            "AXCell" => Role::Cell,
            "AXSecureTextField" => Role::SecureTextField,
            _ => Role::Other,
        }
    }

    impl AxElement {
        /// The window/element title (kAXTitleAttribute), best-effort. Used by the capture source
        /// for the exclusion gate (private-browsing title markers, FR-CAP-05) and the event's
        /// `window_title`.
        pub fn title(&self) -> Option<String> {
            // SAFETY: self.0 is a live element.
            unsafe { copy_string(self.0, kAXTitleAttribute) }
        }
    }

    impl AxNode for AxElement {
        fn role(&self) -> Role {
            // SAFETY: self.0 is a live element.
            match unsafe { copy_string(self.0, kAXRoleAttribute) } {
                Some(r) => role_of(&r),
                None => Role::Other,
            }
        }

        fn value_text(&self) -> Option<String> {
            // value → title → description (spec §3.10.2).
            unsafe { copy_string(self.0, kAXValueAttribute) }
                .or_else(|| unsafe { copy_string(self.0, kAXTitleAttribute) })
                .or_else(|| unsafe { copy_string(self.0, kAXDescriptionAttribute) })
        }

        fn children(&self) -> Vec<Self> {
            // SAFETY: self.0 is a live element.
            unsafe { copy_children(self.0) }
        }
    }

    /// Whether this process is trusted for Accessibility (prompts the user if not).
    pub fn ax_trusted() -> bool {
        // SAFETY: kAXTrustedCheckOptionPrompt is a valid immortal CFString (get rule).
        let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
        let opts = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
        // SAFETY: opts is a valid CFDictionary with the documented option key.
        unsafe { AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef()) }
    }

    /// The focused-window element of `pid` (create rule → owned `AxElement`), with the 100ms
    /// per-message timeout set on both the app and window elements. `None` if the app has no
    /// focused window. Shared by the notch context cache ([`snapshot`]) and the memory capture
    /// source (`capture_source`).
    pub fn focused_window(pid: i32) -> Option<AxElement> {
        // SAFETY: create the app element (+1 create rule); AxElement owns it.
        let app = unsafe { AXUIElementCreateApplication(pid) };
        if app.is_null() {
            return None;
        }
        let _app_owned = AxElement(app);
        // SAFETY: valid element; 0.1s messaging timeout.
        unsafe { AXUIElementSetMessagingTimeout(app, 0.1) };
        // SAFETY: valid app element.
        unsafe { copy_element(app, kAXFocusedWindowAttribute) }
    }

    /// Copy a URL-valued attribute. Browsers hand back a `CFURL` rather than a `CFString`, so
    /// [`copy_string`] returns `None` for these and the value has to be converted explicitly.
    unsafe fn copy_url(el: AXUIElementRef, name: &str) -> Option<String> {
        AX_CALLS.fetch_add(1, Ordering::Relaxed);
        let cf_name = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: valid element + attribute name; out-pointer is a CFTypeRef slot.
        let err = unsafe { AXUIElementCopyAttributeValue(el, cf_name.as_concrete_TypeRef(), &mut value) };
        if err != kAXErrorSuccess || value.is_null() {
            return None;
        }
        // SAFETY: value is a +1 CFType (create rule); consumed or released here. Both shapes are
        // accepted because Safari answers `AXDocument` with a string and Chrome answers `AXURL`
        // with a CFURL.
        unsafe {
            let id = CFGetTypeID(value);
            if id == CFStringGetTypeID() {
                Some(CFString::wrap_under_create_rule(value as CFStringRef).to_string())
            } else if id == CFURLGetTypeID() {
                let url = core_foundation::url::CFURL::wrap_under_create_rule(value as CFURLRef);
                Some(url.get_string().to_string())
            } else {
                CFRelease(value);
                None
            }
        }
    }

    /// The URL of the page the browser is showing, if this app is a browser and exposes one.
    ///
    /// Meeting detection needs the *address*, not the window title (FR-MT-04): a title is what a
    /// page chose to call itself, so matching on it means a document named "… - Google Meet" can
    /// raise an offer to take notes on someone's reading. The URL is the fact.
    ///
    /// Two attributes, because browsers disagree: Safari answers `AXDocument` on the window,
    /// Chromium-based browsers expose `AXURL` on the web area inside it. Both are tried before
    /// giving up, and a browser that answers neither simply is not detected — which is the right
    /// failure, since the alternative is guessing.
    pub fn browser_url(pid: i32) -> Option<String> {
        let window = focused_window(pid)?;
        // SAFETY: `window` is a live retained AXUIElement for the duration of this call.
        if let Some(url) = unsafe { copy_url(window.0, kAXDocumentAttribute) } {
            return Some(url);
        }
        // Chromium: descend to the AXWebArea. Depth is bounded (window → group(s) → web area);
        // this runs once per detection tick, so it must not become a tree walk.
        find_web_area_url(&window, 4)
    }

    /// Depth-limited search for a web area's `AXURL`.
    fn find_web_area_url(el: &AxElement, depth: u8) -> Option<String> {
        if depth == 0 {
            return None;
        }
        // SAFETY: `el` is live for the duration of the call.
        if let Some(role) = unsafe { copy_string(el.0, kAXRoleAttribute) } {
            if role == "AXWebArea" {
                // SAFETY: as above.
                if let Some(url) = unsafe { copy_url(el.0, kAXURLAttribute) } {
                    return Some(url);
                }
            }
        }
        // SAFETY: `el` is live for the duration of the call.
        for child in unsafe { copy_children(el.0) } {
            if let Some(url) = find_web_area_url(&child, depth - 1) {
                return Some(url);
            }
        }
        None
    }

    /// Snapshot the focused window of `pid` into a WalkResult, bounded by `budget_ms`
    /// (spec §3.10.2). Sets a 100ms per-message AX timeout.
    pub fn snapshot(pid: i32, budget_ms: u64) -> Option<WalkResult> {
        let focused = focused_window(pid)?;
        let start = std::time::Instant::now();
        Some(walk(&focused, Limits::default(), || {
            start.elapsed().as_millis() as u64 > budget_ms
        }))
    }
}
