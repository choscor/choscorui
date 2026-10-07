#![cfg(target_os = "windows")]

use choscorui_core::Size;
use choscorui_windows::{Application, WindowOptions};
use std::time::Duration;

#[test]
fn creates_pumps_and_closes_native_window() {
    let app = Application::new().expect("initialize Win32 application");
    let window = app
        .create_window(WindowOptions::new("choscorui smoke", 480.0, 320.0))
        .expect("create native window");
    window.show();
    app.pump_once(Duration::from_millis(1));
    let Size { width, height } = window.content_size();
    assert!((width - 480.0).abs() <= 2.0, "logical width {width}");
    assert!((height - 320.0).abs() <= 2.0, "logical height {height}");
    window.close();
}

#[test]
fn worker_post_returns_to_ui_thread() {
    let app = Application::new().expect("initialize Win32 application");
    let poster = app.poster();
    let origin = std::thread::current().id();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        poster
            .post(move || sender.send(std::thread::current().id()).unwrap())
            .unwrap();
    })
    .join()
    .unwrap();
    assert!(app.dispatch_next_event(Duration::from_secs(1)));
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
        origin
    );
}
