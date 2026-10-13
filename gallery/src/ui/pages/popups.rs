//! Popups: a command-backed popup menu with a submenu, and hover tooltips.

use crate::ui::page::{Build, PageCx, caption, column, stateful};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Button, Configured, DropdownMenu, Env, HStack, State, Text, View, ViewExt,
};
use choscorui::controls::{
    self, ButtonVariant, CommandId, CommandState, ControlContext, ControlEvent, ControlKind,
    CustomControl, MeasureContext, MenuCheck, MenuItem, PaintContext, TooltipController,
};
use choscorui::core::{
    ColorRole, EventReply, KeyEvent, LayoutDirection, Measured, NodeId, PointerEvent, Rect,
    SemanticAction, SemanticNode, Space, TextPreset,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

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

const TIPS: [(&str, &str); 3] = [
    ("tooltip.bold", "tooltip.bold_tip"),
    ("tooltip.italic", "tooltip.italic_tip"),
    ("tooltip.underline", "tooltip.underline_tip"),
];

/// A button with a hover tooltip. The buttons of a page share one
/// controller, so moving between them skips the delay within the grace
/// period.
struct TipButton {
    button: controls::Button,
    tip: String,
    tips: Rc<RefCell<TooltipController>>,
    /// The node, from the first event.
    node: Option<NodeId>,
    /// The host time of the next tooltip check.
    due: Option<f64>,
    /// The window area that the bubble fits in.
    viewport: Rect,
}

impl TipButton {
    fn new(label: String, tip: String, tips: Rc<RefCell<TooltipController>>) -> Self {
        Self {
            button: controls::Button::new(label, CommandId(0)).with_variant(ButtonVariant::Outline),
            tip,
            tips,
            node: None,
            due: None,
            viewport: Rect::default(),
        }
    }

    /// Whether the shared tooltip is this button's.
    fn showing(&self) -> bool {
        let tips = self.tips.borrow();
        tips.visible()
            .is_some_and(|tip| Some(tip.target) == self.node)
    }
}

/// Host seconds as the controller's time.
fn instant(seconds: f64) -> Duration {
    Duration::from_secs_f64(seconds.max(0.0))
}

impl CustomControl for TipButton {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        CustomControl::measure(&mut self.button, cx)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.viewport = cx.viewport;
        CustomControl::arrange(&mut self.button, cx, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        self.node = Some(cx.node());
        let now = instant(cx.time());
        let before = self.showing();
        {
            let mut tips = self.tips.borrow_mut();
            match event {
                ControlEvent::Hover(true) => {
                    tips.hover(cx.node(), &self.tip, cx.bounds(), now);
                    self.due = tips.poll(now).map(|due| due.as_secs_f64());
                }
                ControlEvent::Hover(false) => {
                    tips.leave(now);
                    self.due = None;
                }
                ControlEvent::Pointer(PointerEvent::Down { .. } | PointerEvent::Wheel { .. })
                | ControlEvent::Key(KeyEvent::Down { .. }) => {
                    tips.interrupt(now);
                    self.due = None;
                }
                ControlEvent::Timer => {
                    self.due = tips.poll(now).map(|due| due.as_secs_f64());
                }
                _ => {}
            }
        }
        if before != self.showing() {
            cx.request_paint();
        }
        CustomControl::event(&mut self.button, cx, event)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        CustomControl::paint(&self.button, cx);
    }

    fn overlay(&self) -> Option<Rect> {
        let tips = self.tips.borrow();
        let tip = tips.visible().filter(|tip| Some(tip.target) == self.node)?;
        Some(tip.anchor)
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        let tips = self.tips.borrow();
        let Some(tip) = tips.visible().filter(|tip| Some(tip.target) == self.node) else {
            return;
        };
        let direction = if cx.rtl {
            LayoutDirection::Rtl
        } else {
            LayoutDirection::Ltr
        };
        let placed = tip.place(self.viewport, cx.styles, cx.tokens, cx.text, direction);
        tip.paint(&mut cx.at(placed.rect), placed.placement);
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = CustomControl::semantics(&self.button);
        node.description = self.tip.clone();
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        CustomControl::perform(&mut self.button, cx, action, value)
    }

    fn focusable(&self) -> bool {
        CustomControl::focusable(&self.button)
    }

    fn set_enabled(&mut self, enabled: bool) {
        CustomControl::set_enabled(&mut self.button, enabled);
    }

    fn deadline(&self) -> Option<f64> {
        self.due
    }
}

/// Buttons with hover tooltips: a delay before the first, a grace period
/// between neighbours, and hiding on press.
pub fn tooltip(_: &PageCx) -> AnyView {
    let tips = Rc::new(RefCell::new(TooltipController::new()));
    let buttons: Vec<_> = TIPS
        .iter()
        .map(|&(label, tip)| {
            let tips = tips.clone();
            Configured::custom(ControlKind::Button, move |env| {
                TipButton::new(text(env, label), text(env, tip), tips.clone())
            })
        })
        .collect();
    column((
        caption(t!("tooltip.caption")),
        HStack::new(buttons).spacing(Space::S3),
    ))
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
    use choscorui::core::{Key, KeyEvent, PointerEvent, SemanticAction};

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
        let showing = |page: &Harness| {
            let control = page.host.controls().get(bold).unwrap();
            control.overlay().is_some()
        };
        page.host.advance(0.05);
        assert!(!showing(&page));
        page.host.advance(3.0);
        assert!(showing(&page));
    }
}
