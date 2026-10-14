//! Popups: a command-backed popup menu with a submenu, and hover tooltips.

use crate::ui::page::{Build, PageCx, caption, column, section, stateful};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Button, DropdownMenu, Env, HStack, State, Text, Toggle, View, ViewExt,
};
use choscorui::controls::{ButtonVariant, CommandId, CommandState, MenuCheck, MenuItem, Placement};
use choscorui::core::{ColorRole, FontWeight, Space, TextPreset};

const CUT: CommandId = CommandId(1);
const COPY: CommandId = CommandId(2);
const PASTE: CommandId = CommandId(3);
const WRAP: CommandId = CommandId(4);
const EMAIL: CommandId = CommandId(5);
const MESSAGES: CommandId = CommandId(6);
const BY_NAME: CommandId = CommandId(7);
const BY_DATE: CommandId = CommandId(8);
const DELETE: CommandId = CommandId(9);

/// The translation of `key`, for strings that controls keep.
fn text(env: &Env, key: &str) -> String {
    env.text(key, &[])
}

fn menu_items(env: &Env) -> Vec<MenuItem> {
    let with_shortcut = |key: &str, command, shortcut: &str| {
        MenuItem::action(text(env, key), command).with_shortcut(shortcut)
    };
    vec![
        with_shortcut("dropdown-menu.cut", CUT, "⌘X"),
        with_shortcut("dropdown-menu.copy", COPY, "⌘C"),
        with_shortcut("dropdown-menu.paste_nothing", PASTE, "⌘V"),
        MenuItem::Separator,
        MenuItem::action(text(env, "dropdown-menu.wrap"), WRAP).with_check(MenuCheck::Checkbox),
        MenuItem::submenu(
            text(env, "dropdown-menu.share"),
            vec![
                MenuItem::action(text(env, "dropdown-menu.email"), EMAIL),
                MenuItem::action(text(env, "dropdown-menu.messages"), MESSAGES),
            ],
        ),
        MenuItem::Separator,
        MenuItem::action(text(env, "dropdown-menu.by_name"), BY_NAME).with_check(MenuCheck::Radio),
        MenuItem::action(text(env, "dropdown-menu.by_date"), BY_DATE).with_check(MenuCheck::Radio),
        MenuItem::Separator,
        MenuItem::action(text(env, "dropdown-menu.delete"), DELETE).destructive(),
    ]
}

/// The readout for a chosen command.
fn chose(env: &Env, command: CommandId) -> String {
    let key = match command {
        CUT => "dropdown-menu.cut",
        COPY => "dropdown-menu.copy",
        PASTE => "dropdown-menu.paste",
        WRAP => "dropdown-menu.wrap",
        EMAIL => "dropdown-menu.share_email",
        BY_NAME => "dropdown-menu.by_name",
        BY_DATE => "dropdown-menu.by_date",
        DELETE => "dropdown-menu.delete",
        _ => "dropdown-menu.share_messages",
    };
    env.text("dropdown-menu.chose", &[("name", &text(env, key))])
}

/// The Edit menu: Paste is disabled, Wrap shows `wrap`, and the sort
/// order radio items show `by_date`. A choice updates `last` with a readout
/// in `env` and closes the menu, as Escape does.
fn edit_menu(
    env: Env,
    wrap: State<bool>,
    by_date: State<bool>,
    last: State<String>,
    open: State<bool>,
) -> impl View {
    DropdownMenu::new(t!("dropdown-menu.edit"), menu_items)
        .with(move |mut menu| {
            let disabled = CommandState {
                enabled: false,
                checked: false,
            };
            let wrapped = CommandState {
                enabled: true,
                checked: wrap.get(),
            };
            let _ = menu.set_command(PASTE, disabled);
            let _ = menu.set_command(WRAP, wrapped);
            for order in [BY_NAME, BY_DATE] {
                let state = CommandState {
                    enabled: true,
                    checked: by_date.get() == (order == BY_DATE),
                };
                let _ = menu.set_command(order, state);
            }
            menu
        })
        .on_command(move |command| {
            if command == WRAP {
                wrap.update(|on| *on = !*on);
            }
            if command == BY_NAME || command == BY_DATE {
                by_date.set(command == BY_DATE);
            }
            last.set(chose(&env, command));
            open.set(false);
        })
        .on_dismiss(move || open.set(false))
}

/// A button that opens a popup menu; Paste is disabled, Wrap toggles a
/// check mark, two radio items choose a sort order, and Delete has the
/// destructive look. Arrow keys, Return, and Escape work inside the menu.
pub fn dropdown_menu(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let open = cx.state(false);
        let wrap = cx.state(true);
        let by_date = cx.state(false);
        let env = cx.env().clone();
        let last = cx.state(text(&env, "dropdown-menu.prompt"));
        column((
            Button::new(t!("dropdown-menu.trigger"), move || open.set(true))
                .variant(ButtonVariant::Outline)
                .popover(open.binding(), move || {
                    edit_menu(env.clone(), wrap, by_date, last, open)
                }),
            Text::new(move || last.get())
                .text_style(TextPreset::Small)
                .foreground(ColorRole::MutedForeground),
        ))
    })
}

/// The styles that are on, as a readout line in `env`'s language.
fn applied_styles(env: &Env, styles: [(bool, &str); 3]) -> String {
    let names: Vec<String> = styles
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, key)| env.text(key, &[]))
        .collect();
    if names.is_empty() {
        env.text("tooltip.no_style", &[])
    } else {
        let list = names
            .into_iter()
            .rev()
            .reduce(|rest, first| {
                env.text("tooltip.list_pair", &[("first", &first), ("rest", &rest)])
            })
            .unwrap_or_default();
        env.text("tooltip.applied", &[("styles", &list)])
    }
}

/// Toggle buttons with tooltips: a delay before the first, a grace period
/// between neighbours, hiding on press, and at once on keyboard focus.
/// The last tooltip shows below its button. Each toggle sets a style of
/// the sample, and a disabled button keeps its tooltip.
pub fn tooltip(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let (bold, italic, underline) = (cx.state(false), cx.state(false), cx.state(false));
        let env = cx.env().clone();
        let weight = move || {
            if bold.get() {
                FontWeight::BOLD
            } else {
                FontWeight::REGULAR
            }
        };
        column((
            caption(t!("tooltip.caption")),
            HStack::new((
                Toggle::new(t!("tooltip.bold"), bold.binding()).tooltip(t!("tooltip.bold_tip")),
                Toggle::new(t!("tooltip.italic"), italic.binding())
                    .tooltip(t!("tooltip.italic_tip")),
                Toggle::new(t!("tooltip.underline"), underline.binding())
                    .tooltip_on(t!("tooltip.underline_tip"), Placement::Below),
            ))
            .spacing(Space::S3),
            Text::new(t!("tooltip.sample")).font_weight(weight),
            Text::new(move || {
                let styles = [
                    (bold.get(), "tooltip.bold"),
                    (italic.get(), "tooltip.italic"),
                    (underline.get(), "tooltip.underline"),
                ];
                applied_styles(&env, styles)
            })
            .text_style(TextPreset::Small)
            .foreground(ColorRole::MutedForeground),
            section(
                t!("tooltip.disabled"),
                Button::new(t!("tooltip.share"), || {})
                    .variant(ButtonVariant::Outline)
                    .disabled(true)
                    .tooltip(t!("tooltip.share_tip")),
            ),
        ))
    })
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "dropdown-menu" => dropdown_menu,
        "tooltip" => tooltip,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, Point, PointerEvent, SemanticAction, SemanticRole};

    fn press(page: &mut Harness, key: Key) -> bool {
        let event = KeyEvent::Down { key, repeat: false };
        let handled = page.host.key(event, ShortcutModifiers::default());
        page.host.layout();
        handled
    }

    #[test]
    fn dropdown_menu_opens_from_its_button_and_escape_closes_it() {
        let mut page = Harness::new("dropdown-menu");
        assert!(page.host.find("Edit").is_none());
        assert!(page.act("Edit ▾", SemanticAction::Activate));
        assert!(page.host.find("Edit").is_some());
        assert!(press(&mut page, Key::Escape));
        assert!(page.host.find("Edit").is_none());
    }

    #[test]
    fn choosing_a_menu_action_reports_it_and_closes_the_menu() {
        let mut page = Harness::new("dropdown-menu");
        assert!(page.act("Edit ▾", SemanticAction::Activate));
        assert!(press(&mut page, Key::ArrowDown));
        assert!(press(&mut page, Key::Enter));
        assert!(page.host.find("Chose “Cut”").is_some());
        assert!(page.host.find("Edit").is_none());
    }

    /// Whether a tooltip with `text` shows.
    fn showing(page: &mut Harness, text: &str) -> bool {
        page.host
            .find(text)
            .and_then(|node| page.host.semantic(node))
            .is_some_and(|node| node.role == SemanticRole::Tooltip)
    }

    #[test]
    fn tooltip_appears_only_after_the_hover_delay() {
        let mut page = Harness::new("tooltip");
        let bold = page.node("Bold");
        let node = page.host.semantic(bold).unwrap();
        assert_eq!(node.description, "Make the selection bold (⌘B)");
        let center = page.host.tree().bounds(bold).unwrap().center();
        page.host.pointer(
            PointerEvent::Move { position: center },
            ShortcutModifiers::default(),
        );
        page.host.advance(0.05);
        assert!(!showing(&mut page, "Make the selection bold (⌘B)"));
        page.host.advance(3.0);
        assert!(showing(&mut page, "Make the selection bold (⌘B)"));
    }

    #[test]
    fn the_style_toggles_change_the_sample_and_the_readout() {
        let mut page = Harness::new("tooltip");
        assert!(page.host.find("No style applied").is_some());
        assert!(page.act("Bold", SemanticAction::Activate));
        assert!(page.act("Underline", SemanticAction::Activate));
        assert!(page.host.find("Applied: Bold, Underline").is_some());
        let bold = page.node("Bold");
        assert_eq!(page.host.semantic(bold).unwrap().state.checked, Some(true));
    }

    #[test]
    fn a_disabled_button_shows_its_tooltip_on_hover() {
        let mut page = Harness::new("tooltip");
        let share = page.node("Share");
        assert!(page.host.semantic(share).unwrap().state.disabled);
        let center = page.host.tree().bounds(share).unwrap().center();
        page.host.pointer(
            PointerEvent::Move { position: center },
            ShortcutModifiers::default(),
        );
        page.host.advance(3.0);
        assert!(showing(&mut page, "Sign in to share this file"));
    }

    #[test]
    fn tooltip_shows_at_once_on_keyboard_focus_and_stays_after_a_hover() {
        let mut page = Harness::new("tooltip");
        let (bold, italic) = (page.node("Bold"), page.node("Italic"));
        page.host.advance(1.0);
        press(&mut page, Key::Tab);
        assert_eq!(page.host.tree().focus(), Some(bold));
        page.host.advance(1.0);
        assert!(showing(&mut page, "Make the selection bold (⌘B)"));
        // The pointer passes over Italic and leaves: the focus tooltip
        // shows again.
        let center = page.host.tree().bounds(italic).unwrap().center();
        let modifiers = ShortcutModifiers::default();
        page.host
            .pointer(PointerEvent::Move { position: center }, modifiers);
        page.host.advance(1.1);
        let away = Point::new(center.x, center.y + 200.0);
        page.host
            .pointer(PointerEvent::Move { position: away }, modifiers);
        page.host.advance(1.2);
        assert!(showing(&mut page, "Make the selection bold (⌘B)"));
    }
}
