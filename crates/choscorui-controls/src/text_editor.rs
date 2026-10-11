//! Multiline text editing over [`TextBuffer`] and a shaped [`TextLayout`].
//! The application shapes [`TextEditor::layout_text`] with its platform
//! shaper, passes the layout to [`TextEditor::handle`], and reveals
//! [`TextEditor::caret_rect`] in its scroll area after each change.

use super::text_field::TextComposition;
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, Styles, VisualState};
use choscorui_core::{
    DesignTokens, Dirty, Key, KeyEvent, NodeId, Point, Rect, SemanticAction, SemanticNode,
    SemanticRole, SemanticState, SemanticText, TextBuffer, TextLayout, TextMotion, Tree, TreeError,
};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub enum TextEditorInput {
    Text(String),
    SetMarkedText {
        text: String,
        selected_utf16: Range<usize>,
        replacement_utf16: Option<Range<usize>>,
    },
    CommitMarkedText {
        text: String,
        replacement_utf16: Option<Range<usize>>,
    },
    UnmarkMarkedText,
    CancelMarkedText,
    Key {
        event: KeyEvent,
        extend: bool,
        word: bool,
    },
    /// Pointer press in layout coordinates. `clicks` 2 selects a word,
    /// 3 selects a paragraph.
    PointerDown {
        position: Point,
        extend: bool,
        clicks: u8,
    },
    /// Pointer drag in layout coordinates while the press is held.
    PointerDrag {
        position: Point,
    },
    PointerUp,
    SetValue(String),
    SetCaret {
        caret: usize,
        extend: bool,
    },
    Undo,
    Redo,
    SelectAll,
    Cut,
    /// Assistive technology replaced the whole value.
    Semantic(SemanticAction, Option<String>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextEditorChange {
    None,
    Selection,
    Edited,
    Composition,
}

/// Multiline plain-text editor behavior. Painting and scrolling stay with
/// the caller; the editor reports geometry from the layout it is given.
pub struct TextEditor {
    name: String,
    buffer: TextBuffer,
    enabled: bool,
    read_only: bool,
    composition: Option<TextComposition>,
    preferred_x: Option<f64>,
    dragging: Option<Range<usize>>,
    page_height: f64,
    tab_inserts: bool,
}

impl TextEditor {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            buffer: TextBuffer::new(value.into()),
            enabled: true,
            read_only: false,
            composition: None,
            preferred_x: None,
            dragging: None,
            page_height: 400.0,
            tab_inserts: false,
        }
    }

    pub fn value(&self) -> &str {
        self.buffer.as_str()
    }
    pub fn selection(&self) -> Range<usize> {
        self.buffer.selection()
    }
    pub fn caret(&self) -> usize {
        self.buffer.caret()
    }
    pub fn selected_text(&self) -> &str {
        self.buffer.selected_text()
    }
    pub fn composition(&self) -> Option<&TextComposition> {
        self.composition.as_ref()
    }
    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    /// Sets the selection programmatically (grapheme indices); invalid
    /// boundaries leave it unchanged.
    pub fn set_selection(&mut self, anchor: usize, caret: usize) -> bool {
        self.buffer.set_selection(anchor, caret).is_ok()
    }

    /// Height used by Page Up/Down; set it from the visible viewport.
    pub fn set_page_height(&mut self, height: f64) {
        if height.is_finite() && height > 0.0 {
            self.page_height = height;
        }
    }
    /// When true, Tab inserts a tab character instead of moving focus.
    pub fn set_tab_inserts(&mut self, inserts: bool) {
        self.tab_inserts = inserts;
    }
    pub fn tab_inserts(&self) -> bool {
        self.tab_inserts
    }
    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
        if read_only {
            self.composition = None;
        }
    }

    pub fn set_enabled(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        enabled: bool,
    ) -> Result<(), TreeError> {
        if !tree.contains(id) {
            return Err(TreeError::MissingNode);
        }
        if self.enabled != enabled {
            self.enabled = enabled;
            if !enabled {
                self.composition = None;
                self.dragging = None;
            }
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
        }
        Ok(())
    }

    /// Enables or disables without a tree, for the hosted textarea.
    pub(crate) fn set_enabled_state(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.composition = None;
            self.dragging = None;
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Text to shape: the value with any marked text in place.
    pub fn layout_text(&self) -> String {
        let Some(composition) = &self.composition else {
            return self.buffer.as_str().to_owned();
        };
        let start = self
            .buffer
            .grapheme_to_byte(composition.replacement.start)
            .unwrap_or(0);
        let end = self
            .buffer
            .grapheme_to_byte(composition.replacement.end)
            .unwrap_or(start);
        let mut text = self.buffer.as_str().to_owned();
        text.replace_range(start..end, &composition.text);
        text
    }

    /// Marked range in UTF-16 units of [`layout_text`](Self::layout_text).
    pub fn marked_range_utf16(&self) -> Option<Range<usize>> {
        let composition = self.composition.as_ref()?;
        let byte = self
            .buffer
            .grapheme_to_byte(composition.replacement.start)?;
        let start = self.buffer.byte_to_utf16(byte)?;
        Some(start..start + composition.text.encode_utf16().count())
    }

    pub fn selection_utf16(&self) -> Range<usize> {
        let range = self.buffer.selection();
        let to_utf16 = |g: usize| {
            self.buffer
                .grapheme_to_byte(g)
                .and_then(|b| self.buffer.byte_to_utf16(b))
                .unwrap_or(0)
        };
        to_utf16(range.start)..to_utf16(range.end)
    }

    /// Caret rectangle in layout coordinates, for scroll-to-caret and IME
    /// candidate placement. During composition it follows the marked text.
    pub fn caret_rect(&self, layout: &TextLayout) -> Option<Rect> {
        let caret = match &self.composition {
            Some(composition) => {
                composition.replacement.start + TextBuffer::new(&composition.text).grapheme_count()
            }
            None => self.buffer.caret(),
        };
        layout.caret_rect(caret)
    }

    /// The layout origin in `bounds`: inside the `TextEditor` padding.
    pub fn content_origin(bounds: Rect, styles: &Styles, tokens: &DesignTokens) -> Point {
        let style = styles.resolve(tokens, StyleQuery::new(ControlKind::TextEditor, Slot::Root));
        Point::new(
            bounds.origin.x + style.padding.inline_start,
            bounds.origin.y + style.padding.block_start,
        )
    }

    /// Paints the textarea box in the context's bounds, then the selection,
    /// the text of `layout`, and the caret while `host` is focused. The
    /// layout starts at [`content_origin`](Self::content_origin).
    pub fn paint(&self, cx: &mut PaintContext<'_>, layout: &TextLayout, host: VisualState) {
        let query = |slot| StyleQuery::new(ControlKind::TextEditor, slot).state(host);
        let (root, selection, caret) = (
            cx.style(query(Slot::Root)),
            cx.style(query(Slot::Range)),
            cx.style(query(Slot::Indicator)),
        );
        let bounds = cx.bounds;
        cx.slot(bounds, &root, |_, _| {});
        let origin = Self::content_origin(bounds, cx.styles, cx.tokens);
        cx.list.push_clip(bounds);
        for rect in self.selection_rects(layout) {
            cx.list
                .fill_rect(rect.translate(origin.x, origin.y), selection.background);
        }
        let _ = layout.draw(cx.list, origin, root.foreground);
        if host.focused()
            && let Some(rect) = self.caret_rect(layout)
        {
            let mut rect = rect.translate(origin.x, origin.y);
            rect.size.width = caret.min_width;
            cx.list.fill_rect(rect, caret.background);
        }
        let _ = cx.list.pop_clip();
    }

    /// Selection highlight rectangles in layout coordinates.
    pub fn selection_rects(&self, layout: &TextLayout) -> Vec<Rect> {
        if self.composition.is_some() {
            return Vec::new();
        }
        layout.selection_rects(self.buffer.selection())
    }

    fn utf16_range_to_graphemes(&self, range: Range<usize>) -> Option<Range<usize>> {
        let start = self.buffer.utf16_to_byte(range.start)?;
        let end = self.buffer.utf16_to_byte(range.end)?;
        Some(self.buffer.byte_to_grapheme(start)?..self.buffer.byte_to_grapheme(end)?)
    }

    fn line_bounds(&self, layout: &TextLayout, caret: usize) -> Option<Range<usize>> {
        let line = &layout.lines()[layout.line_for_caret(caret)?];
        Some(line.graphemes.clone())
    }

    fn paragraph_bounds(&self, caret: usize) -> Range<usize> {
        let text = self.buffer.as_str();
        let byte = self.buffer.grapheme_to_byte(caret).unwrap_or(text.len());
        let start = text[..byte].rfind('\n').map_or(0, |i| i + 1);
        let end = text[byte..].find('\n').map_or(text.len(), |i| byte + i);
        self.buffer.byte_to_grapheme(start).unwrap_or(0)
            ..self
                .buffer
                .byte_to_grapheme(end)
                .unwrap_or(self.buffer.grapheme_count())
    }

    fn word_bounds(&mut self, caret: usize) -> Range<usize> {
        let mut probe = self.buffer.clone();
        let _ = probe.set_caret((caret + 1).min(probe.grapheme_count()), false);
        let start = probe.move_caret(TextMotion::WordLeft, false);
        let end = probe.move_caret(TextMotion::WordRight, false);
        start.min(caret)..end.max(caret)
    }

    /// Applies one input. `layout` must be the current layout of
    /// [`layout_text`](Self::layout_text). Returns `MissingNode` and changes
    /// nothing once the editor's node has been removed.
    pub fn handle(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        layout: &TextLayout,
        input: TextEditorInput,
    ) -> Result<TextEditorChange, TreeError> {
        if !tree.contains(id) {
            self.composition = None;
            self.dragging = None;
            return Err(TreeError::MissingNode);
        }
        let external = matches!(input, TextEditorInput::SetValue(_));
        if !self.enabled || (!external && tree.focus() != Some(id)) {
            return Ok(TextEditorChange::None);
        }
        let before_value = self.buffer.as_str().to_owned();
        let before_selection = self.buffer.selection();
        let before_composition = self.composition.clone();
        let editable = !self.read_only;
        let mut keep_preferred_x = false;
        match input {
            TextEditorInput::Text(text) if editable => {
                self.composition = None;
                self.buffer.replace_selection(&text.replace("\r\n", "\n"));
            }
            TextEditorInput::SetMarkedText {
                text,
                selected_utf16,
                replacement_utf16,
            } if editable => {
                let length = text.encode_utf16().count();
                let start = selected_utf16.start.min(length);
                let end = selected_utf16.end.min(length).max(start);
                let replacement = replacement_utf16
                    .and_then(|range| self.utf16_range_to_graphemes(range))
                    .or_else(|| self.composition.as_ref().map(|c| c.replacement.clone()))
                    .unwrap_or_else(|| self.buffer.selection());
                self.composition = Some(TextComposition {
                    text,
                    selected_utf16: start..end,
                    replacement,
                });
            }
            TextEditorInput::CommitMarkedText {
                text,
                replacement_utf16,
            } if editable => {
                let replacement = replacement_utf16
                    .and_then(|range| self.utf16_range_to_graphemes(range))
                    .or_else(|| self.composition.take().map(|c| c.replacement));
                self.composition = None;
                if let Some(range) = replacement {
                    self.buffer
                        .set_selection(range.start, range.end)
                        .map_err(|_| TreeError::MissingNode)?;
                }
                self.buffer.replace_selection(&text);
            }
            TextEditorInput::UnmarkMarkedText => {
                if let Some(composition) = self.composition.take() {
                    self.buffer
                        .set_selection(composition.replacement.start, composition.replacement.end)
                        .map_err(|_| TreeError::MissingNode)?;
                    self.buffer.replace_selection(&composition.text);
                }
            }
            TextEditorInput::CancelMarkedText => self.composition = None,
            TextEditorInput::SetValue(text) => {
                self.composition = None;
                if text != self.buffer.as_str() {
                    let count = self.buffer.grapheme_count();
                    let _ = self.buffer.set_selection(0, count);
                    self.buffer.replace_selection(&text);
                }
            }
            TextEditorInput::Semantic(SemanticAction::SetValue, Some(text)) if editable => {
                self.composition = None;
                let count = self.buffer.grapheme_count();
                let _ = self.buffer.set_selection(0, count);
                self.buffer.replace_selection(&text);
            }
            TextEditorInput::Undo if editable => {
                self.composition = None;
                self.buffer.undo();
            }
            TextEditorInput::Redo if editable => {
                self.composition = None;
                self.buffer.redo();
            }
            TextEditorInput::SelectAll => {
                let count = self.buffer.grapheme_count();
                let _ = self.buffer.set_selection(0, count);
            }
            TextEditorInput::Cut if editable => {
                self.buffer.cut_selection();
            }
            TextEditorInput::SetCaret { caret, extend } => {
                let _ = self.buffer.set_caret(caret, extend);
            }
            TextEditorInput::PointerDown {
                position,
                extend,
                clicks,
            } => {
                // Clicking commits marked text, as AppKit does.
                if let Some(composition) = self.composition.take() {
                    let _ = self
                        .buffer
                        .set_selection(composition.replacement.start, composition.replacement.end);
                    self.buffer.replace_selection(&composition.text);
                    return Ok(TextEditorChange::Edited);
                }
                let caret = layout.hit_test(position).min(self.buffer.grapheme_count());
                let unit = match clicks {
                    2 => self.word_bounds(caret),
                    n if n >= 3 => self.paragraph_bounds(caret),
                    _ => caret..caret,
                };
                if extend {
                    let anchor = self
                        .dragging
                        .as_ref()
                        .map_or(self.buffer.selection().start, |range| range.start);
                    let _ = self.buffer.set_caret(caret, true);
                    self.dragging = Some(anchor..anchor);
                } else {
                    let _ = self.buffer.set_selection(unit.start, unit.end);
                    self.dragging = Some(unit);
                }
            }
            TextEditorInput::PointerDrag { position } => {
                if let Some(unit) = self.dragging.clone() {
                    let caret = layout.hit_test(position).min(self.buffer.grapheme_count());
                    let (anchor, focus) = if caret < unit.start {
                        (unit.end, caret)
                    } else {
                        (unit.start, caret.max(unit.end))
                    };
                    let _ = self.buffer.set_selection(anchor, focus);
                }
            }
            TextEditorInput::PointerUp => self.dragging = None,
            TextEditorInput::Key {
                event: KeyEvent::Down { key, .. },
                extend,
                word,
            } => match key {
                Key::Enter if editable => self.buffer.replace_selection("\n"),
                Key::Tab if editable && self.tab_inserts => self.buffer.replace_selection("\t"),
                Key::Backspace if editable => self.buffer.delete_backward(),
                Key::Delete if editable => self.buffer.delete_forward(),
                Key::ArrowLeft => {
                    self.buffer.move_caret(
                        if word {
                            TextMotion::WordLeft
                        } else {
                            TextMotion::Left
                        },
                        extend,
                    );
                }
                Key::ArrowRight => {
                    self.buffer.move_caret(
                        if word {
                            TextMotion::WordRight
                        } else {
                            TextMotion::Right
                        },
                        extend,
                    );
                }
                Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown => {
                    let caret = self.buffer.caret();
                    let line_height = layout
                        .lines()
                        .first()
                        .map_or(16.0, |line| line.height().max(1.0));
                    let (direction, steps) = match key {
                        Key::ArrowUp => (-1, 1),
                        Key::ArrowDown => (1, 1),
                        Key::PageUp => (-1, (self.page_height / line_height).max(1.0) as usize),
                        _ => (1, (self.page_height / line_height).max(1.0) as usize),
                    };
                    let mut target = caret;
                    let mut preferred = self.preferred_x;
                    for _ in 0..steps {
                        let Some((next, x)) = layout.move_vertical(target, direction, preferred)
                        else {
                            break;
                        };
                        preferred = Some(x);
                        if next == target {
                            break;
                        }
                        target = next;
                    }
                    let _ = self.buffer.set_caret(target, extend);
                    self.preferred_x = preferred;
                    keep_preferred_x = true;
                }
                Key::Home | Key::End => {
                    let caret = self.buffer.caret();
                    let target = if word {
                        if key == Key::Home {
                            0
                        } else {
                            self.buffer.grapheme_count()
                        }
                    } else {
                        let line = self.line_bounds(layout, caret).unwrap_or(0..0);
                        if key == Key::Home {
                            line.start
                        } else {
                            line.end
                        }
                    };
                    let _ = self.buffer.set_caret(target, extend);
                }
                _ => {}
            },
            _ => {}
        }
        if !keep_preferred_x {
            self.preferred_x = None;
        }
        let change = if self.buffer.as_str() != before_value {
            TextEditorChange::Edited
        } else if self.composition != before_composition {
            TextEditorChange::Composition
        } else if self.buffer.selection() != before_selection {
            TextEditorChange::Selection
        } else {
            TextEditorChange::None
        };
        if change != TextEditorChange::None {
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
            if change == TextEditorChange::Edited {
                tree.invalidate(id, Dirty::LAYOUT)?;
            }
        }
        Ok(change)
    }

    pub fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::TextField,
            name: self.name.clone(),
            description: String::new(),
            value: Some(self.buffer.as_str().to_owned()),
            state: SemanticState {
                disabled: !self.enabled,
                read_only: self.read_only,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: if !self.enabled {
                Vec::new()
            } else if self.read_only {
                vec![SemanticAction::Focus]
            } else {
                vec![SemanticAction::SetValue, SemanticAction::Focus]
            },
            text: Some(SemanticText {
                character_count_utf16: self.buffer.as_str().encode_utf16().count(),
                selection_utf16: self.selection_utf16(),
                multiline: true,
            }),
        }
    }
}
