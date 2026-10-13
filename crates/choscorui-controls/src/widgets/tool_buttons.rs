//! Tool buttons and command links, and the hosted controls that give
//! them a node of their own.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::{self, Parts, Press, PressOutcome};
use crate::style::fill_area;
use crate::style::{
    ControlKind, ItemVariant, PaintContext, Slot, StyleQuery, TextAlign, VisualState, content_rect,
};
use crate::{Button, CommandId, CommandRegistry, CommandState};
use choscorui_core::{
    ColorRole, EventPhase, EventReply, Icon, Key, KeyEvent, Measured, Point, PointerButton,
    PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRole, SemanticState, Size,
};

/// What a tool button does on press.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolButtonMenu {
    None,
    /// The whole button opens the menu.
    Instant,
    /// Click runs the command; the arrow (or Down key) opens the menu.
    Split,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolButtonAction {
    None,
    Activated(CommandId),
    Toggled(CommandId, bool),
    OpenMenu,
}

/// Toolbar button: optionally checkable (the command's checked state) and
/// optionally with a menu.
#[derive(Clone, Debug)]
pub struct ToolButton {
    pub label: String,
    pub command: CommandId,
    pub checkable: bool,
    pub menu: ToolButtonMenu,
    /// Width of the split-menu arrow at the trailing edge.
    pub arrow_width: f64,
}

impl ToolButton {
    pub fn new(label: impl Into<String>, command: CommandId) -> Self {
        Self {
            label: label.into(),
            command,
            checkable: false,
            menu: ToolButtonMenu::None,
            arrow_width: 14.0,
        }
    }

    fn run(&self, commands: &mut CommandRegistry) -> ToolButtonAction {
        let Ok(state) = commands.state(self.command) else {
            return ToolButtonAction::None;
        };
        if !state.enabled {
            return ToolButtonAction::None;
        }
        if self.checkable {
            let checked = !state.checked;
            let _ = commands.set_checked(self.command, checked);
            ToolButtonAction::Toggled(self.command, checked)
        } else {
            ToolButtonAction::Activated(self.command)
        }
    }

    /// A completed click at `x` within `bounds`.
    pub fn click(&self, bounds: Rect, x: f64, commands: &mut CommandRegistry) -> ToolButtonAction {
        match self.menu {
            ToolButtonMenu::Instant => ToolButtonAction::OpenMenu,
            ToolButtonMenu::Split
                if x >= bounds.origin.x + bounds.size.width - self.arrow_width =>
            {
                ToolButtonAction::OpenMenu
            }
            _ => self.run(commands),
        }
    }

    pub fn handle_key(&self, key: Key, commands: &mut CommandRegistry) -> ToolButtonAction {
        match key {
            Key::Space | Key::Enter if self.menu == ToolButtonMenu::Instant => {
                ToolButtonAction::OpenMenu
            }
            Key::Space | Key::Enter => self.run(commands),
            Key::ArrowDown if self.menu != ToolButtonMenu::None => ToolButtonAction::OpenMenu,
            _ => ToolButtonAction::None,
        }
    }

    pub fn semantics(&self, commands: &CommandRegistry) -> SemanticNode {
        let state = commands.state(self.command).unwrap_or_default();
        let mut actions = Vec::new();
        if state.enabled {
            actions.push(SemanticAction::Activate);
            if self.menu != ToolButtonMenu::None {
                actions.push(SemanticAction::ShowMenu);
            }
        }
        SemanticNode {
            role: if self.checkable {
                SemanticRole::Checkbox
            } else {
                SemanticRole::Button
            },
            name: self.label.clone(),
            description: String::new(),
            value: None,
            state: SemanticState {
                disabled: !state.enabled,
                checked: self.checkable.then_some(state.checked),
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions,
            text: None,
        }
    }

    /// The flags the toggle recipe reads: the host's flags in `host`, with
    /// the command's checked and disabled state.
    pub fn visual_state(&self, commands: &CommandRegistry, host: VisualState) -> VisualState {
        let state = commands.state(self.command).unwrap_or_default();
        host.set(VisualState::CHECKED, self.checkable && state.checked)
            .set(VisualState::DISABLED, !state.enabled)
    }

    /// Paints the button with the `Toggle` look: transparent, and `muted`
    /// on hover or while checked. A menu adds a chevron at the end; a split
    /// menu also adds a divider before the arrow.
    pub fn paint(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        self.paint_segments(cx, state, None);
    }

    /// Paints the button in `state`. `segments` gives the hover and press
    /// of the label and of the arrow: a split button fills each segment
    /// on its own, and other buttons take the label's flags.
    fn paint_segments(
        &self,
        cx: &mut PaintContext<'_>,
        state: VisualState,
        segments: Option<[VisualState; 2]>,
    ) {
        let split = self.menu == ToolButtonMenu::Split;
        let pointer = VisualState::HOVERED.with(VisualState::PRESSED);
        let state = match segments {
            Some(_) if split => state.without(pointer),
            Some([label, _]) => state
                .without(pointer)
                .with(label.without(VisualState::DISABLED)),
            None => state,
        };
        let style = cx.style(StyleQuery::new(ControlKind::Toggle, Slot::Root).state(state));
        let rule = cx.style(StyleQuery::new(ControlKind::ButtonGroup, Slot::Separator));
        let bounds = cx.bounds;
        let arrow = match self.menu {
            ToolButtonMenu::None => 0.0,
            ToolButtonMenu::Instant => style.icon_size,
            ToolButtonMenu::Split => self.arrow_width,
        }
        .min(bounds.size.width);
        // Instant menus keep the chevron inside the padding; a split arrow
        // spans the button's end edge.
        let (label_bounds, arrow_rect) = if arrow == 0.0 {
            (bounds, None)
        } else if split {
            let x = if cx.rtl {
                bounds.origin.x
            } else {
                bounds.max_x() - arrow
            };
            let label_x = if cx.rtl {
                bounds.origin.x + arrow
            } else {
                bounds.origin.x
            };
            (
                Rect::from_xywh(
                    label_x,
                    bounds.origin.y,
                    bounds.size.width - arrow,
                    bounds.size.height,
                ),
                Some(Rect::from_xywh(
                    x,
                    bounds.origin.y,
                    arrow,
                    bounds.size.height,
                )),
            )
        } else {
            (bounds, None)
        };
        cx.slot(bounds, &style, |cx, inner| {
            if let (Some(arrow), Some(segments)) = (arrow_rect, segments) {
                for (rect, segment, start) in [
                    (label_bounds, segments[0], true),
                    (arrow, segments[1], false),
                ] {
                    if !segment.interactive_hover() {
                        continue;
                    }
                    let query = StyleQuery::new(ControlKind::Toggle, Slot::Root).state(segment);
                    let mut fill = cx.style(query);
                    fill.border_width = 0.0;
                    let radius = &mut fill.radius;
                    if start {
                        (radius.top_end, radius.bottom_end) = (0.0, 0.0);
                    } else {
                        (radius.top_start, radius.bottom_start) = (0.0, 0.0);
                    }
                    cx.slot(rect, &fill, |_, _| {});
                }
            }
            let mut label = content_rect(label_bounds, &style, cx.rtl);
            label.size.height = inner.size.height;
            label.origin.y = inner.origin.y;
            if !split && arrow > 0.0 {
                let x = if cx.rtl {
                    inner.origin.x
                } else {
                    inner.max_x() - arrow
                };
                cx.icon(
                    Rect::from_xywh(x, inner.origin.y, arrow, inner.size.height),
                    Icon::ChevronDown,
                    &style,
                );
                label.size.width = (inner.size.width - arrow - style.gap).max(0.0);
                if cx.rtl {
                    label.origin.x = inner.max_x() - label.size.width;
                }
            }
            cx.label(label, &self.label, &style, TextAlign::Center);
        });
        if let Some(rect) = arrow_rect {
            let rule_x = if cx.rtl { rect.max_x() } else { rect.origin.x };
            let inset = style.padding.block_start.max(rect.size.height / 4.0);
            let line = Rect::from_xywh(
                rule_x - rule.min_width / 2.0,
                rect.origin.y + inset,
                rule.min_width,
                (rect.size.height - 2.0 * inset).max(0.0),
            );
            fill_area(cx.list, line, cx.rtl, &rule.background);
            let mut chevron = style.clone();
            chevron.icon_size = style.icon_size.min(arrow);
            cx.icon(rect, Icon::ChevronDown, &chevron);
        }
    }

    /// The button for a control host, with its own enabled and checked
    /// state in place of an application `CommandRegistry`.
    pub fn hosted(self) -> HostedToolButton {
        let mut commands = CommandRegistry::new();
        let _ = commands.insert(self.command, CommandState::default());
        HostedToolButton {
            button: self,
            commands,
            press: Press::default(),
            segments: Parts::default(),
            open: false,
            rtl: false,
        }
    }
}

/// A [`ToolButton`] mounted in a control host. `ToolButton` reads its
/// state from an application `CommandRegistry` that the host does not
/// have, so this type keeps the command state itself. It reports
/// `Command(id)`, `Changed(Bool)` when checkable, and `OpenChanged` for
/// its menu.
pub struct HostedToolButton {
    button: ToolButton,
    commands: CommandRegistry,
    press: Press,
    /// Hover and press of the label and of the split arrow.
    segments: Parts<Segment>,
    open: bool,
    rtl: bool,
}

/// A part of a hosted tool button that hovers on its own.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Segment {
    /// The label, or the whole button without a split arrow.
    Label,
    /// The split arrow.
    Arrow,
}

impl HostedToolButton {
    /// The button with a check state when `checkable` is true.
    #[must_use]
    pub fn with_checkable(mut self, checkable: bool) -> Self {
        self.button.checkable = checkable;
        self
    }

    /// The button with `menu`.
    #[must_use]
    pub fn with_menu(mut self, menu: ToolButtonMenu) -> Self {
        self.button.menu = menu;
        self
    }

    fn act(&mut self, cx: &mut ControlContext<'_>, action: ToolButtonAction) {
        match action {
            ToolButtonAction::Activated(id) => cx.emit(ControlMessage::Command(id)),
            ToolButtonAction::Toggled(_, checked) => {
                cx.emit(ControlMessage::Changed(ControlValue::Bool(checked)));
            }
            ToolButtonAction::OpenMenu => {
                self.open = !self.open;
                cx.emit(ControlMessage::OpenChanged(self.open));
            }
            ToolButtonAction::None => {}
        }
        cx.request_paint();
        cx.request_semantics();
    }

    /// The x of `point` in reading order, with the split arrow at the end.
    fn reading_x(&self, bounds: Rect, point: Point) -> f64 {
        if self.rtl {
            bounds.origin.x + bounds.max_x() - point.x
        } else {
            point.x
        }
    }

    /// A click at `point`.
    fn click(&mut self, bounds: Rect, point: Point) -> ToolButtonAction {
        let x = self.reading_x(bounds, point);
        self.button.click(bounds, x, &mut self.commands)
    }

    /// The segment at `point` in `bounds`.
    fn segment_at(&self, bounds: Rect, point: Point) -> Option<Segment> {
        let arrow = self.button.menu == ToolButtonMenu::Split
            && self.reading_x(bounds, point) >= bounds.max_x() - self.button.arrow_width;
        bounds.contains(point).then_some(if arrow {
            Segment::Arrow
        } else {
            Segment::Label
        })
    }
}

impl CustomControl for HostedToolButton {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(StyleQuery::new(ControlKind::Toggle, Slot::Root));
        let arrow = match self.button.menu {
            ToolButtonMenu::None => 0.0,
            ToolButtonMenu::Instant => style.icon_size + style.gap,
            ToolButtonMenu::Split => self.button.arrow_width,
        };
        let label = cx.text.width(&self.button.label, &style.text);
        let width = style.padding.horizontal() + label + arrow;
        let height = (style.text.line_height + style.padding.vertical()).max(style.min_height);
        Measured::new(Size::new(width.max(style.min_width), height))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, _bounds: Rect) {
        self.rtl = cx.rtl();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let bounds = cx.bounds();
        let mut segments = self.segments;
        segments.track(cx, event, |point| self.segment_at(bounds, point));
        self.segments = segments;
        let action = match self.press.event(cx, event) {
            PressOutcome::Activated(Some(point)) => self.click(cx.bounds(), point),
            PressOutcome::Activated(None) => self.button.handle_key(Key::Enter, &mut self.commands),
            PressOutcome::Handled => return EventReply::Stop,
            PressOutcome::Ignored => {
                if cx.phase() != EventPhase::Target
                    || hosted::key_down(event) != Some(Key::ArrowDown)
                    || cx.modifiers().is_chord()
                    || self.button.menu == ToolButtonMenu::None
                {
                    return EventReply::Continue;
                }
                ToolButtonAction::OpenMenu
            }
        };
        self.act(cx, action);
        EventReply::Stop
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = self
            .button
            .visual_state(&self.commands, cx.state)
            .set(VisualState::OPEN, self.open);
        let segments =
            [Segment::Label, Segment::Arrow].map(|part| self.segments.state(part, state));
        self.button.paint_segments(cx, state, Some(segments));
    }

    fn semantics(&self) -> SemanticNode {
        self.button.semantics(&self.commands)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        let action = match action {
            SemanticAction::Activate => self.button.handle_key(Key::Enter, &mut self.commands),
            SemanticAction::ShowMenu if !self.open => ToolButtonAction::OpenMenu,
            SemanticAction::ShowMenu => ToolButtonAction::None,
            _ => return false,
        };
        self.act(cx, action);
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        let _ = self.commands.set_enabled(self.button.command, enabled);
        if !enabled {
            self.press = Press::default();
        }
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Bool(checked) if self.button.checkable => self
                .commands
                .set_checked(self.button.command, *checked)
                .is_ok(),
            _ => false,
        }
    }

    fn set_open(&mut self, open: bool, _time: f64) -> bool {
        if self.button.menu == ToolButtonMenu::None {
            return false;
        }
        self.open = open;
        true
    }
}

/// The look of a command link: an arrow, a title, and a description line
/// in an item row. Behavior comes from a [`crate::Button`] on the same
/// node.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandLink {
    pub title: String,
    pub description: String,
}

impl CommandLink {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
        }
    }

    /// Paints the link in `state`; hover and press highlight the row.
    pub fn paint(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        let query = StyleQuery::new(ControlKind::Item, Slot::Root)
            .variant(ItemVariant::Outline)
            .state(state.set(VisualState::HIGHLIGHTED, true));
        let root = cx.style(query);
        let media = cx.style(query.slot(Slot::Media));
        let mut title = cx.style(query.slot(Slot::Title));
        title.foreground = cx.tokens.color(ColorRole::Primary);
        let description = cx.style(query.slot(Slot::Description));
        let bounds = cx.bounds;
        cx.slot(bounds, &root, |cx, inner| {
            let icon = media.icon_size;
            let (left, right) = if cx.rtl {
                (inner.max_x() - icon, inner.origin.x)
            } else {
                (inner.origin.x, inner.origin.x + icon + root.gap)
            };
            let arrow = if cx.rtl {
                Icon::ArrowLeft
            } else {
                Icon::ArrowRight
            };
            let mut arrow_style = media;
            arrow_style.foreground = title.foreground;
            cx.icon(
                Rect::from_xywh(left, inner.origin.y, icon, title.text.line_height),
                arrow,
                &arrow_style,
            );
            let text = Rect::from_xywh(
                right,
                inner.origin.y,
                (inner.size.width - icon - root.gap).max(0.0),
                title.text.line_height,
            );
            cx.label(text, &self.title, &title, TextAlign::Start);
            let below = text.translate(0.0, title.text.line_height + 4.0);
            cx.label(below, &self.description, &description, TextAlign::Start);
        });
    }

    /// The link for a control host, with a [`crate::Button`] for
    /// `command` that gives the behavior.
    pub fn hosted(self, command: CommandId) -> HostedCommandLink {
        HostedCommandLink {
            button: Button::new(self.title.clone(), command),
            link: self,
            pressed: false,
            enabled: true,
        }
    }
}

/// A [`CommandLink`] and its [`crate::Button`] on one hosted node.
/// `CommandLink` is only a look, so this type adds the button. It reports
/// `Activated`, and its node is the button's node with the link
/// description.
pub struct HostedCommandLink {
    link: CommandLink,
    button: Button,
    /// True while a press or Space holds the link down.
    pressed: bool,
    enabled: bool,
}

impl CustomControl for HostedCommandLink {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let query = StyleQuery::new(ControlKind::Item, Slot::Root).variant(ItemVariant::Outline);
        let root = cx.style(query);
        let media = cx.style(query.slot(Slot::Media));
        let title = cx.style(query.slot(Slot::Title));
        let description = cx.style(query.slot(Slot::Description));
        let text = cx
            .text
            .width(&self.link.title, &title.text)
            .max(cx.text.width(&self.link.description, &description.text));
        let width = root.padding.horizontal() + media.icon_size + root.gap + text;
        // The description line sits 4 pt under the title, as `paint` draws.
        let lines = title.text.line_height + 4.0 + description.text.line_height;
        let height = (root.padding.vertical() + lines).max(root.min_height);
        // In less room, the title and the description truncate.
        let size = Size::new(width.max(root.min_width), height);
        Measured::new(cx.constraints.constrain(size))
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() == EventPhase::Target {
            match event {
                ControlEvent::Pointer(PointerEvent::Down {
                    position,
                    button: PointerButton::Primary,
                }) => self.pressed = cx.bounds().contains(*position),
                ControlEvent::Key(KeyEvent::Down {
                    key: Key::Space, ..
                }) => self.pressed = true,
                ControlEvent::Pointer(PointerEvent::Up { .. })
                | ControlEvent::Key(KeyEvent::Up { key: Key::Space })
                | ControlEvent::FocusChanged(false)
                | ControlEvent::Dismiss => self.pressed = false,
                _ => {}
            }
        }
        self.button.event(cx, event)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx
            .state
            .set(VisualState::PRESSED, self.pressed && self.enabled);
        self.link.paint(cx, state);
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = CustomControl::semantics(&self.button);
        node.description.clone_from(&self.link.description);
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        self.button.perform(cx, action, value)
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.pressed &= enabled;
        CustomControl::set_enabled(&mut self.button, enabled);
    }
}
