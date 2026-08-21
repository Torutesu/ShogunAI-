//! CPU and RSS for the benchmark process, on the platform the benchmark is running on.
//!
//! [`spike_harness::cpu`] owns the delta arithmetic (`Δcpu / Δwall × 100`, 1 core = 100%,
//! Activity-Monitor-compatible) and this module reuses it. What it does not own is a *reader* for
//! anything but macOS — `read_process_usage` is gated to `target_os = "macos"` and needs on-device
//! validation. WP2.6 requires the bench to run on Linux CI, so a Linux reader lives here.
//!
//! It lives in this crate rather than being added to `spike-harness` on purpose: v0.1 changes no
//! production code, and `spike-harness` ships inside the product. A benchmark needing a Linux
//! `/proc` reader is not a reason to grow the crate that runs on the user's machine.
//!
//! On any other platform (Windows, where a developer might build the workspace) sampling returns
//! `None`. The bench still runs and still reports latency and storage; the resource section of the
//! report says `null`, which is the honest answer rather than a fabricated zero.

use spike_harness::cpu::CpuMeter;

/// A point-in-time reading of this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    /// Total CPU time consumed (user + system, all threads), nanoseconds.
    pub cpu_ns: u64,
    /// Resident set size, bytes.
    pub rss_bytes: u64,
}

/// Read the current process's CPU time and RSS, or `None` where no reader exists.
#[cfg(target_os = "linux")]
pub fn read_usage() -> Option<Usage> {
    // `/proc/self/stat` fields 14 and 15 (1-indexed) are utime and stime, in USER_HZ. USER_HZ is
    // fixed at 100 for the /proc ABI regardless of the kernel's internal tick rate — it is part of
    // the stable interface, not a guess about this kernel's configuration.
    const USER_HZ: u64 = 100;
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // The second field is the executable name in parentheses and may itself contain spaces or
    // parentheses, so fields are counted from after the final ')' rather than by splitting the
    // whole line.
    let after_comm = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = after_comm.split_whitespace().collect();
    // After ')' the first field is `state` (field 3), so utime (14) is index 11 and stime is 12.
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    let cpu_ns = (utime + stime).saturating_mul(1_000_000_000 / USER_HZ);

    // VmRSS is reported in kB directly, which avoids needing the page size (and therefore libc).
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let rss_kb: u64 = status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())?;

    Some(Usage { cpu_ns, rss_bytes: rss_kb.saturating_mul(1024) })
}

#[cfg(target_os = "macos")]
pub fn read_usage() -> Option<Usage> {
    let u = spike_harness::cpu::read_process_usage().ok()?;
    Some(Usage { cpu_ns: u.cpu_ns, rss_bytes: u.rss_bytes })
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn read_usage() -> Option<Usage> {
    None
}

/// Accumulates resource readings across a run.
///
/// CPU% is only meaningful between two readings, so the first sample establishes a baseline and
/// produces no percentage. Peak RSS is a running maximum of every reading taken, which means it is
/// a *sampled* peak — a spike between two samples is invisible to it. The report says so.
#[derive(Debug, Default)]
pub struct ResourceTracker {
    meter: CpuMeter,
    cpu_samples: Vec<f64>,
    peak_rss_bytes: u64,
    initial_rss_bytes: Option<u64>,
    supported: bool,
    samples_taken: u64,
}

impl ResourceTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take a reading. Cheap enough (two small `/proc` reads on Linux) to call between phases and
    /// periodically during them.
    pub fn sample(&mut self, wall_ns: u64) {
        let Some(usage) = read_usage() else {
            return;
        };
        self.supported = true;
        self.samples_taken += 1;
        self.peak_rss_bytes = self.peak_rss_bytes.max(usage.rss_bytes);
        if self.initial_rss_bytes.is_none() {
            self.initial_rss_bytes = Some(usage.rss_bytes);
        }
        if let Some(pct) = self.meter.sample(usage.cpu_ns, wall_ns) {
            self.cpu_samples.push(pct);
        }
    }

    /// `None` when the platform has no reader, so the report distinguishes "not measured" from
    /// "measured zero".
    pub fn summary(&self) -> Option<ResourceSummary> {
        if !self.supported {
            return None;
        }
        let mean_cpu_pct = if self.cpu_samples.is_empty() {
            None
        } else {
            Some(self.cpu_samples.iter().sum::<f64>() / self.cpu_samples.len() as f64)
        };
        let peak_cpu_pct = self
            .cpu_samples
            .iter()
            .copied()
            .fold(None, |acc: Option<f64>, x| Some(acc.map_or(x, |a: f64| a.max(x))));
        Some(ResourceSummary {
            samples: self.samples_taken,
            cpu_samples: self.cpu_samples.len(),
            mean_cpu_pct,
            peak_cpu_pct,
            initial_rss_bytes: self.initial_rss_bytes,
            peak_rss_bytes: self.peak_rss_bytes,
        })
    }
}

/// Serializable resource summary.
///
/// `mean_cpu_pct` is an average over the benchmark's own measurement window and nothing else —
/// it is not an idle-CPU figure and must never be compared against
/// [`spike_harness::slo::IDLE_CPU_PCT`], which is defined over a 1-minute idle window.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ResourceSummary {
    pub samples: u64,
    pub cpu_samples: usize,
    pub mean_cpu_pct: Option<f64>,
    pub peak_cpu_pct: Option<f64>,
    pub initial_rss_bytes: Option<u64>,
    /// Sampled maximum: a spike between two samples does not appear here.
    pub peak_rss_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_platform_reports_none_not_zero() {
        let t = ResourceTracker::new();
        // No sample has been taken, so regardless of platform this must not claim a measurement.
        assert!(t.summary().is_none());
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn sampling_yields_a_summary_with_nonzero_rss() {
        let mut t = ResourceTracker::new();
        t.sample(1_000_000);
        t.sample(2_000_000_000);
        let s = t.summary().expect("platform has a reader");
        assert!(s.samples >= 2);
        assert!(s.peak_rss_bytes > 0, "a running process has resident memory");
    }

    #[test]
    fn first_sample_produces_no_cpu_percentage() {
        // CpuMeter needs a baseline; this guards against a run reporting a bogus first value.
        let mut m = CpuMeter::new();
        assert_eq!(m.sample(1_000, 1_000), None);
        assert!(m.sample(2_000, 3_000).is_some());
    }
}
