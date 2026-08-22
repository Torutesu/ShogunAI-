//! Meeting notes (§6.16, FR-MT群): detect the meeting, offer once, listen, and hand back a
//! Recap that is already work rather than a record of work.
//!
//! The lane is off by default and opt-in (FR-MT-01), and every path out of Recording closes the
//! microphone (FR-MT-12) — both properties are enforced by the state machine rather than by
//! discipline at each call site.

pub mod detect;
pub mod gate;
/// Prompt construction and model-output parsing for the generated minutes (MT4). Pure logic,
/// no network, no feature gate — the wiring layer (slice 2) calls the Batch API.
pub mod minutes;
/// Recap reads the stored interval, so it lives behind the same `db` feature as every other
/// module that touches shogun-memory.
#[cfg(feature = "db")]
pub mod recap;
pub mod settings;
/// Shared on-disk meeting-settings store. Desktop, MCP, CLI, and REST use this one file so a
/// microphone selected through any supported face is applied by the desktop capture lane.
pub mod settings_store;
pub mod statemachine;
