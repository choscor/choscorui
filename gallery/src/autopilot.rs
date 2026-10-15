//! Scripted end-to-end checks of the gate evidence, run with
//! `CHOSCORUI_GALLERY_AUTOPILOT=1`: every page, themes, a second
//! language, windows, large data, the canvas, structured input, settings
//! and dock restore, desktop capabilities, translations, and window
//! release. The checks drive the views through their semantics, as
//! assistive technology does. One line per check, then `OK (0 failures)`
//! or `FAILED (<n> failures)`.
//!
//! This is the one gallery module that reads native state: the paint
//! count of a window and the windows that the platform still tracks. It
//! reads them through `choscorui::platform` on macOS and Linux; other
//! targets never start the gallery, so they get stand-ins.

use crate::catalog::PAGES;
use crate::settings::{self, Saved};
use crate::strings::{LANGUAGE_NAMES, Source};
use crate::ui::app::Gallery;
use crate::ui::pages::containers::dock::{default_dock, move_console};
use crate::ui::pages::data::RECORDS;
use crate::ui::pages::data::canvas::{CANVAS_COLUMNS, CANVAS_ITEMS, CANVAS_SPACING};
use crate::ui::pages::datetime::{LIMITED, date_time, today};
use crate::ui::pages::system::capability_rows;
use crate::ui::shell::{Theme, WindowState};
use choscorui::compose::view::ViewHost;
use choscorui::controls::{ControlKind, DateTimeEdit, DateTimeKind, ShortcutModifiers};
use choscorui::core::{
    BaseColor, Key, KeyEvent, Locale, Localizer, NodeId, Point, PointerButton, PointerEvent,
    SemanticAction, SemanticNode, StatusItemSpec, Support, WheelUnit, WindowGeometry,
};
use choscorui::{AppContext, KeyConvention, WindowId};
use std::time::{Duration, Instant};

/// The most semantic nodes that paging through the 100k records may add.
const REALIZED_ROW_LIMIT: usize = 200;

/// The most semantic nodes the window may publish on the canvas page.
const CANVAS_NODE_LIMIT: usize = 400;

/// Runs the event loop for a short while.
fn settle(cx: &mut AppContext) {
    let deadline = Instant::now() + Duration::from_millis(200);
    while Instant::now() < deadline && cx.pump() {}
}

/// The native state that the checks read.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod native {
    use choscorui::platform::NativeAccess;
    use choscorui::{AppContext, WindowId};
    use std::time::{Duration, Instant};

    /// The frames that window `id` presented.
    pub fn paint_count(cx: &mut AppContext, id: WindowId) -> usize {
        cx.native_window(id)
            .map_or(0, |window| window.paint_count())
    }

    /// When window `id` finished its first frame. A later frame may end
    /// before the caller looks: a Metal frame ends on another thread,
    /// which does not end the event wait of `AppContext::pump`.
    pub fn first_paint(cx: &mut AppContext, id: WindowId) -> Option<Instant> {
        cx.native_window(id)
            .and_then(|window| window.first_paint())
            .map(|paint| paint.finished)
    }

    /// The windows that the platform still tracks.
    pub fn window_count(cx: &AppContext) -> usize {
        cx.native_application().window_count()
    }

    /// Handles native events until the platform releases every window
    /// or `deadline` passes.
    pub fn release(cx: &AppContext, deadline: Instant) {
        while window_count(cx) > 0 && Instant::now() < deadline {
            // A lost display connection ends the wait at the deadline.
            let _ = cx
                .native_application()
                .dispatch_next_event(Duration::from_millis(20));
        }
    }
}

/// Stand-ins: a target without a backend never starts the gallery.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod native {
    use choscorui::{AppContext, WindowId};
    use std::time::Instant;

    pub fn paint_count(_: &mut AppContext, _: WindowId) -> usize {
        0
    }

    pub fn first_paint(_: &mut AppContext, _: WindowId) -> Option<Instant> {
        None
    }

    pub fn window_count(cx: &AppContext) -> usize {
        cx.windows().len()
    }

    pub fn release(_: &AppContext, _: Instant) {}
}

use native::paint_count;

/// Pumps until the main window presents its first frame and returns the
/// time from `entered` to that frame, or `None` when no window opened.
pub fn first_frame(gallery: &Gallery, cx: &mut AppContext, entered: Instant) -> Option<Duration> {
    loop {
        let main = (*gallery.main.borrow())?;
        if let Some(finished) = native::first_paint(cx, main) {
            return Some(finished - entered);
        }
        if !cx.pump() {
            return None;
        }
    }
}

/// Chooses the menu bar item titled `title`, as the user does. Returns
/// false when no enabled item has that title.
fn choose(cx: &mut AppContext, title: &str) -> bool {
    let item = cx.menu_bar().and_then(|menu| menu.find(title));
    item.is_some_and(|id| cx.choose(id))
}

/// Whether the menu bar item titled `title` has a check mark.
fn checked(cx: &AppContext, title: &str) -> bool {
    cx.menu_bar().is_some_and(|menu| {
        menu.find(title)
            .and_then(|id| menu.state(id).ok())
            .is_some_and(|state| state.checked)
    })
}

/// Pumps until window `id` presents a frame after `before`.
fn painted_after(cx: &mut AppContext, id: WindowId, before: usize) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if paint_count(cx, id) > before {
            return true;
        }
        if !cx.pump() {
            return false;
        }
    }
    false
}

fn page_index(id: &str) -> usize {
    PAGES
        .iter()
        .position(|page| page.id == id)
        .expect("autopilot pages exist")
}

fn host(cx: &mut AppContext, id: WindowId) -> &mut ViewHost {
    cx.host(id).expect("the autopilot window is open")
}

/// The published semantics of the node named `name`.
fn semantic(host: &mut ViewHost, name: &str) -> Option<SemanticNode> {
    let node = host.find(name)?;
    host.semantic(node)
}

/// The canvas control, found by its control kind. The sidebar button and
/// the page title share its name, which is translated.
fn canvas_node(host: &mut ViewHost) -> Option<NodeId> {
    host.semantics()
        .0
        .into_iter()
        .find(|entry| {
            entry.part.is_none() && host.control_kind(entry.id) == Some(ControlKind::Canvas)
        })
        .map(|entry| entry.id)
}

/// Performs `action` with `value` on the node named `name`.
fn act(host: &mut ViewHost, name: &str, action: SemanticAction, value: Option<&str>) -> bool {
    let Some(node) = host.find(name) else {
        println!("     no node named {name:?}");
        return false;
    };
    let done = host.perform(node, action, value);
    host.layout();
    done
}

fn press(host: &mut ViewHost, key: Key, modifiers: ShortcutModifiers) {
    host.key(KeyEvent::Down { key, repeat: false }, modifiers);
    host.key(KeyEvent::Up { key }, modifiers);
    host.layout();
}

fn wheel(host: &mut ViewHost, at: Point, delta: Point, modifiers: ShortcutModifiers) {
    let event = PointerEvent::Wheel {
        position: at,
        delta,
        unit: WheelUnit::Points,
    };
    host.pointer(event, modifiers);
    host.layout();
}

fn drag(host: &mut ViewHost, from: Point, to: Point) {
    let (none, button) = (ShortcutModifiers::default(), PointerButton::Primary);
    let down = PointerEvent::Down {
        position: from,
        button,
    };
    host.pointer(down, none);
    host.pointer(PointerEvent::Move { position: to }, none);
    let up = PointerEvent::Up {
        position: to,
        button,
    };
    host.pointer(up, none);
    host.layout();
}

/// Enters `text` in the field named `field` and returns whether the
/// field then reads as valid and whether it flags an error.
fn enter(host: &mut ViewHost, field: &str, text: &str) -> Option<(bool, bool)> {
    if !act(host, field, SemanticAction::SetValue, Some(text)) {
        return None;
    }
    let node = semantic(host, field)?;
    Some((
        !node.state.invalid && node.description == "Valid",
        node.state.invalid,
    ))
}

/// Sets `text` as the value of `field` and reports whether the field's
/// filter refused it: the value stays as it was.
fn refused(host: &mut ViewHost, field: &str, text: &str) -> bool {
    let before = semantic(host, field).and_then(|node| node.value);
    act(host, field, SemanticAction::SetValue, Some(text));
    before.is_some() && semantic(host, field).and_then(|node| node.value) == before
}

/// The value of `field`.
fn value_of(host: &mut ViewHost, field: &str) -> Option<String> {
    semantic(host, field).and_then(|node| node.value)
}

/// The state of window `id`.
fn state(gallery: &Gallery, id: WindowId) -> WindowState {
    gallery.windows.borrow()[&id].clone()
}

/// Shows page `page` in window `id` and waits for its frame.
fn show(cx: &mut AppContext, gallery: &Gallery, id: WindowId, page: &str) -> bool {
    let index = page_index(page);
    let window = state(gallery, id);
    if window.page.get() == index {
        return true;
    }
    let (before, errors) = (paint_count(cx, id), gallery.errors.borrow().len());
    window.page.set(index);
    let painted = painted_after(cx, id, before);
    if gallery.errors.borrow().len() > errors {
        println!("     page {page} reported an error");
    }
    painted
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
    let (settings, issues) = settings::load(path);
    let _ = std::fs::remove_dir_all(&dir);
    written.is_ok()
        && Saved::read(&settings).page.as_deref() == Some("slider")
        && issues
            .iter()
            .any(|issue| issue.contains("migrated from version 1"))
}

/// What the limited date/time editor reads at `hour`:00 on the demo day.
fn limited_at(hour: u8) -> Option<String> {
    let locale = Locale::parse("en").expect("valid tag");
    DateTimeEdit::new(
        LIMITED,
        locale,
        DateTimeKind::DateTime,
        date_time(today(), hour, 0),
    )
    .semantics(Localizer::builtin())
    .value
}

/// Moves the focus of the limited editor to the section `name`, from
/// any section.
fn focus_section(host: &mut ViewHost, name: &str) -> Option<()> {
    let none = ShortcutModifiers::default();
    for key in [Key::ArrowRight, Key::ArrowLeft] {
        for _ in 0..8 {
            if semantic(host, LIMITED)?.description == name {
                return Some(());
            }
            press(host, key, none);
        }
    }
    None
}

/// Types day `day` into the limited editor, which allows only one day.
/// Returns whether the editor flags the typed value, and the value after
/// Return clamps it.
fn clamp_day(host: &mut ViewHost, day: &str) -> Option<(bool, Option<String>)> {
    let none = ShortcutModifiers::default();
    focus_section(host, "Day field")?;
    for digit in day.chars() {
        host.text(&digit.to_string());
        host.layout();
    }
    let flagged = semantic(host, LIMITED)?.state.invalid;
    press(host, Key::Enter, none);
    Some((flagged, semantic(host, LIMITED)?.value))
}

/// Types a day after and a day before the one day that the limited
/// editor allows; Return clamps. The day moves the value out of the
/// range in a 12-hour and in a 24-hour locale, at any time of day.
/// Returns whether each typed day was flagged, and the two clamped
/// values.
fn clamp_limited(host: &mut ViewHost) -> Option<(bool, Option<String>, Option<String>)> {
    if !act(host, LIMITED, SemanticAction::Focus, None) {
        return None;
    }
    let (late, high) = clamp_day(host, "13")?;
    let (early, low) = clamp_day(host, "11")?;
    Some((late && early, high, low))
}

/// The record count that the item view readout shows.
fn record_matches(host: &mut ViewHost) -> Option<usize> {
    let suffix = format!(" of {RECORDS} records");
    host.semantics()
        .0
        .into_iter()
        .find_map(|entry| entry.node.name.strip_suffix(&suffix)?.parse().ok())
}

/// Searches the 100k records for "7" and pages through the matches.
/// Returns all records, the matches, the most nodes that paging added,
/// and the most paint commands of a frame.
fn search_records(host: &mut ViewHost) -> Option<(usize, usize, usize, usize)> {
    let all = record_matches(host)?;
    if !act(host, "Search records", SemanticAction::SetValue, Some("7"))
        || !act(host, "Records", SemanticAction::Focus, None)
    {
        return None;
    }
    let matches = record_matches(host)?;
    let baseline = host.semantics().0.len();
    let (mut nodes, mut commands) = (0, 0);
    for _ in 0..2_000 {
        press(host, Key::PageDown, ShortcutModifiers::default());
        nodes = nodes.max(host.semantics().0.len().saturating_sub(baseline));
        commands = commands.max(host.paint().len());
    }
    Some((all, matches, nodes, commands))
}

/// Zooms the canvas to 2x about its origin, pans it, hit-tests item 201,
/// and rubber-band selects. Returns the hit item, whether the readout
/// shows 4 selected at 200%, the paint commands, and the published nodes.
fn exercise_canvas(host: &mut ViewHost) -> Option<(Option<String>, bool, usize, usize)> {
    let canvas = canvas_node(host)?;
    let bounds = host.tree().bounds(canvas)?;
    // The physical key of `command`: Command on a Mac, Ctrl on a PC.
    let command = match host.key_convention() {
        KeyConvention::Mac => ShortcutModifiers {
            command: true,
            ..ShortcutModifiers::default()
        },
        _ => ShortcutModifiers {
            control: true,
            ..ShortcutModifiers::default()
        },
    };
    // Command (Ctrl) and the wheel zoom about the pointer: e^(-0.01 dy).
    let doubling = Point::new(0.0, -100.0 * std::f64::consts::LN_2);
    wheel(host, bounds.origin, doubling, command);
    // Pan so item 0 sits 100 points inside the canvas.
    let inside = Point::new(bounds.origin.x + 10.0, bounds.origin.y + 10.0);
    let pan = Point::new(-100.0, -100.0);
    wheel(host, inside, pan, ShortcutModifiers::default());
    let center = |index: usize| {
        let scene = |offset: usize| (offset as f64 * CANVAS_SPACING + 14.0 + 50.0) * 2.0;
        Point::new(
            bounds.origin.x + scene(index % CANVAS_COLUMNS),
            bounds.origin.y + scene(index / CANVAS_COLUMNS),
        )
    };
    let hit = center(201);
    drag(host, hit, hit);
    // The item under the click is the focused part of the canvas.
    let named = host
        .semantics()
        .0
        .into_iter()
        .find(|entry| entry.id == canvas && entry.part.is_some() && entry.focused)
        .map(|entry| entry.node.name);
    // Items are 28 units square on a 40-unit grid; start and end the band
    // 18 units outside them so the press lands on empty space. The band
    // spans items 0, 1, 200 and 201 and stops short of column 2.
    let margin = 18.0 * 2.0;
    let (from, to) = (center(0), center(201));
    drag(
        host,
        Point::new(from.x - margin, from.y - margin),
        Point::new(to.x + margin, to.y + margin),
    );
    let selected = host.find("Zoom 200% · 4 selected").is_some();
    let painted = host.paint().len();
    Some((named, selected, painted, host.semantics().0.len()))
}

/// Opens windows from `saved`, as a relaunch would, and checks what they
/// restore: the page, filter, and canvas zoom, then the dock.
fn restores(gallery: &std::rc::Rc<Gallery>, cx: &mut AppContext, saved: &Saved) -> bool {
    let start = settings::start_page(None, saved.page.as_deref());
    let Some(canvas) = gallery.open(cx, start, &saved.filter, &saved.state, false) else {
        return false;
    };
    let dock = gallery.open(cx, page_index("dock"), "", &saved.state, false);
    settle(cx);
    let window = state(gallery, canvas);
    let host = host(cx, canvas);
    // The canvas reports its zoom in the language of the window.
    let percent = (saved.state.canvas_zoom * 100.0).round().to_string();
    let zoom = host.env().text("choscorui.zoom", &[("percent", &percent)]);
    let shown = canvas_node(host)
        .and_then(|node| host.semantic(node))
        .map(|node| node.description);
    let canvas_ok = PAGES[window.page.get()].id == "canvas"
        && window.filter.get() == saved.filter
        && (window.store.canvas_zoom.get() - saved.state.canvas_zoom).abs() < 1e-9
        && shown.as_deref() == Some(zoom.as_str());
    if !canvas_ok {
        println!(
            "     rebuilt canvas: page {} zoom {shown:?}",
            PAGES[window.page.get()].id
        );
    }
    let dock_ok = dock.is_some_and(|id| state(gallery, id).store.dock.get() == saved.state.dock);
    for id in [Some(canvas), dock].into_iter().flatten() {
        gallery.close(cx, id);
    }
    canvas_ok && dock_ok
}

pub fn run(gallery: &std::rc::Rc<Gallery>, cx: &mut AppContext) -> usize {
    let mut failures = 0;
    let mut check = |name: &str, ok: bool| {
        println!("{} {name}", if ok { "ok  " } else { "FAIL" });
        failures += usize::from(!ok);
    };
    for note in gallery.notes.borrow().iter() {
        println!("     note: {note}");
    }
    let shared = gallery.shared.clone();
    // The rule fields carry the locale in their names; start in English.
    shared.language.set("en");

    // Translations.
    let vi_file = !gallery
        .notes
        .borrow()
        .iter()
        .any(|note| note.contains("vi.lang"));
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
    let Some(main) = *gallery.main.borrow() else {
        check("first window painted", false);
        return failures;
    };
    check("first window painted", painted_after(cx, main, 0));

    // Every page, selected through the window state and presented.
    let mut unpainted = Vec::new();
    for page in PAGES {
        if !show(cx, gallery, main, page.id) {
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
    let before = paint_count(cx, main);
    let brand = shared.text("theme.brand", &[]);
    let pressed = act(host(cx, main), &brand, SemanticAction::Activate, None);
    check(
        "the Brand theme button restyles and repaints",
        pressed && shared.theme.get() == Theme::Brand && painted_after(cx, main, before),
    );
    settle(cx);
    check("the View menu checks the active theme", checked(cx, &brand));
    let light = shared.text("theme.light", &[]);
    let chose_light = choose(cx, &light);
    settle(cx);
    let lit = shared.theme.get() == Theme::Light && checked(cx, &light);
    let chose_brand = choose(cx, &brand);
    settle(cx);
    check(
        "View > Theme sets the theme and moves the check mark",
        chose_light && lit && chose_brand && shared.theme.get() == Theme::Brand,
    );

    // A base color swatch restyles and repaints. The swatch differs from
    // the base that the settings file restored.
    let base = if shared.base.get() == BaseColor::Mauve {
        BaseColor::Olive
    } else {
        BaseColor::Mauve
    };
    let before = paint_count(cx, main);
    let swatch = shared.text(&format!("base.{}", base.id()), &[]);
    let pressed = act(host(cx, main), &swatch, SemanticAction::Activate, None);
    check(
        "a base color swatch restyles and repaints",
        pressed && shared.base.get() == base && painted_after(cx, main, before),
    );

    // The Enabled toggle disables the page and enables it again.
    let enabled = shared.text("state.enabled", &[]);
    let window = state(gallery, main);
    act(host(cx, main), &enabled, SemanticAction::Activate, None);
    let disabled = !window.enabled.get();
    act(host(cx, main), &enabled, SemanticAction::Activate, None);
    check(
        "the Enabled toggle disables and re-enables the page",
        disabled && window.enabled.get(),
    );

    // Structured inputs refuse text that cannot become valid, and mark
    // invalid values.
    show(cx, gallery, main, "number-input");
    let amount = "Amount (0 – 1,000,000,000; 2 decimals, locale en)";
    let quantity = "Quantity (1 – 10,000; whole numbers)";
    let numbers = {
        let host = host(cx, main);
        (
            refused(host, amount, "12abc"),
            enter(host, quantity, "0"),
            enter(host, quantity, "20000"),
            enter(host, quantity, "12"),
        )
    };
    check(
        "number input rejects letters and out-of-range values",
        matches!(
            numbers,
            (
                true,
                Some((false, false)),
                Some((false, true)),
                Some((true, false))
            )
        ),
    );
    // The plate mask `>AA-99999` needs two letters first and uppercases
    // them.
    show(cx, gallery, main, "input-mask");
    let plate = "Licence plate, mask >AA-99999 (uppercased)";
    let mask = {
        let host = host(cx, main);
        let digits = refused(host, plate, "12");
        let partial = enter(host, plate, "a");
        let incomplete = semantic(host, plate).map(|node| node.description);
        let upper = value_of(host, plate);
        let full = enter(host, plate, "ab12345");
        (
            digits,
            partial,
            incomplete,
            upper,
            full,
            value_of(host, plate),
        )
    };
    check(
        "input mask rejects characters its format forbids",
        matches!(mask, (true, Some((false, false)), Some(ref text), Some(ref upper), Some((true, false)), Some(ref value))
            if text == "Incomplete" && upper == "A" && value == "AB-12345"),
    );
    show(cx, gallery, main, "date-input");
    let dates = {
        let host = host(cx, main);
        (
            refused(host, "Any date (mm/dd/yyyy)", "not a date"),
            enter(host, "Any date (mm/dd/yyyy)", "2/30/2026"),
            enter(host, "A date in 2026", "1/1/2025"),
        )
    };
    check(
        "date input rejects text and dates outside 2026",
        matches!(dates, (true, Some((false, true)), Some((false, true)))),
    );
    show(cx, gallery, main, "date-time-edit");
    let limited = clamp_limited(host(cx, main));
    check(
        "date/time edit clamps to its 09:00 – 17:00 range",
        limited.is_some_and(|(flagged, high, low)| {
            flagged && high == limited_at(17) && low == limited_at(9)
        }),
    );

    // 100k records: search, then page through with bounded work.
    show(cx, gallery, main, "item-view");
    let large = search_records(host(cx, main));
    if let Some((all, matches, nodes, commands)) = large {
        println!(
            "     {all} records, {matches} match \"7\", at most {nodes} more nodes and {commands} paint commands"
        );
    }
    check(
        "100k-record search keeps published rows and painting bounded over 2,000 Page Downs",
        large.is_some_and(|(all, matches, nodes, commands)| {
            all == RECORDS
                && matches > 0
                && matches < all
                && nodes <= REALIZED_ROW_LIMIT
                && commands < RECORDS / 20
        }),
    );

    // Canvas: zoomed hit test, rubber band, culling, bounded semantics.
    window.store.canvas_zoom.set(1.0);
    show(cx, gallery, main, "canvas");
    let canvas = exercise_canvas(host(cx, main));
    check(
        "canvas hit-tests at 2x zoom and rubber-band selects",
        canvas
            .as_ref()
            .is_some_and(|(hit, selected, _, _)| hit.as_deref() == Some("Node 201") && *selected),
    );
    check(
        "canvas culls painting and keeps semantics bounded",
        canvas.is_some_and(|(_, _, painted, nodes)| {
            painted > 0 && painted < CANVAS_ITEMS && nodes <= CANVAS_NODE_LIMIT
        }),
    );

    // Dock: the arrangement round-trips through the page state.
    window.store.dock.set(String::new());
    show(cx, gallery, main, "dock");
    let moved = act(
        host(cx, main),
        "Move Console",
        SemanticAction::Activate,
        None,
    );
    let mut expected_dock = default_dock();
    let saved_dock = window.store.dock.get();
    check(
        "dock moves a panel and persists the arrangement",
        moved
            && move_console(&mut expected_dock)
            && !saved_dock.is_empty()
            && saved_dock == expected_dock.save(),
    );

    // View > Language > Vietnamese relabels the window and rebuilds the
    // menu bar.
    let chose_vi = choose(cx, LANGUAGE_NAMES[1]);
    settle(cx);
    let file_title = shared.text("menu.file", &[]);
    let title = shared.window_title(window.page.get());
    let menu_ok = cx
        .menu_bar()
        .is_some_and(|menu| menu.menus()[1].title == file_title);
    check(
        "View > Language > Vietnamese rebuilds the menu bar and window title",
        chose_vi
            && shared.language.get() == "vi"
            && file_title != "File"
            && menu_ok
            && cx.title(main) == Some(title)
            && checked(cx, LANGUAGE_NAMES[1]),
    );

    // A second window through File > New Window.
    let via_menu = choose(cx, &shared.text("menu.new_window", &[]));
    settle(cx);
    let second = cx.windows().into_iter().find(|id| *id != main);
    let second_painted = second.is_some_and(|id| painted_after(cx, id, 0));
    check(
        "File > New Window opens a second painted window",
        via_menu && cx.windows().len() == 2 && native::window_count(cx) == 2 && second_painted,
    );

    // Settings: save, reload from disk, rebuild, and compare.
    window.store.canvas_zoom.set(2.5);
    show(cx, gallery, main, "canvas");
    window.filter.set("canvas".to_owned());
    settle(cx);
    let expected = gallery.session();
    let saved = gallery.save(cx);
    let path = cx.storage().path();
    let reloaded = path.clone().map(|path| {
        let (settings, issues) = settings::load(path);
        let geometry = WindowGeometry::restore(&settings, settings::MAIN_WINDOW);
        (Saved::read(&settings), issues, geometry.is_some())
    });
    if let Err(error) = &saved {
        println!("     save failed: {error}");
    }
    match &path {
        Some(path) => println!("     settings file: {}", path.display()),
        None => println!("     no settings file location"),
    }
    let restored = reloaded.as_ref().is_some_and(|(saved, issues, geometry)| {
        issues.is_empty() && *saved == expected && *geometry
    });
    check(
        "settings save, then reload page, theme, base, language, geometry, filter, zoom, and dock",
        saved.is_ok()
            && restored
            && expected.page.as_deref() == Some("canvas")
            && expected.theme == Theme::Brand
            && expected.base == base
            && expected.language == "vi"
            && expected.filter == "canvas"
            && (expected.state.canvas_zoom - 2.5).abs() < 1e-9
            && expected.state.dock == saved_dock,
    );
    let rebuilt = reloaded.map(|(saved, _, _)| restores(gallery, cx, &saved));
    check(
        "rebuilt windows restore the page, filter, canvas zoom, and dock",
        rebuilt.is_some_and(|ok| ok),
    );
    check("a version 1 settings file migrates", migrates());
    let errors = gallery.errors.borrow().clone();
    for error in &errors {
        println!("     view error: {error}");
    }
    check("no view reported an error", errors.is_empty());

    // Teardown: closing every window releases it.
    for id in cx.windows() {
        gallery.close(cx, id);
        settle(cx);
    }
    native::release(cx, Instant::now() + Duration::from_secs(3));
    let left = native::window_count(cx);
    if left > 0 {
        println!("     {left} native windows still tracked");
    }
    check("closing every window releases it (0 left)", left == 0);

    // Desktop capabilities. This runs after the release check because
    // AppKit keeps its status bar windows in `NSApp.windows` after a status
    // item is removed.
    let capabilities = choscorui::capabilities();
    for (key, support) in capability_rows(&capabilities) {
        println!("     {}: {support:?}", shared.text(key, &[]));
    }
    let services = cx.services().clone();
    let status = if capabilities.status_items.is_supported() {
        services
            .set_status_item(Some(StatusItemSpec::new("autopilot")))
            .is_ok()
            && services.status_item().is_some()
            && services.set_status_item(None).is_ok()
            && services.status_item().is_none()
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
