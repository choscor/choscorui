//! A command palette: a search input over grouped commands that filters
//! as the user types, inline or in a modal dialog.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::{self, Press, PressOutcome, Reveal};
use crate::style::{
    ButtonVariant, ControlKind, ControlSize, Edges, PaintContext, ResolvedStyle, Slot, StyleQuery,
    TextAlign, VisualState,
};
use crate::text_field::TextField;
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyConvention, KeyEvent, Localizer, Measured, PointerButton,
    PointerEvent, Rect, SemanticAction, SemanticCollection, SemanticNode, SemanticParts,
    SemanticRole, Size, TextInputInfo, WheelUnit, fold_for_search,
};

/// The tallest the list grows before it scrolls, in points.
const LIST_MAX_HEIGHT: f64 = 300.0;

/// One command of a [`Command`] palette.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandItem {
    pub label: String,
    pub icon: Option<Icon>,
    /// Display text such as "⌘P", at the end of the row. On
    /// [`KeyConvention::Pc`], a text that [`shortcut_text`](crate::shortcut_text)
    /// makes shows as its PC label, such as "Ctrl+P".
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
    editor: TextField,
    empty: String,
    rows: Vec<Row>,
    /// Each shown item and its row, in order.
    shown: Vec<(usize, usize)>,
    /// The number of items that match the search text.
    matched: usize,
    /// The item under the press, which runs when the release lands on it.
    pressed: Option<usize>,
    /// The top of the dialog form, kept while the list filters.
    top: Option<f64>,
    /// The highlighted item, by its index across all groups.
    highlighted: Option<usize>,
    scroll: f64,
    metrics: Metrics,
    viewport: Rect,
    /// The keyboard conventions of the shortcut labels, from `arrange`.
    key_convention: KeyConvention,
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
            editor: field,
            empty: String::new(),
            rows: Vec::new(),
            shown: Vec::new(),
            matched: 0,
            pressed: None,
            top: None,
            highlighted: None,
            scroll: 0.0,
            metrics: Metrics::default(),
            viewport: Rect::default(),
            key_convention: KeyConvention::Mac,
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
        self.editor.value()
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

    /// The item at `index` across all groups, found group by group.
    fn item(&self, mut index: usize) -> Option<&CommandItem> {
        for (_, items) in &self.groups {
            match items.get(index) {
                Some(item) => return Some(item),
                None => index -= items.len(),
            }
        }
        None
    }

    /// The items that match the search text, in order.
    fn matches(&self) -> Vec<usize> {
        let needle = fold_for_search(&self.query().to_lowercase());
        self.items()
            .filter(|(_, _, item)| fold_for_search(&item.label.to_lowercase()).contains(&needle))
            .map(|(index, _, _)| index)
            .collect()
    }

    fn enabled(&self, index: usize) -> bool {
        self.item(index).is_some_and(|item| item.enabled)
    }

    /// The position in `shown` of item `index`.
    fn position(&self, index: usize) -> Option<usize> {
        self.shown
            .binary_search_by_key(&index, |(item, _)| *item)
            .ok()
    }

    /// The first enabled item from position `from` of `shown`, toward
    /// the end or the start.
    fn enabled_from(&self, from: usize, forward: bool) -> Option<usize> {
        let found = if forward {
            self.shown
                .get(from..)?
                .iter()
                .find(|(index, _)| self.enabled(*index))
        } else {
            let end = (from + 1).min(self.shown.len());
            self.shown[..end]
                .iter()
                .rev()
                .find(|(index, _)| self.enabled(*index))
        };
        found.map(|(index, _)| *index)
    }

    /// Rebuilds the rows for the search text and highlights the first
    /// enabled match.
    fn filter(&mut self) {
        let matches = self.matches();
        let m = self.metrics;
        let mut rows = Vec::new();
        let mut top = 0.0;
        let mut first = 0;
        let mut self_shown = Vec::new();
        for (group, (label, items)) in self.groups.iter().enumerate() {
            let shown: Vec<usize> = (first..first + items.len())
                .filter(|index| matches.binary_search(index).is_ok())
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
                self_shown.push((index, rows.len()));
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
        self.shown = self_shown;
        self.matched = matches.len();
        self.highlighted = self.enabled_from(0, true);
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

    /// The rectangle of `row` with the scroll applied.
    fn row_rect(&self, row: &Row) -> Rect {
        let inset = self.metrics.group_padding;
        Rect::from_xywh(
            self.list.origin.x + inset,
            self.list.origin.y + row.top - self.scroll,
            self.list.size.width - 2.0 * inset,
            row.height,
        )
    }

    /// The rows in view, found by a binary search.
    fn visible_rows(&self) -> &[Row] {
        let first = self
            .rows
            .partition_point(|row| row.top + row.height < self.scroll);
        let bottom = self.scroll + self.list.size.height;
        let end = first + self.rows[first..].partition_point(|row| row.top <= bottom);
        &self.rows[first..end]
    }

    /// The enabled item under `point`, found by a binary search.
    fn item_at(&self, point: choscorui_core::Point) -> Option<usize> {
        if !self.list.contains(point) {
            return None;
        }
        let y = point.y - self.list.origin.y + self.scroll;
        let at = self.rows.partition_point(|row| row.top + row.height <= y);
        let row = self.rows.get(at)?;
        match row.kind {
            RowKind::Item(index) if self.row_rect(row).contains(point) && self.enabled(index) => {
                Some(index)
            }
            _ => None,
        }
    }

    fn highlight(&mut self, index: Option<usize>) {
        self.highlighted = index;
        let Some(row) = index
            .and_then(|index| self.position(index))
            .map(|at| self.rows[self.shown[at].1])
        else {
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

    /// Moves the highlight `count` shown items forward or back, to the
    /// nearest enabled item. It stops at the ends.
    fn step(&mut self, forward: bool, count: usize) {
        let last = self.shown.len().saturating_sub(1);
        let next = match self.highlighted.and_then(|index| self.position(index)) {
            Some(at) if forward => self
                .enabled_from((at + count).min(last), true)
                .or_else(|| self.enabled_from(last, false)),
            Some(at) => self
                .enabled_from(at.saturating_sub(count), false)
                .or_else(|| self.enabled_from(0, true)),
            None if forward => self.enabled_from(0, true),
            None => self.enabled_from(last, false),
        };
        if next.is_some() {
            self.highlight(next);
        }
    }

    /// The number of items that fit in the list, for PageUp and PageDown.
    fn page(&self) -> usize {
        let item = self.metrics.item.max(1.0);
        ((self.list.size.height / item).floor() as usize).max(1)
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
            self.editor.replace_value("");
            self.filter();
            self.top = None;
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
            Key::ArrowDown | Key::ArrowUp => self.step(key == Key::ArrowDown, 1),
            Key::PageDown | Key::PageUp => self.step(key == Key::PageDown, self.page()),
            Key::Home => self.highlight(self.enabled_from(0, true)),
            Key::End => {
                self.highlight(self.enabled_from(self.shown.len().saturating_sub(1), false))
            }
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
                self.pressed = self.item_at(position);
                cx.request_focus();
                Some(EventReply::Stop)
            }
            // A command runs when the press and the release land on it.
            PointerEvent::Up { position, .. } if self.list.contains(position) => {
                let pressed = self.pressed.take();
                if let Some(index) = self
                    .item_at(position)
                    .filter(|index| pressed == Some(*index))
                {
                    self.run(cx, index);
                }
                Some(EventReply::Stop)
            }
            PointerEvent::Up { .. } => {
                self.pressed = None;
                None
            }
            PointerEvent::Move { position } => {
                if let Some(index) = self.item_at(position)
                    && self.highlighted != Some(index)
                {
                    self.highlighted = Some(index);
                    cx.request_paint();
                    cx.request_semantics();
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
            .hosted_state(VisualState::NONE.set(VisualState::FOCUSED, focused));
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
        self.editor.paint_hosted_text(cx, self.text, state);
        self.paint_list(cx);
    }

    fn paint_list(&self, cx: &mut PaintContext<'_>) {
        let list = self.list;
        cx.list.push_clip(list);
        if self.rows.is_empty() && !self.empty.is_empty() {
            let style = cx.style(self.style_query(Slot::Description, VisualState::NONE));
            cx.label(list, &self.empty, &style, TextAlign::Center);
        }
        let glyph = cx.style(self.style_query(Slot::Icon, VisualState::NONE));
        for row in self.visible_rows() {
            let rect = self.row_rect(row);
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
                            let size = glyph.icon_size;
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
                        let keys = item.shortcut.as_deref().unwrap_or_default();
                        let keys = crate::widgets::display_shortcut(keys, cx.key_convention);
                        cx.label_and_trailing(text, &item.label, &style, &keys, &shortcut);
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
        let root = cx.style(self.style_query(Slot::Root, VisualState::NONE));
        Measured {
            size: Size::new(root.max_width, self.natural_height(cx)),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.read_metrics(cx);
        if self.viewport != cx.viewport {
            self.top = None;
        }
        self.viewport = cx.viewport;
        self.key_convention = cx.key_convention;
        let root = cx.style(self.style_query(Slot::Root, VisualState::NONE));
        self.motion_ms = cx
            .style(self.style_query(Slot::Overlay, VisualState::NONE))
            .motion_ms;
        let panel = if self.dialog.is_some() {
            // The dialog form opens centered in the window. Its top stays
            // while the list filters, and the form stays in the window.
            let viewport = cx.viewport;
            let width = root.max_width.min(viewport.size.width - 32.0);
            let height = self.natural_height(cx);
            let centered = viewport.origin.y + (viewport.size.height - height) / 2.0;
            let top = *self.top.get_or_insert(centered);
            let top = top.min(viewport.max_y() - height).max(viewport.origin.y);
            Rect::from_xywh(viewport.center().x - width / 2.0, top, width, height)
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
        let reply = self.editor.hosted_event(cx, event, text);
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

    fn text_input(&self) -> Option<TextInputInfo> {
        self.editor.hosted_text_input()
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
        if self.rows.is_empty() {
            node.description.clone_from(&self.empty);
        }
        node.state.expanded = Some(true);
        node.state.disabled = !self.enabled;
        node
    }

    /// The list is a `List` part with the matching item count. Only the
    /// rows in view are parts, found by a binary search, so the work does
    /// not grow with the number of commands. The highlighted item has the
    /// focus, as the active descendant.
    fn semantic_parts(&self, _strings: &Localizer, parts: &mut SemanticParts) {
        if self
            .dialog
            .as_ref()
            .is_some_and(|dialog| !dialog.reveal.open)
        {
            return;
        }
        let list = parts.push(None, SemanticRole::List, &self.name, self.list);
        list.collection = Some(SemanticCollection {
            item_count: self.matched,
            column_count: None,
        });
        let list = Some(parts.len() - 1);
        for row in self.visible_rows() {
            let bounds = self.row_rect(row);
            match row.kind {
                RowKind::Label(group) => {
                    parts.push(
                        list,
                        SemanticRole::StaticText,
                        &self.groups[group].0,
                        bounds,
                    );
                }
                RowKind::Item(index) => {
                    let Some(item) = self.item(index) else {
                        continue;
                    };
                    let enabled = self.enabled && item.enabled;
                    let part = hosted::pressable(
                        parts,
                        list,
                        SemanticRole::ListItem,
                        &item.label,
                        bounds,
                        enabled,
                    );
                    if let Some(shortcut) = &item.shortcut {
                        part.node.description =
                            crate::widgets::display_shortcut(shortcut, self.key_convention)
                                .into_owned();
                    }
                    part.node.state.selected = Some(self.highlighted == Some(index));
                    part.item(index);
                    if self.highlighted == Some(index) {
                        parts.set_focused(Some(parts.len() - 1));
                    }
                }
                RowKind::Separator => {}
            }
        }
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
                let changed = self.editor.hosted_set_value(cx, value.unwrap_or_default());
                self.filter();
                cx.request_layout();
                changed
            }
            // The inline form has nothing to dismiss.
            SemanticAction::Dismiss if self.dialog.is_none() => false,
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
        self.editor.set_enabled_state(enabled);
        if !enabled && let Some(dialog) = &mut self.dialog {
            dialog.reveal = Reveal::default();
        }
    }

    /// Takes `Text` as the search, and filters the commands.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let taken = self.editor.take_text(value);
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
