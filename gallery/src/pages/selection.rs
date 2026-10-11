//! Choosing one of several: radio groups, tabs, combo boxes, and the font
//! chooser.

use crate::page::{Align, Column, Env, Page, node, pressed, sem};
use choscorui::controls::{
    ComboBox, ComboChange, ComboInput, ComboItem, CompletionMode, FontChooser, RadioGroup,
    RadioInput, RadioOption, ShortcutModifiers, Tab, TabInput, TabSet, text_key,
};
use choscorui::core::{
    DisplayList, Key, KeyEvent, NodeId, Point, PointerButton, PointerEvent, Rect, SemanticAction,
    SemanticNode, SemanticRole, Size, Tree,
};

/// A radio group's caption node, the group, and each option's node and
/// label.
type Radios = (NodeId, RadioGroup, Vec<(NodeId, &'static str)>);

/// Two radio groups, one with a disabled option.
pub struct RadioPage {
    groups: Vec<Radios>,
}

pub fn radio(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let mut groups = Vec::new();
    for (label, options, disabled) in [
        ("Paper size", &["A4", "Letter", "Legal"][..], None),
        (
            "Quality",
            &["Draft", "Normal", "Best (unavailable)"][..],
            Some(2),
        ),
    ] {
        let group_node = node(tree, parent, false);
        let nodes: Vec<(NodeId, &'static str)> = options
            .iter()
            .map(|text| (node(tree, group_node, false), *text))
            .collect();
        let radio_options = nodes
            .iter()
            .enumerate()
            .map(|(index, (id, text))| {
                let option = RadioOption::new(*id, *text);
                if Some(index) == disabled {
                    option.disabled()
                } else {
                    option
                }
            })
            .collect();
        let group =
            RadioGroup::new(label, radio_options, nodes[0].0).expect("options are distinct");
        group.install(tree).expect("options are attached");
        groups.push((group_node, group, nodes));
    }
    Box::new(RadioPage { groups })
}

impl Page for RadioPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (group_node, _, options) in &self.groups {
            let top = column.y;
            column.space(24.0);
            for (id, _) in options {
                let _ = tree.set_bounds(*id, column.cells(20.0, &[20.0])[0]);
            }
            let group = Rect::new(
                Point::new(bounds.origin.x, top),
                Size::new(bounds.size.width, column.y - top),
            );
            let _ = tree.set_bounds(*group_node, group);
            column.space(10.0);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (group_node, group, options) in &self.groups {
            let bounds = tree.bounds(*group_node).unwrap_or_default();
            let title = Rect::new(bounds.origin, Size::new(bounds.size.width, 20.0));
            env.caption(list, title, &group.group_semantics().name);
            for (id, text) in options {
                let Some(state) = group.visual_state(tree, *id) else {
                    continue;
                };
                let rect = tree.bounds(*id).unwrap_or_default();
                env.radio_style().paint(list, rect, state);
                let color = if state.enabled {
                    env.tokens.content
                } else {
                    env.tokens.content.with_alpha(0.4)
                };
                env.text(
                    list,
                    crate::pages::inputs::beside(rect, 260.0, env),
                    text,
                    env.body(),
                    color,
                    Align::Leading,
                );
            }
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = Vec::new();
        for (group_node, group, options) in &self.groups {
            nodes.push((*group_node, group.group_semantics()));
            for (id, _) in options {
                if let Some(node) = group.semantics(*id) {
                    nodes.push((*id, node));
                }
            }
        }
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = event
        else {
            return false;
        };
        for (_, group, options) in &mut self.groups {
            for (id, _) in options.iter() {
                let rect = tree.bounds(*id).unwrap_or_default();
                let label = crate::pages::inputs::beside(rect, 260.0, env);
                if rect.contains(position) || label.contains(position) {
                    return matches!(group.handle(tree, *id, RadioInput::Activate), Ok(Some(_)));
                }
            }
        }
        false
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let Some(focus) = tree.focus() else {
            return false;
        };
        for (_, group, options) in &mut self.groups {
            if options.iter().any(|(id, _)| *id == focus) {
                let _ = group.handle(tree, focus, RadioInput::Key(event));
                return true;
            }
        }
        false
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        for (_, group, options) in &mut self.groups {
            if options.iter().any(|(id, _)| *id == node) {
                return matches!(
                    group.handle(tree, node, RadioInput::Semantic(action)),
                    Ok(Some(_))
                );
            }
        }
        false
    }
}

/// A tab set with three panels; the last tab is disabled.
pub struct TabsPage {
    list_node: NodeId,
    tabs: TabSet,
    items: Vec<(NodeId, NodeId, &'static str, &'static str)>,
}

pub fn tabs(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let list_node = node(tree, parent, false);
    let content = [
        ("General", "Name, icon, and startup behavior."),
        ("Appearance", "Theme, accent color, and font size."),
        ("Advanced (disabled)", "Hidden settings."),
    ];
    let items: Vec<_> = content
        .iter()
        .map(|(label, body)| {
            (
                node(tree, list_node, false),
                node(tree, parent, false),
                *label,
                *body,
            )
        })
        .collect();
    let tab_list = items
        .iter()
        .enumerate()
        .map(|(index, (tab, panel, label, _))| {
            let tab = Tab::new(*tab, *panel, *label);
            if index == 2 { tab.disabled() } else { tab }
        })
        .collect();
    let tabs = TabSet::new("Settings", tab_list, items[0].0).expect("tabs are distinct");
    tabs.install(tree).expect("tabs are attached");
    Box::new(TabsPage {
        list_node,
        tabs,
        items,
    })
}

impl Page for TabsPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let row = column.row(32.0);
        let _ = tree.set_bounds(self.list_node, row);
        let cells = crate::page::split(row, &[120.0, 140.0, 180.0], 2.0, env.rtl());
        let panel = column.row(160.0);
        for ((tab, panel_node, _, _), cell) in self.items.iter().zip(cells) {
            let _ = tree.set_bounds(*tab, cell);
            let _ = tree.set_bounds(*panel_node, panel);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (tab, panel, label, body) in &self.items {
            let Some(state) = self.tabs.visual_state(tree, *tab) else {
                continue;
            };
            let rect = tree.bounds(*tab).unwrap_or_default();
            env.tab_style().paint(list, rect, state);
            let color = if state.enabled {
                env.tokens.content
            } else {
                env.tokens.content.with_alpha(0.4)
            };
            env.text(list, rect, label, env.body(), color, Align::Center);
            if state.selected {
                let panel = tree.bounds(*panel).unwrap_or_default();
                env.panel(list, panel);
                env.label(
                    list,
                    Rect::new(
                        Point::new(panel.origin.x + 12.0, panel.origin.y + 12.0),
                        Size::new(panel.size.width - 24.0, 24.0),
                    ),
                    body,
                );
            }
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = vec![(self.list_node, self.tabs.group_semantics())];
        for (tab, panel, _, _) in &self.items {
            if let Some(node) = self.tabs.tab_semantics(*tab) {
                nodes.push((*tab, node));
            }
            if let Some(node) = self.tabs.panel_semantics(*panel) {
                nodes.push((*panel, node));
            }
        }
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = event
        else {
            return false;
        };
        let hit = self
            .items
            .iter()
            .find(|(tab, _, _, _)| tree.bounds(*tab).is_some_and(|b| b.contains(position)))
            .map(|(tab, _, _, _)| *tab);
        match hit {
            Some(tab) => matches!(self.tabs.handle(tree, tab, TabInput::Activate), Ok(Some(_))),
            None => false,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let Some(focus) = tree.focus() else {
            return false;
        };
        if !self.items.iter().any(|(tab, _, _, _)| *tab == focus) {
            return false;
        }
        let _ = self.tabs.handle(tree, focus, TabInput::Key(event));
        true
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        self.items.iter().any(|(tab, _, _, _)| *tab == node)
            && matches!(
                self.tabs.handle(tree, node, TabInput::Semantic(action)),
                Ok(Some(_))
            )
    }
}

/// Draws a combo box as a field with a popup list of suggestions and turns
/// pointer and key events into `ComboInput`s for whoever owns the combo.
pub struct ComboField {
    pub node: NodeId,
}

const SUGGESTION_HEIGHT: f64 = 24.0;
pub const MAX_SUGGESTIONS: usize = 6;

impl ComboField {
    pub fn new(tree: &mut Tree, parent: NodeId) -> Self {
        Self {
            node: node(tree, parent, true),
        }
    }

    fn popup(&self, tree: &Tree, combo: &ComboBox) -> Rect {
        let bounds = tree.bounds(self.node).unwrap_or_default();
        let count = combo.suggestions().len().min(MAX_SUGGESTIONS);
        Rect::new(
            Point::new(bounds.origin.x, bounds.max_y() + 2.0),
            Size::new(bounds.size.width, count as f64 * SUGGESTION_HEIGHT),
        )
    }

    pub fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>, combo: &ComboBox) {
        let bounds = tree.bounds(self.node).unwrap_or_default();
        let focused = tree.focus() == Some(self.node);
        let text = combo.text().to_owned();
        let shown = match combo.completion_suffix() {
            Some(suffix) if focused => format!("{text}{suffix}"),
            _ => text,
        };
        crate::page::paint_field(
            env,
            list,
            bounds,
            &shown,
            "Type or press ↓",
            focused,
            &choscorui::controls::ValidationState::Valid,
            None,
        );
        let arrow = Rect::new(
            Point::new(bounds.max_x() - 24.0, bounds.origin.y),
            Size::new(24.0, bounds.size.height),
        );
        env.text(
            list,
            arrow,
            "▾",
            env.small(),
            env.tokens.content,
            Align::Center,
        );
    }

    /// Paints the open popup; call after everything else so it is on top.
    pub fn paint_popup(
        &self,
        tree: &Tree,
        list: &mut DisplayList,
        env: &Env<'_>,
        combo: &ComboBox,
    ) {
        if !combo.is_open() {
            return;
        }
        let popup = self.popup(tree, combo);
        let _ = list.fill_rounded_rect(popup, 4.0, env.tokens.surface);
        let _ = list.stroke_rounded_rect(popup, 4.0, env.mix(0.3), 1.0);
        let highlighted = combo.highlighted();
        for (index, item) in combo
            .suggestions()
            .into_iter()
            .take(MAX_SUGGESTIONS)
            .enumerate()
        {
            let row = Rect::new(
                Point::new(
                    popup.origin.x,
                    popup.origin.y + index as f64 * SUGGESTION_HEIGHT,
                ),
                Size::new(popup.size.width, SUGGESTION_HEIGHT),
            );
            let selected = Some(item.key) == highlighted;
            if selected {
                list.fill_rect(row, env.tokens.accent);
            }
            env.text(
                list,
                row.inset(6.0),
                &item.text,
                env.body(),
                if selected {
                    env.on_accent()
                } else {
                    env.tokens.content
                },
                Align::Leading,
            );
        }
    }

    /// The input a pointer event means for `combo`, if any.
    pub fn pointer(
        &self,
        tree: &mut Tree,
        event: PointerEvent,
        combo: &ComboBox,
    ) -> Option<ComboInput> {
        let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = event
        else {
            return None;
        };
        if combo.is_open() {
            let popup = self.popup(tree, combo);
            if popup.contains(position) {
                let index = ((position.y - popup.origin.y) / SUGGESTION_HEIGHT) as usize;
                return Some(ComboInput::Choose(index));
            }
        }
        if tree.bounds(self.node).is_some_and(|b| b.contains(position)) {
            let _ = tree.set_focus(Some(self.node));
            return Some(ComboInput::Press);
        }
        combo.is_open().then_some(ComboInput::Dismiss)
    }

    /// The input a key event means while the field is focused.
    pub fn key(
        &self,
        tree: &Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        combo: &ComboBox,
    ) -> Option<ComboInput> {
        if tree.focus() != Some(self.node) {
            return None;
        }
        let key = pressed(event)?;
        Some(if matches!(key, Key::Backspace | Key::Delete) {
            let mut text = combo.text().to_owned();
            text.pop();
            ComboInput::SetText(text)
        } else {
            ComboInput::Key {
                key,
                alt: modifiers.option,
            }
        })
    }

    pub fn text(&self, tree: &Tree, text: &str) -> Option<ComboInput> {
        if tree.focus() != Some(self.node) {
            return None;
        }
        let printable: String = text.chars().filter(|c| !c.is_control()).collect();
        (!printable.is_empty()).then_some(ComboInput::Text(printable))
    }
}

fn items(texts: &[&str]) -> Vec<ComboItem> {
    texts
        .iter()
        .map(|text| ComboItem {
            key: text_key(text),
            text: (*text).to_owned(),
        })
        .collect()
}

/// A read-only combo, an editable prefix-completing combo, and an editable
/// contains-matching combo.
pub struct ComboBoxPage {
    combos: Vec<(ComboField, ComboBox, &'static str, NodeId)>,
}

pub fn combo_box(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let cities = [
        "Hà Nội",
        "Huế",
        "Đà Lạt",
        "Hội An",
        "Sa Pa",
        "Hải Phòng",
        "Cần Thơ",
        "Nha Trang",
    ];
    let mut combos = Vec::new();
    for (label, editable, mode) in [
        ("City (read-only)", false, CompletionMode::Prefix),
        (
            "City (editable, prefix completion)",
            true,
            CompletionMode::Prefix,
        ),
        (
            "City (editable, contains match)",
            true,
            CompletionMode::Contains,
        ),
    ] {
        let mut combo = ComboBox::new(label, editable);
        combo.set_completion_mode(mode);
        combo.set_items(items(&cities));
        combo.set_enabled(env.enabled);
        if !editable {
            combo.select(Some(text_key("Huế")));
        }
        let caption = node(tree, parent, false);
        combos.push((ComboField::new(tree, parent), combo, label, caption));
    }
    Box::new(ComboBoxPage { combos })
}

impl Page for ComboBoxPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (field, _, _, caption) in &self.combos {
            let _ = tree.set_bounds(*caption, column.row(20.0));
            let _ = tree.set_bounds(field.node, column.cells(30.0, &[280.0])[0]);
            column.space(8.0);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (field, combo, label, caption) in &self.combos {
            let selected = combo
                .selected()
                .and_then(|key| combo.items().iter().find(|item| item.key == key))
                .map_or("none".to_owned(), |item| item.text.clone());
            env.caption(
                list,
                tree.bounds(*caption).unwrap_or_default(),
                &format!("{label} — selected: {selected}"),
            );
            field.paint(tree, list, env, combo);
        }
        for (field, combo, _, _) in &self.combos {
            field.paint_popup(tree, list, env, combo);
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.combos
            .iter()
            .map(|(field, combo, _, _)| (field.node, combo.semantics()))
            .collect()
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        // An open popup takes the press first.
        let order: Vec<usize> = match self.combos.iter().position(|(_, c, _, _)| c.is_open()) {
            Some(open) => vec![open],
            None => (0..self.combos.len()).collect(),
        };
        for index in order {
            let (field, combo, _, _) = &mut self.combos[index];
            if let Some(input) = field.pointer(tree, event, combo) {
                combo.handle(input);
                return true;
            }
        }
        false
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        for (field, combo, _, _) in &mut self.combos {
            if let Some(input) = field.key(tree, event, modifiers, combo) {
                combo.handle(input);
                return true;
            }
        }
        false
    }

    fn text(&mut self, tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        for (field, combo, _, _) in &mut self.combos {
            if let Some(input) = field.text(tree, text) {
                return combo.handle(input) != ComboChange::None;
            }
        }
        false
    }

    fn action(
        &mut self,
        _tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        match self
            .combos
            .iter_mut()
            .find(|(field, _, _, _)| field.node == node)
        {
            Some((_, combo, _, _)) => {
                combo.handle(ComboInput::Semantic(action, value.map(str::to_owned)))
                    != ComboChange::None
            }
            None => false,
        }
    }
}

/// Font families every page can offer without asking the platform.
pub const FAMILIES: [&str; 8] = [
    "Helvetica",
    "Times New Roman",
    "Courier New",
    "Georgia",
    "Menlo",
    "Avenir Next",
    "Gill Sans",
    "Futura",
];

pub fn families() -> Vec<String> {
    FAMILIES.iter().map(|family| (*family).to_owned()).collect()
}

/// The font chooser with a preview in the resolved family.
pub struct FontChooserPage {
    chooser: FontChooser,
    field: ComboField,
    preview: NodeId,
}

pub fn font_chooser(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    Box::new(FontChooserPage {
        chooser: FontChooser::new(families(), "Georgia", "Helvetica"),
        field: ComboField::new(tree, parent),
        preview: node(tree, parent, false),
    })
}

impl Page for FontChooserPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let _ = tree.set_bounds(self.field.node, column.cells(30.0, &[280.0])[0]);
        column.space(MAX_SUGGESTIONS as f64 * SUGGESTION_HEIGHT);
        let _ = tree.set_bounds(self.preview, column.row(80.0));
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        self.field.paint(tree, list, env, self.chooser.combo());
        let preview = tree.bounds(self.preview).unwrap_or_default();
        env.panel(list, preview);
        let family = self.chooser.resolved_family();
        let _ = list.draw_text(
            Point::new(preview.origin.x + 12.0, preview.origin.y + 40.0),
            "The quick brown fox — Chữ Việt",
            family,
            24.0,
            env.tokens.content,
        );
        env.caption(
            list,
            Rect::new(
                Point::new(preview.origin.x + 12.0, preview.origin.y + 52.0),
                Size::new(preview.size.width - 24.0, 24.0),
            ),
            &if self.chooser.is_missing() {
                format!("Missing family; rendering with the fallback {family}")
            } else {
                format!("Rendering with {family}")
            },
        );
        self.field
            .paint_popup(tree, list, env, self.chooser.combo());
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        vec![
            (self.field.node, self.chooser.semantics()),
            (
                self.preview,
                sem(
                    SemanticRole::StaticText,
                    format!("Preview in {}", self.chooser.resolved_family()),
                    None,
                ),
            ),
        ]
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        match self.field.pointer(tree, event, self.chooser.combo()) {
            Some(input) => {
                self.chooser.handle(input);
                true
            }
            None => false,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        match self.field.key(tree, event, modifiers, self.chooser.combo()) {
            Some(input) => {
                self.chooser.handle(input);
                true
            }
            None => false,
        }
    }

    fn text(&mut self, tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        match self.field.text(tree, text) {
            Some(input) => self.chooser.handle(input) != ComboChange::None,
            None => false,
        }
    }

    fn action(
        &mut self,
        _tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        node == self.field.node
            && self
                .chooser
                .handle(ComboInput::Semantic(action, value.map(str::to_owned)))
                != ComboChange::None
    }
}
