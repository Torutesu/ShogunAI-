//! Meeting notes (§6.16, FR-MT群): detect the meeting, offer once, listen, and hand back a
//! Recap that is already work rather than a record of work.
//!
//! The lane is off by default and opt-in (FR-MT-01), and every path out of Recording closes the
//! microphone (FR-MT-12) — both properties are enforced by the state machine rather than by
//! discipline at each call site.

pub mod detect;
/// Recap reads the stored interval, so it lives behind the same `db` feature as every other
/// module that touches shogun-memory.
#[cfg(feature = "db")]
pub mod recap;
pub mod settings;
pub mod statemachine;
