//! Overlay components: menus, popovers, dialogs, sheets, drawers, and the
//! command palette.
//!
//! The dialog, sheet, drawer, and popover pages show a form through the
//! presentation modifiers, which take any view. The panels of the
//! controls show text only, so their samples carry text.

use crate::ui::page::{Build, PageCx, column, row, section, stateful, status_line};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Button, Command, ContextMenu, Cx, Dialog, Env, Field, Frame, HStack,
    HorizontalAlignment, Input, Menubar, Popover, SpinBox, State, VStack, View, ViewExt,
};
use choscorui::controls::{
    self, ButtonVariant, CommandId, CommandItem, MenuCheck, MenuItem, SheetSide,
};
use choscorui::core::{Icon, Space};

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
        MenuItem::action(text("people.pedro"), CommandId(9)).with_check(MenuCheck::Radio),
        MenuItem::action(text("people.colm"), CommandId(10)).with_check(MenuCheck::Radio),
    ]
}

pub fn context_menu(_: &PageCx) -> AnyView {
    column((
        section(
            t!("context-menu.caption"),
            ContextMenu::new(t!("context-menu.name"), t!("context-menu.area"), page_items),
        ),
        section(
            t!("context-menu.disabled"),
            ContextMenu::new(
                t!("context-menu.disabled_name"),
                t!("context-menu.disabled_area"),
                page_items,
            )
            .disabled(true),
        ),
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
        (
            text("menubar.view"),
            vec![
                MenuItem::action(text("menubar.show_toolbar"), CommandId(40))
                    .with_check(MenuCheck::Checkbox),
                MenuItem::action(text("menubar.show_status_bar"), CommandId(41))
                    .with_check(MenuCheck::Checkbox),
                MenuItem::Separator,
                MenuItem::action(text("menubar.zoom_in"), CommandId(42)).with_shortcut("⌘+"),
                MenuItem::action(text("menubar.zoom_out"), CommandId(43)).with_shortcut("⌘-"),
                MenuItem::action(text("menubar.actual_size"), CommandId(44)).with_shortcut("⌘0"),
                MenuItem::Separator,
                MenuItem::action(text("menubar.full_screen"), CommandId(45)).with_shortcut("⌃⌘F"),
            ],
        ),
    ]
}

pub fn menubar(_: &PageCx) -> AnyView {
    column((
        section(
            t!("menubar.section"),
            Menubar::new(t!("menubar.name"), menus),
        ),
        section(
            t!("menubar.disabled"),
            Menubar::new(t!("menubar.disabled_name"), menus).disabled(true),
        ),
    ))
}

/// `views` top to bottom, aligned at the leading edge, as a form.
fn form(views: impl View) -> impl View {
    VStack::new(views)
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S4)
        .frame(Frame::new().width(320.0))
}

/// The closing buttons of a form: `cancel` closes `open`, and the default
/// button runs `done` and closes `open`.
fn form_buttons(
    open: State<bool>,
    cancel: &'static str,
    accept: &'static str,
    done: impl Fn() + 'static,
) -> impl View {
    HStack::new((
        Button::new(t!(cancel), move || open.set(false)).variant(ButtonVariant::Outline),
        Button::new(t!(accept), move || {
            done();
            open.set(false);
        }),
    ))
    .spacing(Space::S2)
}

pub fn popover(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let (open, about) = (cx.state(false), cx.state(false));
        let (width, height) = (cx.state("100%".to_owned()), cx.state("25px".to_owned()));
        let dimensions = move || {
            form((
                Field::new(
                    t!("popover.width"),
                    Input::new(t!("popover.width"), width.binding()),
                ),
                Field::new(
                    t!("popover.height"),
                    Input::new(t!("popover.height"), height.binding()),
                ),
            ))
        };
        column((
            section(
                t!("popover.caption"),
                Button::new(t!("popover.trigger"), move || open.set(true))
                    .variant(ButtonVariant::Outline)
                    .popover(open.binding(), dimensions),
            ),
            section(
                t!("popover.text_only"),
                Popover::new(
                    t!("popover.about"),
                    t!("popover.about_title"),
                    t!("popover.about_description"),
                    about.binding(),
                ),
            ),
        ))
    })
}

pub fn hover_card(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let (env, status) = (cx.env().clone(), cx.state(String::new()));
        column((
            section(
                t!("hover-card.caption"),
                Popover::hover_card("@nextjs", "@nextjs", t!("hover-card.description"))
                    .on_activate(move || status.set(text(&env, "hover-card.opened"))),
            ),
            status_line(status),
        ))
    })
}

/// A profile form: the name and username drafts, the saved readout, and
/// whether the panel that holds it is open.
#[derive(Clone, Copy)]
struct Profile {
    name: State<String>,
    username: State<String>,
    saved: State<String>,
    open: State<bool>,
}

impl Profile {
    fn new(cx: &mut Cx) -> Self {
        Self {
            name: cx.state(text(cx.env(), "people.pedro")),
            username: cx.state("@peduarte".to_owned()),
            saved: cx.state(String::new()),
            open: cx.state(false),
        }
    }

    /// The fields and buttons. Save copies the drafts into the readout.
    fn form(self, env: &Env) -> impl View + use<> {
        let Self {
            name,
            username,
            saved,
            open,
        } = self;
        let env = env.clone();
        form((
            Field::new(
                t!("dialog.name"),
                Input::new(t!("dialog.name"), name.binding()),
            ),
            Field::new(
                t!("dialog.username"),
                Input::new(t!("dialog.username"), username.binding()),
            ),
            form_buttons(open, "dialog.cancel", "dialog.save", move || {
                let (name, username) = (name.get(), username.get());
                let args = [("name", name.as_str()), ("username", username.as_str())];
                saved.set(env.text("dialog.saved", &args));
            }),
        ))
    }
}

/// A text-only panel: its actions and close button.
fn notes(dialog: controls::Dialog, env: &Env) -> controls::Dialog {
    dialog
        .with_action(text(env, "dialog.close"), ButtonVariant::Outline)
        .with_close(text(env, "dialog.close"))
}

pub fn dialog(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let profile = Profile::new(cx);
        let (open, env) = (profile.open, cx.env().clone());
        let text_only = Dialog::new(
            t!("dialog.release_notes"),
            t!("dialog.release_notes"),
            t!("dialog.release_text"),
            cx.state(false).binding(),
        )
        .with_env(notes);
        column((
            section(
                t!("dialog.caption"),
                Button::new(t!("dialog.edit_profile"), move || open.set(true))
                    .variant(ButtonVariant::Outline)
                    .dialog(open.binding(), t!("dialog.edit_profile"), move || {
                        profile.form(&env)
                    }),
            ),
            status_line(profile.saved),
            section(t!("dialog.text_only"), text_only),
        ))
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
        let profile = Profile::new(cx);
        let (open, env) = (profile.open, cx.env().clone());
        let sheets: Vec<_> = SIDES
            .into_iter()
            .map(|(side, label)| {
                let open = cx.state(false);
                Dialog::sheet(
                    side,
                    t!(label),
                    t!("dialog.release_notes"),
                    t!("dialog.release_text"),
                    open.binding(),
                )
                .with_env(notes)
            })
            .collect();
        column((
            section(
                t!("sheet.form"),
                Button::new(t!("dialog.edit_profile"), move || open.set(true))
                    .variant(ButtonVariant::Outline)
                    .sheet(
                        open.binding(),
                        SheetSide::Right,
                        t!("dialog.edit_profile"),
                        move || profile.form(&env),
                    ),
            ),
            status_line(profile.saved),
            section(t!("sheet.caption"), row(sheets)),
        ))
    })
}

pub fn drawer(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let (open, goal, draft) = (cx.state(false), cx.state(350), cx.state(350));
        let goal_form = move || {
            form((
                SpinBox::new(t!("drawer.goal"), draft.binding())
                    .range(100..=1000)
                    .step(10),
                form_buttons(open, "dialog.cancel", "drawer.submit", move || {
                    goal.set(draft.get());
                }),
            ))
        };
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
                    drawer.with_action(text(env, "dialog.close"), ButtonVariant::Outline)
                })
            })
            .collect();
        column((
            section(
                t!("drawer.form"),
                Button::new(t!("drawer.move_goal"), move || open.set(true))
                    .variant(ButtonVariant::Outline)
                    .drawer(
                        open.binding(),
                        SheetSide::Bottom,
                        t!("drawer.title"),
                        goal_form,
                    ),
            ),
            status_line_of(goal),
            section(t!("drawer.caption"), row(drawers)),
        ))
    })
}

/// The goal readout of the drawer page.
fn status_line_of(goal: State<i32>) -> impl View {
    crate::ui::page::caption(t!("drawer.goal_readout", goal = move || goal.get()))
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

/// The readout of running command `index`, counted over every group.
fn ran(env: &Env, index: usize) -> String {
    let name = commands(env)
        .into_iter()
        .flat_map(|(_, items)| items)
        .nth(index)
        .map(|item| item.label)
        .unwrap_or_default();
    env.text("command.ran", &[("command", &name)])
}

pub fn command(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let status = cx.state(String::new());
        let (inline, palette) = (cx.env().clone(), cx.env().clone());
        column((
            section(
                t!("command.inline"),
                Command::new(t!("command.name"), t!("command.placeholder"), commands)
                    .with_env(empty)
                    .on_item(move |index| status.set(ran(&inline, index))),
            ),
            section(
                t!("command.dialog"),
                Command::dialog(
                    t!("command.open_palette"),
                    t!("command.name"),
                    t!("command.placeholder"),
                    commands,
                )
                .with_env(empty)
                .on_item(move |index| status.set(ran(&palette, index))),
            ),
            status_line(status),
            section(
                t!("command.disabled"),
                Command::new(
                    t!("command.disabled_name"),
                    t!("command.placeholder"),
                    commands,
                )
                .with_env(empty)
                .disabled(true),
            ),
        ))
    })
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
            ("menubar", "Main menu", SemanticRole::MenuBar),
            ("context-menu", "Page actions", SemanticRole::Group),
            ("popover", "About this layer", SemanticRole::Button),
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
            ("dialog", "Release notes"),
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

    #[test]
    fn the_form_dialog_saves_its_fields() {
        for id in ["dialog", "sheet"] {
            let mut page = Harness::new(id);
            assert!(page.act("Edit profile", SemanticAction::Activate), "{id}");
            let (snapshot, _) = page.host.semantics();
            let name = snapshot
                .iter()
                .find(|entry| entry.node.name == "Name" && entry.node.text.is_some())
                .map(|entry| entry.id);
            assert!(name.is_some(), "{id}: the name field shows");
            page.host.focus(name);
            assert!(page.host.text(" Jr."), "{id}");
            page.host.layout();
            assert!(page.act("Save changes", SemanticAction::Activate), "{id}");
            assert!(
                page.host
                    .find("Saved Pedro Duarte Jr. (@peduarte).")
                    .is_some(),
                "{id}"
            );
            assert!(page.host.find("Name").is_none(), "{id}: the form closes");
        }
    }

    #[test]
    fn the_inline_command_list_reports_the_command_it_runs() {
        let mut page = Harness::new("command");
        let list = page.node("Commands");
        page.host.focus(Some(list));
        // Focus highlights the first command; ArrowDown moves to the next.
        assert!(press(&mut page, Key::ArrowDown));
        assert!(press(&mut page, Key::Enter));
        let (snapshot, _) = page.host.semantics();
        let ran: Vec<_> = snapshot
            .iter()
            .filter(|entry| entry.node.name.starts_with("Ran"))
            .map(|entry| entry.node.name.clone())
            .collect();
        assert_eq!(ran, ["Ran Search Emoji."]);
    }
}
