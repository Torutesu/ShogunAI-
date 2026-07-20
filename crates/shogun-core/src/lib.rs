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
//! - [`metrics`] — the always-on SLO histograms (NFR-SLO-00): fixed-bucket latency/percent
//!   tracking with pass/fail against each SLO budget.
//! - [`llm`] — the two model-access lanes (Batch / Agent) with compile-time key separation
//!   (invariant 5) and secret redaction.
//! - [`bus`] — the internal event bus (§5.3, AR-06/07): a non-blocking broadcast with
//!   backpressure-by-drop and a drop metric.
//! - [`dreamcycle`] — the nightly-batch orchestration logic (§6.7): run-condition gate, resumable
//!   job plan, and Batch-API failure escalation. Effects stay behind seams; the rules are pure.
//!
//! This crate makes no process-boundary assumptions (AR-03): it is a library the Tauri
//! backend hosts today and a future daemon could host unchanged.
//!
//! CLAUDE.md forbids `unwrap()`/`expect()` outside tests; test modules are exempted below.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod bus;
pub mod capture;
pub mod dreamcycle;
pub mod llm;
pub mod metrics;
pub mod notch;
pub mod traceview;
