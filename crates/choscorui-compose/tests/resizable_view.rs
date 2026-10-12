//! Headless tests of the resizable panel group view: pane placement, the
//! drag of one handle, the binding in both directions, the keys, and
//! right-to-left mirroring.

use choscorui_compose::view::{Binding, Cx, Env, Frame, Resizable, Text, View, ViewExt, ViewHost};
use choscorui_compose::{Property, Runtime};
use choscorui_controls::{EstimatedText, ShortcutModifiers, SplitOrientation};
use choscorui_core::{
    Catalog, DesignTokens, Key, KeyEvent, Locale, Point, PointerButton, PointerEvent, Rect,
    SemanticRole, Size,
};
use std::rc::Rc;

/// Three panes, at least 50 pt each, in a 400 × 100 pt group whose sizes
/// live in `sizes`.
struct Panels(Property<Vec<f64>>);

impl View for Panels {
    fn body(&self, _cx: &mut Cx<'_>) -> impl View {
        let (get, set) = (self.0.clone(), self.0.clone());
        let sizes = Binding::new(move || get.get(), move |value| set.set(value));
        Resizable::new(
            "Panels",
            SplitOrientation::Horizontal,
            (Text::new("A"), Text::new("B"), Text::new("C")),
            sizes,
        )
        .minimums(vec![50.0, 50.0, 50.0])
        .grip(true)
        .frame(Frame::size(400.0, 100.0))
    }
}

/// A host 400 × 100 pt wide in the language `tag`, and the sizes of its
/// group, which start at 100 and 100.
fn host(tag: &str) -> (ViewHost, Property<Vec<f64>>) {
    let runtime = Runtime::new();
    let sizes = runtime.property(vec![100.0, 100.0]);
    let mut host = ViewHost::new(
        Size::new(400.0, 100.0),
        DesignTokens::light(),
        Env::new(Rc::new(Catalog::new()), Locale::parse(tag).unwrap()),
        runtime,
        Rc::new(EstimatedText),
    );
    host.mount(Panels(sizes.clone()));
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    (host, sizes)
}

fn bounds(host: &mut ViewHost, name: &str) -> Rect {
    let node = host
        .find(name)
        .unwrap_or_else(|| panic!("no node named {name}"));
    host.tree().bounds(node).unwrap()
}

fn widths(host: &mut ViewHost) -> [f64; 3] {
    ["A", "B", "C"].map(|name| bounds(host, name).size.width)
}

fn pointer(host: &mut ViewHost, event: PointerEvent) {
    host.pointer(event, ShortcutModifiers::default());
    host.layout();
}

/// Drags from `from` to `to` along the middle of the group.
fn drag(host: &mut ViewHost, from: f64, to: f64) {
    let at = |x| Point::new(x, 50.0);
    let button = PointerButton::Primary;
    pointer(
        host,
        PointerEvent::Down {
            position: at(from),
            button,
        },
    );
    pointer(host, PointerEvent::Move { position: at(to) });
    pointer(
        host,
        PointerEvent::Up {
            position: at(to),
            button,
        },
    );
}

fn press(host: &mut ViewHost, key: Key) {
    host.key(
        KeyEvent::Down { key, repeat: false },
        ShortcutModifiers::default(),
    );
    host.layout();
}

#[test]
fn panes_sit_between_the_handles_of_the_recipe() {
    let (mut host, _) = host("en");
    // The handle is the 4 pt hit area (and grip) of the recipe.
    assert_eq!(
        bounds(&mut host, "A"),
        Rect::from_xywh(0.0, 0.0, 100.0, 100.0)
    );
    assert_eq!(bounds(&mut host, "B").min_x(), 104.0);
    assert_eq!(widths(&mut host), [100.0, 100.0, 192.0]);
    let group = host.find("Panels").unwrap();
    let semantic = host.semantic(group).unwrap();
    assert_eq!(semantic.role, SemanticRole::Slider);
    assert_eq!(semantic.value.as_deref(), Some("100"));
}

#[test]
fn a_drag_of_the_second_handle_moves_only_its_neighbors() {
    let (mut host, sizes) = host("en");
    // The second handle is 204..208.
    drag(&mut host, 206.0, 256.0);
    assert_eq!(widths(&mut host), [100.0, 150.0, 142.0]);
    assert_eq!(sizes.get(), vec![100.0, 150.0]);
    // The group keeps the last pane at its minimum.
    drag(&mut host, 256.0, 900.0);
    assert_eq!(widths(&mut host), [100.0, 242.0, 50.0]);
    assert_eq!(sizes.get(), vec![100.0, 242.0]);
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn the_binding_moves_the_panes() {
    let (mut host, sizes) = host("en");
    sizes.set(vec![60.0, 200.0]);
    host.flush();
    host.layout();
    assert_eq!(widths(&mut host), [60.0, 200.0, 132.0]);
    // Sizes that do not fit the group report an error and change nothing.
    sizes.set(vec![10.0, 200.0]);
    host.flush();
    host.layout();
    assert_eq!(widths(&mut host), [60.0, 200.0, 132.0]);
    assert_eq!(host.take_errors().len(), 1);
}

#[test]
fn the_keys_move_the_active_handle() {
    let (mut host, sizes) = host("en");
    let group = host.find("Panels").unwrap();
    host.focus(Some(group));
    press(&mut host, Key::ArrowRight);
    assert_eq!(sizes.get(), vec![110.0, 90.0]);
    press(&mut host, Key::Tab);
    press(&mut host, Key::ArrowLeft);
    assert_eq!(sizes.get(), vec![110.0, 80.0]);
    assert_eq!(widths(&mut host), [110.0, 80.0, 202.0]);
    press(&mut host, Key::Home);
    assert_eq!(sizes.get(), vec![110.0, 50.0]);
}

#[test]
fn a_horizontal_group_mirrors_in_right_to_left_layouts() {
    let (mut host, sizes) = host("ar");
    assert_eq!(bounds(&mut host, "A").max_x(), 400.0);
    assert_eq!(bounds(&mut host, "C").min_x(), 0.0);
    // The second handle is 192..196 from the left; a drag to the left
    // makes the middle pane larger.
    drag(&mut host, 194.0, 144.0);
    assert_eq!(sizes.get(), vec![100.0, 150.0]);
    assert_eq!(widths(&mut host), [100.0, 150.0, 142.0]);
    // The dragged handle stays active, and ArrowLeft moves it to the
    // left, which grows the pane before it.
    press(&mut host, Key::ArrowLeft);
    assert_eq!(sizes.get(), vec![100.0, 160.0]);
}
