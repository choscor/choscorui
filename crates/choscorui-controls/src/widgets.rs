//! Remaining basic widgets: standalone scroll bar, decimal spin box, dial,
//! tool and command-link buttons, button groups, toolbox (accordion),
//! numeric (LCD) display, focus frame, size grip, and key-sequence capture.
//! Each documents its value, keyboard, and accessibility contract.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted;
use crate::range_controls::paint_stepper_field;
use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, Styles, TextAlign, VisualState, content_rect,
    fill_shape, paint_in,
};
use crate::{IntegerRange, NumberInput, STEPPER_WIDTH, ValidationState};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayList, EventPhase, EventReply, FillRule, Icon, Key, KeyEvent,
    Locale, Measured, Paint, Path, Point, PointerButton, PointerEvent, RadiusScale, Rect,
    SemanticAction, SemanticNode, SemanticRole, SemanticState, Size, Tree, grapheme_boundaries,
};
use std::f64::consts::PI;

mod button_group;
mod key_sequence;
mod tool_buttons;
pub use button_group::{ButtonGroup, ButtonGroupMember, HostedButtonGroup};
pub use key_sequence::{KeySequenceChange, KeySequenceEdit, shortcut_text};
pub use tool_buttons::{
    CommandLink, HostedCommandLink, HostedToolButton, ToolButton, ToolButtonAction, ToolButtonMenu,
};

// ---------------------------------------------------------------- ScrollBar

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollBarInput {
    /// Press at a position along the bar (points from the track start).
    Press {
        at: f64,
    },
    Drag {
        at: f64,
    },
    Release,
    Key(Key),
    Semantic(SemanticAction),
}

/// Standalone scroll bar over `content` with a `viewport`-sized window.
/// Value is the leading offset in `0..=content - viewport`. Pressing the
/// track pages toward the press; dragging the thumb tracks the pointer;
/// arrows step by `line_step`, Page keys by the viewport, Home/End to the
/// ends. Exposed as a slider-like range with increment/decrement actions.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollBar {
    name: String,
    orientation: ScrollOrientation,
    content: f64,
    viewport: f64,
    value: f64,
    line_step: f64,
    track_length: f64,
    drag: Option<(f64, f64)>,
}

impl ScrollBar {
    pub fn new(name: impl Into<String>, orientation: ScrollOrientation) -> Self {
        Self {
            name: name.into(),
            orientation,
            content: 0.0,
            viewport: 0.0,
            value: 0.0,
            line_step: 16.0,
            track_length: 0.0,
            drag: None,
        }
    }
    pub fn orientation(&self) -> ScrollOrientation {
        self.orientation
    }
    pub fn value(&self) -> f64 {
        self.value
    }
    pub fn maximum(&self) -> f64 {
        (self.content - self.viewport).max(0.0)
    }
    pub fn set_line_step(&mut self, step: f64) {
        if step.is_finite() && step > 0.0 {
            self.line_step = step;
        }
    }
    /// Updates sizes; the value is clamped into the new range.
    pub fn set_range(&mut self, content: f64, viewport: f64, track_length: f64) {
        self.content = content.max(0.0);
        self.viewport = viewport.max(0.0);
        self.track_length = track_length.max(0.0);
        self.value = self.value.clamp(0.0, self.maximum());
    }
    pub fn set_value(&mut self, value: f64) -> bool {
        let value = if value.is_finite() {
            value.clamp(0.0, self.maximum())
        } else {
            self.value
        };
        let changed = value != self.value;
        self.value = value;
        changed
    }
    /// Thumb start and length along the track.
    pub fn thumb(&self) -> (f64, f64) {
        if self.content <= 0.0 || self.viewport >= self.content {
            return (0.0, self.track_length);
        }
        let length = (self.track_length * self.viewport / self.content)
            .max(16.0)
            .min(self.track_length);
        let travel = self.track_length - length;
        let start = if self.maximum() > 0.0 {
            travel * self.value / self.maximum()
        } else {
            0.0
        };
        (start, length)
    }
    pub fn is_needed(&self) -> bool {
        self.content > self.viewport
    }

    /// Returns true when the value changed.
    pub fn handle(&mut self, input: ScrollBarInput) -> bool {
        match input {
            ScrollBarInput::Press { at } => {
                let (start, length) = self.thumb();
                if at >= start && at <= start + length {
                    self.drag = Some((at, self.value));
                    false
                } else if at < start {
                    self.set_value(self.value - self.viewport)
                } else {
                    self.set_value(self.value + self.viewport)
                }
            }
            ScrollBarInput::Drag { at } => {
                let Some((origin, value)) = self.drag else {
                    return false;
                };
                let (_, length) = self.thumb();
                let travel = (self.track_length - length).max(1.0);
                self.set_value(value + (at - origin) * self.maximum() / travel)
            }
            ScrollBarInput::Release => {
                self.drag = None;
                false
            }
            ScrollBarInput::Key(key) => {
                let (back, forward) = match self.orientation {
                    ScrollOrientation::Vertical => (Key::ArrowUp, Key::ArrowDown),
                    ScrollOrientation::Horizontal => (Key::ArrowLeft, Key::ArrowRight),
                };
                match key {
                    key if key == back => self.set_value(self.value - self.line_step),
                    key if key == forward => self.set_value(self.value + self.line_step),
                    Key::PageUp => self.set_value(self.value - self.viewport),
                    Key::PageDown => self.set_value(self.value + self.viewport),
                    Key::Home => self.set_value(0.0),
                    Key::End => self.set_value(self.maximum()),
                    _ => false,
                }
            }
            ScrollBarInput::Semantic(action) => match action {
                SemanticAction::Increment => self.set_value(self.value + self.viewport),
                SemanticAction::Decrement => self.set_value(self.value - self.viewport),
                _ => false,
            },
        }
    }

    pub fn semantics(&self) -> SemanticNode {
        let percent = if self.maximum() > 0.0 {
            (self.value / self.maximum() * 100.0).round()
        } else {
            0.0
        };
        SemanticNode {
            role: SemanticRole::ScrollBar,
            name: self.name.clone(),
            description: String::new(),
            value: Some(format!("{percent}%")),
            state: SemanticState {
                hidden: !self.is_needed(),
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: vec![SemanticAction::Increment, SemanticAction::Decrement],
            text: None,
        }
    }

    /// The flags the scroll area recipe reads: pressed while dragging.
    pub fn visual_state(&self) -> VisualState {
        VisualState::NONE.set(VisualState::PRESSED, self.drag.is_some())
    }

    /// Paints the thumb in the context's bounds with the `ScrollArea`
    /// scrollbar look. A bar that is not needed paints nothing.
    pub fn paint(&self, cx: &mut PaintContext<'_>) {
        self.paint_state(cx, self.visual_state());
    }

    /// Paints the thumb with the flags in `state`.
    fn paint_state(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        if !self.is_needed() {
            return;
        }
        let bounds = cx.bounds;
        let query = |slot| StyleQuery::new(ControlKind::ScrollArea, slot).state(state);
        let (bar, thumb) = (
            cx.style(query(Slot::Scrollbar)),
            cx.style(query(Slot::Thumb)),
        );
        let (start, length) = self.thumb();
        let rect = match self.orientation {
            ScrollOrientation::Vertical => Rect::from_xywh(
                bounds.origin.x,
                bounds.origin.y + start,
                bounds.size.width,
                length,
            ),
            ScrollOrientation::Horizontal => Rect::from_xywh(
                bounds.origin.x + start,
                bounds.origin.y,
                length,
                bounds.size.height,
            ),
        };
        let rect = content_rect(rect, &bar, cx.rtl);
        fill_shape(cx.list, rect, &thumb.radius, cx.rtl, &thumb.background);
    }

    /// The offset of `point` along the bar from the start of `bounds`.
    fn along(&self, bounds: Rect, point: Point) -> f64 {
        match self.orientation {
            ScrollOrientation::Vertical => point.y - bounds.origin.y,
            ScrollOrientation::Horizontal => point.x - bounds.origin.x,
        }
    }

    /// Reports a value change in a host.
    fn report(&self, cx: &mut ControlContext<'_>, changed: bool) {
        if changed {
            cx.emit(ControlMessage::Changed(ControlValue::Number(self.value)));
            cx.request_semantics();
        }
        cx.request_paint();
    }
}

/// The bar has no focus, as without a host. It captures the pointer
/// during a thumb drag.
impl CustomControl for ScrollBar {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let bar = cx.style(StyleQuery::new(ControlKind::ScrollArea, Slot::Scrollbar));
        let max = cx.constraints.max;
        let length = |max: f64| {
            if max.is_finite() {
                max
            } else {
                self.track_length
            }
        };
        Measured::new(match self.orientation {
            ScrollOrientation::Vertical => Size::new(bar.min_width, length(max.height)),
            ScrollOrientation::Horizontal => Size::new(length(max.width), bar.min_height),
        })
    }

    fn arrange(&mut self, _cx: &MeasureContext<'_>, bounds: Rect) {
        let length = match self.orientation {
            ScrollOrientation::Vertical => bounds.size.height,
            ScrollOrientation::Horizontal => bounds.size.width,
        };
        self.set_range(self.content, self.viewport, length);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let bounds = cx.bounds();
        let input = match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if bounds.contains(*position) => ScrollBarInput::Press {
                at: self.along(bounds, *position),
            },
            ControlEvent::Pointer(PointerEvent::Move { position }) if self.drag.is_some() => {
                ScrollBarInput::Drag {
                    at: self.along(bounds, *position),
                }
            }
            ControlEvent::Pointer(PointerEvent::Up { .. })
            | ControlEvent::FocusChanged(false)
            | ControlEvent::Dismiss
                if self.drag.is_some() =>
            {
                ScrollBarInput::Release
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                let changed = self.handle(ScrollBarInput::Key(*key));
                self.report(cx, changed);
                return if changed {
                    EventReply::Stop
                } else {
                    EventReply::Continue
                };
            }
            _ => return EventReply::Continue,
        };
        let changed = self.handle(input);
        cx.capture_pointer(self.drag.is_some());
        self.report(cx, changed);
        EventReply::Stop
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx.state.with(self.visual_state());
        self.paint_state(cx, state);
    }

    fn semantics(&self) -> SemanticNode {
        ScrollBar::semantics(self)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if !matches!(
            action,
            SemanticAction::Increment | SemanticAction::Decrement
        ) {
            return false;
        }
        let changed = self.handle(ScrollBarInput::Semantic(action));
        self.report(cx, changed);
        true
    }

    fn focusable(&self) -> bool {
        false
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.drag = None;
        }
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Number(value) if value.is_finite() => {
                ScrollBar::set_value(self, *value);
                true
            }
            _ => false,
        }
    }
}

// ----------------------------------------------------------- DecimalSpinBox

/// Locale-aware decimal spin box: text editing with validation, arrows
/// step, Page keys step ten times, and Return commits only a valid value.
#[derive(Clone, Debug)]
pub struct DecimalSpinBox {
    name: String,
    input: NumberInput,
    minimum: f64,
    maximum: f64,
    step: f64,
    value: f64,
    text: String,
    fraction_digits: usize,
    /// In a control host: the direction, and true while typed text
    /// replaces the text instead of appending to it.
    rtl: bool,
    fresh: bool,
}

impl DecimalSpinBox {
    pub fn new(
        name: impl Into<String>,
        locale: Locale,
        (minimum, maximum): (f64, f64),
        step: f64,
        fraction_digits: usize,
        value: f64,
    ) -> Self {
        let input =
            NumberInput::new(locale, fraction_digits).with_range(Some(minimum), Some(maximum));
        let value = value.clamp(minimum, maximum);
        let text = input.format(value);
        Self {
            name: name.into(),
            input,
            minimum,
            maximum,
            step,
            value,
            text,
            fraction_digits,
            rtl: false,
            fresh: true,
        }
    }
    pub fn value(&self) -> f64 {
        self.value
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn validation(&self) -> ValidationState {
        self.input.validate(&self.text).0
    }
    /// Replaces the edited text (validation is reported, value unchanged).
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }
    fn round(&self, value: f64) -> f64 {
        let scale = 10f64.powi(self.fraction_digits as i32);
        (value * scale).round() / scale
    }
    fn set_value(&mut self, value: f64) -> bool {
        let value = self.round(value.clamp(self.minimum, self.maximum));
        let changed = value != self.value;
        self.value = value;
        self.text = self.input.format(value);
        changed
    }
    /// Returns true when the value changed.
    pub fn handle_key(&mut self, key: Key) -> bool {
        match key {
            Key::ArrowUp => self.set_value(self.value + self.step),
            Key::ArrowDown => self.set_value(self.value - self.step),
            Key::PageUp => self.set_value(self.value + self.step * 10.0),
            Key::PageDown => self.set_value(self.value - self.step * 10.0),
            Key::Home => self.set_value(self.minimum),
            Key::End => self.set_value(self.maximum),
            Key::Enter => self.commit(),
            Key::Escape => {
                self.text = self.input.format(self.value);
                false
            }
            _ => false,
        }
    }
    /// Commits valid text; invalid text stays for correction.
    pub fn commit(&mut self) -> bool {
        match self.input.validate(&self.text) {
            (ValidationState::Valid, Some(value)) => self.set_value(value),
            _ => false,
        }
    }
    /// Re-formats for a new locale without changing the value.
    pub fn set_locale(&mut self, locale: Locale) {
        self.input = NumberInput::new(locale, self.fraction_digits)
            .with_range(Some(self.minimum), Some(self.maximum));
        self.text = self.input.format(self.value);
    }
    pub fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::SpinButton,
            name: self.name.clone(),
            description: match self.validation() {
                ValidationState::Invalid(message) => message,
                _ => String::new(),
            },
            value: Some(self.text.clone()),
            state: SemanticState {
                invalid: matches!(self.validation(), ValidationState::Invalid(_)),
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::SetValue,
                SemanticAction::Focus,
            ],
            text: None,
        }
    }
    pub fn handle_semantic(&mut self, action: SemanticAction, value: Option<&str>) -> bool {
        match action {
            SemanticAction::Increment => self.handle_key(Key::ArrowUp),
            SemanticAction::Decrement => self.handle_key(Key::ArrowDown),
            SemanticAction::SetValue => {
                if let Some(value) = value {
                    self.text = value.to_owned();
                }
                self.commit()
            }
            _ => false,
        }
    }

    /// The flags the input recipe reads: the host's focus flags in
    /// `host`, and invalid while the text does not parse.
    pub fn visual_state(&self, host: VisualState) -> VisualState {
        host.set(
            VisualState::INVALID,
            matches!(self.validation(), ValidationState::Invalid(_)),
        )
    }

    /// The stepper column at the end of `bounds`, in reading order.
    pub fn stepper_rect(bounds: Rect, rtl: bool) -> Rect {
        let width = STEPPER_WIDTH.min(bounds.size.width);
        let x = if rtl {
            bounds.origin.x
        } else {
            bounds.max_x() - width
        };
        Rect::from_xywh(x, bounds.origin.y, width, bounds.size.height)
    }

    /// Paints the input box, the edited text, and the steppers, like
    /// [`crate::SpinBox::paint`].
    pub fn paint(&self, cx: &mut PaintContext<'_>, host: VisualState) {
        let steppers = Self::stepper_rect(cx.bounds, cx.rtl);
        paint_stepper_field(cx, &self.text, self.visual_state(host), steppers);
    }

    /// Replaces the edited text in a host and repaints.
    fn edit(&mut self, cx: &mut ControlContext<'_>, text: String) {
        self.text = text;
        self.fresh = false;
        cx.request_paint();
        cx.request_semantics();
    }

    /// Reports a value change in a host. The next typed text replaces
    /// the formatted value.
    fn report(&mut self, cx: &mut ControlContext<'_>, changed: bool) {
        if changed {
            cx.emit(ControlMessage::Changed(ControlValue::Number(self.value)));
            self.fresh = true;
        }
        cx.request_paint();
        cx.request_semantics();
    }

    /// Handles a key-down in a host.
    fn hosted_key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        if key == Key::Backspace {
            let end = if self.fresh {
                0
            } else {
                let boundaries = grapheme_boundaries(&self.text);
                boundaries.iter().rev().nth(1).copied().unwrap_or(0)
            };
            let text = self.text[..end].to_string();
            self.edit(cx, text);
            return EventReply::Stop;
        }
        let before = self.text.clone();
        let changed = self.handle_key(key);
        let handled = changed
            || self.text != before
            || matches!(
                key,
                Key::ArrowUp
                    | Key::ArrowDown
                    | Key::PageUp
                    | Key::PageDown
                    | Key::Home
                    | Key::End
                    | Key::Enter
            );
        if !handled {
            return EventReply::Continue;
        }
        self.report(cx, changed);
        EventReply::Stop
    }
}

/// Typed text replaces the text after focus or a step, and then appends.
/// Backspace deletes the last character. A loss of focus commits valid
/// text.
impl CustomControl for DecimalSpinBox {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root));
        let widest = [self.minimum, self.maximum]
            .into_iter()
            .map(|bound| cx.text.width(&self.input.format(bound), &style.text))
            .fold(0.0, f64::max);
        let width = style.padding.horizontal() + widest + STEPPER_WIDTH;
        let height = (style.text.line_height + style.padding.vertical()).max(style.min_height);
        Measured::new(Size::new(width.max(style.min_width), height))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, _bounds: Rect) {
        self.rtl = cx.rtl();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if cx.bounds().contains(*position) => {
                let steppers = Self::stepper_rect(cx.bounds(), self.rtl);
                if steppers.contains(*position) {
                    let up = position.y < steppers.center().y;
                    let changed = self.handle_key(if up { Key::ArrowUp } else { Key::ArrowDown });
                    self.report(cx, changed);
                }
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.hosted_key(cx, *key),
            ControlEvent::Text(text) => {
                let text = if self.fresh {
                    text.clone()
                } else {
                    format!("{}{text}", self.text)
                };
                self.edit(cx, text);
                EventReply::Stop
            }
            ControlEvent::FocusChanged(focused) => {
                let changed = !focused && self.commit();
                self.report(cx, changed);
                self.fresh = true;
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        DecimalSpinBox::paint(self, cx, host);
    }

    fn semantics(&self) -> SemanticNode {
        DecimalSpinBox::semantics(self)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Focus => cx.request_focus(),
            SemanticAction::Increment | SemanticAction::Decrement | SemanticAction::SetValue => {
                let changed = self.handle_semantic(action, value);
                self.report(cx, changed);
            }
            _ => return false,
        }
        true
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Number(value) if value.is_finite() => {
                DecimalSpinBox::set_value(self, *value);
                self.fresh = true;
                true
            }
            _ => false,
        }
    }
}

// --------------------------------------------------------------------- Dial

/// Rotary range control. Angle 0 points up; values increase clockwise over
/// `sweep` radians (wrapping dials use a full turn). Keyboard and
/// accessibility actions match [`crate::Slider`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dial {
    range: IntegerRange,
    wrapping: bool,
}

impl Dial {
    pub fn new(range: IntegerRange, wrapping: bool) -> Self {
        Self { range, wrapping }
    }
    pub fn value(&self) -> i32 {
        self.range.value()
    }
    fn sweep(&self) -> f64 {
        if self.wrapping { 2.0 * PI } else { 1.5 * PI }
    }
    /// Sets the value from a pointer position around `center`.
    pub fn point_to(&mut self, center: Point, point: Point) -> bool {
        let dx = point.x - center.x;
        let dy = point.y - center.y;
        if dx == 0.0 && dy == 0.0 {
            return false;
        }
        // Clockwise angle from straight up, in 0..2π.
        let mut angle = dx.atan2(-dy);
        if angle < 0.0 {
            angle += 2.0 * PI;
        }
        let sweep = self.sweep();
        let fraction = if self.wrapping {
            angle / sweep
        } else {
            // Non-wrapping dials start at -135°; the dead zone snaps to the
            // nearer end.
            let shifted = (angle + 0.75 * PI) % (2.0 * PI);
            if shifted > sweep {
                if shifted - sweep < (2.0 * PI - shifted) {
                    1.0
                } else {
                    0.0
                }
            } else {
                shifted / sweep
            }
        };
        let span = f64::from(self.range.maximum()) - f64::from(self.range.minimum());
        let value = f64::from(self.range.minimum()) + (fraction * span).round();
        let old = self.range.value();
        let value = if self.wrapping && value as i32 > self.range.maximum() {
            self.range.minimum()
        } else {
            value as i32
        };
        self.range.set_value(value) != old
    }
    pub fn handle_key(&mut self, key: Key) -> bool {
        let old = self.range.value();
        match key {
            Key::ArrowUp | Key::ArrowRight => {
                if self.wrapping && old == self.range.maximum() {
                    self.range.set_value(self.range.minimum());
                } else {
                    self.range.increment();
                }
            }
            Key::ArrowDown | Key::ArrowLeft => {
                if self.wrapping && old == self.range.minimum() {
                    self.range.set_value(self.range.maximum());
                } else {
                    self.range.decrement();
                }
            }
            Key::PageUp => {
                self.range.page_increment();
            }
            Key::PageDown => {
                self.range.page_decrement();
            }
            Key::Home => {
                self.range.set_value(self.range.minimum());
            }
            Key::End => {
                self.range.set_value(self.range.maximum());
            }
            _ => {}
        }
        self.range.value() != old
    }
    /// Angle of the indicator, clockwise from up.
    pub fn angle(&self) -> f64 {
        let span = f64::from(self.range.maximum()) - f64::from(self.range.minimum());
        let fraction = if span == 0.0 {
            0.0
        } else {
            (f64::from(self.range.value()) - f64::from(self.range.minimum())) / span
        };
        if self.wrapping {
            fraction * self.sweep()
        } else {
            fraction * self.sweep() - 0.75 * PI
        }
    }
    pub fn semantics(&self, name: &str) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Slider,
            name: name.to_owned(),
            description: "Dial".into(),
            value: Some(self.range.value().to_string()),
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::SetValue,
                SemanticAction::Focus,
            ],
            text: None,
        }
    }
    /// Paints the dial in the context's bounds with the `Dial` recipe: a
    /// track arc over the sweep, the value arc, and a thumb at the value.
    /// `state` carries hover, focus, and disabled flags.
    pub fn paint(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        let bounds = cx.bounds;
        let query = |slot| StyleQuery::new(ControlKind::Dial, slot).state(state);
        let (root, track, range, thumb) = (
            cx.style(query(Slot::Root)),
            cx.style(query(Slot::Track)),
            cx.style(query(Slot::Range)),
            cx.style(query(Slot::Thumb)),
        );
        let radius = bounds.size.width.min(bounds.size.height) / 2.0;
        let center = Point::new(
            bounds.origin.x + bounds.size.width / 2.0,
            bounds.origin.y + bounds.size.height / 2.0,
        );
        let circle = Rect::from_xywh(
            center.x - radius,
            center.y - radius,
            radius * 2.0,
            radius * 2.0,
        );
        cx.slot(circle, &root, |_, _| {});
        let arc_radius = (radius - thumb.min_width / 2.0).max(0.0);
        let start = if self.wrapping { 0.0 } else { -0.75 * PI };
        let at = |angle: f64| {
            Point::new(
                center.x + angle.sin() * arc_radius,
                center.y - angle.cos() * arc_radius,
            )
        };
        let arc = |from: f64, to: f64| {
            // Line segments of at most 4 degrees follow the circle.
            let steps = (((to - from).abs() / (PI / 45.0)).ceil() as usize).max(1);
            let mut path = Path::new();
            path.move_to(at(from));
            for step in 1..=steps {
                path.line_to(at(from + (to - from) * step as f64 / steps as f64));
            }
            path
        };
        let rtl = cx.rtl;
        let stroke =
            |list: &mut DisplayList, path: Path, paint: &choscorui_core::Paint, width: f64| {
                let _ = list.stroke_path(
                    path,
                    paint_in(paint, circle, rtl),
                    choscorui_core::StrokeStyle {
                        cap: choscorui_core::LineCap::Round,
                        ..choscorui_core::StrokeStyle::new(width)
                    },
                );
            };
        stroke(
            cx.list,
            arc(start, start + self.sweep()),
            &track.background,
            track.min_width,
        );
        let angle = self.angle();
        if angle > start {
            let faded = range.opacity < 1.0;
            if faded {
                let _ = cx.list.push_opacity(range.opacity);
            }
            stroke(
                cx.list,
                arc(start, angle),
                &range.background,
                range.min_width,
            );
            if faded {
                let _ = cx.list.pop_opacity();
            }
        }
        let tip = at(angle);
        let knob = Rect::from_xywh(
            tip.x - thumb.min_width / 2.0,
            tip.y - thumb.min_height / 2.0,
            thumb.min_width,
            thumb.min_height,
        );
        cx.slot(knob, &thumb, |_, _| {});
    }

    /// The dial with `name`, for a control host.
    pub fn hosted(self, name: impl Into<String>) -> HostedDial {
        HostedDial {
            dial: self,
            name: name.into(),
            dragging: false,
            center: Point::new(0.0, 0.0),
        }
    }
}

/// A [`Dial`] with its name, mounted in a control host. `Dial` is `Copy`
/// and takes its name in `semantics`, so it cannot keep the name itself.
/// A press or a drag points the dial, and keys and assistive actions
/// step it. It reports `Changed(Number)`.
#[derive(Clone, Debug, PartialEq)]
pub struct HostedDial {
    dial: Dial,
    name: String,
    dragging: bool,
    center: Point,
}

impl HostedDial {
    fn report(&self, cx: &mut ControlContext<'_>, changed: bool) {
        if changed {
            let value = f64::from(self.dial.value());
            cx.emit(ControlMessage::Changed(ControlValue::Number(value)));
            cx.request_semantics();
        }
        cx.request_paint();
    }

    fn release(&mut self, cx: &mut ControlContext<'_>) {
        self.dragging = false;
        cx.capture_pointer(false);
        cx.request_paint();
    }
}

impl CustomControl for HostedDial {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let side = cx.tokens.spacing * 16.0;
        Measured::new(Size::new(side, side))
    }

    fn arrange(&mut self, _cx: &MeasureContext<'_>, bounds: Rect) {
        self.center = bounds.center();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if cx.bounds().contains(*position) => {
                self.dragging = true;
                cx.capture_pointer(true);
                let changed = self.dial.point_to(self.center, *position);
                self.report(cx, changed);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) if self.dragging => {
                let changed = self.dial.point_to(self.center, *position);
                self.report(cx, changed);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) if self.dragging => {
                self.release(cx);
                EventReply::Stop
            }
            ControlEvent::FocusChanged(false) | ControlEvent::Dismiss if self.dragging => {
                self.release(cx);
                EventReply::Continue
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                let changed = self.dial.handle_key(*key);
                self.report(cx, changed);
                let step = matches!(
                    key,
                    Key::ArrowUp
                        | Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::PageUp
                        | Key::PageDown
                        | Key::Home
                        | Key::End
                );
                if step {
                    EventReply::Stop
                } else {
                    EventReply::Continue
                }
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx.state;
        self.dial.paint(cx, state);
    }

    fn semantics(&self) -> SemanticNode {
        self.dial.semantics(&self.name)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        let changed = match action {
            SemanticAction::Increment => self.dial.handle_key(Key::ArrowUp),
            SemanticAction::Decrement => self.dial.handle_key(Key::ArrowDown),
            SemanticAction::SetValue => {
                let Some(value) = value.and_then(|value| value.trim().parse::<i32>().ok()) else {
                    return false;
                };
                let old = self.dial.range.value();
                self.dial.range.set_value(value) != old
            }
            SemanticAction::Focus => {
                cx.request_focus();
                return true;
            }
            _ => return false,
        };
        self.report(cx, changed);
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.dragging = false;
        }
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Number(value) if value.is_finite() => {
                self.dial.range.set_value(value.round() as i32);
                true
            }
            _ => false,
        }
    }
}

// ------------------------------------------------------------------ ToolBox

/// Accordion of titled pages: one page open at a time. Up/Down move
/// between headers, Space/Return opens the focused page. Each header is a
/// button exposing its page's expanded state.
#[derive(Clone, Debug)]
pub struct ToolBox {
    titles: Vec<String>,
    current: usize,
    focused: usize,
    /// In a control host: the header height, the bounds, and the header
    /// under a held press.
    header: f64,
    bounds: Rect,
    pressed: Option<usize>,
}

impl ToolBox {
    pub fn new(titles: Vec<String>) -> Self {
        Self {
            titles,
            current: 0,
            focused: 0,
            header: 0.0,
            bounds: Rect::default(),
            pressed: None,
        }
    }
    pub fn current(&self) -> usize {
        self.current
    }
    pub fn focused(&self) -> usize {
        self.focused
    }
    pub fn open(&mut self, page: usize) -> bool {
        if page >= self.titles.len() || page == self.current {
            return false;
        }
        self.current = page;
        self.focused = page;
        true
    }
    pub fn handle_key(&mut self, key: Key) -> bool {
        let last = self.titles.len().saturating_sub(1);
        match key {
            Key::ArrowDown => self.focused = (self.focused + 1).min(last),
            Key::ArrowUp => self.focused = self.focused.saturating_sub(1),
            Key::Home => self.focused = 0,
            Key::End => self.focused = last,
            Key::Space | Key::Enter => return self.open(self.focused),
            _ => {}
        }
        false
    }
    /// Header rectangles and the open page's rectangle inside `bounds`.
    pub fn layout(&self, bounds: Rect, header_height: f64) -> (Vec<Rect>, Rect) {
        let page_height = (bounds.size.height - header_height * self.titles.len() as f64).max(0.0);
        let mut y = bounds.origin.y;
        let mut headers = Vec::new();
        let mut page = Rect::new(Point::new(bounds.origin.x, y), Size::new(0.0, 0.0));
        for index in 0..self.titles.len() {
            headers.push(Rect::new(
                Point::new(bounds.origin.x, y),
                Size::new(bounds.size.width, header_height),
            ));
            y += header_height;
            if index == self.current {
                page = Rect::new(
                    Point::new(bounds.origin.x, y),
                    Size::new(bounds.size.width, page_height),
                );
                y += page_height;
            }
        }
        (headers, page)
    }
    pub fn header_semantics(&self) -> Vec<SemanticNode> {
        self.titles
            .iter()
            .enumerate()
            .map(|(index, title)| SemanticNode {
                role: SemanticRole::Button,
                name: title.clone(),
                description: String::new(),
                value: None,
                state: SemanticState {
                    expanded: Some(index == self.current),
                    ..SemanticState::default()
                },
                relations: Vec::new(),
                actions: vec![SemanticAction::Activate, SemanticAction::Expand],
                text: None,
            })
            .collect()
    }

    /// The header height of the `Accordion` trigger: one line of text and
    /// its padding.
    pub fn header_height(&self, styles: &Styles, tokens: &DesignTokens) -> f64 {
        let style = styles.resolve(
            tokens,
            StyleQuery::new(ControlKind::Accordion, Slot::Trigger),
        );
        style.text.line_height + style.padding.vertical()
    }

    /// Paints the headers with the `Accordion` look: a bottom border under
    /// each item, the title, and a chevron that points up on the open page.
    /// `focus_visible` rings the focused header. The host paints the page.
    pub fn paint(&self, cx: &mut PaintContext<'_>, header_height: f64, focus_visible: bool) {
        let (headers, page) = self.layout(cx.bounds, header_height);
        let query = |slot| StyleQuery::new(ControlKind::Accordion, slot);
        let item = cx.style(query(Slot::Item));
        let icon = cx.style(query(Slot::Icon));
        for (index, (header, title)) in headers.into_iter().zip(&self.titles).enumerate() {
            let open = index == self.current;
            let mut extent = header;
            if open {
                extent.size.height += page.size.height;
            }
            cx.slot(extent, &item, |_, _| {});
            let state = VisualState::NONE.set(VisualState::OPEN, open).set(
                VisualState::FOCUS_VISIBLE,
                focus_visible && index == self.focused,
            );
            let trigger = cx.style(query(Slot::Trigger).state(state));
            cx.slot(header, &trigger, |cx, inner| {
                let width = icon.icon_size.min(inner.size.width);
                let x = if cx.rtl {
                    inner.origin.x
                } else {
                    inner.max_x() - width
                };
                let chevron = if open {
                    Icon::ChevronUp
                } else {
                    Icon::ChevronDown
                };
                cx.icon(
                    Rect::from_xywh(x, inner.origin.y, width, inner.size.height),
                    chevron,
                    &icon,
                );
                let mut text = inner;
                text.size.width = (inner.size.width - width - trigger.gap).max(0.0);
                if cx.rtl {
                    text.origin.x = inner.max_x() - text.size.width;
                }
                cx.label(text, title, &trigger, TextAlign::Start);
            });
        }
    }

    fn header_at(&self, point: Point) -> Option<usize> {
        hosted::hit(&self.layout(self.bounds, self.header).0, point)
    }

    /// Opens `page` in a host and reports it.
    fn open_page(&mut self, cx: &mut ControlContext<'_>, page: usize) {
        if self.open(page) {
            cx.emit(ControlMessage::Changed(ControlValue::Index(Some(page))));
        }
        cx.request_paint();
        cx.request_semantics();
    }
}

/// One node publishes the focused header. A click on a header opens its
/// page. It reports `Changed(Index)` with the open page.
impl CustomControl for ToolBox {
    /// The headers, and all of the height that the constraints give for
    /// the open page.
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let max = cx.constraints.max;
        let headers = self.header_height(cx.styles, cx.tokens) * self.titles.len() as f64;
        let width = if max.width.is_finite() {
            max.width
        } else {
            cx.tokens.spacing * 120.0
        };
        let height = if max.height.is_finite() {
            max.height.max(headers)
        } else {
            headers
        };
        Measured::new(Size::new(width, height))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.header = self.header_height(cx.styles, cx.tokens);
        self.bounds = bounds;
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => {
                let Some(index) = self.header_at(*position) else {
                    return EventReply::Continue;
                };
                self.pressed = Some(index);
                self.focused = index;
                cx.capture_pointer(true);
                cx.request_paint();
                cx.request_semantics();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) if self.pressed.is_some() => {
                cx.capture_pointer(false);
                let hit = self.header_at(*position);
                if let Some(index) = self.pressed.take().filter(|index| hit == Some(*index)) {
                    self.open_page(cx, index);
                }
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                if !matches!(
                    key,
                    Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End | Key::Space | Key::Enter
                ) {
                    return EventReply::Continue;
                }
                if self.handle_key(*key) {
                    let page = self.current;
                    cx.emit(ControlMessage::Changed(ControlValue::Index(Some(page))));
                }
                cx.request_paint();
                cx.request_semantics();
                EventReply::Stop
            }
            ControlEvent::FocusChanged(false) | ControlEvent::Dismiss => {
                if self.pressed.take().is_some() {
                    cx.capture_pointer(false);
                }
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let focus_visible = cx.state.focus_visible();
        ToolBox::paint(self, cx, self.header, focus_visible);
    }

    /// The node of the focused header, as `header_semantics` gives it.
    fn semantics(&self) -> SemanticNode {
        self.header_semantics()
            .into_iter()
            .nth(self.focused)
            .unwrap_or_else(|| hosted::node(SemanticRole::Group, "", Vec::new()))
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if !matches!(action, SemanticAction::Activate | SemanticAction::Expand) {
            return false;
        }
        self.open_page(cx, self.focused);
        true
    }

    fn focusable(&self) -> bool {
        !self.titles.is_empty()
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.pressed = None;
        }
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Index(Some(page)) if *page < self.titles.len() => {
                self.open(*page);
                true
            }
            _ => false,
        }
    }
}

// ----------------------------------------------------------- NumericDisplay

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumberBase {
    Decimal,
    Hexadecimal,
    Octal,
    Binary,
}

/// Seven-segment style numeric display. Values that do not fit in
/// `digits` show as dashes and set the overflow flag; the accessible value
/// is the plain number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumericDisplay {
    digits: usize,
    base: NumberBase,
    value: i64,
}

impl NumericDisplay {
    pub fn new(digits: usize, base: NumberBase) -> Self {
        Self {
            digits: digits.max(1),
            base,
            value: 0,
        }
    }
    pub fn set_value(&mut self, value: i64) {
        self.value = value;
    }
    pub fn set_base(&mut self, base: NumberBase) {
        self.base = base;
    }
    fn formatted(&self) -> String {
        let magnitude = self.value.unsigned_abs();
        let digits = match self.base {
            NumberBase::Decimal => magnitude.to_string(),
            NumberBase::Hexadecimal => format!("{magnitude:X}"),
            NumberBase::Octal => format!("{magnitude:o}"),
            NumberBase::Binary => format!("{magnitude:b}"),
        };
        if self.value < 0 {
            format!("-{digits}")
        } else {
            digits
        }
    }
    pub fn overflow(&self) -> bool {
        self.formatted().chars().count() > self.digits
    }
    /// Right-aligned text, padded with spaces, or dashes on overflow.
    pub fn text(&self) -> String {
        if self.overflow() {
            "-".repeat(self.digits)
        } else {
            format!("{:>width$}", self.formatted(), width = self.digits)
        }
    }
    pub fn semantics(&self, name: &str) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::StaticText,
            name: name.to_owned(),
            description: if self.overflow() {
                "Value too large to display".into()
            } else {
                String::new()
            },
            value: Some(self.formatted()),
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        }
    }

    /// Segments lit for one character: a, b, c, d, e, f, g.
    pub fn segments(character: char) -> [bool; 7] {
        match character.to_ascii_uppercase() {
            '0' => [true, true, true, true, true, true, false],
            '1' => [false, true, true, false, false, false, false],
            '2' => [true, true, false, true, true, false, true],
            '3' => [true, true, true, true, false, false, true],
            '4' => [false, true, true, false, false, true, true],
            '5' => [true, false, true, true, false, true, true],
            '6' => [true, false, true, true, true, true, true],
            '7' => [true, true, true, false, false, false, false],
            '8' => [true; 7],
            '9' => [true, true, true, true, false, true, true],
            'A' => [true, true, true, false, true, true, true],
            'B' => [false, false, true, true, true, true, true],
            'C' => [true, false, false, true, true, true, false],
            'D' => [false, true, true, true, true, false, true],
            'E' => [true, false, false, true, true, true, true],
            'F' => [true, false, false, false, true, true, true],
            '-' => [false, false, false, false, false, false, true],
            _ => [false; 7],
        }
    }

    /// Paints the display in the context's bounds with the
    /// `NumericDisplay` recipe: lit segments take the `Value` color and
    /// unlit ones the `Track` color. `state` carries focus flags.
    pub fn paint(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        let query = |slot| StyleQuery::new(ControlKind::NumericDisplay, slot).state(state);
        let (root, value, track) = (
            cx.style(query(Slot::Root)),
            cx.style(query(Slot::Value)),
            cx.style(query(Slot::Track)),
        );
        let on = Paint::Solid(value.foreground);
        cx.slot(cx.bounds, &root, |cx, bounds| {
            self.paint_segments(cx.list, bounds, &on, &track.background)
        });
    }

    /// Fills each lit segment with `on` and each unlit one with `off`,
    /// both placed on the whole display.
    fn paint_segments(&self, list: &mut DisplayList, bounds: Rect, on: &Paint, off: &Paint) {
        let text = self.text();
        let cell = bounds.size.width / self.digits as f64;
        let thickness = (cell * 0.12).max(1.0);
        for (index, character) in text.chars().enumerate() {
            let x = bounds.origin.x + index as f64 * cell + cell * 0.15;
            let w = cell * 0.7;
            let h = bounds.size.height * 0.9;
            let y = bounds.origin.y + bounds.size.height * 0.05;
            let half = h / 2.0;
            let rects = [
                Rect::new(Point::new(x, y), Size::new(w, thickness)),
                Rect::new(Point::new(x + w - thickness, y), Size::new(thickness, half)),
                Rect::new(
                    Point::new(x + w - thickness, y + half),
                    Size::new(thickness, half),
                ),
                Rect::new(Point::new(x, y + h - thickness), Size::new(w, thickness)),
                Rect::new(Point::new(x, y + half), Size::new(thickness, half)),
                Rect::new(Point::new(x, y), Size::new(thickness, half)),
                Rect::new(
                    Point::new(x, y + half - thickness / 2.0),
                    Size::new(w, thickness),
                ),
            ];
            for (lit, rect) in Self::segments(character).into_iter().zip(rects) {
                let paint = if lit { on } else { off };
                match paint.solid_color() {
                    Some(color) => list.fill_rect(rect, color),
                    None => {
                        let paint = paint_in(paint, bounds, false);
                        let _ = list.fill_path(Path::rect(rect), paint, FillRule::NonZero);
                    }
                }
            }
        }
    }

    /// The display with `name`, for a control host.
    pub fn hosted(self, name: impl Into<String>) -> HostedNumericDisplay {
        HostedNumericDisplay {
            display: self,
            name: name.into(),
        }
    }
}

/// A [`NumericDisplay`] with its name, mounted in a control host.
/// `NumericDisplay` is `Copy` and takes its name in `semantics`, so it
/// cannot keep the name itself. It takes no input and reports nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct HostedNumericDisplay {
    display: NumericDisplay,
    name: String,
}

impl CustomControl for HostedNumericDisplay {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let query = |slot| StyleQuery::new(ControlKind::NumericDisplay, slot);
        let (root, value) = (cx.style(query(Slot::Root)), cx.style(query(Slot::Value)));
        let digits = cx.text.width("8", &value.text) * self.display.digits as f64;
        let width = root.padding.horizontal() + digits;
        let height = root.padding.vertical() + value.text.line_height;
        Measured::new(Size::new(
            width.max(root.min_width),
            height.max(root.min_height),
        ))
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx.state;
        self.display.paint(cx, state);
    }

    fn semantics(&self) -> SemanticNode {
        self.display.semantics(&self.name)
    }

    fn focusable(&self) -> bool {
        false
    }

    /// Takes a whole `Number`.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Number(value)
                if value.fract() == 0.0 && value.abs() <= i64::MAX as f64 =>
            {
                self.display.set_value(*value as i64);
                true
            }
            _ => false,
        }
    }
}

// ------------------------------------------------------- FocusFrame, SizeGrip

/// Draws the focus ring around whichever node has keyboard focus, outside
/// its bounds, so containers that clip their children still show focus.
#[derive(Clone, Copy, Debug, Default)]
pub struct FocusFrame;

impl FocusFrame {
    /// The bounds of the focused node, or None when it is hidden.
    pub fn rect(&self, tree: &Tree) -> Option<Rect> {
        let focused = tree.focus()?;
        if tree.visible(focused) == Some(false) {
            return None;
        }
        tree.bounds(focused)
    }

    /// Draws the theme's focus ring, at radius md, while focus is
    /// visible.
    pub fn paint(&self, cx: &mut PaintContext<'_>, tree: &Tree) {
        let Some(rect) = self.rect(tree) else {
            return;
        };
        if tree
            .focus()
            .is_some_and(|focused| tree.focus_visible(focused))
        {
            let tokens = cx.tokens;
            let color = tokens.ring_color(tokens.color(ColorRole::Ring));
            let radius = tokens.radius(RadiusScale::Md);
            let _ = cx.list.draw_ring(rect, radius, tokens.ring_width, color);
        }
    }
}

/// Bottom-trailing resize affordance. Dragging yields a new window content
/// size clamped to the window's minimum and maximum; RTL grips sit at the
/// bottom-left and grow leftward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeGrip {
    pub minimum: Size,
    pub maximum: Size,
    pub rtl: bool,
    start: Option<(Point, Size)>,
    /// In a control host: the window content size, and the last size the
    /// drag reported.
    viewport: Size,
    reported: Option<Size>,
}

impl SizeGrip {
    pub fn new(minimum: Size, maximum: Size, rtl: bool) -> Self {
        Self {
            minimum,
            maximum,
            rtl,
            start: None,
            viewport: Size::new(0.0, 0.0),
            reported: None,
        }
    }
    pub fn rect(&self, window: Size, extent: f64) -> Rect {
        let x = if self.rtl { 0.0 } else { window.width - extent };
        Rect::new(
            Point::new(x, window.height - extent),
            Size::new(extent, extent),
        )
    }
    pub fn press(&mut self, at: Point, size: Size) {
        self.start = Some((at, size));
    }
    /// New size while dragging (apply with `Window::set_content_size`).
    pub fn drag(&self, at: Point) -> Option<Size> {
        let (origin, size) = self.start?;
        let dx = if self.rtl {
            origin.x - at.x
        } else {
            at.x - origin.x
        };
        Some(Size::new(
            (size.width + dx).clamp(self.minimum.width, self.maximum.width),
            (size.height + at.y - origin.y).clamp(self.minimum.height, self.maximum.height),
        ))
    }
    pub fn release(&mut self) {
        self.start = None;
    }

    /// Paints three diagonal grip lines toward the corner in the context's
    /// bounds, in the `SizeGrip` color.
    pub fn paint(&self, cx: &mut PaintContext<'_>) {
        let style = cx.style(StyleQuery::new(ControlKind::SizeGrip, Slot::Root));
        let bounds = cx.bounds;
        let size = bounds.size.width.min(bounds.size.height);
        let (corner_x, sign) = if self.rtl {
            (bounds.origin.x, 1.0)
        } else {
            (bounds.max_x(), -1.0)
        };
        let bottom = bounds.max_y();
        for step in 1..=3 {
            let reach = size * step as f64 / 3.0;
            let _ = cx.list.stroke_path(
                Path::line(
                    Point::new(corner_x + sign * reach, bottom),
                    Point::new(corner_x, bottom - reach),
                ),
                choscorui_core::Paint::Solid(style.foreground),
                choscorui_core::StrokeStyle::new(1.0),
            );
        }
    }

    fn end_drag(&mut self, cx: &mut ControlContext<'_>) {
        self.release();
        self.reported = None;
        cx.capture_pointer(false);
    }
}

/// A drag reports the new window content size as `Changed(Range(width,
/// height))`. The grip takes the direction of the host and has no focus.
/// Its node is hidden, because the grip has no semantic node.
impl CustomControl for SizeGrip {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(StyleQuery::new(ControlKind::SizeGrip, Slot::Root));
        Measured::new(Size::new(style.min_width, style.min_height))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, _bounds: Rect) {
        self.viewport = cx.viewport.size;
        self.rtl = cx.rtl();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if cx.bounds().contains(*position) => {
                self.press(*position, self.viewport);
                cx.capture_pointer(true);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) if self.start.is_some() => {
                if let Some(size) = self
                    .drag(*position)
                    .filter(|size| self.reported != Some(*size))
                {
                    self.reported = Some(size);
                    cx.emit(ControlMessage::Changed(ControlValue::Range(
                        size.width,
                        size.height,
                    )));
                }
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) if self.start.is_some() => {
                self.end_drag(cx);
                EventReply::Stop
            }
            ControlEvent::FocusChanged(false) | ControlEvent::Dismiss if self.start.is_some() => {
                self.end_drag(cx);
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        SizeGrip::paint(self, cx);
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = hosted::node(SemanticRole::Group, "", Vec::new());
        node.state.hidden = true;
        node
    }

    fn focusable(&self) -> bool {
        false
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.release();
            self.reported = None;
        }
    }
}
