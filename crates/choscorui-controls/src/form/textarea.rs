//! A multiline text input that grows with its content.

use super::input::default_width;
use super::measured::measured_layout;
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::style::{
    ControlKind, EstimatedText, PaintContext, Slot, StyleQuery, TextAlign, TextMeasure,
    VisualState, content_rect,
};
use crate::text_editor::{
    TextEditor, TextEditorChange, TextEditorInput, layout_point, lines_height, reveal_caret,
    scroll_event, window_rect,
};
use crate::{OverlayScrollBar, ScrollOrientation};
use choscorui_core::{
    ColorRole, DesignTokens, EventPhase, EventReply, FontWeight, Measured, Point, Rect,
    SemanticAction, SemanticNode, Size, TextInputInfo, TextLayout, TextSize, Tree, TypeSpec,
};
use std::cell::{Cell, RefCell};

/// A textarea: the `Textarea` recipe box around multiline editable text.
/// It grows with its lines, from a 64 pt minimum, up to an optional
/// maximum height. Then the text scrolls.
pub struct Textarea {
    editor: TextEditor,
    placeholder: String,
    invalid: bool,
    max_height: Option<f64>,
    /// The overlay scroll bar, which keeps the scroll offset.
    bar: OverlayScrollBar,
    /// The text width and type from the last measure or arrange.
    width: f64,
    spec: Option<TypeSpec>,
    /// The layout and text rectangle of the last paint, for pointer hits
    /// and line motion.
    layout: RefCell<Option<TextLayout>>,
    inner: Cell<Rect>,
    rtl: bool,
}

impl Textarea {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            editor: TextEditor::new(name, value),
            placeholder: String::new(),
            invalid: false,
            max_height: None,
            bar: OverlayScrollBar::new(ScrollOrientation::Vertical),
            width: 0.0,
            spec: None,
            layout: RefCell::new(None),
            inner: Cell::new(Rect::default()),
            rtl: false,
        }
    }

    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Stops the growth at `height` points. Taller text scrolls with the
    /// wheel and the scroll bar, and the caret stays in view.
    #[must_use]
    pub fn with_max_height(mut self, height: f64) -> Self {
        self.max_height = Some(height).filter(|height| height.is_finite());
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
            .set(VisualState::INVALID, host.invalid() || self.invalid)
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

    /// Replaces the value without a `Changed` message, also while
    /// disabled.
    fn replace(&mut self, text: &str) {
        // The editor takes a value from outside through any live node and
        // any layout; these exist only for the call.
        let mut scratch = Tree::new(Rect::default());
        let node = scratch.root();
        let spec = self
            .spec
            .unwrap_or_else(|| DesignTokens::light().text(TextSize::Sm, FontWeight::REGULAR));
        let layout = measured_layout("", &spec, 1.0, &EstimatedText);
        let enabled = self.editor.is_enabled();
        self.editor.set_enabled_state(true);
        let input = TextEditorInput::SetValue(text.to_string());
        let _ = self.editor.handle(&mut scratch, node, &layout, input);
        self.editor.set_enabled_state(enabled);
        *self.layout.borrow_mut() = None;
    }

    /// The layout of the last paint, or an estimate before the first
    /// paint.
    fn current_layout(&self) -> Option<TextLayout> {
        let cached = self.layout.borrow().clone();
        cached.or_else(|| self.spec.map(|spec| self.layout_for(&spec, &EstimatedText)))
    }

    /// Scrolls the caret into view, with the painted layout while it has
    /// the current text, otherwise with an estimate.
    fn reveal(&mut self, cx: &mut ControlContext<'_>) {
        let Some(spec) = self.spec else {
            return;
        };
        let text = self.editor.layout_text();
        let layout = self
            .layout
            .borrow()
            .clone()
            .filter(|layout| layout.text() == text)
            .unwrap_or_else(|| self.layout_for(&spec, &EstimatedText));
        reveal_caret(cx, &mut self.bar, &self.editor, (&layout, &spec));
    }

    fn apply(&mut self, cx: &mut ControlContext<'_>, input: TextEditorInput) {
        let Some(layout) = self.current_layout() else {
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

impl CustomControl for Textarea {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(Self::query(Slot::Root, VisualState::NONE));
        let width = default_width(cx, &style);
        self.width = (width - style.padding.horizontal()).max(1.0);
        self.spec = Some(style.text);
        let lines = self.layout_for(&style.text, cx.text).lines().len().max(1);
        let height = lines as f64 * style.text.line_height + style.padding.vertical();
        let height = self.max_height.map_or(height, |max| height.min(max));
        Measured {
            size: Size::new(width, height.max(style.min_height)),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let style = cx.style(Self::query(Slot::Root, VisualState::NONE));
        self.width = (bounds.size.width - style.padding.horizontal()).max(1.0);
        self.spec = Some(style.text);
        self.rtl = cx.rtl();
        self.bar.arrange(cx, content_rect(bounds, &style, self.rtl));
        let layout = self.layout_for(&style.text, cx.text);
        self.bar.set_content(lines_height(&layout, &style.text));
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let Some(layout) = self.current_layout() else {
            return EventReply::Continue;
        };
        let line = self.spec.map_or(0.0, |spec| spec.line_height);
        let pressed = self.editor.is_pressed();
        if let Some(reply) = scroll_event(cx, &mut self.bar, (event, pressed), line) {
            return reply;
        }
        let (inner, rtl, scroll) = (self.inner.get(), self.rtl, self.bar.offset());
        let reply = self.editor.hosted_event(cx, event, &layout, |point| {
            layout_point(inner, rtl, Point::new(point.x, point.y + scroll))
        });
        if reply == EventReply::Stop {
            // The textarea grows or shrinks with its lines.
            cx.request_layout();
            self.reveal(cx);
        }
        reply
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = self.state(cx.state);
        let root = cx.style(Self::query(Slot::Root, state));
        let bounds = cx.bounds;
        let layout = self.layout_for(&root.text, cx.text);
        let focused = state.focused();
        cx.slot(bounds, &root, |cx, mut inner| {
            self.inner.set(inner);
            cx.list.push_clip(inner);
            // The text moves up by the scroll offset, inside the clip.
            inner.origin.y -= self.bar.offset();
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
            self.bar.paint(cx);
        });
        *self.layout.borrow_mut() = Some(layout);
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = self.editor.semantics();
        node.state.invalid = self.invalid;
        node
    }

    fn text_input(&self) -> Option<TextInputInfo> {
        let layout = self.layout.borrow();
        let (inner, rtl, scroll) = (self.inner.get(), self.rtl, self.bar.offset());
        self.editor.hosted_text_input(layout.as_ref()?, |rect| {
            window_rect(inner, rtl, rect.translate(0.0, -scroll))
        })
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

    /// Takes `Text`.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Text(text) = value else {
            return false;
        };
        self.replace(text);
        true
    }
}
