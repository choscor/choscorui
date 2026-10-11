//! A multiline text input that grows with its content.

use super::input::default_width;
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::style::{
    ControlKind, EstimatedText, PaintContext, Slot, StyleQuery, TextAlign, TextMeasure, VisualState,
};
use crate::text_editor::{TextEditor, TextEditorChange, TextEditorInput};
use choscorui_core::{
    CaretStop, ColorRole, EventPhase, EventReply, Key, KeyEvent, LayoutLine, Measured, Point,
    PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, Size, TextLayout, TypeSpec,
    grapheme_boundaries,
};
use std::cell::{Cell, RefCell};

/// Lays out `text` in lines no wider than `width`, breaking after spaces
/// and at line breaks, with advances from `measure`. Each line is one
/// `spec` line height tall.
pub(crate) fn measured_layout(
    text: &str,
    spec: &TypeSpec,
    width: f64,
    measure: &dyn TextMeasure,
) -> TextLayout {
    // The boundaries run from 0 through `text.len()`.
    let boundaries = grapheme_boundaries(text);
    let graphemes: Vec<&str> = boundaries
        .windows(2)
        .map(|pair| &text[pair[0]..pair[1]])
        .collect();
    let advances: Vec<f64> = graphemes
        .iter()
        .map(|g| {
            if *g == "\n" {
                0.0
            } else {
                measure.width(g, spec)
            }
        })
        .collect();
    let mut lines = Vec::new();
    let mut start = 0;
    let push = |lines: &mut Vec<LayoutLine>, from: usize, to: usize| {
        let mut x = 0.0;
        let mut carets = vec![CaretStop {
            primary: 0.0,
            secondary: 0.0,
        }];
        let mut extents = Vec::new();
        for advance in &advances[from..to] {
            extents.push((x, x + advance));
            x += advance;
            carets.push(CaretStop {
                primary: x,
                secondary: x,
            });
        }
        let trailing: f64 = graphemes[from..to]
            .iter()
            .rev()
            .take_while(|g| g.trim().is_empty())
            .zip(advances[from..to].iter().rev())
            .map(|(_, advance)| advance)
            .sum();
        let top = lines.len() as f64 * spec.line_height;
        lines.push(LayoutLine {
            bytes: boundaries[from]..boundaries[to],
            graphemes: from..to,
            baseline: top + spec.baseline(),
            ascent: spec.size * 0.95,
            descent: spec.size * 0.25,
            leading: 0.0,
            width: x - trailing,
            offset_x: 0.0,
            right_to_left: false,
            runs: Vec::new(),
            carets,
            extents,
        });
    };
    let mut x = 0.0;
    let mut last_space: Option<usize> = None;
    let mut index = 0;
    while index < graphemes.len() {
        if graphemes[index] == "\n" {
            push(&mut lines, start, index);
            start = index + 1;
            index += 1;
            x = 0.0;
            last_space = None;
            continue;
        }
        if x + advances[index] > width && index > start {
            // Break after the last space, or before this grapheme.
            let end = last_space.map_or(index, |space| space + 1);
            push(&mut lines, start, end);
            start = end;
            x = advances[start..index].iter().sum();
            last_space = None;
        }
        if graphemes[index].trim().is_empty() {
            last_space = Some(index);
        }
        x += advances[index];
        index += 1;
    }
    push(&mut lines, start, graphemes.len());
    let mut style = spec.text_style();
    style.line_height = spec.line_height / (spec.size * 1.2);
    TextLayout::from_lines(text, style, Some(width.max(1.0)), lines)
        .expect("measured lines cover the text in order")
}

/// A textarea: the `Textarea` recipe box around multiline editable text.
/// It grows with its lines, from a 64 pt minimum.
pub struct Textarea {
    editor: TextEditor,
    placeholder: String,
    invalid: bool,
    /// The text width and type from the last measure or arrange.
    width: f64,
    spec: Option<TypeSpec>,
    /// The layout and text origin of the last paint, for pointer hits and
    /// line motion.
    layout: RefCell<Option<TextLayout>>,
    origin: Cell<Point>,
    dragging: bool,
}

impl Textarea {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            editor: TextEditor::new(name, value),
            placeholder: String::new(),
            invalid: false,
            width: 0.0,
            spec: None,
            layout: RefCell::new(None),
            origin: Cell::new(Point::new(0.0, 0.0)),
            dragging: false,
        }
    }

    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn value(&self) -> &str {
        self.editor.value()
    }

    pub fn set_invalid(&mut self, invalid: bool) {
        self.invalid = invalid;
    }

    fn query(slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Textarea, slot).state(state)
    }

    fn state(&self, host: VisualState) -> VisualState {
        host.set(VisualState::FOCUS_VISIBLE, host.focused())
            .set(VisualState::INVALID, self.invalid)
            .set(
                VisualState::DISABLED,
                host.disabled() || !self.editor.is_enabled(),
            )
    }

    fn layout_for(&self, spec: &TypeSpec, measure: &dyn TextMeasure) -> TextLayout {
        measured_layout(
            &self.editor.layout_text(),
            spec,
            self.width.max(1.0),
            measure,
        )
    }

    fn apply(&mut self, cx: &mut ControlContext<'_>, input: TextEditorInput) {
        let cached = self.layout.borrow().clone();
        let Some(layout) =
            cached.or_else(|| self.spec.map(|spec| self.layout_for(&spec, &EstimatedText)))
        else {
            return;
        };
        let node = cx.node();
        let change = self
            .editor
            .handle(cx.tree_mut(), node, &layout, input)
            .unwrap_or(TextEditorChange::None);
        if change == TextEditorChange::Edited {
            cx.emit(ControlMessage::Changed(ControlValue::Text(
                self.editor.value().to_string(),
            )));
            // The textarea grows or shrinks with its lines.
            cx.request_layout();
        }
        if change != TextEditorChange::None {
            cx.request_paint();
            cx.request_semantics();
        }
    }
}

/// `point` relative to `origin`.
fn local(origin: Point, point: Point) -> Point {
    Point::new(point.x - origin.x, point.y - origin.y)
}

impl CustomControl for Textarea {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(Self::query(Slot::Root, VisualState::NONE));
        let width = default_width(cx, &style);
        self.width = (width - style.padding.horizontal()).max(1.0);
        self.spec = Some(style.text);
        let lines = self.layout_for(&style.text, cx.text).lines().len().max(1);
        let height = lines as f64 * style.text.line_height + style.padding.vertical();
        Measured {
            size: Size::new(width, height.max(style.min_height)),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let style = cx.style(Self::query(Slot::Root, VisualState::NONE));
        self.width = (bounds.size.width - style.padding.horizontal()).max(1.0);
        self.spec = Some(style.text);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let modifiers = cx.modifiers();
        let origin = self.origin.get();
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if cx.bounds().contains(*position) => {
                cx.request_focus();
                let position = local(origin, *position);
                self.apply(
                    cx,
                    TextEditorInput::PointerDown {
                        position,
                        extend: modifiers.shift,
                        clicks: 1,
                    },
                );
                self.dragging = true;
                cx.capture_pointer(true);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) if self.dragging => {
                let position = local(origin, *position);
                self.apply(cx, TextEditorInput::PointerDrag { position });
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) if self.dragging => {
                self.dragging = false;
                cx.capture_pointer(false);
                self.apply(cx, TextEditorInput::PointerUp);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) if cx.is_focused() => {
                let input = match (*key, modifiers.command) {
                    (Key::Character('a'), true) => TextEditorInput::SelectAll,
                    (Key::Character('z'), true) if modifiers.shift => TextEditorInput::Redo,
                    (Key::Character('z'), true) => TextEditorInput::Undo,
                    (Key::Character('x'), true) => TextEditorInput::Cut,
                    (Key::Tab | Key::Escape, _) | (_, true) => return EventReply::Continue,
                    (_, false) => TextEditorInput::Key {
                        event: KeyEvent::Down {
                            key: *key,
                            repeat: false,
                        },
                        extend: modifiers.shift,
                        word: modifiers.option,
                    },
                };
                self.apply(cx, input);
                EventReply::Stop
            }
            ControlEvent::Text(text) if cx.is_focused() => {
                self.apply(cx, TextEditorInput::Text(text.clone()));
                EventReply::Stop
            }
            ControlEvent::FocusChanged(_) => {
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = self.state(cx.state);
        let root = cx.style(Self::query(Slot::Root, state));
        let bounds = cx.bounds;
        let layout = self.layout_for(&root.text, cx.text);
        let focused = state.focused();
        cx.slot(bounds, &root, |cx, inner| {
            self.origin.set(inner.origin);
            cx.list.push_clip(inner);
            if self.editor.value().is_empty() {
                let placeholder = cx.style(Self::query(Slot::Placeholder, state));
                let line = Rect::from_xywh(
                    inner.origin.x,
                    inner.origin.y,
                    inner.size.width,
                    root.text.line_height,
                );
                cx.label(line, &self.placeholder, &placeholder, TextAlign::Start);
            }
            let rtl = cx.rtl;
            let mirror = |rect: Rect| {
                if rtl {
                    Rect::from_xywh(
                        inner.max_x() - (rect.max_x() - inner.origin.x),
                        rect.origin.y,
                        rect.size.width,
                        rect.size.height,
                    )
                } else {
                    rect
                }
            };
            if focused {
                let color = cx.tokens.color(ColorRole::Primary).with_alpha(0.25);
                for rect in self.editor.selection_rects(&layout) {
                    let rect = mirror(rect.translate(inner.origin.x, inner.origin.y));
                    cx.list.fill_rect(rect, color);
                }
            }
            let text = layout.text().to_string();
            for line in layout.lines() {
                let slice = &text[line.bytes.clone()];
                let rect = mirror(Rect::from_xywh(
                    inner.origin.x + line.offset_x,
                    inner.origin.y + line.baseline - root.text.baseline(),
                    line.width.max(1.0),
                    root.text.line_height,
                ));
                cx.label(rect, slice, &root, TextAlign::Start);
            }
            if focused && let Some(caret) = self.editor.caret_rect(&layout) {
                let rect = mirror(Rect::from_xywh(
                    inner.origin.x + caret.origin.x,
                    inner.origin.y + caret.origin.y,
                    1.0,
                    caret.size.height,
                ));
                cx.list.fill_rect(rect, root.foreground);
            }
            let _ = cx.list.pop_clip();
        });
        *self.layout.borrow_mut() = Some(layout);
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = self.editor.semantics();
        node.state.invalid = self.invalid;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::SetValue if self.editor.is_enabled() => {
                self.apply(
                    cx,
                    TextEditorInput::SetValue(value.unwrap_or_default().to_string()),
                );
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
        self.editor.set_enabled_state(enabled);
    }
}
