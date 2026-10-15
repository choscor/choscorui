//! When the display server goes away, `App::run` stops with
//! `AppError::Disconnected`. It runs only with
//! `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside
//! `scripts/linux/headless-session.sh`, because it opens windows.
//!
//! The test starts a private display server of the `CHOSCORUI_BACKEND`
//! kind: a headless sway for `wayland`, or an Xvfb for `x11`. A child
//! process runs an application on that server. After the first paint of
//! its window, the child kills the server, and `run` must return the
//! error.

#[cfg(target_os = "linux")]
fn main() {
    if std::env::var_os("CHOSCORUI_RUN_LINUX_SMOKE").is_none() {
        return;
    }
    if let Ok(server) = std::env::var(smoke::SERVER_PID) {
        smoke::child(&server);
        return;
    }
    smoke::parent();
}

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
mod smoke {
    use choscorui::platform::NativeAccess;
    use choscorui::prelude::*;
    use std::cell::Cell;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    /// The variable that gives the child the process id of the server.
    pub const SERVER_PID: &str = "CHOSCORUI_TEST_SERVER_PID";

    /// How long a server may take to start, and the child to finish.
    const DEADLINE: Duration = Duration::from_secs(60);

    /// A private display server, and the variables that connect to it.
    struct Server {
        process: Child,
        env: Vec<(&'static str, Option<String>)>,
        /// What to remove after the server stops.
        files: Vec<PathBuf>,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.process.kill();
            let _ = self.process.wait();
            for file in &self.files {
                let _ = std::fs::remove_dir_all(file);
                let _ = std::fs::remove_file(file);
            }
        }
    }

    /// Waits until `ready` is true, or fails after the deadline.
    fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + DEADLINE;
        while !ready() {
            assert!(Instant::now() < deadline, "{what} did not come");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// An Xvfb on a free display number.
    fn xvfb() -> Server {
        let number = (400..600)
            .map(|offset| offset + std::process::id() % 200)
            .find(|number| !Path::new(&format!("/tmp/.X{number}-lock")).exists())
            .expect("a free display number");
        let display = format!(":{number}");
        let process = Command::new("Xvfb")
            .args([
                display.as_str(),
                "-screen",
                "0",
                "1280x800x24",
                "-nolisten",
                "tcp",
                "-noreset",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Xvfb starts");
        let socket = PathBuf::from(format!("/tmp/.X11-unix/X{number}"));
        let server = Server {
            process,
            env: vec![
                ("CHOSCORUI_BACKEND", Some("x11".to_owned())),
                ("DISPLAY", Some(display)),
                ("WAYLAND_DISPLAY", None),
            ],
            files: vec![
                socket.clone(),
                PathBuf::from(format!("/tmp/.X{number}-lock")),
            ],
        };
        wait_until("the Xvfb socket", || socket.exists());
        server
    }

    /// A headless sway in a private runtime directory. Sway refuses to
    /// run as root, so a root test runs it as nobody, as the headless
    /// session does.
    fn sway() -> Server {
        let runtime =
            std::env::temp_dir().join(format!("choscorui-display-lost-{}", std::process::id()));
        std::fs::create_dir_all(&runtime).expect("create the runtime directory");
        let config = runtime.join("sway.conf");
        std::fs::write(
            &config,
            "output HEADLESS-1 resolution 1280x800 position 0 0\n",
        )
        .expect("write the sway configuration");
        let root = Command::new("id")
            .arg("-u")
            .output()
            .is_ok_and(|output| output.stdout.trim_ascii() == b"0");
        let mut command = if root {
            let status = Command::new("chown")
                .arg("-R")
                .arg("nobody")
                .arg(&runtime)
                .status()
                .expect("chown runs");
            assert!(status.success(), "chown failed");
            let mut command = Command::new("setpriv");
            command.args([
                "--reuid=nobody",
                "--regid=nogroup",
                "--clear-groups",
                "sway",
            ]);
            command
        } else {
            Command::new("sway")
        };
        let process = command
            .arg("--unsupported-gpu")
            .arg("-c")
            .arg(&config)
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("WLR_BACKENDS", "headless")
            .env("WLR_RENDERER", "pixman")
            .env("WLR_LIBINPUT_NO_DEVICES", "1")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .env_remove("SWAYSOCK")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sway starts");
        let mut server = Server {
            process,
            env: Vec::new(),
            files: vec![runtime.clone()],
        };
        let mut socket = None;
        wait_until("the sway socket", || {
            socket = std::fs::read_dir(&runtime)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .find(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name.starts_with("wayland-") && !name.ends_with(".lock")
                        })
                });
            socket.is_some()
        });
        // An absolute WAYLAND_DISPLAY names the socket directly.
        let socket = socket.expect("the socket").display().to_string();
        server.env = vec![
            ("CHOSCORUI_BACKEND", Some("wayland".to_owned())),
            ("WAYLAND_DISPLAY", Some(socket)),
            ("DISPLAY", None),
        ];
        server
    }

    /// Starts the private server and runs the child on it.
    pub fn parent() {
        let backend = std::env::var("CHOSCORUI_BACKEND").unwrap_or_default();
        let mut server = match backend.as_str() {
            "wayland" => sway(),
            "x11" => xvfb(),
            _ => {
                println!("display lost smoke: skipped without CHOSCORUI_BACKEND");
                return;
            }
        };
        let mut command = Command::new(std::env::current_exe().expect("the test binary"));
        command.env(SERVER_PID, server.process.id().to_string());
        for (key, value) in &server.env {
            match value {
                Some(value) => command.env(key, value),
                None => command.env_remove(key),
            };
        }
        let mut child = command.spawn().expect("the child starts");
        let deadline = Instant::now() + DEADLINE;
        let status = loop {
            if let Some(status) = child.try_wait().expect("the child runs") {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the application did not stop after the {backend} server went away");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(status.success(), "the {backend} child failed: {status}");
        // The child killed the server; this collects its status.
        let _ = server.process.wait();
        println!("display lost smoke ({backend}): ok");
    }

    /// Runs an application on the private server, and kills the server
    /// after the first paint of the window.
    pub fn child(server: &str) {
        let server = server.to_owned();
        let killed = Rc::new(Cell::new(false));
        let seen = killed.clone();
        let result = App::new()
            .window(WindowOptions::new("Display lost", 320.0, 200.0), || {
                Text::new("The server goes away")
            })
            .on_tick(move |cx| {
                if killed.get() {
                    return;
                }
                let Some(id) = cx.windows().first().copied() else {
                    return;
                };
                if cx
                    .native_window(id)
                    .is_some_and(|window| window.paint_count() > 0)
                {
                    let status = Command::new("kill")
                        .args(["-KILL", &server])
                        .status()
                        .expect("kill runs");
                    assert!(status.success(), "kill failed");
                    killed.set(true);
                }
            })
            .run();
        assert!(seen.get(), "the window did not paint");
        assert!(
            matches!(result, Err(AppError::Disconnected)),
            "run returned {result:?}"
        );
    }
}
