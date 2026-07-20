//! SHOGUN agent execution (spec §6.6): the L1/L2/L3 permission model and (later) the preset
//! agents and execution engine.
//!
//! The load-bearing invariant here is CLAUDE.md #4: **L1 (auto-execute) must never contain an
//! external-send action** (send / post / calendar-create are always L3). This crate encodes
//! that in the type system — see [`permission`] — so it cannot be violated by a mislabel.
//!
//! Platform-independent so the gating is exhaustively Linux-testable; OS effects live in the
//! desktop adapter.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod engine;
pub mod permission;
pub mod presets;
