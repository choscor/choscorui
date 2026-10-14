//! The hosted toolbar and status bar: the chevron stop, embedded
//! controls, Tab stops, recipe geometry, icons, and actions.

mod hosted;

use choscorui_controls::{
    CommandId, CommandState, ControlMessage, HostedToolbar, StatusBar, Styles, Toolbar, ToolbarItem,
};
use choscorui_core::{
    DesignTokens, Icon, Key, LayoutDirection, NodeId, SemanticAction, ThemeResolver,
};
use hosted::{Harness, rect, texts};
use std::cell::Cell;

const COPY: CommandId = CommandId(1);
const PASTE: CommandId = CommandId(2);
const SHARE: CommandId = CommandId(3);

fn items(search: Option<NodeId>) -> Vec<ToolbarItem> {
    let mut items = vec![
        ToolbarItem::command("copy", "Copy", COPY),
        ToolbarItem::command("paste", "Paste", PASTE),
        ToolbarItem::command("share", "Share", SHARE),
    ];
    if let Some(search) = search {
        items.push(ToolbarItem::control("search", "Search", search, 120.0));
    }
    items
}

/// A hosted toolbar with an embedded control, and the control's node.
fn with_search() -> (Harness, NodeId) {
    let node = Cell::new(None);
    let h = Harness::mount_with(
        |tree| {
            let search = tree.attach(tree.root(), rect(0.0, 0.0, 1.0, 1.0)).unwrap();
            node.set(Some(search));
            Box::new(Toolbar::new("Edit", items(Some(search))).hosted())
        },
        ThemeResolver::new(DesignTokens::light()),
        Styles::new(),
        LayoutDirection::Ltr,
    );
    (h, node.get().unwrap())
}

/// Places `h` 20 pt narrower than its natural width, so that the last
/// items overflow.
fn narrow(h: &mut Harness) {
    let bounds = h.bounds();
    h.place(rect(
        bounds.origin.x,
        bounds.origin.y,
        bounds.size.width - 20.0,
        bounds.size.height,
    ));
}

#[test]
fn the_overflow_chevron_is_the_last_roving_stop_and_keys_open_it() {
    let mut h = Harness::new(Toolbar::new("Edit", items(None)).hosted());
    narrow(&mut h);
    h.focus();
    // End goes to the chevron; Enter opens the menu with its first row
    // highlighted, and Enter runs it.
    h.key(Key::End);
    h.key(Key::Enter);
    assert_eq!(h.messages(), vec![ControlMessage::OpenChanged(true)]);
    h.key(Key::Enter);
    let messages = h.messages();
    assert!(
        matches!(messages[0], ControlMessage::Command(_)),
        "{messages:?}"
    );
    // ArrowDown opens it too, and the chevron wraps to the first command.
    h.key(Key::ArrowDown);
    assert_eq!(h.messages(), vec![ControlMessage::OpenChanged(true)]);
    h.key(Key::Escape);
    h.messages();
    h.key(Key::ArrowRight);
    h.key(Key::Enter);
    assert_eq!(h.messages(), vec![ControlMessage::Command(COPY)]);
}

#[test]
fn embedded_controls_take_real_focus_and_join_the_overflow_menu() {
    let (mut h, search) = with_search();
    h.focus();
    h.key(Key::End);
    assert_eq!(h.tree.focus(), Some(search), "the control gets focus");

    let (mut h, _) = with_search();
    narrow(&mut h);
    h.focus();
    h.key(Key::End);
    h.key(Key::Enter);
    h.key(Key::End);
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::OpenChanged(true),
            ControlMessage::OpenChanged(false),
            ControlMessage::ItemActivated(3),
        ],
        "the last menu row presents the search item"
    );
}

#[test]
fn a_toolbar_with_no_item_that_takes_focus_is_not_a_tab_stop() {
    let h = Harness::new(Toolbar::new("Empty", Vec::new()).hosted());
    assert!(!h.host.get(h.node).unwrap().focusable());
    let mut toolbar: HostedToolbar = Toolbar::new("Edit", items(None)).hosted();
    for command in [COPY, PASTE, SHARE] {
        toolbar
            .set_command(
                command,
                CommandState {
                    enabled: false,
                    checked: false,
                },
            )
            .unwrap();
    }
    let h = Harness::new(toolbar);
    assert!(!h.host.get(h.node).unwrap().focusable());
    let h = Harness::new(Toolbar::new("Edit", items(None)).hosted());
    assert!(h.host.get(h.node).unwrap().focusable());
}

#[test]
fn focus_returns_to_the_last_focused_item() {
    let mut h = Harness::new(Toolbar::new("Edit", items(None)).hosted());
    h.focus();
    h.key(Key::ArrowRight);
    h.host.move_focus(&mut h.tree, None);
    h.focus();
    h.key(Key::Enter);
    assert_eq!(h.messages(), vec![ControlMessage::Command(PASTE)]);
}

#[test]
fn the_bar_takes_its_geometry_from_the_recipe_and_shows_icons() {
    let items = vec![ToolbarItem::command("copy", "Copy", COPY).with_icon(Icon::Copy)];
    let mut h = Harness::new(Toolbar::new("Edit", items).hosted());
    // The bar is 40 pt thick; a button is 32 pt with 4 pt spacing.
    assert_eq!(h.measure().height, 40.0);
    assert_eq!(h.measure().width, 40.0);
    assert!(
        texts(&h.paint()).is_empty(),
        "the icon shows, not the label"
    );
}

#[test]
fn the_status_bar_has_no_focus_action_and_uses_its_recipe_height() {
    let mut bar = StatusBar::new();
    bar.set_default_text("Ready");
    let mut h = Harness::new(bar);
    assert_eq!(h.measure().height, 24.0);
    assert!(h.semantics().actions.is_empty());
    assert!(!h.perform(SemanticAction::Focus, None));
}
