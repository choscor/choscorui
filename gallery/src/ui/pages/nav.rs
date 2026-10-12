//! Navigation components: breadcrumbs, pagination, the navigation menu,
//! and the sidebar.

use crate::ui::page::{Build, PageCx, column, section};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Breadcrumb, Cx, Env, HStack, NavigationMenu, Pagination, Sidebar, View,
};
use choscorui::controls::{
    NavigationLink, SheetSide, SidebarCollapse, SidebarItem, SidebarVariant,
};
use choscorui::core::{Icon, Space};

/// The translation of `key`, for strings that controls keep.
fn text(env: &Env, key: &str) -> String {
    env.text(key, &[])
}

/// The breadcrumb items with the catalog `keys`.
fn trail(keys: &'static [&'static str]) -> impl Fn(&Env) -> Vec<String> {
    move |env| keys.iter().map(|key| text(env, key)).collect()
}

pub fn breadcrumb(_: &PageCx) -> AnyView {
    column((
        section(
            t!("breadcrumb.links"),
            Breadcrumb::new(
                t!("breadcrumb.name"),
                trail(&[
                    "breadcrumb.home",
                    "breadcrumb.components",
                    "breadcrumb.breadcrumb",
                ]),
            ),
        ),
        section(
            t!("breadcrumb.collapsed"),
            Breadcrumb::new(
                t!("breadcrumb.collapsed_name"),
                trail(&[
                    "breadcrumb.home",
                    "breadcrumb.documentation",
                    "breadcrumb.building",
                    "breadcrumb.components",
                    "breadcrumb.breadcrumb",
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
        let (previous, next) = (|| t!("pagination.previous"), || t!("pagination.next"));
        column((
            section(
                t!("pagination.few"),
                Pagination::new(
                    t!("pagination.results"),
                    previous(),
                    next(),
                    3,
                    few.binding(),
                ),
            ),
            section(
                t!("pagination.many"),
                Pagination::new(
                    t!("pagination.archive"),
                    previous(),
                    next(),
                    20,
                    many.binding(),
                ),
            ),
        ))
    }
}

pub fn pagination(_: &PageCx) -> AnyView {
    PaginationPage.into_any()
}

/// The navigation menu's groups of links.
fn menus(env: &Env) -> Vec<(String, Vec<NavigationLink>)> {
    let link = |title: &str, description: &str| {
        NavigationLink::new(text(env, title), text(env, description))
    };
    vec![
        (
            text(env, "navigation-menu.getting_started"),
            vec![
                link(
                    "navigation-menu.introduction",
                    "navigation-menu.introduction_text",
                ),
                link(
                    "navigation-menu.installation",
                    "navigation-menu.installation_text",
                ),
                link(
                    "navigation-menu.typography",
                    "navigation-menu.typography_text",
                ),
            ],
        ),
        (
            text(env, "breadcrumb.components"),
            vec![
                link(
                    "navigation-menu.alert_dialog",
                    "navigation-menu.alert_dialog_text",
                ),
                link(
                    "navigation-menu.hover_card",
                    "navigation-menu.hover_card_text",
                ),
                link("navigation-menu.progress", "navigation-menu.progress_text"),
                link(
                    "navigation-menu.scroll_area",
                    "navigation-menu.scroll_area_text",
                ),
            ],
        ),
        (text(env, "navigation-menu.docs"), Vec::new()),
    ]
}

pub fn navigation_menu(_: &PageCx) -> AnyView {
    column(section(
        t!("navigation-menu.section"),
        NavigationMenu::new(t!("navigation-menu.name"), menus),
    ))
}

fn sidebar_groups(env: &Env) -> Vec<(String, Vec<SidebarItem>)> {
    let text = |key: &str| text(env, key);
    vec![
        (
            text("sidebar.application"),
            vec![
                SidebarItem::new(text("breadcrumb.home"), Icon::House),
                SidebarItem::new(text("sidebar.inbox"), Icon::Inbox).with_badge("24"),
                SidebarItem::new(text("sidebar.calendar"), Icon::Calendar),
                SidebarItem::new(text("sidebar.search"), Icon::Search),
            ],
        ),
        (
            text("sidebar.settings"),
            vec![SidebarItem::new(text("sidebar.settings"), Icon::Settings)],
        ),
    ]
}

/// Sidebars in each variant and collapse mode, and one on the right.
struct SidebarPage;

impl View for SidebarPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let none = cx.state(None);
        let mut make = |name: &'static str, variant: SidebarVariant, collapse: SidebarCollapse| {
            let selected = cx.state(Some(1));
            Sidebar::new(
                t!(name),
                t!("sidebar.toggle"),
                sidebar_groups,
                selected.binding(),
            )
            .with(move |sidebar| sidebar.with_variant(variant).with_collapse(collapse))
        };
        let row = |sidebars| HStack::new(sidebars).spacing(Space::S4);
        let right = Sidebar::new(
            t!("sidebar.right"),
            t!("sidebar.toggle"),
            sidebar_groups,
            none.binding(),
        )
        .with(|sidebar| sidebar.with_side(SheetSide::Right));
        // Two sidebars fit side by side in the narrowest content area.
        let first = row(vec![
            make(
                "sidebar.offcanvas",
                SidebarVariant::Sidebar,
                SidebarCollapse::Offcanvas,
            )
            .into_any(),
            make(
                "sidebar.icon",
                SidebarVariant::Sidebar,
                SidebarCollapse::Icon,
            )
            .into_any(),
        ]);
        let second = row(vec![
            make(
                "sidebar.fixed",
                SidebarVariant::Sidebar,
                SidebarCollapse::None,
            )
            .into_any(),
            make(
                "sidebar.floating",
                SidebarVariant::Floating,
                SidebarCollapse::Icon,
            )
            .into_any(),
        ]);
        let third = row(vec![
            make(
                "sidebar.inset",
                SidebarVariant::Inset,
                SidebarCollapse::Icon,
            )
            .into_any(),
            right.into_any(),
        ]);
        column((
            section(t!("sidebar.section_first"), first),
            section(t!("sidebar.section_second"), second),
            section(t!("sidebar.section_third"), third),
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
