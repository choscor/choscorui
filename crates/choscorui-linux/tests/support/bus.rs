//! A private message bus for one test: a `dbus-daemon` with no service
//! files, so no real portal or notification server can start on it.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

const CONFIG: &str = r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;

/// A running bus. Its drop stops the daemon.
pub struct TestBus {
    child: Child,
    address: String,
    config: PathBuf,
}

impl TestBus {
    /// Starts a bus. It panics with the package name when `dbus-daemon`
    /// is not installed.
    pub fn start() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let config = std::env::temp_dir().join(format!(
            "choscorui-test-bus-{}-{number}.conf",
            std::process::id()
        ));
        std::fs::write(&config, CONFIG).expect("write the bus configuration");
        let mut child = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address=1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("dbus-daemon did not start; install the dbus package");
        let stdout = child.stdout.take().expect("the daemon output");
        let mut address = String::new();
        BufReader::new(stdout)
            .read_line(&mut address)
            .expect("read the bus address");
        let address = address.trim().to_owned();
        assert!(!address.is_empty(), "dbus-daemon printed no address");
        Self {
            child,
            address,
            config,
        }
    }

    /// The address to connect to.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// A new connection of the test thread to the bus.
    pub fn connect(&self) -> choscorui_linux::dbus::Connection {
        choscorui_linux::dbus::Connection::open_private(&self.address)
            .expect("connect to the test bus")
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.config);
    }
}
