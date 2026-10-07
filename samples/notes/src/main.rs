//! Notes: an external sample application for choscorui.
//!
//! It depends on the `choscorui` facade alone and forbids `unsafe`, so
//! every native call goes through the toolkit's safe API. It shows a
//! themed, localized, multi-window UI: native menus with shortcuts, a
//! toolbar of command buttons, a text field, runtime theme and language
//! switching (including right-to-left Hebrew), plural-aware status text,
//! system accessibility preferences, and VoiceOver semantics.
//!
//! Environment:
//! - `NOTES_LANG=<en|vi|he>` picks the starting language.
//! - `NOTES_AUTOPILOT=1` runs a scripted check, prints a report, and exits
//!   non-zero on failure.

#![forbid(unsafe_code)]

mod strings;
mod ui;

#[cfg(target_os = "macos")]
fn main() {
    let app =
        choscorui::platform::Application::new().expect("AppKit must start on the main thread");
    let mut notes = app::Notes::new(&app);
    if std::env::var_os("NOTES_AUTOPILOT").is_some() {
        let failures = autopilot::run(&app, &mut notes);
        std::process::exit(i32::from(failures > 0));
    }
    while notes.window_count() > 0 && !notes.quit_requested() {
        notes.pump(&app, std::time::Duration::from_millis(250));
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("the notes sample currently runs on macOS only");
}

#[cfg(target_os = "macos")]
mod app {
    use crate::strings;
    use crate::ui::{
        CLOSE_WINDOW, Context, FONT, Measure, NEW_WINDOW, NEXT_LANGUAGE, NoteView, QUIT,
        TOGGLE_THEME,
    };
    use choscorui::controls::{CommandId, CommandRegistry, CommandState};
    use choscorui::core::{AccessibilityPreferences, Locale, Localizer, Size};
    use choscorui::platform::{
        AccessibilityActionRequest, Application, MainMenu, MenuBarModel, MenuCommandId, MenuEntry,
        MenuItemState, MenuShortcut, MenuShortcutModifiers, MenuSpec, ShapedLine, Window,
        WindowEvent, WindowOptions, system_accessibility_preferences, system_prefers_dark,
    };
    use std::time::Duration;

    /// Core Text measurement through the safe shaping API.
    struct CoreText;

    impl Measure for CoreText {
        fn width(&self, text: &str, size: f64) -> f64 {
            ShapedLine::new(text, FONT, size).map_or(0.0, |line| line.metrics().advance)
        }

        fn caret_x(&self, text: &str, size: f64, index: usize) -> f64 {
            ShapedLine::new(text, FONT, size)
                .ok()
                .and_then(|line| line.caret_offsets(index))
                .map_or(0.0, |caret| caret.primary)
        }
    }

    pub struct NoteWindow {
        pub window: Window,
        pub view: NoteView,
    }

    pub struct Notes {
        menu: Option<MainMenu>,
        pub localizer: Localizer,
        pub commands: CommandRegistry,
        pub dark: bool,
        pub windows: Vec<NoteWindow>,
        /// Index of the window that last became key; menu commands such as
        /// Close Window act on it.
        active: usize,
        next_number: usize,
        preferences: Option<AccessibilityPreferences>,
        quit: bool,
    }

    impl Notes {
        pub fn new(app: &Application) -> Self {
            let (catalog, notes) = strings::load(&strings::resources_dir());
            for note in notes {
                eprintln!("notes: {note}");
            }
            let locale = std::env::var("NOTES_LANG")
                .ok()
                .and_then(|tag| Locale::parse(&tag).ok())
                .unwrap_or_else(|| Locale::parse(strings::REFERENCE).expect("valid tag"));
            let mut commands = CommandRegistry::new();
            for command in [NEW_WINDOW, TOGGLE_THEME, NEXT_LANGUAGE, CLOSE_WINDOW, QUIT] {
                commands
                    .insert(command, CommandState::default())
                    .expect("commands are distinct");
            }
            let dark = system_prefers_dark().unwrap_or(false);
            commands
                .set_checked(TOGGLE_THEME, dark)
                .expect("command is registered");
            let mut notes = Self {
                menu: None,
                localizer: Localizer::new(locale, catalog),
                commands,
                dark,
                windows: Vec::new(),
                active: 0,
                next_number: 1,
                preferences: system_accessibility_preferences().ok(),
                quit: false,
            };
            notes.install_menu(app);
            notes.open_window(app);
            app.activate();
            notes
        }

        fn context(&self) -> Context<'_> {
            Context {
                localizer: &self.localizer,
                commands: &self.commands,
                windows_open: self.windows.len(),
                measure: &CoreText,
            }
        }

        pub fn window_count(&self) -> usize {
            self.windows.len()
        }

        pub fn quit_requested(&self) -> bool {
            self.quit
        }

        pub fn menu(&self) -> Option<&MainMenu> {
            self.menu.as_ref()
        }

        /// Builds the menu bar in the current language. Installing a new
        /// menu replaces the previous one.
        fn install_menu(&mut self, app: &Application) {
            let context = self.context();
            let item = |command: CommandId, title: String, key: &str| {
                MenuEntry::command(
                    MenuCommandId(command.0),
                    MenuItemState {
                        title,
                        enabled: true,
                        checked: command == TOGGLE_THEME && self.dark,
                        shortcut: Some(MenuShortcut {
                            key: key.into(),
                            modifiers: MenuShortcutModifiers {
                                command: true,
                                ..MenuShortcutModifiers::default()
                            },
                        }),
                    },
                )
            };
            let language = context.text("language.name", &[]);
            let model = MenuBarModel::new(vec![
                // The first menu is the application menu on macOS.
                MenuSpec::new(
                    context.text("app.name", &[]),
                    vec![item(QUIT, context.text("menu.quit", &[]), "q")],
                ),
                MenuSpec::new(
                    context.text("menu.file", &[]),
                    vec![
                        item(NEW_WINDOW, context.text("button.new", &[]), "n"),
                        item(CLOSE_WINDOW, context.text("menu.close", &[]), "w"),
                    ],
                ),
                MenuSpec::new(
                    context.text("menu.view", &[]),
                    vec![
                        item(TOGGLE_THEME, context.text("button.theme", &[]), "t"),
                        item(
                            NEXT_LANGUAGE,
                            context.text("button.language", &[("language", &language)]),
                            "l",
                        ),
                    ],
                ),
            ])
            .expect("menu commands are distinct");
            match app.install_main_menu(model) {
                Ok(menu) => self.menu = Some(menu),
                Err(error) => eprintln!("notes: menu unavailable: {error}"),
            }
        }

        pub fn open_window(&mut self, app: &Application) {
            let number = self.next_number;
            self.next_number += 1;
            let mut view =
                NoteView::new(number, Size::new(720.0, 420.0), self.dark, &self.context());
            if let Some(preferences) = self.preferences {
                view.set_accessibility_preferences(preferences);
            }
            let title = view.title(&self.context());
            let mut window = match app.create_window(WindowOptions::new(title, 720.0, 420.0)) {
                Ok(window) => window,
                Err(error) => {
                    eprintln!("notes: could not open a window: {error}");
                    return;
                }
            };
            view.layout(window.content_size());
            window.show();
            self.windows.push(NoteWindow { window, view });
            self.active = self.windows.len() - 1;
            // The window count appears in every status line.
            self.refresh_all();
        }

        pub fn close_window(&mut self, index: usize) {
            if index < self.windows.len() {
                let mut closed = self.windows.remove(index);
                closed.window.close();
                self.active = self.active.min(self.windows.len().saturating_sub(1));
                self.refresh_all();
            }
        }

        pub fn run_command(&mut self, app: &Application, command: CommandId) {
            match command {
                NEW_WINDOW => self.open_window(app),
                CLOSE_WINDOW => self.close_window(self.active),
                QUIT => self.quit = true,
                TOGGLE_THEME => {
                    self.dark = !self.dark;
                    let _ = self.commands.set_checked(TOGGLE_THEME, self.dark);
                    for entry in &mut self.windows {
                        entry.view.set_dark(self.dark);
                    }
                    if let Some(menu) = &mut self.menu
                        && let Ok(state) = menu.model().state(MenuCommandId(TOGGLE_THEME.0))
                    {
                        let state = MenuItemState {
                            checked: self.dark,
                            ..state.clone()
                        };
                        let _ = menu.set_state(MenuCommandId(TOGGLE_THEME.0), state);
                    }
                    self.refresh_all();
                }
                NEXT_LANGUAGE => {
                    self.set_language(app, strings::next_language(self.localizer.locale()))
                }
                _ => {}
            }
        }

        pub fn set_language(&mut self, app: &Application, locale: Locale) {
            self.localizer.set_locale(locale);
            let mut windows = std::mem::take(&mut self.windows);
            for entry in &mut windows {
                entry.view.relabel(&self.context());
                let _ = entry.window.set_title(&entry.view.title(&self.context()));
            }
            self.windows = windows;
            self.install_menu(app);
            self.refresh_all();
        }

        pub fn refresh_all(&mut self) {
            for index in 0..self.windows.len() {
                self.refresh(index);
            }
        }

        fn refresh(&mut self, index: usize) {
            let mut entry = self.windows.swap_remove(index);
            let context = self.context();
            // `swap_remove` took one window out; the status counts it too.
            let context = Context {
                windows_open: context.windows_open + 1,
                ..context
            };
            entry.window.set_display_list(entry.view.paint(&context));
            let (snapshot, changes) = entry.view.sync_semantics(&context);
            if let Err(error) = entry.window.set_accessibility(&snapshot, &changes) {
                eprintln!("notes: accessibility update failed: {error}");
            }
            self.windows.push(entry);
            let last = self.windows.len() - 1;
            self.windows.swap(index, last);
        }

        /// Waits up to `timeout` for one native event, then handles window
        /// events, menu commands, and accessibility actions.
        pub fn pump(&mut self, app: &Application, timeout: Duration) {
            app.dispatch_next_event(timeout);
            let mut commands = Vec::new();
            if let Some(menu) = &self.menu {
                commands.extend(
                    menu.take_activations()
                        .into_iter()
                        .map(|command| CommandId(command.0)),
                );
            }
            for command in commands {
                self.run_command(app, command);
            }
            let mut index = 0;
            while index < self.windows.len() {
                let events = self.windows[index].window.take_events();
                let requests = self.windows[index].window.take_accessibility_actions();
                let before = self.windows.len();
                self.deliver(app, index, events, requests);
                // Skip ahead only if this window is still open.
                if self.windows.len() >= before {
                    index += 1;
                }
            }
        }

        /// Routes one window's events and accessibility requests, then runs
        /// any commands they activated.
        pub fn deliver(
            &mut self,
            app: &Application,
            index: usize,
            events: Vec<WindowEvent>,
            requests: Vec<AccessibilityActionRequest>,
        ) {
            let mut windows = std::mem::take(&mut self.windows);
            let mut commands = Vec::new();
            let mut repaint = false;
            let mut closing = false;
            let mut focused = false;
            let context = Context {
                windows_open: windows.len(),
                ..self.context()
            };
            let view = &mut windows[index].view;
            for event in events {
                match event {
                    WindowEvent::Closing => closing = true,
                    WindowEvent::Focused(true) => focused = true,
                    WindowEvent::Focused(false) => {
                        view.cancel_interaction(&context);
                        repaint = true;
                    }
                    WindowEvent::Resized { size, .. } => {
                        view.layout(size);
                        repaint = true;
                    }
                    WindowEvent::Pointer { event, .. } => {
                        commands.extend(view.pointer(event, &context));
                        repaint = true;
                    }
                    // Command-key combinations belong to the menu bar.
                    WindowEvent::Key { event, modifiers } if !modifiers.command => {
                        commands.extend(view.key(
                            event,
                            modifiers.shift,
                            modifiers.option,
                            &context,
                        ));
                        repaint = true;
                    }
                    WindowEvent::Text { text } => repaint |= view.text_input(&text),
                    _ => {}
                }
            }
            for request in requests {
                commands.extend(view.accessibility_action(
                    request.node,
                    request.action,
                    request.value,
                    &context,
                ));
                repaint = true;
            }
            self.windows = windows;
            if focused {
                self.active = index;
            }
            if closing {
                self.close_window(index);
            } else if repaint {
                self.refresh(index);
            }
            for command in commands {
                self.run_command(app, command);
            }
        }
    }
}

/// Scripted end-to-end check of the gate: several themed windows, menu and
/// toolbar commands, typing, theme and language switches, and teardown.
#[cfg(target_os = "macos")]
mod autopilot {
    use crate::app::Notes;
    use crate::ui::{self, CLOSE_WINDOW, NEW_WINDOW, NEXT_LANGUAGE, TOGGLE_THEME};
    use choscorui::core::{Key, KeyEvent, LayoutDirection, PointerButton, PointerEvent};
    use choscorui::platform::{Application, MenuCommandId, Modifiers, WindowEvent};
    use std::time::{Duration, Instant};

    fn settle(app: &Application, notes: &mut Notes) {
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            notes.pump(app, Duration::from_millis(10));
        }
    }

    fn painted(app: &Application, notes: &mut Notes) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            notes.pump(app, Duration::from_millis(10));
            if notes
                .windows
                .iter()
                .all(|entry| entry.window.paint_count() > 0)
            {
                return true;
            }
        }
        false
    }

    pub fn run(app: &Application, notes: &mut Notes) -> usize {
        let mut failures = 0;
        let mut check = |name: &str, ok: bool| {
            println!("{} {name}", if ok { "ok  " } else { "FAIL" });
            failures += usize::from(!ok);
        };
        check(
            "capabilities report multiple windows",
            choscorui::capabilities().multiple_windows == choscorui::Support::Supported,
        );
        check("first window painted", painted(app, notes));

        // A menu shortcut's action, dispatched through AppKit.
        let via_menu = notes.menu().is_some_and(|menu| {
            menu.perform_action(MenuCommandId(NEW_WINDOW.0))
                .unwrap_or(false)
        });
        settle(app, notes);
        check(
            "File > New Window opens a second window",
            via_menu && notes.window_count() == 2,
        );

        // A toolbar click in the first window, routed like a native event.
        let center = notes.windows[0]
            .view
            .button_bounds(NEW_WINDOW)
            .map(|bounds| bounds.center())
            .unwrap_or_default();
        let click = [
            PointerEvent::Down {
                position: center,
                button: PointerButton::Primary,
            },
            PointerEvent::Up {
                position: center,
                button: PointerButton::Primary,
            },
        ]
        .map(|event| WindowEvent::Pointer {
            event,
            modifiers: Modifiers::default(),
        });
        notes.deliver(app, 0, click.to_vec(), Vec::new());
        check(
            "toolbar button opens a third window",
            notes.window_count() == 3 && app.window_count() == 3,
        );
        check("every window painted", painted(app, notes));

        // Typing into the second note: Tab cycles focus through the three
        // buttons back to the field, then text arrives as text events.
        let tab = WindowEvent::Key {
            event: KeyEvent::Down {
                key: Key::Tab,
                repeat: false,
            },
            modifiers: Modifiers::default(),
        };
        let mut events = vec![tab; 4];
        events.push(WindowEvent::Text {
            text: "Xin chào".into(),
        });
        notes.deliver(app, 1, events, Vec::new());
        check(
            "keyboard focus and typing edit the note",
            notes.windows[1].view.focus() == Some(notes.windows[1].view.field_node())
                && notes.windows[1].view.text() == "Xin chào",
        );

        // Theme switch through the menu applies to every window.
        let before = notes.dark;
        let toggled = notes.menu().is_some_and(|menu| {
            menu.perform_action(MenuCommandId(TOGGLE_THEME.0))
                .unwrap_or(false)
        });
        settle(app, notes);
        let expected = ui::theme(!before);
        check(
            "View > Switch Theme restyles every window",
            toggled
                && notes.dark != before
                && notes
                    .windows
                    .iter()
                    .all(|entry| entry.view.tokens() == expected),
        );
        check(
            "theme menu item is checked when dark",
            notes.menu().is_some_and(|menu| {
                menu.model()
                    .state(MenuCommandId(TOGGLE_THEME.0))
                    .is_ok_and(|state| state.checked == notes.dark)
            }),
        );

        // Language switches: Vietnamese, then right-to-left Hebrew.
        notes.run_command(app, NEXT_LANGUAGE);
        settle(app, notes);
        check(
            "Vietnamese titles",
            notes.windows[1].window.title() == "Ghi chú 2",
        );
        notes.run_command(app, NEXT_LANGUAGE);
        settle(app, notes);
        let mirrored = notes.windows.iter().all(|entry| {
            let first = entry.view.button_bounds(NEW_WINDOW).unwrap_or_default();
            let last = entry.view.button_bounds(NEXT_LANGUAGE).unwrap_or_default();
            first.origin.x > last.origin.x
        });
        check(
            "Hebrew mirrors every toolbar",
            notes.localizer.locale().direction() == LayoutDirection::Rtl && mirrored,
        );
        check(
            "Hebrew window titles and menu",
            notes.windows[0].window.title() == "פתק 1"
                && notes
                    .menu()
                    .is_some_and(|menu| menu.model().menus()[1].title == "קובץ"),
        );
        check(
            "the note survives language switches",
            notes.windows[1].view.text() == "Xin chào",
        );

        // Teardown: close every window through the command.
        while notes.window_count() > 0 {
            notes.run_command(app, CLOSE_WINDOW);
            settle(app, notes);
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.window_count() > 0 && Instant::now() < deadline {
            app.dispatch_next_event(Duration::from_millis(20));
        }
        let left = app.window_count();
        if left > 0 {
            println!("     {left} native windows still tracked");
        }
        check("closing every window releases it", left == 0);
        println!(
            "{}",
            if failures == 0 {
                "OK (0 failures)".to_owned()
            } else {
                format!("FAILED ({failures} failures)")
            }
        );
        failures
    }
}
