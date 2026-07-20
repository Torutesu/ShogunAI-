//! SHOGUN Context Fusion (spec §6.5): `f(state, screen_ctx, intent) → action candidates`.
//!
//! This crate is the single home of the confidence-band rule (FR-ST-20 / §6.4.6): the mapping
//! from a state record's `confidence` to how it may appear in a generation. It is implemented
//! here once so no agent re-derives it (a CLAUDE.md invariant: low-confidence state must never
//! be mixed into outputs as fact).
//!
//! Platform-independent so the rules are exhaustively Linux-testable. Depends only on
//! shogun-agents (for the L1/L2/L3 permission tags on action candidates).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod assemble;
pub mod brief;
pub mod confidence;

/// Re-export the permission types that appear in this crate's public API ([`assemble::ActionCandidate`]
/// carries an [`Action`] and a [`Level`]), so consumers can name them without a direct dependency on
/// shogun-agents.
pub use shogun_agents::permission::{Action, Level, LocalAction};
