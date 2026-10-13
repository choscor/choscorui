//! Button groups: joined toggle and button members with separators and
//! text parts, and the hosted control that gives a group a node.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::{self, Parts};
use crate::style::{
    ButtonVariant, Content, ControlKind, ControlSize, Orientation, PaintContext, ResolvedStyle,
    Slot, StyleQuery, TextAlign, ToggleVariant, VisualState, join_segment,
};
use crate::{CommandId, CommandRegistry, CommandState};
use choscorui_core::{
    EventPhase, EventReply, Key, KeyEvent, Measured, PointerEvent, Rect, SemanticAction,
    SemanticNode, SemanticRole, Size,
};

/// One member of a [`ButtonGroup`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonGroupMember {
    /// A checkable command with the outline toggle look.
    Toggle(CommandId),
    /// A command button with the look of `ButtonVariant`. A click runs
    /// the command; the member never stays checked.
    Button(CommandId, ButtonVariant),
    /// A 1 pt line between two members. It takes no focus.
    Separator,
    /// A text part with the `ButtonGroup` `Text` look, for example a unit
    /// or a prefix. It takes no focus.
    Text,
}

impl ButtonGroupMember {
    /// The command of a toggle or button member.
    pub fn command(self) -> Option<CommandId> {
        match self {
            Self::Toggle(command) | Self::Button(command, _) => Some(command),
            Self::Separator | Self::Text => None,
        }
    }

    /// The style query of the member in `state` and `size`.
    fn query(self, state: VisualState, size: ControlSize) -> StyleQuery {
        match self {
            Self::Toggle(_) => StyleQuery::new(ControlKind::ToggleGroup, Slot::Item)
                .variant(ToggleVariant::Outline),
            Self::Button(_, variant) => {
                StyleQuery::new(ControlKind::Button, Slot::Root).variant(variant)
            }
            Self::Separator => StyleQuery::new(ControlKind::ButtonGroup, Slot::Separator),
            Self::Text => StyleQuery::new(ControlKind::ButtonGroup, Slot::Text),
        }
        .size(size)
        .state(state)
    }

    /// The natural size of the member with `label` in `style`.
    fn size(self, style: &ResolvedStyle, label: &str, cx: &MeasureContext<'_>) -> Size {
        match self {
            Self::Separator => Size::new(style.min_width, style.min_height),
            Self::Toggle(_) => {
                let width = style.padding.horizontal() + cx.text.width(label, &style.text);
                let height = style.text.line_height + style.padding.vertical();
                Size::new(width.max(style.min_width), height.max(style.min_height))
            }
            Self::Button(..) | Self::Text => Content::label(label).size(style, cx.text),
        }
    }
}

/// A group of joined members: checkable commands (for example text
/// alignment buttons), command buttons, separators, and text parts.
/// Exclusive groups keep exactly one toggle member checked.
#[derive(Clone, Debug)]
pub struct ButtonGroup {
    members: Vec<ButtonGroupMember>,
    exclusive: bool,
    orientation: Orientation,
    size: ControlSize,
}

impl ButtonGroup {
    /// A group of toggle members, one for each command.
    pub fn new(members: Vec<CommandId>, exclusive: bool) -> Self {
        Self::from_members(
            members.into_iter().map(ButtonGroupMember::Toggle).collect(),
            exclusive,
        )
    }

    /// A group of any members. `exclusive` applies to the toggle members.
    pub fn from_members(members: Vec<ButtonGroupMember>, exclusive: bool) -> Self {
        Self {
            members,
            exclusive,
            orientation: Orientation::Horizontal,
            size: ControlSize::Default,
        }
    }

    /// The size of the toggle, button, and text members: `Sm`,
    /// `Default`, or `Lg`.
    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Joins the members in a row (the default) or a column.
    pub fn with_orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    pub fn members(&self) -> &[ButtonGroupMember] {
        &self.members
    }

    /// The commands of the toggle members, in order.
    fn toggles(&self) -> impl Iterator<Item = CommandId> + '_ {
        self.members.iter().filter_map(|member| match member {
            ButtonGroupMember::Toggle(command) => Some(*command),
            _ => None,
        })
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

    /// Paints member `index` in the context's bounds. A toggle member has
    /// the outline toggle look, and a button member the look of its
    /// variant. Inner corners are square, and each member after the first
    /// overlaps the border before it, so the members read as one control.
    /// A member after a separator does not overlap the separator.
    pub fn paint_member(
        &self,
        cx: &mut PaintContext<'_>,
        index: usize,
        label: &str,
        state: VisualState,
    ) {
        let Some(member) = self.members.get(index).copied() else {
            return;
        };
        let mut style = cx.style(member.query(state, self.size));
        let bounds = cx.bounds;
        if member == ButtonGroupMember::Separator {
            let line = match self.orientation {
                Orientation::Horizontal => Rect::from_xywh(
                    bounds.center().x - style.min_width / 2.0,
                    bounds.origin.y,
                    style.min_width,
                    bounds.size.height,
                ),
                Orientation::Vertical => Rect::from_xywh(
                    bounds.origin.x,
                    bounds.center().y - style.min_height / 2.0,
                    bounds.size.width,
                    style.min_height,
                ),
            };
            cx.slot(line, &style, |_, _| {});
            return;
        }
        let mut rect = bounds;
        join_segment(
            &mut style,
            &mut rect,
            index,
            self.members.len(),
            self.orientation,
            cx.rtl,
        );
        if index > 0 && self.members[index - 1] == ButtonGroupMember::Separator {
            rect = bounds;
        }
        let align = if member == ButtonGroupMember::Text {
            TextAlign::Start
        } else {
            TextAlign::Center
        };
        cx.slot(rect, &style, |cx, inner| {
            cx.label(inner, label, &style, align);
        });
    }

    /// The commands of the checked toggle members.
    pub fn checked(&self, commands: &CommandRegistry) -> Vec<CommandId> {
        self.toggles()
            .filter(|id| commands.state(*id).is_ok_and(|state| state.checked))
            .collect()
    }

    /// Applies a click on the toggle member of `member`. Exclusive groups
    /// ignore unchecking the checked member. Other members do nothing.
    pub fn toggle(&self, commands: &mut CommandRegistry, member: CommandId) -> bool {
        if !self.toggles().any(|id| id == member)
            || !commands.state(member).is_ok_and(|state| state.enabled)
        {
            return false;
        }
        let checked = commands.state(member).is_ok_and(|state| state.checked);
        if self.exclusive {
            if checked {
                return false;
            }
            for id in self.toggles() {
                let _ = commands.set_checked(id, id == member);
            }
        } else {
            let _ = commands.set_checked(member, !checked);
        }
        true
    }

    /// Arrow-key movement among the toggle members of an exclusive group.
    pub fn step(&self, commands: &mut CommandRegistry, forward: bool) -> Option<CommandId> {
        let toggles: Vec<CommandId> = self.toggles().collect();
        let current = self.checked(commands).first().copied();
        let index = current.and_then(|id| toggles.iter().position(|member| *member == id));
        let count = toggles.len();
        if count == 0 {
            return None;
        }
        let mut next = match (index, forward) {
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
            (None, _) => 0,
        };
        for _ in 0..count {
            let id = toggles[next];
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

    /// The group for a control host, with a label for each member. A
    /// separator ignores its label. It keeps the members' enabled and
    /// checked state itself, in place of an application
    /// `CommandRegistry`.
    pub fn hosted(self, labels: Vec<String>) -> HostedButtonGroup {
        let mut commands = CommandRegistry::new();
        for command in self.members.iter().filter_map(|member| member.command()) {
            let _ = commands.insert(command, CommandState::default());
        }
        let focus = self
            .members
            .iter()
            .position(|member| member.command().is_some())
            .unwrap_or(0);
        HostedButtonGroup {
            group: self,
            labels,
            commands,
            focus,
            parts: Parts::default(),
            rects: Vec::new(),
            rtl: false,
        }
    }
}

/// A [`ButtonGroup`] mounted in a control host as one node. `ButtonGroup`
/// has no node of its own and reads its members from an application
/// `CommandRegistry`, so this type keeps the member state and paints the
/// members. It reports `Changed(Indices)` with the checked toggle
/// members, and `ItemActivated(index)` when a button member runs.
pub struct HostedButtonGroup {
    group: ButtonGroup,
    labels: Vec<String>,
    commands: CommandRegistry,
    /// The focused member: always a toggle or button member when the
    /// group has one.
    focus: usize,
    /// The hovered and pressed members.
    parts: Parts,
    /// Each member's rectangle, in member order.
    rects: Vec<Rect>,
    rtl: bool,
}

impl HostedButtonGroup {
    /// Enables or disables member `index`. A disabled member does not
    /// take a click, a key, or an assistive action, and arrow keys skip
    /// it in an exclusive group.
    #[must_use]
    pub fn with_enabled(mut self, index: usize, enabled: bool) -> Self {
        if let Some(command) = self.command(index) {
            let _ = self.commands.set_enabled(command, enabled);
        }
        self
    }

    fn command(&self, index: usize) -> Option<CommandId> {
        self.group.members.get(index)?.command()
    }

    /// True for an enabled toggle or button member.
    fn enabled(&self, index: usize) -> bool {
        self.command(index).is_some_and(|command| {
            self.commands
                .state(command)
                .is_ok_and(|state| state.enabled)
        })
    }

    /// The indices of the enabled toggle and button members.
    fn focusable_members(&self) -> Vec<usize> {
        (0..self.group.members.len())
            .filter(|index| self.enabled(*index))
            .collect()
    }

    fn checked_indices(&self) -> Vec<usize> {
        let checked = self.group.checked(&self.commands);
        self.group
            .members
            .iter()
            .enumerate()
            .filter(|(_, member)| {
                matches!(member, ButtonGroupMember::Toggle(id) if checked.contains(id))
            })
            .map(|(index, _)| index)
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

    /// Runs member `index`: a toggle member flips its check, and an
    /// enabled button member reports `ItemActivated(index)`.
    fn activate(&mut self, cx: &mut ControlContext<'_>, index: usize) {
        match self.group.members.get(index).copied() {
            Some(ButtonGroupMember::Toggle(member)) => {
                if self.group.toggle(&mut self.commands, member) {
                    self.changed(cx);
                }
            }
            Some(ButtonGroupMember::Button(member, _))
                if self.commands.state(member).is_ok_and(|state| state.enabled) =>
            {
                cx.emit(ControlMessage::ItemActivated(index));
            }
            _ => {}
        }
    }

    fn horizontal(&self) -> bool {
        self.group.orientation == Orientation::Horizontal
    }

    /// Moves the focus with `key` over the enabled members. In an
    /// exclusive group, a toggle member that gets the focus also gets the
    /// check. In other groups, Tab and Shift+Tab also move the focus until
    /// they pass the last or the first member.
    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        let stops = self.focusable_members();
        let current = stops
            .iter()
            .position(|index| *index == self.focus)
            .unwrap_or(0);
        let next = match key {
            Key::Tab if self.group.exclusive => None,
            Key::Tab => hosted::tab_stop(current, stops.len(), cx.modifiers().shift),
            _ => hosted::rove(key, current, stops.len(), self.horizontal(), self.rtl),
        };
        let Some(next) = next else {
            return EventReply::Continue;
        };
        self.focus = stops[next];
        if let (true, Some(ButtonGroupMember::Toggle(member))) =
            (self.group.exclusive, self.group.members.get(self.focus))
            && self.group.toggle(&mut self.commands, *member)
        {
            self.changed(cx);
        }
        cx.request_paint();
        cx.request_semantics();
        EventReply::Stop
    }

    /// The natural size of each member.
    fn sizes(&self, cx: &MeasureContext<'_>) -> Vec<Size> {
        self.group
            .members
            .iter()
            .enumerate()
            .map(|(index, member)| {
                let style = cx.style(member.query(VisualState::NONE, self.group.size));
                member.size(&style, self.label(index), cx)
            })
            .collect()
    }
}

impl CustomControl for HostedButtonGroup {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let sizes = self.sizes(cx);
        let cross = |size: &Size| {
            if self.horizontal() {
                size.height
            } else {
                size.width
            }
        };
        // A separator stretches across the group, so only the other
        // members set the cross size.
        let across = self
            .group
            .members
            .iter()
            .zip(&sizes)
            .filter(|(member, _)| **member != ButtonGroupMember::Separator)
            .map(|(_, size)| cross(size))
            .fold(0.0, f64::max);
        Measured::new(if self.horizontal() {
            Size::new(sizes.iter().map(|size| size.width).sum(), across)
        } else {
            Size::new(across, sizes.iter().map(|size| size.height).sum())
        })
    }

    /// Separators and text parts keep their natural length. The toggle and
    /// button members keep theirs too, and share the difference to the
    /// bounds evenly, so that each label shows whole when the group has its
    /// natural length. The first member is at the
    /// start of the reading order.
    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let horizontal = self.horizontal();
        self.rtl = cx.rtl();
        let sizes = self.sizes(cx);
        let main = |size: &Size| if horizontal { size.width } else { size.height };
        let natural: f64 = sizes.iter().map(main).sum();
        let shared = (self.group.members.iter())
            .filter(|member| member.command().is_some())
            .count()
            .max(1) as f64;
        let length = if horizontal {
            bounds.size.width
        } else {
            bounds.size.height
        };
        let share = (length - natural) / shared;
        let mut offset = 0.0;
        self.rects = self
            .group
            .members
            .iter()
            .zip(&sizes)
            .map(|(member, size)| {
                let step = if member.command().is_some() {
                    (main(size) + share).max(0.0)
                } else {
                    main(size)
                };
                let rect = match (horizontal, self.rtl) {
                    (true, false) => Rect::from_xywh(
                        bounds.origin.x + offset,
                        bounds.origin.y,
                        step,
                        bounds.size.height,
                    ),
                    (true, true) => Rect::from_xywh(
                        bounds.max_x() - offset - step,
                        bounds.origin.y,
                        step,
                        bounds.size.height,
                    ),
                    (false, _) => Rect::from_xywh(
                        bounds.origin.x,
                        bounds.origin.y + offset,
                        bounds.size.width,
                        step,
                    ),
                };
                offset += step;
                rect
            })
            .collect();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let pressed = self.parts.pressed;
        let mut parts = self.parts;
        parts.track(cx, event, |point| {
            hosted::hit(&self.rects, point).filter(|index| self.enabled(*index))
        });
        self.parts = parts;
        match event {
            ControlEvent::Pointer(PointerEvent::Down { .. }) => {
                let Some(index) = self.parts.pressed else {
                    return EventReply::Continue;
                };
                self.focus = index;
                cx.request_semantics();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) if pressed.is_some() => {
                if let Some(index) = pressed.filter(|index| self.parts.hovered == Some(*index)) {
                    self.activate(cx, index);
                }
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Space | Key::Enter,
                repeat: false,
            }) => {
                self.activate(cx, self.focus);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.key(cx, *key),
            ControlEvent::FocusChanged(focused) => {
                // Tab into a group with tab stops starts at its first
                // member, and Shift+Tab at its last. An exclusive group
                // starts at its checked member.
                if *focused && self.parts.pressed.is_none() {
                    let stops = self.focusable_members();
                    let edge = if self.group.exclusive {
                        let checked = self.checked_indices();
                        hosted::roving_entry(
                            self.group.members.len(),
                            &|index| self.enabled(index),
                            &|index| checked.contains(&index),
                        )
                    } else if cx.modifiers().shift {
                        stops.last().copied()
                    } else {
                        stops.first().copied()
                    };
                    self.focus = edge.unwrap_or(self.focus);
                }
                cx.request_paint();
                cx.request_semantics();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    /// Paints the `ButtonGroup` `Root` slot, then each member.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let bounds = cx.bounds;
        let root = cx.style(StyleQuery::new(ControlKind::ButtonGroup, Slot::Root).state(host));
        cx.slot(bounds, &root, |_, _| {});
        // While it shows the ring, the focused member paints last, so
        // that no neighbour covers the ring.
        let count = self.rects.len();
        let last = (host.focus_visible() && self.focus < count).then_some(self.focus);
        for index in (0..count).filter(|index| Some(*index) != last).chain(last) {
            let rect = &self.rects[index];
            let own = self.parts.state(index, host).set(
                VisualState::FOCUS_VISIBLE,
                host.focus_visible() && self.focus == index,
            );
            let state = match self.group.members[index] {
                ButtonGroupMember::Toggle(member) => {
                    self.group.member_state(&self.commands, member, own)
                }
                ButtonGroupMember::Button(member, _) => self
                    .group
                    .member_state(&self.commands, member, own)
                    .set(VisualState::CHECKED, false),
                _ => VisualState::NONE.set(VisualState::DISABLED, host.disabled()),
            };
            cx.bounds = *rect;
            self.group.paint_member(cx, index, self.label(index), state);
        }
        cx.bounds = bounds;
    }

    /// The node of the focused member: a checkable tool button for a
    /// toggle member, or a button. The description holds the labels of
    /// the text parts.
    fn semantics(&self) -> SemanticNode {
        let Some(command) = self.command(self.focus) else {
            return hosted::node(SemanticRole::Group, "", Vec::new());
        };
        let state = self.commands.state(command).unwrap_or_default();
        let actions = if state.enabled {
            vec![SemanticAction::Activate, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        // A toggle is a button with a checked state. In an exclusive group
        // it is a radio button.
        let toggle = matches!(self.group.members[self.focus], ButtonGroupMember::Toggle(_));
        let role = if toggle && self.group.exclusive {
            SemanticRole::RadioButton
        } else {
            SemanticRole::Button
        };
        let mut node = hosted::node(role, self.label(self.focus), actions);
        node.state.checked = toggle.then_some(state.checked);
        node.state.disabled = !state.enabled;
        node.description = self
            .group
            .members
            .iter()
            .enumerate()
            .filter(|(_, member)| **member == ButtonGroupMember::Text)
            .map(|(index, _)| self.label(index))
            .collect::<Vec<_>>()
            .join(", ");
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate => self.activate(cx, self.focus),
            SemanticAction::Focus => cx.request_focus(),
            _ => return false,
        }
        true
    }

    fn focusable(&self) -> bool {
        !self.focusable_members().is_empty()
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.parts.clear();
        }
    }

    /// Takes the checked toggle members. An exclusive group takes exactly
    /// one.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Indices(indices) = value else {
            return false;
        };
        let members = &self.group.members;
        if indices
            .iter()
            .any(|index| !matches!(members.get(*index), Some(ButtonGroupMember::Toggle(_))))
            || (self.group.exclusive && indices.len() != 1)
        {
            return false;
        }
        for (index, member) in members.iter().enumerate() {
            if let ButtonGroupMember::Toggle(command) = member {
                let _ = self
                    .commands
                    .set_checked(*command, indices.contains(&index));
            }
        }
        true
    }
}
