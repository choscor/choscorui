//! The window chrome of client-side decorations: the title bar requests,
//! its keyboard and semantics, and the frame: its hit test, its margin,
//! and its input area.

mod hosted;

use choscorui_controls::{
    ControlMessage, EstimatedText, PaintContext, ResizeEdge, ShortcutModifiers, Styles, TitleBar,
    TitleBarButton, TitleBarHandle, TitleBarRequest, WindowFrame, WindowRegion,
};
use choscorui_core::{
    DesignTokens, DisplayList, Insets, Key, KeyEvent, LayoutDirection, Point, PointerButton,
    PointerEvent, Rect, SemanticAction, SemanticRole,
};
use hosted::{Harness, rect, texts};

const BAR: Rect = Rect::from_xywh(0.0, 0.0, 400.0, 40.0);

fn bar(direction: LayoutDirection) -> (Harness, TitleBarHandle) {
    let handle = TitleBarHandle::new();
    let mut h = Harness::with_direction(TitleBar::new("Notes", handle.clone()), direction);
    h.place(BAR);
    (h, handle)
}

fn button_rect(button: TitleBarButton, rtl: bool) -> Rect {
    let rects = TitleBar::button_rects(
        BAR,
        &TitleBarButton::ALL,
        rtl,
        &Styles::new(),
        &DesignTokens::light(),
    );
    rects
        .into_iter()
        .find(|(each, _)| *each == button)
        .map(|(_, rect)| rect)
        .unwrap()
}

fn activations(h: &mut Harness) -> usize {
    h.messages()
        .into_iter()
        .filter(|message| *message == ControlMessage::Activated)
        .count()
}

#[test]
fn a_drag_on_the_title_moves_the_window() {
    let (mut h, handle) = bar(LayoutDirection::Ltr);
    let start = Point::new(100.0, 20.0);
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Down {
            position: start,
            button: PointerButton::Primary,
        },
    );
    // A small move is not a drag yet.
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Move {
            position: Point::new(101.0, 20.0),
        },
    );
    assert!(handle.take_requests().is_empty());
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Move {
            position: Point::new(130.0, 25.0),
        },
    );
    assert_eq!(handle.take_requests(), vec![TitleBarRequest::Move]);
    assert_eq!(activations(&mut h), 1);
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Up {
            position: Point::new(130.0, 25.0),
            button: PointerButton::Primary,
        },
    );
    assert!(handle.take_requests().is_empty());
}

#[test]
fn a_double_click_toggles_maximize_and_a_secondary_press_shows_the_menu() {
    let (mut h, handle) = bar(LayoutDirection::Ltr);
    h.double_click(Point::new(100.0, 20.0));
    assert_eq!(
        handle.take_requests(),
        vec![TitleBarRequest::ToggleMaximize]
    );
    let at = Point::new(80.0, 12.0);
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Down {
            position: at,
            button: PointerButton::Secondary,
        },
    );
    assert_eq!(
        handle.take_requests(),
        vec![TitleBarRequest::ShowWindowMenu(at)]
    );
}

#[test]
fn each_window_button_makes_its_request() {
    let (mut h, handle) = bar(LayoutDirection::Ltr);
    for (button, request) in [
        (TitleBarButton::Minimize, TitleBarRequest::Minimize),
        (TitleBarButton::Maximize, TitleBarRequest::ToggleMaximize),
        (TitleBarButton::Close, TitleBarRequest::Close),
    ] {
        h.click(button_rect(button, false).center());
        assert_eq!(handle.take_requests(), vec![request], "{button:?}");
    }
    // A release away from the pressed button does nothing.
    let close = button_rect(TitleBarButton::Close, false).center();
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Down {
            position: close,
            button: PointerButton::Primary,
        },
    );
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Up {
            position: Point::new(100.0, 20.0),
            button: PointerButton::Primary,
        },
    );
    assert!(handle.take_requests().is_empty());
}

#[test]
fn the_buttons_are_tab_stops_that_enter_and_space_press() {
    let (mut h, handle) = bar(LayoutDirection::Ltr);
    h.focus();
    assert_eq!(h.focused_part(), "Minimize");
    h.key(Key::Enter);
    assert_eq!(handle.take_requests(), vec![TitleBarRequest::Minimize]);
    h.key(Key::Tab);
    assert_eq!(h.focused_part(), "Maximize");
    // Space presses on the way down and runs on the release.
    h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Space,
            repeat: false,
        },
    );
    assert!(handle.take_requests().is_empty());
    h.host
        .dispatch_key(&mut h.tree, KeyEvent::Up { key: Key::Space });
    assert_eq!(
        handle.take_requests(),
        vec![TitleBarRequest::ToggleMaximize]
    );
    h.key(Key::Tab);
    assert_eq!(h.focused_part(), "Close");
    let shift = ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    };
    h.chord(shift, Key::F(10));
    assert!(matches!(
        handle.take_requests().as_slice(),
        [TitleBarRequest::ShowWindowMenu(_)]
    ));
}

#[test]
fn the_semantics_name_the_title_and_each_button() {
    let (mut h, handle) = bar(LayoutDirection::Ltr);
    let node = h.semantics();
    assert_eq!(
        (node.role, node.name.as_str()),
        (SemanticRole::Toolbar, "Notes")
    );
    let names: Vec<String> = h
        .parts()
        .parts()
        .iter()
        .map(|part| part.node.name.clone())
        .collect();
    assert_eq!(names, ["Minimize", "Maximize", "Close"]);
    handle.set_maximized(true);
    handle.set_title("Draft");
    let _ = h.host.advance(&mut h.tree, 1.0);
    let names: Vec<String> = h
        .parts()
        .parts()
        .iter()
        .map(|part| part.node.name.clone())
        .collect();
    assert_eq!(names, ["Minimize", "Restore", "Close"]);
    assert!(texts(&h.paint()).contains(&"Draft".to_owned()));
    assert!(h.perform(SemanticAction::ShowMenu, None));
    assert!(matches!(
        handle.take_requests().as_slice(),
        [TitleBarRequest::ShowWindowMenu(_)]
    ));
}

#[test]
fn a_right_to_left_bar_puts_the_buttons_at_the_left() {
    let ltr = button_rect(TitleBarButton::Close, false);
    let rtl = button_rect(TitleBarButton::Close, true);
    assert!(ltr.min_x() > 300.0, "{ltr:?}");
    assert!(rtl.max_x() < 100.0, "{rtl:?}");
    let minimize = button_rect(TitleBarButton::Minimize, true);
    assert!(minimize.min_x() > rtl.max_x());
    let (mut h, handle) = bar(LayoutDirection::Rtl);
    h.click(rtl.center());
    assert_eq!(handle.take_requests(), vec![TitleBarRequest::Close]);
}

#[test]
fn a_dialog_bar_shows_only_its_buttons() {
    let handle = TitleBarHandle::new();
    let control = TitleBar::new("Open", handle).with_buttons(&[TitleBarButton::Close]);
    let mut h = Harness::new(control);
    h.place(BAR);
    let names: Vec<String> = h
        .parts()
        .parts()
        .iter()
        .map(|part| part.node.name.clone())
        .collect();
    assert_eq!(names, ["Close"]);
}

#[test]
fn the_frame_finds_the_region_under_a_point() {
    let window = rect(10.0, 10.0, 400.0, 300.0);
    let title = rect(10.0, 10.0, 400.0, 40.0);
    let close = rect(370.0, 16.0, 28.0, 28.0);
    let frame = WindowFrame::new(window, title, vec![(TitleBarButton::Close, close)])
        .with_resize_edges(6.0, 2.0);
    let region = |x, y| frame.region(Point::new(x, y));
    assert_eq!(region(0.0, 0.0), WindowRegion::Outside);
    assert_eq!(region(5.0, 5.0), WindowRegion::Resize(ResizeEdge::TopLeft));
    assert_eq!(region(200.0, 6.0), WindowRegion::Resize(ResizeEdge::Top));
    assert_eq!(
        region(200.0, 311.0),
        WindowRegion::Resize(ResizeEdge::Bottom)
    );
    assert_eq!(
        region(412.0, 150.0),
        WindowRegion::Resize(ResizeEdge::Right)
    );
    assert_eq!(region(5.0, 150.0), WindowRegion::Resize(ResizeEdge::Left));
    assert_eq!(
        region(414.0, 314.0),
        WindowRegion::Resize(ResizeEdge::BottomRight)
    );
    assert_eq!(
        region(412.0, 12.0),
        WindowRegion::Resize(ResizeEdge::TopRight)
    );
    assert_eq!(
        region(12.0, 308.0),
        WindowRegion::Resize(ResizeEdge::BottomLeft)
    );
    assert_eq!(region(200.0, 30.0), WindowRegion::TitleBar);
    assert_eq!(
        region(384.0, 30.0),
        WindowRegion::Button(TitleBarButton::Close)
    );
    assert_eq!(region(200.0, 200.0), WindowRegion::Client);
    assert_eq!(
        WindowRegion::Resize(ResizeEdge::Left).press_request(),
        Some(TitleBarRequest::Resize(ResizeEdge::Left))
    );
    assert_eq!(WindowRegion::TitleBar.press_request(), None);
    // A maximized window has no resize band.
    let maximized = frame.clone().with_maximized(true);
    assert_eq!(
        maximized.region(Point::new(11.0, 11.0)),
        WindowRegion::TitleBar
    );
    assert_eq!(
        maximized.region(Point::new(5.0, 5.0)),
        WindowRegion::Outside
    );
}

#[test]
fn the_frame_margin_holds_the_shadow_and_the_outer_band() {
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let window = rect(0.0, 0.0, 400.0, 300.0);
    let frame =
        WindowFrame::new(window, rect(0.0, 0.0, 400.0, 40.0), Vec::new()).with_theme_edges(&tokens);
    assert_eq!(frame.outer_edge, tokens.space(2.0));
    assert_eq!(frame.inner_edge, tokens.space(1.0));
    let margin = frame.margin(&styles, &tokens);
    for side in [margin.left, margin.top, margin.right, margin.bottom] {
        assert!(side >= frame.outer_edge, "{margin:?}");
        assert_eq!(side, side.ceil(), "whole points: {margin:?}");
    }
    // The active shadow drops below the frame, so the bottom is widest.
    assert!(margin.bottom > margin.top, "{margin:?}");

    // The paint stays inside the margin.
    let mut list = DisplayList::new();
    let mut cx = PaintContext::new(&mut list, window, &tokens, &styles, &EstimatedText);
    frame.paint(&mut cx, true);
    let surface = Rect::from_xywh(
        -margin.left,
        -margin.top,
        400.0 + margin.left + margin.right,
        300.0 + margin.top + margin.bottom,
    );
    let painted = list
        .draw_regions()
        .into_iter()
        .map(|(_, region)| region)
        .reduce(Rect::union)
        .expect("the frame paints");
    assert!(
        surface.union(painted) == surface,
        "{painted:?} in {surface:?}"
    );

    // The outer band takes input, and the shadow beyond it does not.
    assert_eq!(
        frame.input_area(),
        rect(-8.0, -8.0, 416.0, 316.0),
        "two spacing units around the frame"
    );
    assert_eq!(
        frame.region(Point::new(-3.0, 150.0)),
        WindowRegion::Resize(ResizeEdge::Left)
    );
    assert_eq!(
        frame.region(Point::new(-12.0, 150.0)),
        WindowRegion::Outside
    );

    // A maximized window has no margin, and only the frame takes input.
    let maximized = frame.with_maximized(true);
    assert_eq!(maximized.margin(&styles, &tokens), Insets::default());
    assert_eq!(maximized.input_area(), window);
}
