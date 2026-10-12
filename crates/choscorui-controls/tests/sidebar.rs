//! Sidebar through the control host.

mod hosted;

use choscorui_controls::{
    ControlMessage, ControlValue, ShortcutModifiers, Sidebar, SidebarCollapse, SidebarItem,
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
