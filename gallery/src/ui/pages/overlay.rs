//! Overlay components: menus, popovers, dialogs, sheets, drawers, and the
//! command palette.

use crate::ui::page::{Build, PageCx, column, row, section, stateful};
use choscorui::compose::t;
use choscorui::compose::view::{AnyView, Command, ContextMenu, Dialog, Env, Menubar, Popover};
use choscorui::controls::{
    self, ButtonVariant, CommandId, CommandItem, MenuCheck, MenuItem, SheetSide,
};
use choscorui::core::Icon;

/// The translation of `key`, for strings that controls keep.
fn text(env: &Env, key: &str) -> String {
    env.text(key, &[])
}

fn page_items(env: &Env) -> Vec<MenuItem> {
    let text = |key: &str| text(env, key);
    vec![
        MenuItem::action(text("context-menu.back"), CommandId(1)).with_shortcut("⌘["),
        MenuItem::action(text("context-menu.forward"), CommandId(2)).with_shortcut("⌘]"),
        MenuItem::action(text("context-menu.reload"), CommandId(3)).with_shortcut("⌘R"),
        MenuItem::submenu(
            text("context-menu.more_tools"),
            vec![
                MenuItem::action(text("context-menu.save_page"), CommandId(4)),
                MenuItem::action(text("context-menu.create_shortcut"), CommandId(5)),
                MenuItem::Separator,
                MenuItem::action(text("context-menu.delete"), CommandId(6)).destructive(),
            ],
        ),
        MenuItem::Separator,
        MenuItem::action(text("context-menu.show_bookmarks"), CommandId(7))
            .with_check(MenuCheck::Checkbox),
        MenuItem::action(text("context-menu.show_full_urls"), CommandId(8))
            .with_check(MenuCheck::Checkbox),
        MenuItem::Separator,
        MenuItem::action("Pedro Duarte", CommandId(9)).with_check(MenuCheck::Radio),
        MenuItem::action("Colm Tuite", CommandId(10)).with_check(MenuCheck::Radio),
    ]
}

pub fn context_menu(_: &PageCx) -> AnyView {
    column(section(
        t!("context-menu.caption"),
        ContextMenu::new(t!("context-menu.name"), t!("context-menu.area"), page_items),
    ))
}

fn menus(env: &Env) -> Vec<(String, Vec<MenuItem>)> {
    let text = |key: &str| text(env, key);
    vec![
        (
            text("menubar.file"),
            vec![
                MenuItem::action(text("menubar.new_tab"), CommandId(20)).with_shortcut("⌘T"),
                MenuItem::action(text("menubar.new_window"), CommandId(21)).with_shortcut("⌘N"),
                MenuItem::Separator,
                MenuItem::submenu(
                    text("menubar.share"),
                    vec![
                        MenuItem::action(text("menubar.email_link"), CommandId(22)),
                        MenuItem::action(text("menubar.messages"), CommandId(23)),
                    ],
                ),
                MenuItem::Separator,
                MenuItem::action(text("menubar.print"), CommandId(24)).with_shortcut("⌘P"),
            ],
        ),
        (
            text("menubar.edit"),
            vec![
                MenuItem::action(text("menubar.undo"), CommandId(30)).with_shortcut("⌘Z"),
                MenuItem::action(text("menubar.redo"), CommandId(31)).with_shortcut("⇧⌘Z"),
                MenuItem::Separator,
                MenuItem::action(text("menubar.cut"), CommandId(32)),
                MenuItem::action(text("menubar.copy"), CommandId(33)),
                MenuItem::action(text("menubar.paste"), CommandId(34)),
            ],
        ),
        (text("menubar.view"), page_items(env)),
    ]
}

pub fn menubar(_: &PageCx) -> AnyView {
    column(section(
        t!("menubar.section"),
        Menubar::new(t!("menubar.name"), menus),
    ))
}

pub fn popover(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let open = cx.state(false);
        column(section(
            t!("popover.caption"),
            Popover::new(
                t!("popover.trigger"),
                t!("popover.title"),
                t!("popover.description"),
                open.binding(),
            ),
        ))
    })
}

pub fn hover_card(_: &PageCx) -> AnyView {
    column(section(
        t!("hover-card.caption"),
        Popover::hover_card("@nextjs", "@nextjs", t!("hover-card.description")),
    ))
}

/// The profile form's actions and close button.
fn profile(dialog: controls::Dialog, env: &Env) -> controls::Dialog {
    dialog
        .with_action(text(env, "dialog.cancel"), ButtonVariant::Outline)
        .with_action(text(env, "dialog.save"), ButtonVariant::Default)
        .with_close(text(env, "dialog.close"))
}

pub fn dialog(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let open = cx.state(false);
        let dialog = Dialog::new(
            t!("dialog.edit_profile"),
            t!("dialog.edit_profile"),
            t!("dialog.profile"),
            open.binding(),
        )
        .with_env(profile);
        column(section(t!("dialog.caption"), dialog))
    })
}

const SIDES: [(SheetSide, &str); 4] = [
    (SheetSide::Top, "sheet.top"),
    (SheetSide::Right, "sheet.right"),
    (SheetSide::Bottom, "sheet.bottom"),
    (SheetSide::Left, "sheet.left"),
];

pub fn sheet(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let sheets: Vec<_> = SIDES
            .into_iter()
            .map(|(side, label)| {
                let open = cx.state(false);
                Dialog::sheet(
                    side,
                    t!(label),
                    t!("dialog.edit_profile"),
                    t!("dialog.profile"),
                    open.binding(),
                )
                .with_env(profile)
            })
            .collect();
        column(section(t!("sheet.caption"), row(sheets)))
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
                    t!(label),
                    t!("drawer.title"),
                    t!("drawer.description"),
                    open.binding(),
                )
                .with_env(|drawer, env| {
                    drawer
                        .with_action(text(env, "drawer.submit"), ButtonVariant::Default)
                        .with_action(text(env, "dialog.cancel"), ButtonVariant::Outline)
                })
            })
            .collect();
        column(section(t!("drawer.caption"), row(drawers)))
    })
}

fn commands(env: &Env) -> Vec<(String, Vec<CommandItem>)> {
    let text = |key: &str| text(env, key);
    vec![
        (
            text("command.suggestions"),
            vec![
                CommandItem::new(text("command.calendar")).with_icon(Icon::Calendar),
                CommandItem::new(text("command.search_emoji")).with_icon(Icon::Search),
                CommandItem::new(text("command.calculator")).disabled(),
            ],
        ),
        (
            text("command.settings"),
            vec![
                CommandItem::new(text("command.profile"))
                    .with_icon(Icon::User)
                    .with_shortcut("⌘P"),
                CommandItem::new(text("command.billing"))
                    .with_icon(Icon::Mail)
                    .with_shortcut("⌘B"),
                CommandItem::new(text("command.settings"))
                    .with_icon(Icon::Settings)
                    .with_shortcut("⌘S"),
            ],
        ),
    ]
}

/// The text the palette shows when no command matches.
fn empty(command: controls::Command, env: &Env) -> controls::Command {
    command.with_empty(text(env, "command.empty"))
}

pub fn command(_: &PageCx) -> AnyView {
    column((
        section(
            t!("command.inline"),
            Command::new(t!("command.name"), t!("command.placeholder"), commands).with_env(empty),
        ),
        section(
            t!("command.dialog"),
            Command::dialog(
                t!("command.open_palette"),
                t!("command.name"),
                t!("command.placeholder"),
                commands,
            )
            .with_env(empty),
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
