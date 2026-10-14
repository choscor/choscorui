//! Sidebar through the control host.

mod hosted;

use choscorui_controls::{
    ControlMessage, ControlSize, ControlValue, Input, ShortcutModifiers, Sidebar, SidebarCollapse,
    SidebarItem, SidebarItemVariant, SidebarVariant, TOOLTIP_DELAY,
};
use choscorui_core::{
    ColorRole, DesignTokens, Icon, Key, KeyEvent, LayoutDirection, Rect, SemanticAction,
};
use hosted::{Harness, rounded_fills, texts};

fn sidebar() -> Sidebar {
    Sidebar::new(
        "Navigation",
        "Toggle sidebar",
        vec![
            (
                "Application".to_string(),
                vec![
                    SidebarItem::new("Inbox", Icon::Inbox).with_badge("24"),
                    SidebarItem::new("Calendar", Icon::Calendar),
                    SidebarItem::new("Search", Icon::Search),
                ],
            ),
            (
                "Settings".to_string(),
                vec![SidebarItem::new("Settings", Icon::Settings)],
            ),
        ],
    )
}

fn relayout(h: &mut Harness) {
    let size = h.measure();
    h.place(Rect::from_xywh(0.0, 0.0, size.width, size.height));
}

#[test]
fn items_rove_and_select_and_command_b_collapses() {
    let mut h = Harness::new(sidebar());
    assert_eq!(h.measure().width, 256.0);
    let painted = texts(&h.paint());
    for text in ["Application", "Inbox", "24", "Settings"] {
        assert!(painted.contains(&text.to_string()), "{text}");
    }
    h.focus();
    assert_eq!(h.focused_part(), "Toggle sidebar");
    h.key(Key::ArrowDown);
    assert_eq!(h.focused_part(), "Inbox");
    h.key(Key::End);
    assert_eq!(h.focused_part(), "Settings");
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Index(Some(3)))]
    );
    assert_eq!(h.semantics().value.as_deref(), Some("Settings"));
    h.modifiers(ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    });
    h.key(Key::Character('b'));
    h.modifiers(ShortcutModifiers::default());
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(false)]);
    relayout(&mut h);
    assert_eq!(h.measure().width, 48.0, "only the toggle stays");
    assert!(texts(&h.paint()).is_empty());
    assert!(h.perform(SemanticAction::Expand, None));
    relayout(&mut h);
    assert_eq!(h.measure().width, 256.0);
}

fn command_b(h: &mut Harness) {
    h.modifiers(ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    });
    h.key(Key::Character('b'));
    h.modifiers(ShortcutModifiers::default());
}

#[test]
fn command_b_toggles_the_sidebar_from_anywhere_in_the_window() {
    let mut h = Harness::new(sidebar());
    let root = h.tree.root();
    let input = h
        .host
        .mount(&mut h.tree, root, Box::new(Input::new("Search", "")))
        .unwrap();

    // No focus: the window-wide shortcut reaches the sidebar.
    command_b(&mut h);
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(false)]);

    // Focus in a text input that does not use Command+B.
    h.host.move_focus(&mut h.tree, Some(input));
    command_b(&mut h);
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(true)]);
    assert_eq!(h.tree.focus(), Some(input), "focus stays in the input");

    // Focus in the sidebar toggles it once.
    h.focus();
    command_b(&mut h);
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(false)]);

    // A disabled sidebar ignores the shortcut.
    h.host.move_focus(&mut h.tree, Some(input));
    h.set_enabled(false);
    command_b(&mut h);
    assert!(h.messages().is_empty());
}

#[test]
fn a_sidebar_that_never_collapses_leaves_command_b_to_others() {
    let mut h = Harness::new(sidebar().with_collapse(SidebarCollapse::None));
    h.modifiers(ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    });
    let used = h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Character('b'),
            repeat: false,
        },
    );
    assert!(!used, "the key stays unused");
    assert!(h.messages().is_empty());
}

#[test]
fn an_icon_sidebar_keeps_its_icons_when_collapsed() {
    let mut h = Harness::new(sidebar().with_collapse(SidebarCollapse::Icon));
    h.focus();
    h.key(Key::Enter);
    relayout(&mut h);
    assert_eq!(h.measure().width, 48.0);
    assert_eq!(h.semantics().state.expanded, Some(false));
    h.key(Key::ArrowDown);
    assert_eq!(h.focused_part(), "Inbox", "items stay reachable");
    assert!(texts(&h.paint()).is_empty(), "labels hide");

    let mut fixed = Harness::new(sidebar().with_collapse(SidebarCollapse::None));
    fixed.focus();
    assert_eq!(fixed.focused_part(), "Inbox", "no toggle");
    assert!(!fixed.perform(SemanticAction::Collapse, None));
}

#[test]
fn a_narrow_window_shows_the_sidebar_as_a_sheet() {
    let mut h = Harness::new(sidebar().with_selected(1));
    h.tree
        .set_bounds(h.tree.root(), Rect::from_xywh(0.0, 0.0, 600.0, 600.0))
        .unwrap();
    relayout(&mut h);
    assert_eq!(h.measure().width, 48.0);
    assert_eq!(h.semantics().state.expanded, Some(false));
    h.focus();
    h.key(Key::Enter);
    assert_eq!(h.host.modal(&h.tree), Some(h.node));
    assert_eq!(
        h.host.get(h.node).unwrap().overlay(),
        Some(Rect::from_xywh(0.0, 0.0, 600.0, 600.0))
    );
    let tab = |h: &mut Harness| {
        h.host.dispatch_key(
            &mut h.tree,
            KeyEvent::Down {
                key: Key::Tab,
                repeat: false,
            },
        )
    };
    for _ in 0..5 {
        assert!(tab(&mut h), "focus stays in the sheet");
    }
    assert_eq!(h.focused_part(), "Toggle sidebar", "it wraps");
    h.key(Key::Escape);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(true),
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed,
        ]
    );
    assert_eq!(h.host.modal(&h.tree), None);
}

#[test]
fn floating_items_keep_the_default_look_and_outline_items_take_theirs() {
    let background = DesignTokens::light().color(ColorRole::Background);
    let floating = Harness::new(sidebar().with_variant(SidebarVariant::Floating));
    assert!(
        rounded_fills(&floating.paint(), background).is_empty(),
        "a floating sidebar paints no outline items"
    );
    let outline = Harness::new(Sidebar::new(
        "Navigation",
        "Toggle sidebar",
        vec![(
            String::new(),
            vec![SidebarItem::new("Inbox", Icon::Inbox).with_variant(SidebarItemVariant::Outline)],
        )],
    ));
    assert_eq!(rounded_fills(&outline.paint(), background).len(), 1);
}

#[test]
fn the_menu_size_sets_the_item_height() {
    let height = |size| {
        Harness::new(sidebar().with_menu_size(size))
            .measure()
            .height
    };
    let default = height(ControlSize::Default);
    // Four items: 28 pt each when small, 48 pt when large.
    assert_eq!(height(ControlSize::Sm), default - 4.0 * 4.0);
    assert_eq!(height(ControlSize::Lg), default + 4.0 * 16.0);
    assert_eq!(
        height(ControlSize::Icon),
        default,
        "other sizes take default"
    );
}

fn with_menu() -> Sidebar {
    Sidebar::new(
        "Navigation",
        "Toggle sidebar",
        vec![(
            "Application".to_string(),
            vec![
                SidebarItem::new("Inbox", Icon::Inbox),
                SidebarItem::new("Projects", Icon::File).with_children(vec![
                    SidebarItem::new("Design", Icon::File),
                    SidebarItem::new("Sales", Icon::File),
                ]),
            ],
        )],
    )
}

#[test]
fn arrows_open_and_close_a_sub_menu_and_move_into_it() {
    let mut h = Harness::new(with_menu());
    let closed = h.measure().height;
    assert!(!texts(&h.paint()).contains(&"Design".to_string()));
    h.focus();
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    let node = h.semantics();
    assert_eq!(h.focused_part(), "Projects");
    assert_eq!(node.state.expanded, Some(false), "the sub-menu state");
    assert!(node.actions.contains(&SemanticAction::Expand));

    h.key(Key::ArrowRight);
    relayout(&mut h);
    assert_eq!(h.semantics().state.expanded, Some(true));
    // Two 28 pt sub-items with a 4 pt gap, 2 pt padding above and below,
    // and the 4 pt group gap after the box.
    assert_eq!(h.measure().height, closed + 28.0 * 2.0 + 4.0 + 4.0 + 4.0);
    assert!(texts(&h.paint()).contains(&"Design".to_string()));
    h.key(Key::ArrowRight);
    assert_eq!(h.focused_part(), "Design");
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Index(Some(3)))]
    );
    assert_eq!(h.semantics().value.as_deref(), Some("Sales"));
    h.key(Key::ArrowLeft);
    assert_eq!(h.focused_part(), "Projects", "back to the item");
    h.key(Key::ArrowLeft);
    relayout(&mut h);
    assert_eq!(h.measure().height, closed);
    assert!(h.messages().is_empty(), "sub-menu changes emit nothing");

    // Enter and the semantic actions toggle the focused sub-menu; the
    // sidebar stays expanded.
    h.key(Key::Enter);
    relayout(&mut h);
    assert_eq!(h.semantics().state.expanded, Some(true));
    assert!(h.perform(SemanticAction::Collapse, None));
    relayout(&mut h);
    assert_eq!(h.measure().height, closed);
    assert_eq!(h.measure().width, 256.0);
}

#[test]
fn right_to_left_arrows_mirror_and_values_reach_into_sub_menus() {
    let mut h = Harness::with_direction(with_menu(), LayoutDirection::Rtl);
    let closed = h.measure().height;
    h.focus();
    h.key(Key::End);
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().state.expanded, Some(false));
    h.key(Key::ArrowLeft);
    relayout(&mut h);
    assert_eq!(h.semantics().state.expanded, Some(true));
    assert!(h.measure().height > closed);

    let mut h = Harness::new(with_menu());
    let node = h.node;
    let one = ControlValue::Index(Some(1));
    assert!(
        !h.host.set_value(&mut h.tree, node, &one),
        "an item with a sub-menu is not a value"
    );
    let two = ControlValue::Index(Some(2));
    assert!(h.host.set_value(&mut h.tree, node, &two));
    relayout(&mut h);
    assert_eq!(h.semantics().value.as_deref(), Some("Design"));
    assert!(h.measure().height > closed, "the sub-menu opens");
}

#[test]
fn a_sub_menu_item_on_the_icon_rail_expands_the_sidebar() {
    let mut h = Harness::new(with_menu().with_collapse(SidebarCollapse::Icon));
    h.focus();
    h.key(Key::Enter);
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(false)]);
    relayout(&mut h);
    assert_eq!(h.measure().width, 48.0);
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    assert_eq!(h.focused_part(), "Projects");
    h.key(Key::ArrowRight);
    assert!(h.messages().is_empty(), "no sub-menu on the rail");
    h.key(Key::Enter);
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(true)]);
    relayout(&mut h);
    assert_eq!(h.measure().width, 256.0);
    assert!(texts(&h.paint()).contains(&"Design".to_string()));
    assert_eq!(h.focused_part(), "Projects", "focus stays on it");
}

#[test]
fn command_b_on_a_fixed_focused_sidebar_stays_unused() {
    let mut h = Harness::new(sidebar().with_collapse(SidebarCollapse::None));
    h.focus();
    h.modifiers(ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    });
    let used = h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Character('b'),
            repeat: false,
        },
    );
    assert!(!used);
}

fn accent() -> choscorui_core::Color {
    DesignTokens::light().color(ColorRole::SidebarAccent)
}

#[test]
fn a_floating_icon_rail_centers_its_items_and_shows_labels_as_tooltips() {
    let mut h = Harness::new(
        sidebar()
            .with_variant(SidebarVariant::Floating)
            .with_collapse(SidebarCollapse::Icon)
            .with_selected(0),
    );
    h.focus();
    h.key(Key::Enter);
    relayout(&mut h);
    assert_eq!(h.measure().width, 64.0, "the rail keeps its padding");
    let bounds = h.bounds();
    let selected = rounded_fills(&h.paint(), accent());
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].center().x, bounds.center().x);
    h.hover(selected[0].center());
    assert!(!texts(&h.paint()).contains(&"Inbox".to_string()));
    assert!(h.host.advance(&mut h.tree, TOOLTIP_DELAY.as_secs_f64()));
    assert!(texts(&h.paint()).contains(&"Inbox".to_string()));
}

#[test]
fn a_closed_sub_menu_marks_the_parent_of_the_selected_item() {
    let mut h = Harness::new(with_menu().with_selected(2));
    assert_eq!(rounded_fills(&h.paint(), accent()).len(), 1, "Design");
    h.focus();
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    h.key(Key::ArrowLeft);
    relayout(&mut h);
    assert_eq!(rounded_fills(&h.paint(), accent()).len(), 1, "Projects");

    let none = Harness::new(with_menu().with_selected(1));
    assert_eq!(none.semantics().value, None, "an item with a sub-menu");
    let none = Harness::new(with_menu().with_selected(9));
    assert_eq!(none.semantics().value, None, "past the items");
}
