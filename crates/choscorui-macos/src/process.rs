//! Resource usage of this process, for benchmarks and resource checks.

use crate::ffi::process::{RUSAGE_SELF, Rusage, RusageInfoV2, Timeval, getrusage, process_rusage};
use std::time::Duration;

/// Resource usage of this process at one time.
///
/// # Examples
///
/// ```
/// let usage = choscorui_macos::process_usage().unwrap();
/// assert!(usage.footprint > 0);
/// ```
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProcessUsage {
    /// User plus system CPU time.
    pub cpu: Duration,
    /// The peak resident size in bytes.
    pub peak_rss: u64,
    /// The resident size in bytes.
    pub resident: u64,
    /// The physical footprint in bytes, the figure that Activity Monitor
    /// shows as memory.
    pub footprint: u64,
    /// The wakeups of the process from idle.
    pub idle_wakeups: u64,
    /// The wakeups of the process from interrupts.
    pub interrupt_wakeups: u64,
}

/// Why [`process_usage`] failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessUsageError {
    /// `getrusage` failed, so the CPU time and the peak size are unknown.
    ResourceUsage,
    /// `proc_pid_rusage` failed, so the footprint and the wakeups are
    /// unknown.
    ProcessInfo,
}

impl std::fmt::Display for ProcessUsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResourceUsage => f.write_str("getrusage failed"),
            Self::ProcessInfo => f.write_str("proc_pid_rusage failed"),
        }
    }
}

impl std::error::Error for ProcessUsageError {}

fn duration(time: Timeval) -> Duration {
    let seconds = u64::try_from(time.seconds).unwrap_or(0);
    let micros = u64::try_from(time.microseconds).unwrap_or(0);
    Duration::from_secs(seconds) + Duration::from_micros(micros)
}

/// Reads the resource usage of this process from the kernel.
pub fn process_usage() -> Result<ProcessUsage, ProcessUsageError> {
    let mut usage = Rusage::default();
    if getrusage(RUSAGE_SELF, &mut usage) != 0 {
        return Err(ProcessUsageError::ResourceUsage);
    }
    let mut info = RusageInfoV2::default();
    if !process_rusage(&mut info) {
        return Err(ProcessUsageError::ProcessInfo);
    }
    Ok(ProcessUsage {
        cpu: duration(usage.user) + duration(usage.system),
        peak_rss: u64::try_from(usage.max_rss).unwrap_or(0),
        resident: info.resident,
        footprint: info.footprint,
        idle_wakeups: info.idle_wakeups,
        interrupt_wakeups: info.interrupt_wakeups,
    })
}
