//! SHOGUN Phase 0 spike behavioural logic (pure, platform-independent).
//!
//! The macOS shell (`apps/desktop/src-tauri`) is a thin adapter: it sources OS events
//! (CGEventTap, NSWorkspace, AXUIElement, timers) and feeds them into these modules, then
//! applies the emitted effects. Keeping the logic here makes the Q2 (expand) and Q4
//! (false-positive) behaviour — the parts most likely to hide bugs — unit-testable without
//! a Mac. See `docs/phase0-findings.md` for the adapter boundary.
//!
//! CLAUDE.md forbids `unwrap()`/`expect()` outside tests; test modules are exempted below.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod axcache;
pub mod geometry;
pub mod hover;
pub mod statemachine;
