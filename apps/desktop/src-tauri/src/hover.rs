//! Hover detection (spec §3.4). Event-driven; polling is forbidden.
//!
//! on-device (T-07): per research item 2, implement with a listen-only CGEventTap
//! (`kCGEventMouseMoved`, `kCGEventTapOptionListenOnly`) from the start rather than
//! NSEvent global monitor — the global monitor drops mouseMoved during menu tracking and
//! over other apps' fullscreen. Apply the early-reject (top 40pt band), 16ms coalesce, and
//! velocity estimate here; emit intent to `statemachine` over a channel. No allocation or
//! log I/O in the handler (Q3 CPU budget, spec §3.4.1). Requires Accessibility permission
//! (research item 3 — single TCC category covers tap + keyDown + AX).
#![allow(dead_code)]
