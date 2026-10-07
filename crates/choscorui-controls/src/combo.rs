//! Combo box with completion, and a font chooser built on it.
//!
//! Items carry stable [`ItemKey`]s, so the selection and the highlighted
//! suggestion survive item lists that update while the popup is open
//! (for example asynchronous search results).

use choscorui_core::{
    ItemKey, Key, SemanticAction, SemanticNode, SemanticRole, SemanticState, fold_for_search,
};

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

pub struct ComboBox {
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
    enabled: bool,
}

impl ComboBox {
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
            enabled: true,
        }
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
    combo: ComboBox,
    fallback: String,
}

impl FontChooser {
    pub fn new(families: Vec<String>, current: &str, fallback: impl Into<String>) -> Self {
        let mut combo = ComboBox::new("Font", true);
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
    pub fn combo(&self) -> &ComboBox {
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
