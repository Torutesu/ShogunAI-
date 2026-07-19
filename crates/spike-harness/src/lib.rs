//! SHOGUN Phase 0 measurement harness (spec §4).
//!
//! This crate is the one Phase 0 asset carried into the real implementation (spec §2.1).
//! It owns: the SLO constants ([`slo`]), the Rust⇄webview clock offset ([`clock`]),
//! the JSONL record schema ([`record`]), non-blocking capture + flushing ([`writer`]),
//! aggregation ([`stats`]), the CPU meter ([`cpu`]), and the text digest that keeps
//! captured bodies out of every sink ([`digest`]).
//!
//! Everything here is platform-independent and unit-tested on any host **except** the
//! macOS CPU reader ([`cpu::read_process_cpu_ns`]), which is gated to `target_os = "macos"`
//! and requires on-device verification.
//!
//! CLAUDE.md forbids `unwrap()`/`expect()` outside tests; test modules are exempted below.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod clock;
pub mod cpu;
pub mod digest;
pub mod record;
pub mod slo;
pub mod stats;
pub mod writer;

use std::time::{SystemTime, UNIX_EPOCH};

/// Monotonic clock anchored at process/harness start. `elapsed_ns` is the `mono`
/// field of every [`record::Record`]; it never goes backwards and is immune to wall-clock
/// adjustment (spec §4.1 — the Rust monotonic clock is authoritative).
#[derive(Debug, Clone, Copy)]
pub struct MonoClock {
    start: std::time::Instant,
}

impl Default for MonoClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MonoClock {
    pub fn new() -> Self {
        Self { start: std::time::Instant::now() }
    }

    /// Nanoseconds since this clock was created.
    pub fn elapsed_ns(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64
    }
}

/// Wall-clock epoch milliseconds for the `ts` field. Used only for human-facing
/// timeline display in the report; never for latency math (that uses [`MonoClock`]).
pub fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_clock_is_monotonic() {
        let c = MonoClock::new();
        let a = c.elapsed_ns();
        let b = c.elapsed_ns();
        assert!(b >= a);
    }

    #[test]
    fn epoch_ms_is_populated() {
        // Sanity: after 2020-01-01 (1_577_836_800_000 ms).
        assert!(now_epoch_ms() > 1_577_836_800_000);
    }
}
