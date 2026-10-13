//! Headless tests of the window cursor: the host shows the cursor of the
//! slot under the pointer, and keeps it during a drag.

use choscorui_compose::Runtime;
use choscorui_compose::view::{
    Badge, Binding, Env, Frame, Resizable, Text, View, ViewExt, ViewHost,
};
use choscorui_controls::{BadgeVariant, EstimatedText, ShortcutModifiers, SplitOrientation};
use choscorui_core::{
    Catalog, CursorShape, DesignTokens, Locale, Point, PointerButton, PointerEvent, Size,
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
    let badge = Badge::new("Docs").with(|badge| badge.with_variant(BadgeVariant::Link));
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
