//! Choosing one of several: radio groups, tabs, combo boxes, and the font
//! chooser.

use crate::page::{Column, Env, Page, node, pressed, sem};
use choscorui::controls::{
    ComboChange, ComboInput, ComboItem, Combobox, CompletionMode, FontChooser, RadioGroup,
    RadioInput, RadioOption, ShortcutModifiers, Tab, TabInput, Tabs, TabsVariant, VisualState,
    text_key,
};
use choscorui::core::{
    ColorRole, DisplayList, Key, KeyEvent, NodeId, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, Size, Tree,
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
                let _ = tree.set_bounds(*id, column.cells(20.0, &[280.0])[0]);
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
            for (id, _) in options {
                let rect = tree.bounds(*id).unwrap_or_default();
                group.paint_option(&mut env.cx(list, rect), tree, *id);
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
        _env: &Env<'_>,
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
                if rect.contains(position) {
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

/// One tab list: its node, the tabs, and each tab's node, panel node,
/// label, and panel text.
type TabList = (
    NodeId,
    Tabs,
    Vec<(NodeId, NodeId, &'static str, &'static str)>,
);

/// Tabs in the default and line looks, with three panels each; the last
/// tab is disabled.
pub struct TabsPage {
    sets: Vec<TabList>,
}

pub fn tabs(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let content = [
        ("General", "Name, icon, and startup behavior."),
        ("Appearance", "Theme, accent color, and font size."),
        ("Advanced (disabled)", "Hidden settings."),
    ];
    let sets = [TabsVariant::Default, TabsVariant::Line]
        .into_iter()
        .map(|variant| {
            let list_node = node(tree, parent, false);
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
            let tabs = Tabs::new("Settings", tab_list, items[0].0)
                .expect("tabs are distinct")
                .with_variant(variant);
            tabs.install(tree).expect("tabs are attached");
            (list_node, tabs, items)
        })
        .collect();
    Box::new(TabsPage { sets })
}

impl TabsPage {
    fn owner(&mut self, node: NodeId) -> Option<&mut Tabs> {
        self.sets
            .iter_mut()
            .find(|(_, _, items)| items.iter().any(|(tab, _, _, _)| *tab == node))
            .map(|(_, tabs, _)| tabs)
    }
}

impl Page for TabsPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (list_node, tabs, items) in &self.sets {
            let row = column.row(36.0);
            let widths: Vec<f64> = items
                .iter()
                .map(|(tab, _, _, _)| {
                    tabs.measure_tab(*tab, env.styles(), env.tokens, env)
                        .map_or(120.0, |size| size.width)
                })
                .collect();
            let total = widths.iter().sum::<f64>() + 6.0;
            let list = Rect::new(
                Point::new(
                    if env.rtl() {
                        row.max_x() - total
                    } else {
                        row.origin.x
                    },
                    row.origin.y,
                ),
                Size::new(total, row.size.height),
            );
            let _ = tree.set_bounds(*list_node, list);
            let cells = crate::page::split(list.inset(3.0), &widths, 0.0, env.rtl());
            let panel = column.row(96.0);
            for ((tab, panel_node, _, _), cell) in items.iter().zip(cells) {
                let _ = tree.set_bounds(*tab, cell);
                let _ = tree.set_bounds(*panel_node, panel);
            }
            column.space(12.0);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (list_node, tabs, items) in &self.sets {
            let bounds = tree.bounds(*list_node).unwrap_or_default();
            tabs.paint_list(&mut env.cx(list, bounds));
            for (tab, panel, _, body) in items {
                let rect = tree.bounds(*tab).unwrap_or_default();
                tabs.paint_tab(&mut env.cx(list, rect), tree, *tab);
                if tabs.selected_tab() == *tab {
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
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = Vec::new();
        for (list_node, tabs, items) in &self.sets {
            nodes.push((*list_node, tabs.group_semantics()));
            for (tab, panel, _, _) in items {
                if let Some(node) = tabs.tab_semantics(*tab) {
                    nodes.push((*tab, node));
                }
                if let Some(node) = tabs.panel_semantics(*panel) {
                    nodes.push((*panel, node));
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
            .sets
            .iter()
            .flat_map(|(_, _, items)| items.iter())
            .find(|(tab, _, _, _)| tree.bounds(*tab).is_some_and(|b| b.contains(position)))
            .map(|(tab, _, _, _)| *tab);
        match hit.and_then(|tab| Some((tab, self.owner(tab)?))) {
            Some((tab, tabs)) => matches!(tabs.handle(tree, tab, TabInput::Activate), Ok(Some(_))),
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
        let Some(tabs) = self.owner(focus) else {
            return false;
        };
        let _ = tabs.handle(tree, focus, TabInput::Key(event));
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
        self.owner(node).is_some_and(|tabs| {
            matches!(
                tabs.handle(tree, node, TabInput::Semantic(action)),
                Ok(Some(_))
            )
        })
    }
}

/// Draws a combobox as a field with a popup list of suggestions and turns
/// pointer and key events into `ComboInput`s for whoever owns the combo.
pub struct ComboField {
    pub node: NodeId,
}

/// The space a page reserves below a field for the popup: rows of the
/// default recipe's 32 pt height plus the 4 pt insets.
pub const POPUP_SPACE: f64 = MAX_SUGGESTIONS as f64 * 32.0 + 8.0;
pub const MAX_SUGGESTIONS: usize = 6;

impl ComboField {
    pub fn new(tree: &mut Tree, parent: NodeId) -> Self {
        Self {
            node: node(tree, parent, true),
        }
    }

    fn popup(&self, tree: &Tree, combo: &Combobox) -> Rect {
        let bounds = tree.bounds(self.node).unwrap_or_default();
        combo.popup_rect(bounds, MAX_SUGGESTIONS)
    }

    pub fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>, combo: &Combobox) {
        let bounds = tree.bounds(self.node).unwrap_or_default();
        let state = VisualState::NONE
            .set(VisualState::FOCUSED, tree.focus() == Some(self.node))
            .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(self.node));
        combo.paint(&mut env.cx(list, bounds), state, "Type or press ↓");
    }

    /// Paints the open popup; call after everything else so it is on top.
    pub fn paint_popup(
        &self,
        tree: &Tree,
        list: &mut DisplayList,
        env: &Env<'_>,
        combo: &Combobox,
    ) {
        combo.paint_popup(&mut env.cx(list, self.popup(tree, combo)));
    }

    /// The input a pointer event means for `combo`, if any.
    pub fn pointer(
        &self,
        tree: &mut Tree,
        event: PointerEvent,
        combo: &Combobox,
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
                return combo.suggestion_at(popup, position).map(ComboInput::Choose);
            }
        }
        if tree.bounds(self.node).is_some_and(|b| b.contains(position)) {
            let _ = tree.focus_by_pointer(self.node);
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
        combo: &Combobox,
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
pub struct ComboboxPage {
    combos: Vec<(ComboField, Combobox, &'static str, NodeId)>,
}

pub fn combobox(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
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
        let mut combo = Combobox::new(label, editable);
        combo.set_completion_mode(mode);
        combo.set_items(items(&cities));
        combo.set_enabled(env.enabled);
        if !editable {
            combo.select(Some(text_key("Huế")));
        }
        let caption = node(tree, parent, false);
        combos.push((ComboField::new(tree, parent), combo, label, caption));
    }
    Box::new(ComboboxPage { combos })
}

impl Page for ComboboxPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (field, combo, _, caption) in &mut self.combos {
            combo.set_metrics(env.styles(), env.tokens);
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
        column.space(POPUP_SPACE);
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
            env.tokens.color(ColorRole::Foreground),
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
