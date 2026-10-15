//! Process resource usage, which the platform module reads from the
//! kernel, and the machine manifest recorded with every result.

use crate::platform;
use std::process::Command;
use std::time::Duration;

/// The resource usage of this process at one time, on each OS.
#[derive(Clone, Copy, Debug, Default)]
pub struct Usage {
    /// User plus system CPU time.
    pub cpu: Duration,
    /// The peak resident size in bytes.
    pub peak_rss: u64,
    /// The resident size in bytes.
    pub resident: u64,
    /// The memory that only this process holds, in bytes.
    pub footprint: u64,
    /// The wakeups of the process threads.
    pub wakeups: u64,
}

/// The resource usage of this process, or zeros when the kernel does not
/// report it.
pub fn usage() -> Usage {
    platform::usage()
}

pub fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// The trimmed standard output of a program, or `unknown`.
pub fn command(program: &str, args: &[&str]) -> String {
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
    let dirty = !command("git", &["status", "--porcelain", "--untracked-files=no"]).is_empty();
    let revision = command("git", &["rev-parse", "--short", "HEAD"]);
    let mut lines = platform::machine();
    lines.extend([
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
    ]);
    lines
}
