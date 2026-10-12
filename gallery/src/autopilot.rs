//! Scripted end-to-end checks of the gate evidence, run with
//! `CHOSCORUI_GALLERY_AUTOPILOT=1`: every page, themes, a second
//! language, windows, large data, the canvas, structured input, settings
//! and dock restore, desktop capabilities, translations, and window
//! release. One line per check, then `OK (0 failures)` or
//! `FAILED (<n> failures)`.

use crate::app::{self, Gallery};
use crate::catalog::PAGES;
use crate::page::{Env, Page};
use crate::pages::containers::DockPage;
use crate::pages::data::{CANVAS_ITEMS, CanvasPage, ItemViewPage, RECORDS};
use crate::pages::datetime::DateTimeEditPage;
use crate::pages::system::capability_rows;
use crate::pages::text::RulePage;
use crate::settings::{self, Saved};
use crate::strings::Source;
use crate::view::{GalleryView, Theme};
use choscorui::controls::{DateTimeField, ValidationState};
use choscorui::core::{
    BaseColor, CivilDate, CivilDateTime, CivilTime, Key, Point, PointerButton, PointerEvent,
    Support, Tree,
};
use choscorui::platform::{Application, Modifiers, WindowEvent};
use std::time::{Duration, Instant};

/// Upper bound on realized rows while scrolling 100k records; anything
/// near the record count means virtualization broke.
const REALIZED_ROW_LIMIT: usize = 200;

fn settle(app: &Application, gallery: &mut Gallery) {
    let deadline = Instant::now() + Duration::from_millis(200);
    while Instant::now() < deadline {
        gallery.pump(app, Duration::from_millis(10));
    }
}

/// Pumps until window `index` presents a frame after `before`.
fn painted_after(app: &Application, gallery: &mut Gallery, index: usize, before: usize) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        gallery.pump(app, Duration::from_millis(5));
        if gallery
            .windows
            .get(index)
            .is_some_and(|entry| entry.window.paint_count() > before)
        {
            return true;
        }
    }
    false
}

fn paint_count(gallery: &Gallery, index: usize) -> usize {
    gallery
        .windows
        .get(index)
        .map_or(0, |entry| entry.window.paint_count())
}

fn click(gallery: &mut Gallery, app: &Application, index: usize, at: Point) {
    let events = [
        PointerEvent::Down {
            position: at,
            button: PointerButton::Primary,
        },
        PointerEvent::Up {
            position: at,
            button: PointerButton::Primary,
        },
    ]
    .map(|event| WindowEvent::Pointer {
        event,
        modifiers: Modifiers::default(),
    });
    gallery.deliver(app, index, events.to_vec(), Vec::new());
}

fn page_index(id: &str) -> usize {
    PAGES
        .iter()
        .position(|page| page.id == id)
        .expect("autopilot pages exist")
}

/// Shows page `id` in window 0 (if it is not already shown) and runs `f`
/// on it.
fn with_page<P: Page + 'static, R>(
    gallery: &mut Gallery,
    id: &str,
    f: impl FnOnce(&mut Tree, &mut P, &Env<'_>) -> R,
) -> Option<R> {
    if gallery.page_of(0) != Some(id) {
        gallery.select(0, page_index(id));
    }
    gallery
        .with_page(0, |tree, page, env| {
            page.as_any_mut()
                .downcast_mut::<P>()
                .map(|page| f(tree, page, env))
        })
        .flatten()
}

/// Builds fresh views from `saved`, as a relaunch would, and checks what
/// they restore.
fn rebuild(gallery: &Gallery, saved: &Saved) -> bool {
    let ctx = gallery.ctx();
    let start = settings::start_page(None, saved.page.as_deref());
    let mut view = GalleryView::new(
        app::DEFAULT_SIZE,
        start,
        &saved.filter,
        saved.state.clone(),
        &ctx,
    );
    let zoom = view.with_page(&ctx, |_, page, _| {
        page.as_any_mut()
            .downcast_mut::<CanvasPage>()
            .map(|canvas| canvas.zoom())
    });
    let canvas_ok = view.selected_id() == "canvas"
        && view.filter_text() == saved.filter
        && zoom.is_some_and(|zoom| (zoom - saved.state.canvas_zoom).abs() < 1e-9);
    let mut docked = GalleryView::new(
        app::DEFAULT_SIZE,
        page_index("dock"),
        "",
        saved.state.clone(),
        &ctx,
    );
    let dock = docked.with_page(&ctx, |_, page, _| {
        page.as_any_mut()
            .downcast_mut::<DockPage>()
            .map(|dock| dock.save())
    });
    if !canvas_ok {
        println!(
            "     rebuilt canvas: page {} zoom {zoom:?}",
            view.selected_id()
        );
    }
    canvas_ok && dock.as_deref() == Some(saved.state.dock.as_str())
}

/// A version 1 file (page under `page`) loads through the migration.
fn migrates() -> bool {
    let dir = std::env::temp_dir().join(format!(
        "choscorui-gallery-autopilot-{}",
        std::process::id()
    ));
    let path = dir.join("settings.txt");
    let written = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&path, "choscorui-settings 1\npage = text:slider\n"));
    let (settings, issues) = settings::load(Some(path));
    let _ = std::fs::remove_dir_all(&dir);
    written.is_ok()
        && Saved::read(&settings).page.as_deref() == Some("slider")
        && issues
            .iter()
            .any(|issue| issue.contains("migrated from version 1"))
}

pub fn run(app: &Application, gallery: &mut Gallery) -> usize {
    let mut failures = 0;
    let mut check = |name: &str, ok: bool| {
        println!("{} {name}", if ok { "ok  " } else { "FAIL" });
        failures += usize::from(!ok);
    };
    for note in &gallery.notes {
        println!("     note: {note}");
    }

    // Translations.
    let vi_file = !gallery.notes.iter().any(|note| note.contains("vi.lang"));
    check(
        match gallery.source {
            Source::Resources => "translations load from the resources directory",
            Source::BuiltInEnglish => "translations fall back to built-in English",
        },
        match gallery.source {
            Source::Resources => vi_file,
            Source::BuiltInEnglish => true,
        },
    );

    check(
        "capabilities report multiple windows",
        choscorui::capabilities().multiple_windows == Support::Supported,
    );
    let before = paint_count(gallery, 0);
    check(
        "first window painted",
        painted_after(app, gallery, 0, before.saturating_sub(1)),
    );

    // Every page, selected through the sidebar model and presented.
    let mut unpainted = Vec::new();
    for (index, page) in PAGES.iter().enumerate() {
        // The page shown at launch has already been presented.
        let before = if gallery.page_of(0) == Some(page.id) {
            0
        } else {
            paint_count(gallery, 0)
        };
        gallery.select(0, index);
        if !painted_after(app, gallery, 0, before) || gallery.page_of(0) != Some(page.id) {
            unpainted.push(page.id);
        }
    }
    if !unpainted.is_empty() {
        println!("     not painted: {}", unpainted.join(", "));
    }
    check(
        &format!("all {} pages selected and painted", PAGES.len()),
        unpainted.is_empty(),
    );

    // Theme change through the state bar repaints.
    let before = paint_count(gallery, 0);
    let brand = gallery.windows[0].view.theme_button(Theme::Brand).center();
    click(gallery, app, 0, brand);
    check(
        "the Brand theme button restyles and repaints",
        gallery.theme == Theme::Brand && painted_after(app, gallery, 0, before),
    );
    check(
        "the View menu checks the active theme",
        gallery.menu().is_some_and(|menu| {
            menu.model()
                .state(app::theme_command(Theme::Brand))
                .is_ok_and(|state| state.checked)
        }),
    );

    // A base color swatch restyles and repaints.
    let before = paint_count(gallery, 0);
    let mauve = gallery.windows[0]
        .view
        .base_button(BaseColor::Mauve)
        .center();
    click(gallery, app, 0, mauve);
    check(
        "the Mauve base swatch restyles and repaints",
        gallery.base == BaseColor::Mauve && painted_after(app, gallery, 0, before),
    );

    // The Enabled toggle disables the page and enables it again.
    let toggle = gallery.windows[0].view.enabled_toggle().center();
    click(gallery, app, 0, toggle);
    let disabled = !gallery.windows[0].view.enabled();
    click(gallery, app, 0, toggle);
    check(
        "the Enabled toggle disables and re-enables the page",
        disabled && gallery.windows[0].view.enabled(),
    );

    // Structured inputs reject invalid values.
    let numbers = with_page::<RulePage, _>(gallery, "number-input", |_, page, _| {
        (
            page.enter(0, "12abc"),
            page.enter(1, "0"),
            page.enter(1, "12"),
        )
    });
    check(
        "number input rejects letters and out-of-range values",
        numbers.is_some_and(|(letters, low, ok)| {
            letters != ValidationState::Valid
                && low != ValidationState::Valid
                && ok == ValidationState::Valid
        }),
    );
    // The plate mask `>AA-99999` needs two letters first.
    let mask = with_page::<RulePage, _>(gallery, "input-mask", |_, page, _| {
        (
            page.enter(1, "12"),
            page.enter(1, "a"),
            page.enter(1, "ab12345"),
        )
    });
    check(
        "input mask rejects characters its format forbids",
        mask.is_some_and(|(digits, partial, full)| {
            matches!(digits, ValidationState::Invalid(_))
                && partial == ValidationState::Intermediate
                && full == ValidationState::Valid
        }),
    );
    let dates = with_page::<RulePage, _>(gallery, "date-input", |_, page, _| {
        (page.enter(0, "not a date"), page.enter(1, "1/1/2025"))
    });
    check(
        "date input rejects text and dates outside 2026",
        dates.is_some_and(|(text, outside)| {
            matches!(text, ValidationState::Invalid(_)) && outside != ValidationState::Valid
        }),
    );
    let limited = with_page::<DateTimeEditPage, _>(gallery, "date-time-edit", |_, page, _| {
        let edit = page.limited_mut();
        for _ in 0..edit.fields().len() {
            if edit.focused_field() == DateTimeField::Hour {
                break;
            }
            edit.handle_key(Key::ArrowRight);
        }
        // End and Home jump to 23 and 0, outside the range; Return clamps.
        edit.handle_key(Key::End);
        let flagged = edit.validation() != ValidationState::Valid;
        edit.handle_key(Key::Enter);
        let high = edit.value();
        edit.handle_key(Key::Home);
        edit.handle_key(Key::Enter);
        (flagged, high, edit.value())
    });
    let at = |hour| CivilDateTime {
        date: CivilDate::new(2026, 10, 12).expect("valid date"),
        time: CivilTime::new(hour, 0, 0).expect("valid time"),
    };
    check(
        "date/time edit clamps to its 09:00 – 17:00 range",
        limited.is_some_and(|(flagged, high, low)| flagged && high == at(17) && low == at(9)),
    );

    // 100k records: search, then page through with bounded realization.
    let large = with_page::<ItemViewPage, _>(gallery, "item-view", |tree, page, _| {
        let all = page.matches();
        page.search(tree, "7");
        let matches = page.matches();
        let mut peak = page.realized();
        for _ in 0..2_000 {
            page.page_down(tree);
            peak = peak.max(page.realized());
        }
        (all, matches, peak)
    });
    if let Some((all, matches, peak)) = large {
        println!("     {all} records, {matches} match \"7\", at most {peak} rows realized");
    }
    check(
        "100k-record search keeps realized rows bounded over 2,000 Page Downs",
        large.is_some_and(|(all, matches, peak)| {
            all == RECORDS && matches > 0 && matches < all && peak > 0 && peak <= REALIZED_ROW_LIMIT
        }),
    );

    // Canvas: zoomed hit test, rubber band, culling, bounded semantics.
    let canvas = with_page::<CanvasPage, _>(gallery, "canvas", |tree, page, _| {
        page.set_zoom(tree, 2.0);
        let hit = page.hit(page.item_center(201));
        let from = page.item_center(0);
        let to = page.item_center(201);
        // Items are 28 units square on a 40-unit grid; start and end the
        // band 18 units outside them so the press lands on empty space.
        let margin = 18.0 * page.zoom();
        let selected = page.rubber_band(
            tree,
            Point::new(from.x - margin, from.y - margin),
            Point::new(to.x + margin, to.y + margin),
        );
        (page.zoom(), hit, selected, page.realized())
    });
    let before = paint_count(gallery, 0);
    gallery.refresh(0);
    let presented = painted_after(app, gallery, 0, before);
    let painted = with_page::<CanvasPage, _>(gallery, "canvas", |_, page, _| page.painted());
    check(
        "canvas hit-tests at 2x zoom and rubber-band selects",
        canvas.as_ref().is_some_and(|(zoom, hit, selected, _)| {
            // The band spans items 0, 1, 200 and 201 (two columns, two rows)
            // and stops short of column 2, which starts at 80 units.
            (*zoom - 2.0).abs() < 1e-9 && hit.as_deref() == Some("Node 201") && *selected == 4
        }),
    );
    check(
        "canvas culls painting and keeps semantics bounded",
        presented
            && painted.is_some_and(|painted| painted > 0 && painted < CANVAS_ITEMS)
            && canvas.is_some_and(|(_, _, _, realized)| realized > 0 && realized <= 400),
    );

    // Dock: the arrangement round-trips through the page state.
    let dock = with_page::<DockPage, _>(gallery, "dock", |tree, page, _| {
        let moved = page.move_console(tree);
        (moved, page.save())
    });
    let saved_dock = gallery.windows[0].view.page_state().dock;
    check(
        "dock moves a panel and persists the arrangement",
        dock.is_some_and(|(moved, text)| moved && !text.is_empty() && text == saved_dock),
    );

    // Vietnamese relabels the window and rebuilds the menu bar.
    gallery.run_command(app, app::language_command("vi"));
    settle(app, gallery);
    let view = &gallery.windows[0].view;
    let file_title = gallery.ctx().text("menu.file", &[]);
    check(
        "Vietnamese rebuilds the menu bar and window title",
        file_title != "File"
            && gallery
                .menu()
                .is_some_and(|menu| menu.model().menus()[1].title == file_title)
            && gallery.windows[0].window.title() == view.window_title(&gallery.ctx()),
    );

    // A second window through File > New Window.
    let via_menu = gallery
        .menu()
        .is_some_and(|menu| menu.perform_action(app::NEW_WINDOW).unwrap_or(false));
    settle(app, gallery);
    let second_painted = painted_after(app, gallery, 1, 0);
    check(
        "File > New Window opens a second painted window",
        via_menu && gallery.window_count() == 2 && app.window_count() == 2 && second_painted,
    );

    // Settings: save, reload from disk, rebuild, and compare.
    with_page::<CanvasPage, _>(gallery, "canvas", |tree, page, _| page.set_zoom(tree, 2.5));
    gallery.set_filter(0, "canvas");
    let expected = gallery.session();
    let saved = gallery.save();
    let path = gallery.settings.path().map(std::path::Path::to_path_buf);
    let reloaded = path.clone().map(|path| {
        let (settings, issues) = settings::load(Some(path));
        (Saved::read(&settings), issues)
    });
    if let Err(error) = &saved {
        println!("     save failed: {error}");
    }
    match &path {
        Some(path) => println!("     settings file: {}", path.display()),
        None => println!("     no settings file location"),
    }
    let restored = reloaded
        .as_ref()
        .is_some_and(|(saved, issues)| issues.is_empty() && *saved == expected);
    check(
        "settings save, then reload page, theme, base, language, geometry, filter, zoom, and dock",
        saved.is_ok()
            && restored
            && expected.page.as_deref() == Some("canvas")
            && expected.theme == Theme::Brand
            && expected.base == BaseColor::Mauve
            && expected.language == "vi"
            && expected.filter == "canvas"
            && (expected.state.canvas_zoom - 2.5).abs() < 1e-9
            && expected.state.dock == saved_dock
            && expected.window.is_some(),
    );
    let rebuilt = reloaded.map(|(saved, _)| rebuild(gallery, &saved));
    check(
        "a rebuilt window restores the page, filter, canvas zoom, and dock",
        rebuilt.is_some_and(|ok| ok),
    );
    check("a version 1 settings file migrates", migrates());

    // Teardown: closing every window releases it.
    while gallery.window_count() > 0 {
        gallery.run_command(app, app::CLOSE_WINDOW);
        settle(app, gallery);
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.window_count() > 0 && Instant::now() < deadline {
        app.dispatch_next_event(Duration::from_millis(20));
    }
    let left = app.window_count();
    if left > 0 {
        println!("     {left} native windows still tracked");
    }
    check("closing every window releases it (0 left)", left == 0);

    // Desktop capabilities. This runs after the release check because
    // AppKit keeps its status bar windows in `NSApp.windows` after a status
    // item is removed.
    let capabilities = choscorui::capabilities();
    let rows = capability_rows(&capabilities);
    for (name, support) in &rows {
        println!("     {name}: {support:?}");
    }
    let status = if capabilities.status_items.is_supported() {
        use crate::page::Desktop;
        gallery.desktop.toggle_status_item("autopilot") == Ok(true)
            && gallery.desktop.toggle_status_item("autopilot") == Ok(false)
    } else {
        true
    };
    check(
        "desktop capabilities are reported and the status item toggles",
        capabilities.backend == "macos" && status,
    );

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
