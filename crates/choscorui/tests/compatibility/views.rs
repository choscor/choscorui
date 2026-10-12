//! Views through the compose `ViewHost`, mounted without a window and
//! measured with `EstimatedText`: the order of layout and paint
//! modifiers, a stack and a form in both directions, and the semantics
//! of a tooltip on a button.

use choscorui::compose::Runtime;
use choscorui::compose::view::{Button, Env, Form, FormRow, HStack, Text, View, ViewExt, ViewHost};
use choscorui::controls::{EstimatedText, ShortcutModifiers, TOOLTIP_DELAY};
use choscorui::core::{
    Catalog, ColorRole, DesignTokens, Locale, NodeId, PointerEvent, SemanticEntry,
    SemanticRelation, Size,
};
use std::fmt::Write;
use std::rc::Rc;

/// A host of `view` in a 400 × 300 window whose locale has the language
/// `tag`.
fn mount(view: impl View, tag: &str) -> ViewHost {
    let locale = Locale::parse(tag).unwrap();
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        Env::new(Rc::new(Catalog::new()), locale),
        Runtime::new(),
        Rc::new(EstimatedText),
    );
    host.mount(view);
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    host
}

/// The semantic role and name of `node`, or `-` when it publishes none.
fn label(entries: &[SemanticEntry], node: NodeId) -> String {
    entries.iter().find(|entry| entry.id == node).map_or_else(
        || "-".to_owned(),
        |entry| format!("{:?} {:?}", entry.node.role, entry.node.name),
    )
}

/// Every node of the tree, depth first and indented by depth, with its
/// bounds and its semantic role and name.
fn layout(out: &mut String, host: &mut ViewHost) {
    let (entries, _) = host.semantics();
    let tree = host.tree();
    let mut stack = vec![(tree.root(), 0)];
    while let Some((node, depth)) = stack.pop() {
        let bounds = tree.bounds(node).unwrap();
        writeln!(
            out,
            "{:indent$}{bounds:?} {}",
            "",
            label(&entries, node),
            indent = depth * 2
        )
        .unwrap();
        let children = tree.children(node).unwrap_or_default();
        stack.extend(children.iter().rev().map(|child| (*child, depth + 1)));
    }
}

/// The display list of `host`, one command per line.
fn paint(out: &mut String, host: &mut ViewHost) {
    for command in host.paint().commands() {
        writeln!(out, "{command:?}").unwrap();
    }
}

fn stack() -> impl View {
    HStack::new((Button::new("First", || {}), Button::new("Second", || {}))).spacing(8.0)
}

fn form() -> impl View {
    Form::new((
        FormRow::new("Name", Button::new("first field", || {})),
        FormRow::new("Email address", Button::new("second field", || {})),
    ))
}

/// Layout and paint of `padding` then `background` and the reverse, and
/// the layout of a stack and a form in both directions.
pub fn record() -> String {
    let mut out = String::new();
    let padded = Text::new("Text")
        .padding(10.0)
        .background(ColorRole::Primary);
    let filled = Text::new("Text")
        .background(ColorRole::Primary)
        .padding(10.0);
    for (name, mut host) in [
        ("padding then background", mount(padded, "en")),
        ("background then padding", mount(filled, "en")),
    ] {
        writeln!(out, "# {name} layout").unwrap();
        layout(&mut out, &mut host);
        writeln!(out, "# {name} paint").unwrap();
        paint(&mut out, &mut host);
    }
    for tag in ["en", "ar"] {
        writeln!(out, "# stack {tag} layout").unwrap();
        layout(&mut out, &mut mount(stack(), tag));
        writeln!(out, "# form {tag} layout").unwrap();
        layout(&mut out, &mut mount(form(), tag));
    }
    out
}

/// The semantics of a button with a tooltip: the button's description,
/// and the `Tooltip` node while the tooltip shows.
pub fn tooltip_semantics(out: &mut String) {
    for tag in ["en", "ar"] {
        let mut host = mount(Button::new("Lưu", || {}).tooltip("Lưu tệp"), tag);
        let button = host.find("Lưu").unwrap();
        let anchor = host.semantic(button).unwrap();
        writeln!(
            out,
            "tooltip on button {tag}: role {:?} name {:?} description {:?} actions {:?}",
            anchor.role, anchor.name, anchor.description, anchor.actions
        )
        .unwrap();
        let center = host.tree().bounds(button).unwrap().center();
        host.advance(1.0);
        host.pointer(
            PointerEvent::Move { position: center },
            ShortcutModifiers::default(),
        );
        host.advance(1.0 + TOOLTIP_DELAY.as_secs_f64());
        host.layout();
        let (entries, _) = host.semantics();
        for entry in entries
            .iter()
            .filter(|entry| entry.node.role == choscorui::core::SemanticRole::Tooltip)
        {
            let relations: Vec<String> = entry
                .node
                .relations
                .iter()
                .map(|relation| match relation {
                    SemanticRelation::DescribedBy(node) => {
                        format!("DescribedBy({})", label(&entries, *node))
                    }
                    other => format!("{other:?}"),
                })
                .collect();
            writeln!(
                out,
                "tooltip shown {tag}: role {:?} name {:?} relations {relations:?}",
                entry.node.role, entry.node.name
            )
            .unwrap();
        }
        assert_eq!(host.take_errors(), Vec::new());
    }
}
