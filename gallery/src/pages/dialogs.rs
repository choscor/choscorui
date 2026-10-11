//! Dialogs: message dialogs with ordered buttons and modality, input
//! dialogs with validation, the color picker, and the font dialog.

use crate::page::{
    Align, Column, Env, Page, Push, field_semantics, node, paint_field, pressed, sem, type_into,
};
use crate::pages::selection::{ComboField, POPUP_SPACE, families};
use choscorui::controls::{
    AlertDialog, Button, ButtonVariant, ColorChannel, ColorPicker, ComboChange, ComboInput,
    CommandId, DialogButton, DialogButtonRole, DialogId, DialogStack, FontDialog, InputDialog,
    InputKind, Modality, ShortcutModifiers, ValidationState, VisualState, dialog_key_button,
    order_dialog_buttons,
};
use choscorui::core::{
    Color, ColorRole, DisplayList, FontSlant, FontWeight, Key, KeyEvent, NodeId, Point,
    PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRole, Size, TextStyle,
    Tree,
};

// ---------------------------------------------------------------------------
// Alert dialog

/// One open custom dialog: its root node, buttons in display order, and
/// the stack id.
struct OpenDialog {
    id: DialogId,
    dialog: AlertDialog,
    buttons: Vec<(usize, Push)>,
}

/// Opens a modal "save changes" alert or a modeless note. While the modal
/// alert is open, input outside it is blocked; Return chooses Save and
/// Escape chooses Cancel.
pub struct AlertDialogPage {
    open_modal: Push,
    open_modeless: Push,
    root: NodeId,
    status: NodeId,
    stack: DialogStack,
    current: Option<OpenDialog>,
    last: String,
}

fn save_alert() -> AlertDialog {
    let mut dialog = AlertDialog::new(
        "Unsaved changes",
        "Do you want to save the changes you made to “Notes.txt”?",
        vec![
            DialogButton::new("Save", DialogButtonRole::Accept),
            DialogButton::new("Don't Save", DialogButtonRole::Discard),
            DialogButton::new("Cancel", DialogButtonRole::Reject),
            DialogButton::new("?", DialogButtonRole::Help),
        ],
    );
    dialog.detail = Some("Your changes will be lost if you don't save them.".into());
    dialog
}

fn info_note() -> AlertDialog {
    AlertDialog::new(
        "Tip",
        "This note is modeless: the rest of the page keeps working.",
        vec![DialogButton::new("OK", DialogButtonRole::Accept)],
    )
}

pub fn alert_dialog(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let open_modal = Push::new(tree, parent, "Close document…", env);
    let open_modeless = Push::new(tree, parent, "Show tip", env);
    let status = node(tree, parent, false);
    let root = node(tree, parent, false);
    let _ = tree.set_visible(root, false);
    Box::new(AlertDialogPage {
        open_modal,
        open_modeless,
        root,
        status,
        stack: DialogStack::new(),
        current: None,
        last: "No dialog open.".into(),
    })
}

impl AlertDialogPage {
    /// Opens the modal alert (`true`) or the modeless note.
    pub fn open(&mut self, tree: &mut Tree, modal: bool, env: &Env<'_>) {
        self.close(tree);
        let dialog = if modal { save_alert() } else { info_note() };
        let (order, _) = order_dialog_buttons(&dialog.buttons, env.rtl());
        let buttons: Vec<(usize, Push)> = order
            .into_iter()
            .map(|index| {
                let button = &dialog.buttons[index];
                let mut push = Push::new(tree, self.root, button.title.clone(), env);
                push.button = push
                    .button
                    .with_variant(AlertDialog::button_variant(button.role));
                (index, push)
            })
            .collect();
        let _ = tree.set_visible(self.root, true);
        let default = dialog_key_button(&dialog.buttons, Key::Enter)
            .and_then(|index| buttons.iter().find(|(i, _)| *i == index))
            .map(|(_, push)| push.node);
        let modality = if modal {
            Modality::Modal
        } else {
            Modality::Modeless
        };
        if let Ok(id) = self.stack.open(tree, self.root, modality, default) {
            self.current = Some(OpenDialog {
                id,
                dialog,
                buttons,
            });
        }
        self.arrange(tree, env);
    }

    fn close(&mut self, tree: &mut Tree) {
        if let Some(open) = self.current.take() {
            self.stack.close(tree, open.id);
            for (_, push) in open.buttons {
                let _ = tree.detach(push.node);
            }
        }
        let _ = tree.set_visible(self.root, false);
    }

    /// Chooses button `index` (in the dialog's own order) and closes.
    fn choose(&mut self, tree: &mut Tree, index: usize) {
        if let Some(open) = &self.current {
            self.last = format!("Chose “{}”", open.dialog.buttons[index].title);
        }
        self.close(tree);
    }

    /// Presses a key in the open dialog; returns the chosen title.
    pub fn press_key(&mut self, tree: &mut Tree, key: Key) -> Option<String> {
        let index = dialog_key_button(&self.current.as_ref()?.dialog.buttons, key)?;
        let title = self.current.as_ref()?.dialog.buttons[index].title.clone();
        self.choose(tree, index);
        Some(title)
    }

    /// Whether input may reach `node` (false outside an open modal).
    pub fn accepts_input(&self, tree: &Tree, node: NodeId) -> bool {
        self.stack.accepts_input(tree, node)
    }

    fn arrange(&self, tree: &mut Tree, env: &Env<'_>) {
        let Some(open) = &self.current else {
            return;
        };
        let page = tree
            .parent(self.root)
            .and_then(|p| tree.bounds(p))
            .unwrap_or_default();
        let available = Size::new((page.size.width - 32.0).max(0.0), page.size.height);
        let size = open
            .dialog
            .measure(available, env.styles(), env.tokens, env);
        let rect = Rect::new(
            Point::new(
                page.origin.x + (page.size.width - size.width).max(0.0) / 2.0,
                page.origin.y + 90.0,
            ),
            size,
        );
        let _ = tree.set_bounds(self.root, rect);
        let rects = open
            .dialog
            .button_rects(rect, env.styles(), env.tokens, env, env.rtl());
        for (index, push) in &open.buttons {
            if let Some((_, cell)) = rects.iter().find(|(i, _)| i == index) {
                let _ = tree.set_bounds(push.node, *cell);
            }
        }
    }

    fn button_hit(
        &mut self,
        tree: &mut Tree,
        mut hit: impl FnMut(&mut Push, &mut Tree) -> bool,
    ) -> Option<usize> {
        let open = self.current.as_mut()?;
        open.buttons
            .iter_mut()
            .find_map(|(index, push)| hit(push, tree).then_some(*index))
    }
}

impl Page for AlertDialogPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(30.0, &[150.0, 110.0]);
        let _ = tree.set_bounds(self.open_modal.node, cells[0]);
        let _ = tree.set_bounds(self.open_modeless.node, cells[1]);
        let _ = tree.set_bounds(self.status, column.row(24.0));
        self.arrange(tree, env);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        self.open_modal.paint(tree, list, env);
        self.open_modeless.paint(tree, list, env);
        env.caption(
            list,
            tree.bounds(self.status).unwrap_or_default(),
            &self.last,
        );
        let Some(open) = &self.current else {
            return;
        };
        let rect = tree.bounds(self.root).unwrap_or_default();
        if self.stack.active_modal().is_some()
            && let Some(page) = tree.parent(self.root).and_then(|p| tree.bounds(p))
        {
            open.dialog.paint_overlay(&mut env.cx(list, page));
        }
        open.dialog.paint(&mut env.cx(list, rect));
        for (_, push) in &open.buttons {
            push.paint(tree, list, env);
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = vec![
            self.open_modal.semantics(tree),
            self.open_modeless.semantics(tree),
            (
                self.status,
                sem(SemanticRole::StaticText, self.last.clone(), None),
            ),
        ];
        let mut root = match &self.current {
            Some(open) => {
                let mut node = self
                    .stack
                    .semantics(open.id, &open.dialog.title)
                    .unwrap_or_else(|| open.dialog.semantics());
                node.description = open.dialog.semantics().description;
                node
            }
            None => sem(SemanticRole::Dialog, "Closed dialog", None),
        };
        root.state.hidden = self.current.is_none();
        nodes.push((self.root, root));
        if let Some(open) = &self.current {
            nodes.extend(open.buttons.iter().map(|(_, push)| push.semantics(tree)));
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
        if let Some(index) = self.button_hit(tree, |push, tree| push.pointer(tree, event)) {
            self.choose(tree, index);
            return true;
        }
        let blocked = |page: &Self, tree: &Tree, node: NodeId| !page.accepts_input(tree, node);
        if !blocked(self, tree, self.open_modal.node) && self.open_modal.pointer(tree, event) {
            self.open(tree, true, env);
            return true;
        }
        if !blocked(self, tree, self.open_modeless.node) && self.open_modeless.pointer(tree, event)
        {
            self.open(tree, false, env);
            return true;
        }
        false
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if let Some(index) = self.button_hit(tree, |push, tree| push.key(tree, event)) {
            self.choose(tree, index);
            return true;
        }
        if let Some(key) = pressed(event)
            && self.current.is_some()
        {
            if key == Key::Tab {
                let reverse = false;
                return self.stack.focus_next(tree, reverse).is_some();
            }
            if self.press_key(tree, key).is_some() {
                return true;
            }
            // A modal dialog swallows every other key.
            if self.stack.active_modal().is_some() {
                return true;
            }
        }
        if self.open_modal.key(tree, event) {
            self.open(tree, true, env);
            return true;
        }
        if self.open_modeless.key(tree, event) {
            self.open(tree, false, env);
            return true;
        }
        false
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        env: &Env<'_>,
    ) -> bool {
        if node == self.root && action == SemanticAction::Dismiss {
            let cancel = self
                .current
                .as_ref()
                .and_then(|open| dialog_key_button(&open.dialog.buttons, Key::Escape));
            match cancel {
                Some(index) => self.choose(tree, index),
                None => self.close(tree),
            }
            return true;
        }
        if let Some(index) = self.button_hit(tree, |push, tree| push.action(tree, node, action)) {
            self.choose(tree, index);
            return true;
        }
        if !self.accepts_input(tree, node) {
            return false;
        }
        if self.open_modal.action(tree, node, action) {
            self.open(tree, true, env);
            return true;
        }
        if self.open_modeless.action(tree, node, action) {
            self.open(tree, false, env);
            return true;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Input dialog

/// Four input dialogs shown inline: text, integer, decimal, and choice.
/// Each OK button is enabled only while its value is valid.
pub struct InputDialogPage {
    rows: Vec<(NodeId, NodeId, Push, InputDialog)>,
    status: NodeId,
    last: String,
}

pub fn input_dialog(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let dialogs = [
        InputDialog::new(
            "Rename",
            "New name",
            InputKind::Text { allow_empty: false },
            env.locale.clone(),
            "Notes.txt",
        ),
        InputDialog::new(
            "Go to line",
            "Line (1 – 500)",
            InputKind::Integer {
                minimum: 1,
                maximum: 500,
            },
            env.locale.clone(),
            "42",
        ),
        InputDialog::new(
            "Price",
            "Price (0 – 1,000,000; 2 decimals)",
            InputKind::Decimal {
                minimum: 0.0,
                maximum: 1e6,
                fraction_digits: 2,
            },
            env.locale.clone(),
            env.locale
                .format_decimal(9.5, 2)
                .unwrap_or_else(|_| "9.50".into()),
        ),
        InputDialog::new(
            "Size",
            "Size (Small, Medium, or Large)",
            InputKind::Choice(vec!["Small".into(), "Medium".into(), "Large".into()]),
            env.locale.clone(),
            "Medium",
        ),
    ];
    let rows = dialogs
        .into_iter()
        .map(|dialog| {
            let caption = node(tree, parent, false);
            let field = node(tree, parent, true);
            let ok = Push::new(tree, parent, "OK", env);
            (caption, field, ok, dialog)
        })
        .collect();
    let status = node(tree, parent, false);
    Box::new(InputDialogPage {
        rows,
        status,
        last: "Type a value and press Return.".into(),
    })
}

impl InputDialogPage {
    fn accept(&mut self, index: usize) -> bool {
        let dialog = &self.rows[index].3;
        match dialog.handle_key(Key::Enter) {
            Some(Ok(value)) => {
                self.last = format!("{}: accepted {value:?}", dialog.title);
                true
            }
            _ => {
                self.last = format!("{}: fix the value first", dialog.title);
                false
            }
        }
    }

    fn focused(&self, tree: &Tree) -> Option<usize> {
        self.rows
            .iter()
            .position(|(_, field, _, _)| tree.focus() == Some(*field))
    }
}

impl Page for InputDialogPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (caption, field, ok, _) in &self.rows {
            let _ = tree.set_bounds(*caption, column.row(20.0));
            let cells = column.cells(32.0, &[260.0, 70.0]);
            let _ = tree.set_bounds(*field, cells[0]);
            let _ = tree.set_bounds(ok.node, cells[1]);
            column.space(14.0);
        }
        let _ = tree.set_bounds(self.status, column.row(24.0));
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (caption, field, ok, dialog) in &self.rows {
            env.caption(
                list,
                tree.bounds(*caption).unwrap_or_default(),
                &format!("{} — {}", dialog.title, dialog.prompt),
            );
            let rect = tree.bounds(*field).unwrap_or_default();
            let message = Rect::new(
                Point::new(rect.origin.x, rect.max_y() + 1.0),
                Size::new(rect.size.width + 200.0, 14.0),
            );
            paint_field(
                env,
                list,
                rect,
                dialog.text(),
                "",
                tree.focus() == Some(*field),
                &dialog.validate().0,
                Some(message),
            );
            ok.paint(tree, list, env);
            if !dialog.can_accept() {
                // Dim OK while the value is invalid.
                list.fill_rect(
                    tree.bounds(ok.node).unwrap_or_default(),
                    env.tokens.color(ColorRole::Background).with_alpha(0.55),
                );
            }
        }
        env.caption(
            list,
            tree.bounds(self.status).unwrap_or_default(),
            &self.last,
        );
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = Vec::new();
        for (_, field, ok, dialog) in &self.rows {
            nodes.push((
                *field,
                field_semantics(&dialog.prompt, dialog.text(), &dialog.validate().0),
            ));
            let (id, mut node) = ok.semantics(tree);
            node.name = format!("OK ({})", dialog.title);
            node.state.disabled = node.state.disabled || !dialog.can_accept();
            nodes.push((id, node));
        }
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
        if let Some(index) = self
            .rows
            .iter_mut()
            .position(|(_, _, ok, _)| ok.pointer(tree, event))
        {
            return self.accept(index);
        }
        if let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = event
            && let Some((_, field, _, _)) = self
                .rows
                .iter()
                .find(|(_, field, _, _)| tree.bounds(*field).is_some_and(|b| b.contains(position)))
        {
            return tree.set_focus(Some(*field)).is_ok();
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
        if let Some(index) = self
            .rows
            .iter_mut()
            .position(|(_, _, ok, _)| ok.key(tree, event))
        {
            return self.accept(index);
        }
        let Some(index) = self.focused(tree) else {
            return false;
        };
        match pressed(event) {
            Some(Key::Enter) => {
                self.accept(index);
                true
            }
            Some(Key::Escape) => {
                self.last = format!("{}: cancelled", self.rows[index].3.title);
                true
            }
            Some(Key::Backspace | Key::Delete) => {
                let mut text = self.rows[index].3.text().to_owned();
                text.pop();
                self.rows[index].3.set_text(text);
                true
            }
            _ => false,
        }
    }

    fn text(&mut self, tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        let Some(index) = self.focused(tree) else {
            return false;
        };
        let mut value = self.rows[index].3.text().to_owned();
        let changed = type_into(&mut value, text);
        self.rows[index].3.set_text(value);
        changed
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        if let Some(index) = self
            .rows
            .iter_mut()
            .position(|(_, _, ok, _)| ok.action(tree, node, action))
        {
            return self.accept(index);
        }
        match self.rows.iter_mut().find(|(_, field, _, _)| *field == node) {
            Some((_, _, _, dialog)) if action == SemanticAction::SetValue => {
                dialog.set_text(value.unwrap_or_default());
                true
            }
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Color picker

const CHANNELS: [&str; 4] = ["Hue", "Saturation", "Brightness", "Opacity"];

/// Hue, saturation, brightness, and opacity bars with a hex field and the
/// recent colors chosen with Use Color.
pub struct ColorPickerPage {
    bars: Vec<NodeId>,
    hex: NodeId,
    hex_text: String,
    hex_state: ValidationState,
    swatch: NodeId,
    accept: Push,
    picker: ColorPicker,
    dragging: Option<usize>,
}

pub fn color_picker(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let bars = CHANNELS.iter().map(|_| node(tree, parent, true)).collect();
    let hex = node(tree, parent, true);
    let swatch = node(tree, parent, false);
    let accept = Push::new(tree, parent, "Use Color", env);
    let picker = ColorPicker::new(Color::rgba8(0, 128, 128, 255));
    Box::new(ColorPickerPage {
        bars,
        hex,
        hex_text: picker.hex(),
        hex_state: ValidationState::Valid,
        swatch,
        accept,
        picker,
        dragging: None,
    })
}

impl ColorPickerPage {
    /// Channel `index` as a fraction of its range.
    fn fraction(&self, index: usize) -> f64 {
        self.picker.channel(ColorChannel::ALL[index])
    }

    fn set_fraction(&mut self, index: usize, fraction: f64) {
        self.picker.set_channel(ColorChannel::ALL[index], fraction);
        self.hex_text = self.picker.hex();
        self.hex_state = ValidationState::Valid;
    }

    /// Applies hex text, for scripted checks.
    pub fn set_hex(&mut self, text: &str) -> ValidationState {
        self.hex_text = text.to_owned();
        self.hex_state = self.picker.set_hex(text);
        self.hex_state.clone()
    }

    /// The fraction at `x` on bar `index`; bars fill from the end in
    /// right-to-left layouts.
    fn bar_fraction(&self, tree: &Tree, index: usize, x: f64, rtl: bool) -> f64 {
        let rect = tree.bounds(self.bars[index]).unwrap_or_default();
        let fraction = (x - rect.origin.x) / rect.size.width.max(1.0);
        if rtl { 1.0 - fraction } else { fraction }
    }
}

impl Page for ColorPickerPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for id in &self.bars {
            column.space(16.0);
            let _ = tree.set_bounds(*id, column.cells(18.0, &[300.0])[0]);
        }
        let cells = column.cells(32.0, &[140.0, 60.0, 110.0]);
        let _ = tree.set_bounds(self.hex, cells[0]);
        let _ = tree.set_bounds(self.swatch, cells[1]);
        let _ = tree.set_bounds(self.accept.node, cells[2]);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (index, id) in self.bars.iter().enumerate() {
            let rect = tree.bounds(*id).unwrap_or_default();
            env.caption(
                list,
                Rect::new(
                    Point::new(rect.origin.x, rect.origin.y - 16.0),
                    Size::new(rect.size.width, 14.0),
                ),
                &format!(
                    "{} {:.0}{}",
                    CHANNELS[index],
                    if index == 0 {
                        self.fraction(0) * 360.0
                    } else {
                        self.fraction(index) * 100.0
                    },
                    if index == 0 { "°" } else { "%" }
                ),
            );
            let state = VisualState::NONE
                .set(VisualState::FOCUSED, tree.focus() == Some(*id))
                .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(*id));
            self.picker
                .paint_channel(&mut env.cx(list, rect), ColorChannel::ALL[index], state);
        }
        let hex = tree.bounds(self.hex).unwrap_or_default();
        paint_field(
            env,
            list,
            hex,
            &self.hex_text,
            "#RRGGBB",
            tree.focus() == Some(self.hex),
            &self.hex_state,
            None,
        );
        let swatch = tree.bounds(self.swatch).unwrap_or_default();
        self.picker.paint_swatch(&mut env.cx(list, swatch));
        self.accept.paint(tree, list, env);
        let recent_row = Rect::new(
            Point::new(hex.origin.x, hex.max_y() + 12.0),
            Size::new(320.0, 20.0),
        );
        for (index, color) in self.picker.recent().iter().enumerate() {
            let chip = Rect::new(
                Point::new(
                    recent_row.origin.x + index as f64 * 24.0,
                    recent_row.origin.y,
                ),
                Size::new(20.0, 20.0),
            );
            ColorPicker::new(*color).paint_swatch(&mut env.cx(list, chip));
        }
        if let ValidationState::Invalid(message) = &self.hex_state {
            env.text(
                list,
                Rect::new(
                    Point::new(hex.origin.x, hex.max_y() + 36.0),
                    Size::new(320.0, 16.0),
                ),
                message,
                env.small(),
                env.tokens.color(ColorRole::Destructive),
                Align::Leading,
            );
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = Vec::new();
        for (index, id) in self.bars.iter().enumerate() {
            let value = if index == 0 {
                format!("{:.0} degrees", self.fraction(0) * 360.0)
            } else {
                format!("{:.0}%", self.fraction(index) * 100.0)
            };
            let mut bar = sem(SemanticRole::Slider, CHANNELS[index], Some(value));
            bar.actions = vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus,
            ];
            nodes.push((*id, bar));
        }
        nodes.push((
            self.hex,
            field_semantics("Hex color", &self.hex_text, &self.hex_state),
        ));
        nodes.push((self.swatch, self.picker.semantics("Current color")));
        nodes.push(self.accept.semantics(tree));
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if self.accept.pointer(tree, event) {
            self.picker.accept();
            return true;
        }
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => {
                if tree.bounds(self.hex).is_some_and(|b| b.contains(position)) {
                    return tree.set_focus(Some(self.hex)).is_ok();
                }
                let Some(index) = self.bars.iter().position(|id| {
                    tree.bounds(*id)
                        .is_some_and(|b| b.inset(-4.0).contains(position))
                }) else {
                    return false;
                };
                let _ = tree.set_focus(Some(self.bars[index]));
                let _ = tree.set_pointer_capture(Some(self.bars[index]));
                self.dragging = Some(index);
                let fraction = self.bar_fraction(tree, index, position.x, env.rtl());
                self.set_fraction(index, fraction);
                true
            }
            PointerEvent::Move { position } => match self.dragging {
                Some(index) => {
                    let fraction = self.bar_fraction(tree, index, position.x, env.rtl());
                    self.set_fraction(index, fraction);
                    true
                }
                None => false,
            },
            PointerEvent::Up { .. } => {
                if self.dragging.take().is_some() {
                    let _ = tree.set_pointer_capture(None);
                }
                false
            }
            _ => false,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if self.accept.key(tree, event) {
            self.picker.accept();
            return true;
        }
        let Some(key) = pressed(event) else {
            return false;
        };
        if tree.focus() == Some(self.hex) {
            return match key {
                Key::Backspace | Key::Delete => {
                    let mut text = self.hex_text.clone();
                    text.pop();
                    self.set_hex(&text);
                    true
                }
                _ => false,
            };
        }
        let Some(index) = self.bars.iter().position(|id| tree.focus() == Some(*id)) else {
            return false;
        };
        let forward = if env.rtl() { -1.0 } else { 1.0 };
        let delta = match key {
            Key::ArrowUp => 0.02,
            Key::ArrowRight => 0.02 * forward,
            Key::ArrowLeft => -0.02 * forward,
            Key::ArrowDown => -0.02,
            Key::PageUp => 0.1,
            Key::PageDown => -0.1,
            _ => return false,
        };
        let fraction = self.fraction(index) + delta;
        self.set_fraction(index, fraction);
        true
    }

    fn text(&mut self, tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        if tree.focus() != Some(self.hex) {
            return false;
        }
        let mut value = self.hex_text.clone();
        let changed = type_into(&mut value, text);
        self.set_hex(&value);
        changed
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        if self.accept.action(tree, node, action) {
            self.picker.accept();
            return true;
        }
        if node == self.hex && action == SemanticAction::SetValue {
            self.set_hex(value.unwrap_or_default());
            return true;
        }
        let Some(index) = self.bars.iter().position(|id| *id == node) else {
            return false;
        };
        let delta = match action {
            SemanticAction::Increment => 0.05,
            SemanticAction::Decrement => -0.05,
            SemanticAction::Focus => return tree.set_focus(Some(node)).is_ok(),
            _ => return false,
        };
        let fraction = self.fraction(index) + delta;
        self.set_fraction(index, fraction);
        true
    }
}

// ---------------------------------------------------------------------------
// Font dialog

const PREVIEW: &str = "Sphinx of black quartz — Chữ Việt — 日本語";

/// Family (with completion), size, weight, and slant, with a live preview.
pub struct FontDialogPage {
    family: ComboField,
    size_node: NodeId,
    size_text: String,
    size_state: ValidationState,
    bold: Push,
    italic: Push,
    preview: NodeId,
    dialog: FontDialog,
}

pub fn font_dialog(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let family = ComboField::new(tree, parent);
    let size_node = node(tree, parent, true);
    let bold = Push::new(tree, parent, "Bold", env);
    let italic = Push::new(tree, parent, "Italic", env);
    let preview = node(tree, parent, false);
    let dialog = FontDialog::new(families(), &TextStyle::new("Georgia", 22.0));
    Box::new(FontDialogPage {
        family,
        size_node,
        size_text: "22".into(),
        size_state: ValidationState::Valid,
        bold,
        italic,
        preview,
        dialog,
    })
}

impl FontDialogPage {
    /// Sets the size text, for scripted checks.
    pub fn set_size(&mut self, text: &str) -> ValidationState {
        self.size_text = text.to_owned();
        self.size_state = match text.trim().parse::<f64>() {
            Ok(size) => self.dialog.set_size(size),
            Err(_) => ValidationState::Invalid("Enter a size in points".into()),
        };
        self.size_state.clone()
    }

    fn toggle_bold(&mut self) {
        let weight = if self.dialog.style().weight >= FontWeight::BOLD {
            FontWeight::REGULAR
        } else {
            FontWeight::BOLD
        };
        self.dialog.set_weight(weight);
    }

    fn toggle_italic(&mut self) {
        let slant = match self.dialog.style().slant {
            FontSlant::Italic => FontSlant::Upright,
            FontSlant::Upright => FontSlant::Italic,
        };
        self.dialog.set_slant(slant);
    }

    fn family_input(&mut self, input: Option<ComboInput>) -> bool {
        match input {
            Some(input) => {
                // Opening or moving through suggestions changes nothing
                // in the model but still needs a repaint.
                let _: ComboChange = self.dialog.family(input);
                true
            }
            None => false,
        }
    }
}

impl Page for FontDialogPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(30.0, &[240.0, 70.0, 70.0, 70.0]);
        let _ = tree.set_bounds(self.family.node, cells[0]);
        let _ = tree.set_bounds(self.size_node, cells[1]);
        let _ = tree.set_bounds(self.bold.node, cells[2]);
        let _ = tree.set_bounds(self.italic.node, cells[3]);
        column.space(POPUP_SPACE);
        let _ = tree.set_bounds(self.preview, column.cells(90.0, &[560.0])[0]);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        self.family
            .paint(tree, list, env, self.dialog.chooser().combo());
        paint_field(
            env,
            list,
            tree.bounds(self.size_node).unwrap_or_default(),
            &self.size_text,
            "pt",
            tree.focus() == Some(self.size_node),
            &self.size_state,
            None,
        );
        let style = self.dialog.style();
        // A style that is on shows as a secondary (filled) button.
        for (push, on) in [
            (&self.bold, style.weight >= FontWeight::BOLD),
            (&self.italic, style.slant == FontSlant::Italic),
        ] {
            let rect = tree.bounds(push.node).unwrap_or_default();
            let variant = if on {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Outline
            };
            Button::new(push.label.clone(), CommandId(0))
                .with_variant(variant)
                .paint(&mut env.cx(list, rect), tree, push.node);
        }
        let preview = tree.bounds(self.preview).unwrap_or_default();
        env.panel(list, preview);
        list.push_clip(preview);
        match env
            .shaper
            .shape(PREVIEW, &style, Some(preview.size.width - 24.0))
        {
            Ok(layout) => {
                let _ = layout.draw(
                    list,
                    Point::new(preview.origin.x + 12.0, preview.origin.y + 12.0),
                    env.tokens.color(ColorRole::Foreground),
                );
            }
            Err(_) => env.label(list, preview, PREVIEW),
        }
        let _ = list.pop_clip();
        let message = match &self.size_state {
            ValidationState::Invalid(message) => message.clone(),
            _ => format!("{} {:.0} pt", style.family, style.size),
        };
        env.caption(
            list,
            Rect::new(
                Point::new(preview.origin.x, preview.max_y() + 6.0),
                Size::new(preview.size.width, 18.0),
            ),
            &message,
        );
        self.family
            .paint_popup(tree, list, env, self.dialog.chooser().combo());
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let style = self.dialog.style();
        let (bold_id, mut bold) = self.bold.semantics(tree);
        bold.state.checked = Some(style.weight >= FontWeight::BOLD);
        let (italic_id, mut italic) = self.italic.semantics(tree);
        italic.state.checked = Some(style.slant == FontSlant::Italic);
        vec![
            (self.family.node, self.dialog.chooser().semantics()),
            (
                self.size_node,
                field_semantics("Size", &self.size_text, &self.size_state),
            ),
            (bold_id, bold),
            (italic_id, italic),
            (
                self.preview,
                sem(
                    SemanticRole::StaticText,
                    format!("Preview in {} {:.0} point", style.family, style.size),
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
        if self.bold.pointer(tree, event) {
            self.toggle_bold();
            return true;
        }
        if self.italic.pointer(tree, event) {
            self.toggle_italic();
            return true;
        }
        if let PointerEvent::Down { position, .. } = event
            && tree
                .bounds(self.size_node)
                .is_some_and(|b| b.contains(position))
        {
            return tree.set_focus(Some(self.size_node)).is_ok();
        }
        let input = self
            .family
            .pointer(tree, event, self.dialog.chooser().combo());
        self.family_input(input)
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if self.bold.key(tree, event) {
            self.toggle_bold();
            return true;
        }
        if self.italic.key(tree, event) {
            self.toggle_italic();
            return true;
        }
        if tree.focus() == Some(self.size_node) {
            return match pressed(event) {
                Some(Key::Backspace | Key::Delete) => {
                    let mut text = self.size_text.clone();
                    text.pop();
                    self.set_size(&text);
                    true
                }
                Some(Key::ArrowUp | Key::ArrowDown) => {
                    let step = if pressed(event) == Some(Key::ArrowUp) {
                        1.0
                    } else {
                        -1.0
                    };
                    let size = self.dialog.style().size + step;
                    self.set_size(&format!("{size:.0}"));
                    true
                }
                _ => false,
            };
        }
        let input = self
            .family
            .key(tree, event, modifiers, self.dialog.chooser().combo());
        self.family_input(input)
    }

    fn text(&mut self, tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        if tree.focus() == Some(self.size_node) {
            let mut value = self.size_text.clone();
            let changed = type_into(&mut value, text);
            self.set_size(&value);
            return changed;
        }
        let input = self.family.text(tree, text);
        self.family_input(input)
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        if self.bold.action(tree, node, action) {
            self.toggle_bold();
            return true;
        }
        if self.italic.action(tree, node, action) {
            self.toggle_italic();
            return true;
        }
        if node == self.size_node && action == SemanticAction::SetValue {
            self.set_size(value.unwrap_or_default());
            return true;
        }
        node == self.family.node
            && self.family_input(Some(ComboInput::Semantic(action, value.map(str::to_owned))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::tests::Harness;

    #[test]
    fn alert_dialog_opens_modally_and_escape_cancels_it() {
        let mut h = Harness::new("alert-dialog");
        let open = h.node("Close document…", None);
        h.act(open, SemanticAction::Activate);
        let Harness { tree, page, .. } = &mut h;
        let page = page.as_any_mut().downcast_mut::<AlertDialogPage>().unwrap();
        assert!(page.current.is_some());
        assert!(page.press_key(tree, Key::Escape).is_some());
        assert!(h.get::<AlertDialogPage>().current.is_none());
    }

    #[test]
    fn input_dialogs_validate_their_kind() {
        let mut h = Harness::new("input-dialog");
        let page = h.get::<InputDialogPage>();
        let mut enter = |index: usize, text: &str| {
            page.rows[index].3.set_text(text);
            page.rows[index].3.validate().0
        };
        assert_ne!(enter(0, ""), ValidationState::Valid, "a name is required");
        assert_eq!(enter(1, "42"), ValidationState::Valid);
        assert_ne!(enter(1, "501"), ValidationState::Valid);
        assert_ne!(enter(1, "forty"), ValidationState::Valid);
    }
}
