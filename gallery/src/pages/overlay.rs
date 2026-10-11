//! Overlay components mounted through `ControlHost`: menus, popovers,
//! dialogs, sheets, drawers, and the command palette.

use crate::page::{Env, Page};
use crate::pages::hosted::{HostedPage, Row};
use choscorui::controls::{
    ButtonVariant, CommandId, ContextMenu, CustomControl, Dialog, MenuCheck, MenuItem, Menubar,
    Popover, SheetSide,
};
use choscorui::core::{NodeId, Tree};

fn boxed(control: impl CustomControl + 'static) -> Box<dyn CustomControl> {
    Box::new(control)
}

fn page_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("Back", CommandId(1)).with_shortcut("⌘["),
        MenuItem::action("Forward", CommandId(2)).with_shortcut("⌘]"),
        MenuItem::action("Reload", CommandId(3)).with_shortcut("⌘R"),
        MenuItem::submenu(
            "More Tools",
            vec![
                MenuItem::action("Save Page…", CommandId(4)),
                MenuItem::action("Create Shortcut…", CommandId(5)),
                MenuItem::Separator,
                MenuItem::action("Delete", CommandId(6)).destructive(),
            ],
        ),
        MenuItem::Separator,
        MenuItem::action("Show Bookmarks", CommandId(7)).with_check(MenuCheck::Checkbox),
        MenuItem::action("Show Full URLs", CommandId(8)).with_check(MenuCheck::Checkbox),
        MenuItem::Separator,
        MenuItem::action("Pedro Duarte", CommandId(9)).with_check(MenuCheck::Radio),
        MenuItem::action("Colm Tuite", CommandId(10)).with_check(MenuCheck::Radio),
    ]
}

pub fn context_menu(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![(
        "Secondary click or Control-click the area",
        vec![boxed(ContextMenu::new(
            "Page actions",
            "Right click here",
            page_items(),
        ))],
    )];
    HostedPage::build(tree, parent, rows)
}

pub fn menubar(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let menus = vec![
        (
            "File".to_string(),
            vec![
                MenuItem::action("New Tab", CommandId(20)).with_shortcut("⌘T"),
                MenuItem::action("New Window", CommandId(21)).with_shortcut("⌘N"),
                MenuItem::Separator,
                MenuItem::submenu(
                    "Share",
                    vec![
                        MenuItem::action("Email link", CommandId(22)),
                        MenuItem::action("Messages", CommandId(23)),
                    ],
                ),
                MenuItem::Separator,
                MenuItem::action("Print…", CommandId(24)).with_shortcut("⌘P"),
            ],
        ),
        (
            "Edit".to_string(),
            vec![
                MenuItem::action("Undo", CommandId(30)).with_shortcut("⌘Z"),
                MenuItem::action("Redo", CommandId(31)).with_shortcut("⇧⌘Z"),
                MenuItem::Separator,
                MenuItem::action("Cut", CommandId(32)),
                MenuItem::action("Copy", CommandId(33)),
                MenuItem::action("Paste", CommandId(34)),
            ],
        ),
        ("View".to_string(), page_items()),
    ];
    let rows: Vec<Row> = vec![("Menu bar", vec![boxed(Menubar::new("Main menu", menus))])];
    HostedPage::build(tree, parent, rows)
}

pub fn popover(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![(
        "Press the button",
        vec![boxed(Popover::new(
            "Open popover",
            "Dimensions",
            "Set the dimensions for the layer.",
        ))],
    )];
    HostedPage::build(tree, parent, rows)
}

pub fn hover_card(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![(
        "Rest the pointer on the link",
        vec![boxed(Popover::hover_card(
            "@nextjs",
            "@nextjs",
            "The React Framework – created and maintained by @vercel. Joined December 2021.",
        ))],
    )];
    HostedPage::build(tree, parent, rows)
}

fn profile(dialog: Dialog) -> Dialog {
    dialog
        .with_action("Cancel", ButtonVariant::Outline)
        .with_action("Save changes", ButtonVariant::Default)
        .with_close("Close")
}

const PROFILE: &str = "Make changes to your profile here. Click save when you're done.";

pub fn dialog(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![(
        "Modal dialog",
        vec![boxed(profile(Dialog::new(
            "Edit profile",
            "Edit profile",
            PROFILE,
        )))],
    )];
    HostedPage::build(tree, parent, rows)
}

const SIDES: [(SheetSide, &str); 4] = [
    (SheetSide::Top, "Top"),
    (SheetSide::Right, "Right"),
    (SheetSide::Bottom, "Bottom"),
    (SheetSide::Left, "Left"),
];

pub fn sheet(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let sheets = SIDES
        .into_iter()
        .map(|(side, label)| boxed(profile(Dialog::sheet(side, label, "Edit profile", PROFILE))))
        .collect();
    let rows: Vec<Row> = vec![("Sides", sheets)];
    HostedPage::build(tree, parent, rows)
}

pub fn drawer(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let drawers = SIDES
        .into_iter()
        .map(|(side, label)| {
            boxed(
                Dialog::drawer(side, label, "Move goal", "Set your daily activity goal.")
                    .with_action("Submit", ButtonVariant::Default)
                    .with_action("Cancel", ButtonVariant::Outline),
            )
        })
        .collect();
    let rows: Vec<Row> = vec![("Directions; drag toward the edge to dismiss", drawers)];
    HostedPage::build(tree, parent, rows)
}

#[cfg(test)]
mod tests {
    use crate::pages::hosted::HostedPage;
    use crate::pages::tests::Harness;
    use choscorui::core::{Key, SemanticAction, SemanticRole};

    fn expanded(h: &mut Harness, node: choscorui::core::NodeId) -> Option<bool> {
        let page = h.get::<HostedPage>();
        page.host.get(node).unwrap().semantics().state.expanded
    }

    #[test]
    fn menus_open_from_the_keyboard() {
        for (id, name, role) in [
            ("menubar", "Main menu", SemanticRole::Menu),
            ("context-menu", "Page actions", SemanticRole::Group),
            ("popover", "Open popover", SemanticRole::Button),
        ] {
            let mut h = Harness::new(id);
            let node = h.node(name, Some(role));
            assert!(h.act(node, SemanticAction::Focus));
            assert!(h.press(Key::Enter));
            assert_eq!(expanded(&mut h, node), Some(true), "{id}");
            assert!(h.press(Key::Escape));
            assert_eq!(expanded(&mut h, node), Some(false), "{id}");
        }
    }

    #[test]
    fn dialogs_open_from_the_keyboard_and_close_on_escape() {
        for (id, name) in [
            ("dialog", "Edit profile"),
            ("sheet", "Right"),
            ("drawer", "Bottom"),
        ] {
            let mut h = Harness::new(id);
            let node = h.node(name, Some(SemanticRole::Button));
            assert!(h.act(node, SemanticAction::Focus));
            assert!(h.press(Key::Enter));
            let page = h.get::<HostedPage>();
            let role = page.host.get(node).unwrap().semantics().role;
            assert_eq!(role, SemanticRole::Dialog, "{id}");
            assert!(h.press(Key::Escape));
            assert_eq!(expanded(&mut h, node), Some(false), "{id}");
        }
    }
}
