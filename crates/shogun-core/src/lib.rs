//! SHOGUN core — pure, platform-independent behavioural logic.
//!
//! The macOS shell (`apps/desktop/src-tauri`) is a thin adapter: it sources OS events
//! (CGEventTap, NSWorkspace, AXUIElement, timers) and feeds them into these modules, then
//! applies the emitted effects. Keeping the logic here makes the notch behaviour (Q2 expand,
//! Q4 false-positive) and the capture walk policy unit-testable without a Mac.
//!
//! Module layout (docs/phase1-implementation-plan.md WP1.1):
//! - [`notch`] — the notch UI: screen geometry / hit regions, hover judgement, the state
//!   machine, and the integrated engine that wires them together.
//! - [`capture`] — the context-capture policy: the bounded accessibility-tree walk.
//!
//! This crate makes no process-boundary assumptions (AR-03): it is a library the Tauri
//! backend hosts today and a future daemon could host unchanged.
//!
//! CLAUDE.md forbids `unwrap()`/`expect()` outside tests; test modules are exempted below.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod capture;
pub mod notch;
