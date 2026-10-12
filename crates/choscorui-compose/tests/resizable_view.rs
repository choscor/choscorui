//! Headless tests of the resizable panel group view: pane placement, the
//! drag of one handle, the binding in both directions, the handle nodes
//! and their keys, actions, and semantics, the two-pane slider, and
//! right-to-left mirroring.

use choscorui_compose::view::{Binding, Cx, Env, Frame, Resizable, Text, View, ViewExt, ViewHost};
use choscorui_compose::{Property, Runtime};
use choscorui_controls::{EstimatedText, ShortcutModifiers, SplitOrientation};
use choscorui_core::{
    Catalog, DesignTokens, Key, KeyEvent, Locale, NodeId, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticRole, Size,
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

/// Two panes, at least 50 pt each, in a 400 × 100 pt group whose sizes
/// live in `sizes`.
struct Pair(Property<Vec<f64>>);

impl View for Pair {
    fn body(&self, _cx: &mut Cx<'_>) -> impl View {
        let (get, set) = (self.0.clone(), self.0.clone());
        let sizes = Binding::new(move || get.get(), move |value| set.set(value));
        Resizable::new(
            "Pair",
            SplitOrientation::Horizontal,
            (Text::new("A"), Text::new("B")),
            sizes,
        )
        .minimums(vec![50.0, 50.0])
        .frame(Frame::size(400.0, 100.0))
    }
}

/// A host 400 × 100 pt wide in the language `tag` that mounts the view
/// that `view` makes from a property of `initial` sizes.
fn host_of<V: View>(
    tag: &str,
    initial: Vec<f64>,
    view: impl FnOnce(Property<Vec<f64>>) -> V,
) -> (ViewHost, Property<Vec<f64>>) {
    let runtime = Runtime::new();
    let sizes = runtime.property(initial);
    let mut host = ViewHost::new(
        Size::new(400.0, 100.0),
        DesignTokens::light(),
        Env::new(Rc::new(Catalog::new()), Locale::parse(tag).unwrap()),
        runtime,
        Rc::new(EstimatedText),
    );
    host.mount(view(sizes.clone()));
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    (host, sizes)
}

/// A host of [`Panels`] in the language `tag`, and the sizes of its
/// group, which start at 100 and 100.
fn host(tag: &str) -> (ViewHost, Property<Vec<f64>>) {
    host_of(tag, vec![100.0, 100.0], Panels)
}

/// The group node of [`Panels`] and its two handle nodes.
fn handles(host: &mut ViewHost) -> (NodeId, [NodeId; 2]) {
    let group = host.find("Panels").unwrap();
    let children = host.tree().children(group).unwrap();
    (group, [children[0], children[1]])
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
}

#[test]
fn each_handle_is_a_slider_node_in_a_group() {
    let (mut host, _) = host("en");
    let (group, [first, second]) = handles(&mut host);
    let semantic = host.semantic(group).unwrap();
    assert_eq!(semantic.role, SemanticRole::Group);
    assert!(semantic.actions.is_empty());
    // Each handle node sits in its handle rectangle and gives the size of
    // the pane before it, with that size's range.
    assert_eq!(
        host.tree().bounds(first).unwrap(),
        Rect::from_xywh(100.0, 0.0, 4.0, 100.0)
    );
    assert_eq!(host.tree().bounds(second).unwrap().min_x(), 204.0);
    for (node, value, range) in [(first, "100", "50–150"), (second, "100", "50–242")] {
        let semantic = host.semantic(node).unwrap();
        assert_eq!(semantic.role, SemanticRole::Slider);
        assert_eq!(semantic.name, "Panels");
        assert_eq!(semantic.value.as_deref(), Some(value));
        assert_eq!(semantic.description, range);
        assert_eq!(
            semantic.actions,
            vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus
            ]
        );
    }
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
fn tab_visits_each_handle_and_the_keys_move_the_focused_one() {
    let (mut host, sizes) = host("en");
    let (_, [first, second]) = handles(&mut host);
    assert_eq!(host.focus_next(false), Some(first));
    press(&mut host, Key::ArrowRight);
    assert_eq!(sizes.get(), vec![110.0, 90.0]);
    press(&mut host, Key::Tab);
    assert_eq!(host.tree().focus(), Some(second));
    press(&mut host, Key::ArrowLeft);
    assert_eq!(sizes.get(), vec![110.0, 80.0]);
    assert_eq!(widths(&mut host), [110.0, 80.0, 202.0]);
    press(&mut host, Key::Home);
    assert_eq!(sizes.get(), vec![110.0, 50.0]);
    // Shift+Tab goes back to the first handle.
    host.key(
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false,
        },
        ShortcutModifiers {
            shift: true,
            ..ShortcutModifiers::default()
        },
    );
    assert_eq!(host.tree().focus(), Some(first));
}

#[test]
fn a_handle_action_moves_it_and_refreshes_its_neighbor() {
    let (mut host, sizes) = host("en");
    let (_, [first, second]) = handles(&mut host);
    assert!(host.perform(second, SemanticAction::Increment, None));
    assert_eq!(sizes.get(), vec![100.0, 110.0]);
    assert!(host.perform(first, SemanticAction::Decrement, None));
    host.layout();
    assert_eq!(sizes.get(), vec![90.0, 120.0]);
    // The move of the first handle widened the pane before the second.
    let semantic = host.semantic(second).unwrap();
    assert_eq!(semantic.value.as_deref(), Some("120"));
    assert_eq!(semantic.description, "50–252");
    assert_eq!(host.semantic(first).unwrap().description, "50–160");
    // A change of the binding also refreshes the handles.
    sizes.set(vec![60.0, 200.0]);
    host.flush();
    assert_eq!(host.semantic(second).unwrap().value.as_deref(), Some("200"));
}

#[test]
fn two_panes_keep_one_slider_node() {
    let (mut host, sizes) = host_of("en", vec![100.0], Pair);
    let group = host.find("Pair").unwrap();
    let semantic = host.semantic(group).unwrap();
    assert_eq!(semantic.role, SemanticRole::Slider);
    assert_eq!(semantic.value.as_deref(), Some("100"));
    assert_eq!(semantic.description, "");
    // The group node has the two panes and no handle nodes.
    assert_eq!(host.tree().children(group).unwrap().len(), 2);
    assert_eq!(host.focus_next(false), Some(group));
    press(&mut host, Key::ArrowRight);
    assert_eq!(sizes.get(), vec![110.0]);
    drag(&mut host, 112.0, 162.0);
    assert_eq!(sizes.get(), vec![160.0]);
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
