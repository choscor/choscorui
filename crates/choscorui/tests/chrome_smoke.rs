//! The chrome that the toolkit draws on Wayland, driven with the virtual
//! keyboard and pointer of `scripts/linux/headless-session.sh`: the
//! in-window menu bar, its shortcuts and keys, a context menu in a popup,
//! and, with `CHOSCORUI_DECORATIONS=client`, the title bar. It runs only
//! with `CHOSCORUI_RUN_LINUX_SMOKE=1` on Wayland.

#[cfg(target_os = "linux")]
fn main() {
    if std::env::var("CHOSCORUI_RUN_LINUX_SMOKE").is_ok_and(|value| value == "1")
        && choscorui::capabilities().backend == "wayland"
    {
        smoke::run();
        println!("chrome smoke: ok");
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
mod smoke {
    use choscorui::core::{MenuCommandId, MenuEntry, MenuItemState, MenuSpec, Point, Rect};
    use choscorui::platform::{ContextMenu, NativeAccess};
    use choscorui::prelude::*;
    use std::process::Command;
    use std::time::{Duration, Instant};

    // Linux input event codes.
    const ENTER: u32 = 28;
    const CTRL: u32 = 29;
    const F: u32 = 33;
    const K: u32 = 37;
    const ALT: u32 = 56;
    const F10: u32 = 68;
    const DOWN: u32 = 108;
    const LEFT_BUTTON: u32 = 272;
    const RIGHT_BUTTON: u32 = 273;

    /// Where sway puts the window, in screen points.
    const ORIGIN: Point = Point::new(100.0, 100.0);

    pub fn run() {
        let app = App::new();
        let count = app.runtime().property(0_u32);
        let counted = count.clone();
        let ctrl = ShortcutModifiers {
            command: true,
            ..ShortcutModifiers::default()
        };
        let menu = CommandMenu::new("File", move |_: &Env| {
            let counted = counted.clone();
            vec![
                MenuChoice::action("Count", move || counted.set(counted.get() + 1))
                    .keyboard_shortcut('k', ctrl),
            ]
        });
        let mut running = app
            .commands([menu])
            .window(WindowOptions::new("Chrome", 400.0, 300.0), || {
                Text::new("content")
            })
            .start()
            .expect("the app must start");
        let window = running.windows()[0];
        pump_until(&mut running, |running| {
            running.native_window(window).unwrap().is_visible()
        });
        place("Chrome", &mut running);
        let csd = !running
            .native_window(window)
            .unwrap()
            .has_server_decorations();

        // The bar shows the menu below the title bar.
        let file = bounds(&mut running, window, "File");
        if csd {
            let close = bounds(&mut running, window, "Close");
            assert!(file.origin.y >= close.max_y(), "{file:?} {close:?}");
            assert!(bounds(&mut running, window, "Minimize").size.width > 0.0);
        }
        let content = bounds(&mut running, window, "content");
        assert!(
            content.origin.y > file.max_y(),
            "the content is below the bars"
        );

        // Focus the window, then press Ctrl+K: the shortcut runs.
        click(&mut running, content.center());
        chord(&mut running, &[CTRL, K]);
        assert_eq!(count.get(), 1, "Ctrl+K runs the command");

        // A click on the title and then on the item runs it.
        click(&mut running, file.center());
        let item = bounds(&mut running, window, "Count");
        assert!(item.max_y() <= 300.0, "the menu shows in the window");
        click(&mut running, item.center());
        assert_eq!(count.get(), 2, "a click runs the command");

        // Alt+F opens the menu with its first row highlighted.
        chord(&mut running, &[ALT, F]);
        chord(&mut running, &[ENTER]);
        assert_eq!(count.get(), 3, "Alt+F, Enter runs the command");

        // F10 focuses the bar, and ArrowDown opens the menu.
        chord(&mut running, &[F10]);
        chord(&mut running, &[DOWN]);
        chord(&mut running, &[ENTER]);
        assert_eq!(count.get(), 4, "F10, ArrowDown, Enter runs the command");

        context_menus(&mut running, window);
        if csd {
            title_bar(&mut running, window);
        }
    }

    /// A context menu returns the chosen command and queues it; Escape
    /// and a click outside return nothing.
    fn context_menus(running: &mut AppContext, window: WindowId) {
        let command =
            |id, title: &str| MenuEntry::command(MenuCommandId(id), MenuItemState::new(title));
        let menu = ContextMenu::new(MenuSpec::new(
            "Edit",
            vec![command(1, "Alpha"), command(2, "Beta")],
        ))
        .unwrap();
        let at = Point::new(50.0, 80.0);

        // The keyboard chooses the second row.
        press_in_window(running, at);
        later(&[
            "key 108 1",
            "key 108 0",
            "key 108 1",
            "key 108 0",
            "key 28 1",
            "key 28 0",
        ]);
        let chosen = running.show_context_menu(window, &menu, at);
        assert_eq!(chosen, Some(MenuCommandId(2)));
        assert_eq!(menu.take_activations(), vec![MenuCommandId(2)]);

        press_in_window(running, at);
        later(&["key 1 1", "key 1 0"]);
        assert_eq!(running.show_context_menu(window, &menu, at), None);

        press_in_window(running, at);
        later(&["move 1000 700", "button 272 1", "button 272 0"]);
        assert_eq!(running.show_context_menu(window, &menu, at), None);
        assert!(menu.take_activations().is_empty());
    }

    /// The drawn title bar moves the window and closes it.
    fn title_bar(running: &mut AppContext, window: WindowId) {
        let before = sway_x("Chrome");
        let title = bounds(running, window, "Chrome");
        let start = screen(Point::new(title.origin.x + 40.0, title.center().y));
        virtual_input(&format!("move {} {}", start.x, start.y));
        pump(running, Duration::from_millis(100));
        virtual_input(&format!("button {LEFT_BUTTON} 1"));
        pump(running, Duration::from_millis(100));
        for step in 1..=6 {
            let x = start.x + f64::from(step) * 10.0;
            virtual_input(&format!("move {x} {}", start.y));
            pump(running, Duration::from_millis(60));
        }
        virtual_input(&format!("button {LEFT_BUTTON} 0"));
        pump(running, Duration::from_millis(300));
        let after = sway_x("Chrome");
        assert!(
            after > before,
            "the drag moves the window: {before} -> {after}"
        );
        place("Chrome", running);

        let close = bounds(running, window, "Close");
        let deadline = Instant::now() + Duration::from_secs(5);
        let center = screen(close.center());
        virtual_input(&format!("move {} {}", center.x, center.y));
        virtual_input(&format!("button {LEFT_BUTTON} 1"));
        virtual_input(&format!("button {LEFT_BUTTON} 0"));
        while running.pump() {
            assert!(Instant::now() < deadline, "Close closes the window");
        }
    }

    /// Presses and releases the right button at `at` in the window: the
    /// popup's grab takes the serial of the press.
    fn press_in_window(running: &mut AppContext, at: Point) {
        let point = screen(at);
        virtual_input(&format!("move {} {}", point.x, point.y));
        virtual_input(&format!("button {RIGHT_BUTTON} 1"));
        virtual_input(&format!("button {RIGHT_BUTTON} 0"));
        pump(running, Duration::from_millis(200));
    }

    /// Sends `commands` to the virtual input after the popup opens.
    fn later(commands: &'static [&'static str]) {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(700));
            for command in commands {
                virtual_input(command);
                std::thread::sleep(Duration::from_millis(50));
            }
        });
    }

    fn screen(point: Point) -> Point {
        Point::new((ORIGIN.x + point.x).round(), (ORIGIN.y + point.y).round())
    }

    /// The bounds of the semantic node named `name`, in window points.
    fn bounds(running: &mut AppContext, window: WindowId, name: &str) -> Rect {
        let host = running.host(window).unwrap();
        let (snapshot, _) = host.semantics();
        snapshot
            .iter()
            .find(|entry| entry.node.name == name)
            .unwrap_or_else(|| panic!("no node named {name}"))
            .bounds
    }

    fn click(running: &mut AppContext, at: Point) {
        let point = screen(at);
        virtual_input(&format!("move {} {}", point.x, point.y));
        pump(running, Duration::from_millis(50));
        virtual_input(&format!("button {LEFT_BUTTON} 1"));
        virtual_input(&format!("button {LEFT_BUTTON} 0"));
        pump(running, Duration::from_millis(200));
    }

    /// Presses `keys` in order and releases them in reverse order.
    fn chord(running: &mut AppContext, keys: &[u32]) {
        for key in keys {
            virtual_input(&format!("key {key} 1"));
        }
        for key in keys.iter().rev() {
            virtual_input(&format!("key {key} 0"));
        }
        pump(running, Duration::from_millis(200));
    }

    fn pump(running: &mut AppContext, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            assert!(running.pump(), "the window stays open");
        }
    }

    fn pump_until(running: &mut AppContext, done: impl Fn(&mut AppContext) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(running) {
            assert!(Instant::now() < deadline, "timed out");
            running.pump();
        }
    }

    /// Moves the window titled `title` to `ORIGIN` in sway.
    fn place(title: &str, running: &mut AppContext) {
        let output = Command::new("swaymsg")
            .arg(format!(
                "[title=\"{title}\"] move position {} {}",
                ORIGIN.x, ORIGIN.y
            ))
            .output()
            .unwrap();
        assert!(output.status.success());
        pump(running, Duration::from_millis(300));
    }

    /// The x position of the window titled `title` in sway's tree.
    fn sway_x(title: &str) -> f64 {
        let script = format!(
            "import json,sys\n\
             def walk(node):\n\
             \x20   if node.get('name') == '{title}':\n\
             \x20       print(node['rect']['x']); sys.exit()\n\
             \x20   for child in node.get('nodes', []) + node.get('floating_nodes', []):\n\
             \x20       walk(child)\n\
             walk(json.load(sys.stdin))\n"
        );
        let tree = Command::new("swaymsg")
            .args(["-t", "get_tree"])
            .output()
            .unwrap();
        let mut python = Command::new("python3")
            .args(["-c", &script])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        python
            .stdin
            .take()
            .unwrap()
            .write_all(&tree.stdout)
            .unwrap();
        let output = python.wait_with_output().unwrap();
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .expect("sway knows the window")
    }

    /// Sends one command to the virtual input of the headless session.
    fn virtual_input(command: &str) {
        use std::io::Write;
        let path = std::env::var("CHOSCORUI_VIRTUAL_INPUT").expect("run in headless-session.sh");
        let mut fifo = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        writeln!(fifo, "{command}").unwrap();
    }
}
