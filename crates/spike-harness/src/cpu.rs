//! Self-process CPU accounting (spec §4.2.3).
//!
//! CPU% is `Δcpu_time / Δwall_time × 100`, summed across all threads and **not** divided
//! by core count (Activity-Monitor-compatible, 1 core = 100%). The delta arithmetic in
//! [`CpuMeter`] is platform-independent and unit-tested here; the macOS reader that feeds
//! it (`task_info` / `proc_pid_rusage`) is gated to `target_os = "macos"` and must be
//! verified on-device (T-04 on-device completion).

/// Method tag recorded on every CPU sample so a run uses exactly one (spec §4.2.3).
/// Must match the actual reader below (`proc_pid_rusage`, RUSAGE_INFO_V4).
pub const CPU_METHOD: &str = "proc_pid_rusage";

/// Computes instantaneous CPU% from successive (cpu_time, wall_time) readings.
#[derive(Debug, Default)]
pub struct CpuMeter {
    last: Option<(u64, u64)>, // (cpu_ns, wall_ns)
}

impl CpuMeter {
    pub fn new() -> Self {
        Self { last: None }
    }

    /// Feed a new absolute reading. Returns CPU% since the previous reading,
    /// or `None` on the first sample (no baseline yet). Non-monotonic or zero-Δwall
    /// readings yield `None` rather than a divide-by-zero or negative percent.
    pub fn sample(&mut self, cpu_ns: u64, wall_ns: u64) -> Option<f64> {
        let out = match self.last {
            Some((last_cpu, last_wall)) if wall_ns > last_wall && cpu_ns >= last_cpu => {
                let d_cpu = (cpu_ns - last_cpu) as f64;
                let d_wall = (wall_ns - last_wall) as f64;
                Some(d_cpu / d_wall * 100.0)
            }
            _ => None,
        };
        self.last = Some((cpu_ns, wall_ns));
        out
    }
}

/// Point-in-time process usage (macOS): CPU time in ns and resident set size.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy)]
pub struct ProcessUsage {
    /// Total CPU time (user+system, all threads incl. finished), nanoseconds.
    pub cpu_ns: u64,
    /// Resident set size in bytes (`ri_resident_size`).
    pub rss_bytes: u64,
}

/// macOS reader via `libc::proc_pid_rusage(RUSAGE_INFO_V4)` — libc owns the struct
/// layout, so there is no hand-rolled 40-field mirror to drift out of sync.
///
/// IMPORTANT (research / osquery#7459): on Apple Silicon `ri_user_time`/`ri_system_time`
/// are `mach_absolute_time` TICKS, not ns; converted here via `mach_timebase_info`.
/// MUST be validated against Activity Monitor on-device before trusting Q3-B (spec §4.2.3).
#[cfg(target_os = "macos")]
#[allow(deprecated)] // libc marks mach_timebase_info deprecated in favour of mach2; we
// deliberately use one FFI source (libc) for the whole reader rather than a second crate.
pub fn read_process_usage() -> std::io::Result<ProcessUsage> {
    // SAFETY: rusage_info_v4 is POD; zeroed is a valid initial value for an out-param.
    let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    // SAFETY: pid is our own; buffer points at a correctly sized rusage_info_v4, passed
    // as the `rusage_info_t*` (void**-shaped) parameter per the Apple prototype.
    let rc = unsafe {
        libc::proc_pid_rusage(
            std::process::id() as libc::c_int,
            libc::RUSAGE_INFO_V4,
            &mut info as *mut libc::rusage_info_v4 as *mut libc::rusage_info_t,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut tb = libc::mach_timebase_info { numer: 0, denom: 0 };
    // SAFETY: valid out-param for mach_timebase_info.
    let tb_rc = unsafe { libc::mach_timebase_info(&mut tb) };
    let ticks = info.ri_user_time.saturating_add(info.ri_system_time) as u128;
    let cpu_ns = if tb_rc != 0 || tb.numer == 0 || tb.denom == 0 {
        // Fall back to raw ticks (still monotonic) if the timebase is unavailable.
        ticks as u64
    } else {
        (ticks * tb.numer as u128 / tb.denom as u128) as u64
    };
    Ok(ProcessUsage { cpu_ns, rss_bytes: info.ri_resident_size })
}

/// Back-compat: CPU time only.
#[cfg(target_os = "macos")]
pub fn read_process_cpu_ns() -> std::io::Result<u64> {
    read_process_usage().map(|u| u.cpu_ns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sample_has_no_baseline() {
        let mut m = CpuMeter::new();
        assert_eq!(m.sample(1000, 1000), None);
    }

    #[test]
    fn fifty_percent_utilisation() {
        let mut m = CpuMeter::new();
        assert_eq!(m.sample(0, 0), None);
        // 5ms cpu over 10ms wall = 50%.
        assert_eq!(m.sample(5_000_000, 10_000_000), Some(50.0));
    }

    #[test]
    fn full_core_is_one_hundred_percent() {
        let mut m = CpuMeter::new();
        m.sample(0, 0);
        assert_eq!(m.sample(10_000_000, 10_000_000), Some(100.0));
    }

    #[test]
    fn multi_thread_can_exceed_one_hundred() {
        let mut m = CpuMeter::new();
        m.sample(0, 0);
        // 20ms cpu over 10ms wall = 200% (two busy cores).
        assert_eq!(m.sample(20_000_000, 10_000_000), Some(200.0));
    }

    #[test]
    fn non_monotonic_reading_is_ignored() {
        let mut m = CpuMeter::new();
        m.sample(10, 100);
        assert_eq!(m.sample(5, 50), None); // wall went backwards
    }
}
