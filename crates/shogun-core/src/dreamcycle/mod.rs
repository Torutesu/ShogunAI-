//! Dream Cycle orchestration logic (WP3.4, §6.7). The pure, Linux-testable core of the nightly
//! batch: the run-condition [`gate`], the resumable job [`plan`], and the Batch-API failure
//! [`health`] escalation. The job *effects* (Batch-API consolidation, Warm→Cold demotion,
//! Morning-Brief generation) are I/O the daemon runs — they live behind seams, not here — so the
//! scheduling/idempotency rules that must never regress stay unit-tested without a Mac or a network.
//!
//! Invariants this module upholds:
//! - LLM work is Batch-API/Select-KK only (invariant 5) — the plan never routes a job to the Agent
//!   lane; that lane isn't reachable from here.
//! - Batch-API failure never degrades local features (FR-DC-05, see [`health::local_features_blocked`]).

pub mod gate;
pub mod health;
pub mod plan;
/// The nightly execution loop (gate → resume → run jobs). Needs the DB handle, so it is behind
/// the `db` feature.
#[cfg(feature = "db")]
pub mod run;
/// The concrete job effects behind the `DreamJobRunner` seam (consolidation, state maintenance,
/// cold demotion, brief). Needs the DB handle → `db` feature.
#[cfg(feature = "db")]
pub mod jobs;
/// Scheduling glue: input-window math + the DB-driven driver between the macOS timer and the run
/// loop. Needs the DB handle → `db` feature.
#[cfg(feature = "db")]
pub mod schedule;
