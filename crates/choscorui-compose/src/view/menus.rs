//! Menus that a view presents: a context menu at the pointer and a
//! dropdown menu below the view. Both show the controls' hosted dropdown
//! menu in an anchored layer, so its keys, hover, and type-ahead apply.
//! Focus moves into the menu, a press outside or Escape closes it, and
//! closing returns focus.

use super::View;
use super::configured::Configured;
use super::env::Env;
use super::host::{Cx, Inner};
use super::modifier::{Modifier, Trigger};
use super::present::{Anchor, Content, Placement, Presentation, mount_presentation};
use super::state::{Binding, IntoLive, Live};
use super::text::TextContent;
use super::widgets::Items;
use choscorui_controls::{
    CommandId, CommandState, ControlContext, ControlEvent, ControlKind, ControlMessage,
    CustomControl, DropdownMenu, MeasureContext, MenuCheck, MenuItem, PaintContext, Shortcut,
    ShortcutModifiers, Variant,
};
use choscorui_core::{
    EventPhase, EventReply, InputModality, KeyEvent, Measured, NodeId, Point, PointerButton,
    PointerEvent, SemanticNode, SemanticRole, SemanticState,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Whether a check row shows its mark.
type Checked = Rc<dyn Fn() -> bool>;

/// What a menu row is.
#[derive(Clone)]
pub(crate) enum Row {
    Action(Live<TextContent>, Rc<dyn Fn()>),
    /// A row with a check mark or a radio dot while `Checked` is true.
    Check(Live<TextContent>, MenuCheck, Checked, Rc<dyn Fn()>),
    Submenu(Live<TextContent>, Vec<MenuChoice>),
    Separator,
}

/// One row of a menu that a view presents: an action, a checkbox or
/// radio row, a submenu, or a separator. Labels take literal text or a catalog string (`t!`), and
/// follow the locale.
#[derive(Clone)]
pub struct MenuChoice {
    pub(crate) row: Row,
    shortcut: Option<String>,
    /// The key and modifiers of a command menu item.
    pub(crate) keys: Option<(char, ShortcutModifiers)>,
    destructive: bool,
    pub(crate) disabled: Live<bool>,
}

impl MenuChoice {
    fn new(row: Row) -> Self {
        Self {
            row,
            shortcut: None,
            keys: None,
            destructive: false,
            disabled: Live::Value(false),
        }
    }

    /// A row that runs `action` and closes the menu.
    pub fn action<M>(label: impl IntoLive<TextContent, M>, action: impl Fn() + 'static) -> Self {
        Self::new(Row::Action(label.into_live(), Rc::new(action)))
    }

    /// A row with a check mark while `checked` is true. Choosing it flips
    /// `checked` and closes the menu.
    pub fn checkbox<M>(label: impl IntoLive<TextContent, M>, checked: Binding<bool>) -> Self {
        Self::new(Row::Check(
            label.into_live(),
            MenuCheck::Checkbox,
            Rc::new(move || checked.get()),
            Rc::new(move || checked.update(|on| *on = !*on)),
        ))
    }

    /// A row of a radio group with a dot while `selection` is `value`.
    /// Choosing it sets `selection` to `value` and closes the menu. The
    /// rows that share `selection` make one group.
    pub fn radio<T, M>(
        label: impl IntoLive<TextContent, M>,
        selection: Binding<T>,
        value: T,
    ) -> Self
    where
        T: Clone + PartialEq + 'static,
    {
        let chosen = value.clone();
        Self::new(Row::Check(
            label.into_live(),
            MenuCheck::Radio,
            Rc::new(move || selection.try_get().is_ok_and(|current| current == chosen)),
            Rc::new(move || selection.set(value.clone())),
        ))
    }

    /// A row that opens `items` beside the menu.
    pub fn submenu<M>(label: impl IntoLive<TextContent, M>, items: Vec<MenuChoice>) -> Self {
        Self::new(Row::Submenu(label.into_live(), items))
    }

    /// A line between groups of rows.
    pub fn separator() -> Self {
        Self::new(Row::Separator)
    }

    /// Shows the shortcut text of an action, such as "⌘C". The text is
    /// a label only; `.keyboard_shortcut` runs the action from the keys.
    #[must_use]
    pub fn shortcut(mut self, text: impl Into<String>) -> Self {
        self.shortcut = Some(text.into());
        self
    }

    /// The key and modifiers that choose the item of a command menu, such
    /// as `'n'` with Command. The menu bar shows them, and the keys run
    /// the item from any window. A menu that a view presents ignores them;
    /// use [`MenuChoice::shortcut`] there.
    #[must_use]
    pub fn keyboard_shortcut(mut self, key: char, modifiers: ShortcutModifiers) -> Self {
        self.keys = Some((key, modifiers));
        self
    }

    /// Paints an action with the destructive look.
    #[must_use]
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// Disables an action while `disabled` is true when the menu opens.
    /// The keys skip a disabled action, and it does not run.
    #[must_use]
    pub fn disabled<M>(mut self, disabled: impl IntoLive<bool, M>) -> Self {
        self.disabled = disabled.into_live();
        self
    }
}

/// The actions of a built menu, by command id less one.
pub(super) type Actions = Rc<RefCell<Vec<Rc<dyn Fn()>>>>;

/// Converts `choices` to menu items in `env`. Each action and check row
/// gets the next command id; `actions` collects their actions, and
/// `states` the commands that are disabled or checked.
pub(super) fn build(
    choices: &[MenuChoice],
    env: &Env,
    actions: &mut Vec<Rc<dyn Fn()>>,
    states: &mut Vec<(CommandId, CommandState)>,
) -> Vec<MenuItem> {
    choices
        .iter()
        .map(|choice| {
            let (label, check, checked, action) = match &choice.row {
                Row::Action(label, action) => (label, MenuCheck::None, false, action),
                Row::Check(label, check, checked, action) => (label, *check, checked(), action),
                Row::Submenu(label, items) => {
                    return MenuItem::submenu(
                        label.get().resolve(env),
                        build(items, env, actions, states),
                    );
                }
                Row::Separator => return MenuItem::Separator,
            };
            actions.push(action.clone());
            let command = CommandId(actions.len() as u64);
            let enabled = !choice.disabled.get();
            if !enabled || checked {
                states.push((command, CommandState { enabled, checked }));
            }
            let mut item = MenuItem::action(label.get().resolve(env), command).with_check(check);
            if let Some(shortcut) = &choice.shortcut {
                item = item.with_shortcut(shortcut.clone());
            }
            if choice.destructive {
                item = item.destructive();
            }
            item
        })
        .collect()
}

/// The look and place of a presented menu: anchored, with the menu's own
/// panel.
const MENU: Presentation = Presentation {
    kind: ControlKind::Menu,
    variant: Variant(0),
    placement: Placement::Anchored,
    surface: false,
    modal: true,
};

/// The accessible name of `node`, which names its menu.
fn name_of(inner: &Inner, node: NodeId) -> String {
    let label = inner
        .nodes
        .get(&node)
        .and_then(|data| data.access.label.clone());
    label
        .or_else(|| {
            inner
                .controls
                .get(node)
                .map(|control| control.semantics().name)
        })
        .unwrap_or_default()
}

/// The menu content: a hosted dropdown menu of `items`. A chosen row
/// sets `presented` to false and runs its action; Escape sets it to
/// false. While `keyboard` is true, the menu opens with the first row
/// highlighted.
fn menu_content(
    name: String,
    items: Rc<dyn Items<MenuChoice>>,
    presented: Binding<bool>,
    keyboard: Rc<Cell<bool>>,
) -> Content {
    Rc::new(move || {
        let actions: Actions = Rc::default();
        let built = actions.clone();
        let (name, items, keyboard) = (name.clone(), items.clone(), keyboard.clone());
        Configured::custom(ControlKind::Menu, move |env| {
            let mut list = Vec::new();
            let mut states = Vec::new();
            let items = build(&items.items(env), env, &mut list, &mut states);
            *built.borrow_mut() = list;
            let mut menu = DropdownMenu::new(items)
                .hosted(name.clone())
                .opened_from_keyboard(keyboard.get());
            for (command, state) in states {
                // Every row of the menu has a command, so this cannot
                // fail.
                let _ = menu.set_command(command, state);
            }
            menu
        })
        .on_message(move |message| match message {
            ControlMessage::Command(CommandId(id)) => {
                presented.set(false);
                let index = usize::try_from(*id).unwrap_or(0).wrapping_sub(1);
                let action = actions.borrow().get(index).cloned();
                if let Some(action) = action {
                    action();
                }
            }
            ControlMessage::Dismissed => presented.set(false),
            _ => {}
        })
        .into_any()
    })
}

/// The modifier that shows a menu of `items` below the view.
pub(super) fn dropdown_menu(presented: Binding<bool>, items: impl Items<MenuChoice>) -> Modifier {
    let items: Rc<dyn Items<MenuChoice>> = Rc::new(items);
    Modifier::Present(Rc::new(move |cx: &mut Cx, anchor| {
        let name = name_of(cx.inner, anchor);
        // A menu that opens after a key highlights its first row.
        let keyboard: Rc<Cell<bool>> = Rc::default();
        let opened = keyboard.clone();
        cx.react(move |inner| {
            if presented.get() {
                opened.set(inner.tree.input_modality() == InputModality::Keyboard);
            }
        });
        let content = menu_content(name, items.clone(), presented, keyboard);
        mount_presentation(
            cx,
            Anchor::Node(anchor),
            presented,
            (MENU, None),
            (Rc::default(), content),
        );
    }))
}

/// The control that wraps a view with a context menu: a secondary press
/// or a Control-press inside the view records the point and reports
/// `OpenChanged(true)`. It sees the press before the views inside, which
/// keep it. Shift-F10 or the Menu key that no focused view inside uses
/// does the same at the focused view's center, and the menu opens with
/// the first row highlighted. It paints nothing and takes no focus.
struct MenuTrigger {
    point: Rc<Cell<Point>>,
    enabled: Rc<Cell<bool>>,
    /// Whether the keyboard opened the menu.
    keyboard: Rc<Cell<bool>>,
}

impl CustomControl for MenuTrigger {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.min)
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if !self.enabled.get() {
            return EventReply::Continue;
        }
        match (cx.phase(), event) {
            (
                EventPhase::Capture,
                ControlEvent::Pointer(PointerEvent::Down { position, button }),
            ) => {
                let secondary = *button == PointerButton::Secondary
                    || *button == PointerButton::Primary && cx.modifiers().control;
                if secondary {
                    self.point.set(*position);
                    self.keyboard.set(false);
                    cx.emit(ControlMessage::OpenChanged(true));
                }
                EventReply::Continue
            }
            // The key bubbles from the focused view, so the innermost
            // context menu opens and stops it.
            (EventPhase::Bubble, ControlEvent::Key(KeyEvent::Down { key, .. })) => {
                let shortcut = Shortcut {
                    key: *key,
                    modifiers: cx.modifiers(),
                };
                if !shortcut.opens_context_menu() {
                    return EventReply::Continue;
                }
                let at = cx.focus_bounds().unwrap_or_else(|| cx.bounds());
                self.point.set(at.center());
                self.keyboard.set(true);
                cx.emit(ControlMessage::OpenChanged(true));
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, _cx: &mut PaintContext<'_>) {}

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Group,
            name: String::new(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        }
    }

    fn focusable(&self) -> bool {
        false
    }
}

/// The modifier that opens a menu of `items` at a secondary click.
pub(super) fn context_menu(items: impl Items<MenuChoice>) -> Modifier {
    let items: Rc<dyn Items<MenuChoice>> = Rc::new(items);
    let point: Rc<Cell<Point>> = Rc::new(Cell::new(Point::new(0.0, 0.0)));
    let enabled = Rc::new(Cell::new(true));
    let keyboard: Rc<Cell<bool>> = Rc::default();
    let (control_point, control_enabled, control_keyboard) =
        (point.clone(), enabled.clone(), keyboard.clone());
    Modifier::Trigger(Trigger {
        control: Box::new(move |_inner: &mut Inner| {
            Box::new(MenuTrigger {
                point: control_point,
                enabled: control_enabled,
                keyboard: control_keyboard,
            })
        }),
        mount: Box::new(move |cx: &mut Cx, wrapper, view| {
            let open = cx.state(false).binding();
            let disabled = cx.env.disabled.clone();
            let live = enabled.clone();
            cx.bind(disabled, move |_inner, disabled| live.set(!disabled));
            // Of nested context menus, the innermost one opens.
            let press = cx.inner.menu_press.clone();
            cx.inner
                .data(wrapper)
                .handlers
                .push(Rc::new(move |message: &ControlMessage| {
                    if *message == ControlMessage::OpenChanged(true) {
                        *press.borrow_mut() = Some(Rc::new(move || open.set(true)));
                    }
                }));
            let (center, shown_by_action) = (point.clone(), keyboard.clone());
            cx.inner.data(view).show_menu = Some(Rc::new(move |inner: &Inner| {
                if !enabled.get() {
                    return;
                }
                if let Some(bounds) = inner.tree.bounds(wrapper) {
                    center.set(bounds.center());
                }
                shown_by_action.set(true);
                open.set(true);
            }));
            let name = name_of(cx.inner, view);
            let content = menu_content(name, items, open, keyboard);
            mount_presentation(
                cx,
                Anchor::Point(point),
                open,
                (MENU, None),
                (Rc::default(), content),
            );
        }),
    })
}
