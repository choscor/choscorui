//! Pins text that does not fit on one line: controls placed narrower than
//! their labels truncate with an ellipsis that follows the reading order,
//! and tooltips and alerts wrap their text and grow taller
//! (`painted_text.txt`).

use super::{golden, rect};
use choscorui::controls::{
    AlertDialog, ControlHost, ControlValue, CustomControl, DataTable, DialogButton,
    DialogButtonRole, EstimatedText, HostEnv, ListEntry, PaintContext, Select, Sidebar,
    SidebarItem, Styles, VisibleTooltip,
};
use choscorui::core::{
    DesignTokens, DisplayList, Icon, ItemData, LayoutDirection, ListStore, NodeId, Rect, Role,
    Size, ThemeResolver, Tree, Value,
};
use std::fmt::Write;

/// A row of one text column.
struct Cell(&'static str);

impl ItemData for Cell {
    fn data(&self, _column: usize, role: Role) -> Value {
        match role {
            Role::Display | Role::Sort => Value::Text(self.0.into()),
            _ => Value::None,
        }
    }
}

fn long_select() -> Box<dyn CustomControl> {
    Box::new(Select::new(
        "Fruit",
        vec![ListEntry::option(
            "Dragon fruit from the southern provinces",
        )],
    ))
}

fn long_sidebar() -> Box<dyn CustomControl> {
    Box::new(Sidebar::new(
        "Navigation",
        "Toggle sidebar",
        vec![(
            "Application".to_string(),
            vec![
                SidebarItem::new("Notifications from every project", Icon::Inbox).with_badge("128"),
            ],
        )],
    ))
}

fn long_table() -> Box<dyn CustomControl> {
    let mut store = ListStore::new(1);
    store.set_headers(["Email"]);
    store
        .insert(None, 0, vec![Cell("monserrat.de.la.cruz.44@example.com")])
        .unwrap();
    Box::new(DataTable::new(
        "Payments",
        Box::new(store),
        "Previous",
        "Next",
    ))
}

/// Mounts `control` at `bounds`, settles it, and writes its paint.
fn paint_at(
    out: &mut String,
    name: &str,
    control: Box<dyn CustomControl>,
    bounds: Rect,
    direction: LayoutDirection,
    value: Option<ControlValue>,
) {
    let themes = ThemeResolver::new(DesignTokens::light());
    let styles = Styles::new();
    let env = HostEnv::new(&themes, &styles, &EstimatedText).direction(direction);
    let mut tree = Tree::new(rect(0.0, 0.0, 800.0, 600.0));
    let mut host = ControlHost::new();
    let root = tree.root();
    let node: NodeId = host.mount(&mut tree, root, control).unwrap();
    if let Some(value) = value {
        assert!(host.set_value(&mut tree, node, &value), "{name}");
    }
    tree.set_bounds(node, bounds).unwrap();
    host.arrange(&tree, &env);
    host.advance(&mut tree, 0.0);
    host.arrange(&tree, &env);
    let mut list = DisplayList::new();
    host.paint(&tree, &mut list, &env);
    writeln!(out, "# {name} {direction:?}").unwrap();
    for command in list.commands() {
        writeln!(out, "{command:?}").unwrap();
    }
}

#[test]
fn painted_text() {
    let mut out = String::new();
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        paint_at(
            &mut out,
            "select long value",
            long_select(),
            rect(10.0, 10.0, 160.0, 36.0),
            direction,
            Some(ControlValue::Index(Some(0))),
        );
        paint_at(
            &mut out,
            "sidebar long label",
            long_sidebar(),
            rect(10.0, 10.0, 200.0, 160.0),
            direction,
            None,
        );
        paint_at(
            &mut out,
            "data table long cell",
            long_table(),
            rect(10.0, 10.0, 220.0, 240.0),
            direction,
            None,
        );
    }

    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let tip = VisibleTooltip {
        target: Tree::new(rect(0.0, 0.0, 1.0, 1.0)).root(),
        text: "Saves the document, and keeps a copy of each earlier version so that you can go back to it later".into(),
        anchor: rect(300.0, 300.0, 80.0, 24.0),
    };
    let viewport = rect(0.0, 0.0, 800.0, 600.0);
    let placed = tip.place(
        viewport,
        &styles,
        &tokens,
        &EstimatedText,
        LayoutDirection::Ltr,
    );
    writeln!(out, "# tooltip long text\nplaced {:?}", placed.rect).unwrap();
    let mut list = DisplayList::new();
    tip.paint(
        &mut PaintContext::new(&mut list, placed.rect, &tokens, &styles, &EstimatedText),
        placed.placement,
    );
    for command in list.commands() {
        writeln!(out, "{command:?}").unwrap();
    }

    let alert = AlertDialog::new(
        "Delete the project?",
        "This deletes the project, its files, and its history for every member of the team, and you cannot undo it.",
        vec![
            DialogButton::new("Delete", DialogButtonRole::Destructive),
            DialogButton::new("Cancel", DialogButtonRole::Reject),
        ],
    );
    let size = alert.measure(Size::new(800.0, 600.0), &styles, &tokens, &EstimatedText);
    let panel = rect(10.0, 10.0, size.width, size.height);
    writeln!(out, "# alert dialog long message\nsize {size:?}").unwrap();
    let mut list = DisplayList::new();
    alert.paint(&mut PaintContext::new(
        &mut list,
        panel,
        &tokens,
        &styles,
        &EstimatedText,
    ));
    for command in list.commands() {
        writeln!(out, "{command:?}").unwrap();
    }
    golden("painted_text", &out);
}
