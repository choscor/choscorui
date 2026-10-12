//! Hosted menus: a context menu area and an in-window menu bar.

use super::menu_popup::{CommandResult, MenuPopup};
use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::hosted;
use crate::menu::{MenuItem, MenuResponse};
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState};
use crate::{CommandId, CommandState, Shortcut};
use choscorui_core::{
    EventPhase, EventReply, Key, KeyEvent, Measured, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, Size,
};

/// An area that opens a menu at the pointer on a secondary click or a
/// Control-click. Enter, Space, Shift-F10, the Menu key, or the `ShowMenu`
/// action opens it at the area's center with the first row highlighted. The area shows a hint text in a bordered box.
pub struct ContextMenu {
    name: String,
    hint: String,
    popup: MenuPopup,
    enabled: bool,
    /// The area's center from the last arrange, where the application
    /// opens the menu.
    center: Point,
}

impl ContextMenu {
    pub fn new(name: impl Into<String>, hint: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            name: name.into(),
            hint: hint.into(),
            popup: MenuPopup::new(items),
            enabled: true,
            center: Point::new(0.0, 0.0),
        }
    }

    pub fn is_open(&self) -> bool {
        self.popup.is_open()
    }

    /// Sets the enabled and checked state of a menu command.
    pub fn set_command(&mut self, command: CommandId, state: CommandState) -> CommandResult {
        self.popup.set_command(command, state)
    }

    pub fn command(&self, command: CommandId) -> Option<CommandState> {
        self.popup.commands.state(command).ok()
    }

    fn open(&mut self, cx: &mut ControlContext<'_>, at: Point, keyboard: bool) {
        let was_open = self.popup.is_open();
        self.popup.open(at, cx.time(), keyboard);
        if !was_open {
            cx.emit(ControlMessage::OpenChanged(true));
        }
        cx.request_paint();
        cx.request_semantics();
    }

    fn close(&mut self, cx: &mut ControlContext<'_>) {
        if self.popup.is_open() {
            self.popup.close(cx.time());
            cx.emit(ControlMessage::OpenChanged(false));
            cx.request_paint();
            cx.request_semantics();
        }
    }

    fn respond(&mut self, cx: &mut ControlContext<'_>, response: MenuResponse) {
        if self.popup.finish(cx, response) {
            cx.emit(ControlMessage::OpenChanged(false));
        }
        cx.request_paint();
        cx.request_semantics();
    }
}

impl CustomControl for ContextMenu {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let area = cx.style(StyleQuery::new(ControlKind::Menu, Slot::Placeholder));
        let text = cx.text.width(&self.hint, &area.text) + area.padding.horizontal();
        Measured {
            size: Size::new(area.min_width.max(text), area.min_height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let content = cx.style(StyleQuery::new(ControlKind::Menu, Slot::Content));
        self.popup.arrange(cx, content.min_width);
        self.center = bounds.center();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let bounds = cx.bounds();
        match event {
            ControlEvent::Pointer(PointerEvent::Down { position, button }) => {
                if self.popup.pointer(*position, false).is_some() {
                    return EventReply::Stop;
                }
                let secondary = *button == PointerButton::Secondary
                    || *button == PointerButton::Primary && cx.modifiers().control;
                if secondary && bounds.contains(*position) {
                    cx.request_focus();
                    self.open(cx, *position, false);
                    return EventReply::Stop;
                }
                self.close(cx);
                EventReply::Continue
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) if self.popup.is_open() => {
                if self.popup.pointer(*position, false).is_some() {
                    cx.request_paint();
                }
                EventReply::Continue
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) if self.popup.is_open() => {
                match self.popup.pointer(*position, true) {
                    Some(response) => {
                        self.respond(cx, response);
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                }
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                if !self.popup.is_open() {
                    let shortcut = Shortcut {
                        key: *key,
                        modifiers: cx.modifiers(),
                    };
                    let opens =
                        matches!(key, Key::Enter | Key::Space) || shortcut.opens_context_menu();
                    if opens {
                        self.open(cx, bounds.center(), true);
                        return EventReply::Stop;
                    }
                    return EventReply::Continue;
                }
                if *key == Key::Tab {
                    self.close(cx);
                    return EventReply::Continue;
                }
                let response = self.popup.menu.handle_key(*key, &self.popup.commands);
                self.respond(cx, response);
                EventReply::Stop
            }
            ControlEvent::Dismiss | ControlEvent::FocusChanged(false) => {
                self.close(cx);
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx.state.set(VisualState::OPEN, self.popup.is_open());
        let area = cx.style(StyleQuery::new(ControlKind::Menu, Slot::Placeholder).state(state));
        let hint = &self.hint;
        cx.slot(cx.bounds, &area, |cx, inner| {
            cx.label(inner, hint, &area, TextAlign::Center);
        });
    }

    fn overlay(&self) -> Option<Rect> {
        self.popup.frame()
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        self.popup.paint(cx);
    }

    fn animating(&self, time: f64) -> bool {
        self.popup.animating(time)
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![SemanticAction::ShowMenu, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Group, &self.name, actions);
        node.value = Some(self.hint.clone());
        if let Some(label) = self.popup.highlighted_label().filter(|_| self.is_open()) {
            node.description = label.to_string();
        }
        node.state.expanded = Some(self.popup.is_open());
        node.state.disabled = !self.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::ShowMenu => {
                let center = cx.bounds().center();
                self.open(cx, center, true);
                true
            }
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.popup.reveal = Default::default();
        }
    }

    /// Opens the menu at the area's center, as `ShowMenu` does.
    fn set_open(&mut self, open: bool, time: f64) -> bool {
        if !open {
            self.popup.close(time);
        } else if !self.enabled {
            return false;
        } else if !self.popup.is_open() {
            self.popup.open(self.center, time, true);
        }
        true
    }
}

/// One menu of a [`Menubar`]: its title and its items.
struct BarMenu {
    title: String,
    popup: MenuPopup,
}

/// A menu bar inside the window: a row of titles that open dropdown
/// menus. The application's native menu bar stays native.
pub struct Menubar {
    name: String,
    menus: Vec<BarMenu>,
    triggers: Vec<Rect>,
    /// The title that keys move between.
    current: usize,
    open: Option<usize>,
    enabled: bool,
    rtl: bool,
    /// The gap between a title and its menu.
    offset: f64,
}

impl Menubar {
    /// A bar with one menu per `(title, items)` pair.
    pub fn new(name: impl Into<String>, menus: Vec<(String, Vec<MenuItem>)>) -> Self {
        Self {
            name: name.into(),
            menus: menus
                .into_iter()
                .map(|(title, items)| BarMenu {
                    title,
                    popup: MenuPopup::new(items),
                })
                .collect(),
            triggers: Vec::new(),
            current: 0,
            open: None,
            enabled: true,
            rtl: false,
            offset: 0.0,
        }
    }

    /// The index of the open menu.
    pub fn open_menu(&self) -> Option<usize> {
        self.open
    }

    /// Sets the state of a command in every menu that lists it.
    pub fn set_command(&mut self, command: CommandId, state: CommandState) -> CommandResult {
        let mut result = Err(crate::CommandError::UnknownId);
        for menu in &mut self.menus {
            if menu.popup.set_command(command, state).is_ok() {
                result = Ok(());
            }
        }
        result
    }

    pub fn command(&self, command: CommandId) -> Option<CommandState> {
        self.menus
            .iter()
            .find_map(|menu| menu.popup.commands.state(command).ok())
    }

    fn query(slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Menubar, slot).state(state)
    }

    /// The top-start corner of menu `index` below its title.
    fn anchor(&self, index: usize) -> Point {
        let trigger = self.triggers[index];
        let x = if self.rtl {
            trigger.max_x()
        } else {
            trigger.origin.x
        };
        Point::new(x, trigger.max_y() + self.offset)
    }

    /// Opens menu `index` at `time` and closes another open menu. Returns
    /// false when the menu or its title rectangle does not exist.
    fn show(&mut self, index: usize, time: f64, keyboard: bool) -> bool {
        if index >= self.menus.len() || index >= self.triggers.len() {
            return false;
        }
        if let Some(old) = self.open.filter(|old| *old != index) {
            self.menus[old].popup.close(f64::NEG_INFINITY);
        }
        let anchor = self.anchor(index);
        self.menus[index].popup.open(anchor, time, keyboard);
        self.open = Some(index);
        self.current = index;
        true
    }

    /// Closes the open menu at `time`. Returns true when a menu was open.
    fn hide(&mut self, time: f64) -> bool {
        let Some(index) = self.open.take() else {
            return false;
        };
        self.menus[index].popup.close(time);
        true
    }

    fn open_at(&mut self, cx: &mut ControlContext<'_>, index: usize, keyboard: bool) {
        let was_open = self.open.is_some();
        if !self.show(index, cx.time(), keyboard) {
            return;
        }
        if !was_open {
            cx.emit(ControlMessage::OpenChanged(true));
        }
        cx.request_paint();
        cx.request_semantics();
    }

    fn close(&mut self, cx: &mut ControlContext<'_>) {
        if self.hide(cx.time()) {
            cx.emit(ControlMessage::OpenChanged(false));
            cx.request_paint();
            cx.request_semantics();
        }
    }

    fn respond(&mut self, cx: &mut ControlContext<'_>, index: usize, response: MenuResponse) {
        if self.menus[index].popup.finish(cx, response) {
            self.open = None;
            cx.emit(ControlMessage::OpenChanged(false));
        }
        cx.request_paint();
        cx.request_semantics();
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        let count = self.menus.len();
        let Some(index) = self.open else {
            if let Some(next) = hosted::rove(key, self.current, count, true, self.rtl) {
                self.current = next;
                cx.request_paint();
                cx.request_semantics();
                return EventReply::Stop;
            }
            return match key {
                Key::Enter | Key::Space | Key::ArrowDown => {
                    self.open_at(cx, self.current, true);
                    EventReply::Stop
                }
                _ => EventReply::Continue,
            };
        };
        let popup = &mut self.menus[index].popup;
        let on_submenu = popup.menu.highlighted().is_some_and(|at| {
            matches!(
                popup.menu.level_items(popup.menu.depth() - 1).get(at),
                Some(MenuItem::Submenu { .. })
            )
        });
        let nested = popup.menu.depth() > 1;
        match key {
            Key::Tab => {
                self.close(cx);
                EventReply::Continue
            }
            Key::Escape if !nested => {
                self.close(cx);
                cx.emit(ControlMessage::Dismissed);
                EventReply::Stop
            }
            Key::ArrowRight if on_submenu => self.forward(cx, index, key),
            Key::ArrowLeft if nested => self.forward(cx, index, key),
            Key::ArrowLeft | Key::ArrowRight => {
                // Left and right at the root move to the next title.
                if let Some(next) = hosted::rove(key, index, count, true, self.rtl) {
                    self.open_at(cx, next, true);
                }
                EventReply::Stop
            }
            _ => self.forward(cx, index, key),
        }
    }

    /// Sends `key` to menu `index`.
    fn forward(&mut self, cx: &mut ControlContext<'_>, index: usize, key: Key) -> EventReply {
        let popup = &mut self.menus[index].popup;
        let response = popup.menu.handle_key(key, &popup.commands);
        self.respond(cx, index, response);
        EventReply::Stop
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, event: &PointerEvent) -> EventReply {
        let position = event.position();
        if let Some(index) = self.open
            && let Some(response) = self.menus[index]
                .popup
                .pointer(position, matches!(event, PointerEvent::Up { .. }))
        {
            if matches!(event, PointerEvent::Up { .. }) {
                self.respond(cx, index, response);
            } else {
                cx.request_paint();
            }
            return EventReply::Stop;
        }
        let trigger = hosted::hit(&self.triggers, position);
        match *event {
            PointerEvent::Down {
                button: PointerButton::Primary,
                ..
            } => match trigger {
                Some(index) if self.open == Some(index) => {
                    self.close(cx);
                    EventReply::Stop
                }
                Some(index) => {
                    cx.request_focus();
                    self.open_at(cx, index, false);
                    EventReply::Stop
                }
                None => {
                    self.close(cx);
                    EventReply::Continue
                }
            },
            // Hover moves an open menu to another title.
            PointerEvent::Move { .. } => {
                if let (Some(open), Some(index)) = (self.open, trigger)
                    && open != index
                {
                    self.open_at(cx, index, false);
                }
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }
}

impl CustomControl for Menubar {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(Self::query(Slot::Root, VisualState::NONE));
        let trigger = cx.style(Self::query(Slot::Trigger, VisualState::NONE));
        let titles: f64 = self
            .menus
            .iter()
            .map(|menu| cx.text.width(&menu.title, &trigger.text) + trigger.padding.horizontal())
            .sum();
        let gaps = root.gap * self.menus.len().saturating_sub(1) as f64;
        let height = (trigger.text.line_height + trigger.padding.vertical())
            .max(root.min_height - root.padding.vertical())
            + root.padding.vertical();
        Measured {
            size: Size::new(titles + gaps + root.padding.horizontal(), height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let root = cx.style(Self::query(Slot::Root, VisualState::NONE));
        let trigger = cx.style(Self::query(Slot::Trigger, VisualState::NONE));
        let content = cx.style(Self::query(Slot::Content, VisualState::NONE));
        self.rtl = cx.rtl();
        self.offset = cx.tokens.spacing;
        let top = bounds.origin.y + root.padding.block_start;
        let height = bounds.size.height - root.padding.vertical();
        let mut x = if self.rtl {
            bounds.max_x() - root.padding.inline_start
        } else {
            bounds.origin.x + root.padding.inline_start
        };
        self.triggers = self
            .menus
            .iter()
            .map(|menu| {
                let width =
                    cx.text.width(&menu.title, &trigger.text) + trigger.padding.horizontal();
                if self.rtl {
                    x -= width;
                    let rect = Rect::from_xywh(x, top, width, height);
                    x -= root.gap;
                    rect
                } else {
                    let rect = Rect::from_xywh(x, top, width, height);
                    x += width + root.gap;
                    rect
                }
            })
            .collect();
        for menu in &mut self.menus {
            menu.popup.arrange(cx, content.min_width);
        }
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(pointer) => self.pointer(cx, pointer),
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.key(cx, *key),
            ControlEvent::Dismiss | ControlEvent::FocusChanged(false) => {
                self.close(cx);
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let root = cx.style(Self::query(
            Slot::Root,
            host.set(VisualState::HOVERED, false),
        ));
        cx.slot(cx.bounds, &root, |_, _| {});
        for (index, (menu, rect)) in self.menus.iter().zip(&self.triggers).enumerate() {
            let current = index == self.current;
            let state = VisualState::NONE
                .set(VisualState::OPEN, self.open == Some(index))
                .set(
                    VisualState::HIGHLIGHTED,
                    current && host.focused() && self.open.is_none(),
                )
                .set(VisualState::FOCUS_VISIBLE, current && host.focus_visible())
                .set(VisualState::DISABLED, host.disabled());
            let style = cx.style(Self::query(Slot::Trigger, state));
            cx.slot(*rect, &style, |cx, inner| {
                cx.label(inner, &menu.title, &style, TextAlign::Center);
            });
        }
    }

    fn overlay(&self) -> Option<Rect> {
        self.open.and_then(|index| self.menus[index].popup.frame())
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        for menu in &self.menus {
            menu.popup.paint(cx);
        }
    }

    fn animating(&self, time: f64) -> bool {
        self.menus.iter().any(|menu| menu.popup.animating(time))
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![
                SemanticAction::ShowMenu,
                if self.open.is_some() {
                    SemanticAction::Collapse
                } else {
                    SemanticAction::Expand
                },
                SemanticAction::Focus,
            ]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Menu, &self.name, actions);
        if let Some(menu) = self.menus.get(self.current) {
            node.value = Some(menu.title.clone());
            if let Some(label) = menu
                .popup
                .highlighted_label()
                .filter(|_| self.open.is_some())
            {
                node.description = label.to_string();
            }
        }
        node.state.expanded = Some(self.open.is_some());
        node.state.disabled = !self.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::ShowMenu | SemanticAction::Expand => {
                self.open_at(cx, self.current, true);
                true
            }
            SemanticAction::Collapse => {
                self.close(cx);
                true
            }
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.open = None;
            for menu in &mut self.menus {
                menu.popup.reveal = Default::default();
            }
        }
    }

    /// Opens the menu of the current title, as the keyboard does.
    fn set_open(&mut self, open: bool, time: f64) -> bool {
        if !open {
            self.hide(time);
            return true;
        }
        if !self.enabled {
            return false;
        }
        self.open.is_some() || self.show(self.current, time, true)
    }
}
