//! Main-window shell: movable toolbars with embedded controls and
//! overflow, a status bar with temporary messages, and the composition
//! contract that places toolbars, status bar, and the dock area.
//!
//! On macOS the menu bar is the native global menu (`MainMenu`), so it
//! takes no space in the window.

use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::hosted::{self, Parts, Reveal};
use crate::menu::MenuResponse;
use crate::overlay::menu_popup::{CommandResult, MenuPopup};
use crate::popup::Placement;
use crate::style::fill_area;
use crate::style::{ControlKind, PaintContext, Sides, Slot, StyleQuery, TextAlign, VisualState};
use crate::{CommandId, CommandRegistry, CommandState, MenuItem};
use choscorui_core::{
    AnnouncementPriority, EventPhase, EventReply, Icon, Key, KeyEvent, LayoutDirection, Localizer,
    Measured, NodeId, Point, PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode,
    SemanticRole, SemanticState, Size,
};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolbarArea {
    Top,
    Bottom,
    Left,
    Right,
}

impl ToolbarArea {
    fn horizontal(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
    fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolbarItemKind {
    Command {
        command: CommandId,
        label: String,
    },
    /// An embedded control (search field, combo box) owned by the app.
    Control {
        node: NodeId,
        label: String,
        length: f64,
    },
    Separator,
    /// Pushes later items to the far end (horizontal toolbars only).
    FlexibleSpace,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolbarItem {
    /// Stable key used for customization persistence.
    pub key: String,
    pub kind: ToolbarItemKind,
    pub visible: bool,
}

impl ToolbarItem {
    pub fn command(key: impl Into<String>, label: impl Into<String>, command: CommandId) -> Self {
        Self {
            key: key.into(),
            kind: ToolbarItemKind::Command {
                command,
                label: label.into(),
            },
            visible: true,
        }
    }
    pub fn control(
        key: impl Into<String>,
        label: impl Into<String>,
        node: NodeId,
        length: f64,
    ) -> Self {
        Self {
            key: key.into(),
            kind: ToolbarItemKind::Control {
                node,
                label: label.into(),
                length,
            },
            visible: true,
        }
    }
    pub fn separator(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            kind: ToolbarItemKind::Separator,
            visible: true,
        }
    }
    pub fn flexible_space(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            kind: ToolbarItemKind::FlexibleSpace,
            visible: true,
        }
    }
    fn focusable(&self, commands: &CommandRegistry) -> bool {
        match &self.kind {
            ToolbarItemKind::Command { command, .. } => {
                commands.state(*command).is_ok_and(|state| state.enabled)
            }
            ToolbarItemKind::Control { .. } => true,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolbarLayout {
    pub rect: Rect,
    /// Placed items as (index into `items()`, rect).
    pub items: Vec<(usize, Rect)>,
    /// Items that did not fit, in order.
    pub overflow: Vec<usize>,
    pub overflow_button: Option<Rect>,
}

/// A toolbar: a single Tab stop with arrow-key (roving) focus between
/// items. Overflowing items move to a chevron menu.
#[derive(Clone, Debug)]
pub struct Toolbar {
    pub name: String,
    items: Vec<ToolbarItem>,
    area: ToolbarArea,
    /// Thickness across the toolbar.
    pub thickness: f64,
    pub button_length: f64,
    pub spacing: f64,
    focused: Option<usize>,
    /// The hovered and pressed command items of a hosted toolbar.
    parts: Parts,
}

impl Toolbar {
    pub fn new(name: impl Into<String>, items: Vec<ToolbarItem>) -> Self {
        Self {
            name: name.into(),
            items,
            area: ToolbarArea::Top,
            thickness: 38.0,
            button_length: 32.0,
            spacing: 4.0,
            focused: None,
            parts: Parts::default(),
        }
    }
    pub fn items(&self) -> &[ToolbarItem] {
        &self.items
    }
    pub fn area(&self) -> ToolbarArea {
        self.area
    }
    /// Moves the toolbar to another edge (from a drag or a menu command).
    pub fn move_to(&mut self, area: ToolbarArea) {
        self.area = area;
    }
    pub fn set_item_visible(&mut self, key: &str, visible: bool) -> bool {
        match self.items.iter_mut().find(|item| item.key == key) {
            Some(item) => {
                item.visible = visible;
                if !visible && self.focused.is_some_and(|f| self.items[f].key == key) {
                    self.focused = None;
                }
                true
            }
            None => false,
        }
    }
    /// Reorders by key (customization). Unknown keys are ignored and
    /// items not listed keep their relative order at the end.
    pub fn set_order(&mut self, keys: &[&str]) {
        let mut ordered = Vec::with_capacity(self.items.len());
        for key in keys {
            if let Some(index) = self.items.iter().position(|item| item.key == *key) {
                ordered.push(self.items.remove(index));
            }
        }
        ordered.append(&mut self.items);
        self.items = ordered;
        self.focused = None;
    }

    fn item_length(&self, item: &ToolbarItem) -> f64 {
        match &item.kind {
            ToolbarItemKind::Command { .. } => self.button_length,
            ToolbarItemKind::Control { length, .. } => *length,
            ToolbarItemKind::Separator => 9.0,
            ToolbarItemKind::FlexibleSpace => 0.0,
        }
    }

    /// Places items along `rect`. RTL mirrors horizontal toolbars.
    pub fn layout(&self, rect: Rect, direction: LayoutDirection) -> ToolbarLayout {
        let horizontal = self.area.horizontal();
        let total = if horizontal {
            rect.size.width
        } else {
            rect.size.height
        } - 2.0 * self.spacing;
        let visible: Vec<usize> = (0..self.items.len())
            .filter(|index| self.items[*index].visible)
            .collect();
        let needed: f64 = visible
            .iter()
            .map(|index| self.item_length(&self.items[*index]) + self.spacing)
            .sum::<f64>()
            - self.spacing;
        let mut placed = Vec::new();
        let mut overflow = Vec::new();
        let chevron = self.button_length * 0.6;
        let mut budget = if needed > total {
            total - chevron - self.spacing
        } else {
            total
        };
        for index in &visible {
            let length = self.item_length(&self.items[*index]);
            if overflow.is_empty() && length <= budget {
                placed.push((*index, length));
                budget -= length + self.spacing;
            } else if !matches!(
                self.items[*index].kind,
                ToolbarItemKind::Separator | ToolbarItemKind::FlexibleSpace
            ) {
                overflow.push(*index);
            }
        }
        if !overflow.is_empty() {
            // No dangling separator before the chevron.
            while placed.last().is_some_and(|(index, _)| {
                matches!(
                    self.items[*index].kind,
                    ToolbarItemKind::Separator | ToolbarItemKind::FlexibleSpace
                )
            }) {
                placed.pop();
            }
        }
        // Flexible spaces share whatever is left over when nothing overflows.
        let spaces = placed
            .iter()
            .filter(|(index, _)| self.items[*index].kind == ToolbarItemKind::FlexibleSpace)
            .count();
        let extra = if overflow.is_empty() && spaces > 0 && horizontal {
            (budget + self.spacing).max(0.0) / spaces as f64
        } else {
            0.0
        };
        let mut offset = self.spacing;
        let mut items = Vec::new();
        for (index, mut length) in placed {
            if self.items[index].kind == ToolbarItemKind::FlexibleSpace {
                length += extra;
            }
            let item_rect = if horizontal {
                Rect::new(
                    Point::new(rect.origin.x + offset, rect.origin.y),
                    Size::new(length, rect.size.height),
                )
            } else {
                Rect::new(
                    Point::new(rect.origin.x, rect.origin.y + offset),
                    Size::new(rect.size.width, length),
                )
            };
            items.push((index, item_rect));
            offset += length + self.spacing;
        }
        let mut overflow_button = (!overflow.is_empty()).then(|| {
            if horizontal {
                Rect::new(
                    Point::new(rect.max_x() - self.spacing - chevron, rect.origin.y),
                    Size::new(chevron, rect.size.height),
                )
            } else {
                Rect::new(
                    Point::new(rect.origin.x, rect.max_y() - self.spacing - chevron),
                    Size::new(rect.size.width, chevron),
                )
            }
        });
        if horizontal && direction == LayoutDirection::Rtl {
            let mirror = |r: Rect| {
                Rect::new(
                    Point::new(rect.min_x() + rect.max_x() - r.max_x(), r.origin.y),
                    r.size,
                )
            };
            for (_, item_rect) in &mut items {
                *item_rect = mirror(*item_rect);
            }
            overflow_button = overflow_button.map(mirror);
        }
        ToolbarLayout {
            rect,
            items,
            overflow,
            overflow_button,
        }
    }

    /// Menu for the overflow chevron: commands become actions; embedded
    /// controls are listed by label and activate `control_command` so the
    /// app can present them (for example in a popover).
    pub fn overflow_menu(
        &self,
        layout: &ToolbarLayout,
        control_command: CommandId,
    ) -> Vec<MenuItem> {
        layout
            .overflow
            .iter()
            .filter_map(|index| match &self.items[*index].kind {
                ToolbarItemKind::Command { command, label } => {
                    Some(MenuItem::action(label.clone(), *command))
                }
                ToolbarItemKind::Control { label, .. } => {
                    Some(MenuItem::action(label.clone(), control_command))
                }
                _ => None,
            })
            .collect()
    }

    pub fn focused(&self) -> Option<usize> {
        self.focused
    }

    /// Roving focus among placed, enabled items. Tab entering the toolbar
    /// calls this with `Key::Home`. Enter/Space on a command returns it.
    pub fn handle_key(
        &mut self,
        key: Key,
        layout: &ToolbarLayout,
        commands: &CommandRegistry,
        direction: LayoutDirection,
    ) -> Option<CommandId> {
        let order: Vec<usize> = layout
            .items
            .iter()
            .map(|(index, _)| *index)
            .filter(|index| self.items[*index].focusable(commands))
            .collect();
        if order.is_empty() {
            self.focused = None;
            return None;
        }
        let current = self
            .focused
            .and_then(|focused| order.iter().position(|index| *index == focused));
        let rtl = direction == LayoutDirection::Rtl && self.area.horizontal();
        let (next, previous) = if self.area.horizontal() {
            if rtl {
                (Key::ArrowLeft, Key::ArrowRight)
            } else {
                (Key::ArrowRight, Key::ArrowLeft)
            }
        } else {
            (Key::ArrowDown, Key::ArrowUp)
        };
        let position = match key {
            Key::Home => 0,
            Key::End => order.len() - 1,
            k if k == next => current.map_or(0, |c| (c + 1) % order.len()),
            k if k == previous => {
                current.map_or(order.len() - 1, |c| (c + order.len() - 1) % order.len())
            }
            Key::Enter | Key::Space => {
                let index = order[current?];
                return match &self.items[index].kind {
                    ToolbarItemKind::Command { command, .. } => {
                        commands.activate(*command).ok().flatten()
                    }
                    _ => None,
                };
            }
            _ => return None,
        };
        self.focused = Some(order[position]);
        None
    }

    /// Paints the toolbar in `layout`: the bar with a border on the edge
    /// that faces the content, ghost buttons for commands, 1 pt
    /// separators, and the overflow chevron. The host paints embedded
    /// controls; `focus_visible` rings the focused item, an embedded
    /// control too.
    pub fn paint(
        &self,
        cx: &mut PaintContext<'_>,
        layout: &ToolbarLayout,
        commands: &CommandRegistry,
        focus_visible: bool,
    ) {
        let query = |slot| StyleQuery::new(ControlKind::Toolbar, slot);
        let mut bar = cx.style(query(Slot::Root));
        bar.border_sides = match self.area {
            ToolbarArea::Top => Sides::BOTTOM,
            ToolbarArea::Bottom => Sides::TOP,
            ToolbarArea::Left if cx.rtl => Sides::START,
            ToolbarArea::Left => Sides::END,
            ToolbarArea::Right if cx.rtl => Sides::END,
            ToolbarArea::Right => Sides::START,
        };
        cx.slot(layout.rect, &bar, |_, _| {});
        let horizontal = self.area.horizontal();
        let host = cx.state;
        for (index, rect) in &layout.items {
            match &self.items[*index].kind {
                ToolbarItemKind::Command { command, label } => {
                    let own = commands.state(*command).unwrap_or(CommandState {
                        enabled: false,
                        checked: false,
                    });
                    let state = self
                        .parts
                        .state(*index, host)
                        .set(VisualState::DISABLED, host.disabled() || !own.enabled)
                        .set(VisualState::CHECKED, own.checked)
                        .set(
                            VisualState::FOCUS_VISIBLE,
                            focus_visible && self.focused == Some(*index),
                        );
                    let style = cx.style(query(Slot::Item).state(state));
                    cx.slot(*rect, &style, |cx, inner| {
                        cx.label(inner, label, &style, TextAlign::Center);
                    });
                }
                ToolbarItemKind::Separator => {
                    let rule = cx.style(query(Slot::Separator));
                    let center = rect.center();
                    let line = if horizontal {
                        Rect::from_xywh(
                            center.x - rule.min_width / 2.0,
                            rect.origin.y + rule.padding.block_start,
                            rule.min_width,
                            (rect.size.height - rule.padding.vertical()).max(0.0),
                        )
                    } else {
                        Rect::from_xywh(
                            rect.origin.x + rule.padding.block_start,
                            center.y - rule.min_height / 2.0,
                            (rect.size.width - rule.padding.vertical()).max(0.0),
                            rule.min_height,
                        )
                    };
                    fill_area(cx.list, line, cx.rtl, &rule.background);
                }
                ToolbarItemKind::Control { .. }
                    if focus_visible && self.focused == Some(*index) =>
                {
                    // The host paints the control over its rect, so the
                    // ring of the item goes around the rect.
                    let state = VisualState::FOCUS_VISIBLE;
                    let style = cx.style(query(Slot::Item).state(state));
                    if let Some(ring) = style.ring {
                        let radius = style.radius.top_start;
                        let _ = cx.list.draw_ring(*rect, radius, ring.width, ring.color);
                    }
                }
                ToolbarItemKind::Control { .. } | ToolbarItemKind::FlexibleSpace => {}
            }
        }
        if let Some(rect) = layout.overflow_button {
            let style = cx.style(query(Slot::Action));
            let icon = match (horizontal, cx.rtl) {
                (false, _) => Icon::ChevronDown,
                (true, false) => Icon::ChevronsRight,
                (true, true) => Icon::ChevronsLeft,
            };
            cx.slot(rect, &style, |cx, inner| cx.icon(inner, icon, &style));
        }
    }

    pub fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Toolbar,
            name: self.name.clone(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        }
    }

    /// Persisted form: `<area>;<key>[,!hidden-key]...`.
    pub fn save(&self) -> String {
        let keys: Vec<String> = self
            .items
            .iter()
            .map(|item| {
                if item.visible {
                    item.key.clone()
                } else {
                    format!("!{}", item.key)
                }
            })
            .collect();
        format!("{};{}", self.area.name(), keys.join(","))
    }

    /// Restores area, order, and visibility; ignores unknown keys.
    pub fn restore(&mut self, text: &str) -> bool {
        let Some((area, keys)) = text.split_once(';') else {
            return false;
        };
        let Some(area) = [
            ToolbarArea::Top,
            ToolbarArea::Bottom,
            ToolbarArea::Left,
            ToolbarArea::Right,
        ]
        .into_iter()
        .find(|candidate| candidate.name() == area) else {
            return false;
        };
        self.area = area;
        let entries: Vec<&str> = keys.split(',').filter(|key| !key.is_empty()).collect();
        let order: Vec<&str> = entries
            .iter()
            .map(|key| key.trim_start_matches('!'))
            .collect();
        self.set_order(&order);
        for entry in entries {
            let key = entry.trim_start_matches('!');
            self.set_item_visible(key, !entry.starts_with('!'));
        }
        true
    }
}

/// A permanent status-bar section, such as "Ln 12, Col 4".
#[derive(Clone, Debug, PartialEq)]
pub struct StatusSection {
    pub key: String,
    pub text: String,
    pub width: f64,
}

/// Status bar with a temporary-message slot on the leading side and
/// permanent sections on the trailing side.
#[derive(Clone, Debug, Default)]
pub struct StatusBar {
    pub height: f64,
    sections: Vec<StatusSection>,
    default_text: String,
    message: Option<(String, Option<Duration>)>,
    announcement: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StatusBarLayout {
    pub message: Rect,
    pub sections: Vec<(usize, Rect)>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            height: 22.0,
            ..Self::default()
        }
    }
    pub fn set_section(&mut self, key: &str, text: impl Into<String>, width: f64) {
        let text = text.into();
        match self.sections.iter_mut().find(|section| section.key == key) {
            Some(section) => {
                section.text = text;
                section.width = width;
            }
            None => self.sections.push(StatusSection {
                key: key.to_owned(),
                text,
                width,
            }),
        }
    }
    pub fn remove_section(&mut self, key: &str) -> bool {
        let before = self.sections.len();
        self.sections.retain(|section| section.key != key);
        before != self.sections.len()
    }
    pub fn sections(&self) -> &[StatusSection] {
        &self.sections
    }
    /// Text shown when no temporary message is active.
    pub fn set_default_text(&mut self, text: impl Into<String>) {
        self.default_text = text.into();
    }

    /// Shows a temporary message until `now + timeout` (or until replaced
    /// or cleared when `timeout` is `None`). It is also queued for a
    /// low-priority screen-reader announcement.
    pub fn show_message(
        &mut self,
        text: impl Into<String>,
        timeout: Option<Duration>,
        now: Duration,
    ) {
        let text = text.into();
        self.announcement = Some(text.clone());
        self.message = Some((text, timeout.map(|timeout| now + timeout)));
    }
    pub fn clear_message(&mut self) {
        self.message = None;
    }
    /// Expires the message; returns the next deadline to wake for.
    pub fn poll(&mut self, now: Duration) -> Option<Duration> {
        match &self.message {
            Some((_, Some(deadline))) if *deadline <= now => {
                self.message = None;
                None
            }
            Some((_, Some(deadline))) => Some(*deadline - now),
            _ => None,
        }
    }
    pub fn text(&self) -> &str {
        self.message
            .as_ref()
            .map_or(self.default_text.as_str(), |(text, _)| text.as_str())
    }
    /// The message to pass to `Window::announce` (taken once).
    pub fn take_announcement(&mut self) -> Option<String> {
        self.announcement.take()
    }

    /// Sends the message of the last [`show_message`](Self::show_message)
    /// to the screen reader at low priority, once. A hosted bar does this
    /// itself; a control that wraps a bar calls it after `show_message`.
    pub fn announce(&mut self, cx: &mut ControlContext<'_>) {
        if let Some(text) = self.take_announcement() {
            cx.announce(text, AnnouncementPriority::Low);
        }
    }

    pub fn layout(&self, rect: Rect, direction: LayoutDirection) -> StatusBarLayout {
        let mut trailing = rect.max_x();
        let mut sections = Vec::new();
        for (index, section) in self.sections.iter().enumerate().rev() {
            let width = section.width.min(trailing - rect.min_x()).max(0.0);
            trailing -= width;
            sections.push((
                index,
                Rect::new(
                    Point::new(trailing, rect.origin.y),
                    Size::new(width, rect.size.height),
                ),
            ));
        }
        sections.reverse();
        let mut message = Rect::new(
            rect.origin,
            Size::new(trailing - rect.min_x(), rect.size.height),
        );
        if direction == LayoutDirection::Rtl {
            let mirror = |r: Rect| {
                Rect::new(
                    Point::new(rect.min_x() + rect.max_x() - r.max_x(), r.origin.y),
                    r.size,
                )
            };
            message = mirror(message);
            for (_, section) in &mut sections {
                *section = mirror(*section);
            }
        }
        StatusBarLayout { message, sections }
    }

    /// Paints the bar in the context's bounds with the `StatusBar` look:
    /// a top border, the message, and each section after a 1 pt rule.
    pub fn paint(&self, cx: &mut PaintContext<'_>, layout: &StatusBarLayout) {
        let style = cx.style(StyleQuery::new(ControlKind::StatusBar, Slot::Root));
        let rule = cx.style(StyleQuery::new(ControlKind::StatusBar, Slot::Separator));
        let bounds = cx.bounds;
        cx.slot(bounds, &style, |_, _| {});
        let inset = |rect: Rect| {
            Rect::from_xywh(
                rect.origin.x + style.padding.inline_start,
                rect.origin.y,
                (rect.size.width - style.padding.horizontal()).max(0.0),
                rect.size.height,
            )
        };
        cx.label(inset(layout.message), self.text(), &style, TextAlign::Start);
        for (index, section) in &layout.sections {
            let x = if cx.rtl {
                section.max_x() - rule.min_width
            } else {
                section.origin.x
            };
            let line = Rect::from_xywh(
                x,
                section.origin.y + rule.padding.block_start,
                rule.min_width,
                (section.size.height - rule.padding.vertical()).max(0.0),
            );
            fill_area(cx.list, line, cx.rtl, &rule.background);
            cx.label(
                inset(*section),
                &self.sections[*index].text,
                &style,
                TextAlign::Start,
            );
        }
    }

    /// The semantics, named in the language of `strings`.
    pub fn semantics(&self, strings: &Localizer) -> SemanticNode {
        let mut parts = vec![self.text().to_owned()];
        parts.extend(self.sections.iter().map(|section| section.text.clone()));
        parts.retain(|part| !part.is_empty());
        SemanticNode {
            role: SemanticRole::StatusBar,
            name: strings.message("choscorui.status", &[]),
            description: String::new(),
            value: Some(parts.join(", ")),
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Focus],
            text: None,
        }
    }
}

/// Frames of the main-window shell.
#[derive(Clone, Debug, PartialEq)]
pub struct MainWindowFrames {
    /// One rect per toolbar passed to `compose_main_window`, same order.
    pub toolbars: Vec<Rect>,
    pub status_bar: Option<Rect>,
    /// What remains for `DockLayout::arrange` (or the central widget).
    pub content: Rect,
}

/// Main-window composition: toolbars stack inward from their edge in the
/// given order (top/bottom span the full width, left/right sit between
/// them), the status bar spans the bottom edge below everything, and the
/// rest is content.
pub fn compose_main_window(
    bounds: Rect,
    toolbars: &[(ToolbarArea, f64)],
    status_height: Option<f64>,
) -> MainWindowFrames {
    let mut rest = bounds;
    let status_bar = status_height.map(|height| {
        let height = height.min(rest.size.height).max(0.0);
        rest.size.height -= height;
        Rect::new(
            Point::new(rest.origin.x, rest.max_y()),
            Size::new(rest.size.width, height),
        )
    });
    let mut frames = vec![Rect::new(bounds.origin, Size::new(0.0, 0.0)); toolbars.len()];
    for pass in [true, false] {
        for (index, (area, thickness)) in toolbars.iter().enumerate() {
            if area.horizontal() != pass {
                continue;
            }
            let length = if pass {
                rest.size.height
            } else {
                rest.size.width
            };
            let thickness = thickness.min(length).max(0.0);
            frames[index] = match area {
                ToolbarArea::Top => {
                    let frame = Rect::new(rest.origin, Size::new(rest.size.width, thickness));
                    rest.origin.y += thickness;
                    rest.size.height -= thickness;
                    frame
                }
                ToolbarArea::Bottom => {
                    rest.size.height -= thickness;
                    Rect::new(
                        Point::new(rest.origin.x, rest.max_y()),
                        Size::new(rest.size.width, thickness),
                    )
                }
                ToolbarArea::Left => {
                    let frame = Rect::new(rest.origin, Size::new(thickness, rest.size.height));
                    rest.origin.x += thickness;
                    rest.size.width -= thickness;
                    frame
                }
                ToolbarArea::Right => {
                    rest.size.width -= thickness;
                    Rect::new(
                        Point::new(rest.max_x(), rest.origin.y),
                        Size::new(thickness, rest.size.height),
                    )
                }
            };
        }
    }
    MainWindowFrames {
        toolbars: frames,
        status_bar,
        content: rest,
    }
}

impl Toolbar {
    /// The toolbar as a control for a [`crate::ControlHost`], with its own
    /// command states. Every command starts enabled and unchecked.
    pub fn hosted(self) -> HostedToolbar {
        let mut commands = CommandRegistry::new();
        for item in &self.items {
            if let ToolbarItemKind::Command { command, .. } = item.kind {
                // A command listed twice keeps one state.
                let _ = commands.insert(command, CommandState::default());
            }
        }
        HostedToolbar {
            toolbar: self,
            commands,
            layout: ToolbarLayout {
                rect: Rect::default(),
                items: Vec::new(),
                overflow: Vec::new(),
                overflow_button: None,
            },
            direction: LayoutDirection::Ltr,
            menu: MenuPopup::new(Vec::new()),
            menu_items: Vec::new(),
        }
    }

    /// The length of the visible items along the bar, with the spacing.
    fn natural_length(&self) -> f64 {
        let visible: Vec<&ToolbarItem> = self.items.iter().filter(|item| item.visible).collect();
        let items: f64 = visible.iter().map(|item| self.item_length(item)).sum();
        items + (visible.len() + 1) as f64 * self.spacing
    }
}

/// A [`Toolbar`] mounted in a [`crate::ControlHost`]: it owns the command
/// states, places the items in its bounds, and opens the overflowed
/// commands in a menu below the chevron. Make it with [`Toolbar::hosted`].
pub struct HostedToolbar {
    toolbar: Toolbar,
    commands: CommandRegistry,
    layout: ToolbarLayout,
    direction: LayoutDirection,
    menu: MenuPopup,
    /// The commands in the overflow menu.
    menu_items: Vec<CommandId>,
}

impl HostedToolbar {
    pub fn toolbar(&self) -> &Toolbar {
        &self.toolbar
    }

    /// Moves the bar to the `area` edge of the window.
    pub fn move_to(&mut self, area: ToolbarArea) {
        self.toolbar.move_to(area);
    }

    /// Sets the enabled and checked state of a toolbar command.
    pub fn set_command(&mut self, command: CommandId, state: CommandState) -> CommandResult {
        self.commands.set_enabled(command, state.enabled)?;
        self.commands.set_checked(command, state.checked)?;
        // The overflow menu lists only the overflowed commands.
        let _ = self.menu.set_command(command, state);
        Ok(())
    }

    pub fn command(&self, command: CommandId) -> Option<CommandState> {
        self.commands.state(command).ok()
    }

    /// The placed items, the overflow, and the chevron of the last
    /// arrangement. The view layer places embedded controls in the item
    /// rectangles.
    pub fn layout(&self) -> &ToolbarLayout {
        &self.layout
    }

    fn item_at(&self, point: Point) -> Option<usize> {
        self.layout
            .items
            .iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(index, _)| *index)
    }

    /// The enabled command item at `point`. The bar has no hover or press
    /// while the overflow menu is open.
    fn command_at(&self, point: Point) -> Option<usize> {
        let index = self.item_at(point).filter(|_| !self.menu.is_open())?;
        match self.toolbar.items[index].kind {
            ToolbarItemKind::Command { command, .. } => self
                .commands
                .state(command)
                .is_ok_and(|state| state.enabled)
                .then_some(index),
            _ => None,
        }
    }

    fn activate(&mut self, cx: &mut ControlContext<'_>, index: usize) {
        if let ToolbarItemKind::Command { command, .. } = self.toolbar.items[index].kind
            && let Ok(Some(command)) = self.commands.activate(command)
        {
            cx.emit(ControlMessage::Command(command));
        }
    }

    /// Opens the overflow menu below the chevron at host `time`. Returns
    /// false when nothing overflows.
    fn open_menu(&mut self, time: f64) -> bool {
        let Some(button) = self.layout.overflow_button else {
            return false;
        };
        if !self.menu.is_open() {
            // The menu opens away from the window edge of the toolbar.
            let rtl = self.direction == LayoutDirection::Rtl;
            let side = match (self.toolbar.area, rtl) {
                (ToolbarArea::Top, _) => Placement::Below,
                (ToolbarArea::Bottom, _) => Placement::Above,
                (ToolbarArea::Left, false) | (ToolbarArea::Right, true) => Placement::After,
                (ToolbarArea::Left, true) | (ToolbarArea::Right, false) => Placement::Before,
            };
            self.menu.open_beside(button, side, time, false);
        }
        true
    }

    /// Closes the overflow menu without a choice: reports
    /// `OpenChanged(false)` and `Dismissed`. The reply is `Stop` when the
    /// menu closed.
    fn dismiss_menu(&mut self, cx: &mut ControlContext<'_>) -> EventReply {
        let open = self.menu.is_open();
        if open {
            self.menu.close(cx.time());
            cx.emit(ControlMessage::OpenChanged(false));
            cx.request_paint();
            cx.request_semantics();
        }
        hosted::dismissed(cx, open)
    }

    fn respond(&mut self, cx: &mut ControlContext<'_>, response: MenuResponse) {
        if self.menu.finish(cx, response) {
            cx.emit(ControlMessage::OpenChanged(false));
            cx.request_semantics();
        }
        cx.request_paint();
    }

    /// `pressed` is the command item pressed before `event`.
    fn pointer(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: PointerEvent,
        pressed: Option<usize>,
    ) -> EventReply {
        if self.menu.wheel(&event) {
            cx.request_paint();
            return EventReply::Stop;
        }
        if self.menu.is_open() {
            let released = matches!(event, PointerEvent::Up { .. });
            if let Some(response) = self.menu.pointer(event.position(), released) {
                if released {
                    self.respond(cx, response);
                }
                cx.request_paint();
                return EventReply::Stop;
            }
        }
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => {
                if self
                    .layout
                    .overflow_button
                    .is_some_and(|button| button.contains(position))
                {
                    if self.menu.is_open() {
                        self.dismiss_menu(cx);
                    } else if self.open_menu(cx.time()) {
                        cx.emit(ControlMessage::OpenChanged(true));
                        cx.request_paint();
                        cx.request_semantics();
                    }
                    return EventReply::Stop;
                }
                if self.menu.is_open() {
                    // A press on the bar outside the menu only closes it.
                    return self.dismiss_menu(cx);
                }
                let Some(index) = self.item_at(position) else {
                    return EventReply::Continue;
                };
                if self.toolbar.items[index].focusable(&self.commands) {
                    self.toolbar.focused = Some(index);
                }
                cx.request_paint();
                EventReply::Stop
            }
            PointerEvent::Up { .. } => {
                let Some(index) = pressed else {
                    return EventReply::Continue;
                };
                if self.toolbar.parts.hovered == Some(index) {
                    self.activate(cx, index);
                }
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        if self.menu.is_open() {
            if key == Key::Tab {
                self.dismiss_menu(cx);
                return EventReply::Continue;
            }
            // Command and Control chords go on to the shortcuts.
            if cx.modifiers().is_chord() {
                return EventReply::Continue;
            }
            let response = self.menu.key(key);
            self.respond(cx, response);
            return EventReply::Stop;
        }
        let before = self.toolbar.focused;
        let command = self
            .toolbar
            .handle_key(key, &self.layout, &self.commands, self.direction);
        if let Some(command) = command {
            cx.emit(ControlMessage::Command(command));
        }
        if self.toolbar.focused != before {
            cx.request_paint();
        }
        match key {
            Key::Home
            | Key::End
            | Key::Enter
            | Key::Space
            | Key::ArrowLeft
            | Key::ArrowRight
            | Key::ArrowUp
            | Key::ArrowDown => EventReply::Stop,
            _ => EventReply::Continue,
        }
    }
}

impl CustomControl for HostedToolbar {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let length = self.toolbar.natural_length();
        let thickness = self.toolbar.thickness;
        let size = if self.toolbar.area.horizontal() {
            Size::new(length, thickness)
        } else {
            Size::new(thickness, length)
        };
        Measured::new(cx.constraints.constrain(size))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.direction = cx.direction;
        self.layout = self.toolbar.layout(bounds, cx.direction);
        let mut commands = Vec::new();
        let mut items = Vec::new();
        for index in &self.layout.overflow {
            if let ToolbarItemKind::Command { command, label } = &self.toolbar.items[*index].kind {
                commands.push(*command);
                items.push(MenuItem::action(label.clone(), *command));
            }
        }
        if commands != self.menu_items {
            // Embedded controls stay with the view layer, so the menu
            // lists only the overflowed commands.
            self.menu = MenuPopup::new(items);
            for command in &commands {
                if let Ok(state) = self.commands.state(*command) {
                    let _ = self.menu.set_command(*command, state);
                }
            }
            self.menu_items = commands;
        }
        self.menu.arrange(cx, 0.0);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let pressed = self.toolbar.parts.pressed;
        let mut parts = self.toolbar.parts;
        parts.track(cx, event, |point| self.command_at(point));
        self.toolbar.parts = parts;
        match event {
            ControlEvent::Pointer(pointer) => self.pointer(cx, *pointer, pressed),
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.key(cx, *key),
            ControlEvent::FocusChanged(true) => {
                // Focus entering the toolbar goes to the first item.
                if self.toolbar.focused.is_none() {
                    self.toolbar.handle_key(
                        Key::Home,
                        &self.layout,
                        &self.commands,
                        self.direction,
                    );
                }
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::FocusChanged(false) | ControlEvent::Dismiss => {
                cx.request_paint();
                self.dismiss_menu(cx)
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let focus_visible = cx.state.focus_visible();
        self.toolbar
            .paint(cx, &self.layout, &self.commands, focus_visible);
    }

    fn overlay(&self) -> Option<Rect> {
        self.menu.frame()
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        self.menu.paint(cx);
    }

    fn animating(&self, time: f64) -> bool {
        self.menu.animating(time)
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = self.toolbar.semantics();
        if self.layout.overflow_button.is_some() {
            node.state.expanded = Some(self.menu.is_open());
        }
        node
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.toolbar.parts.clear();
            self.menu.reveal = Reveal::default();
        }
    }

    fn set_open(&mut self, open: bool, time: f64) -> bool {
        if !open {
            self.menu.close(time);
            return self.layout.overflow_button.is_some();
        }
        self.open_menu(time)
    }
}

impl CustomControl for StatusBar {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(StyleQuery::new(ControlKind::StatusBar, Slot::Root));
        let message = cx.text.width(self.text(), &style.text) + style.padding.horizontal();
        let sections: f64 = self.sections.iter().map(|section| section.width).sum();
        Measured::new(
            cx.constraints
                .constrain(Size::new(message + sections, self.height)),
        )
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if *event != ControlEvent::Timer {
            return EventReply::Continue;
        }
        self.announce(cx);
        self.poll(Duration::from_secs_f64(cx.time().max(0.0)));
        cx.request_paint();
        cx.request_semantics();
        EventReply::Stop
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let direction = if cx.rtl {
            LayoutDirection::Rtl
        } else {
            LayoutDirection::Ltr
        };
        let layout = self.layout(cx.bounds, direction);
        StatusBar::paint(self, cx, &layout);
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        StatusBar::semantics(self, strings)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if action != SemanticAction::Focus {
            return false;
        }
        cx.request_focus();
        true
    }

    fn focusable(&self) -> bool {
        false
    }

    /// The expiry of a timed message, in host time: pass the host time as
    /// `now` to [`StatusBar::show_message`].
    /// A message that waits for its announcement is due at once.
    fn deadline(&self) -> Option<f64> {
        if self.announcement.is_some() {
            return Some(f64::NEG_INFINITY);
        }
        match &self.message {
            Some((_, Some(deadline))) => Some(deadline.as_secs_f64()),
            _ => None,
        }
    }
}
