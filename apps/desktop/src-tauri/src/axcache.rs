//! Context-cache adapter (spec §3.10). AX calls are confined to THIS module.
//!
//! The walk policy lives in `spike_core::axcache` (`walk`, `Limits`, `Role`, `AxNode`,
//! `ContextCache` — depth ≤8/≤300/≤32KB/SecureTextField-skip, unit-tested on Linux).
//! on-device (T-11) this module implements `AxNode` for a wrapped AXUIElement (value→title
//! →description; `AXUIElementSetMessagingTimeout` 100ms; 250ms timebox via `should_stop`),
//! subscribes to NSWorkspace/AXObserver focus events, and updates the `RwLock<ContextCache>`.
//! The state machine may only READ the cache — it must never trigger a walk (spec §3.10.3).
#![allow(dead_code)]

pub use spike_core::axcache::{walk, AxNode, ContextCache, Limits, Role, WalkResult};
