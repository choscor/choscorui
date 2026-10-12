//! Navigation components mounted through `ControlHost`: breadcrumbs,
//! pagination, the navigation menu, and the sidebar.

use crate::page::{Env, Page};
use crate::pages::hosted::{HostedPage, Row};
use choscorui::controls::{
    Breadcrumb, CustomControl, NavigationLink, NavigationMenu, Pagination, SheetSide, Sidebar,
    SidebarCollapse, SidebarItem, SidebarVariant,
};
use choscorui::core::{Icon, NodeId, Tree};

fn boxed(control: impl CustomControl + 'static) -> Box<dyn CustomControl> {
    Box::new(control)
}

fn trail(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

pub fn breadcrumb(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "Links",
            vec![boxed(Breadcrumb::new(
                "Location",
                trail(&["Home", "Components", "Breadcrumb"]),
            ))],
        ),
        (
            "Collapsed",
            vec![boxed(
                Breadcrumb::new(
                    "Collapsed location",
                    trail(&[
                        "Home",
                        "Documentation",
                        "Building Your App",
                        "Components",
                        "Breadcrumb",
                    ]),
                )
                .with_collapsed(1..4),
            )],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn pagination(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "Few pages",
            vec![boxed(Pagination::new("Results", "Previous", "Next", 3, 1))],
        ),
        (
            "Many pages",
            vec![boxed(Pagination::new("Archive", "Previous", "Next", 20, 9))],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn navigation_menu(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let menu = NavigationMenu::new(
        "Site",
        vec![
            (
                "Getting started".to_string(),
                vec![
                    NavigationLink::new(
                        "Introduction",
                        "Re-usable components built with design tokens.",
                    ),
                    NavigationLink::new(
                        "Installation",
                        "How to install dependencies and structure your app.",
                    ),
                    NavigationLink::new(
                        "Typography",
                        "Styles for headings, paragraphs, and lists.",
                    ),
                ],
            ),
            (
                "Components".to_string(),
                vec![
                    NavigationLink::new(
                        "Alert Dialog",
                        "A modal dialog that interrupts the user with important content.",
                    ),
                    NavigationLink::new(
                        "Hover Card",
                        "For sighted users to preview content behind a link.",
                    ),
                    NavigationLink::new(
                        "Progress",
                        "Shows how far a task has come, as a progress bar.",
                    ),
                    NavigationLink::new(
                        "Scroll-area",
                        "Visually or semantically separates content.",
                    ),
                ],
            ),
            ("Docs".to_string(), Vec::new()),
        ],
    );
    let rows: Vec<Row> = vec![("Menu", vec![boxed(menu)])];
    HostedPage::build(tree, parent, rows)
}

fn sidebar_groups() -> Vec<(String, Vec<SidebarItem>)> {
    vec![
        (
            "Application".to_string(),
            vec![
                SidebarItem::new("Home", Icon::House),
                SidebarItem::new("Inbox", Icon::Inbox).with_badge("24"),
                SidebarItem::new("Calendar", Icon::Calendar),
                SidebarItem::new("Search", Icon::Search),
            ],
        ),
        (
            "Settings".to_string(),
            vec![SidebarItem::new("Settings", Icon::Settings)],
        ),
    ]
}

pub fn sidebar(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let make = |name: &str, variant, collapse| {
        boxed(
            Sidebar::new(name, "Toggle sidebar", sidebar_groups())
                .with_variant(variant)
                .with_collapse(collapse)
                .with_selected(1),
        )
    };
    let rows: Vec<Row> = vec![
        (
            "Sidebar: off-canvas, icons, fixed",
            vec![
                make(
                    "Off-canvas sidebar",
                    SidebarVariant::Sidebar,
                    SidebarCollapse::Offcanvas,
                ),
                make(
                    "Icon sidebar",
                    SidebarVariant::Sidebar,
                    SidebarCollapse::Icon,
                ),
                make(
                    "Fixed sidebar",
                    SidebarVariant::Sidebar,
                    SidebarCollapse::None,
                ),
            ],
        ),
        (
            "Floating and inset",
            vec![
                make(
                    "Floating sidebar",
                    SidebarVariant::Floating,
                    SidebarCollapse::Icon,
                ),
                make(
                    "Inset sidebar",
                    SidebarVariant::Inset,
                    SidebarCollapse::Icon,
                ),
            ],
        ),
        (
            "Right side",
            vec![boxed(
                Sidebar::new("Right sidebar", "Toggle sidebar", sidebar_groups())
                    .with_side(SheetSide::Right),
            )],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

#[cfg(test)]
mod tests {
    use crate::pages::hosted::HostedPage;
    use crate::pages::tests::Harness;
    use choscorui::core::{Key, SemanticAction, SemanticRole};

    #[test]
    fn pagination_page_moves_with_assistive_actions() {
        let mut h = Harness::new("pagination");
        let node = h.node("Archive", Some(SemanticRole::Group));
        assert!(h.act(node, SemanticAction::Increment));
        let page = h.get::<HostedPage>();
        let value = page.host.get(node).unwrap().semantics().value;
        assert_eq!(value.as_deref(), Some("11"));
    }

    #[test]
    fn breadcrumb_takes_keys_while_focused() {
        let mut h = Harness::new("breadcrumb");
        let node = h.node("Collapsed location", Some(SemanticRole::Group));
        assert!(h.act(node, SemanticAction::Focus));
        assert!(h.press(Key::Tab));
        assert!(h.press(Key::Enter));
        let page = h.get::<HostedPage>();
        let expanded = page.host.get(node).unwrap().semantics().state.expanded;
        assert_eq!(expanded, Some(true));
    }
}
