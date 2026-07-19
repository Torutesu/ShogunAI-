//! Context cache pipeline (spec §3.10). AX calls are confined to THIS module.
//!
//! on-device (T-11): subscribe to `NSWorkspace.didActivateApplicationNotification` and per-
//! app AXObservers; on focus change, walk the focused window (depth ≤8, ≤300 elements,
//! ≤32KB, ≤250ms, MessagingTimeout 100ms), skipping AXSecureTextField entirely, into an
//! in-memory `RwLock<ContextCache>`. The state machine may only READ the current cache;
//! it must never trigger a fetch (the "no collect-on-press" proof — the harness asserts
//! zero AX calls during an Expanded span, spec §3.10.3). Text bodies never leave this
//! module except as a digest (spike_harness::digest).
#![allow(dead_code)]
