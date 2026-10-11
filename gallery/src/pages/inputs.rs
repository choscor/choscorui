//! Buttons and other momentary or toggled inputs.

use crate::page::{Align, Clicks, Column, Env, Page, node, pressed, sem};
use choscorui::controls::{
    Button, ButtonGroup, ButtonInput, ButtonVariant, Checkbox, CheckboxInput, CommandId,
    CommandRegistry, CommandState, KeySequenceChange, KeySequenceEdit,
    SemanticAction as ControlAction, Shortcut, ShortcutModifiers, ShortcutScope, ToolButton,
    ToolButtonAction, ToolButtonMenu, shortcut_text,
};
use choscorui::core::{
    ColorRole, DisplayList, Icon, Key, KeyEvent, NodeId, PointerEvent, Rect, SemanticAction,
    SemanticNode, SemanticRole, Tree,
};

const HELLO: CommandId = CommandId(1);
const ARCHIVE: CommandId = CommandId(2);

/// Buttons in several variants, a command-driven button, and a window
/// shortcut.
pub struct ButtonPage {
    buttons: Vec<(NodeId, Button)>,
    toggle_node: NodeId,
    toggle: Checkbox,
    status: NodeId,
    commands: CommandRegistry,
    activations: usize,
}

pub fn button(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let mut commands = CommandRegistry::new();
    commands
        .insert(HELLO, CommandState::default())
        .expect("command ids are distinct");
    commands
        .insert(
            ARCHIVE,
            CommandState {
                enabled: false,
                checked: false,
            },
        )
        .expect("command ids are distinct");
    commands
        .bind_shortcut(
            ShortcutScope::Window(0),
            Shortcut {
                key: Key::Enter,
                modifiers: ShortcutModifiers {
                    command: true,
                    ..ShortcutModifiers::default()
                },
            },
            HELLO,
        )
        .expect("shortcut is free");
    let mut buttons = Vec::new();
    for button in [
        Button::new("Say hello (⌘↩)", HELLO),
        Button::new("Outline hello", HELLO)
            .with_variant(ButtonVariant::Outline)
            .with_icon(Icon::Send),
        Button::new("Archive (command disabled)", ARCHIVE).with_variant(ButtonVariant::Destructive),
    ] {
        let id = node(tree, parent, true);
        buttons.push((id, button));
    }
    let toggle_node = node(tree, parent, true);
    let mut toggle = Checkbox::new("Enable the Archive command", false);
    let status = node(tree, parent, false);
    for (id, button) in &mut buttons {
        let _ = button.set_enabled(tree, *id, env.enabled);
    }
    let _ = toggle.set_enabled(tree, toggle_node, env.enabled);
    Box::new(ButtonPage {
        buttons,
        toggle_node,
        toggle,
        status,
        commands,
        activations: 0,
    })
}

impl ButtonPage {
    fn activated(&mut self, command: Option<CommandId>) -> bool {
        match command.and_then(|id| self.commands.activate(id).ok().flatten()) {
            Some(_) => {
                self.activations += 1;
                true
            }
            None => false,
        }
    }

    fn toggled(&mut self, value: Option<bool>) -> bool {
        match value {
            Some(enabled) => {
                let _ = self.commands.set_enabled(ARCHIVE, enabled);
                true
            }
            None => false,
        }
    }
}

impl Page for ButtonPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (id, _) in &self.buttons {
            let cell = column.cells(36.0, &[260.0])[0];
            let _ = tree.set_bounds(*id, cell);
        }
        let _ = tree.set_bounds(self.toggle_node, column.cells(22.0, &[320.0])[0]);
        let _ = tree.set_bounds(self.status, column.row(24.0));
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (id, button) in &self.buttons {
            let bounds = tree.bounds(*id).unwrap_or_default();
            let state = button
                .visual_state_with_commands(tree, *id, &self.commands)
                .expect("button commands are registered");
            button.paint_state(&mut env.cx(list, bounds), state);
        }
        let toggle = tree.bounds(self.toggle_node).unwrap_or_default();
        self.toggle
            .paint(&mut env.cx(list, toggle), tree, self.toggle_node);
        env.caption(
            list,
            tree.bounds(self.status).unwrap_or_default(),
            &format!("Activations: {}", self.activations),
        );
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes: Vec<_> = self
            .buttons
            .iter()
            .map(|(id, button)| {
                let semantics = button
                    .semantics_with_commands(tree, *id, &self.commands)
                    .expect("button commands are registered");
                (*id, semantics.shared())
            })
            .collect();
        nodes.push((
            self.toggle_node,
            self.toggle.semantics(tree, self.toggle_node).shared(),
        ));
        nodes.push((
            self.status,
            sem(
                SemanticRole::StaticText,
                format!("Activations: {}", self.activations),
                None,
            ),
        ));
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let mut changed = false;
        for index in 0..self.buttons.len() {
            let (id, button) = &mut self.buttons[index];
            let command = button
                .handle_with_commands(tree, *id, ButtonInput::Pointer(event), &self.commands)
                .ok()
                .flatten();
            changed |= self.activated(command);
        }
        let value = self
            .toggle
            .handle(tree, self.toggle_node, CheckboxInput::Pointer(event))
            .ok()
            .flatten();
        self.toggled(value) || changed || !matches!(event, PointerEvent::Wheel { .. })
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if let KeyEvent::Down { key, repeat: false } = event
            && let Some(command) =
                self.commands
                    .resolve_shortcut(Shortcut { key, modifiers }, 0, tree.focus())
        {
            return self.activated(Some(command));
        }
        let focus = tree.focus();
        let mut consumed = false;
        for index in 0..self.buttons.len() {
            let (id, button) = &mut self.buttons[index];
            if focus == Some(*id) || matches!(event, KeyEvent::Up { .. }) {
                consumed |= focus == Some(*id);
                let command = button
                    .handle_with_commands(tree, *id, ButtonInput::Key(event), &self.commands)
                    .ok()
                    .flatten();
                self.activated(command);
            }
        }
        if focus == Some(self.toggle_node) {
            let value = self
                .toggle
                .handle(tree, self.toggle_node, CheckboxInput::Key(event))
                .ok()
                .flatten();
            self.toggled(value);
            consumed = true;
        }
        consumed
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        if action != SemanticAction::Activate {
            return false;
        }
        if node == self.toggle_node {
            let value = self
                .toggle
                .handle(tree, node, CheckboxInput::Semantic(ControlAction::Activate))
                .ok()
                .flatten();
            return self.toggled(value);
        }
        let Some(index) = self.buttons.iter().position(|(id, _)| *id == node) else {
            return false;
        };
        let (id, button) = &mut self.buttons[index];
        let command = button
            .handle_with_commands(
                tree,
                *id,
                ButtonInput::Semantic(ControlAction::Activate),
                &self.commands,
            )
            .ok()
            .flatten();
        self.activated(command)
    }

    fn native_disabled(&self) -> bool {
        true
    }
}

/// Checked, unchecked, mixed, invalid, and permanently disabled
/// checkboxes.
pub struct CheckboxPage {
    boxes: Vec<(NodeId, Checkbox)>,
}

pub fn checkbox(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let mut boxes = Vec::new();
    for (label, checked, mixed, invalid, enabled) in [
        ("Show line numbers", true, false, false, true),
        ("Wrap long lines", false, false, false, true),
        ("Select all files", false, true, false, true),
        ("Accept the terms", false, false, true, true),
        ("Sync settings (unavailable)", true, false, false, false),
    ] {
        let id = node(tree, parent, true);
        let mut checkbox = Checkbox::new(label, checked);
        let _ = checkbox.set_indeterminate(tree, id, mixed);
        let _ = checkbox.set_invalid(tree, id, invalid);
        let _ = checkbox.set_enabled(tree, id, enabled && env.enabled);
        boxes.push((id, checkbox));
    }
    Box::new(CheckboxPage { boxes })
}

impl Page for CheckboxPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (id, _) in &self.boxes {
            let _ = tree.set_bounds(*id, column.cells(22.0, &[320.0])[0]);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (id, checkbox) in &self.boxes {
            let bounds = tree.bounds(*id).unwrap_or_default();
            checkbox.paint(&mut env.cx(list, bounds), tree, *id);
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.boxes
            .iter()
            .map(|(id, checkbox)| (*id, checkbox.semantics(tree, *id).shared()))
            .collect()
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        for (id, checkbox) in &mut self.boxes {
            let _ = checkbox.handle(tree, *id, CheckboxInput::Pointer(event));
        }
        true
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let focus = tree.focus();
        let mut consumed = false;
        for (id, checkbox) in &mut self.boxes {
            if focus == Some(*id) || matches!(event, KeyEvent::Up { .. }) {
                consumed |= focus == Some(*id);
                let _ = checkbox.handle(tree, *id, CheckboxInput::Key(event));
            }
        }
        consumed
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let Some((id, checkbox)) = self.boxes.iter_mut().find(|(id, _)| *id == node) else {
            return false;
        };
        action == SemanticAction::Activate
            && matches!(
                checkbox.handle(tree, *id, CheckboxInput::Semantic(ControlAction::Activate)),
                Ok(Some(_))
            )
    }

    fn native_disabled(&self) -> bool {
        true
    }
}

/// Plain, checkable, instant-menu, and split-menu tool buttons.
pub struct ToolButtonPage {
    buttons: Vec<(NodeId, ToolButton)>,
    commands: CommandRegistry,
    clicks: Clicks,
    last: String,
    status: NodeId,
}

pub fn tool_button(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let mut commands = CommandRegistry::new();
    let mut buttons = Vec::new();
    for (index, (label, checkable, menu)) in [
        ("Cut", false, ToolButtonMenu::None),
        ("Bold", true, ToolButtonMenu::None),
        ("Recent", false, ToolButtonMenu::Instant),
        ("Undo", false, ToolButtonMenu::Split),
    ]
    .into_iter()
    .enumerate()
    {
        let command = CommandId(index as u64 + 1);
        commands
            .insert(command, CommandState::default())
            .expect("command ids are distinct");
        let mut button = ToolButton::new(label, command);
        button.checkable = checkable;
        button.menu = menu;
        buttons.push((node(tree, parent, true), button));
    }
    Box::new(ToolButtonPage {
        buttons,
        commands,
        clicks: Clicks::default(),
        last: "None yet".into(),
        status: node(tree, parent, false),
    })
}

impl ToolButtonPage {
    fn record(&mut self, label: &str, action: ToolButtonAction) -> bool {
        self.last = match action {
            ToolButtonAction::None => return false,
            ToolButtonAction::Activated(_) => format!("{label} activated"),
            ToolButtonAction::Toggled(_, on) => {
                format!("{label} {}", if on { "on" } else { "off" })
            }
            ToolButtonAction::OpenMenu => format!("{label} menu opened"),
        };
        true
    }
}

impl Page for ToolButtonPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(32.0, &[72.0, 72.0, 96.0, 96.0]);
        for ((id, _), cell) in self.buttons.iter().zip(cells) {
            let _ = tree.set_bounds(*id, cell);
        }
        let _ = tree.set_bounds(self.status, column.row(24.0));
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (id, button) in &self.buttons {
            let bounds = tree.bounds(*id).unwrap_or_default();
            let checked = self
                .commands
                .state(button.command)
                .is_ok_and(|state| state.checked);
            let fill = if checked {
                choscorui::core::lerp_color(
                    env.tokens.color(ColorRole::Card),
                    env.tokens.color(ColorRole::Primary),
                    0.45,
                )
            } else {
                env.mix(0.08)
            };
            let _ = list.fill_rounded_rect(bounds, 4.0, fill);
            let mut label_rect = bounds;
            if button.menu != ToolButtonMenu::None {
                let arrow = button.arrow_width;
                label_rect.size.width -= arrow;
                let arrow_rect = Rect::new(
                    choscorui::core::Point::new(bounds.max_x() - arrow, bounds.origin.y),
                    choscorui::core::Size::new(arrow, bounds.size.height),
                );
                if button.menu == ToolButtonMenu::Split {
                    list.fill_rect(
                        Rect::new(
                            arrow_rect.origin,
                            choscorui::core::Size::new(1.0, bounds.size.height),
                        ),
                        env.mix(0.3),
                    );
                }
                env.text(
                    list,
                    arrow_rect,
                    "▾",
                    env.small(),
                    env.tokens.color(ColorRole::Foreground),
                    Align::Center,
                );
            }
            env.text(
                list,
                label_rect,
                &button.label,
                env.body(),
                env.tokens.color(ColorRole::Foreground),
                Align::Center,
            );
            if tree.focus() == Some(*id) {
                env.focus_ring(list, bounds);
            }
        }
        env.caption(
            list,
            tree.bounds(self.status).unwrap_or_default(),
            &format!("Last action: {}", self.last),
        );
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes: Vec<_> = self
            .buttons
            .iter()
            .map(|(id, button)| (*id, button.semantics(&self.commands)))
            .collect();
        nodes.push((
            self.status,
            sem(
                SemanticRole::StaticText,
                format!("Last action: {}", self.last),
                None,
            ),
        ));
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let nodes: Vec<NodeId> = self.buttons.iter().map(|(id, _)| *id).collect();
        let Some((id, at)) = self.clicks.feed(tree, event, &nodes) else {
            return false;
        };
        let index = nodes
            .iter()
            .position(|n| *n == id)
            .expect("clicked a button");
        let bounds = tree.bounds(id).unwrap_or_default();
        let action = self.buttons[index]
            .1
            .click(bounds, at.x, &mut self.commands);
        let label = self.buttons[index].1.label.clone();
        self.record(&label, action)
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let Some(key) = pressed(event) else {
            return false;
        };
        let Some(index) = self
            .buttons
            .iter()
            .position(|(id, _)| tree.focus() == Some(*id))
        else {
            return false;
        };
        let action = self.buttons[index].1.handle_key(key, &mut self.commands);
        let label = self.buttons[index].1.label.clone();
        self.record(&label, action)
    }

    fn action(
        &mut self,
        _tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let Some(index) = self.buttons.iter().position(|(id, _)| *id == node) else {
            return false;
        };
        let key = match action {
            SemanticAction::Activate => Key::Enter,
            SemanticAction::ShowMenu => Key::ArrowDown,
            _ => return false,
        };
        let result = self.buttons[index].1.handle_key(key, &mut self.commands);
        let label = self.buttons[index].1.label.clone();
        self.record(&label, result)
    }
}

/// A button group and each button's node, command, and label.
type Group = (ButtonGroup, Vec<(NodeId, CommandId, &'static str)>);

/// An exclusive alignment group and a non-exclusive style group.
pub struct ButtonGroupPage {
    groups: Vec<Group>,
    commands: CommandRegistry,
    clicks: Clicks,
    captions: [NodeId; 2],
}

pub fn button_group(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let mut commands = CommandRegistry::new();
    let captions = [node(tree, parent, false), node(tree, parent, false)];
    let mut groups = Vec::new();
    let mut next = 1;
    for (labels, exclusive) in [
        (&["Left", "Center", "Right", "Justify"][..], true),
        (&["Bold", "Italic", "Underline"][..], false),
    ] {
        let mut members = Vec::new();
        for label in labels {
            let command = CommandId(next);
            next += 1;
            commands
                .insert(
                    command,
                    CommandState {
                        enabled: *label != "Justify",
                        checked: *label == "Left" || *label == "Bold",
                    },
                )
                .expect("command ids are distinct");
            members.push((node(tree, parent, true), command, *label));
        }
        let group = ButtonGroup::new(members.iter().map(|(_, c, _)| *c).collect(), exclusive);
        groups.push((group, members));
    }
    Box::new(ButtonGroupPage {
        groups,
        commands,
        clicks: Clicks::default(),
        captions,
    })
}

impl Page for ButtonGroupPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for ((_, members), caption) in self.groups.iter().zip(self.captions) {
            let _ = tree.set_bounds(caption, column.row(22.0));
            let widths = vec![92.0; members.len()];
            for ((id, _, _), cell) in members.iter().zip(column.cells(30.0, &widths)) {
                let _ = tree.set_bounds(*id, cell);
            }
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let captions = [
            "Exclusive (arrow keys move the choice; Justify is disabled)",
            "Non-exclusive (each toggles)",
        ];
        for (caption, text) in self.captions.iter().zip(captions) {
            env.caption(list, tree.bounds(*caption).unwrap_or_default(), text);
        }
        for (_, members) in &self.groups {
            for (id, command, label) in members {
                let bounds = tree.bounds(*id).unwrap_or_default();
                let state = self.commands.state(*command).unwrap_or_default();
                let fill = if state.checked {
                    choscorui::core::lerp_color(
                        env.tokens.color(ColorRole::Card),
                        env.tokens.color(ColorRole::Primary),
                        0.5,
                    )
                } else {
                    env.mix(0.08)
                };
                list.fill_rect(bounds, fill);
                let color = if state.enabled {
                    env.tokens.color(ColorRole::Foreground)
                } else {
                    env.tokens.color(ColorRole::Foreground).with_alpha(0.35)
                };
                env.text(list, bounds, label, env.body(), color, Align::Center);
                if tree.focus() == Some(*id) {
                    env.focus_ring(list, bounds);
                }
            }
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = Vec::new();
        for (index, (_, members)) in self.groups.iter().enumerate() {
            nodes.push((
                self.captions[index],
                sem(
                    SemanticRole::StaticText,
                    if index == 0 { "Alignment" } else { "Style" },
                    None,
                ),
            ));
            for (id, command, label) in members {
                let state = self.commands.state(*command).unwrap_or_default();
                let mut node = sem(
                    if index == 0 {
                        SemanticRole::RadioButton
                    } else {
                        SemanticRole::Checkbox
                    },
                    *label,
                    None,
                );
                node.state.checked = Some(state.checked);
                node.state.disabled = !state.enabled;
                node.actions = vec![SemanticAction::Activate];
                nodes.push((*id, node));
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
        let nodes: Vec<NodeId> = self
            .groups
            .iter()
            .flat_map(|(_, members)| members.iter().map(|(id, _, _)| *id))
            .collect();
        match self.clicks.feed(tree, event, &nodes) {
            Some((id, _)) => self.toggle(id),
            None => false,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        let Some(key) = pressed(event) else {
            return false;
        };
        let Some(focus) = tree.focus() else {
            return false;
        };
        let Some((group, _)) = self
            .groups
            .iter()
            .find(|(_, members)| members.iter().any(|(id, _, _)| *id == focus))
        else {
            return false;
        };
        let forward = match (key, env.rtl()) {
            (Key::ArrowRight, false) | (Key::ArrowLeft, true) => Some(true),
            (Key::ArrowLeft, false) | (Key::ArrowRight, true) => Some(false),
            _ => None,
        };
        match key {
            Key::Space | Key::Enter => self.toggle(focus),
            _ => match forward {
                Some(forward) => group.clone().step(&mut self.commands, forward).is_some(),
                None => false,
            },
        }
    }

    fn action(
        &mut self,
        _tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        action == SemanticAction::Activate && self.toggle(node)
    }
}

impl ButtonGroupPage {
    fn toggle(&mut self, node: NodeId) -> bool {
        for (group, members) in &self.groups {
            if let Some((_, command, _)) = members.iter().find(|(id, _, _)| *id == node) {
                return group.toggle(&mut self.commands, *command);
            }
        }
        false
    }
}

/// Records a shortcut of up to two chords.
pub struct KeySequencePage {
    field: NodeId,
    edit: KeySequenceEdit,
    examples: NodeId,
    clicks: Clicks,
    last: KeySequenceChange,
}

pub fn key_sequence(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    Box::new(KeySequencePage {
        field: node(tree, parent, true),
        edit: KeySequenceEdit::new(2),
        examples: node(tree, parent, false),
        clicks: Clicks::default(),
        last: KeySequenceChange::None,
    })
}

fn examples() -> String {
    let all = ShortcutModifiers {
        shift: true,
        control: true,
        option: true,
        command: true,
    };
    let command = ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    };
    [
        Shortcut {
            key: Key::Character('s'),
            modifiers: command,
        },
        Shortcut {
            key: Key::ArrowLeft,
            modifiers: all,
        },
        Shortcut {
            key: Key::Enter,
            modifiers: ShortcutModifiers::default(),
        },
    ]
    .map(shortcut_text)
    .join("   ")
}

impl Page for KeySequencePage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let _ = tree.set_bounds(self.field, column.cells(34.0, &[320.0])[0]);
        let _ = tree.set_bounds(self.examples, column.row(60.0));
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let field = tree.bounds(self.field).unwrap_or_default();
        let mut state = choscorui::controls::TextFieldVisualState {
            enabled: env.enabled,
            focused: tree.focus() == Some(self.field),
            validation: choscorui::controls::ValidationKind::Valid,
        };
        if self.edit.is_recording() {
            state.validation = choscorui::controls::ValidationKind::Intermediate;
        }
        env.field_style().paint(list, field, state);
        let text = if self.edit.is_recording() {
            "Recording… press keys, Escape cancels".to_owned()
        } else if self.edit.sequence().is_empty() {
            "Click or press Return to record".to_owned()
        } else {
            self.edit.text()
        };
        env.text(
            list,
            field.inset(8.0),
            &text,
            env.body(),
            env.tokens.color(ColorRole::Foreground),
            Align::Leading,
        );
        let examples = tree.bounds(self.examples).unwrap_or_default();
        let (top, bottom) = halves(examples);
        env.caption(list, top, &format!("Last change: {:?}", self.last));
        env.caption(
            list,
            bottom,
            &format!("shortcut_text examples: {}", examples_text()),
        );
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        vec![
            (self.field, self.edit.semantics("Shortcut", self.field)),
            (
                self.examples,
                sem(SemanticRole::StaticText, examples_text(), None),
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
        if self.clicks.feed(tree, event, &[self.field]).is_some() {
            self.edit.start();
            let _ = tree.set_focus(Some(self.field));
            return true;
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
        if tree.focus() != Some(self.field) {
            return false;
        }
        let Some(key) = pressed(event) else {
            return self.edit.is_recording();
        };
        if !self.edit.is_recording() {
            if matches!(key, Key::Enter | Key::Space) {
                self.edit.start();
                self.last = KeySequenceChange::Recording;
                return true;
            }
            return false;
        }
        self.last = self.edit.press(key, modifiers);
        true
    }

    fn action(
        &mut self,
        _tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        if node == self.field && action == SemanticAction::Activate {
            self.edit.start();
            return true;
        }
        false
    }
}

fn examples_text() -> String {
    examples()
}

/// Splits a rect into top and bottom halves.
pub fn halves(rect: Rect) -> (Rect, Rect) {
    let half = rect.size.height / 2.0;
    (
        Rect::new(
            rect.origin,
            choscorui::core::Size::new(rect.size.width, half),
        ),
        Rect::new(
            choscorui::core::Point::new(rect.origin.x, rect.origin.y + half),
            choscorui::core::Size::new(rect.size.width, half),
        ),
    )
}
