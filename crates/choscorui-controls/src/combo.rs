//! Combobox with completion, and a font chooser built on it.
//!
//! Items carry stable [`ItemKey`]s, so the selection and the highlighted
//! suggestion survive item lists that update while the popup is open
//! (for example asynchronous search results).

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::Reveal;
use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, Styles, TextAlign, VisualState, content_rect,
};
use choscorui_core::{
    DesignTokens, EventPhase, EventReply, Icon, ItemKey, Key, KeyEvent, Measured, Point,
    PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRole, SemanticState,
    Size, fold_for_search, grapheme_boundaries,
};

/// The most suggestions that the hosted popup shows.
const POPUP_ROWS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComboItem {
    pub key: ItemKey,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionMode {
    /// Suggestions start with the typed text; inline completion offered.
    Prefix,
    /// Suggestions contain the typed text anywhere.
    Contains,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ComboInput {
    Key {
        key: Key,
        alt: bool,
    },
    /// Text typed into an editable combo (already composed by the IME).
    Text(String),
    /// Replaces the editable text (paste, accessibility, field sync).
    SetText(String),
    /// Click on the control: toggles the popup.
    Press,
    /// Click on suggestion `index` in the open popup.
    Choose(usize),
    Semantic(SemanticAction, Option<String>),
    /// Outside press or window deactivation.
    Dismiss,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComboChange {
    None,
    Opened,
    Closed,
    Highlighted,
    TextChanged,
    /// The selection was committed (None: free text with no matching item).
    Committed(Option<ItemKey>),
}

/// Row height and popup inset, read from the `Combobox` recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ComboMetrics {
    row: f64,
    inset: f64,
}

impl ComboMetrics {
    fn resolve(styles: &Styles, tokens: &DesignTokens) -> Self {
        let style = |slot| styles.resolve(tokens, StyleQuery::new(ControlKind::Combobox, slot));
        let (item, content) = (style(Slot::Item), style(Slot::Content));
        Self {
            row: item
                .min_height
                .max(item.text.line_height + item.padding.vertical()),
            inset: content.padding.block_start,
        }
    }
}

/// A text field or a button with a filterable list of suggestions in a
/// popover.
pub struct Combobox {
    name: String,
    editable: bool,
    mode: CompletionMode,
    items: Vec<ComboItem>,
    selected: Option<ItemKey>,
    text: String,
    open: bool,
    /// Indexes into `items` shown in the popup.
    matches: Vec<usize>,
    highlighted: Option<ItemKey>,
    text_before_open: String,
    invalid: bool,
    enabled: bool,
    metrics: ComboMetrics,
    /// The popup animation, its duration, and the trigger bounds in a
    /// control host.
    reveal: Reveal,
    motion_ms: u32,
    anchor: Rect,
}

/// Deprecated name of [`Combobox`].
#[deprecated(since = "0.1.0", note = "renamed to `Combobox`")]
pub type ComboBox = Combobox;

impl Combobox {
    pub fn new(name: impl Into<String>, editable: bool) -> Self {
        Self {
            name: name.into(),
            editable,
            mode: CompletionMode::Prefix,
            items: Vec::new(),
            selected: None,
            text: String::new(),
            open: false,
            matches: Vec::new(),
            highlighted: None,
            text_before_open: String::new(),
            invalid: false,
            enabled: true,
            metrics: ComboMetrics::resolve(&Styles::new(), &DesignTokens::light()),
            reveal: Reveal::default(),
            motion_ms: 0,
            anchor: Rect::default(),
        }
    }

    /// Reads the suggestion row height and the popup inset from `styles`.
    pub fn set_metrics(&mut self, styles: &Styles, tokens: &DesignTokens) {
        self.metrics = ComboMetrics::resolve(styles, tokens);
    }
    pub fn set_completion_mode(&mut self, mode: CompletionMode) {
        self.mode = mode;
        self.refilter();
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.open = false;
        }
    }

    /// The combobox with the invalid state from the start.
    #[must_use]
    pub fn with_invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn set_invalid(&mut self, invalid: bool) {
        self.invalid = invalid;
    }

    pub fn items(&self) -> &[ComboItem] {
        &self.items
    }
    pub fn selected(&self) -> Option<ItemKey> {
        self.selected
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn highlighted(&self) -> Option<ItemKey> {
        self.highlighted
    }
    /// Suggestions currently shown, in order.
    pub fn suggestions(&self) -> Vec<&ComboItem> {
        self.matches
            .iter()
            .map(|index| &self.items[*index])
            .collect()
    }

    /// Inline completion: the rest of the first prefix suggestion.
    pub fn completion_suffix(&self) -> Option<&str> {
        if !self.editable || self.mode != CompletionMode::Prefix || self.text.is_empty() {
            return None;
        }
        let item = &self.items[*self.matches.first()?];
        let typed = self.text.chars().count();
        let boundary = item.text.char_indices().nth(typed).map(|(byte, _)| byte)?;
        Some(&item.text[boundary..])
    }

    /// Replaces the item list, keeping selection and highlight by key.
    pub fn set_items(&mut self, items: Vec<ComboItem>) {
        self.items = items;
        if self
            .selected
            .is_some_and(|key| !self.items.iter().any(|item| item.key == key))
        {
            self.selected = None;
        }
        if !self.editable {
            self.text = self
                .selected
                .and_then(|key| self.items.iter().find(|item| item.key == key))
                .map(|item| item.text.clone())
                .unwrap_or_default();
        }
        self.refilter();
    }

    /// Selects `key` without opening the popup.
    pub fn select(&mut self, key: Option<ItemKey>) {
        self.selected = key.filter(|key| self.items.iter().any(|item| item.key == *key));
        if let Some(item) = self.selected_item() {
            self.text = item.text.clone();
        } else if !self.editable {
            self.text.clear();
        }
        self.refilter();
    }

    fn selected_item(&self) -> Option<&ComboItem> {
        let key = self.selected?;
        self.items.iter().find(|item| item.key == key)
    }

    fn refilter(&mut self) {
        let needle = fold_for_search(&self.text);
        let filtering = self.editable && !needle.is_empty();
        self.matches = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if !filtering {
                    return true;
                }
                let hay = fold_for_search(&item.text);
                match self.mode {
                    CompletionMode::Prefix => hay.starts_with(&needle),
                    CompletionMode::Contains => hay.contains(&needle),
                }
            })
            .map(|(index, _)| index)
            .collect();
        let visible = |key: ItemKey| {
            self.matches
                .iter()
                .any(|index| self.items[*index].key == key)
        };
        if self.highlighted.is_none_or(|key| !visible(key)) {
            self.highlighted = self
                .selected
                .filter(|key| visible(*key))
                .or_else(|| self.matches.first().map(|index| self.items[*index].key));
        }
        if self.open && self.matches.is_empty() && self.editable {
            self.open = false;
        }
    }

    fn open_popup(&mut self) -> ComboChange {
        if self.open || !self.enabled {
            return ComboChange::None;
        }
        self.text_before_open = self.text.clone();
        self.open = true;
        if !self.editable {
            self.highlighted = self
                .selected
                .or_else(|| self.items.first().map(|item| item.key));
        }
        self.refilter();
        if self.open {
            ComboChange::Opened
        } else {
            ComboChange::None
        }
    }

    fn close(&mut self) -> ComboChange {
        if self.open {
            self.open = false;
            ComboChange::Closed
        } else {
            ComboChange::None
        }
    }

    fn move_highlight(&mut self, delta: isize) -> ComboChange {
        if self.matches.is_empty() {
            return ComboChange::None;
        }
        let position = self.highlighted.and_then(|key| {
            self.matches
                .iter()
                .position(|index| self.items[*index].key == key)
        });
        let last = self.matches.len() as isize - 1;
        let next = match position {
            Some(position) => (position as isize + delta).clamp(0, last),
            None if delta > 0 => 0,
            None => last,
        } as usize;
        let key = self.items[self.matches[next]].key;
        if self.highlighted == Some(key) {
            return ComboChange::None;
        }
        self.highlighted = Some(key);
        ComboChange::Highlighted
    }

    fn commit_key(&mut self, key: Option<ItemKey>) -> ComboChange {
        self.open = false;
        match key.and_then(|key| self.items.iter().find(|item| item.key == key)) {
            Some(item) => {
                self.selected = Some(item.key);
                self.text = item.text.clone();
            }
            None if self.editable => {
                let folded = fold_for_search(&self.text);
                self.selected = self
                    .items
                    .iter()
                    .find(|item| fold_for_search(&item.text) == folded)
                    .map(|item| item.key);
            }
            None => return ComboChange::Closed,
        }
        self.refilter();
        ComboChange::Committed(self.selected)
    }

    pub fn handle(&mut self, input: ComboInput) -> ComboChange {
        if !self.enabled {
            return ComboChange::None;
        }
        match input {
            ComboInput::Press => {
                if self.open {
                    self.close()
                } else {
                    self.open_popup()
                }
            }
            ComboInput::Dismiss => self.close(),
            ComboInput::Choose(index) => match self.matches.get(index) {
                Some(index) => {
                    let key = self.items[*index].key;
                    self.commit_key(Some(key))
                }
                None => ComboChange::None,
            },
            ComboInput::Text(text) if self.editable => {
                self.text.push_str(&text);
                self.after_edit()
            }
            ComboInput::SetText(text) if self.editable => {
                self.text = text;
                self.after_edit()
            }
            ComboInput::Text(_) | ComboInput::SetText(_) => ComboChange::None,
            ComboInput::Key { key, alt } => self.handle_key(key, alt),
            ComboInput::Semantic(action, value) => match action {
                SemanticAction::ShowMenu | SemanticAction::Expand => self.open_popup(),
                SemanticAction::Collapse | SemanticAction::Dismiss => self.close(),
                SemanticAction::SetValue => {
                    let Some(value) = value else {
                        return ComboChange::None;
                    };
                    let folded = fold_for_search(&value);
                    let key = self
                        .items
                        .iter()
                        .find(|item| fold_for_search(&item.text) == folded)
                        .map(|item| item.key);
                    if key.is_none() && self.editable {
                        self.text = value;
                    }
                    if key.is_some() || self.editable {
                        self.commit_key(key)
                    } else {
                        ComboChange::None
                    }
                }
                _ => ComboChange::None,
            },
        }
    }

    fn after_edit(&mut self) -> ComboChange {
        self.highlighted = None;
        self.refilter();
        if !self.text.is_empty() && !self.matches.is_empty() && !self.open {
            self.text_before_open = self.text.clone();
            self.open = true;
        }
        ComboChange::TextChanged
    }

    fn handle_key(&mut self, key: Key, alt: bool) -> ComboChange {
        match (key, self.open) {
            (Key::ArrowDown, false) if alt || self.editable => self.open_popup(),
            (Key::ArrowDown | Key::ArrowUp, false) => {
                // Closed, non-editable: step the selection directly.
                let delta = if key == Key::ArrowDown { 1 } else { -1 };
                self.highlighted = self.selected;
                match self.move_highlight(delta) {
                    ComboChange::Highlighted => self.commit_key(self.highlighted),
                    _ => ComboChange::None,
                }
            }
            (Key::Space, false) if !self.editable => self.open_popup(),
            (Key::ArrowDown, true) => self.move_highlight(1),
            (Key::ArrowUp, true) => self.move_highlight(-1),
            (Key::PageDown | Key::End, true) => self.move_highlight(isize::MAX / 2),
            (Key::PageUp | Key::Home, true) => self.move_highlight(-(isize::MAX / 2)),
            (Key::Enter, true) | (Key::Space, true) if !self.editable || key == Key::Enter => {
                self.commit_key(self.highlighted)
            }
            (Key::Enter, false) if self.editable => self.commit_key(None),
            (Key::Tab, _) if self.editable && self.completion_suffix().is_some() => {
                let key = self.matches.first().map(|index| self.items[*index].key);
                self.commit_key(key)
            }
            (Key::Escape, true) => {
                self.text = self.text_before_open.clone();
                self.refilter();
                self.close()
            }
            _ => ComboChange::None,
        }
    }

    /// The flags the trigger recipe reads, from the host's focus, hover,
    /// and invalid flags in `host`. The invalid state of the combobox
    /// adds to the host's.
    pub fn visual_state(&self, host: VisualState) -> VisualState {
        host.set(VisualState::OPEN, self.open)
            .set(VisualState::DISABLED, !self.enabled)
            .set(VisualState::INVALID, host.invalid() || self.invalid)
    }

    /// Paints the trigger in the context's bounds: the field box, the text
    /// or `placeholder`, the inline completion while focused, and a chevron.
    pub fn paint(&self, cx: &mut PaintContext<'_>, host: VisualState, placeholder: &str) {
        let state = self.visual_state(host);
        let query = |slot| StyleQuery::new(ControlKind::Combobox, slot).state(state);
        let (trigger, value, muted) = (
            cx.style(query(Slot::Trigger)),
            cx.style(query(Slot::Value)),
            cx.style(query(Slot::Placeholder)),
        );
        let bounds = cx.bounds;
        cx.slot(bounds, &trigger, |cx, inner| {
            let icon_width = trigger.icon_size.min(inner.size.width);
            let icon_x = if cx.rtl {
                inner.origin.x
            } else {
                inner.max_x() - icon_width
            };
            let icon = Rect::from_xywh(icon_x, inner.origin.y, icon_width, inner.size.height);
            cx.icon(icon, Icon::ChevronDown, &muted);
            let mut text = inner;
            text.size.width = (inner.size.width - icon_width - trigger.gap).max(0.0);
            if cx.rtl {
                text.origin.x = inner.max_x() - text.size.width;
            }
            if self.text.is_empty() {
                cx.label(text, placeholder, &muted, TextAlign::Start);
                return;
            }
            let typed = cx.label(text, &self.text, &value, TextAlign::Start);
            if let Some(suffix) = self.completion_suffix().filter(|_| state.focused()) {
                let rest = if cx.rtl {
                    Rect::from_xywh(
                        text.origin.x,
                        text.origin.y,
                        (typed.origin.x - text.origin.x).max(0.0),
                        text.size.height,
                    )
                } else {
                    Rect::from_xywh(
                        typed.max_x(),
                        text.origin.y,
                        (text.max_x() - typed.max_x()).max(0.0),
                        text.size.height,
                    )
                };
                cx.label(rest, suffix, &muted, TextAlign::Start);
            }
        });
    }

    /// The popup below `anchor`, showing at most `max_rows` suggestions.
    pub fn popup_rect(&self, anchor: Rect, max_rows: usize) -> Rect {
        let rows = self.matches.len().min(max_rows) as f64;
        Rect::from_xywh(
            anchor.origin.x,
            anchor.max_y() + self.metrics.inset,
            anchor.size.width,
            rows * self.metrics.row + 2.0 * self.metrics.inset,
        )
    }

    /// The suggestion under `point` in a popup at `popup`.
    pub fn suggestion_at(&self, popup: Rect, point: Point) -> Option<usize> {
        if !popup.contains(point) {
            return None;
        }
        let y = point.y - popup.origin.y - self.metrics.inset;
        let index = (y / self.metrics.row).floor();
        (index >= 0.0 && (index as usize) < self.matches.len()).then_some(index as usize)
    }

    /// Paints the open popup in the context's bounds, from
    /// [`Combobox::popup_rect`]. Paint it above other content.
    pub fn paint_popup(&self, cx: &mut PaintContext<'_>) {
        if self.open {
            self.paint_rows(cx);
        }
    }

    /// Paints the popup box and its rows in the context's bounds.
    fn paint_rows(&self, cx: &mut PaintContext<'_>) {
        let popup = cx.bounds;
        let query = |slot| StyleQuery::new(ControlKind::Combobox, slot);
        let content = cx.style(query(Slot::Content));
        cx.slot(popup, &content, |_, _| {});
        let inner = content_rect(popup, &content, cx.rtl);
        let rows =
            ((inner.size.height / self.metrics.row).floor() as usize).min(self.matches.len());
        for (index, item) in self.suggestions().into_iter().take(rows).enumerate() {
            let state = VisualState::NONE
                .set(VisualState::HIGHLIGHTED, self.highlighted == Some(item.key))
                .set(VisualState::SELECTED, self.selected == Some(item.key));
            let style = cx.style(query(Slot::Item).state(state));
            let indicator = cx.style(query(Slot::ItemIndicator).state(state));
            let row = Rect::from_xywh(
                inner.origin.x,
                inner.origin.y + index as f64 * self.metrics.row,
                inner.size.width,
                self.metrics.row,
            );
            cx.slot(row, &style, |cx, text| {
                cx.label(text, &item.text, &style, TextAlign::Start);
            });
            if state.selected() {
                // The check sits in the end padding, like the reference.
                let width = indicator.min_width;
                let x = if cx.rtl {
                    row.origin.x + style.padding.inline_start
                } else {
                    row.max_x() - style.padding.inline_start - width
                };
                let mark = Rect::from_xywh(x, row.origin.y, width, row.size.height);
                cx.icon(mark, Icon::Check, &indicator);
            }
        }
    }

    pub fn semantics(&self) -> SemanticNode {
        let mut actions = Vec::new();
        if self.enabled {
            actions.push(SemanticAction::ShowMenu);
            actions.push(SemanticAction::SetValue);
            actions.push(SemanticAction::Focus);
        }
        SemanticNode {
            role: SemanticRole::ComboBox,
            name: self.name.clone(),
            description: String::new(),
            value: Some(self.text.clone()),
            state: SemanticState {
                disabled: !self.enabled,
                expanded: Some(self.open),
                invalid: self.invalid,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions,
            text: None,
        }
    }

    /// Semantic nodes for the open popup's suggestions.
    pub fn suggestion_semantics(&self) -> Vec<SemanticNode> {
        self.suggestions()
            .into_iter()
            .map(|item| SemanticNode {
                role: SemanticRole::ListItem,
                name: item.text.clone(),
                description: String::new(),
                value: None,
                state: SemanticState {
                    selected: Some(self.highlighted == Some(item.key)),
                    ..SemanticState::default()
                },
                relations: Vec::new(),
                actions: vec![SemanticAction::Activate],
                text: None,
            })
            .collect()
    }
}

/// Stable key for a text item (FNV-1a), so items rebuilt from strings keep
/// their identity.
pub fn text_key(text: &str) -> ItemKey {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    ItemKey(hash)
}

/// Font family chooser. Families come from the platform (for example
/// `choscorui_macos::available_font_families`); a typed family that is not
/// installed resolves to the fallback and is reported as missing.
pub struct FontChooser {
    combo: Combobox,
    fallback: String,
}

impl FontChooser {
    pub fn new(families: Vec<String>, current: &str, fallback: impl Into<String>) -> Self {
        let mut combo = Combobox::new("Font", true);
        combo.set_items(
            families
                .into_iter()
                .map(|family| ComboItem {
                    key: text_key(&family),
                    text: family,
                })
                .collect(),
        );
        combo.handle(ComboInput::SetText(current.into()));
        combo.handle(ComboInput::Key {
            key: Key::Enter,
            alt: false,
        });
        Self {
            combo,
            fallback: fallback.into(),
        }
    }
    pub fn combo(&self) -> &Combobox {
        &self.combo
    }
    pub fn handle(&mut self, input: ComboInput) -> ComboChange {
        self.combo.handle(input)
    }
    pub fn is_missing(&self) -> bool {
        self.combo.selected().is_none()
    }
    /// The family to render with: the chosen installed family, or the
    /// fallback when the typed family is missing.
    pub fn resolved_family(&self) -> &str {
        match self.combo.selected() {
            Some(key) => self
                .combo
                .items()
                .iter()
                .find(|item| item.key == key)
                .map_or(self.fallback.as_str(), |item| item.text.as_str()),
            None => &self.fallback,
        }
    }
    pub fn semantics(&self) -> SemanticNode {
        let mut node = self.combo.semantics();
        if self.is_missing() {
            node.description = format!("Not installed; using {}", self.fallback);
            node.state.invalid = true;
        }
        node
    }
}

impl Combobox {
    /// The index of the selected item in `items`.
    fn selected_index(&self) -> Option<usize> {
        let key = self.selected?;
        self.items.iter().position(|item| item.key == key)
    }

    /// The text without its last grapheme, for Backspace in a host.
    fn without_last_grapheme(&self) -> String {
        let boundaries = grapheme_boundaries(&self.text);
        let end = boundaries.iter().rev().nth(1).copied().unwrap_or(0);
        self.text[..end].to_string()
    }

    /// The hosted popup below the trigger.
    fn hosted_popup(&self) -> Rect {
        self.popup_rect(self.anchor, POPUP_ROWS)
    }

    /// Reports the changes since `before` (text and selection) and the
    /// open state to the host.
    fn report(&mut self, cx: &mut ControlContext<'_>, before: (String, Option<ItemKey>)) {
        let (text, selected) = before;
        if self.editable && self.text != text {
            cx.emit(ControlMessage::Changed(ControlValue::Text(
                self.text.clone(),
            )));
        }
        if self.selected != selected {
            cx.emit(ControlMessage::Changed(ControlValue::Index(
                self.selected_index(),
            )));
        }
        if self.reveal.open != self.open {
            self.reveal.set(self.open, cx.time());
            cx.emit(ControlMessage::OpenChanged(self.open));
        }
        cx.request_paint();
        cx.request_semantics();
    }

    /// Maps a pointer event to an input, and replies.
    fn pointer_input(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &PointerEvent,
    ) -> (Option<ComboInput>, EventReply) {
        let popup = self.hosted_popup();
        match *event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => {
                if self.open && popup.contains(position) {
                    (None, EventReply::Stop)
                } else if cx.bounds().contains(position) {
                    cx.request_focus();
                    (Some(ComboInput::Press), EventReply::Stop)
                } else {
                    (None, EventReply::Continue)
                }
            }
            PointerEvent::Up { position, .. } if self.open => {
                match self.suggestion_at(popup, position) {
                    Some(index) => (Some(ComboInput::Choose(index)), EventReply::Stop),
                    None => (None, EventReply::Continue),
                }
            }
            PointerEvent::Move { position } if self.open => {
                if let Some(index) = self.suggestion_at(popup, position) {
                    let key = self.items[self.matches[index]].key;
                    if self.highlighted != Some(key) {
                        self.highlighted = Some(key);
                        cx.request_paint();
                    }
                }
                (None, EventReply::Continue)
            }
            _ => (None, EventReply::Continue),
        }
    }
}

impl CustomControl for Combobox {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let query = |slot| StyleQuery::new(ControlKind::Combobox, slot);
        let (trigger, value) = (cx.style(query(Slot::Trigger)), cx.style(query(Slot::Value)));
        let widest = self
            .items
            .iter()
            .map(|item| cx.text.width(&item.text, &value.text))
            .fold(cx.text.width(&self.text, &value.text), f64::max);
        let width = trigger.padding.horizontal() + widest + trigger.gap + trigger.icon_size;
        let height = (value.text.line_height + trigger.padding.vertical()).max(trigger.min_height);
        Measured::new(Size::new(width.max(trigger.min_width), height))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.set_metrics(cx.styles, cx.tokens);
        self.anchor = bounds;
        self.motion_ms = cx
            .style(StyleQuery::new(ControlKind::Combobox, Slot::Content).state(VisualState::OPEN))
            .motion_ms;
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let before = (self.text.clone(), self.selected);
        let (input, reply) = match event {
            ControlEvent::Pointer(pointer) => self.pointer_input(cx, pointer),
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Backspace,
                ..
            }) if self.editable => (
                Some(ComboInput::SetText(self.without_last_grapheme())),
                EventReply::Stop,
            ),
            ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                let input = ComboInput::Key {
                    key: *key,
                    alt: cx.modifiers().option,
                };
                (Some(input), EventReply::Continue)
            }
            ControlEvent::Text(text) if self.editable => {
                (Some(ComboInput::Text(text.clone())), EventReply::Stop)
            }
            ControlEvent::Dismiss | ControlEvent::FocusChanged(false) => {
                (Some(ComboInput::Dismiss), EventReply::Continue)
            }
            _ => (None, EventReply::Continue),
        };
        let Some(input) = input else {
            return reply;
        };
        let escape = matches!(
            input,
            ComboInput::Key {
                key: Key::Escape,
                ..
            }
        );
        let change = self.handle(input);
        if change == ComboChange::None {
            return reply;
        }
        self.report(cx, before);
        if escape {
            cx.emit(ControlMessage::Dismissed);
        }
        EventReply::Stop
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        Combobox::paint(self, cx, host, "");
    }

    fn overlay(&self) -> Option<Rect> {
        self.open.then(|| self.hosted_popup())
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        let progress = self.reveal.progress(cx.tokens, self.motion_ms, cx.time);
        if progress <= 0.0 {
            return;
        }
        let popup = self.hosted_popup();
        let slide = (0.0, -cx.tokens.spacing * 2.0);
        let bounds = cx.bounds;
        cx.bounds = popup;
        Reveal::paint(cx, popup, progress, slide, |cx| self.paint_rows(cx));
        cx.bounds = bounds;
    }

    fn animating(&self, time: f64) -> bool {
        self.reveal.animating(self.motion_ms, time)
    }

    fn semantics(&self) -> SemanticNode {
        Combobox::semantics(self)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        if action == SemanticAction::Focus {
            cx.request_focus();
            return true;
        }
        let before = (self.text.clone(), self.selected);
        let change = self.handle(ComboInput::Semantic(action, value.map(str::to_string)));
        if change == ComboChange::None {
            return false;
        }
        self.report(cx, before);
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        Combobox::set_enabled(self, enabled);
        if !enabled {
            self.reveal = Reveal::default();
        }
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Index(index) => {
                let key = index
                    .and_then(|index| self.items.get(index))
                    .map(|item| item.key);
                if index.is_some() && key.is_none() {
                    return false;
                }
                self.select(key);
                true
            }
            ControlValue::Text(text) => {
                let folded = fold_for_search(text);
                let key = self
                    .items
                    .iter()
                    .find(|item| fold_for_search(&item.text) == folded)
                    .map(|item| item.key);
                if key.is_none() && !self.editable {
                    return false;
                }
                self.select(key);
                self.text.clone_from(text);
                self.refilter();
                true
            }
            _ => false,
        }
    }

    fn set_open(&mut self, open: bool, time: f64) -> bool {
        if open {
            self.open_popup();
        } else {
            self.close();
        }
        self.reveal.set(self.open, time);
        self.open == open
    }
}

impl CustomControl for FontChooser {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        CustomControl::measure(&mut self.combo, cx)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        CustomControl::arrange(&mut self.combo, cx, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        self.combo.event(cx, event)
    }

    /// The trigger shows the invalid look while the family is missing.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state.set(VisualState::INVALID, self.is_missing());
        self.combo.paint(cx, host, "");
    }

    fn overlay(&self) -> Option<Rect> {
        self.combo.overlay()
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        self.combo.paint_overlay(cx);
    }

    fn animating(&self, time: f64) -> bool {
        self.combo.animating(time)
    }

    fn semantics(&self) -> SemanticNode {
        FontChooser::semantics(self)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        self.combo.perform(cx, action, value)
    }

    fn set_enabled(&mut self, enabled: bool) {
        CustomControl::set_enabled(&mut self.combo, enabled);
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        CustomControl::set_value(&mut self.combo, value)
    }

    fn set_open(&mut self, open: bool, time: f64) -> bool {
        self.combo.set_open(open, time)
    }
}
