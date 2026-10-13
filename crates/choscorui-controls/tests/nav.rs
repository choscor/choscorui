//! Breadcrumb, Pagination, and NavigationMenu through the control host.

mod hosted;

use choscorui_controls::{
    Breadcrumb, ControlMessage, ControlValue, NAVIGATION_OPEN_DELAY, NavigationLink,
    NavigationMenu, Pagination, ShortcutModifiers,
};
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

fn site() -> NavigationMenu {
    let links = |titles: &[&str]| {
        titles
            .iter()
            .map(|title| NavigationLink::new(*title, "Re-usable components built with tokens."))
            .collect::<Vec<_>>()
    };
    NavigationMenu::new(
        "Site",
        vec![
            (
                "Getting started".to_string(),
                links(&["Introduction", "Installation", "Typography"]),
            ),
            (
                "Components".to_string(),
                links(&["Alert Dialog", "Hover Card", "Progress", "Scroll-area"]),
            ),
            ("Docs".to_string(), Vec::new()),
        ],
    )
}

#[test]
fn navigation_menu_keys_open_panels_and_activate_links() {
    let mut h = Harness::new(site());
    assert_eq!(h.measure().height, 36.0);
    h.focus();
    assert_eq!(h.semantics().value.as_deref(), Some("Getting started"));
    h.key(Key::ArrowDown);
    assert_eq!(h.semantics().value.as_deref(), Some("Introduction"));
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(true),
            ControlMessage::ItemActivated(1),
            ControlMessage::OpenChanged(false),
        ]
    );
    assert_eq!(h.semantics().value.as_deref(), Some("Getting started"));
    h.key(Key::ArrowRight);
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().value.as_deref(), Some("Docs"));
    h.key(Key::Enter);
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(7)]);
}

#[test]
fn navigation_menu_tab_walks_through_the_open_panel() {
    let mut h = Harness::new(site());
    h.focus();
    h.key(Key::ArrowRight);
    h.key(Key::Enter);
    let panel = h.host.get(h.node).unwrap().overlay().expect("open");
    assert!(panel.size.width > 480.0, "four links take two columns");
    for title in ["Alert Dialog", "Hover Card", "Progress", "Scroll-area"] {
        assert!(tab(&mut h));
        assert_eq!(h.semantics().value.as_deref(), Some(title));
    }
    assert!(tab(&mut h));
    assert_eq!(h.semantics().value.as_deref(), Some("Docs"));
    assert_eq!(
        h.semantics().state.expanded,
        Some(false),
        "the panel closed"
    );
    assert!(!tab(&mut h), "Tab leaves after the last trigger");
}

#[test]
fn navigation_menu_opens_on_hover_after_a_delay() {
    let mut h = Harness::new(site());
    let bounds = h.bounds();
    h.hover(Point::new(bounds.origin.x + 10.0, bounds.center().y));
    assert_eq!(h.host.next_deadline(&h.tree), Some(NAVIGATION_OPEN_DELAY));
    assert!(h.host.advance(&mut h.tree, NAVIGATION_OPEN_DELAY));
    assert_eq!(h.semantics().state.expanded, Some(true));
    // Another trigger switches at once while a panel is open.
    let components = Point::new(bounds.center().x, bounds.center().y);
    h.hover(components);
    assert_eq!(h.host.next_deadline(&h.tree), None);
    h.hover(Point::new(790.0, 590.0));
    assert!(h.host.advance(&mut h.tree, 1.0));
    assert_eq!(h.semantics().state.expanded, Some(false));
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(true),
            ControlMessage::OpenChanged(false)
        ]
    );
}

#[test]
fn a_press_on_the_trail_outside_the_ellipsis_menu_only_closes_it() {
    let mut h = Harness::new(trail());
    h.focus();
    assert!(tab(&mut h));
    h.key(Key::Enter);
    h.messages();
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 5.0, bounds.center().y));
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ],
        "the press closes the menu and does not activate Home"
    );
    h.click(Point::new(bounds.origin.x + 5.0, bounds.center().y));
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(0)]);
}

#[test]
fn a_navigation_panel_stays_in_the_window_and_disabling_closes_it() {
    let mut h = Harness::new(site());
    let size = h.bounds().size;
    h.place(hosted::rect(0.0, 540.0, size.width, size.height));
    h.focus();
    h.key(Key::ArrowDown);
    let panel = h.host.get(h.node).unwrap().overlay().expect("open");
    assert!(
        panel.max_y() <= 540.0,
        "near the bottom edge it opens above"
    );
    h.messages();
    h.set_enabled(false);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
}
