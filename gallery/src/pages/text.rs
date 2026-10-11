//! Text entry: single-line fields, the multi-line editor, and structured
//! inputs (numbers, masks, dates) with validation feedback.

use crate::page::{
    Align, Column, Env, FONT, Page, field_semantics, node, paint_field, pressed, type_into,
};
use choscorui::controls::{
    DateInput, InputMask, NumberInput, ShortcutModifiers, TextEditor, TextEditorInput, TextField,
    TextFieldChange, TextFieldInput, ValidationState,
};
use choscorui::core::{
    CivilDate, DisplayList, Key, KeyEvent, NodeId, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, Size, TextLayout, TextShaper, TextStyle, Tree,
    grapheme_boundaries,
};

/// Plain, password, required, and disabled text fields.
pub struct TextFieldPage {
    fields: Vec<(NodeId, NodeId, TextField, &'static str, bool)>,
}

pub fn text_field(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let mut fields = Vec::new();
    for (label, value, password, required, enabled) in [
        ("Name", "Nguyễn Thị Lựu", false, false, true),
        ("Password", "hunter2", true, false, true),
        ("Email (required; Return validates)", "", false, true, true),
        ("Disabled", "Read only", false, false, false),
    ] {
        let caption = node(tree, parent, false);
        let id = node(tree, parent, true);
        let mut field = if password {
            TextField::password(label, value)
        } else {
            TextField::new(label, value)
        };
        let _ = field.set_enabled(tree, id, enabled && env.enabled);
        fields.push((caption, id, field, label, required));
    }
    Box::new(TextFieldPage { fields })
}

/// Grapheme index nearest to `x` (relative to the text start) in `text`.
fn caret_at(env: &Env<'_>, text: &str, x: f64) -> usize {
    let boundaries = grapheme_boundaries(text);
    let mut best = 0;
    let mut best_distance = f64::INFINITY;
    for (index, end) in boundaries.iter().enumerate() {
        let width = env.measure(&text[..*end], env.body());
        let distance = (width - x).abs();
        if distance < best_distance {
            best = index;
            best_distance = distance;
        }
    }
    best
}

fn validate_required(value: &str) -> ValidationState {
    if value.is_empty() {
        ValidationState::Invalid("An email address is required".into())
    } else if !value.contains('@') {
        ValidationState::Invalid("Enter an address like name@example.com".into())
    } else {
        ValidationState::Valid
    }
}

impl Page for TextFieldPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (caption, id, _, _, _) in &self.fields {
            let _ = tree.set_bounds(*caption, column.row(20.0));
            let _ = tree.set_bounds(*id, column.cells(32.0, &[340.0])[0]);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (caption, id, field, label, _) in &self.fields {
            let semantics = field.semantics(tree, *id);
            let note = match &semantics.announcement {
                Some(message) => format!("{label} — {message}"),
                None => (*label).to_owned(),
            };
            env.caption(list, tree.bounds(*caption).unwrap_or_default(), &note);
            let bounds = tree.bounds(*id).unwrap_or_default();
            let state = field.visual_state(tree, *id);
            env.field_style().paint(list, bounds, state);
            let display = field.display_value();
            let inner = bounds.inset(8.0);
            list.push_clip(bounds);
            let color = if state.enabled {
                env.tokens.content
            } else {
                env.tokens.content.with_alpha(0.45)
            };
            env.text(list, inner, &display, env.body(), color, Align::Leading);
            if state.focused {
                let boundaries = grapheme_boundaries(&display);
                let caret = field.caret().min(boundaries.len().saturating_sub(1));
                let before = env.measure(&display[..boundaries[caret]], env.body());
                let width = env.measure(&display, env.body());
                let x = if env.rtl() {
                    inner.max_x() - width + before
                } else {
                    inner.origin.x + before
                };
                list.fill_rect(
                    Rect::new(
                        Point::new(x, bounds.origin.y + 6.0),
                        Size::new(1.0, bounds.size.height - 12.0),
                    ),
                    env.tokens.content,
                );
            }
            let _ = list.pop_clip();
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.fields
            .iter()
            .map(|(_, id, field, _, _)| (*id, field.semantics(tree, *id).shared()))
            .collect()
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = event
        else {
            return false;
        };
        for (_, id, field, _, _) in &mut self.fields {
            let bounds = tree.bounds(*id).unwrap_or_default();
            if !bounds.contains(position) || !field.visual_state(tree, *id).enabled {
                continue;
            }
            let _ = tree.set_focus(Some(*id));
            let display = field.display_value();
            let inner = bounds.inset(8.0);
            let x = if env.rtl() {
                position.x - (inner.max_x() - env.measure(&display, env.body()))
            } else {
                position.x - inner.origin.x
            };
            let caret = caret_at(env, &display, x);
            let _ = field.handle(
                tree,
                *id,
                TextFieldInput::SetCaret {
                    caret,
                    extend: modifiers.shift,
                },
            );
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
        let focus = tree.focus();
        let Some((_, id, field, _, required)) = self
            .fields
            .iter_mut()
            .find(|(_, id, _, _, _)| Some(*id) == focus)
        else {
            return false;
        };
        if modifiers.command || modifiers.control {
            let input = match pressed(event) {
                Some(Key::Character('a')) => TextFieldInput::SelectAll,
                Some(Key::Character('z')) if modifiers.shift => TextFieldInput::Redo,
                Some(Key::Character('z')) => TextFieldInput::Undo,
                _ => return false,
            };
            let _ = field.handle(tree, *id, input);
            return true;
        }
        if *required && pressed(event) == Some(Key::Enter) {
            let validation = validate_required(field.value());
            let _ = field.handle(tree, *id, TextFieldInput::SetValidation(validation));
            return true;
        }
        let change = field.handle(
            tree,
            *id,
            TextFieldInput::Key {
                event,
                extend: modifiers.shift,
                word: modifiers.option,
            },
        );
        if *required && change == Ok(TextFieldChange::Edited) {
            let _ = field.handle(
                tree,
                *id,
                TextFieldInput::SetValidation(ValidationState::Intermediate),
            );
        }
        true
    }

    fn text(&mut self, tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        let focus = tree.focus();
        let Some((_, id, field, _, required)) = self
            .fields
            .iter_mut()
            .find(|(_, id, _, _, _)| Some(*id) == focus)
        else {
            return false;
        };
        let printable: String = text.chars().filter(|c| !c.is_control()).collect();
        if printable.is_empty() {
            return false;
        }
        let change = field.handle(tree, *id, TextFieldInput::Text(printable));
        if *required && change == Ok(TextFieldChange::Edited) {
            let _ = field.handle(
                tree,
                *id,
                TextFieldInput::SetValidation(ValidationState::Intermediate),
            );
        }
        true
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let Some((_, id, field, _, _)) =
            self.fields.iter_mut().find(|(_, id, _, _, _)| *id == node)
        else {
            return false;
        };
        action == SemanticAction::SetValue
            && field
                .handle(
                    tree,
                    *id,
                    TextFieldInput::SetValue(value.unwrap_or_default().to_owned()),
                )
                .is_ok()
    }

    fn native_disabled(&self) -> bool {
        true
    }
}

const EDITOR_TEXT: &str = "Multilingual editing: Tiếng Việt có dấu, 日本語, 中文, and emoji 👩‍💻.\n\
Click to place the caret, drag to select, and use the arrow keys with Shift and Option.\n\
⌘A selects all; ⌘Z undoes and ⇧⌘Z redoes.";

/// The multi-line editor over a shaped layout.
pub struct TextEditorPage {
    node: NodeId,
    editor: TextEditor,
    layout: Option<TextLayout>,
    width: f64,
    dragging: bool,
}

fn editor_style(env: &Env<'_>) -> TextStyle {
    TextStyle::new(FONT, env.body()).with_line_height(1.3)
}

pub fn text_editor(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let id = node(tree, parent, true);
    let mut editor = TextEditor::new("Document", EDITOR_TEXT);
    let _ = editor.set_enabled(tree, id, env.enabled);
    Box::new(TextEditorPage {
        node: id,
        editor,
        layout: None,
        width: 0.0,
        dragging: false,
    })
}

impl TextEditorPage {
    fn reshape(&mut self, shaper: &dyn TextShaper, env: &Env<'_>) {
        let width = self.width.max(1.0);
        self.layout = shaper
            .shape(&self.editor.layout_text(), &editor_style(env), Some(width))
            .ok();
    }

    fn origin(&self, tree: &Tree) -> Point {
        let bounds = tree.bounds(self.node).unwrap_or_default();
        Point::new(bounds.origin.x + 10.0, bounds.origin.y + 10.0)
    }

    fn input(&mut self, tree: &mut Tree, input: TextEditorInput, env: &Env<'_>) -> bool {
        let Some(layout) = &self.layout else {
            return false;
        };
        let changed = self.editor.handle(tree, self.node, layout, input).is_ok();
        self.reshape(env.shaper, env);
        changed
    }

    fn local(&self, tree: &Tree, position: Point) -> Point {
        let origin = self.origin(tree);
        Point::new(position.x - origin.x, position.y - origin.y)
    }
}

impl Page for TextEditorPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let rect = Rect::new(
            bounds.origin,
            Size::new(bounds.size.width.min(620.0), bounds.size.height.min(320.0)),
        );
        let _ = tree.set_bounds(self.node, rect);
        self.width = (rect.size.width - 20.0).max(1.0);
        self.editor.set_page_height(rect.size.height - 20.0);
        self.reshape(env.shaper, env);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let bounds = tree.bounds(self.node).unwrap_or_default();
        env.panel(list, bounds);
        if tree.focus() == Some(self.node) {
            env.focus_ring(list, bounds);
        }
        let Some(layout) = &self.layout else {
            return;
        };
        let origin = self.origin(tree);
        list.push_clip(bounds);
        let selection = choscorui::core::lerp_color(env.tokens.surface, env.tokens.accent, 0.35);
        for rect in self.editor.selection_rects(layout) {
            list.fill_rect(rect.translate(origin.x, origin.y), selection);
        }
        let _ = layout.draw(list, origin, env.tokens.content);
        if tree.focus() == Some(self.node)
            && let Some(caret) = self.editor.caret_rect(layout)
        {
            list.fill_rect(caret.translate(origin.x, origin.y), env.tokens.content);
        }
        let _ = list.pop_clip();
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        vec![(self.node, self.editor.semantics())]
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } if tree.bounds(self.node).is_some_and(|b| b.contains(position)) => {
                let _ = tree.set_focus(Some(self.node));
                let _ = tree.set_pointer_capture(Some(self.node));
                self.dragging = true;
                let local = self.local(tree, position);
                self.input(
                    tree,
                    TextEditorInput::PointerDown {
                        position: local,
                        extend: modifiers.shift,
                        clicks: 1,
                    },
                    env,
                )
            }
            PointerEvent::Move { position } if self.dragging => {
                let local = self.local(tree, position);
                self.input(tree, TextEditorInput::PointerDrag { position: local }, env)
            }
            PointerEvent::Up { .. } if self.dragging => {
                self.dragging = false;
                let _ = tree.set_pointer_capture(None);
                self.input(tree, TextEditorInput::PointerUp, env)
            }
            _ => false,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if tree.focus() != Some(self.node) {
            return false;
        }
        let input = if modifiers.command {
            match pressed(event) {
                Some(Key::Character('a')) => TextEditorInput::SelectAll,
                Some(Key::Character('z')) if modifiers.shift => TextEditorInput::Redo,
                Some(Key::Character('z')) => TextEditorInput::Undo,
                _ => return false,
            }
        } else {
            TextEditorInput::Key {
                event,
                extend: modifiers.shift,
                word: modifiers.option,
            }
        };
        self.input(tree, input, env);
        true
    }

    fn text(&mut self, tree: &mut Tree, text: &str, env: &Env<'_>) -> bool {
        if tree.focus() != Some(self.node) {
            return false;
        }
        let printable: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .collect();
        !printable.is_empty() && self.input(tree, TextEditorInput::Text(printable), env)
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        env: &Env<'_>,
    ) -> bool {
        node == self.node
            && self.input(
                tree,
                TextEditorInput::Semantic(action, value.map(str::to_owned)),
                env,
            )
    }

    fn native_disabled(&self) -> bool {
        true
    }
}

/// How a structured field turns raw text into what it shows and whether
/// it is valid.
type Rule = Box<dyn Fn(&str) -> (String, ValidationState)>;

/// One structured-input field.
pub struct RuleField {
    caption: NodeId,
    node: NodeId,
    label: String,
    raw: String,
    rule: Rule,
    placeholder: String,
}

/// A page of structured fields, each validated as the user types.
pub struct RulePage {
    fields: Vec<RuleField>,
}

impl RulePage {
    fn boxed(
        tree: &mut Tree,
        parent: NodeId,
        fields: Vec<(String, &str, String, Rule)>,
    ) -> Box<dyn Page> {
        Box::new(Self {
            fields: fields
                .into_iter()
                .map(|(label, raw, placeholder, rule)| RuleField {
                    caption: node(tree, parent, false),
                    node: node(tree, parent, true),
                    label,
                    raw: raw.to_owned(),
                    rule,
                    placeholder,
                })
                .collect(),
        })
    }

    /// Replaces the raw text of field `index` and returns its validation,
    /// for scripted checks.
    #[cfg(target_os = "macos")]
    pub fn enter(&mut self, index: usize, text: &str) -> ValidationState {
        let field = &mut self.fields[index];
        field.raw = text.to_owned();
        (field.rule)(&field.raw).1
    }

    fn focused(&mut self, tree: &Tree) -> Option<&mut RuleField> {
        self.fields
            .iter_mut()
            .find(|field| tree.focus() == Some(field.node))
    }
}

impl Page for RulePage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for field in &self.fields {
            let _ = tree.set_bounds(field.caption, column.row(20.0));
            let _ = tree.set_bounds(field.node, column.cells(32.0, &[300.0])[0]);
            column.space(18.0);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for field in &self.fields {
            env.caption(
                list,
                tree.bounds(field.caption).unwrap_or_default(),
                &field.label,
            );
            let bounds = tree.bounds(field.node).unwrap_or_default();
            let (shown, validation) = (field.rule)(&field.raw);
            let message = Rect::new(
                Point::new(bounds.origin.x, bounds.max_y() + 2.0),
                Size::new(bounds.size.width + 240.0, 16.0),
            );
            paint_field(
                env,
                list,
                bounds,
                &shown,
                &field.placeholder,
                tree.focus() == Some(field.node),
                &validation,
                Some(message),
            );
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.fields
            .iter()
            .map(|field| {
                let (shown, validation) = (field.rule)(&field.raw);
                (
                    field.node,
                    field_semantics(&field.label, &shown, &validation),
                )
            })
            .collect()
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
        let Some(field) = self.focused(tree) else {
            return false;
        };
        match key {
            Key::Backspace | Key::Delete => field.raw.pop().is_some(),
            Key::Escape => {
                field.raw.clear();
                true
            }
            _ => false,
        }
    }

    fn text(&mut self, tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        match self.focused(tree) {
            Some(field) => type_into(&mut field.raw, text),
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
        match self.fields.iter_mut().find(|field| field.node == node) {
            Some(field) if action == SemanticAction::SetValue => {
                field.raw = value.unwrap_or_default().to_owned();
                true
            }
            _ => false,
        }
    }
}

pub fn number_input(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let amount = NumberInput::new(env.locale.clone(), 2).with_range(Some(0.0), Some(1e9));
    let quantity = NumberInput::new(env.locale.clone(), 0).with_range(Some(1.0), Some(10_000.0));
    let sample = env
        .locale
        .format_decimal(1234.5, 2)
        .unwrap_or_else(|_| "1234.50".into());
    RulePage::boxed(
        tree,
        parent,
        vec![
            (
                format!(
                    "Amount (0 – 1,000,000,000; 2 decimals, locale {})",
                    env.locale.tag()
                ),
                sample.as_str(),
                sample.clone(),
                Box::new(move |text: &str| (text.to_owned(), amount.validate(text).0)) as Rule,
            ),
            (
                "Quantity (1 – 10,000; whole numbers)".into(),
                "12",
                "12".into(),
                Box::new(move |text: &str| (text.to_owned(), quantity.validate(text).0)),
            ),
        ],
    )
}

pub fn input_mask(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let masked = |pattern: &str| -> (String, Rule) {
        let mask = InputMask::parse(pattern).expect("demo masks are valid");
        let placeholder = mask.placeholder('_');
        (
            placeholder,
            Box::new(move |raw: &str| (mask.apply(raw).display, mask.validate(raw))),
        )
    };
    let (phone_placeholder, phone) = masked("(999) 999-9999");
    let (plate_placeholder, plate) = masked(">AA-99999");
    RulePage::boxed(
        tree,
        parent,
        vec![
            (
                "Phone, mask (999) 999-9999".into(),
                "0905",
                phone_placeholder,
                phone,
            ),
            (
                "Licence plate, mask >AA-99999 (uppercased)".into(),
                "",
                plate_placeholder,
                plate,
            ),
        ],
    )
}

pub fn date_input(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let any = DateInput::new(env.locale.clone());
    let placeholder = any.placeholder();
    let ranged = DateInput::new(env.locale.clone()).with_range(
        CivilDate::new(2026, 1, 1).ok(),
        CivilDate::new(2026, 12, 31).ok(),
    );
    let today = CivilDate::new(2026, 10, 12).expect("valid date");
    let sample = any.format(today);
    RulePage::boxed(
        tree,
        parent,
        vec![
            (
                format!("Any date ({placeholder})"),
                sample.as_str(),
                placeholder.clone(),
                Box::new(move |text: &str| (text.to_owned(), any.validate(text).0)) as Rule,
            ),
            (
                "A date in 2026".into(),
                "",
                placeholder,
                Box::new(move |text: &str| (text.to_owned(), ranged.validate(text).0)),
            ),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::tests::Harness;

    #[test]
    fn structured_input_pages_hold_two_fields_each() {
        for id in ["number-input", "input-mask", "date-input"] {
            let mut h = Harness::new(id);
            assert_eq!(h.get::<RulePage>().fields.len(), 2, "{id}");
        }
    }
}
