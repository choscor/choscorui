//! Resource usage of this process, for benchmarks and resource checks.
//! `getrusage` gives the CPU time, the peak size, and the context
//! switches. `/proc/self/status` gives the current memory.

use std::time::Duration;

/// Resource usage of this process at one time.
///
/// # Examples
///
/// ```
/// let usage = choscorui_linux::process_usage().unwrap();
/// assert!(usage.resident > 0);
/// ```
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProcessUsage {
    /// User plus system CPU time of all threads.
    pub cpu: Duration,
    /// The peak resident size in bytes (`VmHWM`).
    pub peak_rss: u64,
    /// The resident size in bytes (`VmRSS`).
    pub resident: u64,
    /// The memory that only this process holds, in bytes: the resident
    /// anonymous and shared-memory pages plus the swapped pages
    /// (`RssAnon`, `RssShmem`, and `VmSwap`).
    pub footprint: u64,
    /// The times that a thread of the process waited and woke again.
    pub voluntary_switches: u64,
    /// The times that the scheduler stopped a running thread of the
    /// process.
    pub involuntary_switches: u64,
}

/// Why [`process_usage`] failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessUsageError {
    /// `getrusage` failed, so the CPU time and the context switches are
    /// unknown.
    ResourceUsage,
    /// `/proc/self/status` is not readable or lacks a field, so the
    /// memory sizes are unknown.
    ProcessStatus,
}

impl std::fmt::Display for ProcessUsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResourceUsage => f.write_str("getrusage failed"),
            Self::ProcessStatus => f.write_str("/proc/self/status is not readable"),
        }
    }
}

impl std::error::Error for ProcessUsageError {}

/// The memory sizes of `/proc/self/status`, in bytes.
#[derive(Debug, Default, Eq, PartialEq)]
struct Status {
    peak: u64,
    resident: u64,
    footprint: u64,
}

/// The value in bytes of the `kB` field `key` of a status text.
fn field(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let value = line.strip_prefix(key)?.strip_prefix(':')?;
        let kib = value
            .trim()
            .strip_suffix("kB")?
            .trim()
            .parse::<u64>()
            .ok()?;
        Some(kib * 1024)
    })
}

fn parse_status(text: &str) -> Option<Status> {
    let footprint = field(text, "RssAnon")? + field(text, "RssShmem")?;
    Some(Status {
        peak: field(text, "VmHWM")?,
        resident: field(text, "VmRSS")?,
        // A kernel without swap support leaves out `VmSwap`.
        footprint: footprint + field(text, "VmSwap").unwrap_or(0),
    })
}

fn seconds(seconds: i64, micros: i64) -> Duration {
    let seconds = u64::try_from(seconds).unwrap_or(0);
    let micros = u64::try_from(micros).unwrap_or(0);
    Duration::from_secs(seconds) + Duration::from_micros(micros)
}

/// Reads the resource usage of this process from the kernel.
pub fn process_usage() -> Result<ProcessUsage, ProcessUsageError> {
    let usage = crate::ffi::process::self_usage().ok_or(ProcessUsageError::ResourceUsage)?;
    let status = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| parse_status(&text))
        .ok_or(ProcessUsageError::ProcessStatus)?;
    Ok(ProcessUsage {
        cpu: seconds(usage.ru_utime.tv_sec, usage.ru_utime.tv_usec)
            + seconds(usage.ru_stime.tv_sec, usage.ru_stime.tv_usec),
        peak_rss: status.peak,
        resident: status.resident,
        footprint: status.footprint,
        voluntary_switches: u64::try_from(usage.ru_nvcsw).unwrap_or(0),
        involuntary_switches: u64::try_from(usage.ru_nivcsw).unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_fields_parse_in_bytes() {
        let text = "Name:\tbench\nVmHWM:\t  2048 kB\nVmRSS:\t  1024 kB\n\
                    RssAnon:\t 512 kB\nRssFile:\t 400 kB\nRssShmem:\t 100 kB\n\
                    VmSwap:\t 12 kB\nvoluntary_ctxt_switches:\t5\n";
        assert_eq!(
            parse_status(text),
            Some(Status {
                peak: 2048 * 1024,
                resident: 1024 * 1024,
                footprint: (512 + 100 + 12) * 1024,
            })
        );
        // `VmRSSx` is not `VmRSS`, and a missing field fails the parse.
        assert_eq!(field("VmRSSx:\t1 kB\n", "VmRSS"), None);
        assert_eq!(parse_status("VmHWM:\t1 kB\n"), None);
    }

    #[test]
    fn the_kernel_reports_this_process() {
        let first = process_usage().unwrap();
        assert!(first.resident > 0 && first.peak_rss >= first.resident);
        let mut sum = 0u64;
        for value in 0..5_000_000u64 {
            sum = sum.wrapping_add(std::hint::black_box(value));
        }
        std::hint::black_box(sum);
        let second = process_usage().unwrap();
        assert!(second.cpu >= first.cpu);
        assert!(second.voluntary_switches >= first.voluntary_switches);
    }
}
