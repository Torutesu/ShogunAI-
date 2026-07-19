//! Rust⇄webview clock offset estimation (spec §4.1).
//!
//! The Rust monotonic clock (`std::time::Instant`) is authoritative. The webview
//! reports `performance.now()` timestamps; to convert them onto the Rust timeline
//! we estimate a fixed offset using an NTP-style minimum-round-trip method.
//!
//! For each `clock_sync` round trip we hold `(rust_send, rust_recv, js_perf)` where
//! `js_perf` is `performance.now()` sampled by the webview when it received the ping.
//! The true JS sample instant lies between `rust_send` and `rust_recv`; using the
//! sample with the smallest RTT minimises the asymmetry error. The offset maps a JS
//! timestamp to Rust monotonic nanoseconds:
//!
//! ```text
//! rust_ns ≈ js_perf_ns + offset_ns
//! ```

/// One clock-sync round trip, all values in nanoseconds on their own clock.
#[derive(Debug, Clone, Copy)]
pub struct SyncSample {
    /// Rust monotonic ns when the ping was sent.
    pub rust_send_ns: u64,
    /// Rust monotonic ns when the ack was received.
    pub rust_recv_ns: u64,
    /// `performance.now()` (ns) captured by the webview on receipt.
    pub js_perf_ns: u64,
}

impl SyncSample {
    fn rtt_ns(&self) -> u64 {
        self.rust_recv_ns.saturating_sub(self.rust_send_ns)
    }
}

/// Accumulates sync samples and yields the best-estimate offset.
#[derive(Debug, Default)]
pub struct OffsetEstimator {
    best: Option<SyncSample>,
}

impl OffsetEstimator {
    pub fn new() -> Self {
        Self { best: None }
    }

    /// Feed one round trip. Keeps the sample with the smallest RTT.
    pub fn observe(&mut self, sample: SyncSample) {
        match self.best {
            Some(cur) if cur.rtt_ns() <= sample.rtt_ns() => {}
            _ => self.best = Some(sample),
        }
    }

    /// Offset in ns such that `rust_ns = js_perf_ns + offset`.
    ///
    /// The JS sample is assumed taken at the midpoint of the best RTT window.
    /// Returns `None` until at least one sample has been observed.
    pub fn offset_ns(&self) -> Option<i64> {
        let s = self.best?;
        let midpoint = s.rust_send_ns as i128 + (s.rtt_ns() as i128) / 2;
        Some((midpoint - s.js_perf_ns as i128) as i64)
    }

    /// Half the best RTT, the dominant error term. Reported alongside all metrics (spec §4.1).
    pub fn est_error_ns(&self) -> Option<u64> {
        self.best.map(|s| s.rtt_ns() / 2)
    }

    /// Convert a webview `performance.now()` ns timestamp onto the Rust timeline.
    pub fn js_to_rust_ns(&self, js_perf_ns: u64) -> Option<u64> {
        let off = self.offset_ns()?;
        Some((js_perf_ns as i64 + off).max(0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_minimum_rtt_sample() {
        let mut est = OffsetEstimator::new();
        // Wide RTT, JS clock 1_000_000 ns behind Rust.
        est.observe(SyncSample { rust_send_ns: 1_000, rust_recv_ns: 5_000, js_perf_ns: 0 });
        // Tight RTT: send=10_000, recv=10_200 (rtt 200), js sampled at ~10_100 → offset ≈ 10_100.
        est.observe(SyncSample { rust_send_ns: 10_000, rust_recv_ns: 10_200, js_perf_ns: 0 });
        let off = est.offset_ns().expect("offset");
        assert_eq!(off, 10_100);
        assert_eq!(est.est_error_ns(), Some(100));
    }

    #[test]
    fn conversion_maps_js_onto_rust_timeline() {
        let mut est = OffsetEstimator::new();
        est.observe(SyncSample { rust_send_ns: 10_000, rust_recv_ns: 10_200, js_perf_ns: 500 });
        // offset = midpoint(10_100) - 500 = 9_600
        assert_eq!(est.offset_ns(), Some(9_600));
        assert_eq!(est.js_to_rust_ns(1_500), Some(11_100));
    }

    #[test]
    fn no_samples_yields_none() {
        let est = OffsetEstimator::new();
        assert_eq!(est.offset_ns(), None);
        assert_eq!(est.js_to_rust_ns(42), None);
    }
}
