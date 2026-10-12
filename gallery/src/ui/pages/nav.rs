//! Navigation components: breadcrumbs, pagination, the navigation menu,
//! and the sidebar.

use crate::ui::page::{Build, PageCx, column, section};
use choscorui::compose::view::{
    AnyView, Breadcrumb, Cx, HStack, NavigationMenu, Pagination, Sidebar, View,
};
use choscorui::controls::{
    NavigationLink, SheetSide, SidebarCollapse, SidebarItem, SidebarVariant,
};
use choscorui::core::{Icon, Space};

fn trail(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

pub fn breadcrumb(_: &PageCx) -> AnyView {
    column((
        section(
            "Links",
            Breadcrumb::new("Location", trail(&["Home", "Components", "Breadcrumb"])),
        ),
        section(
            "Collapsed",
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
            .with(|breadcrumb| breadcrumb.with_collapsed(1..4)),
        ),
    ))
}

/// A short and a long run of pages, each on its own page state.
struct PaginationPage;

impl View for PaginationPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let few = cx.state(1);
        let many = cx.state(9);
        column((
            section(
                "Few pages",
                Pagination::new("Results", "Previous", "Next", 3, few.binding()),
            ),
            section(
                "Many pages",
                Pagination::new("Archive", "Previous", "Next", 20, many.binding()),
            ),
        ))
    }
}

pub fn pagination(_: &PageCx) -> AnyView {
    PaginationPage.into_any()
}

fn link(title: &str, description: &str) -> NavigationLink {
    NavigationLink::new(title, description)
}

pub fn navigation_menu(_: &PageCx) -> AnyView {
    let menus = vec![
        (
            "Getting started".to_string(),
            vec![
                link(
                    "Introduction",
                    "Re-usable components built with design tokens.",
                ),
                link(
                    "Installation",
                    "How to install dependencies and structure your app.",
                ),
                link("Typography", "Styles for headings, paragraphs, and lists."),
            ],
        ),
        (
            "Components".to_string(),
            vec![
                link(
                    "Alert Dialog",
                    "A modal dialog that interrupts the user with important content.",
                ),
                link(
                    "Hover Card",
                    "For sighted users to preview content behind a link.",
                ),
                link(
                    "Progress",
                    "Shows how far a task has come, as a progress bar.",
                ),
                link("Scroll-area", "Visually or semantically separates content."),
            ],
        ),
        ("Docs".to_string(), Vec::new()),
    ];
    column(section("Menu", NavigationMenu::new("Site", menus)))
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

/// Sidebars in each variant and collapse mode, and one on the right.
struct SidebarPage;

impl View for SidebarPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let mut make = |name: &str, variant: SidebarVariant, collapse: SidebarCollapse| {
            let selected = cx.state(Some(1));
            Sidebar::new(
                name.to_owned(),
                "Toggle sidebar",
                sidebar_groups(),
                selected.binding(),
            )
            .with(move |sidebar| sidebar.with_variant(variant).with_collapse(collapse))
        };
        let row = |sidebars| HStack::new(sidebars).spacing(Space::S4);
        let first = row(vec![
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
        ]);
        let second = row(vec![
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
        ]);
        let none = cx.state(None);
        let right = Sidebar::new(
            "Right sidebar",
            "Toggle sidebar",
            sidebar_groups(),
            none.binding(),
        )
        .with(|sidebar| sidebar.with_side(SheetSide::Right));
        column((
            section("Sidebar: off-canvas, icons, fixed", first),
            section("Floating and inset", second),
            section("Right side", right),
        ))
    }
}

pub fn sidebar(_: &PageCx) -> AnyView {
    SidebarPage.into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "breadcrumb" => breadcrumb,
        "pagination" => pagination,
        "navigation-menu" => navigation_menu,
        "sidebar" => sidebar,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, SemanticAction};

    fn press(page: &mut Harness, key: Key) -> bool {
        let none = ShortcutModifiers::default();
        let handled = page.host.key(KeyEvent::Down { key, repeat: false }, none);
        page.host.key(KeyEvent::Up { key }, none);
        page.host.layout();
        handled
    }

    #[test]
    fn pagination_page_moves_with_assistive_actions() {
        let mut page = Harness::new("pagination");
        assert_eq!(page.value("Archive").as_deref(), Some("10"));
        assert!(page.act("Archive", SemanticAction::Increment));
        assert_eq!(page.value("Archive").as_deref(), Some("11"));
    }

    #[test]
    fn breadcrumb_takes_keys_while_focused() {
        let mut page = Harness::new("breadcrumb");
        assert!(page.act("Collapsed location", SemanticAction::Focus));
        assert!(press(&mut page, Key::Tab));
        assert!(press(&mut page, Key::Enter));
        let node = page.node("Collapsed location");
        let expanded = page.host.semantic(node).unwrap().state.expanded;
        assert_eq!(expanded, Some(true));
    }

    #[test]
    fn sidebars_start_on_the_inbox() {
        let mut page = Harness::new("sidebar");
        let inbox = page.value("Icon sidebar");
        assert!(
            inbox.as_ref().is_some_and(|value| value.contains("Inbox")),
            "{inbox:?}"
        );
    }
}
