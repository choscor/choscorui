//! Opens view-based windows through `App` and drives them: a counter,
//! the command menus, and window restore. It opens AppKit windows, so it
//! runs only with `CHOSCORUI_RUN_MACOS_SMOKE=1`.

#[cfg(target_os = "macos")]
fn main() {
    if std::env::var_os("CHOSCORUI_RUN_MACOS_SMOKE").is_none() {
        return;
    }
    let settings = std::env::temp_dir().join(format!(
        "choscorui-app-smoke-{}/settings.txt",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&settings);
    smoke::counter(&settings);
    smoke::commands(&settings);
    smoke::restore(&settings);
    smoke::recovery_and_services(&settings);
    let _ = std::fs::remove_dir_all(settings.parent().unwrap());
    println!("app smoke: ok");
}

#[cfg(target_os = "macos")]
mod smoke {
    use choscorui::compose::view::StorageError;
    use choscorui::core::{Point, Rect, SemanticAction, Service, Size, WindowGeometry};
    use std::cell::RefCell;
    use std::rc::Rc;
    use choscorui::platform::{MenuShortcutModifiers, NativeAccess};
    use choscorui::prelude::*;
    use std::path::Path;

    struct Counter;

    impl View for Counter {
        fn body(&self, cx: &mut Cx) -> impl View {
            let count = cx.state(0);
            VStack::new((
                Text::new(move || format!("count {}", count.get())),
                Button::new("increment", move || count.update(|n| *n += 1)),
            ))
        }
    }

    /// A window counts clicks, and its title follows state.
    pub fn counter(settings: &Path) {
        let app = App::new().settings_path(settings);
        let clicks = app.runtime().property(0_u32);
        let title = clicks.clone();
        let mut running = app
            .scene(
                Scene::new(WindowOptions::new("counter", 320.0, 240.0), || Counter)
                    .title(move || format!("clicks {}", title.get())),
            )
            .start()
            .expect("the app must start on the main thread");
        let window = running.windows()[0];
        assert!(running.pump());
        let host = running.host(window).unwrap();
        let button = host
            .find("increment")
            .expect("the button is in the semantic tree");
        assert!(host.perform(button, SemanticAction::Activate, None));
        assert!(host.find("count 1").is_some());
        clicks.set(3);
        assert!(running.pump());
        assert_eq!(running.title(window).as_deref(), Some("clicks 3"));
        running.close(window);
        assert!(!running.pump());
    }

    /// A declared command menu installs; choosing an item runs its
    /// closure; live labels and check marks update the menu; a shortcut
    /// runs its item; a panicking action is counted and stays usable.
    pub fn commands(settings: &Path) {
        let app = App::new().settings_path(settings);
        let runtime = app.runtime().clone();
        let count = runtime.property(0_u32);
        let checked = runtime.property(false);
        let command = ShortcutModifiers {
            command: true,
            ..ShortcutModifiers::default()
        };
        let (counted, label, check) = (count.clone(), count.clone(), checked.clone());
        let menu = CommandMenu::new("Smoke", move |_: &Env| {
            let (counted, label, get, set) =
                (counted.clone(), label.clone(), check.clone(), check.clone());
            vec![
                MenuChoice::action(move || format!("Count {}", label.get()), move || {
                    counted.set(counted.get() + 1);
                })
                .keyboard_shortcut('k', command),
                MenuChoice::separator(),
                MenuChoice::checkbox(
                    "Checked",
                    Binding::new(move || get.get(), move |on| set.set(on)),
                ),
                MenuChoice::submenu(
                    "More",
                    vec![MenuChoice::action("Panic", || panic!("menu action panic"))],
                ),
            ]
        });
        let mut running = app
            .commands([menu])
            .window(WindowOptions::new("commands", 320.0, 240.0), || {
                Text::new("commands")
            })
            .start()
            .expect("the app must start on the main thread");
        assert!(running.pump());
        let model = running.menu_bar().expect("the command menu installs");
        assert_eq!(model.menus()[0].title, "Smoke");
        let item = model.find("Count 0").expect("the live label shows");

        assert!(running.choose(item), "the item is enabled");
        assert_eq!(count.get(), 1, "choosing the item runs its closure");
        assert!(running.pump());
        let model = running.menu_bar().unwrap();
        assert!(model.find("Count 1").is_some(), "the label follows state");

        let checkbox = model.find("Checked").unwrap();
        assert!(!model.state(checkbox).unwrap().checked);
        checked.set(true);
        assert!(running.pump());
        let model = running.menu_bar().unwrap();
        let checkbox = model.find("Checked").unwrap();
        assert!(model.state(checkbox).unwrap().checked, "the check mark follows state");
        assert!(running.choose(checkbox));
        assert!(!checked.get(), "choosing a checkbox row flips its binding");

        let pressed = running
            .native_menu()
            .unwrap()
            .perform_key_equivalent(
                "k",
                MenuShortcutModifiers {
                    command: true,
                    ..MenuShortcutModifiers::default()
                },
            )
            .unwrap();
        assert!(pressed, "the menu bar takes the shortcut");
        assert!(running.pump());
        assert_eq!(count.get(), 2, "the shortcut runs the action");

        let panicking = running.menu_bar().unwrap().find("Panic").unwrap();
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        assert!(running.choose(panicking));
        assert!(running.choose(panicking));
        std::panic::set_hook(hook);
        assert_eq!(running.menu_panic_count(), 2, "the item stays usable");
        assert_eq!(running.native_menu().unwrap().callback_panic_count(), 0);

        for id in running.windows() {
            running.close(id);
        }
        assert!(!running.pump());
    }

    fn restored_frame(settings: &Path, saved: Option<Rect>) -> (Rect, Rect) {
        let mut running = App::new()
            .settings_path(settings)
            .scene(
                Scene::new(
                    WindowOptions::new("restore", 400.0, 300.0)
                        .minimum_size(Size::new(200.0, 150.0)),
                    || Text::new("restore"),
                )
                .restore_geometry("window.smoke"),
            )
            .start()
            .expect("the app must start on the main thread");
        let window = running.windows()[0];
        assert!(running.pump());
        let opened = running.native_window(window).unwrap().geometry().frame;
        if let Some(frame) = saved {
            let geometry = WindowGeometry {
                frame,
                zoomed: false,
                full_screen: false,
            };
            running
                .native_window(window)
                .unwrap()
                .set_geometry(&geometry)
                .unwrap();
        }
        let closed = running.native_window(window).unwrap().geometry().frame;
        running.close(window);
        assert!(!running.pump());
        (opened, closed)
    }

    fn near(a: Rect, b: Rect) -> bool {
        (a.origin.x - b.origin.x).abs() < 1.0
            && (a.origin.y - b.origin.y).abs() < 1.0
            && (a.size.width - b.size.width).abs() < 1.0
            && (a.size.height - b.size.height).abs() < 1.0
    }

    /// A corrupt settings file is moved aside and reported through
    /// `on_error` without a window; the services reach AppKit.
    pub fn recovery_and_services(settings: &Path) {
        std::fs::write(settings, "not a settings file\n").unwrap();
        let errors: Rc<RefCell<Vec<(Option<WindowId>, ViewError)>>> = Rc::default();
        let seen = errors.clone();
        let mut running = App::new()
            .settings_path(settings)
            .on_error(move |window, error| seen.borrow_mut().push((window, error.clone())))
            .window(WindowOptions::new("services", 320.0, 240.0), || {
                Text::new("services")
            })
            .start()
            .expect("the app must start on the main thread");
        assert!(running.pump());
        assert!(
            matches!(
                &errors.borrow()[..],
                [(None, ViewError::Storage(StorageError::Recovered { .. }))]
            ),
            "{:?}",
            errors.borrow()
        );
        let services = running.services().clone();
        let posted = services.post_notification(&SystemNotification::new(
            "com.choscor.smoke",
            "Smoke",
            "app smoke",
        ));
        if !choscorui::capabilities().notifications.is_supported() {
            assert!(
                matches!(
                    posted,
                    Err(ServiceError::Unavailable {
                        service: Service::Notifications,
                        ..
                    })
                ),
                "{posted:?}"
            );
        }
        if std::env::var_os("CHOSCORUI_RUN_MACOS_CLIPBOARD").is_some() {
            services.set_clipboard_text("choscorui app smoke").unwrap();
            assert_eq!(
                services.clipboard_text().unwrap().as_deref(),
                Some("choscorui app smoke")
            );
        }
        assert!(services.url_handler("not a url").is_err());
        for id in running.windows() {
            running.close(id);
        }
        assert!(!running.pump());
    }

    /// A window with `restore_geometry` reopens at its saved frame, and a
    /// frame off every screen moves onto one.
    pub fn restore(settings: &Path) {
        let screens = choscorui::platform::screen_visible_frames().unwrap();
        let main = screens[0];
        let placed = Rect::new(
            Point::new(main.origin.x + 40.0, main.origin.y + 60.0),
            Size::new(420.0, 310.0),
        );
        let (_, closed) = restored_frame(settings, Some(placed));
        let (reopened, _) = restored_frame(settings, None);
        assert!(near(reopened, closed), "{reopened:?} != {closed:?}");

        let away = Rect::new(Point::new(-100_000.0, -100_000.0), Size::new(420.0, 310.0));
        let (_, _) = restored_frame(settings, Some(away));
        let (fitted, _) = restored_frame(settings, None);
        assert!(
            screens
                .iter()
                .any(|screen| screen.intersection(fitted).is_some()),
            "{fitted:?} is on no screen of {screens:?}"
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {}
