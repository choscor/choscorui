//! Tool buttons, button groups, and command links, and the hosted
//! controls that give them a node of their own.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::{self, Press, PressOutcome};
use crate::style::{
    ControlKind, ItemVariant, Orientation, PaintContext, Slot, StyleQuery, TextAlign,
    ToggleVariant, VisualState, content_rect, join_segment,
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
        let split = self.menu == ToolButtonMenu::Split;
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
            cx.list.fill_rect(line, rule.background);
            let mut chevron = style;
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
    open: bool,
    rtl: bool,
}

impl HostedToolButton {
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

    /// A click at `point`, with the split arrow at the end of the reading
    /// order.
    fn click(&mut self, bounds: Rect, point: Point) -> ToolButtonAction {
        let x = if self.rtl {
            bounds.origin.x + bounds.max_x() - point.x
        } else {
            point.x
        };
        self.button.click(bounds, x, &mut self.commands)
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
        let action = match self.press.event(cx, event) {
            PressOutcome::Activated(Some(point)) => self.click(cx.bounds(), point),
            PressOutcome::Activated(None) => self.button.handle_key(Key::Enter, &mut self.commands),
            PressOutcome::Handled => return EventReply::Stop,
            PressOutcome::Ignored => {
                if cx.phase() != EventPhase::Target
                    || hosted::key_down(event) != Some(Key::ArrowDown)
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
            .set(VisualState::PRESSED, self.press.pressed)
            .set(VisualState::OPEN, self.open);
        self.button.paint(cx, state);
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

/// Exclusive or non-exclusive group of checkable commands (for example
/// text alignment buttons). Exclusive groups keep exactly one checked.
#[derive(Clone, Debug)]
pub struct ButtonGroup {
    members: Vec<CommandId>,
    exclusive: bool,
    orientation: Orientation,
}

impl ButtonGroup {
    pub fn new(members: Vec<CommandId>, exclusive: bool) -> Self {
        Self {
            members,
            exclusive,
            orientation: Orientation::Horizontal,
        }
    }

    /// Joins the members in a row (the default) or a column.
    pub fn with_orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    pub fn members(&self) -> &[CommandId] {
        &self.members
    }

    /// The flags the member recipe reads: the host's focus and hover flags
    /// in `host`, with the command's checked and disabled state.
    pub fn member_state(
        &self,
        commands: &CommandRegistry,
        member: CommandId,
        host: VisualState,
    ) -> VisualState {
        let state = commands.state(member).unwrap_or_default();
        host.set(VisualState::CHECKED, state.checked)
            .set(VisualState::DISABLED, !state.enabled)
    }

    /// Paints member `index` in the context's bounds with the outline
    /// toggle look. Inner corners are square, and each member after the
    /// first overlaps the border before it, so the members read as one
    /// control.
    pub fn paint_member(
        &self,
        cx: &mut PaintContext<'_>,
        index: usize,
        label: &str,
        state: VisualState,
    ) {
        let query = StyleQuery::new(ControlKind::ToggleGroup, Slot::Item)
            .variant(ToggleVariant::Outline)
            .state(state);
        let mut style = cx.style(query);
        let mut rect = cx.bounds;
        join_segment(
            &mut style,
            &mut rect,
            index,
            self.members.len(),
            self.orientation,
            cx.rtl,
        );
        cx.slot(rect, &style, |cx, inner| {
            cx.label(inner, label, &style, TextAlign::Center);
        });
    }
    pub fn checked(&self, commands: &CommandRegistry) -> Vec<CommandId> {
        self.members
            .iter()
            .copied()
            .filter(|id| commands.state(*id).is_ok_and(|state| state.checked))
            .collect()
    }
    /// Applies a click on `member`. Exclusive groups ignore unchecking the
    /// checked member.
    pub fn toggle(&self, commands: &mut CommandRegistry, member: CommandId) -> bool {
        if !self.members.contains(&member)
            || !commands.state(member).is_ok_and(|state| state.enabled)
        {
            return false;
        }
        let checked = commands.state(member).is_ok_and(|state| state.checked);
        if self.exclusive {
            if checked {
                return false;
            }
            for id in &self.members {
                let _ = commands.set_checked(*id, *id == member);
            }
        } else {
            let _ = commands.set_checked(member, !checked);
        }
        true
    }
    /// Arrow-key movement within an exclusive group.
    pub fn step(&self, commands: &mut CommandRegistry, forward: bool) -> Option<CommandId> {
        let current = self.checked(commands).first().copied();
        let index = current.and_then(|id| self.members.iter().position(|member| *member == id));
        let count = self.members.len();
        if count == 0 {
            return None;
        }
        let mut next = match (index, forward) {
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
            (None, _) => 0,
        };
        for _ in 0..count {
            let id = self.members[next];
            if commands.state(id).is_ok_and(|state| state.enabled) {
                self.toggle(commands, id);
                return Some(id);
            }
            next = if forward {
                (next + 1) % count
            } else {
                (next + count - 1) % count
            };
        }
        None
    }

    /// The group for a control host, with a label for each member. It
    /// keeps the members' enabled and checked state itself, in place of an
    /// application `CommandRegistry`.
    pub fn hosted(self, labels: Vec<String>) -> HostedButtonGroup {
        let mut commands = CommandRegistry::new();
        for member in &self.members {
            let _ = commands.insert(*member, CommandState::default());
        }
        HostedButtonGroup {
            group: self,
            labels,
            commands,
            focus: 0,
            pressed: None,
            hovered: None,
            rects: Vec::new(),
            rtl: false,
        }
    }
}

/// A [`ButtonGroup`] mounted in a control host as one node. `ButtonGroup`
/// has no node of its own and reads its members from an application
/// `CommandRegistry`, so this type keeps the member state and paints the
/// members. It reports `Changed(Indices)` with the checked members.
pub struct HostedButtonGroup {
    group: ButtonGroup,
    labels: Vec<String>,
    commands: CommandRegistry,
    focus: usize,
    pressed: Option<usize>,
    hovered: Option<usize>,
    /// Each member's rectangle, in visual order.
    rects: Vec<Rect>,
    rtl: bool,
}

impl HostedButtonGroup {
    /// Enables or disables member `index`. A disabled member does not
    /// take a click, a key, or an assistive action, and arrow keys skip
    /// it in an exclusive group.
    #[must_use]
    pub fn with_enabled(mut self, index: usize, enabled: bool) -> Self {
        if let Some(member) = self.group.members.get(index) {
            let _ = self.commands.set_enabled(*member, enabled);
        }
        self
    }

    fn checked_indices(&self) -> Vec<usize> {
        let checked = self.group.checked(&self.commands);
        (0..self.group.members.len())
            .filter(|index| checked.contains(&self.group.members[*index]))
            .collect()
    }

    fn label(&self, index: usize) -> &str {
        self.labels.get(index).map_or("", String::as_str)
    }

    fn changed(&self, cx: &mut ControlContext<'_>) {
        cx.emit(ControlMessage::Changed(ControlValue::Indices(
            self.checked_indices(),
        )));
        cx.request_paint();
        cx.request_semantics();
    }

    fn toggle(&mut self, cx: &mut ControlContext<'_>, index: usize) {
        let Some(member) = self.group.members.get(index).copied() else {
            return;
        };
        if self.group.toggle(&mut self.commands, member) {
            self.changed(cx);
        }
    }

    fn horizontal(&self) -> bool {
        self.group.orientation == Orientation::Horizontal
    }

    /// True for the key that moves forward in reading order, false for the
    /// key that moves back, and `None` for other keys.
    fn direction(&self, key: Key) -> Option<bool> {
        let (next, previous) = match (self.horizontal(), self.rtl) {
            (true, false) => (Key::ArrowRight, Key::ArrowLeft),
            (true, true) => (Key::ArrowLeft, Key::ArrowRight),
            (false, _) => (Key::ArrowDown, Key::ArrowUp),
        };
        (key == next || key == previous).then_some(key == next)
    }

    /// Moves the focus with `key`. In an exclusive group the check moves
    /// with it.
    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        let count = self.group.members.len();
        if self.group.exclusive {
            let Some(forward) = self.direction(key) else {
                return EventReply::Continue;
            };
            if let Some(member) = self.group.step(&mut self.commands, forward) {
                self.focus = self
                    .group
                    .members
                    .iter()
                    .position(|id| *id == member)
                    .unwrap_or(0);
                self.changed(cx);
            }
            return EventReply::Stop;
        }
        match hosted::rove(key, self.focus, count, self.horizontal(), self.rtl) {
            Some(next) => {
                self.focus = next;
                cx.request_paint();
                cx.request_semantics();
                EventReply::Stop
            }
            None => EventReply::Continue,
        }
    }
}

impl CustomControl for HostedButtonGroup {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(
            StyleQuery::new(ControlKind::ToggleGroup, Slot::Item).variant(ToggleVariant::Outline),
        );
        let height = (style.text.line_height + style.padding.vertical()).max(style.min_height);
        let widths = (0..self.group.members.len()).map(|index| {
            let label = cx.text.width(self.label(index), &style.text);
            (style.padding.horizontal() + label).max(style.min_width)
        });
        let count = self.group.members.len() as f64;
        Measured::new(if self.horizontal() {
            Size::new(widths.sum(), height)
        } else {
            Size::new(widths.fold(0.0, f64::max), height * count)
        })
    }

    /// Divides the bounds evenly. The first member is at the start of the
    /// reading order.
    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let count = self.group.members.len();
        let horizontal = self.horizontal();
        self.rtl = cx.rtl();
        let (width, height) = if horizontal {
            (bounds.size.width / count.max(1) as f64, bounds.size.height)
        } else {
            (bounds.size.width, bounds.size.height / count.max(1) as f64)
        };
        self.rects = (0..count)
            .map(|index| {
                let step = index as f64;
                match (horizontal, cx.rtl()) {
                    (true, false) => bounds.translate(step * width, 0.0),
                    (true, true) => bounds.translate(bounds.size.width - (step + 1.0) * width, 0.0),
                    (false, _) => bounds.translate(0.0, step * height),
                }
            })
            .map(|rect| Rect::from_xywh(rect.origin.x, rect.origin.y, width, height))
            .collect();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => {
                let Some(index) = hosted::hit(&self.rects, *position) else {
                    return EventReply::Continue;
                };
                self.pressed = Some(index);
                self.focus = index;
                cx.capture_pointer(true);
                cx.request_paint();
                cx.request_semantics();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) if self.pressed.is_some() => {
                cx.capture_pointer(false);
                let hit = hosted::hit(&self.rects, *position);
                if let Some(index) = self.pressed.take().filter(|index| hit == Some(*index)) {
                    self.toggle(cx, index);
                }
                cx.request_paint();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                let hovered = hosted::hit(&self.rects, *position);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_paint();
                }
                EventReply::Continue
            }
            ControlEvent::Hover(false) => {
                self.hovered = None;
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Space | Key::Enter,
                repeat: false,
            }) => {
                self.toggle(cx, self.focus);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.key(cx, *key),
            ControlEvent::FocusChanged(false) | ControlEvent::Dismiss => {
                if self.pressed.take().is_some() {
                    cx.capture_pointer(false);
                }
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let bounds = cx.bounds;
        for (index, (member, rect)) in self.group.members.iter().zip(&self.rects).enumerate() {
            let own = VisualState::NONE
                .set(VisualState::HOVERED, self.hovered == Some(index))
                .set(VisualState::PRESSED, self.pressed == Some(index))
                .set(
                    VisualState::FOCUS_VISIBLE,
                    host.focus_visible() && self.focus == index,
                )
                .set(VisualState::DISABLED, host.disabled());
            let state = self.group.member_state(&self.commands, *member, own);
            cx.bounds = *rect;
            self.group.paint_member(cx, index, self.label(index), state);
        }
        cx.bounds = bounds;
    }

    /// The node of the focused member, as a checkable tool button.
    fn semantics(&self) -> SemanticNode {
        let Some(member) = self.group.members.get(self.focus) else {
            return hosted::node(SemanticRole::Group, "", Vec::new());
        };
        let state = self.commands.state(*member).unwrap_or_default();
        let actions = if state.enabled {
            vec![SemanticAction::Activate, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Checkbox, self.label(self.focus), actions);
        node.state.checked = Some(state.checked);
        node.state.disabled = !state.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate => self.toggle(cx, self.focus),
            SemanticAction::Focus => cx.request_focus(),
            _ => return false,
        }
        true
    }

    fn focusable(&self) -> bool {
        !self.group.members.is_empty()
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.pressed = None;
            self.hovered = None;
        }
    }

    /// Takes the checked members. An exclusive group takes exactly one.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Indices(indices) = value else {
            return false;
        };
        let count = self.group.members.len();
        if indices.iter().any(|index| *index >= count)
            || (self.group.exclusive && indices.len() != 1)
        {
            return false;
        }
        for (index, member) in self.group.members.iter().enumerate() {
            let _ = self.commands.set_checked(*member, indices.contains(&index));
        }
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
        Measured::new(Size::new(width.max(root.min_width), height))
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
