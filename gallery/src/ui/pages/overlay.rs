//! Overlay components: menus, popovers, dialogs, sheets, drawers, and the
//! command palette.

use crate::ui::page::{Build, PageCx, column, row, section, stateful};
use choscorui::compose::view::{
    AnyView, Command, Configured, ContextMenu, Dialog, Menubar, Popover,
};
use choscorui::controls::{
    self, ButtonVariant, CommandId, CommandItem, ControlKind, MenuCheck, MenuItem, SheetSide,
};
use choscorui::core::Icon;

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

pub fn context_menu(_: &PageCx) -> AnyView {
    column(section(
        "Secondary click or Control-click the area",
        ContextMenu::new("Page actions", "Right click here", page_items()),
    ))
}

pub fn menubar(_: &PageCx) -> AnyView {
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
    column(section("Menu bar", Menubar::new("Main menu", menus)))
}

pub fn popover(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let open = cx.state(false);
        column(section(
            "Press the button",
            Popover::new(
                "Open popover",
                "Dimensions",
                "Set the dimensions for the layer.",
                open.binding(),
            ),
        ))
    })
}

pub fn hover_card(_: &PageCx) -> AnyView {
    column(section(
        "Rest the pointer on the link",
        Popover::hover_card(
            "@nextjs",
            "@nextjs",
            "The React Framework – created and maintained by @vercel. Joined December 2021.",
        ),
    ))
}

/// The profile form's actions and close button.
fn profile(dialog: controls::Dialog) -> controls::Dialog {
    dialog
        .with_action("Cancel", ButtonVariant::Outline)
        .with_action("Save changes", ButtonVariant::Default)
        .with_close("Close")
}

const PROFILE: &str = "Make changes to your profile here. Click save when you're done.";

pub fn dialog(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let open = cx.state(false);
        column(section(
            "Modal dialog",
            Dialog::new("Edit profile", "Edit profile", PROFILE, open.binding()).with(profile),
        ))
    })
}

const SIDES: [(SheetSide, &str); 4] = [
    (SheetSide::Top, "Top"),
    (SheetSide::Right, "Right"),
    (SheetSide::Bottom, "Bottom"),
    (SheetSide::Left, "Left"),
];

pub fn sheet(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let sheets: Vec<_> = SIDES
            .into_iter()
            .map(|(side, label)| {
                let open = cx.state(false);
                Dialog::sheet(side, label, "Edit profile", PROFILE, open.binding()).with(profile)
            })
            .collect();
        column(section("Sides", row(sheets)))
    })
}

pub fn drawer(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let drawers: Vec<_> = SIDES
            .into_iter()
            .map(|(side, label)| {
                let open = cx.state(false);
                Dialog::drawer(
                    side,
                    label,
                    "Move goal",
                    "Set your daily activity goal.",
                    open.binding(),
                )
                .with(|drawer| {
                    drawer
                        .with_action("Submit", ButtonVariant::Default)
                        .with_action("Cancel", ButtonVariant::Outline)
                })
            })
            .collect();
        column(section(
            "Directions; drag toward the edge to dismiss",
            row(drawers),
        ))
    })
}

fn commands() -> Vec<(String, Vec<CommandItem>)> {
    vec![
        (
            "Suggestions".to_string(),
            vec![
                CommandItem::new("Calendar").with_icon(Icon::Calendar),
                CommandItem::new("Search Emoji").with_icon(Icon::Search),
                CommandItem::new("Calculator").disabled(),
            ],
        ),
        (
            "Settings".to_string(),
            vec![
                CommandItem::new("Profile")
                    .with_icon(Icon::User)
                    .with_shortcut("⌘P"),
                CommandItem::new("Billing")
                    .with_icon(Icon::Mail)
                    .with_shortcut("⌘B"),
                CommandItem::new("Settings")
                    .with_icon(Icon::Settings)
                    .with_shortcut("⌘S"),
            ],
        ),
    ]
}

pub fn command(_: &PageCx) -> AnyView {
    let empty = |command: controls::Command| command.with_empty("No results found.");
    column((
        section(
            "Inline",
            Command::new("Commands", "Type a command or search…", commands()).with(empty),
        ),
        section(
            "Dialog",
            Configured::custom(ControlKind::Command, move |_| {
                empty(controls::Command::dialog(
                    "Open command palette",
                    "Commands",
                    "Type a command or search…",
                    commands(),
                ))
            }),
        ),
    ))
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "context-menu" => context_menu,
        "menubar" => menubar,
        "popover" => popover,
        "hover-card" => hover_card,
        "dialog" => dialog,
        "sheet" => sheet,
        "drawer" => drawer,
        "command" => command,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, SemanticAction, SemanticRole};

    /// Presses `key` on the focused control.
    fn press(page: &mut Harness, key: Key) -> bool {
        let event = KeyEvent::Down { key, repeat: false };
        let handled = page.host.key(event, ShortcutModifiers::default());
        page.host.layout();
        handled
    }

    #[test]
    fn menus_open_from_the_keyboard() {
        for (id, name, role) in [
            ("menubar", "Main menu", SemanticRole::Menu),
            ("context-menu", "Page actions", SemanticRole::Group),
            ("popover", "Open popover", SemanticRole::Button),
        ] {
            let mut page = Harness::new(id);
            let node = page.node(name);
            assert_eq!(page.host.semantic(node).unwrap().role, role, "{id}");
            assert!(page.act(name, SemanticAction::Focus), "{id}");
            assert!(press(&mut page, Key::Enter), "{id}");
            let expanded = |page: &mut Harness| page.host.semantic(node).unwrap().state.expanded;
            assert_eq!(expanded(&mut page), Some(true), "{id}");
            assert!(press(&mut page, Key::Escape), "{id}");
            assert_eq!(expanded(&mut page), Some(false), "{id}");
        }
    }

    #[test]
    fn dialogs_open_from_the_keyboard_and_close_on_escape() {
        for (id, name) in [
            ("dialog", "Edit profile"),
            ("sheet", "Right"),
            ("drawer", "Bottom"),
        ] {
            let mut page = Harness::new(id);
            let node = page.node(name);
            assert!(page.act(name, SemanticAction::Focus), "{id}");
            assert!(press(&mut page, Key::Enter), "{id}");
            let role = page.host.semantic(node).unwrap().role;
            assert_eq!(role, SemanticRole::Dialog, "{id}");
            assert!(press(&mut page, Key::Escape), "{id}");
            let expanded = page.host.semantic(node).unwrap().state.expanded;
            assert_eq!(expanded, Some(false), "{id}");
        }
    }

    #[test]
    fn the_command_palette_dialog_opens_from_its_button() {
        let mut page = Harness::new("command");
        let node = page.node("Open command palette");
        assert_eq!(page.host.semantic(node).unwrap().role, SemanticRole::Button);
        assert!(page.act("Open command palette", SemanticAction::Focus));
        assert!(press(&mut page, Key::Enter));
        let open = page.host.semantic(node).unwrap();
        assert_eq!(
            (open.role, open.name.as_str()),
            (SemanticRole::ComboBox, "Commands")
        );
    }
}
