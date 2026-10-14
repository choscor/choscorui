//! The semantics scenario: controls with large models publish their
//! parts. A key step and the semantics read after it must not scale with
//! the model, and the published entries stay bounded by the view.

use crate::scenarios::ms;
use crate::stats::Results;
use choscorui_compose::Runtime;
use choscorui_compose::view::{AnyView, Command, Configured, Cx, Env, List, View, ViewHost};
use choscorui_controls::{CanvasView, CommandItem, ControlKind, ShapedText, ShortcutModifiers};
use choscorui_core::{
    Catalog, Color, DesignTokens, ItemShape, ItemStyle, Key, KeyEvent, Locale, Rect, Scene,
    SceneItem, SemanticAction, SemanticRole, Size, StringListModel,
};
use choscorui_macos::CoreTextShaper;
use std::rc::Rc;
use std::time::Instant;

static SHAPER: CoreTextShaper = CoreTextShaper;

/// The commands, list rows, and canvas items of the large models.
const COMMANDS: usize = 10_000;
const ROWS: usize = 100_000;
const ITEMS: usize = 20_000;
/// The key steps that each case times.
const STEPS: usize = 60;

fn mount(size: Size, view: AnyView) -> ViewHost {
    let env = Env::new(
        Rc::new(Catalog::new()),
        Locale::parse("en").expect("valid tag"),
    );
    let mut host = ViewHost::new(
        size,
        DesignTokens::light(),
        env,
        Runtime::new(),
        Rc::new(ShapedText::new(&SHAPER)),
    );
    host.mount(view);
    host.layout();
    host
}

/// The name of the focused part of `node`.
fn focused_part(host: &mut ViewHost, node: choscorui_core::NodeId) -> Option<String> {
    let (snapshot, _) = host.semantics();
    snapshot
        .into_iter()
        .find(|entry| entry.id == node && entry.part.is_some() && entry.focused)
        .map(|entry| entry.node.name)
}

/// Focuses the node named `name`, then presses `key` `STEPS` times and
/// reads the semantics after each press. Records the step times, the
/// most entries that one read published, and how many steps moved the
/// focused part, as `semantics.<case>_*`.
fn run(results: &mut Results, case: &str, mut host: ViewHost, name: &str, key: Key) {
    let node = host.find(name).expect("the control is mounted");
    host.perform(node, SemanticAction::Focus, None);
    let none = ShortcutModifiers::default();
    let (mut times, mut entries, mut moved) = (Vec::new(), 0, 0);
    let mut focused = focused_part(&mut host, node);
    for _ in 0..STEPS {
        let started = Instant::now();
        host.key(KeyEvent::Down { key, repeat: false }, none);
        host.key(KeyEvent::Up { key }, none);
        let (snapshot, _) = host.semantics();
        times.push(ms(started.elapsed()));
        entries = entries.max(snapshot.len());
        let now = focused_part(&mut host, node);
        moved += usize::from(now.is_some() && now != focused);
        focused = now;
    }
    results.samples(&format!("semantics.{case}_ms"), times);
    results.metric(format!("semantics.{case}_entries"), entries as f64);
    results.metric(format!("semantics.{case}_moved"), moved as f64);
    let errors = host.take_errors().len();
    results.metric(format!("semantics.{case}_errors"), errors as f64);
}

/// A list of `ROWS` rows that owns its selection.
struct Rows;

impl View for Rows {
    fn body(&self, cx: &mut Cx) -> impl View {
        let selection = cx.state(Vec::new());
        let model = || {
            let rows = (0..ROWS).map(|index| format!("Row {index}"));
            Box::new(StringListModel::from_strings(rows.collect::<Vec<_>>())) as _
        };
        List::new("Rows", model, selection.binding())
    }
}

/// A canvas of `ITEMS` named squares on a grid.
fn canvas(size: Size) -> AnyView {
    let viewport = Rect::from_xywh(0.0, 0.0, size.width, size.height);
    Configured::custom(ControlKind::Canvas, move |_| {
        let mut scene = Scene::new();
        for index in 0..ITEMS {
            let (x, y) = ((index % 200) as f64 * 40.0, (index / 200) as f64 * 40.0);
            let shape = ItemShape::Rect(Rect::from_xywh(x, y, 28.0, 28.0));
            let style = ItemStyle::filled(Color::rgba8(37, 99, 235, 255));
            let item =
                SceneItem::new(shape, style).named(format!("Node {index}"), SemanticRole::Button);
            scene.add(item).expect("a valid item");
        }
        CanvasView::new("Canvas", viewport).hosted(scene)
    })
    .into_any()
}

pub fn semantics(size: Size, results: &mut Results) {
    let commands: Vec<_> = (0..COMMANDS)
        .map(|index| CommandItem::new(format!("Command {index}")))
        .collect();
    let command = Command::new("Commands", "Search", vec![("All".to_owned(), commands)]);
    run(
        results,
        "command",
        mount(size, command.into_any()),
        "Commands",
        Key::ArrowDown,
    );

    run(
        results,
        "list",
        mount(size, Rows.into_any()),
        "Rows",
        Key::ArrowDown,
    );

    run(
        results,
        "canvas",
        mount(size, canvas(size)),
        "Canvas",
        Key::Tab,
    );
}
