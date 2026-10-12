//! Popups: a command-backed popup menu with a submenu, and hover tooltips.

use crate::ui::page::{Build, PageCx, caption, column};
use crate::ui::pages::hosted::stateful;
use choscorui::compose::view::{AnyView, Button, Configured, HStack, State, Text, View, ViewExt};
use choscorui::controls::{
    self, ButtonVariant, CommandId, CommandState, ControlContext, ControlEvent, ControlKind,
    CustomControl, DropdownMenu, MeasureContext, MenuCheck, MenuItem, PaintContext,
    TooltipController,
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

fn menu_items() -> Vec<MenuItem> {
    let with_shortcut = |label: &str, command, shortcut: &str| {
        MenuItem::action(label, command).with_shortcut(shortcut)
    };
    vec![
        with_shortcut("Cut", CUT, "⌘X"),
        with_shortcut("Copy", COPY, "⌘C"),
        with_shortcut("Paste (nothing copied)", PASTE, "⌘V"),
        MenuItem::Separator,
        MenuItem::action("Wrap long lines", WRAP).with_check(MenuCheck::Checkbox),
        MenuItem::submenu(
            "Share",
            vec![
                MenuItem::action("Email", EMAIL),
                MenuItem::action("Messages", CommandId(6)),
            ],
        ),
    ]
}

/// The readout for a chosen command.
fn chose(command: CommandId) -> String {
    let name = match command {
        CUT => "Cut",
        COPY => "Copy",
        PASTE => "Paste",
        WRAP => "Wrap long lines",
        EMAIL => "Share › Email",
        _ => "Share › Messages",
    };
    format!("Chose “{name}”")
}

/// The Edit menu: Paste is disabled and Wrap shows `wrap`. A choice
/// updates `last` and closes the menu, as Escape does.
fn edit_menu(wrap: State<bool>, last: State<String>, open: State<bool>) -> impl View {
    Configured::custom(ControlKind::Menu, move |_| {
        let mut menu = DropdownMenu::new(menu_items()).hosted("Edit");
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
        menu
    })
    .on_command(move |command| {
        if command == WRAP {
            wrap.update(|on| *on = !*on);
        }
        last.set(chose(command));
        open.set(false);
    })
    .on_dismiss(move || open.set(false))
}

/// A button that opens a popup menu; Paste is disabled and Wrap toggles
/// a check mark. Arrow keys, Return, and Escape work inside the menu.
pub fn dropdown_menu(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let open = cx.state(false);
        let wrap = cx.state(true);
        let last = cx.state("Click the button or press Space on it.".to_owned());
        column((
            Button::new("Edit ▾", move || open.set(true))
                .variant(ButtonVariant::Outline)
                .popover(open.binding(), move || edit_menu(wrap, last, open)),
            Text::new(move || last.get())
                .text_style(TextPreset::Small)
                .foreground(ColorRole::MutedForeground),
        ))
    })
}

const TIPS: [(&str, &str); 3] = [
    ("Bold", "Make the selection bold (⌘B)"),
    ("Italic", "Make the selection italic (⌘I)"),
    ("Underline", "Underline the selection (⌘U)"),
];

/// A button with a hover tooltip. The buttons of a page share one
/// controller, so moving between them skips the delay within the grace
/// period.
struct TipButton {
    button: controls::Button,
    tip: &'static str,
    tips: Rc<RefCell<TooltipController>>,
    /// The node, from the first event.
    node: Option<NodeId>,
    /// The host time of the next tooltip check.
    due: Option<f64>,
    /// The window area that the bubble fits in.
    viewport: Rect,
}

impl TipButton {
    fn new(label: &str, tip: &'static str, tips: Rc<RefCell<TooltipController>>) -> Self {
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
                    tips.hover(cx.node(), self.tip, cx.bounds(), now);
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
        node.description = self.tip.into();
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
        .map(|(label, tip)| {
            let tips = tips.clone();
            Configured::custom(ControlKind::Button, move |_| {
                TipButton::new(label, tip, tips.clone())
            })
        })
        .collect();
    column((
        caption("Hover a button and wait; move to the next one to see the grace period."),
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
