//! NSPanel creation, attributes, frame, and `ignoresMouseEvents` (spec §3.1).
//!
//! on-device (T-05): swap the Tauri WebviewWindow to an NSPanel via tauri-nspanel v2.1
//! (research item 1: `object_setClass`, `to_panel()`), then apply spec §3.1.2 attributes:
//! styleMask `.borderless | .nonactivatingPanel`, level 25 (fallback 101 — but 101 blocks
//! IME per tauri-nspanel #104, so drop to 25 while the search field is key, spec §3.5),
//! collectionBehavior `.canJoinAllSpaces | .fullScreenAuxiliary | .stationary | .ignoresCycle`,
//! and the Expanded-sized fixed frame (spec §3.1.3). `ignoresMouseEvents` is toggled
//! true in Idle/HoverIntent/Collapsing and false in Expanded.
#![allow(dead_code)]
