//! Breadcrumb and Pagination through the control host.

mod hosted;

use choscorui_controls::{Breadcrumb, ControlMessage, ControlValue, Pagination, ShortcutModifiers};
use choscorui_core::{Key, KeyEvent, LayoutDirection, Point, SemanticAction, SemanticRole};
use hosted::{Harness, texts};

fn trail() -> Breadcrumb {
    Breadcrumb::new(
        "Breadcrumb",
        ["Home", "Components", "Docs", "Breadcrumb"]
            .map(String::from)
            .to_vec(),
    )
    .with_collapsed(1..3)
}

fn tab(h: &mut Harness) -> bool {
    h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false,
        },
    )
}

#[test]
fn breadcrumb_links_are_tab_stops_and_the_ellipsis_opens_a_menu() {
    let mut h = Harness::new(trail());
    assert_eq!(h.measure().height, 20.0);
    assert_eq!(texts(&h.paint()), ["Home", "Breadcrumb"]);
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::Group);
    assert_eq!(
        node.value.as_deref(),
        Some("Breadcrumb"),
        "the current page"
    );
    h.focus();
    assert_eq!(h.semantics().description, "Home");
    h.key(Key::Enter);
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(0)]);
    assert!(tab(&mut h));
    assert_eq!(h.semantics().description, "Components\nDocs");
    h.key(Key::Enter);
    assert_eq!(h.semantics().state.expanded, Some(true));
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(true),
            ControlMessage::ItemActivated(2),
            ControlMessage::OpenChanged(false),
        ]
    );
    assert!(!tab(&mut h), "Tab leaves after the last link");
}

#[test]
fn breadcrumb_clicks_activate_links_and_rtl_mirrors() {
    let mut h = Harness::new(trail());
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 5.0, bounds.center().y));
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(0)]);
    h.click(Point::new(bounds.max_x() - 5.0, bounds.center().y));
    assert!(h.messages().is_empty(), "the current page is not a link");

    let mut rtl = Harness::with_direction(trail(), LayoutDirection::Rtl);
    let bounds = rtl.bounds();
    rtl.click(Point::new(bounds.max_x() - 5.0, bounds.center().y));
    assert_eq!(rtl.messages(), [ControlMessage::ItemActivated(0)]);
}

fn pages(current: usize) -> Pagination {
    Pagination::new("Pages", "Previous", "Next", 10, current)
}

#[test]
fn pagination_collapses_pages_and_moves_with_keys() {
    let mut h = Harness::new(pages(0));
    let painted = texts(&h.paint());
    assert_eq!(
        painted,
        ["Previous", "1", "2", "3", "4", "5", "10", "Next"],
        "seven slots with one ellipsis"
    );
    h.focus();
    assert_eq!(h.semantics().description, "1", "Previous is disabled");
    assert!(tab(&mut h));
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Index(Some(1)))]
    );
    assert_eq!(h.semantics().value.as_deref(), Some("2"));
    assert_eq!(h.semantics().description, "2", "focus stays on the page");
    let mut middle = Harness::new(pages(5));
    assert_eq!(
        texts(&middle.paint()),
        ["Previous", "1", "5", "6", "7", "10", "Next"]
    );
    middle.modifiers(ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    });
    middle.focus();
    assert_eq!(middle.semantics().description, "Next");
    middle.modifiers(ShortcutModifiers::default());
    middle.key(Key::Space);
    assert_eq!(middle.semantics().value.as_deref(), Some("7"));
    assert!(middle.perform(SemanticAction::Decrement, None));
    assert_eq!(middle.semantics().value.as_deref(), Some("6"));
}

#[test]
fn pagination_previous_and_next_follow_the_direction() {
    let mut h = Harness::new(pages(3));
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 5.0, bounds.center().y));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Index(Some(2)))]
    );
    let mut rtl = Harness::with_direction(pages(3), LayoutDirection::Rtl);
    let bounds = rtl.bounds();
    rtl.click(Point::new(bounds.origin.x + 5.0, bounds.center().y));
    assert_eq!(
        rtl.messages(),
        [ControlMessage::Changed(ControlValue::Index(Some(4)))],
        "Next is at the left end"
    );
}
