//! Multiline text editing over [`TextBuffer`] and a shaped [`TextLayout`].
//! The application shapes [`TextEditor::layout_text`] with its platform
//! shaper, passes the layout to [`TextEditor::handle`], and reveals
//! [`TextEditor::caret_rect`] in its scroll area after each change.

use super::text_field::TextComposition;
use crate::ShortcutModifiers;
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::editing::{CaretMotion, EditingKey, TextEdit, editing_key, is_typing_key, text_edit};
use crate::form::input::default_width;
use crate::form::measured::measured_layout;
use crate::style::fill_area;
use crate::style::{
    ControlKind, EstimatedText, PaintContext, Slot, StyleQuery, Styles, TextAlign, TextMeasure,
    VisualState, content_rect,
};
use crate::{OverlayScrollBar, ScrollOrientation};
use choscorui_core::{
    CompositionEvent, DesignTokens, Dirty, EventPhase, EventReply, KeyConvention, KeyEvent,
    Measured, NodeId, Point, PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode,
    SemanticRole, SemanticState, SemanticText, Size, TextBuffer, TextInputInfo, TextLayout,
    TextMotion, Tree, TreeError, TypeSpec,
};
use std::cell::RefCell;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub enum TextEditorInput {
    Text(String),
    /// Text from the clipboard in place of the selection.
    Paste(String),
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
    /// A key with the held modifiers, through the shared editing key
    /// map of [`TextEditor::set_key_convention`]. On `Mac`, Option moves
    /// and deletes by word, and Command goes to the line start or end
    /// (Left, Right, Backspace, Delete) or to the document start or end
    /// (Up, Down). On `Pc`, Ctrl moves and deletes by word, Home and End
    /// go to the line start or end, and Ctrl with Home or End goes to the
    /// document start or end. Shift extends the selection.
    Key {
        event: KeyEvent,
        modifiers: ShortcutModifiers,
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
    /// True while a hosted press is held.
    pressed: bool,
    host: HostedText,
    /// The editing key map. A control host sets it with each event.
    key_convention: KeyConvention,
}

/// What the editor keeps in a control host, where it lays out its own
/// text: the text width and type, the content rectangle, the direction,
/// the layout of the last paint, and the scroll bar that keeps the scroll
/// offset.
struct HostedText {
    width: f64,
    spec: Option<TypeSpec>,
    inner: Rect,
    rtl: bool,
    layout: RefCell<Option<TextLayout>>,
    bar: OverlayScrollBar,
}

impl Default for HostedText {
    fn default() -> Self {
        Self {
            width: 0.0,
            spec: None,
            inner: Rect::default(),
            rtl: false,
            layout: RefCell::new(None),
            bar: OverlayScrollBar::new(ScrollOrientation::Vertical),
        }
    }
}

/// The height of the lines of `layout`, in `spec` lines.
pub(crate) fn lines_height(layout: &TextLayout, spec: &TypeSpec) -> f64 {
    layout.lines().len().max(1) as f64 * spec.line_height
}

/// Scrolls `bar` to show the caret of `editor` in `layout`, and repaints
/// when the offset changed.
pub(crate) fn reveal_caret(
    cx: &mut ControlContext<'_>,
    bar: &mut OverlayScrollBar,
    editor: &TextEditor,
    (layout, spec): (&TextLayout, &TypeSpec),
) {
    bar.set_content(lines_height(layout, spec));
    if editor
        .caret_rect(layout)
        .is_some_and(|caret| bar.reveal(caret.min_y(), caret.max_y()))
    {
        cx.request_paint();
    }
}

/// The scroll input of hosted text: the wheel scrolls `bar`, and the bar
/// takes the pointer before the text under it while no text press is
/// held. Returns the reply when the scroll used the event.
pub(crate) fn scroll_event(
    cx: &mut ControlContext<'_>,
    bar: &mut OverlayScrollBar,
    (event, pressed): (&ControlEvent, bool),
    line: f64,
) -> Option<EventReply> {
    match event {
        ControlEvent::Pointer(PointerEvent::Wheel { delta, unit, .. }) => {
            Some(bar.wheel(cx, *delta, *unit, line))
        }
        ControlEvent::Pointer(pointer) if !pressed => {
            bar.hosted_pointer(cx, pointer).map(|_| EventReply::Stop)
        }
        ControlEvent::Hover(false) | ControlEvent::Dismiss => {
            if bar.release() {
                cx.request_paint();
            }
            None
        }
        _ => None,
    }
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
            pressed: false,
            host: HostedText::default(),
            key_convention: KeyConvention::Mac,
        }
    }

    /// Sets the editing key map of [`TextEditorInput::Key`]. A new editor
    /// uses [`KeyConvention::Mac`]. In a control host, the editor takes
    /// the conventions of the host with each event.
    pub fn set_key_convention(&mut self, key_convention: KeyConvention) {
        self.key_convention = key_convention;
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
            self.pressed = false;
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Replaces the whole value and drops marked text. The same value
    /// keeps the selection.
    fn replace_value(&mut self, text: &str) {
        self.composition = None;
        if text != self.buffer.as_str() {
            let count = self.buffer.grapheme_count();
            let _ = self.buffer.set_selection(0, count);
            self.buffer.replace_selection(text);
        }
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
            let rect = rect.translate(origin.x, origin.y);
            fill_area(cx.list, rect, cx.rtl, &selection.background);
        }
        let _ = layout.draw(cx.list, origin, root.foreground);
        if host.focused()
            && let Some(rect) = self.caret_rect(layout)
        {
            let mut rect = rect.translate(origin.x, origin.y);
            rect.size.width = caret.min_width;
            fill_area(cx.list, rect, cx.rtl, &caret.background);
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

    /// The caret position that `motion` reaches in `layout`, for the
    /// motions that do not move by grapheme or word.
    fn line_target(&self, layout: &TextLayout, motion: CaretMotion) -> usize {
        let caret = self.buffer.caret();
        match motion {
            CaretMotion::DocumentStart => 0,
            CaretMotion::DocumentEnd => self.buffer.grapheme_count(),
            CaretMotion::LineStart => self.line_bounds(layout, caret).map_or(0, |line| line.start),
            CaretMotion::LineEnd => self
                .line_bounds(layout, caret)
                .map_or(self.buffer.grapheme_count(), |line| line.end),
            _ => caret,
        }
    }

    /// Moves the caret up or down by lines or pages in `layout`, and keeps
    /// the preferred x for the next vertical move.
    fn move_vertical(&mut self, layout: &TextLayout, motion: CaretMotion, extend: bool) {
        let caret = self.buffer.caret();
        let line_height = layout
            .lines()
            .first()
            .map_or(16.0, |line| line.height().max(1.0));
        let page = (self.page_height / line_height).max(1.0) as usize;
        let (direction, steps) = match motion {
            CaretMotion::Up => (-1, 1),
            CaretMotion::Down => (1, 1),
            CaretMotion::PageUp => (-1, page),
            _ => (1, page),
        };
        let mut target = caret;
        let mut preferred = self.preferred_x;
        for _ in 0..steps {
            let Some((next, x)) = layout.move_vertical(target, direction, preferred) else {
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
    }

    /// Applies an editing key in `layout`. Returns true when the move
    /// keeps the preferred x of vertical motion.
    fn editing(&mut self, layout: &TextLayout, key: EditingKey, editable: bool) -> bool {
        let word = |motion| match motion {
            CaretMotion::Left => Some(TextMotion::Left),
            CaretMotion::Right => Some(TextMotion::Right),
            CaretMotion::WordLeft => Some(TextMotion::WordLeft),
            CaretMotion::WordRight => Some(TextMotion::WordRight),
            _ => None,
        };
        match key {
            EditingKey::Enter if editable => self.buffer.replace_selection("\n"),
            EditingKey::Tab if editable && self.tab_inserts => {
                self.buffer.replace_selection("\t");
            }
            EditingKey::Delete(motion) if editable => {
                let target = match word(motion) {
                    Some(motion) => self.buffer.motion_target(motion),
                    None => self.line_target(layout, motion),
                };
                let _ = self.buffer.delete_to(target);
            }
            EditingKey::Move {
                motion:
                    motion @ (CaretMotion::Up
                    | CaretMotion::Down
                    | CaretMotion::PageUp
                    | CaretMotion::PageDown),
                extend,
            } => {
                self.move_vertical(layout, motion, extend);
                return true;
            }
            EditingKey::Move { motion, extend } => match word(motion) {
                Some(motion) => {
                    self.buffer.move_caret(motion, extend);
                }
                None => {
                    let target = self.line_target(layout, motion);
                    let _ = self.buffer.set_caret(target, extend);
                }
            },
            _ => {}
        }
        false
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
        // A blur ends a composition after the focus moved.
        let external = matches!(
            input,
            TextEditorInput::SetValue(_)
                | TextEditorInput::UnmarkMarkedText
                | TextEditorInput::CancelMarkedText
        );
        if !self.enabled || (!external && tree.focus() != Some(id)) {
            return Ok(TextEditorChange::None);
        }
        let before_value = self.buffer.as_str().to_owned();
        let before_selection = self.buffer.selection();
        let before_composition = self.composition.clone();
        let editable = !self.read_only;
        let mut keep_preferred_x = false;
        match input {
            TextEditorInput::Text(text) | TextEditorInput::Paste(text) if editable => {
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
            TextEditorInput::SetValue(text) => self.replace_value(&text),
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
                    2 => self.buffer.word_range(caret),
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
            // The input method owns the keys while it composes.
            TextEditorInput::Key {
                event: KeyEvent::Down { key, .. },
                modifiers,
            } if self.composition.is_none() => {
                if let Some(key) = editing_key(key, modifiers, self.key_convention) {
                    keep_preferred_x = self.editing(layout, key, editable);
                }
            }
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

impl TextEditor {
    fn query(slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::TextEditor, slot).state(state)
    }

    /// The layout of the text in the hosted width, with `measure`.
    fn hosted_layout(&self, measure: &dyn TextMeasure) -> Option<TextLayout> {
        let spec = self.host.spec?;
        Some(measured_layout(
            &self.layout_text(),
            &spec,
            self.host.width.max(1.0),
            measure,
        ))
    }

    /// The layout of the last paint, or an estimate when the text changed
    /// after it.
    fn current_layout(&self) -> Option<TextLayout> {
        let text = self.layout_text();
        let painted = self.host.layout.borrow().clone();
        painted
            .filter(|layout| layout.text() == text)
            .or_else(|| self.hosted_layout(&EstimatedText))
    }

    /// Applies `input` in a host and reports an edit.
    fn apply(&mut self, cx: &mut ControlContext<'_>, input: TextEditorInput) {
        let Some(layout) = self.current_layout() else {
            return;
        };
        self.apply_in(cx, &layout, input);
    }

    /// Applies `input` with `layout` and reports an edit as
    /// `Changed(Text)`.
    fn apply_in(
        &mut self,
        cx: &mut ControlContext<'_>,
        layout: &TextLayout,
        input: TextEditorInput,
    ) {
        let node = cx.node();
        let change = self.handle(cx.tree_mut(), node, layout, input);
        if change == Ok(TextEditorChange::Edited) {
            cx.emit(ControlMessage::Changed(ControlValue::Text(
                self.value().to_string(),
            )));
        }
    }

    /// The input of an Edit command, after its clipboard part ran. A
    /// read-only editor copies and selects only.
    fn edit_input(
        &self,
        cx: &mut ControlContext<'_>,
        command: crate::EditCommand,
    ) -> Option<TextEditorInput> {
        use crate::EditCommand;
        if self.read_only && !matches!(command, EditCommand::Copy | EditCommand::SelectAll) {
            return None;
        }
        Some(
            match text_edit(cx, command, Some(self.buffer.selected_text()))? {
                TextEdit::Cut => TextEditorInput::Cut,
                TextEdit::Paste(text) => TextEditorInput::Paste(text),
                TextEdit::SelectAll => TextEditorInput::SelectAll,
                TextEdit::Undo => TextEditorInput::Undo,
                TextEdit::Redo => TextEditorInput::Redo,
            },
        )
    }

    /// The editor input for an input method event.
    fn composition_input(event: &CompositionEvent) -> TextEditorInput {
        match event.clone() {
            CompositionEvent::Mark {
                text,
                selected_utf16,
                replacement_utf16,
            } => TextEditorInput::SetMarkedText {
                text,
                selected_utf16,
                replacement_utf16,
            },
            CompositionEvent::Commit {
                text,
                replacement_utf16,
            } => TextEditorInput::CommitMarkedText {
                text,
                replacement_utf16,
            },
            CompositionEvent::Unmark => TextEditorInput::UnmarkMarkedText,
        }
    }

    /// True while a hosted press is held.
    pub(crate) fn is_pressed(&self) -> bool {
        self.pressed
    }

    /// Handles `event` in a control host for text laid out as `layout`.
    /// `to_layout` maps a window point to layout coordinates. Presses use
    /// the click count: two select a word and three a paragraph. Keys go
    /// through the shared editing key map, and Edit commands through the
    /// clipboard. The textarea uses it too.
    pub(crate) fn hosted_event(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &ControlEvent,
        layout: &TextLayout,
        to_layout: impl Fn(Point) -> Point,
    ) -> EventReply {
        let modifiers = cx.modifiers();
        self.key_convention = cx.key_convention();
        let input = match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if cx.bounds().contains(*position) => {
                cx.request_focus();
                self.pressed = true;
                cx.capture_pointer(true);
                TextEditorInput::PointerDown {
                    position: to_layout(*position),
                    extend: modifiers.shift,
                    clicks: u8::try_from(cx.click_count()).unwrap_or(u8::MAX),
                }
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) if self.pressed => {
                TextEditorInput::PointerDrag {
                    position: to_layout(*position),
                }
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) if self.pressed => {
                self.pressed = false;
                cx.capture_pointer(false);
                TextEditorInput::PointerUp
            }
            ControlEvent::Key(event @ KeyEvent::Down { key, .. }) if cx.is_focused() => {
                match editing_key(*key, modifiers, self.key_convention) {
                    Some(EditingKey::Tab) if !self.tab_inserts => return EventReply::Continue,
                    Some(_) => TextEditorInput::Key {
                        event: *event,
                        modifiers,
                    },
                    // The typed text arrives as `Text`.
                    None if is_typing_key(*key, modifiers) => return EventReply::Stop,
                    None => return EventReply::Continue,
                }
            }
            ControlEvent::Text(text) if cx.is_focused() => TextEditorInput::Text(text.clone()),
            ControlEvent::Compose(composition) if cx.is_focused() => {
                Self::composition_input(composition)
            }
            ControlEvent::Edit(command) if cx.is_focused() => match self.edit_input(cx, *command) {
                Some(input) => input,
                None => return EventReply::Stop,
            },
            ControlEvent::FocusChanged(focused) => {
                if !focused && self.composition.is_some() {
                    self.apply_in(cx, layout, TextEditorInput::UnmarkMarkedText);
                }
                if self.pressed {
                    self.pressed = false;
                    cx.capture_pointer(false);
                }
                cx.request_paint();
                return EventReply::Continue;
            }
            _ => return EventReply::Continue,
        };
        self.apply_in(cx, layout, input);
        cx.request_paint();
        cx.request_semantics();
        EventReply::Stop
    }

    /// The input method state for `layout` placed by `to_window`, which
    /// maps a layout rectangle to window coordinates.
    pub(crate) fn hosted_text_input(
        &self,
        layout: &TextLayout,
        to_window: impl Fn(Rect) -> Rect,
    ) -> Option<TextInputInfo> {
        if !self.enabled || self.read_only {
            return None;
        }
        Some(TextInputInfo {
            selected_utf16: self.selection_utf16(),
            marked_utf16: self.marked_range_utf16(),
            caret: to_window(self.caret_rect(layout)?),
        })
    }
}

impl CustomControl for TextEditor {
    /// Grows with the lines of the text, in the default input width, up to
    /// the offered height. Then the text scrolls.
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(Self::query(Slot::Root, VisualState::NONE));
        let spec = cx.style(Self::query(Slot::Text, VisualState::NONE)).text;
        let width = default_width(cx, &root);
        self.host.width = (width - root.padding.horizontal()).max(1.0);
        self.host.spec = Some(spec);
        let lines = self
            .hosted_layout(cx.text)
            .map_or(1, |layout| layout.lines().len().max(1));
        let height = (lines as f64 * spec.line_height + root.padding.vertical())
            .min(cx.constraints.max.height);
        Measured::new(Size::new(width, height.max(root.min_height)))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let root = cx.style(Self::query(Slot::Root, VisualState::NONE));
        let inner = content_rect(bounds, &root, cx.rtl());
        self.host.inner = inner;
        self.host.rtl = cx.rtl();
        self.host.width = inner.size.width.max(1.0);
        let spec = cx.style(Self::query(Slot::Text, VisualState::NONE)).text;
        self.host.spec = Some(spec);
        self.set_page_height(inner.size.height);
        self.host.bar.arrange(cx, inner);
        if let Some(layout) = self.hosted_layout(cx.text) {
            self.host.bar.set_content(lines_height(&layout, &spec));
        }
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let (Some(layout), Some(spec)) = (self.current_layout(), self.host.spec) else {
            return EventReply::Continue;
        };
        let mut bar = self.host.bar.clone();
        let scrolled = scroll_event(cx, &mut bar, (event, self.pressed), spec.line_height);
        self.host.bar = bar;
        if let Some(reply) = scrolled {
            return reply;
        }
        let (inner, rtl, scroll) = (self.host.inner, self.host.rtl, self.host.bar.offset());
        let reply = self.hosted_event(cx, event, &layout, |point| {
            layout_point(inner, rtl, Point::new(point.x, point.y + scroll))
        });
        if reply == EventReply::Stop
            && let Some(layout) = self.current_layout()
        {
            let mut bar = self.host.bar.clone();
            reveal_caret(cx, &mut bar, self, (&layout, &spec));
            self.host.bar = bar;
        }
        reply
    }

    /// Paints the box, then the selection, the lines, and the caret while
    /// focused, scrolled by the offset, and the scroll bar above them.
    /// Right-to-left layouts mirror the lines in the box.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx.state.set(VisualState::DISABLED, !self.enabled);
        let root = cx.style(Self::query(Slot::Root, state));
        let text = cx.style(Self::query(Slot::Text, state));
        let selection = cx.style(Self::query(Slot::Range, state)).background;
        let caret = cx.style(Self::query(Slot::Indicator, state));
        let layout = self.hosted_layout(cx.text);
        let bounds = cx.bounds;
        cx.slot(bounds, &root, |cx, inner| {
            let Some(layout) = &layout else {
                return;
            };
            let rtl = cx.rtl;
            let scroll = self.host.bar.offset();
            let place = |rect: Rect| {
                let rect = rect.translate(inner.origin.x, inner.origin.y - scroll);
                if rtl {
                    let x = inner.origin.x + inner.max_x() - rect.max_x();
                    Rect::from_xywh(x, rect.origin.y, rect.size.width, rect.size.height)
                } else {
                    rect
                }
            };
            cx.list.push_clip(inner);
            // Without focus, the selection stays in the inactive color.
            for rect in self.selection_rects(layout) {
                fill_area(cx.list, place(rect), cx.rtl, &selection);
            }
            let source = layout.text();
            for line in layout.lines() {
                let top = line.baseline - text.text.baseline();
                let width = line.width.max(1.0);
                let rect = place(Rect::from_xywh(
                    line.offset_x,
                    top,
                    width,
                    text.text.line_height,
                ));
                cx.label(rect, &source[line.bytes.clone()], &text, TextAlign::Start);
            }
            if state.focused()
                && let Some(rect) = self.caret_rect(layout)
            {
                let mut rect = place(rect);
                rect.size.width = caret.min_width;
                fill_area(cx.list, rect, cx.rtl, &caret.background);
            }
            let _ = cx.list.pop_clip();
            self.host.bar.paint(cx);
        });
        *self.host.layout.borrow_mut() = layout;
    }

    fn semantics(&self) -> SemanticNode {
        TextEditor::semantics(self)
    }

    fn text_input(&self) -> Option<TextInputInfo> {
        let layout = self.host.layout.borrow();
        let (inner, rtl, scroll) = (self.host.inner, self.host.rtl, self.host.bar.offset());
        self.hosted_text_input(layout.as_ref()?, |rect| {
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
            SemanticAction::SetValue if self.enabled && !self.read_only => {
                let text = value.unwrap_or_default().to_string();
                self.apply(cx, TextEditorInput::SetValue(text));
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
        self.set_enabled_state(enabled);
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Text(text) = value else {
            return false;
        };
        self.replace_value(text);
        true
    }
}

/// `point` in the layout coordinates of text in `inner`. Right-to-left
/// layouts mirror.
pub(crate) fn layout_point(inner: Rect, rtl: bool, point: Point) -> Point {
    let x = if rtl {
        inner.max_x() - point.x
    } else {
        point.x - inner.origin.x
    };
    Point::new(x, point.y - inner.origin.y)
}

/// The window rectangle of `rect` in the layout coordinates of text in
/// `inner`. Right-to-left layouts mirror.
pub(crate) fn window_rect(inner: Rect, rtl: bool, rect: Rect) -> Rect {
    let rect = rect.translate(inner.origin.x, inner.origin.y);
    if rtl {
        let x = inner.origin.x + inner.max_x() - rect.max_x();
        Rect::from_xywh(x, rect.origin.y, rect.size.width, rect.size.height)
    } else {
        rect
    }
}
