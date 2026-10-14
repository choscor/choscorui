//! Remaining basic widgets: standalone scroll bar, decimal spin box, dial,
//! tool and command-link buttons, button groups, toolbox (accordion),
//! numeric (LCD) display, focus frame, size grip, and key-sequence capture.
//! Each documents its value, keyboard, and accessibility contract.

use crate::custom::{
    ControlContext, ControlEvent, ControlHost, ControlMessage, ControlValue, CustomControl,
    MeasureContext,
};
use crate::form::step_text::{Repeat, StepText, text_rect};
use crate::hosted::{self, Parts};
use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, Styles, TextAlign, VisualState, content_rect,
    fill_shape, paint_in,
};
use crate::{
    IntegerRange, NumberError, NumberInput, RangeError, STEPPER_WIDTH, ValidationError,
    ValidationState,
};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayList, EventPhase, EventReply, FillRule, Icon, Key, KeyEvent,
    LayoutDirection, Locale, Localizer, Measured, Paint, Path, Point, PointerButton, PointerEvent,
    RadiusScale, Rect, SemanticAction, SemanticNode, SemanticRange, SemanticRole, SemanticState,
    Size, Tree,
};
use std::f64::consts::PI;

mod button_group;
mod key_sequence;
mod overlay_scroll_bar;
mod tool_buttons;
pub use button_group::{ButtonGroup, ButtonGroupMember, HostedButtonGroup};
pub use key_sequence::{KeySequenceChange, KeySequenceEdit, shortcut_text};
pub use overlay_scroll_bar::{OverlayScrollBar, OverlayScrollReply};
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
/// In a control host, the bar is a tab stop, and a held press on the
/// track repeats the paging until the thumb reaches the pointer.
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
    /// Whether a horizontal bar starts at the right.
    rtl: bool,
    enabled: bool,
    /// The paging repeat of a held press on the track, and the press
    /// position along the bar.
    paging: (Repeat, f64),
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
            rtl: false,
            enabled: true,
            paging: (Repeat::default(), 0.0),
        }
    }
    pub fn orientation(&self) -> ScrollOrientation {
        self.orientation
    }
    /// Starts a horizontal bar at the right in a right-to-left layout:
    /// the thumb, the track presses, and ArrowLeft and ArrowRight mirror.
    /// A hosted bar takes the direction from `arrange`.
    pub fn set_direction(&mut self, direction: LayoutDirection) {
        self.rtl = direction == LayoutDirection::Rtl;
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
                match key.in_reading_order(self.rtl) {
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
                disabled: !self.enabled,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: if self.enabled {
                vec![
                    SemanticAction::Increment,
                    SemanticAction::Decrement,
                    SemanticAction::Focus,
                ]
            } else {
                Vec::new()
            },
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
        if self.is_needed() {
            let bounds = cx.bounds;
            self.paint_track(cx, bounds, state);
        }
    }

    /// Paints the bar fill in `track`, and the thumb inside it, with the
    /// `ScrollArea` slots in `state`.
    fn paint_track(&self, cx: &mut PaintContext<'_>, track: Rect, state: VisualState) {
        let query = |slot| StyleQuery::new(ControlKind::ScrollArea, slot).state(state);
        let (bar, thumb) = (
            cx.style(query(Slot::Scrollbar)),
            cx.style(query(Slot::Thumb)),
        );
        fill_shape(cx.list, track, &bar.radius, cx.rtl, &bar.background);
        let rect = self.thumb_rect(track);
        let rect = content_rect(rect, &bar, cx.rtl);
        fill_shape(cx.list, rect, &thumb.radius, cx.rtl, &thumb.background);
    }

    /// The thumb in `track`, across the full thickness of the bar.
    pub(crate) fn thumb_rect(&self, track: Rect) -> Rect {
        let (start, length) = self.thumb();
        match self.orientation {
            ScrollOrientation::Vertical => Rect::from_xywh(
                track.origin.x,
                track.origin.y + start,
                track.size.width,
                length,
            ),
            ScrollOrientation::Horizontal if self.rtl => Rect::from_xywh(
                track.max_x() - start - length,
                track.origin.y,
                length,
                track.size.height,
            ),
            ScrollOrientation::Horizontal => Rect::from_xywh(
                track.origin.x + start,
                track.origin.y,
                length,
                track.size.height,
            ),
        }
    }

    /// The offset of `point` along the bar from the start of `bounds`,
    /// in reading order.
    pub(crate) fn along(&self, bounds: Rect, point: Point) -> f64 {
        match self.orientation {
            ScrollOrientation::Vertical => point.y - bounds.origin.y,
            ScrollOrientation::Horizontal if self.rtl => bounds.max_x() - point.x,
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

/// The bar is a tab stop that takes its keys. It captures the pointer
/// during a thumb drag and while a track press repeats.
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

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.rtl = cx.rtl();
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
            }) if bounds.contains(*position) => {
                let at = self.along(bounds, *position);
                let (start, _) = self.thumb();
                let changed = self.handle(ScrollBarInput::Press { at });
                if self.drag.is_none() {
                    self.paging = (Repeat::default(), at);
                    self.paging.0.press(at > start, cx.time());
                }
                cx.capture_pointer(true);
                self.report(cx, changed);
                return EventReply::Stop;
            }
            ControlEvent::Timer => {
                let (start, length) = self.thumb();
                let at = self.paging.1;
                let changed = match self.paging.0.fire(cx.time()) {
                    Some(true) if at > start + length => self.set_value(self.value + self.viewport),
                    Some(false) if at < start => self.set_value(self.value - self.viewport),
                    _ => false,
                };
                self.report(cx, changed);
                return EventReply::Continue;
            }
            ControlEvent::Pointer(PointerEvent::Up { .. })
            | ControlEvent::FocusChanged(false)
            | ControlEvent::Dismiss
                if self.paging.0.release() =>
            {
                cx.capture_pointer(false);
                return EventReply::Stop;
            }
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

    fn deadline(&self) -> Option<f64> {
        self.paging.0.deadline()
    }

    /// Steps one page. Returns false at the end of the range.
    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if action == SemanticAction::Focus {
            cx.request_focus();
            return true;
        }
        let changed = self.handle(ScrollBarInput::Semantic(action));
        self.report(cx, changed);
        changed
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.drag = None;
            self.paging.0.release();
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

/// Locale-aware decimal spin box: text editing with a caret, a selection,
/// and the number filter; arrows step, Page keys step ten times, and
/// Return commits only a valid value.
pub struct DecimalSpinBox {
    name: String,
    input: NumberInput,
    minimum: f64,
    maximum: f64,
    step: f64,
    value: f64,
    fraction_digits: usize,
    text: StepText,
    /// The error of the last commit, until the next edit.
    error: Option<ValidationError>,
    /// In a control host: the direction.
    rtl: bool,
}

impl DecimalSpinBox {
    /// A spin box from `minimum` to `maximum` that steps by `step` and
    /// shows `fraction_digits` digits after the decimal sign. `value` is
    /// rounded and then clamped to the range. A NaN bound, a minimum
    /// greater than the maximum, or a step that is not positive is an
    /// error.
    pub fn new(
        name: impl Into<String>,
        locale: Locale,
        (minimum, maximum): (f64, f64),
        step: f64,
        fraction_digits: usize,
        value: f64,
    ) -> Result<Self, RangeError> {
        if minimum.is_nan() || maximum.is_nan() {
            return Err(RangeError::NotANumber);
        }
        if minimum > maximum {
            return Err(RangeError::Reversed);
        }
        if step.is_nan() || step <= 0.0 {
            return Err(RangeError::ZeroStep);
        }
        let name = name.into();
        let input =
            NumberInput::new(locale, fraction_digits).with_range(Some(minimum), Some(maximum));
        let text = StepText::new(&name, "", input.filter());
        let mut spin = Self {
            name,
            input,
            minimum,
            maximum,
            step,
            value: minimum,
            fraction_digits,
            text,
            error: None,
            rtl: false,
        };
        let value = if value.is_nan() { minimum } else { value };
        spin.set_value(value);
        Ok(spin)
    }
    pub fn value(&self) -> f64 {
        self.value
    }
    pub fn text(&self) -> &str {
        self.text.text()
    }
    /// The validation of the text: the error of a failed commit until
    /// the next edit, and else the state while typing.
    pub fn validation(&self) -> ValidationState {
        match &self.error {
            Some(error) => ValidationState::Invalid(error.clone()),
            None => self.input.validate(self.text()).0,
        }
    }
    /// Replaces the edited text (validation is reported, value unchanged).
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text.set(&text.into());
        self.edited();
    }
    /// Rounds `value` to the fraction digits, and then clamps it to the
    /// range.
    fn settle(&self, value: f64) -> f64 {
        let scale = 10f64.powi(self.fraction_digits as i32);
        ((value * scale).round() / scale).clamp(self.minimum, self.maximum)
    }
    fn set_value(&mut self, value: f64) -> bool {
        let value = self.settle(value);
        let changed = value != self.value;
        self.value = value;
        self.text.set(&self.input.format(value));
        self.edited();
        changed
    }
    /// Clears the commit error after the text changes.
    fn edited(&mut self) {
        self.error = None;
        let validation = self.validation();
        self.text.set_validation(validation);
    }
    /// Takes valid pending text as the value before a step.
    fn take_pending(&mut self) {
        if let (ValidationState::Valid, Some(value)) = self.input.validate(self.text()) {
            self.value = self.settle(value);
        }
    }
    /// Steps by `steps` steps from the value, after the pending text.
    fn step_by(&mut self, steps: f64) -> bool {
        let before = self.value;
        self.take_pending();
        self.set_value(self.value + self.step * steps);
        self.value != before
    }
    /// True when `key` steps past the end of the range, so the spin box
    /// refuses it.
    fn at_bound(&self, key: Key) -> bool {
        match key {
            Key::ArrowUp | Key::PageUp | Key::End => self.value >= self.maximum,
            Key::ArrowDown | Key::PageDown | Key::Home => self.value <= self.minimum,
            _ => false,
        }
    }
    /// Returns true when the value changed.
    pub fn handle_key(&mut self, key: Key) -> bool {
        match key {
            Key::ArrowUp => self.step_by(1.0),
            Key::ArrowDown => self.step_by(-1.0),
            Key::PageUp => self.step_by(10.0),
            Key::PageDown => self.step_by(-10.0),
            Key::Home => self.set_value(self.minimum),
            Key::End => self.set_value(self.maximum),
            Key::Enter => self.commit(),
            Key::Escape => {
                self.set_value(self.value);
                false
            }
            _ => false,
        }
    }
    /// Commits valid text and formats it; invalid text stays for
    /// correction with its error.
    pub fn commit(&mut self) -> bool {
        match self.input.commit(self.text()) {
            (ValidationState::Valid, Some(value)) => self.set_value(value),
            (state, _) => {
                self.error = Some(match state {
                    ValidationState::Invalid(error) => error,
                    _ => NumberError::NotANumber.into(),
                });
                let validation = self.validation();
                self.text.set_validation(validation);
                false
            }
        }
    }
    /// Commits valid text when focus leaves; invalid text gives way to
    /// the last committed value.
    fn blur(&mut self) -> bool {
        let changed = self.commit();
        if self.error.is_some() {
            self.set_value(self.value);
        }
        changed
    }
    /// Re-formats for a new locale without changing the value.
    pub fn set_locale(&mut self, locale: Locale) {
        self.input = NumberInput::new(locale, self.fraction_digits)
            .with_range(Some(self.minimum), Some(self.maximum));
        self.text.set_filter(self.input.filter());
        self.set_value(self.value);
    }
    /// The semantics, with the built-in English messages.
    pub fn semantics(&self) -> SemanticNode {
        self.semantics_with(Localizer::builtin())
    }
    /// The semantics, with the validation message in the locale of
    /// `strings`.
    pub fn semantics_with(&self, strings: &Localizer) -> SemanticNode {
        let validation = self.validation();
        let text = self.text.semantics(strings).text;
        SemanticNode {
            role: SemanticRole::SpinButton,
            name: self.name.clone(),
            description: match &validation {
                ValidationState::Invalid(error) => error.message(strings),
                _ => String::new(),
            },
            value: Some(self.text().to_owned()),
            state: SemanticState {
                invalid: matches!(validation, ValidationState::Invalid(_)),
                range: Some(SemanticRange {
                    value: self.value,
                    minimum: self.minimum,
                    maximum: self.maximum,
                    step: self.step,
                }),
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::SetValue,
                SemanticAction::Focus,
            ],
            text,
        }
    }
    pub fn handle_semantic(&mut self, action: SemanticAction, value: Option<&str>) -> bool {
        match action {
            SemanticAction::Increment => self.handle_key(Key::ArrowUp),
            SemanticAction::Decrement => self.handle_key(Key::ArrowDown),
            SemanticAction::SetValue => {
                if let Some(value) = value {
                    if !self.input.accepts(value.trim()) {
                        return false;
                    }
                    self.set_text(value);
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
            host.invalid() || matches!(self.validation(), ValidationState::Invalid(_)),
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

    /// Paints the input box, the edited text with its caret and
    /// selection, and the steppers, like [`crate::SpinBox::paint`].
    pub fn paint(&self, cx: &mut PaintContext<'_>, host: VisualState) {
        let steppers = Self::stepper_rect(cx.bounds, cx.rtl);
        let state = self.text.state(self.visual_state(host));
        let ends = (self.value >= self.maximum, self.value <= self.minimum);
        self.text.paint(cx, state, steppers, ends);
    }

    /// Reports a value change in a host.
    fn report(&mut self, cx: &mut ControlContext<'_>, changed: bool) {
        if changed {
            cx.emit(ControlMessage::Changed(ControlValue::Number(self.value)));
        }
        cx.request_paint();
        cx.request_semantics();
    }

    /// Runs a step or commit key in a host. A step past the end of the
    /// range beeps.
    fn hosted_key(&mut self, cx: &mut ControlContext<'_>, key: Key) {
        let refused = self.at_bound(key);
        let changed = self.handle_key(key);
        if refused && !changed {
            cx.beep();
        }
        self.report(cx, changed);
    }
}

/// The text edits like an input: a press places the caret, typed text
/// replaces the selection, and the number filter rejects text that cannot
/// become a number. Focus selects the whole text. Arrows, Page keys, Home,
/// End, and the steppers take valid pending text first and then step.
/// Return commits; a loss of focus commits valid text and restores the
/// value after invalid text.
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
        let steppers = Self::stepper_rect(cx.bounds(), self.rtl);
        if let Some(key) = self.text.track_steppers(cx, event, steppers) {
            cx.request_focus();
            self.hosted_key(cx, key);
            return EventReply::Stop;
        }
        match event {
            ControlEvent::Key(KeyEvent::Down { key, .. })
                if cx.is_focused()
                    && matches!(
                        key,
                        Key::ArrowUp
                            | Key::ArrowDown
                            | Key::PageUp
                            | Key::PageDown
                            | Key::Home
                            | Key::End
                            | Key::Enter
                            | Key::Escape
                    ) =>
            {
                self.hosted_key(cx, *key);
                EventReply::Stop
            }
            ControlEvent::FocusChanged(true) => {
                self.text.select_all();
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::FocusChanged(false) => {
                let changed = self.blur();
                self.report(cx, changed);
                EventReply::Continue
            }
            _ => {
                let rect = text_rect(cx.bounds(), steppers);
                let (reply, edited) = self.text.event(cx, event, rect);
                if edited {
                    self.edited();
                    cx.request_semantics();
                }
                reply
            }
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        DecimalSpinBox::paint(self, cx, host);
    }

    fn semantics(&self) -> SemanticNode {
        DecimalSpinBox::semantics(self)
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        self.semantics_with(strings)
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

    fn set_enabled(&mut self, enabled: bool) {
        self.text.set_enabled(enabled);
    }

    fn deadline(&self) -> Option<f64> {
        self.text.deadline()
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Number(value) if value.is_finite() => {
                DecimalSpinBox::set_value(self, *value);
                true
            }
            _ => false,
        }
    }
}

// --------------------------------------------------------------------- Dial

/// Rotary range control. Angle 0 points up; values increase clockwise over
/// `sweep` radians. A wrapping dial uses a full turn of `span + 1` steps,
/// so the maximum sits one step before the minimum, and its steps and
/// page steps wrap. Pointer input snaps to the step. Keyboard and
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
    /// The number of values on the scale: one more than the span, so that
    /// a wrapping dial does not put the maximum on the minimum.
    fn scale(&self) -> f64 {
        let span = f64::from(self.range.maximum()) - f64::from(self.range.minimum());
        if self.wrapping { span + 1.0 } else { span }
    }

    /// Sets the value from a pointer position around `center`, snapped to
    /// the step.
    pub fn point_to(&mut self, center: Point, point: Point) -> bool {
        self.aim(center, point, false)
    }

    /// Sets the value from a pointer position around `center`. During a
    /// drag (`dragging`), a non-wrapping dial stays at the end that the
    /// pointer left, and does not jump across the gap to the other end.
    fn aim(&mut self, center: Point, point: Point, dragging: bool) -> bool {
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
        let current = self.range.fraction();
        let fraction = if self.wrapping {
            angle / sweep
        } else {
            // Non-wrapping dials start at -135°. In the dead zone, a drag
            // keeps its end, and a press takes the nearer end.
            let shifted = (angle + 0.75 * PI) % (2.0 * PI);
            let fraction = if shifted <= sweep {
                shifted / sweep
            } else if dragging {
                current.round()
            } else {
                f64::from(u8::from(shifted - sweep < 2.0 * PI - shifted))
            };
            if dragging && (fraction - current).abs() > 0.5 {
                return false;
            }
            fraction
        };
        let offset = (fraction * self.scale()).round() as i64;
        let old = self.range.value();
        let value = if self.wrapping {
            self.wrapped(offset - (i64::from(old) - i64::from(self.range.minimum())))
        } else {
            (i64::from(self.range.minimum()) + offset) as i32
        };
        self.range.set_value(self.range.snap(value)) != old
    }

    /// The value `delta` away from the value, around the scale of a
    /// wrapping dial.
    fn wrapped(&self, delta: i64) -> i32 {
        let minimum = i64::from(self.range.minimum());
        let scale = self.scale() as i64;
        let offset = (i64::from(self.range.value()) - minimum + delta).rem_euclid(scale.max(1));
        (minimum + offset) as i32
    }

    /// Moves by `steps` steps of `size`: around the scale of a wrapping
    /// dial, and else clamped to the range.
    fn step(&mut self, steps: i64, size: u32) {
        let delta = steps * i64::from(size);
        if self.wrapping {
            self.range.set_value(self.wrapped(delta));
        } else {
            let value = i64::from(self.range.value()) + delta;
            self.range
                .set_value(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32);
        }
    }

    pub fn handle_key(&mut self, key: Key) -> bool {
        let old = self.range.value();
        let (step, page) = (self.range.step(), self.range.page_step());
        match key {
            Key::ArrowUp | Key::ArrowRight => self.step(1, step),
            Key::ArrowDown | Key::ArrowLeft => self.step(-1, step),
            Key::PageUp => self.step(1, page),
            Key::PageDown => self.step(-1, page),
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
        let scale = self.scale();
        let fraction = if scale == 0.0 {
            0.0
        } else {
            (f64::from(self.range.value()) - f64::from(self.range.minimum())) / scale
        };
        if self.wrapping {
            fraction * self.sweep()
        } else {
            fraction * self.sweep() - 0.75 * PI
        }
    }
    /// The semantics, described in the language of `strings`.
    pub fn semantics(&self, name: &str, strings: &Localizer) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Slider,
            name: name.to_owned(),
            description: strings.message("choscorui.dial", &[]),
            value: Some(self.range.value().to_string()),
            state: SemanticState {
                range: Some(self.range.into()),
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
    /// Paints the dial in the context's bounds with the `Dial` recipe: a
    /// track arc over the sweep, the value arc, and a thumb at the value.
    /// `state` carries hover, focus, and disabled flags.
    pub fn paint(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        let bounds = cx.bounds;
        let query = |slot| StyleQuery::new(ControlKind::Dial, slot).state(state);
        // The root shows the one focus ring, so the thumb takes no
        // focus flag.
        let knob_state = state.without(VisualState::FOCUS_VISIBLE);
        let (mut root, track, range, thumb) = (
            cx.style(query(Slot::Root)),
            cx.style(query(Slot::Track)),
            cx.style(query(Slot::Range)),
            cx.style(StyleQuery::new(ControlKind::Dial, Slot::Thumb).state(knob_state)),
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
        // The ring goes inside the circle, which touches the bounds.
        let ring = root.ring.take();
        cx.slot(circle, &root, |_, _| {});
        root.ring = ring;
        cx.ring_inside(circle, &root);
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

    /// True when `point` is on the dial circle in `bounds`.
    fn hit(bounds: Rect, point: Point) -> bool {
        let radius = bounds.size.width.min(bounds.size.height) / 2.0;
        let center = bounds.center();
        (point.x - center.x).hypot(point.y - center.y) <= radius
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
/// A press on the dial circle or a drag points the dial, and keys and
/// assistive actions step it. It shows the pressed state during a drag.
/// It reports `Changed(Number)`.
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
            }) if Dial::hit(cx.bounds(), *position) => {
                self.dragging = true;
                cx.capture_pointer(true);
                let changed = self.dial.point_to(self.center, *position);
                self.report(cx, changed);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) if self.dragging => {
                let changed = self.dial.aim(self.center, *position, true);
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
        let state = cx.state.set(VisualState::PRESSED, self.dragging);
        self.dial.paint(cx, state);
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        self.dial.semantics(&self.name, strings)
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
                let value = self.dial.range.snap(value);
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
    /// In a control host: the header height, the bounds, and the hovered
    /// and pressed headers.
    header: f64,
    bounds: Rect,
    parts: Parts,
}

impl ToolBox {
    pub fn new(titles: Vec<String>) -> Self {
        Self {
            titles,
            current: 0,
            focused: 0,
            header: 0.0,
            bounds: Rect::default(),
            parts: Parts::default(),
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
        let host = cx.state;
        for (index, (header, title)) in headers.into_iter().zip(&self.titles).enumerate() {
            let open = index == self.current;
            let mut extent = header;
            if open {
                extent.size.height += page.size.height;
            }
            cx.slot(extent, &item, |_, _| {});
            let state = self
                .parts
                .state(index, host)
                .set(VisualState::OPEN, open)
                .set(
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
        let pressed = self.parts.pressed;
        let mut parts = self.parts;
        parts.track(cx, event, |point| self.header_at(point));
        self.parts = parts;
        match event {
            ControlEvent::Pointer(PointerEvent::Down { .. }) => {
                let Some(index) = self.parts.pressed else {
                    return EventReply::Continue;
                };
                self.focused = index;
                cx.request_semantics();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) if pressed.is_some() => {
                if let Some(index) = pressed.filter(|index| self.parts.hovered == Some(*index)) {
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
            self.parts.clear();
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
    /// The semantics, with an overflow described in the language of
    /// `strings`.
    pub fn semantics(&self, name: &str, strings: &Localizer) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::StaticText,
            name: name.to_owned(),
            description: if self.overflow() {
                strings.message("choscorui.overflow", &[])
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
            counter: false,
        }
    }
}

/// A [`NumericDisplay`] with its name, mounted in a control host.
/// `NumericDisplay` is `Copy` and takes its name in `semantics`, so it
/// cannot keep the name itself. It takes no input and reports nothing,
/// unless it is a [`counter`](Self::counter). It takes and reports its
/// value as `Text` with the decimal digits, so that each `i64` stays
/// exact.
#[derive(Clone, Debug, PartialEq)]
pub struct HostedNumericDisplay {
    display: NumericDisplay,
    name: String,
    counter: bool,
}

impl HostedNumericDisplay {
    /// Makes the display a counter: a tab stop whose keys change the
    /// value. ArrowUp and ArrowRight add 1, ArrowDown and ArrowLeft
    /// subtract 1, PageUp multiplies by 10, and PageDown divides by 10.
    /// Each change saturates at the `i64` bounds and reports
    /// `Changed(Text)`. A counter is a spin button with the increment and
    /// decrement actions.
    #[must_use]
    pub fn counter(mut self) -> Self {
        self.counter = true;
        self
    }

    /// The value after `key`, for the keys that change a counter.
    fn stepped(&self, key: Key) -> Option<i64> {
        let value = self.display.value;
        Some(match key {
            Key::ArrowUp | Key::ArrowRight => value.saturating_add(1),
            Key::ArrowDown | Key::ArrowLeft => value.saturating_sub(1),
            Key::PageUp => value.saturating_mul(10),
            Key::PageDown => value / 10,
            _ => return None,
        })
    }

    /// Sets the value and reports it.
    fn change(&mut self, cx: &mut ControlContext<'_>, value: i64) {
        self.display.set_value(value);
        cx.emit(ControlMessage::Changed(ControlValue::Text(
            value.to_string(),
        )));
        cx.request_paint();
        cx.request_semantics();
    }
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

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let ControlEvent::Key(KeyEvent::Down { key, .. }) = event else {
            return EventReply::Continue;
        };
        if !self.counter || cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let Some(value) = self.stepped(*key) else {
            return EventReply::Continue;
        };
        self.change(cx, value);
        EventReply::Stop
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx.state;
        self.display.paint(cx, state);
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        let mut node = self.display.semantics(&self.name, strings);
        if self.counter {
            node.role = SemanticRole::SpinButton;
            node.actions = vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus,
            ];
        }
        node
    }

    /// A counter steps by 1.
    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        let key = match action {
            SemanticAction::Increment => Key::ArrowUp,
            SemanticAction::Decrement => Key::ArrowDown,
            SemanticAction::Focus if self.counter => {
                cx.request_focus();
                return true;
            }
            _ => return false,
        };
        match self.stepped(key).filter(|_| self.counter) {
            Some(value) => {
                self.change(cx, value);
                true
            }
            None => false,
        }
    }

    fn focusable(&self) -> bool {
        self.counter
    }

    /// Takes `Text` with an `i64`, or a whole `Number`.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let value = match value {
            ControlValue::Text(text) => text.trim().parse::<i64>().ok(),
            ControlValue::Number(value)
                if value.fract() == 0.0 && value.abs() <= i64::MAX as f64 =>
            {
                Some(*value as i64)
            }
            _ => None,
        };
        value.is_some_and(|value| {
            self.display.set_value(value);
            true
        })
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

    /// Draws the focus ring while focus is visible. The ring is the one
    /// that the focused control in `host` left at its last paint, with
    /// the shape of its part and the color of its state; a host painted
    /// with [`HostEnv::framed`](crate::HostEnv::framed) leaves it in place
    /// of painting it, so the ring shows once. A control that left no ring
    /// gets the focus ring of `cx.tokens` at radius md: give the context
    /// the tokens of the focused node.
    pub fn paint(&self, cx: &mut PaintContext<'_>, tree: &Tree, host: &ControlHost) {
        let Some(rect) = self.rect(tree) else {
            return;
        };
        if !tree
            .focus()
            .is_some_and(|focused| tree.focus_visible(focused))
        {
            return;
        }
        let tokens = cx.tokens;
        let ring = host.framed_ring().unwrap_or(crate::style::FramedRing {
            rect,
            radii: [tokens.radius(RadiusScale::Md); 4],
            ring: crate::style::Ring {
                width: tokens.ring_width,
                color: tokens.ring_color(tokens.color(ColorRole::Ring)),
            },
        });
        ring.paint(cx.list);
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
    /// In a control host: the size that the application gave as the
    /// value, which a drag starts from in place of the viewport.
    size: Option<Size>,
}

/// Escape, which reaches a dragging grip from anywhere in its window.
const ESCAPE: [crate::Shortcut; 1] = [crate::Shortcut {
    key: Key::Escape,
    modifiers: crate::ShortcutModifiers {
        shift: false,
        control: false,
        option: false,
        command: false,
    },
}];

impl SizeGrip {
    pub fn new(minimum: Size, maximum: Size, rtl: bool) -> Self {
        Self {
            minimum,
            maximum,
            rtl,
            start: None,
            viewport: Size::new(0.0, 0.0),
            reported: None,
            size: None,
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
        cx.cursor(bounds, style.cursor);
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
/// height))`; a `Range` value sets the size that a drag starts from, for
/// a grip that resizes a pane. Escape during a drag reports the start size
/// and ends the drag. The grip takes the direction of the host and has no
/// focus. Its node is hidden, because the grip has no semantic node.
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
                self.press(*position, self.size.unwrap_or(self.viewport));
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
            ControlEvent::Shortcut(_) => match self.start {
                Some((_, size)) => {
                    // Escape puts the size back where the drag started.
                    if self.reported.is_some() {
                        cx.emit(ControlMessage::Changed(ControlValue::Range(
                            size.width,
                            size.height,
                        )));
                    }
                    self.end_drag(cx);
                    EventReply::Stop
                }
                None => EventReply::Continue,
            },
            ControlEvent::FocusChanged(false) | ControlEvent::Dismiss if self.start.is_some() => {
                self.end_drag(cx);
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn window_shortcuts(&self) -> &[crate::Shortcut] {
        if self.start.is_some() { &ESCAPE } else { &[] }
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Range(width, height) = *value else {
            return false;
        };
        self.size = Some(Size::new(width, height));
        true
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
