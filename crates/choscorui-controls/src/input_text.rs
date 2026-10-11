//! The text of an input box: the value or the placeholder, the selection,
//! IME marked text, and the caret. Text fields, masks, spin boxes, and
//! combo boxes share it.

use crate::style::{
    ControlKind, PaintContext, ResolvedStyle, Slot, StyleQuery, TextAlign, VisualState,
};
use choscorui_core::{ColorRole, Rect, grapheme_boundaries};
use std::ops::Range;

/// One line of input text and its editing marks, in grapheme indices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InputText<'a> {
    pub text: &'a str,
    /// Shown in muted text while `text` is empty.
    pub placeholder: &'a str,
    /// The caret, drawn only when the state is focused.
    pub caret: Option<usize>,
    pub selection: Range<usize>,
    /// IME marked text, drawn underlined.
    pub marked: Option<Range<usize>>,
}

impl<'a> InputText<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    #[must_use]
    pub fn caret(mut self, caret: usize) -> Self {
        self.caret = Some(caret);
        self
    }

    /// Paints the input box of the `Input` recipe in `state`, then the text
    /// inside its padding. The text scrolls so the caret stays visible.
    pub fn paint(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root).state(state));
        let bounds = cx.bounds;
        cx.slot(bounds, &style, |cx, rect| self.paint_text(cx, rect, state));
    }

    /// Paints only the text, selection, marks, and caret inside `rect`.
    pub fn paint_text(&self, cx: &mut PaintContext<'_>, rect: Rect, state: VisualState) {
        let query = |slot| StyleQuery::new(ControlKind::Input, slot).state(state);
        if self.text.is_empty() {
            let placeholder = cx.style(query(Slot::Placeholder));
            cx.label(rect, self.placeholder, &placeholder, TextAlign::Start);
            if state.focused() {
                let style = cx.style(query(Slot::Root));
                let x = if cx.rtl {
                    rect.max_x() - 1.0
                } else {
                    rect.origin.x
                };
                paint_caret(cx, rect, x, &style);
            }
            return;
        }
        let style = cx.style(query(Slot::Root));
        let bounds = grapheme_boundaries(self.text);
        let byte = |index: usize| bounds.get(index).copied().unwrap_or(self.text.len());
        let x_of =
            |cx: &PaintContext<'_>, index: usize| cx.measure(&self.text[..byte(index)], &style);
        let width = cx.measure(self.text, &style);
        let caret_x = self.caret.map_or(0.0, |caret| x_of(cx, caret));
        // Scroll so the caret stays inside the box.
        let scroll = if state.focused() {
            (caret_x - rect.size.width + 1.0).max(0.0)
        } else {
            0.0
        };
        // Physical x of a logical offset from the start of the text.
        let at = |offset: f64, rtl: bool| {
            if rtl {
                rect.max_x() - offset + scroll
            } else {
                rect.origin.x + offset - scroll
            }
        };
        cx.list.push_clip(rect);
        let line_height = style.text.line_height.min(rect.size.height);
        let top = rect.origin.y + (rect.size.height - line_height) / 2.0;
        if self.selection.start < self.selection.end && state.focused() {
            let (a, b) = (x_of(cx, self.selection.start), x_of(cx, self.selection.end));
            let (left, right) = if cx.rtl {
                (at(b, true), at(a, true))
            } else {
                (at(a, false), at(b, false))
            };
            let color = cx.tokens.color(ColorRole::Primary).with_alpha(0.25);
            cx.list
                .fill_rect(Rect::from_xywh(left, top, right - left, line_height), color);
        }
        let start = if cx.rtl {
            at(width, true)
        } else {
            at(0.0, false)
        };
        let line = Rect::from_xywh(start, rect.origin.y, width, rect.size.height);
        cx.label(line, self.text, &style, TextAlign::Start);
        if let Some(marked) = &self.marked {
            let (a, b) = (x_of(cx, marked.start), x_of(cx, marked.end));
            let (left, right) = if cx.rtl {
                (at(b, true), at(a, true))
            } else {
                (at(a, false), at(b, false))
            };
            let y = top + line_height - 1.0;
            cx.list.fill_rect(
                Rect::from_xywh(left, y, right - left, 1.0),
                style.foreground,
            );
        }
        if state.focused() && self.caret.is_some() {
            let x = at(caret_x, cx.rtl) - if cx.rtl { 1.0 } else { 0.0 };
            paint_caret(cx, rect, x, &style);
        }
        let _ = cx.list.pop_clip();
    }
}

/// A 1 pt caret of the line height at physical `x`.
fn paint_caret(cx: &mut PaintContext<'_>, rect: Rect, x: f64, style: &ResolvedStyle) {
    let height = style.text.line_height.min(rect.size.height);
    let y = rect.origin.y + (rect.size.height - height) / 2.0;
    cx.list
        .fill_rect(Rect::from_xywh(x, y, 1.0, height), style.foreground);
}
