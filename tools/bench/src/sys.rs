//! Process resource usage, which `choscorui-macos` reads from the
//! kernel, and the machine manifest recorded with every result.

use choscorui_macos::{ProcessUsage, process_usage};
use std::process::Command;

/// The resource usage of this process, or zeros when the kernel does not
/// report it.
pub fn usage() -> ProcessUsage {
    process_usage().unwrap_or_default()
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
