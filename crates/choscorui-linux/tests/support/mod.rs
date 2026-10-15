//! Test support with std and the public D-Bus binding of the crate: a
//! private bus and fakes of the desktop services.

pub mod bus;
pub mod fake;
pub mod notifications;
pub mod portal;
pub mod watcher;

use choscorui_linux::dbus::Connection;
use std::time::{Duration, Instant};

/// Turns the loops of `connections` until `done` returns a value. It
/// panics after ten seconds.
pub fn pump_until<T>(connections: &[&Connection], mut done: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(value) = done() {
            return value;
        }
        assert!(Instant::now() < deadline, "the condition did not come true");
        for connection in connections {
            connection
                .turn(Some(Duration::from_millis(2)))
                .expect("the connection runs");
        }
    }
}

/// True in the child process of `run_child`.
pub fn in_child() -> bool {
    std::env::var_os("CHOSCORUI_TEST_CHILD").is_some()
}

/// Runs the test `name` of this binary again in a child process with
/// `env` changes (`None` removes a variable), and checks that it passed.
pub fn run_child(name: &str, env: &[(&str, Option<&str>)]) {
    let mut command = std::process::Command::new(std::env::current_exe().expect("the test binary"));
    command
        .args([name, "--exact", "--test-threads=1", "--nocapture"])
        .env("CHOSCORUI_TEST_CHILD", "1");
    for (key, value) in env {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
    }
    let output = command.output().expect("the child test runs");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && text.contains("1 passed"),
        "the child test {name} failed:\n{text}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
