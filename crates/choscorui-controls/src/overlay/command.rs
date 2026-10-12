//! A command palette: a search input over grouped commands that filters
//! as the user types, inline or in a modal dialog.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::form::input::Editor;
use crate::hosted::{self, Press, PressOutcome, Reveal};
use crate::style::{
    ButtonVariant, ControlKind, ControlSize, Edges, PaintContext, ResolvedStyle, Slot, StyleQuery,
    TextAlign, VisualState,
};
use crate::text_field::TextField;
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Measured, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, Size, WheelUnit, fold_for_search,
};

/// The tallest the list grows before it scrolls, in points.
const LIST_MAX_HEIGHT: f64 = 300.0;

/// One command of a [`Command`] palette.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandItem {
    pub label: String,
    pub icon: Option<Icon>,
    /// Display text such as "⌘P", at the end of the row.
    pub shortcut: Option<String>,
    pub enabled: bool,
}

impl CommandItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            shortcut: None,
            enabled: true,
        }
    }

    #[must_use]
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    #[must_use]
    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// A command that shows but cannot run.
    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RowKind {
    Label(usize),
    /// An item by its index across all groups.
    Item(usize),
    Separator,
}

/// A row of the filtered list: its kind, top offset, and height.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Row {
    kind: RowKind,
    top: f64,
    height: f64,
}

/// Row heights read from the `Command` recipe.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Metrics {
    item: f64,
    label: f64,
    separator: f64,
    group_padding: f64,
    empty: f64,
}

/// The modal form: a trigger button and the reveal of the palette.
struct DialogForm {
    trigger: String,
    reveal: Reveal,
    press: Press,
}

/// A command palette: a search input and grouped commands. Typing
/// filters the commands by label. ArrowDown and ArrowUp move the
/// highlight, and Enter runs the highlighted command.
/// `Command::dialog` puts the palette in a modal dialog behind a button.
pub struct Command {
    name: String,
    groups: Vec<(String, Vec<CommandItem>)>,
    editor: Editor,
    empty: String,
    rows: Vec<Row>,
    /// The highlighted item, by its index across all groups.
    highlighted: Option<usize>,
    scroll: f64,
    metrics: Metrics,
    viewport: Rect,
    panel: Rect,
    input: Rect,
    text: Rect,
    list: Rect,
    dialog: Option<DialogForm>,
    motion_ms: u32,
    enabled: bool,
}

impl Command {
    /// An inline palette with `groups` of commands. The input shows
    /// `placeholder` while empty.
    pub fn new(
        name: impl Into<String>,
        placeholder: impl Into<String>,
        groups: Vec<(String, Vec<CommandItem>)>,
    ) -> Self {
        let name = name.into();
        let field = TextField::new(name.clone(), "").with_placeholder(placeholder);
        let mut command = Self {
            name,
            groups,
            editor: Editor::new(field),
            empty: String::new(),
            rows: Vec::new(),
            highlighted: None,
            scroll: 0.0,
            metrics: Metrics::default(),
            viewport: Rect::default(),
            panel: Rect::default(),
            input: Rect::default(),
            text: Rect::default(),
            list: Rect::default(),
            dialog: None,
            motion_ms: 0,
            enabled: true,
        };
        command.filter();
        command
    }

    /// The palette in a modal dialog that an outline button labeled
    /// `trigger` opens.
    pub fn dialog(
        trigger: impl Into<String>,
        name: impl Into<String>,
        placeholder: impl Into<String>,
        groups: Vec<(String, Vec<CommandItem>)>,
    ) -> Self {
        Self {
            dialog: Some(DialogForm {
                trigger: trigger.into(),
                reveal: Reveal::default(),
                press: Press::default(),
            }),
            ..Self::new(name, placeholder, groups)
        }
    }

    /// The text the list shows when no command matches.
    #[must_use]
    pub fn with_empty(mut self, text: impl Into<String>) -> Self {
        self.empty = text.into();
        self
    }

    /// The search text.
    pub fn query(&self) -> &str {
        self.editor.field.value()
    }

    pub fn is_open(&self) -> bool {
        self.dialog.as_ref().is_none_or(|dialog| dialog.reveal.open)
    }

    fn size(&self) -> ControlSize {
        if self.dialog.is_some() {
            ControlSize::Lg
        } else {
            ControlSize::Default
        }
    }

    fn style_query(&self, slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Command, slot)
            .size(self.size())
            .state(state)
    }

    fn items(&self) -> impl Iterator<Item = (usize, usize, &CommandItem)> {
        self.groups
            .iter()
            .enumerate()
            .flat_map(|(group, (_, items))| items.iter().map(move |item| (group, item)))
            .enumerate()
            .map(|(index, (group, item))| (index, group, item))
    }

    fn item(&self, index: usize) -> Option<&CommandItem> {
        self.items()
            .find(|(at, _, _)| *at == index)
            .map(|(_, _, item)| item)
    }

    /// The items that match the search text, in order.
    fn matches(&self) -> Vec<usize> {
        let needle = fold_for_search(&self.query().to_lowercase());
        self.items()
            .filter(|(_, _, item)| fold_for_search(&item.label.to_lowercase()).contains(&needle))
            .map(|(index, _, _)| index)
            .collect()
    }

    fn enabled_matches(&self) -> Vec<usize> {
        self.matches()
            .into_iter()
            .filter(|index| self.item(*index).is_some_and(|item| item.enabled))
            .collect()
    }

    /// Rebuilds the rows for the search text and highlights the first
    /// enabled match.
    fn filter(&mut self) {
        let matches = self.matches();
        let m = self.metrics;
        let mut rows = Vec::new();
        let mut top = 0.0;
        let mut first = 0;
        for (group, (label, items)) in self.groups.iter().enumerate() {
            let shown: Vec<usize> = (first..first + items.len())
                .filter(|index| matches.contains(index))
                .collect();
            first += items.len();
            if shown.is_empty() {
                continue;
            }
            if !rows.is_empty() {
                rows.push(Row {
                    kind: RowKind::Separator,
                    top,
                    height: m.separator,
                });
                top += m.separator;
            }
            top += m.group_padding;
            if !label.is_empty() {
                rows.push(Row {
                    kind: RowKind::Label(group),
                    top,
                    height: m.label,
                });
                top += m.label;
            }
            for index in shown {
                rows.push(Row {
                    kind: RowKind::Item(index),
                    top,
                    height: m.item,
                });
                top += m.item;
            }
            top += m.group_padding;
        }
        self.rows = rows;
        self.highlighted = self.enabled_matches().first().copied();
        self.scroll = 0.0;
    }

    fn content_height(&self) -> f64 {
        match self.rows.last() {
            Some(row) => row.top + row.height + self.metrics.group_padding,
            None if self.empty.is_empty() => 0.0,
            None => self.metrics.empty,
        }
    }

    fn list_height(&self) -> f64 {
        self.content_height().min(LIST_MAX_HEIGHT)
    }

    fn max_scroll(&self) -> f64 {
        (self.content_height() - self.list.size.height).max(0.0)
    }

    /// The row rectangle of item `index` with the scroll applied.
    fn item_rect(&self, index: usize) -> Option<Rect> {
        let row = self
            .rows
            .iter()
            .find(|row| row.kind == RowKind::Item(index))?;
        let inset = self.metrics.group_padding;
        Some(Rect::from_xywh(
            self.list.origin.x + inset,
            self.list.origin.y + row.top - self.scroll,
            self.list.size.width - 2.0 * inset,
            row.height,
        ))
    }

    fn item_at(&self, point: choscorui_core::Point) -> Option<usize> {
        if !self.list.contains(point) {
            return None;
        }
        self.rows.iter().find_map(|row| match row.kind {
            RowKind::Item(index)
                if self
                    .item_rect(index)
                    .is_some_and(|rect| rect.contains(point))
                    && self.item(index).is_some_and(|item| item.enabled) =>
            {
                Some(index)
            }
            _ => None,
        })
    }

    fn highlight(&mut self, index: Option<usize>) {
        self.highlighted = index;
        let Some(row) = index.and_then(|index| {
            self.rows
                .iter()
                .find(|row| row.kind == RowKind::Item(index))
                .copied()
        }) else {
            return;
        };
        let height = self.list.size.height;
        if row.top < self.scroll {
            self.scroll = row.top;
        } else if row.top + row.height > self.scroll + height {
            self.scroll = row.top + row.height - height;
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    /// Moves the highlight one enabled match forward or back. It stops at
    /// the ends.
    fn step(&mut self, forward: bool) {
        let matches = self.enabled_matches();
        let next = match self
            .highlighted
            .and_then(|current| matches.iter().position(|index| *index == current))
        {
            Some(at) if forward => matches.get(at + 1).or(matches.get(at)),
            Some(at) => matches.get(at.saturating_sub(1)),
            None if forward => matches.first(),
            None => matches.last(),
        };
        self.highlight(next.copied());
    }

    fn run(&mut self, cx: &mut ControlContext<'_>, index: usize) {
        cx.emit(ControlMessage::ItemActivated(index));
        if self.dialog.is_some() {
            self.change_open(cx, false);
        }
    }

    /// Opens or closes the dialog form at `time`; opening clears the
    /// search. Returns true when it changed.
    fn apply_open(&mut self, open: bool, time: f64) -> bool {
        let Some(dialog) = &mut self.dialog else {
            return false;
        };
        if dialog.reveal.open == open {
            return false;
        }
        dialog.reveal.set(open, time);
        if open {
            self.editor.replace("");
            self.filter();
        }
        true
    }

    fn change_open(&mut self, cx: &mut ControlContext<'_>, open: bool) {
        if !self.apply_open(open, cx.time()) {
            return;
        }
        cx.emit(ControlMessage::OpenChanged(open));
        if open {
            cx.request_focus();
        }
        cx.request_paint();
        cx.request_semantics();
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> Option<EventReply> {
        match key {
            Key::ArrowDown | Key::ArrowUp => self.step(key == Key::ArrowDown),
            Key::Enter => {
                if let Some(index) = self.highlighted {
                    self.run(cx, index);
                }
            }
            Key::Escape if self.dialog.is_some() => {
                self.change_open(cx, false);
                cx.emit(ControlMessage::Dismissed);
            }
            _ => return None,
        }
        cx.request_paint();
        cx.request_semantics();
        Some(EventReply::Stop)
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, event: &PointerEvent) -> Option<EventReply> {
        match *event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } if self.dialog.is_some() && !self.panel.contains(position) => {
                self.change_open(cx, false);
                cx.emit(ControlMessage::Dismissed);
                Some(EventReply::Stop)
            }
            PointerEvent::Down { position, .. } if self.list.contains(position) => {
                cx.request_focus();
                Some(EventReply::Stop)
            }
            PointerEvent::Up { position, .. } if self.list.contains(position) => {
                if let Some(index) = self.item_at(position) {
                    self.run(cx, index);
                }
                Some(EventReply::Stop)
            }
            PointerEvent::Move { position } => {
                if let Some(index) = self.item_at(position)
                    && self.highlighted != Some(index)
                {
                    self.highlighted = Some(index);
                    cx.request_paint();
                }
                None
            }
            PointerEvent::Wheel {
                position,
                delta,
                unit,
            } if self.list.contains(position) => {
                let step = match unit {
                    WheelUnit::Points => delta.y,
                    WheelUnit::Lines => delta.y * self.metrics.item,
                };
                self.scroll = (self.scroll + step).clamp(0.0, self.max_scroll());
                cx.request_paint();
                Some(EventReply::Stop)
            }
            _ => None,
        }
    }

    /// Places the input and the list inside `panel`.
    fn place(&mut self, cx: &MeasureContext<'_>, panel: Rect) {
        let root = cx.style(self.style_query(Slot::Root, VisualState::NONE));
        let input = cx.style(self.style_query(Slot::Input, VisualState::NONE));
        let icon = cx.style(self.style_query(Slot::Icon, VisualState::NONE));
        self.panel = panel;
        let inner = Rect::from_xywh(
            panel.origin.x + root.padding.inline_start,
            panel.origin.y + root.padding.block_start,
            panel.size.width - root.padding.horizontal(),
            panel.size.height - root.padding.vertical(),
        );
        self.input = Rect::from_xywh(
            inner.origin.x,
            inner.origin.y,
            inner.size.width,
            input.min_height,
        );
        let skip = input.padding.inline_start + icon.icon_size + input.gap;
        let width = (self.input.size.width - skip - input.padding.inline_end).max(0.0);
        let x = if cx.rtl() {
            self.input.max_x() - skip - width
        } else {
            self.input.origin.x + skip
        };
        self.text = Rect::from_xywh(x, self.input.origin.y, width, self.input.size.height);
        self.list = Rect::from_xywh(
            inner.origin.x,
            self.input.max_y(),
            inner.size.width,
            self.list_height(),
        );
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    fn natural_height(&self, cx: &MeasureContext<'_>) -> f64 {
        let root = cx.style(self.style_query(Slot::Root, VisualState::NONE));
        let input = cx.style(self.style_query(Slot::Input, VisualState::NONE));
        root.padding.vertical() + input.min_height + self.list_height()
    }

    fn paint_palette(&self, cx: &mut PaintContext<'_>, focused: bool) {
        let root = cx.style(self.style_query(Slot::Root, VisualState::NONE));
        cx.slot(self.panel, &root, |_, _| {});
        let state = self
            .editor
            .state(VisualState::NONE.set(VisualState::FOCUSED, focused));
        let input = cx.style(self.style_query(Slot::Input, state));
        let icon = cx.style(self.style_query(Slot::Icon, VisualState::NONE));
        let flat = ResolvedStyle {
            padding: Edges::ZERO,
            ..input
        };
        cx.slot(self.input, &flat, |_, _| {});
        let icon_x = if cx.rtl {
            self.input.max_x() - input.padding.inline_start - icon.icon_size
        } else {
            self.input.origin.x + input.padding.inline_start
        };
        let mark = Rect::from_xywh(
            icon_x,
            self.input.origin.y,
            icon.icon_size,
            self.input.size.height,
        );
        cx.icon(mark, Icon::Search, &icon);
        self.editor.paint_text(cx, self.text, state);
        self.paint_list(cx);
    }

    fn paint_list(&self, cx: &mut PaintContext<'_>) {
        let list = self.list;
        cx.list.push_clip(list);
        if self.rows.is_empty() && !self.empty.is_empty() {
            let style = cx.style(self.style_query(Slot::Description, VisualState::NONE));
            cx.label(list, &self.empty, &style, TextAlign::Center);
        }
        let inset = self.metrics.group_padding;
        for row in &self.rows {
            let rect = Rect::from_xywh(
                list.origin.x + inset,
                list.origin.y + row.top - self.scroll,
                list.size.width - 2.0 * inset,
                row.height,
            );
            if rect.max_y() < list.origin.y || rect.origin.y > list.max_y() {
                continue;
            }
            match row.kind {
                RowKind::Label(group) => {
                    let style = cx.style(self.style_query(Slot::GroupLabel, VisualState::NONE));
                    let label = &self.groups[group].0;
                    cx.slot(rect, &style, |cx, inner| {
                        cx.label(inner, label, &style, TextAlign::Start);
                    });
                }
                RowKind::Separator => {
                    let style = cx.style(self.style_query(Slot::Separator, VisualState::NONE));
                    let line = Rect::from_xywh(
                        list.origin.x,
                        rect.center().y - style.min_height / 2.0,
                        list.size.width,
                        style.min_height,
                    );
                    let rule = ResolvedStyle {
                        padding: Edges::ZERO,
                        ..style
                    };
                    cx.slot(line, &rule, |_, _| {});
                }
                RowKind::Item(index) => {
                    let Some(item) = self.item(index) else {
                        continue;
                    };
                    let state = VisualState::NONE
                        .set(VisualState::HIGHLIGHTED, self.highlighted == Some(index))
                        .set(VisualState::DISABLED, !item.enabled);
                    let style = cx.style(self.style_query(Slot::Item, state));
                    let shortcut = cx.style(self.style_query(Slot::Shortcut, state));
                    cx.slot(rect, &style, |cx, inner| {
                        let mut text = inner;
                        if let Some(icon) = item.icon {
                            let size = style.icon_size.max(16.0);
                            let x = if cx.rtl {
                                inner.max_x() - size
                            } else {
                                inner.origin.x
                            };
                            let mark = Rect::from_xywh(x, inner.origin.y, size, inner.size.height);
                            cx.icon(mark, icon, &style);
                            let skip = size + style.gap;
                            text = Rect::from_xywh(
                                if cx.rtl {
                                    inner.origin.x
                                } else {
                                    inner.origin.x + skip
                                },
                                inner.origin.y,
                                (inner.size.width - skip).max(0.0),
                                inner.size.height,
                            );
                        }
                        if let Some(keys) = &item.shortcut {
                            cx.label(text, keys, &shortcut, TextAlign::End);
                        }
                        cx.label(text, &item.label, &style, TextAlign::Start);
                    });
                }
            }
        }
        let _ = cx.list.pop_clip();
    }
}

impl CustomControl for Command {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        if let Some(dialog) = &self.dialog {
            let style = cx.style(trigger_query(VisualState::NONE));
            let width = cx.text.width(&dialog.trigger, &style.text) + style.padding.horizontal();
            return Measured {
                size: Size::new(width.max(style.min_width), style.min_height),
                baseline: None,
            };
        }
        self.read_metrics(cx);
        Measured {
            size: Size::new(448.0, self.natural_height(cx)),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.read_metrics(cx);
        self.viewport = cx.viewport;
        let root = cx.style(self.style_query(Slot::Root, VisualState::NONE));
        self.motion_ms = cx
            .style(self.style_query(Slot::Overlay, VisualState::NONE))
            .motion_ms;
        let panel = if self.dialog.is_some() {
            // The dialog form is centered in the window.
            let viewport = cx.viewport;
            let width = root.max_width.min(viewport.size.width - 32.0);
            let height = self.natural_height(cx);
            Rect::from_xywh(
                viewport.center().x - width / 2.0,
                viewport.origin.y + (viewport.size.height - height) / 2.0,
                width,
                height,
            )
        } else {
            bounds
        };
        self.place(cx, panel);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        if let Some(dialog) = &mut self.dialog
            && !dialog.reveal.open
        {
            let outcome = dialog.press.event(cx, event);
            if let PressOutcome::Activated(_) = outcome {
                self.change_open(cx, true);
            }
            return hosted::reply(outcome);
        }
        if let ControlEvent::Pointer(pointer) = event
            && let Some(reply) = self.pointer(cx, pointer)
        {
            return reply;
        }
        if let ControlEvent::Key(KeyEvent::Down { key, .. }) = event
            && let Some(reply) = self.key(cx, *key)
        {
            return reply;
        }
        let before = self.query().to_string();
        let text = self.text;
        let reply = self.editor.event(cx, event, text);
        if self.query() != before {
            self.filter();
            cx.request_layout();
            cx.request_paint();
            cx.request_semantics();
        }
        if self.dialog.is_some() {
            // Input stays in the open dialog.
            return EventReply::Stop;
        }
        reply
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let focused = cx.state.focused();
        match &self.dialog {
            Some(dialog) => {
                let state = cx
                    .state
                    .set(VisualState::PRESSED, dialog.press.pressed)
                    .set(VisualState::OPEN, dialog.reveal.open);
                let style = cx.style(trigger_query(state));
                let label = &dialog.trigger;
                cx.slot(cx.bounds, &style, |cx, inner| {
                    cx.label(inner, label, &style, TextAlign::Center);
                });
            }
            None => self.paint_palette(cx, focused),
        }
    }

    fn overlay(&self) -> Option<Rect> {
        let dialog = self.dialog.as_ref()?;
        // The scrim covers the window, so every press reaches the palette.
        dialog.reveal.open.then_some(self.viewport)
    }

    fn modal(&self) -> bool {
        self.dialog
            .as_ref()
            .is_some_and(|dialog| dialog.reveal.open)
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        let Some(dialog) = &self.dialog else {
            return;
        };
        let progress = dialog.reveal.progress(cx.tokens, self.motion_ms, cx.time);
        if progress <= 0.0 {
            return;
        }
        let scrim = cx.style(self.style_query(Slot::Overlay, VisualState::NONE));
        let _ = cx.list.push_opacity(progress.clamp(0.0, 1.0));
        cx.slot(self.viewport, &scrim, |_, _| {});
        let _ = cx.list.pop_opacity();
        let focused = cx.state.focused();
        Reveal::paint(cx, self.panel, progress, (0.0, 0.0), |cx| {
            self.paint_palette(cx, focused);
        });
    }

    fn animating(&self, time: f64) -> bool {
        self.dialog
            .as_ref()
            .is_some_and(|dialog| dialog.reveal.animating(self.motion_ms, time))
    }

    fn semantics(&self) -> SemanticNode {
        if let Some(dialog) = self.dialog.as_ref().filter(|dialog| !dialog.reveal.open) {
            let actions = if self.enabled {
                vec![SemanticAction::Activate, SemanticAction::Focus]
            } else {
                Vec::new()
            };
            let mut node = hosted::node(SemanticRole::Button, &dialog.trigger, actions);
            node.state.expanded = Some(false);
            node.state.disabled = !self.enabled;
            return node;
        }
        let actions = if self.enabled {
            let mut actions = vec![SemanticAction::SetValue, SemanticAction::Focus];
            if self.highlighted.is_some() {
                actions.insert(0, SemanticAction::Activate);
            }
            if self.dialog.is_some() {
                actions.push(SemanticAction::Dismiss);
            }
            actions
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::ComboBox, &self.name, actions);
        node.value = Some(self.query().to_string());
        node.description = match self.highlighted.and_then(|index| self.item(index)) {
            Some(item) => item.label.clone(),
            None if self.rows.is_empty() => self.empty.clone(),
            None => String::new(),
        };
        node.state.expanded = Some(true);
        node.state.disabled = !self.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        let closed = self
            .dialog
            .as_ref()
            .is_some_and(|dialog| !dialog.reveal.open);
        match action {
            SemanticAction::Activate if closed => {
                self.change_open(cx, true);
                true
            }
            SemanticAction::Activate => match self.highlighted {
                Some(index) => {
                    self.run(cx, index);
                    true
                }
                None => false,
            },
            SemanticAction::SetValue => {
                let changed = self.editor.set_value(cx, value.unwrap_or_default());
                self.filter();
                cx.request_layout();
                changed
            }
            SemanticAction::Dismiss => {
                self.change_open(cx, false);
                cx.emit(ControlMessage::Dismissed);
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
        self.editor.field.set_enabled_state(enabled);
        if !enabled && let Some(dialog) = &mut self.dialog {
            dialog.reveal = Reveal::default();
        }
    }

    /// Takes `Text` as the search, and filters the commands.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let taken = self.editor.set_text(value);
        if taken {
            self.filter();
        }
        taken
    }

    /// Works for the dialog form only.
    fn set_open(&mut self, open: bool, time: f64) -> bool {
        if self.dialog.is_none() || (open && !self.enabled) {
            return false;
        }
        self.apply_open(open, time);
        true
    }
}

impl Command {
    fn read_metrics(&mut self, cx: &MeasureContext<'_>) {
        let style = |slot| cx.style(self.style_query(slot, VisualState::NONE));
        let (item, label, rule, group, empty) = (
            style(Slot::Item),
            style(Slot::GroupLabel),
            style(Slot::Separator),
            style(Slot::Group),
            style(Slot::Description),
        );
        let metrics = Metrics {
            item: item
                .min_height
                .max(item.text.line_height + item.padding.vertical()),
            label: label.text.line_height + label.padding.vertical(),
            separator: rule.min_height + rule.padding.vertical(),
            group_padding: group.padding.block_start,
            empty: empty.text.line_height + empty.padding.vertical(),
        };
        if metrics != self.metrics {
            self.metrics = metrics;
            let highlighted = self.highlighted;
            self.filter();
            self.highlighted = highlighted.or(self.highlighted);
        }
    }
}

fn trigger_query(state: VisualState) -> StyleQuery {
    StyleQuery::new(ControlKind::Button, Slot::Root)
        .variant(ButtonVariant::Outline)
        .size(ControlSize::Default)
        .state(state)
}
