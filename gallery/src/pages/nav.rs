//! Navigation components mounted through `ControlHost`: breadcrumbs,
//! pagination, the navigation menu, and the sidebar.

use crate::page::{Env, Page};
use crate::pages::hosted::{HostedPage, Row};
use choscorui::controls::{Breadcrumb, CustomControl, Pagination};
use choscorui::core::{NodeId, Tree};

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
