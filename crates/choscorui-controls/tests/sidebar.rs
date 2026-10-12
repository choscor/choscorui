//! Sidebar through the control host.

mod hosted;

use choscorui_controls::{
    ControlMessage, ControlValue, Input, ShortcutModifiers, Sidebar, SidebarCollapse, SidebarItem,
};
use choscorui_core::{Icon, Key, KeyEvent, Rect, SemanticAction};
use hosted::{Harness, texts};

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
    assert_eq!(h.semantics().description, "Toggle sidebar");
    h.key(Key::ArrowDown);
    assert_eq!(h.semantics().description, "Inbox");
    h.key(Key::End);
    assert_eq!(h.semantics().description, "Settings");
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
    assert_eq!(h.semantics().description, "Inbox", "items stay reachable");
    assert!(texts(&h.paint()).is_empty(), "labels hide");

    let mut fixed = Harness::new(sidebar().with_collapse(SidebarCollapse::None));
    fixed.focus();
    assert_eq!(fixed.semantics().description, "Inbox", "no toggle");
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
    assert_eq!(h.semantics().description, "Toggle sidebar", "it wraps");
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
