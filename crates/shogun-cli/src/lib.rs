//! SHOGUN CLI (`shogun`) — the terminal/script face of the Memory API (§6.11, FR-API-01). One of
//! three symmetric faces (MCP / CLI / REST) over the same shared dispatcher, so the CLI grants no
//! capability the UI doesn't (invariant 6).
//!
//! The grammar ([`parse`]), the command → tool/level model ([`command`]), and the call resolution
//! ([`plan`]) are pure and Linux-testable; `main.rs` is a thin runner over them. Executing a
//! resolved call round-trips to the running daemon's REST endpoint — wired when the REST face lands.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod command;
pub mod parse;
pub mod plan;
