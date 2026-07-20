//! Context-capture policy (spec §3.10 / §6.2, carried from the Phase 0 spike).
//!
//! - [`walk_policy`] — the bounded accessibility-tree walk: BFS with depth/element/byte
//!   budgets, role filtering, total skip of `AXSecureTextField` subtrees, and a
//!   cancellation/timebox hook. Pure over an abstract `AxNode`; the macOS adapter implements
//!   `AxNode` over `AXUIElement`.

pub mod dedup;
pub mod exclusion;
pub mod pipeline;
pub mod walk_policy;
