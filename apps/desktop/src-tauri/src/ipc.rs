//! Rust⇄webview IPC (spec §3.11.2). The message set is CLOSED — do not add messages.
//!
//! Rust→webview events: `state`, `geometry`, `context`, `fs_mode`, `clock_sync`.
//! webview→Rust commands: `painted`, `anim_done`, `interact`, `collapse_request`,
//! `focus_field`, `clock_sync_ack`. The webview's only jobs are class-swap, paint-done
//! notification, and input forwarding (spec §3.11.2); no timers/state/cache in the webview.
//! `context.text` is display-only and must never be echoed back or logged by the webview.
#![allow(dead_code, unused_imports)]
