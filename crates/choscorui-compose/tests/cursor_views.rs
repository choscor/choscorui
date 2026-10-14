//! Headless tests of the window cursor: the host shows the cursor of the
//! slot under the pointer, and keeps it during a drag.

use choscorui_compose::Runtime;
use choscorui_compose::view::{
    Badge, Binding, Env, Frame, HeaderView, InputGroup, Resizable, SizeGrip, SpinBox, Text, View,
    ViewExt, ViewHost,
};
use choscorui_controls::{
    AddonAlign, BadgeVariant, ControlSize, EstimatedText, HeaderColumn, InputGroupAddon,
    ShortcutModifiers, SplitOrientation,
};
use choscorui_core::{
    Catalog, CursorShape, DesignTokens, Locale, Point, PointerButton, PointerEvent, Rect, Size,
};
use std::rc::Rc;

/// A host 400 × 100 pt wide in the language `tag` that mounts `view`.
fn host(tag: &str, view: impl View + 'static) -> ViewHost {
    let mut host = ViewHost::new(
        Size::new(400.0, 100.0),
        DesignTokens::light(),
        Env::new(Rc::new(Catalog::new()), Locale::parse(tag).unwrap()),
        Runtime::new(),
        Rc::new(EstimatedText),
    );
    host.mount(view);
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    host
}

/// Sends `event` and paints, as a window frame does, and returns the
/// cursor that the window then shows.
fn cursor_after(host: &mut ViewHost, event: PointerEvent) -> CursorShape {
    host.pointer(event, ShortcutModifiers::default());
    host.paint();
    host.cursor()
}

fn moved(x: f64, y: f64) -> PointerEvent {
    PointerEvent::Move {
        position: Point::new(x, y),
    }
}

fn panes(orientation: SplitOrientation) -> impl View {
    let sizes = Binding::new(|| vec![100.0], |_| {});
    Resizable::new(
        "Panes",
        orientation,
        (Text::new("A"), Text::new("B")),
        sizes,
    )
    .frame(Frame::size(400.0, 100.0))
}

#[test]
fn a_resizable_handle_shows_the_resize_cursor_and_keeps_it_while_dragged() {
    let mut host = host("en", panes(SplitOrientation::Horizontal));
    assert_eq!(
        cursor_after(&mut host, moved(50.0, 50.0)),
        CursorShape::Arrow
    );
    assert_eq!(
        cursor_after(&mut host, moved(102.0, 50.0)),
        CursorShape::ResizeLeftRight,
        "over the handle"
    );
    let button = PointerButton::Primary;
    let position = Point::new(102.0, 50.0);
    cursor_after(&mut host, PointerEvent::Down { position, button });
    assert_eq!(
        cursor_after(&mut host, moved(160.0, 50.0)),
        CursorShape::ResizeLeftRight,
        "a drag keeps the cursor off the handle"
    );
    let position = Point::new(160.0, 50.0);
    cursor_after(&mut host, PointerEvent::Up { position, button });
    assert_eq!(
        cursor_after(&mut host, moved(300.0, 50.0)),
        CursorShape::Arrow
    );

    let mut stacked = host_vertical();
    assert_eq!(
        cursor_after(&mut stacked, moved(200.0, 51.0)),
        CursorShape::ResizeUpDown,
        "a handle between stacked panes"
    );
}

fn host_vertical() -> ViewHost {
    let sizes = Binding::new(|| vec![50.0], |_| {});
    let view = Resizable::new(
        "Rows",
        SplitOrientation::Vertical,
        (Text::new("A"), Text::new("B")),
        sizes,
    )
    .frame(Frame::size(400.0, 100.0));
    host("en", view)
}

#[test]
fn a_link_badge_shows_the_pointing_hand() {
    let badge = Badge::new("Docs").with(|badge| badge.with_variant(BadgeVariant::Link).as_link());
    let mut host = host("en", badge);
    let node = host.find("Docs").expect("the badge node");
    let center = host.tree().bounds(node).unwrap().center();
    assert_eq!(
        cursor_after(&mut host, moved(center.x, center.y)),
        CursorShape::PointingHand
    );
    assert_eq!(
        cursor_after(&mut host, moved(390.0, 90.0)),
        CursorShape::Arrow
    );
}

#[test]
fn a_plain_badge_in_the_link_look_keeps_the_arrow() {
    let badge = Badge::new("Docs").with(|badge| badge.with_variant(BadgeVariant::Link));
    let mut host = host("en", badge);
    let node = host.find("Docs").expect("the badge node");
    let center = host.tree().bounds(node).unwrap().center();
    assert_eq!(
        cursor_after(&mut host, moved(center.x, center.y)),
        CursorShape::Arrow,
        "the badge is not a link, so it does not look clickable"
    );
}

/// The cursor over the point at `fraction` of the bounds of the node
/// `label`, from the top start corner.
fn cursor_over(host: &mut ViewHost, label: &str, fraction: (f64, f64)) -> CursorShape {
    let node = host.find(label).expect("the node");
    let bounds: Rect = host.tree().bounds(node).unwrap();
    let x = bounds.origin.x + bounds.size.width * fraction.0;
    let y = bounds.origin.y + bounds.size.height * fraction.1;
    cursor_after(host, moved(x, y))
}

#[test]
fn a_size_grip_shows_the_diagonal_of_its_corner() {
    let grip = || SizeGrip::new(Size::new(100.0, 100.0), Size::new(900.0, 900.0), |_| {});
    for (tag, shape) in [
        ("en", CursorShape::ResizeNorthWestSouthEast),
        ("he", CursorShape::ResizeNorthEastSouthWest),
    ] {
        // The 16 pt grip sits at the center of the frame.
        let mut host = host(tag, grip().frame(Frame::size(400.0, 100.0)));
        assert_eq!(cursor_after(&mut host, moved(200.0, 50.0)), shape, "{tag}");
        assert_eq!(
            cursor_after(&mut host, moved(20.0, 20.0)),
            CursorShape::Arrow
        );
    }
}

#[test]
fn buttons_inside_a_field_show_the_arrow_not_the_text_cursor() {
    let copies = Binding::new(|| 1, |_| {});
    let spin = SpinBox::new("Copies", copies).frame(Frame::size(200.0, 32.0));
    let mut host = host("en", spin);
    assert_eq!(
        cursor_over(&mut host, "Copies", (0.3, 0.5)),
        CursorShape::IBeam
    );
    assert_eq!(
        cursor_over(&mut host, "Copies", (0.97, 0.3)),
        CursorShape::Arrow,
        "stepper"
    );

    let text = Binding::new(String::new, |_| {});
    let addons = vec![InputGroupAddon::button(
        AddonAlign::InlineEnd,
        "Go",
        None,
        ControlSize::Xs,
    )];
    let group = InputGroup::new("Search", addons, text).frame(Frame::size(200.0, 32.0));
    let mut host = self::host("en", group);
    assert_eq!(
        cursor_over(&mut host, "Search", (0.3, 0.5)),
        CursorShape::IBeam
    );
    assert_eq!(
        cursor_over(&mut host, "Search", (0.92, 0.5)),
        CursorShape::Arrow,
        "button"
    );
}

#[test]
fn a_header_column_edge_shows_the_resize_cursor() {
    let column = |title: &str| HeaderColumn {
        title: title.into(),
        width: 120.0,
        min_width: 40.0,
    };
    let sort = Binding::new(|| None, |_| {});
    let header = HeaderView::new(vec![column("Name"), column("Size")], sort);
    let mut host = host("en", header.frame(Frame::size(400.0, 100.0)));
    let node = host.find("Name").expect("the header");
    let bounds = host.tree().bounds(node).unwrap();
    let y = bounds.center().y;
    let edge = bounds.origin.x + 120.0;
    assert_eq!(
        cursor_after(&mut host, moved(edge + 2.0, y)),
        CursorShape::ResizeLeftRight
    );
    assert_eq!(
        cursor_after(&mut host, moved(edge - 60.0, y)),
        CursorShape::Arrow
    );
}
