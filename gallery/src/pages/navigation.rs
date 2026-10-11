//! Keyboard navigation: mnemonics and default/cancel dialog keys.

use crate::page::{
    Align, Column, Env, Page, Push, field_semantics, node, paint_field, pressed, sem, type_into,
};
use choscorui::controls::{
    CommandId, CommandRegistry, CommandState, DialogKeys, MnemonicTable, ShortcutModifiers,
    ValidationState, parse_mnemonic,
};
use choscorui::core::{
    ColorRole, DisplayList, Key, KeyEvent, NodeId, Point, PointerEvent, Rect, SemanticAction,
    SemanticNode, SemanticRole, Size, Tree,
};

/// Labels with mnemonics; `&P` is shared, so pressing it cycles focus.
const MNEMONIC_LABELS: [&str; 5] = ["&Save", "Save &As…", "&Print", "&Preview", "Fish && Chips"];

/// Buttons reachable with Control-Option and their underlined letter.
pub struct MnemonicsPage {
    pushes: Vec<(Push, Option<char>)>,
    table: MnemonicTable,
    status: NodeId,
    last: String,
}

pub fn mnemonics(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let mut table = MnemonicTable::new();
    let pushes = MNEMONIC_LABELS
        .iter()
        .map(|label| {
            let (display, mnemonic) = parse_mnemonic(label);
            let mut push = Push::new(tree, parent, display, env);
            push.label = table.register(label, push.node);
            (push, mnemonic)
        })
        .collect();
    let status = node(tree, parent, false);
    Box::new(MnemonicsPage {
        pushes,
        table,
        status,
        last: "Press Control-Option and a letter (S, A, P, or F).".into(),
    })
}

impl MnemonicsPage {
    /// Handles Control-Option-`letter`; returns the label it activated or
    /// focused.
    pub fn press(&mut self, tree: &mut Tree, letter: char) -> Option<String> {
        let hit = self.table.resolve(letter, tree.focus(), |id| {
            tree.contains(id) && tree.visible(id) != Some(false)
        })?;
        let _ = tree.set_focus(Some(hit.node));
        let label = self
            .pushes
            .iter()
            .find(|(push, _)| push.node == hit.node)
            .map(|(push, _)| push.label.clone())
            .unwrap_or_default();
        self.last = if hit.activate {
            format!("Activated “{label}”")
        } else {
            format!("Focused “{label}”; press again for the next match")
        };
        Some(label)
    }
}

impl Page for MnemonicsPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(30.0, &[110.0, 110.0, 110.0, 110.0, 130.0]);
        for ((push, _), cell) in self.pushes.iter().zip(cells) {
            let _ = tree.set_bounds(push.node, cell);
        }
        let _ = tree.set_bounds(self.status, column.row(24.0));
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (push, mnemonic) in &self.pushes {
            push.paint(tree, list, env);
            // Underline the mnemonic letter.
            let Some(letter) = mnemonic else { continue };
            let Some(at) = push.label.to_lowercase().find(*letter) else {
                continue;
            };
            let bounds = tree.bounds(push.node).unwrap_or_default();
            let size = env.body();
            let total = env.measure(&push.label, size);
            let before = env.measure(&push.label[..at], size);
            let width = push.label[at..]
                .chars()
                .next()
                .map_or(0.0, |c| env.measure(&c.to_string(), size));
            let start = bounds.origin.x + (bounds.size.width - total) / 2.0;
            list.fill_rect(
                Rect::new(
                    Point::new(
                        start + before,
                        bounds.origin.y + bounds.size.height / 2.0 + size * 0.5,
                    ),
                    Size::new(width, 1.0),
                ),
                env.tokens.color(ColorRole::Foreground),
            );
        }
        env.caption(
            list,
            tree.bounds(self.status).unwrap_or_default(),
            &self.last,
        );
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes: Vec<_> = self
            .pushes
            .iter()
            .map(|(push, mnemonic)| {
                let (id, mut node) = push.semantics(tree);
                if let Some(letter) = mnemonic {
                    node.description = format!("Control-Option-{}", letter.to_uppercase());
                }
                (id, node)
            })
            .collect();
        nodes.push((
            self.status,
            sem(SemanticRole::StaticText, self.last.clone(), None),
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
        for (push, _) in &mut self.pushes {
            if push.pointer(tree, event) {
                self.last = format!("Clicked “{}”", push.label);
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
        if modifiers.control
            && modifiers.option
            && let Some(Key::Character(letter)) = pressed(event)
        {
            return self.press(tree, letter).is_some();
        }
        for (push, _) in &mut self.pushes {
            if push.key(tree, event) {
                self.last = format!("Activated “{}”", push.label);
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
        for (push, _) in &mut self.pushes {
            if push.action(tree, node, action) {
                self.last = format!("Activated “{}”", push.label);
                return true;
            }
        }
        false
    }
}

const OK: CommandId = CommandId(1);
const CANCEL: CommandId = CommandId(2);

/// A small form where Return runs the default button and Escape (or
/// ⌘-period) cancels, except in the multi-line note, which uses Return.
pub struct DialogKeysPage {
    name: (NodeId, NodeId, String),
    note: (NodeId, NodeId, String),
    ok: Push,
    cancel: Push,
    toggle: Push,
    status: NodeId,
    keys: DialogKeys,
    commands: CommandRegistry,
    last: String,
}

pub fn dialog_keys(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let mut commands = CommandRegistry::new();
    for command in [OK, CANCEL] {
        let _ = commands.insert(command, CommandState::default());
    }
    let name = (
        node(tree, parent, false),
        node(tree, parent, true),
        "Lựu".to_owned(),
    );
    let note = (
        node(tree, parent, false),
        node(tree, parent, true),
        String::new(),
    );
    let ok = Push::new(tree, parent, "OK", env);
    let cancel = Push::new(tree, parent, "Cancel", env);
    let toggle = Push::new(tree, parent, "Disable OK", env);
    let status = node(tree, parent, false);
    Box::new(DialogKeysPage {
        name,
        note,
        ok,
        cancel,
        toggle,
        status,
        keys: DialogKeys {
            default: Some(OK),
            cancel: Some(CANCEL),
        },
        commands,
        last: "Press Return or Escape.".into(),
    })
}

impl DialogKeysPage {
    /// Routes a key as the dialog would; returns the command it ran.
    pub fn route(
        &mut self,
        tree: &Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
    ) -> Option<CommandId> {
        let in_note = tree.focus() == Some(self.note.1);
        let command = self.keys.route(event, modifiers, in_note, &self.commands)?;
        self.run(command);
        Some(command)
    }

    fn run(&mut self, command: CommandId) {
        self.last = if command == OK {
            format!("OK: saved “{}”", self.name.2)
        } else {
            "Cancelled".into()
        };
    }

    fn toggle_ok(&mut self) {
        let enabled = !self.commands.state(OK).is_ok_and(|state| state.enabled);
        let _ = self.commands.set_enabled(OK, enabled);
        self.toggle.label = if enabled { "Disable OK" } else { "Enable OK" }.into();
        self.last = if enabled {
            "OK enabled".into()
        } else {
            "OK disabled: Return is consumed but does nothing".into()
        };
    }

    fn ok_enabled(&self) -> bool {
        self.commands.state(OK).is_ok_and(|state| state.enabled)
    }

    fn focused_text(&mut self, tree: &Tree) -> Option<&mut String> {
        if tree.focus() == Some(self.name.1) {
            Some(&mut self.name.2)
        } else if tree.focus() == Some(self.note.1) {
            Some(&mut self.note.2)
        } else {
            None
        }
    }
}

impl Page for DialogKeysPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let _ = tree.set_bounds(self.name.0, column.row(20.0));
        let _ = tree.set_bounds(self.name.1, column.cells(32.0, &[320.0])[0]);
        let _ = tree.set_bounds(self.note.0, column.row(20.0));
        let _ = tree.set_bounds(self.note.1, column.cells(80.0, &[320.0])[0]);
        let cells = column.cells(30.0, &[100.0, 100.0, 120.0]);
        let _ = tree.set_bounds(self.cancel.node, cells[0]);
        let _ = tree.set_bounds(self.ok.node, cells[1]);
        let _ = tree.set_bounds(self.toggle.node, cells[2]);
        let _ = tree.set_bounds(self.status, column.row(24.0));
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        env.caption(list, tree.bounds(self.name.0).unwrap_or_default(), "Name");
        paint_field(
            env,
            list,
            tree.bounds(self.name.1).unwrap_or_default(),
            &self.name.2,
            "",
            tree.focus() == Some(self.name.1),
            &ValidationState::Valid,
            None,
        );
        env.caption(
            list,
            tree.bounds(self.note.0).unwrap_or_default(),
            "Note (Return inserts a line here)",
        );
        let note = tree.bounds(self.note.1).unwrap_or_default();
        env.field_style().paint(
            list,
            note,
            choscorui::controls::TextFieldVisualState {
                enabled: env.enabled,
                focused: tree.focus() == Some(self.note.1),
                validation: ValidationState::Valid.kind(),
            },
        );
        for (index, line) in self.note.2.split('\n').enumerate().take(4) {
            let rect = Rect::new(
                Point::new(
                    note.origin.x + 8.0,
                    note.origin.y + 4.0 + index as f64 * 18.0,
                ),
                Size::new(note.size.width - 16.0, 18.0),
            );
            env.text(
                list,
                rect,
                line,
                env.body(),
                env.tokens.color(ColorRole::Foreground),
                Align::Leading,
            );
        }
        self.cancel.paint(tree, list, env);
        self.ok.paint(tree, list, env);
        if self.ok_enabled() {
            // The default button is outlined in the accent color.
            let rect = tree.bounds(self.ok.node).unwrap_or_default();
            let _ = list.stroke_rounded_rect(
                rect,
                env.tokens.radius.min(8.0),
                env.tokens.color(ColorRole::Primary),
                2.0,
            );
        }
        self.toggle.paint(tree, list, env);
        env.caption(
            list,
            tree.bounds(self.status).unwrap_or_default(),
            &self.last,
        );
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let (ok_id, mut ok) = self.ok.semantics(tree);
        ok.description = "Default button (Return)".into();
        ok.state.disabled = !self.ok_enabled();
        let (cancel_id, mut cancel) = self.cancel.semantics(tree);
        cancel.description = "Cancel button (Escape)".into();
        vec![
            (
                self.name.1,
                field_semantics("Name", &self.name.2, &ValidationState::Valid),
            ),
            (
                self.note.1,
                field_semantics("Note", &self.note.2, &ValidationState::Valid),
            ),
            (ok_id, ok),
            (cancel_id, cancel),
            self.toggle.semantics(tree),
            (
                self.status,
                sem(SemanticRole::StaticText, self.last.clone(), None),
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
        if self.ok.pointer(tree, event) {
            if self.ok_enabled() {
                self.run(OK);
            }
            return true;
        }
        if self.cancel.pointer(tree, event) {
            self.run(CANCEL);
            return true;
        }
        if self.toggle.pointer(tree, event) {
            self.toggle_ok();
            return true;
        }
        if let PointerEvent::Down { position, .. } = event {
            for id in [self.name.1, self.note.1] {
                if tree.bounds(id).is_some_and(|b| b.contains(position)) {
                    return tree.set_focus(Some(id)).is_ok();
                }
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
        if self.ok.key(tree, event) {
            if self.ok_enabled() {
                self.run(OK);
            }
            return true;
        }
        if self.cancel.key(tree, event) {
            self.run(CANCEL);
            return true;
        }
        if self.toggle.key(tree, event) {
            self.toggle_ok();
            return true;
        }
        let in_note = tree.focus() == Some(self.note.1);
        match pressed(event) {
            Some(Key::Enter) if in_note => {
                self.note.2.push('\n');
                return true;
            }
            Some(Key::Backspace | Key::Delete) => {
                if let Some(text) = self.focused_text(tree) {
                    text.pop();
                    return true;
                }
            }
            _ => {}
        }
        if matches!(
            pressed(event),
            Some(Key::Enter | Key::Escape | Key::Character('.'))
        ) {
            let before = self.last.clone();
            let routed = self.route(tree, event, modifiers).is_some();
            if !routed && pressed(event) == Some(Key::Enter) && !self.ok_enabled() {
                self.last = "Return ignored: OK is disabled".into();
            }
            return routed || self.last != before;
        }
        false
    }

    fn text(&mut self, tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        match self.focused_text(tree) {
            Some(value) => type_into(value, text),
            None => false,
        }
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        if self.ok.action(tree, node, action) {
            if self.ok_enabled() {
                self.run(OK);
            }
            return true;
        }
        if self.cancel.action(tree, node, action) {
            self.run(CANCEL);
            return true;
        }
        if self.toggle.action(tree, node, action) {
            self.toggle_ok();
            return true;
        }
        if action == SemanticAction::SetValue {
            let text = value.unwrap_or_default().to_owned();
            if node == self.name.1 {
                self.name.2 = text;
                return true;
            }
            if node == self.note.1 {
                self.note.2 = text;
                return true;
            }
        }
        false
    }
}
