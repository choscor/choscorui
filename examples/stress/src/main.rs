//! Stress gallery: large searchable data, a large canvas, dockable
//! panels, structured input, settings/dock restore across restarts, and
//! desktop integration.
//!
//! Environment:
//! - `CHOSCORUI_STRESS_SETTINGS=<path>` overrides the per-user settings file.
//! - `CHOSCORUI_STRESS_AUTOPILOT=1` runs the scripted gate checks, prints a
//!   report, and exits non-zero on failure.
//! - `CHOSCORUI_STRESS_AUTOCLOSE_MS=<n>` closes the interactive window.

#[cfg(target_os = "macos")]
use choscorui_stress::workspace;

#[cfg(target_os = "macos")]
fn main() {
    use choscorui_core::{PointerEvent, Settings, Size, WheelUnit, user_settings_path};
    use choscorui_macos::{Application, WindowEvent, WindowOptions};
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::time::Duration;
    use workspace::Workspace;

    let settings_path = std::env::var_os("CHOSCORUI_STRESS_SETTINGS")
        .map(PathBuf::from)
        .or_else(|| user_settings_path("com.choscor.stress"))
        .expect("a settings location is required");
    let app = Application::new().expect("AppKit must start on the main thread");

    if std::env::var_os("CHOSCORUI_STRESS_AUTOPILOT").is_some() {
        let failures = autopilot::run(&app, &settings_path);
        std::process::exit(i32::from(failures > 0));
    }

    let (mut settings, report) = Settings::load_or_recover(workspace::schema(), &settings_path)
        .unwrap_or_else(|error| {
            eprintln!("settings unavailable ({error}); using defaults without saving");
            (Settings::new(workspace::schema()), Default::default())
        });
    if let Some(aside) = report.recovered_to {
        eprintln!("corrupt settings moved to {}", aside.display());
    }
    let mut window = app
        .create_window(WindowOptions::new(
            "choscorui stress gallery",
            1100.0,
            760.0,
        ))
        .expect("window must be created");
    if let Some(geometry) = choscorui_core::WindowGeometry::restore(&settings, "window.main")
        && let Ok(screens) = choscorui_macos::screen_visible_frames()
    {
        let fitted = choscorui_core::WindowGeometry {
            frame: geometry.fit_to_screens(&screens, Size::new(640.0, 480.0)),
            ..geometry
        };
        let _ = window.set_geometry(&fitted);
    }
    let mut workspace = Workspace::new(window.content_size());
    for note in workspace.restore(&settings) {
        eprintln!("{note}");
    }
    window.set_display_list(workspace.paint());
    window.show();
    app.activate();

    let close = Rc::new(Cell::new(false));
    let _timer = std::env::var("CHOSCORUI_STRESS_AUTOCLOSE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|ms| {
            let flag = Rc::clone(&close);
            app.schedule_once(Duration::from_millis(ms), move || flag.set(true))
                .expect("timer must schedule")
        });
    let mut dragging = false;
    while !close.get() {
        app.dispatch_next_event(Duration::from_millis(250));
        let mut repaint = false;
        for event in window.take_events() {
            match event {
                WindowEvent::Closing => close.set(true),
                WindowEvent::Resized { size, .. } => {
                    workspace.layout(size);
                    repaint = true;
                }
                WindowEvent::Pointer { event, modifiers } => match event {
                    PointerEvent::Down { position, .. } => {
                        dragging = true;
                        repaint |= workspace.press(position, modifiers.command || modifiers.shift);
                    }
                    PointerEvent::Move { position } if dragging => {
                        repaint |= workspace.drag(position)
                    }
                    PointerEvent::Up { position, .. } => {
                        dragging = false;
                        repaint |= workspace.release(position);
                    }
                    PointerEvent::Wheel {
                        position,
                        delta,
                        unit,
                    } => {
                        let scale = if unit == WheelUnit::Lines { 22.0 } else { 1.0 };
                        repaint |= workspace.wheel(
                            position,
                            delta.x * scale,
                            delta.y * scale,
                            modifiers.command,
                        );
                    }
                    _ => {}
                },
                WindowEvent::Key {
                    event: choscorui_core::KeyEvent::Down { key, .. },
                    modifiers,
                } => repaint |= workspace.key(key, modifiers.shift, modifiers.command),
                WindowEvent::Text { text } => {
                    workspace.text(&text);
                    repaint = true;
                }
                _ => {}
            }
        }
        for request in window.take_accessibility_actions() {
            let _ = request;
            repaint = true;
        }
        if repaint {
            window.set_display_list(workspace.paint());
            let changes = workspace.sync_semantics();
            let _ =
                window.set_accessibility(&workspace.semantics.snapshot(&workspace.tree), &changes);
        }
    }
    workspace.store(&mut settings);
    let _ = window.geometry().store(&mut settings, "window.main");
    if let Err(error) = settings.save() {
        eprintln!("could not save settings: {error}");
    }
}

#[cfg(target_os = "macos")]
mod autopilot {
    use crate::workspace::{self, CANVAS_ITEMS, INSPECTOR_PANEL, LOG_PANEL, RECORDS, Workspace};
    use choscorui_controls::{DockArea, DockTarget, ValidationState};
    use choscorui_core::{Key, Point, Rect, SettingValue, Settings, Size, WindowGeometry};
    use choscorui_macos::{
        Application, DesktopError, StatusItem, WindowOptions, has_bundle_identifier,
        post_notification, screen_visible_frames, url_handler,
    };
    use std::path::Path;
    use std::time::Instant;

    struct Report {
        failures: usize,
    }

    impl Report {
        fn check(&mut self, name: &str, ok: bool, detail: impl std::fmt::Display) {
            println!("{} {name}: {detail}", if ok { "PASS" } else { "FAIL" });
            if !ok {
                self.failures += 1;
            }
        }
    }

    pub fn run(app: &Application, settings_path: &Path) -> usize {
        let mut report = Report { failures: 0 };
        let _ = std::fs::remove_file(settings_path);
        // A version-1 file exercises migration on the first launch.
        if let Some(parent) = settings_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(settings_path, "choscorui-settings 1\nsearch = text:hà\n").unwrap();

        let size = Size::new(1100.0, 760.0);
        let mut window = app
            .create_window(WindowOptions::new(
                "choscorui stress autopilot",
                size.width,
                size.height,
            ))
            .expect("window");
        let (mut settings, load) = Settings::load(workspace::schema(), settings_path).unwrap();
        report.check(
            "settings migration",
            load.migrated_from == Some(1),
            format!("{load:?}"),
        );
        let start = Instant::now();
        let mut ws = Workspace::new(window.content_size());
        ws.restore(&settings);
        report.check(
            "startup",
            true,
            format!(
                "{RECORDS} records + {CANVAS_ITEMS} canvas items in {:?}",
                start.elapsed()
            ),
        );
        report.check("migrated search restored", ws.search == "hà", &ws.search);

        // Search: diacritic-insensitive filtering over 100,000 rows.
        let start = Instant::now();
        ws.set_search("ha noi");
        let matches = ws.visible_records();
        report.check(
            "search",
            matches == RECORDS / 5,
            format!("{matches} matches in {:?}", start.elapsed()),
        );
        ws.set_search("");

        // Scrolling: realized nodes stay bounded.
        let baseline_children = ws.tree.children(ws.list_node).unwrap().len();
        let mut peak = 0;
        let start = Instant::now();
        for _ in 0..2_000 {
            ws.key(Key::PageDown, false, false);
            peak = peak.max(ws.tree.children(ws.list_node).unwrap().len());
        }
        ws.key(Key::End, false, false);
        let rows_visible = (ws.list_rect().size.height / 22.0).ceil() as usize;
        report.check(
            "virtualized scrolling",
            peak <= rows_visible + 8 && peak <= baseline_children + 8,
            format!(
                "peak realized {peak}, viewport rows {rows_visible}, {:?}",
                start.elapsed()
            ),
        );
        let list = ws.paint();
        report.check(
            "paint bounded",
            list.len() < 2_000,
            format!("{} display commands", list.len()),
        );
        let changes = ws.sync_semantics();
        let snapshot = ws.semantics.snapshot(&ws.tree);
        report.check(
            "list accessibility",
            snapshot.len() < 200,
            format!(
                "{} semantic entries, {} changes",
                snapshot.len(),
                changes.len()
            ),
        );
        let _ = window.set_accessibility(&snapshot, &changes);

        // Canvas: zoom about a point, select in transformed coordinates.
        let canvas = ws.canvas_rect();
        let center = canvas.center();
        // Frame scene (0,0)–(120,120): a zoomed, translated view.
        ws.canvas.fit(
            Rect::new(Point::new(0.0, 0.0), Size::new(120.0, 120.0)),
            0.0,
        );
        let target = ws.canvas.scene_to_view(Point::new(54.0, 54.0));
        ws.press(target, false);
        ws.release(target);
        let selected = ws.canvas.selection().len();
        report.check(
            "canvas hit test when zoomed",
            selected == 1,
            format!("{selected} selected at {:.1}x", ws.canvas.zoom()),
        );
        let start = Instant::now();
        let mut list = choscorui_core::DisplayList::new();
        let painted = ws.canvas.paint(&mut ws.scene, &mut list).unwrap();
        report.check(
            "canvas culling",
            painted < 2_000,
            format!(
                "{painted} of {CANVAS_ITEMS} painted in {:?}",
                start.elapsed()
            ),
        );
        ws.canvas.fit(
            Rect::new(Point::new(0.0, 0.0), Size::new(800.0, 400.0)),
            0.0,
        );
        ws.press(
            Point::new(canvas.max_x() - 2.0, canvas.max_y() - 2.0),
            false,
        );
        ws.drag(Point::new(canvas.origin.x + 1.0, canvas.origin.y + 1.0));
        ws.release(Point::new(canvas.origin.x + 1.0, canvas.origin.y + 1.0));
        let band = ws.canvas.selection().len();
        report.check("rubber band", band > 10, format!("{band} selected"));
        let items = ws.canvas.item_semantics(&mut ws.scene);
        report.check(
            "canvas accessibility virtualized",
            !items.is_empty() && items.len() < 2_000,
            format!("{} items exposed", items.len()),
        );
        ws.canvas.zoom_at(center, 1.5);

        // Structured input validation.
        ws.amount.set_text("1,234.5x");
        let invalid = matches!(ws.amount.validate().0, ValidationState::Invalid(_));
        ws.amount.set_text("1,234.50");
        ws.quantity.set_text("20000");
        let out_of_range = matches!(ws.quantity.validate().0, ValidationState::Invalid(_));
        ws.quantity.set_text("42");
        report.check(
            "structured input",
            invalid && out_of_range && ws.amount.can_accept() && ws.quantity.can_accept(),
            "invalid and out-of-range rejected, valid accepted",
        );

        // Change dock, toolbar, search, and window geometry, then save.
        ws.dock
            .dock(
                INSPECTOR_PANEL,
                DockTarget::Floating(Rect::new(Point::new(700.0, 120.0), Size::new(280.0, 220.0))),
            )
            .unwrap();
        ws.dock.close(LOG_PANEL).unwrap();
        ws.dock.set_extent(DockArea::Left, 420.0);
        workspace::customize_toolbar(&mut ws.toolbar);
        ws.set_search("huế");
        ws.layout(ws.size);
        let screens = screen_visible_frames().unwrap_or_default();
        let main = screens
            .first()
            .copied()
            .unwrap_or(Rect::new(Point::new(0.0, 0.0), Size::new(1440.0, 900.0)));
        let geometry = WindowGeometry {
            frame: Rect::new(
                Point::new(main.origin.x + 60.0, main.origin.y + 40.0),
                Size::new(900.0, 640.0),
            ),
            zoomed: false,
            full_screen: false,
        };
        window.set_geometry(&geometry).unwrap();
        ws.store(&mut settings);
        window
            .geometry()
            .store(&mut settings, "window.main")
            .unwrap();
        settings.save().unwrap();
        let saved_dock = ws.dock.save();
        let saved_toolbar = ws.toolbar.save();
        let saved_zoom = ws.canvas.zoom();
        window.close();
        drop(window);
        drop(ws);

        // "Restart": everything is rebuilt from the file.
        let (settings, load) = Settings::load(workspace::schema(), settings_path).unwrap();
        report.check(
            "reload without migration",
            load.migrated_from.is_none(),
            format!("{load:?}"),
        );
        let mut window = app
            .create_window(WindowOptions::new(
                "choscorui stress autopilot (restarted)",
                800.0,
                600.0,
            ))
            .expect("window");
        let restored_geometry = WindowGeometry::restore(&settings, "window.main").unwrap();
        let fitted = WindowGeometry {
            frame: restored_geometry.fit_to_screens(&screens, Size::new(640.0, 480.0)),
            ..restored_geometry
        };
        window.set_geometry(&fitted).unwrap();
        let actual = window.geometry().frame;
        report.check(
            "window geometry restored",
            (actual.origin.x - geometry.frame.origin.x).abs() <= 1.0
                && (actual.origin.y - geometry.frame.origin.y).abs() <= 1.0
                && (actual.size.width - 900.0).abs() <= 1.0,
            format!("{actual:?}"),
        );
        let mut ws = Workspace::new(window.content_size());
        let notes = ws.restore(&settings);
        report.check("restore notes", notes.is_empty(), format!("{notes:?}"));
        report.check(
            "dock restored",
            ws.dock.save() == saved_dock,
            ws.dock.save().replace('\n', " | "),
        );
        report.check(
            "toolbar restored",
            ws.toolbar.save() == saved_toolbar,
            ws.toolbar.save(),
        );
        report.check(
            "search restored",
            ws.search == "huế" && ws.visible_records() == RECORDS / 5,
            format!("{:?} → {} rows", ws.search, ws.visible_records()),
        );
        report.check(
            "zoom restored",
            (ws.canvas.zoom() - saved_zoom).abs() < 1e-9,
            ws.canvas.zoom(),
        );
        report.check(
            "settings key present",
            settings
                .get("dock.main")
                .is_some_and(|value| matches!(value, SettingValue::Text(_))),
            "dock.main",
        );

        // Desktop integration supported on macOS.
        let status = StatusItem::new("Stress");
        report.check(
            "status item",
            status.as_ref().is_ok_and(|item| item.title() == "Stress"),
            "NSStatusItem created",
        );
        drop(status);
        report.check(
            "URL handler lookup",
            url_handler("https://example.com").is_ok(),
            format!("{:?}", url_handler("https://example.com")),
        );
        let notification = post_notification("stress", "Stress gallery", "Autopilot finished");
        report.check(
            "notifications",
            if has_bundle_identifier() {
                notification.is_ok() || notification == Err(DesktopError::PermissionDenied)
            } else {
                notification == Err(DesktopError::RequiresAppBundle)
            },
            format!("{notification:?}"),
        );
        window.close();
        let _ = std::fs::remove_file(settings_path);
        println!(
            "{} ({} failure{})",
            if report.failures == 0 { "OK" } else { "FAILED" },
            report.failures,
            if report.failures == 1 { "" } else { "s" }
        );
        report.failures
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The stress gallery currently runs on macOS only.");
}
