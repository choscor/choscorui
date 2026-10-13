//! Combobox with completion, and a font chooser built on it.
//!
//! Items carry stable [`ItemKey`]s, so the selection and the highlighted
//! suggestion survive item lists that update while the popup is open
//! (for example asynchronous search results).

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::Reveal;
use crate::popup::{Placement, gap_anchor, place_popup};
use crate::style::{
    ControlKind, PaintContext, ResolvedStyle, Slot, StyleQuery, Styles, TextAlign, VisualState,
    content_rect,
};
use crate::text_field::TextField;
use choscorui_core::{
    DesignTokens, EventPhase, EventReply, Icon, ItemKey, Key, KeyEvent, LayoutDirection, Localizer,
    Measured, Point, PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRole,
    SemanticState, Size, TextInputInfo, TypeSpec, WheelUnit, fold_for_search, grapheme_boundaries,
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
    /// Focus loss: closes the popup. An editable combo commits the item
    /// that matches its text, or else restores the last committed text.
    Blur,
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
/// popover. The editable text uses the editing model of [`TextField`]:
/// caret, selection, editing keys, the clipboard, and input methods.
pub struct Combobox {
    name: String,
    editable: bool,
    mode: CompletionMode,
    items: Vec<ComboItem>,
    selected: Option<ItemKey>,
    /// The text, its caret, and its selection.
    editor: TextField,
    placeholder: String,
    open: bool,
    /// Indexes into `items` shown in the popup.
    matches: Vec<usize>,
    highlighted: Option<ItemKey>,
    /// The text and the selection of the last commit. Escape and a blur
    /// without a match restore them.
    committed: (String, Option<ItemKey>),
    invalid: bool,
    enabled: bool,
    metrics: ComboMetrics,
    /// The popup animation, its duration, and the trigger bounds in a
    /// control host.
    reveal: Reveal,
    motion_ms: u32,
    anchor: Rect,
    /// The text area and the chevron of the hosted trigger.
    text_rect: Rect,
    chevron: Rect,
    /// The area that the hosted popup fits in.
    bounds: Rect,
    /// The index in `matches` of the first suggestion that the popup
    /// shows, and the most rows that it shows at a time.
    first: usize,
    page: usize,
    /// The popup width that shows the widest suggestion whole.
    list_width: f64,
    /// The width of the widest item in each text style, measured once
    /// for each item list.
    widest: Vec<(TypeSpec, f64)>,
}

/// Deprecated name of [`Combobox`].
#[deprecated(since = "0.1.0", note = "renamed to `Combobox`")]
pub type ComboBox = Combobox;

/// The text area and the chevron inside a trigger at `bounds`.
fn trigger_parts(bounds: Rect, trigger: &ResolvedStyle, rtl: bool) -> (Rect, Rect) {
    let inner = content_rect(bounds, trigger, rtl);
    let icon_width = trigger.icon_size.min(inner.size.width);
    let icon_x = if rtl {
        inner.origin.x
    } else {
        inner.max_x() - icon_width
    };
    let icon = Rect::from_xywh(icon_x, inner.origin.y, icon_width, inner.size.height);
    let mut text = inner;
    text.size.width = (inner.size.width - icon_width - trigger.gap).max(0.0);
    if rtl {
        text.origin.x = inner.max_x() - text.size.width;
    }
    (text, icon)
}

impl Combobox {
    pub fn new(name: impl Into<String>, editable: bool) -> Self {
        let name = name.into();
        Self {
            editor: TextField::new(name.clone(), "").quiet(),
            name,
            editable,
            mode: CompletionMode::Prefix,
            items: Vec::new(),
            selected: None,
            placeholder: String::new(),
            open: false,
            matches: Vec::new(),
            highlighted: None,
            committed: (String::new(), None),
            invalid: false,
            enabled: true,
            metrics: ComboMetrics::resolve(&Styles::new(), &DesignTokens::light()),
            reveal: Reveal::default(),
            motion_ms: 0,
            anchor: Rect::default(),
            list_width: 0.0,
            text_rect: Rect::default(),
            chevron: Rect::default(),
            bounds: Rect::from_xywh(0.0, 0.0, f64::MAX, f64::MAX),
            first: 0,
            page: POPUP_ROWS,
            widest: Vec::new(),
        }
    }

    /// The text the trigger shows in muted color while the text is empty.
    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.set_placeholder(placeholder);
        self
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<String>) {
        self.placeholder = placeholder.into();
        self.editor.set_placeholder(self.placeholder.clone());
    }

    pub fn placeholder(&self) -> &str {
        &self.placeholder
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
        self.editor.set_enabled_state(enabled);
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
        self.editor.value()
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
        if !self.editable || self.mode != CompletionMode::Prefix || self.text().is_empty() {
            return None;
        }
        let item = &self.items[*self.matches.first()?];
        let typed = self.text().chars().count();
        let boundary = item.text.char_indices().nth(typed).map(|(byte, _)| byte)?;
        Some(&item.text[boundary..])
    }

    /// Replaces the item list, keeping selection and highlight by key.
    pub fn set_items(&mut self, items: Vec<ComboItem>) {
        self.items = items;
        self.widest.clear();
        let known = |key: Option<ItemKey>, items: &[ComboItem]| {
            key.filter(|key| items.iter().any(|item| item.key == *key))
        };
        self.selected = known(self.selected, &self.items);
        self.committed.1 = known(self.committed.1, &self.items);
        if !self.editable {
            let text = self
                .selected_item()
                .map(|item| item.text.clone())
                .unwrap_or_default();
            self.set_text(&text);
            self.committed = (text, self.selected);
        }
        self.refilter();
    }

    /// Selects `key` without opening the popup. `None` clears the text.
    pub fn select(&mut self, key: Option<ItemKey>) {
        self.selected = key.filter(|key| self.items.iter().any(|item| item.key == *key));
        let text = self
            .selected_item()
            .map(|item| item.text.clone())
            .unwrap_or_default();
        self.set_text(&text);
        self.committed = (text, self.selected);
        self.refilter();
    }

    fn selected_item(&self) -> Option<&ComboItem> {
        let key = self.selected?;
        self.items.iter().find(|item| item.key == key)
    }

    fn set_text(&mut self, text: &str) {
        self.editor.replace_value(text);
    }

    /// The item whose text matches `text`, ignoring case and accents.
    fn find_text(&self, text: &str) -> Option<ItemKey> {
        let folded = fold_for_search(text);
        self.items
            .iter()
            .find(|item| fold_for_search(&item.text) == folded)
            .map(|item| item.key)
    }

    fn refilter(&mut self) {
        let needle = fold_for_search(self.text());
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
        self.reveal_highlight();
    }

    /// Scrolls the popup rows so that the highlighted suggestion shows.
    fn reveal_highlight(&mut self) {
        let page = self.page.max(1);
        let position = self.highlighted.and_then(|key| {
            self.matches
                .iter()
                .position(|index| self.items[*index].key == key)
        });
        if let Some(position) = position {
            if position < self.first {
                self.first = position;
            } else if position >= self.first + page {
                self.first = position + 1 - page;
            }
        }
        self.first = self.first.min(self.matches.len().saturating_sub(page));
    }

    /// Scrolls the popup rows by `rows`, within the suggestions. Returns
    /// true when the first row changed.
    fn scroll_rows(&mut self, rows: isize) -> bool {
        let last = self.matches.len().saturating_sub(self.page.max(1));
        let first = self.first.saturating_add_signed(rows).min(last);
        let changed = first != self.first;
        self.first = first;
        changed
    }

    /// The suggestion rows that fit in `popup`.
    fn rows_in(&self, popup: Rect) -> usize {
        let inner = (popup.size.height - 2.0 * self.metrics.inset).max(0.0);
        (inner / self.metrics.row).floor() as usize
    }

    fn open_popup(&mut self) -> ComboChange {
        if self.open || !self.enabled {
            return ComboChange::None;
        }
        self.open = true;
        if !self.editable {
            self.highlighted = self
                .selected
                .or_else(|| self.items.first().map(|item| item.key));
        }
        self.refilter();
        ComboChange::Opened
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
        self.reveal_highlight();
        ComboChange::Highlighted
    }

    /// Commits the item `key`. Without a key, an editable combo commits
    /// its text and the item that matches it, if any.
    fn commit_key(&mut self, key: Option<ItemKey>) -> ComboChange {
        self.open = false;
        match key.and_then(|key| self.items.iter().find(|item| item.key == key)) {
            Some(item) => {
                self.selected = Some(item.key);
                let text = item.text.clone();
                self.set_text(&text);
            }
            None if self.editable => self.selected = self.find_text(self.text()),
            None => return ComboChange::Closed,
        }
        self.committed = (self.text().to_owned(), self.selected);
        self.refilter();
        ComboChange::Committed(self.selected)
    }

    /// Restores the text and the selection of the last commit, and closes
    /// the popup.
    fn revert(&mut self) -> ComboChange {
        let (text, selected) = self.committed.clone();
        let edited = text != self.text() || selected != self.selected;
        self.set_text(&text);
        self.selected = selected;
        self.refilter();
        match self.close() {
            ComboChange::None if edited => ComboChange::TextChanged,
            change => change,
        }
    }

    /// A blur commits the matching item of an editable combo, and else
    /// restores the last commit. Empty text commits no item.
    fn blur(&mut self) -> ComboChange {
        if !self.editable {
            return self.close();
        }
        if self.text() == self.committed.0 {
            return self.revert();
        }
        match self.find_text(self.text()) {
            Some(key) => self.commit_key(Some(key)),
            None if self.text().is_empty() => self.commit_key(None),
            None => self.revert(),
        }
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
            ComboInput::Blur => self.blur(),
            ComboInput::Choose(index) => match self.matches.get(index) {
                Some(index) => {
                    let key = self.items[*index].key;
                    self.commit_key(Some(key))
                }
                None => ComboChange::None,
            },
            ComboInput::Text(text) if self.editable => {
                let text = format!("{}{text}", self.text());
                self.set_text(&text);
                self.after_edit()
            }
            ComboInput::SetText(text) if self.editable => {
                self.set_text(&text);
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
                    let key = self.find_text(&value);
                    if key.is_none() && self.editable {
                        self.set_text(&value);
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

    /// After a text edit: the selection clears until the user commits a
    /// match, and the popup opens on the suggestions, or on its "No
    /// results" row.
    fn after_edit(&mut self) -> ComboChange {
        self.highlighted = None;
        self.selected = None;
        self.refilter();
        if !self.text().is_empty() {
            self.open = true;
        }
        ComboChange::TextChanged
    }

    fn handle_key(&mut self, key: Key, alt: bool) -> ComboChange {
        // In editable text, Home and End move the caret.
        let edge = !self.editable && matches!(key, Key::Home | Key::End);
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
            (Key::PageDown, true) => self.move_highlight(self.page.max(1) as isize),
            (Key::PageUp, true) => self.move_highlight(-(self.page.max(1) as isize)),
            (Key::End, true) if edge => self.move_highlight(isize::MAX / 2),
            (Key::Home, true) if edge => self.move_highlight(-(isize::MAX / 2)),
            (Key::Enter, true) | (Key::Space, true) if !self.editable || key == Key::Enter => {
                self.commit_key(self.highlighted)
            }
            (Key::Enter, false) if self.editable => self.commit_key(None),
            // Tab accepts the highlighted row, and the focus moves on.
            (Key::Tab, true) if self.highlighted.is_some() => self.commit_key(self.highlighted),
            (Key::Escape, _) if self.editable => self.revert(),
            (Key::Escape, true) => self.close(),
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
    /// Editable text shows its caret and selection while focused.
    pub fn paint(&self, cx: &mut PaintContext<'_>, host: VisualState, placeholder: &str) {
        let state = self.visual_state(host);
        let query = |slot| StyleQuery::new(ControlKind::Combobox, slot).state(state);
        let (trigger, value, muted) = (
            cx.style(query(Slot::Trigger)),
            cx.style(query(Slot::Value)),
            cx.style(query(Slot::Placeholder)),
        );
        let bounds = cx.bounds;
        cx.slot(bounds, &trigger, |_, _| {});
        let (text, icon) = trigger_parts(bounds, &trigger, cx.rtl);
        cx.icon(icon, Icon::ChevronDown, &muted);
        let placeholder = if placeholder.is_empty() {
            &self.placeholder
        } else {
            placeholder
        };
        if self.text().is_empty() && !(self.editable && state.focused()) {
            cx.label(text, placeholder, &muted, TextAlign::Start);
            return;
        }
        let typed = if self.editable {
            self.editor.paint_hosted_text(cx, text, state);
            let width = cx.measure(self.text(), &value).min(text.size.width);
            let x = if cx.rtl {
                text.max_x() - width
            } else {
                text.origin.x
            };
            Rect::from_xywh(x, text.origin.y, width, text.size.height)
        } else {
            cx.label(text, self.text(), &value, TextAlign::Start)
        };
        let at_end = self.editor.selection().is_empty()
            && self.editor.caret() + 1 >= grapheme_boundaries(self.text()).len();
        if let Some(suffix) = self
            .completion_suffix()
            .filter(|_| state.focused() && at_end)
        {
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
    }

    /// The popup below `anchor`, as wide as the anchor or as the widest
    /// suggestion when that is wider, showing at most `max_rows`
    /// suggestions, or one "No results" row. [`place_popup`] keeps it
    /// inside `bounds` (from [`crate::popup_bounds`]): it opens above the
    /// anchor when only that side fits, and else it shrinks and its rows
    /// scroll.
    pub fn popup_rect(&self, anchor: Rect, max_rows: usize, bounds: Rect) -> Rect {
        let rows = self.matches.len().clamp(1, max_rows.max(1)) as f64;
        let size = Size::new(
            anchor.size.width.max(self.list_width),
            rows * self.metrics.row + 2.0 * self.metrics.inset,
        );
        let anchor = gap_anchor(anchor, self.metrics.inset, None);
        place_popup(anchor, size, bounds, Placement::Below, LayoutDirection::Ltr).rect
    }

    /// The suggestion under `point` in a popup at `popup`, as an index in
    /// [`Combobox::suggestions`].
    pub fn suggestion_at(&self, popup: Rect, point: Point) -> Option<usize> {
        if !popup.contains(point) {
            return None;
        }
        let y = point.y - popup.origin.y - self.metrics.inset;
        let row = (y / self.metrics.row).floor();
        if row < 0.0 || row as usize >= self.rows_in(popup) {
            return None;
        }
        let index = self.first + row as usize;
        (index < self.matches.len()).then_some(index)
    }

    /// Paints the open popup in the context's bounds, from
    /// [`Combobox::popup_rect`]. Paint it above other content.
    pub fn paint_popup(&self, cx: &mut PaintContext<'_>) {
        if self.open {
            self.paint_rows(cx);
        }
    }

    /// Paints the popup box and its rows in the context's bounds. With no
    /// suggestion, the popup shows one muted "No results" row.
    fn paint_rows(&self, cx: &mut PaintContext<'_>) {
        let popup = cx.bounds;
        let query = |slot| StyleQuery::new(ControlKind::Combobox, slot);
        let content = cx.style(query(Slot::Content));
        cx.slot(popup, &content, |_, _| {});
        let inner = content_rect(popup, &content, cx.rtl);
        let row_at = |index: usize| {
            Rect::from_xywh(
                inner.origin.x,
                inner.origin.y + index as f64 * self.metrics.row,
                inner.size.width,
                self.metrics.row,
            )
        };
        if self.matches.is_empty() {
            let style = cx.style(query(Slot::Item));
            let muted = cx.style(query(Slot::Placeholder));
            let empty = cx.strings.message("choscorui.combobox.no_results", &[]);
            cx.slot(row_at(0), &style, |cx, text| {
                cx.label(text, &empty, &muted, TextAlign::Start);
            });
            return;
        }
        let rows = self.rows_in(popup);
        let shown = self.suggestions().into_iter().skip(self.first).take(rows);
        for (index, item) in shown.enumerate() {
            let state = VisualState::NONE
                .set(VisualState::HIGHLIGHTED, self.highlighted == Some(item.key))
                .set(VisualState::SELECTED, self.selected == Some(item.key));
            let style = cx.style(query(Slot::Item).state(state));
            let indicator = cx.style(query(Slot::ItemIndicator).state(state));
            let row = row_at(index);
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
            value: Some(self.text().to_owned()),
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
/// installed resolves to the fallback and is reported as missing. Empty
/// text means the fallback, and is not missing. The resolved family
/// follows only committed text, so typing does not change it.
pub struct FontChooser {
    combo: Combobox,
    fallback: String,
}

impl FontChooser {
    pub fn new(families: Vec<String>, current: &str, fallback: impl Into<String>) -> Self {
        // The semantics name the chooser in the user's language.
        let mut combo = Combobox::new("", true);
        combo.set_items(
            families
                .into_iter()
                .map(|family| ComboItem {
                    key: text_key(&family),
                    text: family,
                })
                .collect(),
        );
        combo.set_text(current);
        combo.commit_key(None);
        Self {
            combo,
            fallback: fallback.into(),
        }
    }
    /// Sets the accessible name of the chooser. It is "Font" until set.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.combo.name = name.into();
        self
    }
    pub fn combo(&self) -> &Combobox {
        &self.combo
    }
    pub fn handle(&mut self, input: ComboInput) -> ComboChange {
        self.combo.handle(input)
    }
    /// True when the committed text names a family that is not installed.
    pub fn is_missing(&self) -> bool {
        let (text, key) = &self.combo.committed;
        key.is_none() && !text.trim().is_empty()
    }
    /// The family to render with: the committed installed family, or the
    /// fallback when the committed family is missing or empty.
    pub fn resolved_family(&self) -> &str {
        self.combo
            .committed
            .1
            .and_then(|key| self.combo.items().iter().find(|item| item.key == key))
            .map_or(self.fallback.as_str(), |item| item.text.as_str())
    }
    /// The semantics, named and described in the language of `strings`.
    pub fn semantics(&self, strings: &Localizer) -> SemanticNode {
        let mut node = self.combo.semantics();
        if node.name.is_empty() {
            node.name = strings.message("choscorui.font", &[]);
        }
        if self.is_missing() {
            node.description =
                strings.message("choscorui.font.fallback", &[("font", &self.fallback)]);
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

    /// The hosted popup below the trigger.
    fn hosted_popup(&self) -> Rect {
        self.popup_rect(self.anchor, POPUP_ROWS, self.bounds)
    }

    /// Reports the changes since `before` (text and selection) and the
    /// open state to the host.
    fn report(&mut self, cx: &mut ControlContext<'_>, before: (String, Option<ItemKey>)) {
        let (text, selected) = before;
        if self.editable && self.text() != text {
            cx.emit(ControlMessage::Changed(ControlValue::Text(
                self.text().to_owned(),
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

    /// Maps a pointer event to an input, and replies. A press in editable
    /// text goes to the editing model; only the chevron toggles the popup.
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
                } else if self.editable && !self.chevron.contains(position) {
                    (None, EventReply::Continue)
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
            PointerEvent::Wheel {
                position,
                delta,
                unit,
            } if self.open && popup.contains(position) => {
                let rows = match unit {
                    WheelUnit::Lines => delta.y,
                    WheelUnit::Points => delta.y / self.metrics.row,
                };
                if self.scroll_rows(rows.round() as isize) {
                    cx.request_paint();
                }
                (None, EventReply::Stop)
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

    /// Gives `event` to the editing model of editable text, and reports
    /// a text change as an edit.
    fn edit_event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if !self.editable || !self.enabled {
            return EventReply::Continue;
        }
        let before = (self.text().to_owned(), self.selected);
        let reply = self.editor.hosted_event(cx, event, self.text_rect);
        if self.text() != before.0 {
            self.after_edit();
            self.report(cx, before);
        }
        reply
    }

    /// The width of the widest item in `spec`, measured once for each
    /// item list and text style.
    fn widest_item(&mut self, cx: &MeasureContext<'_>, spec: TypeSpec) -> f64 {
        if let Some((_, width)) = self.widest.iter().find(|(known, _)| *known == spec) {
            return *width;
        }
        let width = self
            .items
            .iter()
            .map(|item| cx.text.width(&item.text, &spec))
            .fold(0.0, f64::max);
        self.widest.push((spec, width));
        width
    }
}

impl CustomControl for Combobox {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let query = |slot| StyleQuery::new(ControlKind::Combobox, slot);
        let (trigger, value) = (cx.style(query(Slot::Trigger)), cx.style(query(Slot::Value)));
        let widest = self.widest_item(cx, value.text);
        let widest = widest
            .max(cx.text.width(self.text(), &value.text))
            .max(cx.text.width(&self.placeholder, &value.text));
        let width = trigger.padding.horizontal() + widest + trigger.gap + trigger.icon_size;
        let height = (value.text.line_height + trigger.padding.vertical()).max(trigger.min_height);
        Measured::new(Size::new(width.max(trigger.min_width), height))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.set_metrics(cx.styles, cx.tokens);
        self.anchor = bounds;
        self.bounds = cx.popup_bounds();
        let query = |slot| StyleQuery::new(ControlKind::Combobox, slot);
        let (content, item) = (cx.style(query(Slot::Content)), cx.style(query(Slot::Item)));
        let widest = self.widest_item(cx, item.text);
        self.list_width = widest + item.padding.horizontal() + content.padding.horizontal();
        let trigger = cx.style(query(Slot::Trigger));
        (self.text_rect, self.chevron) = trigger_parts(bounds, &trigger, cx.rtl());
        // The rows that fit when the popup has all of its rows.
        let full = place_popup(
            gap_anchor(bounds, self.metrics.inset, None),
            Size::new(
                bounds.size.width.max(self.list_width),
                POPUP_ROWS as f64 * self.metrics.row + 2.0 * self.metrics.inset,
            ),
            self.bounds,
            Placement::Below,
            LayoutDirection::Ltr,
        );
        self.page = self.rows_in(full.rect).clamp(1, POPUP_ROWS);
        self.reveal_highlight();
        self.motion_ms = cx
            .style(StyleQuery::new(ControlKind::Combobox, Slot::Content).state(VisualState::OPEN))
            .motion_ms;
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let (input, reply) = match event {
            ControlEvent::Pointer(pointer) => self.pointer_input(cx, pointer),
            ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                let input = ComboInput::Key {
                    key: *key,
                    alt: cx.modifiers().option,
                };
                (Some(input), EventReply::Continue)
            }
            ControlEvent::Dismiss | ControlEvent::FocusChanged(false) => {
                // The editing model ends a composition and a drag first.
                self.edit_event(cx, event);
                let input = if matches!(event, ControlEvent::Dismiss) {
                    ComboInput::Dismiss
                } else {
                    ComboInput::Blur
                };
                (Some(input), EventReply::Continue)
            }
            _ => (None, EventReply::Continue),
        };
        if let Some(input) = input {
            let before = (self.text().to_owned(), self.selected);
            let tab = matches!(input, ComboInput::Key { key: Key::Tab, .. });
            // Escape, an outside press, and focus loss close without a
            // choice.
            let dismiss = matches!(
                input,
                ComboInput::Dismiss
                    | ComboInput::Blur
                    | ComboInput::Key {
                        key: Key::Escape,
                        ..
                    }
            );
            let change = self.handle(input);
            if change != ComboChange::None {
                self.report(cx, before);
                if dismiss && change == ComboChange::Closed {
                    cx.emit(ControlMessage::Dismissed);
                }
                // Tab commits and still moves the focus.
                return if tab {
                    EventReply::Continue
                } else {
                    EventReply::Stop
                };
            }
        }
        if reply == EventReply::Stop || matches!(event, ControlEvent::FocusChanged(false)) {
            return reply;
        }
        self.edit_event(cx, event)
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

    fn text_input(&self) -> Option<TextInputInfo> {
        self.editable
            .then(|| self.editor.hosted_text_input())
            .flatten()
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
        let before = (self.text().to_owned(), self.selected);
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
                let key = self.find_text(text);
                if key.is_none() && !self.editable {
                    return false;
                }
                self.select(key);
                self.set_text(text);
                self.committed.0.clone_from(text);
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
        let host = cx.state.set(
            VisualState::INVALID,
            cx.state.invalid() || self.is_missing(),
        );
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
        self.semantics_in(Localizer::builtin())
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        FontChooser::semantics(self, strings)
    }

    fn text_input(&self) -> Option<TextInputInfo> {
        self.combo.text_input()
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
