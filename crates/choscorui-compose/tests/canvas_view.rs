//! Headless tests of `Canvas`: drawing in canvas coordinates, and pan and
//! zoom on a zoomable canvas.

use choscorui_compose::Runtime;
use choscorui_compose::view::{Canvas, Env, Frame, View, ViewExt, ViewHost};
use choscorui_controls::{EstimatedText, ShortcutModifiers};
use choscorui_core::{
    Catalog, Color, DesignTokens, DisplayCommand, Key, KeyEvent, Locale, NodeId, Point,
    PointerButton, PointerEvent, Rect, Size, Space, WheelUnit,
};
use std::cell::Cell;
use std::rc::Rc;

const MARK: Color = Color::rgba8(1, 2, 3, 255);

/// What the draw closure got the last time it ran.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Drawn {
    bounds: Rect,
    size: Size,
    zoom: f64,
}

/// A host with one canvas inside a padded frame, and the record of its
/// last draw.
fn canvas_host(zoomable: bool) -> (ViewHost, NodeId, Rc<Cell<Drawn>>) {
    let drawn: Rc<Cell<Drawn>> = Rc::default();
    let record = drawn.clone();
    let view = Canvas::new(move |ctx, size| {
        record.set(Drawn {
            bounds: ctx.bounds,
            size,
            zoom: ctx.zoom,
        });
        ctx.list
            .fill_rect(Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0)), MARK);
    })
    .label("drawing")
    .zoomable(zoomable)
    .frame(Frame::size(200.0, 100.0))
    .padding(Space::S6);
    let mut host = mounted(view);
    let node = host.find("drawing").unwrap();
    host.paint();
    (host, node, drawn)
}

fn mounted(view: impl View) -> ViewHost {
    let en = Locale::parse("en").unwrap();
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        Env::new(Rc::new(Catalog::new()), en),
        Runtime::new(),
        Rc::new(EstimatedText),
    );
    host.mount(view);
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    host
}

fn command() -> ShortcutModifiers {
    ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    }
}

fn wheel(host: &mut ViewHost, at: Point, delta: Point, modifiers: ShortcutModifiers) {
    host.pointer(
        PointerEvent::Wheel {
            position: at,
            delta,
            unit: WheelUnit::Points,
        },
        modifiers,
    );
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_canvas_draws_in_coordinates_that_start_at_its_corner() {
    let (mut host, node, drawn) = canvas_host(false);
    let bounds = host.tree().bounds(node).unwrap();
    assert_ne!(bounds.origin, Point::new(0.0, 0.0));

    let list = host.paint();
    let commands = list.commands();
    let mark = commands
        .iter()
        .position(
            |command| matches!(command, DisplayCommand::FillRect { color, .. } if *color == MARK),
        )
        .unwrap();
    assert!(matches!(
        commands[mark],
        DisplayCommand::FillRect { rect, .. }
            if rect == Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0))
    ));
    // The last transform before the mark moves canvas (0, 0) to the corner.
    let transform = commands[..mark]
        .iter()
        .rev()
        .find_map(|command| match command {
            DisplayCommand::PushTransform(transform) => Some(*transform),
            _ => None,
        })
        .unwrap();
    assert_eq!(transform.apply(Point::new(0.0, 0.0)), bounds.origin);
    assert!(matches!(commands[mark + 1], DisplayCommand::PopTransform));
    assert_eq!(
        drawn.get(),
        Drawn {
            bounds: Rect::new(Point::new(0.0, 0.0), bounds.size),
            size: bounds.size,
            zoom: 1.0,
        }
    );
}

#[test]
fn a_canvas_that_is_not_zoomable_ignores_input_and_focus() {
    let (mut host, node, drawn) = canvas_host(false);
    let center = host.tree().bounds(node).unwrap().center();
    wheel(&mut host, center, Point::new(0.0, -100.0), command());
    host.paint();

    assert_eq!(drawn.get().zoom, 1.0);
    assert_eq!(host.focus_next(false), None);
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_zoomable_canvas_zooms_with_command_and_the_wheel() {
    let (mut host, node, drawn) = canvas_host(true);
    let bounds = host.tree().bounds(node).unwrap();
    let at = Point::new(bounds.origin.x + 50.0, bounds.origin.y + 20.0);

    wheel(&mut host, at, Point::new(0.0, -100.0), command());
    host.paint();

    let zoomed = drawn.get();
    assert!(close(zoomed.zoom, 1.0_f64.exp()));
    assert_eq!(zoomed.size, bounds.size);
    assert!(close(
        zoomed.bounds.size.width,
        bounds.size.width / zoomed.zoom
    ));
    // The canvas point under the pointer stays under the pointer.
    assert!(close(zoomed.bounds.origin.x + 50.0 / zoomed.zoom, 50.0));
    assert!(close(zoomed.bounds.origin.y + 20.0 / zoomed.zoom, 20.0));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_zoomable_canvas_pans_with_the_wheel_a_drag_and_the_keys() {
    let (mut host, node, drawn) = canvas_host(true);
    let center = host.tree().bounds(node).unwrap().center();

    wheel(
        &mut host,
        center,
        Point::new(30.0, 0.0),
        ShortcutModifiers::default(),
    );
    host.paint();
    assert_eq!(drawn.get().bounds.origin, Point::new(30.0, 0.0));

    // The content follows a drag.
    let modifiers = ShortcutModifiers::default();
    let button = PointerButton::Primary;
    host.pointer(
        PointerEvent::Down {
            position: center,
            button,
        },
        modifiers,
    );
    host.pointer(
        PointerEvent::Move {
            position: Point::new(center.x + 20.0, center.y + 5.0),
        },
        modifiers,
    );
    host.pointer(
        PointerEvent::Up {
            position: Point::new(center.x + 20.0, center.y + 5.0),
            button,
        },
        modifiers,
    );
    host.paint();
    assert_eq!(drawn.get().bounds.origin, Point::new(10.0, -5.0));
    assert_eq!(host.tree().focus(), Some(node));

    let press = |host: &mut ViewHost, key| {
        host.key(KeyEvent::Down { key, repeat: false }, modifiers);
        host.paint();
    };
    press(&mut host, Key::Character('+'));
    assert!(close(drawn.get().zoom, 1.25));
    press(&mut host, Key::Character('0'));
    assert!(close(drawn.get().zoom, 1.0));
    let before = drawn.get().bounds.origin;
    press(&mut host, Key::ArrowDown);
    assert!(close(drawn.get().bounds.origin.y, before.y + 20.0));
    assert_eq!(host.take_errors(), Vec::new());
}
