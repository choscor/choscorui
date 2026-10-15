//! Menus, the date picker calendar, and navigation panels in a window
//! that is too short for them: the window limits the popup, its content
//! scrolls, and the highlighted or focused row stays in view. A menu in
//! place, as in a popup window that the compositor made shorter, scrolls
//! in its own bounds.

mod hosted;

use choscorui_controls::{
    CalendarMode, CommandId, ContextMenu, ControlMessage, DatePicker, DropdownMenu, MenuItem,
    Menubar, NavigationLink, NavigationMenu,
};
use choscorui_core::{
    CivilDate, DisplayCommand, DisplayList, Key, Locale, Point, PointerButton, PointerEvent, Rect,
    WheelUnit,
};
use hosted::{Harness, rect};

/// Makes the window `height` tall, and places the control at `bounds`.
fn short_window(h: &mut Harness, height: f64, bounds: Rect) {
    let root = h.tree.root();
    h.tree
        .set_bounds(root, rect(0.0, 0.0, 800.0, height))
        .unwrap();
    h.place(bounds);
}

fn overlay(h: &Harness) -> Rect {
    h.host
        .get(h.node)
        .unwrap()
        .overlay()
        .expect("the popup is open")
}

/// The baseline y of the first text run that reads `text`.
fn baseline(list: &DisplayList, text: &str) -> Option<f64> {
    list.commands().iter().find_map(|command| match command {
        DisplayCommand::DrawText {
            text: run,
            baseline,
            ..
        } if run == text => Some(baseline.y),
        _ => None,
    })
}

/// Whether `text` paints inside `frame`, once the open motion is done.
fn shows(h: &mut Harness, text: &str, frame: Rect) -> bool {
    h.set_time(h.time + 1.0);
    baseline(&h.paint(), text).is_some_and(|y| y > frame.origin.y && y < frame.max_y())
}

fn wheel(h: &mut Harness, at: Point, dy: f64) {
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Wheel {
            position: at,
            delta: Point::new(0.0, dy),
            unit: WheelUnit::Points,
        },
    );
}

fn long_menu() -> Vec<MenuItem> {
    let mut items: Vec<MenuItem> = (0..30)
        .map(|index| MenuItem::action(format!("Item {index}"), CommandId(index + 1)))
        .collect();
    items.push(MenuItem::submenu(
        "More",
        (0..30)
            .map(|index| MenuItem::action(format!("Sub {index}"), CommandId(index + 100)))
            .collect(),
    ));
    items
}

fn open_context_menu(h: &mut Harness, at: Point) {
    for event in [
        PointerEvent::Down {
            position: at,
            button: PointerButton::Secondary,
        },
        PointerEvent::Up {
            position: at,
            button: PointerButton::Secondary,
        },
    ] {
        h.host.dispatch_pointer(&mut h.tree, event);
    }
}

#[test]
fn a_long_context_menu_fits_the_window_and_its_rows_scroll() {
    let mut h = Harness::new(ContextMenu::new("Actions", "Right click here", long_menu()));
    short_window(&mut h, 300.0, rect(0.0, 0.0, 800.0, 300.0));
    open_context_menu(&mut h, Point::new(100.0, 250.0));
    let frame = overlay(&h);
    assert!(frame.origin.y >= 8.0 && frame.max_y() <= 292.0, "{frame:?}");
    assert!(shows(&mut h, "Item 0", frame));
    assert!(!shows(&mut h, "Item 29", frame));

    // End highlights the last row, and the rows scroll to show it.
    h.key(Key::End);
    assert!(shows(&mut h, "More", frame));
    assert!(!shows(&mut h, "Item 0", frame));
    // Home scrolls back.
    h.key(Key::Home);
    assert!(shows(&mut h, "Item 0", frame));

    // The wheel scrolls the rows, and stops at the end.
    wheel(&mut h, frame.center(), 10_000.0);
    assert!(shows(&mut h, "More", frame));
    assert!(!shows(&mut h, "Item 0", frame));
    wheel(&mut h, frame.center(), -10_000.0);
    assert!(shows(&mut h, "Item 0", frame));
}

#[test]
fn a_long_menu_of_the_menu_bar_fits_the_window_and_its_rows_scroll() {
    let mut h = Harness::new(Menubar::new(
        "Menu bar",
        vec![("File".to_owned(), long_menu())],
    ));
    short_window(&mut h, 300.0, rect(0.0, 0.0, 800.0, 32.0));
    let title = Point::new(20.0, 16.0);
    for event in [
        PointerEvent::Down {
            position: title,
            button: PointerButton::Primary,
        },
        PointerEvent::Up {
            position: title,
            button: PointerButton::Primary,
        },
    ] {
        h.host.dispatch_pointer(&mut h.tree, event);
    }
    let frame = overlay(&h);
    assert!(frame.origin.y >= 8.0 && frame.max_y() <= 292.0, "{frame:?}");
    assert!(shows(&mut h, "Item 0", frame));
    assert!(!shows(&mut h, "Item 29", frame));

    // End highlights the last row, and the rows scroll to show it.
    h.key(Key::End);
    assert!(shows(&mut h, "More", frame));
    assert!(!shows(&mut h, "Item 0", frame));
    h.key(Key::Home);
    assert!(shows(&mut h, "Item 0", frame));

    // The wheel scrolls the rows.
    wheel(&mut h, frame.center(), 10_000.0);
    assert!(shows(&mut h, "More", frame));
    assert!(!shows(&mut h, "Item 0", frame));
}

#[test]
fn a_long_submenu_fits_the_window_and_keeps_its_highlight_in_view() {
    let mut h = Harness::new(ContextMenu::new("Actions", "Right click here", long_menu()));
    short_window(&mut h, 300.0, rect(0.0, 0.0, 800.0, 300.0));
    open_context_menu(&mut h, Point::new(100.0, 20.0));
    h.key(Key::End);
    h.key(Key::ArrowRight);
    let frame = overlay(&h);
    assert!(frame.origin.y >= 8.0 && frame.max_y() <= 292.0, "{frame:?}");
    assert!(shows(&mut h, "Sub 0", frame));
    h.key(Key::End);
    assert!(shows(&mut h, "Sub 29", frame));
    assert!(!shows(&mut h, "Sub 0", frame));
}

#[test]
fn a_menu_in_place_with_less_height_than_its_rows_scrolls() {
    let mut h = Harness::new(DropdownMenu::new(long_menu()).hosted("Actions"));
    let natural = h.measure();
    let bounds = rect(0.0, 0.0, natural.width, 200.0);
    short_window(&mut h, 200.0, bounds);
    // The control keeps asking for the height of all of its rows.
    assert_eq!(h.measure(), natural);
    assert!(shows(&mut h, "Item 0", bounds));
    assert!(!shows(&mut h, "Item 29", bounds));

    // Keys scroll the highlighted row into view.
    h.focus();
    h.key(Key::End);
    assert!(shows(&mut h, "More", bounds));
    assert!(!shows(&mut h, "Item 0", bounds));
    h.key(Key::Home);
    assert!(shows(&mut h, "Item 0", bounds));
    for _ in 0..12 {
        h.key(Key::ArrowDown);
    }
    assert!(shows(&mut h, "Item 12", bounds));
    assert!(!shows(&mut h, "Item 0", bounds));

    // The wheel scrolls, and a click chooses the row under the pointer.
    wheel(&mut h, bounds.center(), 10_000.0);
    assert!(shows(&mut h, "Item 29", bounds));
    let list = h.paint();
    let y = baseline(&list, "Item 29").expect("the row shows");
    h.messages();
    h.click(Point::new(bounds.center().x, y - 4.0));
    assert!(
        h.messages()
            .contains(&ControlMessage::Command(CommandId(30))),
        "the click chooses the scrolled row"
    );
}

fn date(day: u8) -> CivilDate {
    CivilDate::new(2026, 10, day).unwrap()
}

#[test]
fn a_calendar_in_a_short_window_scrolls_to_the_cursor_day() {
    let picker = DatePicker::new("Due date", Locale::parse("en").unwrap(), date(12))
        .with_mode(CalendarMode::Single)
        .with_placeholder("Pick a date");
    let mut h = Harness::new(picker);
    let size = h.bounds().size;
    short_window(&mut h, 220.0, rect(10.0, 10.0, size.width, size.height));
    h.focus();
    h.key(Key::Enter);
    // The picker asks for a layout to scroll; the window lays it out
    // again after each change.
    let bounds = h.bounds();
    h.place(bounds);
    let frame = overlay(&h);
    assert!(frame.max_y() <= 212.0, "limited to the window: {frame:?}");
    assert!(shows(&mut h, "12", frame), "the cursor day shows");

    // Arrow keys move the cursor down, and the grid scrolls to it.
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    h.place(bounds);
    assert!(shows(&mut h, "26", frame));

    // The wheel scrolls back to the caption.
    wheel(&mut h, frame.center(), -10_000.0);
    h.place(bounds);
    assert!(shows(&mut h, "October 2026", frame));
    assert!(!shows(&mut h, "26", frame));
}

#[test]
fn a_navigation_panel_in_a_short_window_scrolls_to_the_focused_link() {
    let links = ["Alert Dialog", "Hover Card", "Progress", "Scroll-area"]
        .map(|title| NavigationLink::new(title, "Re-usable components built with tokens."))
        .to_vec();
    let menu = NavigationMenu::new("Site", vec![("Components".to_string(), links)]);
    let mut h = Harness::new(menu);
    let size = h.bounds().size;
    short_window(&mut h, 150.0, rect(10.0, 10.0, size.width, size.height));
    h.focus();
    h.key(Key::ArrowDown);
    let panel = overlay(&h);
    assert!(panel.max_y() <= 142.0, "limited to the window: {panel:?}");
    assert!(shows(&mut h, "Alert Dialog", panel));
    for _ in 0..3 {
        h.key(Key::ArrowDown);
    }
    assert!(shows(&mut h, "Scroll-area", panel));
    assert!(!shows(&mut h, "Alert Dialog", panel));
    wheel(&mut h, panel.center(), -10_000.0);
    assert!(shows(&mut h, "Alert Dialog", panel));
}
