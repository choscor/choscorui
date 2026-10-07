//! Process resource usage (hand-written libSystem declarations) and the
//! machine manifest recorded with every result.

use std::process::Command;
use std::time::Duration;

#[repr(C)]
#[derive(Default)]
struct Timeval {
    seconds: i64,
    microseconds: i32,
}

#[repr(C)]
#[derive(Default)]
struct Rusage {
    user: Timeval,
    system: Timeval,
    /// Bytes on macOS.
    max_rss: i64,
    rest: [i64; 13],
}

unsafe extern "C" {
    fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    fn getpid() -> i32;
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut *mut std::ffi::c_void) -> i32;
}

const RUSAGE_SELF: i32 = 0;
const RUSAGE_INFO_V2: i32 = 2;

#[derive(Clone, Copy, Debug, Default)]
pub struct Usage {
    /// User plus system CPU time.
    pub cpu: Duration,
    pub peak_rss: u64,
    pub resident: u64,
    /// Physical footprint, the figure Activity Monitor reports as memory.
    pub footprint: u64,
    pub idle_wakeups: u64,
    pub interrupt_wakeups: u64,
}

pub fn usage() -> Usage {
    let mut usage = Rusage::default();
    // SAFETY: `Rusage` matches `struct rusage` on 64-bit Darwin.
    let ok = unsafe { getrusage(RUSAGE_SELF, &mut usage) } == 0;
    let to_duration = |t: &Timeval| {
        Duration::from_secs(t.seconds as u64) + Duration::from_micros(t.microseconds as u64)
    };
    // rusage_info_v2: uuid (2 words), user, system, pkg_idle_wkups,
    // interrupt_wkups, pageins, wired, resident, phys_footprint, ... The
    // buffer is larger than the struct.
    let mut info = [0u64; 40];
    // SAFETY: The flavor's struct fits in `info`; the API takes the buffer
    // address typed as `rusage_info_t *`.
    let info_ok = unsafe {
        proc_pid_rusage(
            getpid(),
            RUSAGE_INFO_V2,
            info.as_mut_ptr() as *mut *mut std::ffi::c_void,
        )
    } == 0;
    Usage {
        cpu: if ok {
            to_duration(&usage.user) + to_duration(&usage.system)
        } else {
            Duration::ZERO
        },
        peak_rss: if ok { usage.max_rss as u64 } else { 0 },
        resident: if info_ok { info[8] } else { 0 },
        footprint: if info_ok { info[9] } else { 0 },
        idle_wakeups: if info_ok { info[4] } else { 0 },
        interrupt_wakeups: if info_ok { info[5] } else { 0 },
    }
}

pub fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn command(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".into())
}

/// `key=value` lines describing the machine, OS, toolchain, and build.
pub fn manifest() -> Vec<(String, String)> {
    let memory = command("sysctl", &["-n", "hw.memsize"])
        .parse::<u64>()
        .map(|bytes| format!("{} GiB", bytes / (1 << 30)))
        .unwrap_or_else(|_| "unknown".into());
    let power = command("pmset", &["-g", "batt"])
        .lines()
        .next()
        .and_then(|line| line.split('\'').nth(1))
        .unwrap_or("unknown")
        .to_owned();
    let dirty = !command("git", &["status", "--porcelain", "--untracked-files=no"]).is_empty();
    let revision = command("git", &["rev-parse", "--short", "HEAD"]);
    vec![
        (
            "machine.model".into(),
            command("sysctl", &["-n", "hw.model"]),
        ),
        (
            "machine.cpu".into(),
            command("sysctl", &["-n", "machdep.cpu.brand_string"]),
        ),
        ("machine.memory".into(), memory),
        ("machine.arch".into(), std::env::consts::ARCH.into()),
        ("machine.power".into(), power),
        (
            "os.version".into(),
            command("sw_vers", &["-productVersion"]),
        ),
        ("os.build".into(), command("sw_vers", &["-buildVersion"])),
        ("rust.version".into(), command("rustc", &["-V"])),
        (
            "git.revision".into(),
            if dirty {
                format!("{revision}-dirty")
            } else {
                revision
            },
        ),
        (
            "build.profile".into(),
            if cfg!(debug_assertions) {
                "debug".into()
            } else {
                "release".into()
            },
        ),
    ]
}
