//! Headless tests of the virtualized views: lazy stacks mount only the
//! items in view, and `List` paints only the rows in view of a large
//! item model.

use choscorui_compose::view::{
    Binding, Button, Cx, Env, HStack, LazyHStack, LazyVStack, List, ScrollView, Text, View,
    ViewExt, ViewHost,
};
use choscorui_compose::{Property, Runtime};
use choscorui_controls::{EstimatedText, ShortcutModifiers};
use choscorui_core::{
    Axis, Catalog, DesignTokens, DisplayCommand, ItemModel, Key, KeyEvent, Locale, Point,
    PointerEvent, Rect, SemanticAction, Size, StringListModel, Tree, WheelUnit,
};
use std::rc::Rc;

/// The window is 400 by 300 points, so 15 rows of 20 points show.
const ROW: f64 = 20.0;

fn env(tag: &str) -> Env {
    Env::new(Rc::new(Catalog::new()), Locale::parse(tag).unwrap())
}

fn host_with(view: impl View, env: Env, runtime: Runtime) -> ViewHost {
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        env,
        runtime,
        Rc::new(EstimatedText),
    );
    host.mount(view);
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    host
}

fn node_count(tree: &Tree) -> usize {
    let mut count = 0;
    let mut stack = vec![tree.root()];
    while let Some(node) = stack.pop() {
        count += 1;
        stack.extend_from_slice(tree.children(node).unwrap_or_default());
    }
    count
}

/// The published names that start with `prefix`, in tree order.
fn shown(host: &mut ViewHost, prefix: &str) -> Vec<String> {
    let (snapshot, _) = host.semantics();
    snapshot
        .into_iter()
        .map(|entry| entry.node.name)
        .filter(|name| name.starts_with(prefix))
        .collect()
}

fn bounds(host: &mut ViewHost, name: &str) -> Rect {
    let (snapshot, _) = host.semantics();
    snapshot
        .into_iter()
        .find(|entry| entry.node.name == name)
        .map(|entry| entry.bounds)
        .unwrap_or_else(|| panic!("{name} is not mounted"))
}

fn wheel(host: &mut ViewHost, delta: f64) {
    host.pointer(
        PointerEvent::Wheel {
            position: Point::new(200.0, 150.0),
            delta: Point::new(0.0, delta),
            unit: WheelUnit::Points,
        },
        ShortcutModifiers::default(),
    );
    host.layout();
}

fn rows(count: &Property<usize>) -> impl View + use<> {
    let count = count.clone();
    ScrollView::new(
        LazyVStack::new(
            move || count.get(),
            ROW,
            |index| Text::new(format!("Row {index}")),
        )
        .spacing(0.0),
    )
}

fn names(range: std::ops::Range<usize>) -> Vec<String> {
    range.map(|index| format!("Row {index}")).collect()
}

#[test]
fn mounted_nodes_stay_bounded_when_the_item_count_grows() {
    let runtime = Runtime::new();
    let count = runtime.property(100_usize);
    let mut host = host_with(rows(&count), env("en"), runtime);
    let small = node_count(host.tree());
    assert_eq!(shown(&mut host, "Row "), names(0..15));
    count.set(100_000);
    host.flush();
    host.layout();
    assert_eq!(node_count(host.tree()), small);
    assert_eq!(shown(&mut host, "Row "), names(0..15));
    // The track is as long as every item, so the scroll view can reach
    // the end of the list.
    wheel(&mut host, 1.0e9);
    assert_eq!(shown(&mut host, "Row "), names(99_985..100_000));
    assert_eq!(node_count(host.tree()), small);
    assert_eq!(bounds(&mut host, "Row 99999").max_y(), 300.0);
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_smaller_count_unmounts_the_items_past_the_end() {
    let runtime = Runtime::new();
    let count = runtime.property(100_usize);
    let mut host = host_with(rows(&count), env("en"), runtime);
    count.set(4);
    host.flush();
    assert_eq!(shown(&mut host, "Row "), names(0..4));
    count.set(0);
    host.flush();
    assert!(shown(&mut host, "Row ").is_empty());
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn scrolling_mounts_the_items_that_come_into_view_and_unmounts_the_rest() {
    let runtime = Runtime::new();
    let count = runtime.property(1_000_usize);
    let mut host = host_with(rows(&count), env("en"), runtime);
    let first = bounds(&mut host, "Row 0");
    assert_eq!((first.min_y(), first.size.height), (0.0, ROW));
    wheel(&mut host, 5.0 * ROW);
    assert_eq!(shown(&mut host, "Row "), names(5..20));
    assert_eq!(bounds(&mut host, "Row 5").min_y(), 0.0);
    // Half a row scrolls row 5 partly out of view; it stays, and row 20
    // comes in at the bottom.
    wheel(&mut host, ROW / 2.0);
    assert_eq!(shown(&mut host, "Row "), names(5..21));
    assert_eq!(bounds(&mut host, "Row 6").min_y(), ROW / 2.0);
    wheel(&mut host, -1.0e9);
    assert_eq!(shown(&mut host, "Row "), names(0..15));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_wheel_at_the_end_of_an_inner_scroll_view_scrolls_the_outer_one() {
    use choscorui_compose::view::{Frame, VStack};
    let inner = ScrollView::new(
        LazyVStack::new(20_usize, ROW, |index| Text::new(format!("Inner {index}"))).spacing(0.0),
    )
    .frame(Frame::new().height(200.0));
    let outer = ScrollView::new(VStack::new((
        inner,
        LazyVStack::new(40_usize, ROW, |index| Text::new(format!("Row {index}"))).spacing(0.0),
    )));
    let mut host = host_with(outer, env("en"), Runtime::new());
    let first = bounds(&mut host, "Row 0").min_y();
    // The inner view takes the wheel until it reaches its end.
    wheel(&mut host, 1.0e9);
    assert_eq!(bounds(&mut host, "Inner 19").max_y(), 200.0);
    assert_eq!(bounds(&mut host, "Row 0").min_y(), first);
    // At its end the inner view returns Continue, so the outer view scrolls.
    let before = bounds(&mut host, "Inner 19").max_y();
    wheel(&mut host, 2.0 * ROW);
    assert_eq!(bounds(&mut host, "Inner 19").max_y(), before - 2.0 * ROW);
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn space_pages_a_focused_scroll_view_down_and_shift_space_pages_it_up() {
    let rows = ScrollView::new(
        LazyVStack::new(100_usize, ROW, |index| Text::new(format!("Row {index}"))).spacing(0.0),
    )
    .focusable(true);
    let mut host = host_with(rows, env("en"), Runtime::new());
    key(&mut host, Key::Tab);
    key(&mut host, Key::Space);
    host.layout();
    let paged = bounds(&mut host, "Row 14").min_y();
    assert!(paged < 14.0 * ROW, "{paged}");
    let shift = ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    };
    host.key(
        KeyEvent::Down {
            key: Key::Space,
            repeat: false,
        },
        shift,
    );
    host.layout();
    assert_eq!(bounds(&mut host, "Row 0").min_y(), 0.0);
    assert_eq!(host.take_errors(), Vec::new());
}

/// A row with its own state: a button that counts its presses.
struct Counted(usize);

impl View for Counted {
    fn body(&self, cx: &mut Cx) -> impl View {
        let presses = cx.state(0);
        let index = self.0;
        Button::new(
            move || format!("Item {index}: {}", presses.get()),
            move || presses.update(|n| *n += 1),
        )
    }
}

#[test]
fn items_that_stay_in_view_keep_their_state() {
    let runtime = Runtime::new();
    let mut host = host_with(
        ScrollView::new(LazyVStack::new(1_000_usize, 40.0, Counted).spacing(0.0)),
        env("en"),
        runtime,
    );
    let item = host.find("Item 3: 0").expect("item 3 shows");
    assert!(host.perform(item, SemanticAction::Activate, None));
    host.layout();
    assert_eq!(host.find("Item 3: 1"), Some(item));
    // Item 3 stays in view, so it keeps its node and its state.
    wheel(&mut host, 80.0);
    assert_eq!(host.find("Item 3: 1"), Some(item));
    // Far away, item 3 unmounts; when it comes back, it mounts again with
    // new state.
    wheel(&mut host, 4_000.0);
    assert!(shown(&mut host, "Item 3:").is_empty());
    wheel(&mut host, -1.0e9);
    assert!(host.find("Item 3: 0").is_some());
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_lazy_hstack_mirrors_in_right_to_left_layouts() {
    let columns = || {
        ScrollView::new(
            LazyHStack::new(100_usize, 50.0, |index| Text::new(format!("Col {index}")))
                .spacing(0.0),
        )
        .axis(Axis::Horizontal)
    };
    let mut ltr = host_with(columns(), env("en"), Runtime::new());
    let mut rtl = host_with(columns(), env("he"), Runtime::new());
    for host in [&mut ltr, &mut rtl] {
        // Eight 50 pt columns fill the 400 pt window.
        assert_eq!(shown(host, "Col ").len(), 8);
    }
    let first = shown(&mut ltr, "Col ");
    assert_eq!(first.first().map(String::as_str), Some("Col 0"));
    assert_eq!(bounds(&mut ltr, "Col 0").min_x(), 0.0);
    assert_eq!(bounds(&mut ltr, "Col 1").min_x(), 50.0);
    // In right-to-left layouts each column sits left of the one before,
    // and the text of a column sits at its leading (right) edge.
    // The view starts at the right edge, with the first column.
    assert_eq!(first_index(&mut rtl), 0);
    assert_eq!(right_edges(&mut rtl, 0, 8), steps(400.0, 8));
    // A scroll by 2.4 columns goes on in the reading order: the content
    // moves to the right, and parts of nine columns show.
    wheel(&mut rtl, 120.0);
    assert_eq!(first_index(&mut rtl), 2);
    assert_eq!(shown(&mut rtl, "Col ").len(), 9);
    assert_eq!(right_edges(&mut rtl, 2, 9), steps(420.0, 9));
    assert_eq!(rtl.take_errors(), Vec::new());
}

#[test]
fn horizontal_scroll_keys_follow_the_reading_order() {
    let columns = || {
        ScrollView::new(
            LazyHStack::new(100_usize, 50.0, |index| Text::new(format!("Col {index}")))
                .spacing(0.0),
        )
        .axis(Axis::Horizontal)
        .focusable(true)
    };
    // The right edge of the text of a middle column, then after each
    // key.
    let edges = |tag: &str, keys: [Key; 2]| {
        let mut host = host_with(columns(), env(tag), Runtime::new());
        key(&mut host, Key::Tab);
        let column = format!("Col {}", first_index(&mut host) + 3);
        let mut edges = vec![bounds(&mut host, &column).max_x()];
        for pressed in keys {
            key(&mut host, pressed);
            host.layout();
            edges.push(bounds(&mut host, &column).max_x());
        }
        edges
    };
    // One line is 40 points. The key toward the end of the reading order
    // scrolls forward, and the other key scrolls back.
    let moves = |edges: Vec<f64>| [edges[1] - edges[0], edges[2] - edges[1]];
    assert_eq!(
        moves(edges("en", [Key::ArrowRight, Key::ArrowLeft])),
        [-40.0, 40.0]
    );
    assert_eq!(
        moves(edges("he", [Key::ArrowLeft, Key::ArrowRight])),
        [40.0, -40.0]
    );
}

#[test]
fn focus_scrolls_a_right_to_left_row_to_the_focused_button() {
    let buttons = (0..20)
        .map(|index| Button::new(format!("Button {index}"), || {}).into_any())
        .collect::<Vec<_>>();
    let row = ScrollView::new(HStack::new(buttons)).axis(Axis::Horizontal);
    let mut host = host_with(row, env("he"), Runtime::new());
    let window = Rect::new(Point::new(0.0, 0.0), Size::new(400.0, 300.0));
    let first = bounds(&mut host, "Button 0");
    assert!(first.max_x() <= 400.0 && first.max_x() > 350.0, "{first:?}");
    for index in 0..20 {
        host.focus_next(false);
        host.layout();
        let focused = bounds(&mut host, &format!("Button {index}"));
        assert!(
            focused.min_x() > window.min_x() - 1e-6 && focused.max_x() < window.max_x() + 1e-6,
            "Button {index} at {focused:?}"
        );
    }
}

/// The smallest index among the mounted columns.
fn first_index(host: &mut ViewHost) -> usize {
    shown(host, "Col ")
        .iter()
        .map(|name| name["Col ".len()..].parse().unwrap())
        .min()
        .unwrap()
}

/// The right edges of `count` columns from `first` on.
fn right_edges(host: &mut ViewHost, first: usize, count: usize) -> Vec<f64> {
    (first..first + count)
        .map(|index| bounds(host, &format!("Col {index}")).max_x())
        .collect()
}

/// `count` edges 50 points apart, from `start` to the left.
fn steps(start: f64, count: usize) -> Vec<f64> {
    (0..count).map(|step| start - 50.0 * step as f64).collect()
}

fn cities(count: usize) -> Box<dyn ItemModel> {
    Box::new(StringListModel::from_strings(
        (0..count).map(|index| format!("City {index}")),
    ))
}

fn key(host: &mut ViewHost, key: Key) {
    host.key(KeyEvent::Down { key, repeat: false }, Default::default());
}

#[test]
fn a_list_binds_its_selection_and_paints_only_the_rows_in_view() {
    let runtime = Runtime::new();
    let selection = runtime.property(Vec::<usize>::new());
    let (get, set) = (selection.clone(), selection.clone());
    let binding = Binding::new(move || get.get(), move |value| set.set(value));
    let mut host = host_with(
        List::new("Cities", || cities(100_000), binding),
        env("en"),
        runtime,
    );
    let list = host.find("Cities").expect("the list is named");
    host.focus(Some(list));
    key(&mut host, Key::ArrowDown);
    assert_eq!(selection.get(), vec![0]);
    selection.set(vec![41]);
    host.flush();
    key(&mut host, Key::ArrowDown);
    assert_eq!(selection.get(), vec![42]);
    key(&mut host, Key::End);
    assert_eq!(selection.get(), vec![99_999]);
    let painted: Vec<String> = host
        .paint()
        .commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(painted.len() <= 15, "painted {} rows", painted.len());
    assert_eq!(painted.last().map(String::as_str), Some("City 99999"));
    // The list is one node, whatever the size of the model.
    assert_eq!(node_count(host.tree()), 2);
    assert_eq!(host.take_errors(), Vec::new());
}
