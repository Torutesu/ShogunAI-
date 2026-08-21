//! SHOGUN memory benchmark — deterministic workloads and p95 measurement over the real memory
//! layer (WP2.6 of `docs/phase1-implementation-plan.md`, NFR-SLO-04).
//!
//! ```text
//! workloads/  what we run      → a corpus + queries, generated from a seed
//! backend     how we connect   → the MemoryBackend seam over shogun-memory
//! metrics     how we measure   → latency distributions, recall/MRR, amplification, staleness
//! runner      how we execute   → the fixed sequence
//! report      what we save     → one JSON artifact per run, with config + commit
//! ```
//!
//! The point of the layering is the [`backend::MemoryBackend`] seam. A later experiment implements
//! that trait a second time and is measured by this same evaluator, unchanged — which is the only
//! way "the intervention is better" can mean anything.
//!
//! **v0.1 is infrastructure, not a research result.** It establishes deterministic corpora and a
//! system-level baseline. Its deliberate limits, each surfaced in the report rather than hidden:
//! retrieval is lexical-only (no embedder wired in yet, so `mode.semantic` is false); duplicate
//! detection is measured against the workload's own notion of a repeated fact, not a semantic
//! judgement; and staleness is measured by fact supersession, not contradiction detection.
//!
//! CLAUDE.md forbids `unwrap()`/`expect()` outside tests; test modules are exempted below.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod backend;
pub mod config;
pub mod metrics;
pub mod report;
pub mod resources;
pub mod rng;
pub mod runner;
pub mod workload;
pub mod workloads;

pub use config::BenchConfig;
pub use report::BenchReport;
pub use runner::{run, RunError};
