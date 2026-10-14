//! The theme focus ring on keyboard focus, through the control host.

mod hosted;

use choscorui_controls::{
    Attachment, Breadcrumb, CalendarMode, CanvasView, Chart, ChartKind, ChartSeries, ColumnBrowser,
    DatePicker, Dial, HeaderColumn, HeaderView, IntegerRange, Toast, Toaster, Toasts,
};
use choscorui_core::{
    CivilDate, ColorRole, DesignTokens, DisplayCommand, Key, KeyEvent, Locale, Paint, Point, Rect,
    Scene, Size, TreeStore,
};
use hosted::Harness;

/// The focus rings that `h` paints once its state transitions settle.
fn rings(h: &mut Harness) -> Vec<Rect> {
    h.paint();
    h.set_time(h.time + 1.0);
    let tokens = DesignTokens::light();
    let ring = tokens.ring_color(tokens.color(ColorRole::Ring));
    h.paint()
        .commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::StrokeRoundedRect {
                rect, color, width, ..
            } if *color == ring && *width == tokens.ring_width => Some(*rect),
            _ => None,
        })
        .collect()
}

fn tab(h: &mut Harness) -> bool {
    h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false,
        },
    )
}

/// True when every ring of `h` is inside the bounds that clip it.
fn inside(h: &Harness, rings: &[Rect]) -> bool {
    let bounds = h.bounds();
    rings.iter().all(|ring| bounds.union(*ring) == bounds)
}

#[test]
fn the_toast_close_button_shows_the_ring_on_keyboard_focus() {
    let toasts = Toasts::new();
    let mut h = Harness::new(Toaster::new("Notifications", toasts.clone(), "Close"));
    toasts.push(Toast::new("Message deleted").with_action("Undo"));
    h.set_time(0.0);
    h.host.advance(&mut h.tree, 0.0);
    let bounds = h.bounds();
    h.place(bounds);
    assert!(rings(&mut h).is_empty());
    h.focus();
    let undo = rings(&mut h);
    assert!(tab(&mut h));
    assert_eq!(h.focused_part(), "Close");
    let close = rings(&mut h);
    assert_eq!((undo.len(), close.len()), (1, 1), "one ring on each button");
    assert_ne!(undo, close, "the ring moves to the close button");
}

#[test]
fn the_breadcrumb_ellipsis_shows_the_ring_on_keyboard_focus() {
    let trail = Breadcrumb::new(
        "Breadcrumb",
        ["Home", "Components", "Docs", "Breadcrumb"]
            .map(String::from)
            .to_vec(),
    )
    .with_collapsed(1..3);
    let mut h = Harness::new(trail);
    assert!(rings(&mut h).is_empty());
    h.focus();
    let home = rings(&mut h);
    assert!(tab(&mut h));
    assert_eq!(h.focused_part(), "Components, Docs");
    let ellipsis = rings(&mut h);
    assert_eq!((home.len(), ellipsis.len()), (1, 1));
    assert!(
        ellipsis[0].origin.x > home[0].max_x(),
        "after the first link"
    );
}

#[test]
fn a_chart_shows_the_ring_inside_its_bounds_on_keyboard_focus() {
    let chart = Chart::new(
        "Visitors",
        ChartKind::Bar,
        ["January", "February"].map(String::from).to_vec(),
        vec![ChartSeries::new("Desktop", vec![186.0, 305.0])],
    );
    let mut h = Harness::new(chart);
    assert!(rings(&mut h).is_empty());
    h.focus();
    let ring = rings(&mut h);
    assert_eq!(ring.len(), 1);
    assert!(inside(&h, &ring), "the clip does not cut the ring");
}

#[test]
fn a_column_browser_rings_the_focused_column_on_keyboard_focus() {
    let mut store = TreeStore::new(1);
    let documents = store.push(None, "Documents".to_owned()).unwrap();
    store.push(Some(documents), "Report".to_owned()).unwrap();
    let mut h = Harness::new(ColumnBrowser::new().hosted("Files", Box::new(store)));
    assert!(rings(&mut h).is_empty());
    h.focus();
    let first = rings(&mut h);
    assert_eq!(first.len(), 1);
    assert!(inside(&h, &first));
    h.key(Key::ArrowDown);
    h.key(Key::ArrowRight);
    let second = rings(&mut h);
    assert_eq!(second.len(), 1);
    assert!(second[0].origin.x > first[0].origin.x, "the next column");
}

#[test]
fn a_canvas_rings_itself_until_an_item_has_focus() {
    let viewport = Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0));
    let mut h = Harness::new(CanvasView::new("Diagram", viewport).hosted(Scene::new()));
    assert!(rings(&mut h).is_empty());
    h.focus();
    let ring = rings(&mut h);
    assert_eq!(ring.len(), 1);
    assert!(inside(&h, &ring));
}

/// The commands of `h` in the ring color, of any shape, once its state
/// transitions settle.
fn ring_strokes(h: &mut Harness) -> Vec<DisplayCommand> {
    h.paint();
    h.set_time(h.time + 1.0);
    let tokens = DesignTokens::light();
    let ring = tokens.ring_color(tokens.color(ColorRole::Ring));
    h.paint()
        .commands()
        .iter()
        .filter(|command| match command {
            DisplayCommand::StrokeRoundedRect { color, .. }
            | DisplayCommand::StrokeRect { color, .. } => *color == ring,
            DisplayCommand::StrokePath { paint, .. } => *paint == Paint::Solid(ring),
            _ => false,
        })
        .cloned()
        .collect()
}

#[test]
fn an_attachment_shows_one_ring_on_keyboard_focus() {
    let mut h = Harness::new(Attachment::new("report.pdf").with_remove("Remove report.pdf"));
    assert!(ring_strokes(&mut h).is_empty());
    h.focus();
    assert_eq!(ring_strokes(&mut h).len(), 1, "one ring, not two");
}

#[test]
fn a_dial_shows_one_ring_on_keyboard_focus() {
    let range = IntegerRange::new(0, 100, 10, 1, 10).unwrap();
    let mut h = Harness::new(Dial::new(range, false).hosted("Volume"));
    assert!(ring_strokes(&mut h).is_empty());
    h.focus();
    assert_eq!(ring_strokes(&mut h).len(), 1, "one ring, not two");
}

#[test]
fn a_header_view_insets_the_ring_of_the_focused_column() {
    let column = |title: &str| HeaderColumn {
        title: title.into(),
        width: 100.0,
        min_width: 40.0,
    };
    let mut h = Harness::new(HeaderView::new(vec![column("Name"), column("Size")]));
    assert!(rings(&mut h).is_empty());
    h.focus();
    let ring = rings(&mut h);
    assert_eq!(ring.len(), 1);
    let bounds = h.bounds();
    // The cells clip, so the ring is inside the first cell.
    assert!(inside(&h, &ring), "{ring:?} in {bounds:?}");
    assert!(ring[0].max_x() <= bounds.origin.x + 100.0, "{ring:?}");
}

fn due_date() -> DatePicker {
    let day = CivilDate::new(2026, 10, 12).unwrap();
    DatePicker::new("Due date", Locale::parse("en").unwrap(), day)
        .with_mode(CalendarMode::Single)
        .with_today(day)
}

#[test]
fn a_date_picker_rings_the_day_only_after_a_keyboard_open() {
    let mut pointer = Harness::new(due_date());
    let bounds = pointer.bounds();
    pointer.click(bounds.center());
    assert!(pointer.host.get(pointer.node).unwrap().overlay().is_some());
    assert!(rings(&mut pointer).is_empty(), "no ring after a click");

    let mut keyboard = Harness::new(due_date());
    keyboard.focus();
    let trigger = rings(&mut keyboard);
    assert_eq!(trigger.len(), 1, "the trigger ring");
    keyboard.key(Key::Enter);
    let frame = keyboard
        .host
        .get(keyboard.node)
        .unwrap()
        .overlay()
        .expect("the calendar is open");
    let open = rings(&mut keyboard);
    assert!(
        open.iter().any(|ring| frame.union(*ring) == frame),
        "a ring on the day in the calendar: {open:?} in {frame:?}"
    );
}
