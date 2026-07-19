//! Self-process CPU accounting (spec §4.2.3).
//!
//! CPU% is `Δcpu_time / Δwall_time × 100`, summed across all threads and **not** divided
//! by core count (Activity-Monitor-compatible, 1 core = 100%). The delta arithmetic in
//! [`CpuMeter`] is platform-independent and unit-tested here; the macOS reader that feeds
//! it (`task_info` / `proc_pid_rusage`) is gated to `target_os = "macos"` and must be
//! verified on-device (T-04 on-device completion).

/// Method tag recorded on every CPU sample so a run uses exactly one (spec §4.2.3).
pub const CPU_METHOD: &str = "task_info";

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

/// macOS reader: total CPU time (user+system, all threads incl. finished) in ns.
///
/// Uses `proc_pid_rusage(RUSAGE_INFO_V4)` which already aggregates finished and live
/// threads; simpler and less error-prone than combining `MACH_TASK_BASIC_INFO` with
/// `TASK_THREAD_TIMES_INFO`. Requires on-device verification (T-04).
#[cfg(target_os = "macos")]
pub fn read_process_cpu_ns() -> std::io::Result<u64> {
    use std::os::raw::c_int;
    // rusage_info_v4 layout (sys/resource.h). We only read the two cpu-time fields.
    #[repr(C)]
    #[derive(Default)]
    struct RUsageInfoV4 {
        ri_uuid: [u8; 16],
        ri_user_time: u64,
        ri_system_time: u64,
        ri_pkg_idle_wkups: u64,
        ri_interrupt_wkups: u64,
        ri_pageins: u64,
        ri_wired_size: u64,
        ri_resident_size: u64,
        ri_phys_footprint: u64,
        ri_proc_start_abstime: u64,
        ri_proc_exit_abstime: u64,
        ri_child_user_time: u64,
        ri_child_system_time: u64,
        ri_child_pkg_idle_wkups: u64,
        ri_child_interrupt_wkups: u64,
        ri_child_pageins: u64,
        ri_child_elapsed_abstime: u64,
        ri_diskio_bytesread: u64,
        ri_diskio_byteswritten: u64,
        ri_cpu_time_qos_default: u64,
        ri_cpu_time_qos_maintenance: u64,
        ri_cpu_time_qos_background: u64,
        ri_cpu_time_qos_utility: u64,
        ri_cpu_time_qos_legacy: u64,
        ri_cpu_time_qos_user_initiated: u64,
        ri_cpu_time_qos_user_interactive: u64,
        ri_billed_system_time: u64,
        ri_serviced_system_time: u64,
        ri_logical_writes: u64,
        ri_lifetime_max_phys_footprint: u64,
        ri_instructions: u64,
        ri_cycles: u64,
        ri_billed_energy: u64,
        ri_serviced_energy: u64,
        ri_interval_max_phys_footprint: u64,
        ri_runnable_time: u64,
        ri_flags: u64,
    }
    const RUSAGE_INFO_V4: c_int = 4;
    extern "C" {
        fn proc_pid_rusage(pid: c_int, flavor: c_int, buffer: *mut std::ffi::c_void) -> c_int;
    }
    let mut info = RUsageInfoV4::default();
    // SAFETY: `info` is a correctly sized, zero-initialized rusage_info_v4 buffer, and
    // the pid is our own. The call fills the POD struct or returns non-zero on error.
    let rc = unsafe {
        proc_pid_rusage(
            std::process::id() as c_int,
            RUSAGE_INFO_V4,
            &mut info as *mut _ as *mut std::ffi::c_void,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // ri_user_time / ri_system_time are already in nanoseconds on macOS.
    Ok(info.ri_user_time.saturating_add(info.ri_system_time))
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
