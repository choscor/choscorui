use crate::form::step_text::StepText;
use crate::style::fill_area;
use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState, fill_shape,
};
use crate::{InputFilter, NumberInput};
use choscorui_core::{
    Dirty, Icon, Key, KeyEvent, LayoutDirection, Locale, NodeId, Path, Point, PointerButton,
    PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRange, SemanticRole, SemanticState,
    Tree, TreeError,
};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeError {
    Reversed,
    ZeroStep,
    InvalidMaximum,
    MissingNode,
    /// A bound is NaN.
    NotANumber,
}

impl std::fmt::Display for RangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reversed => f.write_str("the minimum is greater than the maximum"),
            Self::ZeroStep => f.write_str("the step must be positive"),
            Self::InvalidMaximum => f.write_str("the maximum must be greater than zero"),
            Self::MissingNode => f.write_str("the control's node is not in the tree"),
            Self::NotANumber => f.write_str("a bound is not a number"),
        }
    }
}

impl std::error::Error for RangeError {}

/// Validated integer value state shared by integer range controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntegerRange {
    minimum: i32,
    maximum: i32,
    value: i32,
    step: u32,
    page_step: u32,
}

impl IntegerRange {
    /// A range from `minimum` to `maximum`, with `value` clamped into it.
    /// It fails when the bounds are reversed or a step is zero.
    pub fn new(
        minimum: i32,
        maximum: i32,
        value: i32,
        step: u32,
        page_step: u32,
    ) -> Result<Self, RangeError> {
        if minimum > maximum {
            return Err(RangeError::Reversed);
        }
        if step == 0 || page_step == 0 {
            return Err(RangeError::ZeroStep);
        }
        Ok(Self {
            minimum,
            maximum,
            value: value.clamp(minimum, maximum),
            step,
            page_step,
        })
    }

    /// The lowest value.
    pub const fn minimum(&self) -> i32 {
        self.minimum
    }
    /// The highest value.
    pub const fn maximum(&self) -> i32 {
        self.maximum
    }
    /// The current value.
    pub const fn value(&self) -> i32 {
        self.value
    }
    /// The change of one small step, for example an arrow key.
    pub const fn step(&self) -> u32 {
        self.step
    }
    /// The change of one page step, for example the Page Up key.
    pub const fn page_step(&self) -> u32 {
        self.page_step
    }

    /// Sets the value, clamped to the range, and returns the result.
    pub fn set_value(&mut self, value: i32) -> i32 {
        self.value = value.clamp(self.minimum, self.maximum);
        self.value
    }

    /// Adds one step, clamped to the range, and returns the value.
    pub fn increment(&mut self) -> i32 {
        self.add(self.step as i64)
    }
    /// Removes one step, clamped to the range, and returns the value.
    pub fn decrement(&mut self) -> i32 {
        self.add(-(self.step as i64))
    }
    /// Adds one page step, clamped to the range, and returns the value.
    pub fn page_increment(&mut self) -> i32 {
        self.add(self.page_step as i64)
    }
    /// Removes one page step, clamped to the range, and returns the value.
    pub fn page_decrement(&mut self) -> i32 {
        self.add(-(self.page_step as i64))
    }

    /// The value on the step grid from the minimum that is nearest to
    /// `value`, clamped to the range. The maximum is always reachable.
    pub fn snap(&self, value: i32) -> i32 {
        snap_to_step(
            f64::from(value),
            f64::from(self.minimum),
            f64::from(self.maximum),
            f64::from(self.step),
        ) as i32
    }

    fn add(&mut self, delta: i64) -> i32 {
        let candidate = i64::from(self.value) + delta;
        self.value = candidate.clamp(i64::from(self.minimum), i64::from(self.maximum)) as i32;
        self.value
    }

    pub(crate) fn fraction(&self) -> f64 {
        let span = i64::from(self.maximum) - i64::from(self.minimum);
        if span == 0 {
            0.0
        } else {
            (i64::from(self.value) - i64::from(self.minimum)) as f64 / span as f64
        }
    }

    fn value_at_fraction(&self, fraction: f64) -> i32 {
        let span = i64::from(self.maximum) - i64::from(self.minimum);
        let offset = (fraction.clamp(0.0, 1.0) * span as f64).round() as i64;
        (i64::from(self.minimum) + offset) as i32
    }
}

/// The value on the `step` grid from `minimum` that is nearest to
/// `value`, clamped to `minimum..=maximum` and rounded to the decimals of
/// `step`, so that 0.1 + 0.2 gives 0.3. A value at the maximum stays
/// there, also when the maximum is not on the grid.
pub(crate) fn snap_to_step(value: f64, minimum: f64, maximum: f64, step: f64) -> f64 {
    if value >= maximum {
        return maximum;
    }
    let snapped = minimum + ((value - minimum) / step).round() * step;
    let scale = 10f64.powi(step_decimals(step));
    ((snapped * scale).round() / scale).clamp(minimum, maximum)
}

/// The number of decimals of `step`, at most 9.
fn step_decimals(step: f64) -> i32 {
    (0..9)
        .find(|&decimals| {
            let scaled = step * 10f64.powi(decimals);
            (scaled - scaled.round()).abs() < 1e-9 * scaled.abs().max(1.0)
        })
        .unwrap_or(9)
}

/// The travel of a thumb of `thumb` points along a track in `bounds`: the
/// thumb center stays half a thumb inside each end. Returns the start and
/// the length along the axis.
fn travel(bounds: Rect, vertical: bool, thumb: f64) -> (f64, f64) {
    let (start, length) = if vertical {
        (bounds.origin.y, bounds.size.height)
    } else {
        (bounds.origin.x, bounds.size.width)
    };
    let inset = (thumb / 2.0).min(length / 2.0).max(0.0);
    (start + inset, length - 2.0 * inset)
}

/// The fraction of the range at `point` along a track in `bounds`, from
/// the start in reading order, or from the bottom when `vertical`. The
/// ends of the travel give 0 and 1.
pub(crate) fn track_fraction(
    bounds: Rect,
    vertical: bool,
    rtl: bool,
    thumb: f64,
    point: Point,
) -> f64 {
    let (start, length) = travel(bounds, vertical, thumb);
    if length <= 0.0 {
        return 0.0;
    }
    let along = (if vertical { point.y } else { point.x } - start) / length;
    let along = along.clamp(0.0, 1.0);
    if vertical || rtl { 1.0 - along } else { along }
}

/// The thumb center at `fraction` of a track in `bounds`, the inverse of
/// [`track_fraction`].
pub(crate) fn track_point(
    bounds: Rect,
    vertical: bool,
    rtl: bool,
    thumb: f64,
    fraction: f64,
) -> Point {
    let (start, length) = travel(bounds, vertical, thumb);
    let fraction = fraction.clamp(0.0, 1.0);
    let along = if vertical || rtl {
        1.0 - fraction
    } else {
        fraction
    };
    let at = start + length.max(0.0) * along;
    if vertical {
        Point::new(bounds.center().x, at)
    } else {
        Point::new(at, bounds.center().y)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SliderOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SliderSemanticAction {
    SetValue(i32),
    Increment,
    Decrement,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderInput {
    Pointer(PointerEvent),
    Key(KeyEvent),
    Semantic(SliderSemanticAction),
    Cancel,
}

pub struct Slider {
    label: String,
    range: IntegerRange,
    orientation: SliderOrientation,
    enabled: bool,
    dragging: bool,
    hovered: bool,
    direction: LayoutDirection,
    /// The thumb length along the track, from the last arrange.
    thumb: f64,
}

impl Slider {
    pub fn new(
        label: impl Into<String>,
        range: IntegerRange,
        orientation: SliderOrientation,
    ) -> Self {
        Self {
            label: label.into(),
            range,
            orientation,
            enabled: true,
            dragging: false,
            hovered: false,
            direction: LayoutDirection::Ltr,
            thumb: 0.0,
        }
    }

    /// Runs a horizontal slider from the right in a right-to-left layout,
    /// for pointer input and the arrow keys. Paint reads the direction
    /// from its context.
    pub fn set_direction(&mut self, direction: LayoutDirection) {
        self.direction = direction;
    }
    pub const fn value(&self) -> i32 {
        self.range.value()
    }
    pub const fn range(&self) -> IntegerRange {
        self.range
    }
    pub const fn orientation(&self) -> SliderOrientation {
        self.orientation
    }

    pub fn set_enabled(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        enabled: bool,
    ) -> Result<(), TreeError> {
        require_node(tree, id)?;
        if self.enabled == enabled {
            return Ok(());
        }
        self.enabled = enabled;
        if !enabled {
            self.dragging = false;
            self.hovered = false;
            if tree.pointer_capture() == Some(id) {
                tree.set_pointer_capture(None)?;
            }
        }
        invalidate_value(tree, id)
    }

    pub fn handle(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        input: SliderInput,
    ) -> Result<Option<i32>, TreeError> {
        let bounds = tree.bounds(id).ok_or(TreeError::MissingNode)?;
        if !self.enabled {
            return Ok(None);
        }
        let old = self.range.value();
        match input {
            SliderInput::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if tree.hit_test(position) == Some(id) => {
                tree.focus_by_pointer(id)?;
                tree.set_pointer_capture(Some(id))?;
                self.dragging = true;
                self.set_from_point(bounds, position);
            }
            SliderInput::Pointer(PointerEvent::Move { position }) => {
                let hovered = bounds.contains(position)
                    && (tree.pointer_capture() == Some(id) || tree.hit_test(position) == Some(id));
                if hovered != self.hovered {
                    self.hovered = hovered;
                    tree.invalidate(id, Dirty::PAINT)?;
                }
                if self.dragging && tree.pointer_capture() == Some(id) {
                    self.set_from_point(bounds, position);
                }
            }
            SliderInput::Pointer(PointerEvent::Up {
                position,
                button: PointerButton::Primary,
            }) if self.dragging => {
                if tree.pointer_capture() == Some(id) {
                    self.set_from_point(bounds, position);
                    tree.set_pointer_capture(None)?;
                }
                self.dragging = false;
            }
            SliderInput::Key(KeyEvent::Down { key, .. }) if tree.focus() == Some(id) => {
                self.handle_key(key)
            }
            SliderInput::Semantic(action) => self.handle_semantic(action),
            SliderInput::Cancel => {
                self.dragging = false;
                if tree.pointer_capture() == Some(id) {
                    tree.set_pointer_capture(None)?;
                }
            }
            _ => {}
        }
        changed_value(tree, id, old, self.range.value())
    }

    /// Moves the value to `point` on the thumb's travel, snapped to the
    /// step.
    fn set_from_point(&mut self, bounds: Rect, point: Point) {
        let vertical = self.orientation == SliderOrientation::Vertical;
        let rtl = self.direction == LayoutDirection::Rtl;
        let fraction = track_fraction(bounds, vertical, rtl, self.thumb, point);
        let value = self.range.value_at_fraction(fraction);
        self.range.set_value(self.range.snap(value));
    }

    fn handle_key(&mut self, key: Key) {
        let rtl = self.direction == LayoutDirection::Rtl
            && self.orientation == SliderOrientation::Horizontal;
        match key.in_reading_order(rtl) {
            Key::ArrowRight | Key::ArrowUp => {
                self.range.increment();
            }
            Key::ArrowLeft | Key::ArrowDown => {
                self.range.decrement();
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
    }

    fn handle_semantic(&mut self, action: SliderSemanticAction) {
        match action {
            SliderSemanticAction::SetValue(value) => {
                self.range.set_value(self.range.snap(value));
            }
            SliderSemanticAction::Increment => {
                self.range.increment();
            }
            SliderSemanticAction::Decrement => {
                self.range.decrement();
            }
        }
    }

    /// The flags the slider recipe reads. A drag counts as pressed.
    pub fn visual_state(&self, tree: &Tree, id: NodeId) -> VisualState {
        VisualState::NONE
            .set(VisualState::DISABLED, !self.enabled)
            .set(VisualState::HOVERED, self.hovered || self.dragging)
            .set(VisualState::PRESSED, self.dragging)
            .set(VisualState::FOCUSED, tree.focus() == Some(id))
            .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(id))
    }

    /// The semantics, with the value, the bounds, and the step.
    pub fn semantics(&self) -> SemanticNode {
        range_semantics(SemanticRole::Slider, &self.label, self.range, self.enabled)
    }

    /// Paints the track, the filled range, and the thumb in the context's
    /// bounds. The value runs from the start in reading order, or from the
    /// bottom when vertical.
    pub fn paint(&self, cx: &mut PaintContext<'_>, tree: &Tree, id: NodeId) {
        let state = self.visual_state(tree, id);
        self.paint_state(cx, state);
    }

    /// Paints the slider in an explicit visual state.
    pub fn paint_state(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        let query = |slot| StyleQuery::new(ControlKind::Slider, slot).state(state);
        let (track, range, thumb) = (
            cx.style(query(Slot::Track)),
            cx.style(query(Slot::Range)),
            cx.style(query(Slot::Thumb)),
        );
        let bounds = cx.bounds;
        let vertical = self.orientation == SliderOrientation::Vertical;
        let along = if vertical {
            thumb.min_height
        } else {
            thumb.min_width
        };
        let center = track_point(bounds, vertical, cx.rtl, along, self.range.fraction());
        let (track_rect, range_rect) = if vertical {
            let t = track.min_width;
            let x = bounds.origin.x + (bounds.size.width - t) / 2.0;
            (
                Rect::from_xywh(x, bounds.origin.y, t, bounds.size.height),
                Rect::from_xywh(x, center.y, t, bounds.max_y() - center.y),
            )
        } else {
            let t = track.min_height;
            let y = bounds.origin.y + (bounds.size.height - t) / 2.0;
            let (start, end) = if cx.rtl {
                (center.x, bounds.max_x())
            } else {
                (bounds.origin.x, center.x)
            };
            (
                Rect::from_xywh(bounds.origin.x, y, bounds.size.width, t),
                Rect::from_xywh(start, y, end - start, t),
            )
        };
        cx.slot(track_rect, &track, |_, _| {});
        let faded = range.opacity < 1.0;
        if faded {
            let _ = cx.list.push_opacity(range.opacity);
        }
        fill_shape(
            cx.list,
            range_rect,
            &range.radius,
            cx.rtl,
            &range.background,
        );
        if faded {
            let _ = cx.list.pop_opacity();
        }
        // The travel keeps the thumb inside the bounds at both ends.
        let (w, h) = (thumb.min_width, thumb.min_height);
        let knob = Rect::from_xywh(center.x - w / 2.0, center.y - h / 2.0, w, h);
        cx.slot(knob, &thumb, |_, _| {});
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpinBoxSemanticAction {
    SetValue(i32),
    Increment,
    Decrement,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpinBoxInput {
    Pointer(PointerEvent),
    Key(KeyEvent),
    Semantic(SpinBoxSemanticAction),
}

/// The width of the stepper column at the end of a spin box.
pub const STEPPER_WIDTH: f64 = 24.0;

/// An integer field with up and down steppers at its end. In a control
/// host, its text edits like an input.
pub struct SpinBox {
    label: String,
    range: IntegerRange,
    enabled: bool,
    direction: LayoutDirection,
    /// The editable text in a control host.
    text: StepText,
    /// The locale that formats and reads the text.
    locale: Locale,
}

impl SpinBox {
    pub fn new(label: impl Into<String>, range: IntegerRange) -> Self {
        let label = label.into();
        let locale = Locale::root();
        let text = StepText::new(
            &label,
            &locale.format_integer(range.value().into()),
            integer_filter(range, locale.clone()),
        );
        Self {
            label,
            range,
            enabled: true,
            direction: LayoutDirection::Ltr,
            text,
            locale,
        }
    }

    /// Formats and reads the text with the digit grouping of `locale`.
    #[must_use]
    pub fn with_locale(mut self, locale: Locale) -> Self {
        self.text
            .set_filter(integer_filter(self.range, locale.clone()));
        self.locale = locale;
        self.text.set(&self.formatted());
        self
    }

    /// The value as the text shows it.
    fn formatted(&self) -> String {
        self.locale.format_integer(self.range.value().into())
    }

    /// Puts the steppers at the end in reading order: at the left in a
    /// right-to-left layout.
    pub fn set_direction(&mut self, direction: LayoutDirection) {
        self.direction = direction;
    }

    /// The stepper column inside `bounds`.
    pub fn stepper_rect(&self, bounds: Rect) -> Rect {
        let width = STEPPER_WIDTH.min(bounds.size.width);
        let x = match self.direction {
            LayoutDirection::Ltr => bounds.max_x() - width,
            LayoutDirection::Rtl => bounds.origin.x,
        };
        Rect::from_xywh(x, bounds.origin.y, width, bounds.size.height)
    }
    pub const fn value(&self) -> i32 {
        self.range.value()
    }
    pub const fn range(&self) -> IntegerRange {
        self.range
    }
    pub fn set_enabled(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        enabled: bool,
    ) -> Result<(), TreeError> {
        require_node(tree, id)?;
        if self.enabled != enabled {
            self.enabled = enabled;
            invalidate_value(tree, id)?;
        }
        Ok(())
    }
    pub fn handle(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        input: SpinBoxInput,
    ) -> Result<Option<i32>, TreeError> {
        let bounds = tree.bounds(id).ok_or(TreeError::MissingNode)?;
        if !self.enabled {
            return Ok(None);
        }
        let old = self.range.value();
        match input {
            SpinBoxInput::Key(KeyEvent::Down { key, .. }) if tree.focus() == Some(id) => {
                match key {
                    Key::ArrowUp => {
                        self.range.increment();
                    }
                    Key::ArrowDown => {
                        self.range.decrement();
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
            }
            SpinBoxInput::Semantic(action) => match action {
                SpinBoxSemanticAction::SetValue(value) => {
                    self.range.set_value(value);
                }
                SpinBoxSemanticAction::Increment => {
                    self.range.increment();
                }
                SpinBoxSemanticAction::Decrement => {
                    self.range.decrement();
                }
            },
            SpinBoxInput::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if tree.hit_test(position) == Some(id) => {
                tree.focus_by_pointer(id)?;
                if self.stepper_rect(bounds).contains(position) {
                    if position.y < bounds.origin.y + bounds.size.height / 2.0 {
                        self.range.increment();
                    } else {
                        self.range.decrement();
                    }
                }
            }
            _ => {}
        }
        changed_value(tree, id, old, self.range.value())
    }
    /// The flags the input recipe reads.
    pub fn visual_state(&self, tree: &Tree, id: NodeId) -> VisualState {
        VisualState::NONE
            .set(VisualState::DISABLED, !self.enabled)
            .set(VisualState::FOCUSED, tree.focus() == Some(id))
            .set(VisualState::FOCUS_VISIBLE, tree.focus() == Some(id))
    }
    /// The semantics, with the value, the bounds, and the step.
    pub fn semantics(&self) -> SemanticNode {
        range_semantics(
            SemanticRole::SpinButton,
            &self.label,
            self.range,
            self.enabled,
        )
    }

    /// Paints the input box, the value at the start, and the steppers at
    /// the end.
    pub fn paint(&self, cx: &mut PaintContext<'_>, tree: &Tree, id: NodeId) {
        let state = self.visual_state(tree, id);
        paint_stepper_field(cx, &self.formatted(), state, self.stepper_rect(cx.bounds));
    }
}

/// Paints an input box with `text` at the start and a stepper column with
/// up and down chevrons at `steppers`.
pub(crate) fn paint_stepper_field(
    cx: &mut PaintContext<'_>,
    text: &str,
    state: VisualState,
    steppers: Rect,
) {
    let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root).state(state));
    let arrow = VisualState::NONE.set(VisualState::DISABLED, state.disabled());
    paint_stepper_box(cx, state, steppers, [arrow; 2], |cx, rect| {
        cx.label(rect, text, &style, TextAlign::Start);
    });
}

/// Paints an input box with a stepper column with up and down chevrons
/// at `steppers` in the states `arrows`. `text` paints the text in the
/// rest of the box.
pub(crate) fn paint_stepper_box(
    cx: &mut PaintContext<'_>,
    state: VisualState,
    steppers: Rect,
    arrows: [VisualState; 2],
    text: impl FnOnce(&mut PaintContext<'_>, Rect),
) {
    let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root).state(state));
    let bounds = cx.bounds;
    cx.slot(bounds, &style, |cx, rect| {
        let width = (rect.size.width - steppers.size.width).max(0.0);
        let x = if cx.rtl {
            rect.max_x() - width
        } else {
            rect.origin.x
        };
        text(
            cx,
            Rect::from_xywh(x, rect.origin.y, width, rect.size.height),
        );
        let half = steppers.size.height / 2.0;
        let up = Rect::from_xywh(
            steppers.origin.x,
            steppers.origin.y,
            steppers.size.width,
            half,
        );
        let down = up.translate(0.0, half);
        for ((rect, glyph), arrow) in [(up, Icon::ChevronUp), (down, Icon::ChevronDown)]
            .into_iter()
            .zip(arrows)
        {
            let query = StyleQuery::new(ControlKind::Input, Slot::Icon).state(arrow);
            let mut chevron = cx.style(query);
            chevron.icon_size = chevron.icon_size.min(half + 2.0);
            cx.slot(rect, &chevron, |cx, rect| cx.icon(rect, glyph, &chevron));
        }
    });
}

/// The old name of [`Progress`].
#[deprecated(since = "0.1.0", note = "renamed to `Progress`")]
pub type ProgressBar = Progress;

/// A determinate or indeterminate progress bar. In a control host, a new
/// value moves the fill over the `motion_ms` of the `Indicator` recipe.
pub struct Progress {
    label: String,
    value: Option<(u32, u32)>,
    /// The fill animation: the fraction that it starts from, and the host
    /// time that it starts at. The first paint after a change sets the
    /// time.
    fill: Cell<(f64, Option<f64>)>,
    /// The fraction that the last paint showed.
    shown: Cell<f64>,
    /// The fill duration, from the last arrange.
    motion_ms: u32,
}
impl Progress {
    pub fn new(label: impl Into<String>, value: u32, maximum: u32) -> Result<Self, RangeError> {
        if maximum == 0 {
            return Err(RangeError::InvalidMaximum);
        }
        let value = value.min(maximum);
        let fraction = f64::from(value) / f64::from(maximum);
        Ok(Self {
            label: label.into(),
            value: Some((value, maximum)),
            fill: Cell::new((fraction, Some(f64::NEG_INFINITY))),
            shown: Cell::new(fraction),
            motion_ms: 0,
        })
    }
    pub fn indeterminate(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: None,
            fill: Cell::new((0.0, Some(f64::NEG_INFINITY))),
            shown: Cell::new(0.0),
            motion_ms: 0,
        }
    }

    /// Sets a determinate value; the fill moves from where it shows now.
    fn change(&mut self, value: u32, maximum: u32) {
        self.value = Some((value, maximum));
        self.fill.set((self.shown.get(), None));
    }

    /// The fill fraction to show at `time`: on the way from the start of
    /// the fill animation to the value.
    fn shown_at(&self, tokens: &choscorui_core::DesignTokens, time: f64) -> f64 {
        let target = self.fraction().unwrap_or(0.0).clamp(0.0, 1.0);
        let (from, started) = self.fill.get();
        let started = started.unwrap_or(time);
        self.fill.set((from, Some(started)));
        let linear = if tokens.motion.reduced || self.motion_ms == 0 {
            1.0
        } else {
            ((time - started) * 1000.0 / f64::from(self.motion_ms)).clamp(0.0, 1.0)
        };
        let shown = from + (target - from) * tokens.motion.easing.solve(linear);
        self.shown.set(shown);
        shown
    }
    pub const fn value(&self) -> Option<(u32, u32)> {
        self.value
    }
    pub fn set_value(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        value: u32,
        maximum: u32,
    ) -> Result<(), RangeError> {
        if maximum == 0 {
            return Err(RangeError::InvalidMaximum);
        }
        if !tree.contains(id) {
            return Err(RangeError::MissingNode);
        }
        let next = Some((value.min(maximum), maximum));
        if self.value != next {
            self.change(value.min(maximum), maximum);
            invalidate_value(tree, id).map_err(|_| RangeError::MissingNode)?;
        }
        Ok(())
    }
    pub fn set_indeterminate(&mut self, tree: &mut Tree, id: NodeId) -> Result<(), TreeError> {
        require_node(tree, id)?;
        if self.value.take().is_some() {
            invalidate_value(tree, id)?;
        }
        Ok(())
    }
    /// Indeterminate bars animate only while visible; a hidden or
    /// determinate bar requests no frames.
    pub fn needs_frame(&self, visible: bool) -> bool {
        visible && self.value.is_none()
    }
    /// The filled fraction, or `None` while indeterminate.
    pub fn fraction(&self) -> Option<f64> {
        self.value
            .map(|(value, maximum)| f64::from(value) / f64::from(maximum))
    }

    /// Paints the track and the indicator in the context's bounds. An
    /// indeterminate bar sweeps a segment once per
    /// [`INDETERMINATE_PERIOD`] at `cx.time`. With reduced motion, it
    /// shows still stripes over the whole track, which show no value.
    pub fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx.state;
        let track = cx.style(StyleQuery::new(ControlKind::Progress, Slot::Track).state(state));
        let fill = cx.style(StyleQuery::new(ControlKind::Progress, Slot::Indicator).state(state));
        let bounds = cx.bounds;
        let height = track.min_height.min(bounds.size.height);
        let bar = Rect::from_xywh(
            bounds.origin.x,
            bounds.origin.y + (bounds.size.height - height) / 2.0,
            bounds.size.width,
            height,
        );
        let reduced = cx.tokens.motion.reduced;
        let shown = self.shown_at(cx.tokens, cx.time);
        cx.slot(bar, &track, |cx, _| {
            // Each segment is a start and a width along the bar.
            let segments: Vec<(f64, f64)> = match self.fraction() {
                Some(_) => vec![(0.0, bar.size.width * shown)],
                None if reduced => {
                    // Stripes of twice the bar height, with gaps of the
                    // same width.
                    let stripe = (bar.size.height * 2.0).max(1.0);
                    let count = (bar.size.width / (stripe * 2.0)).ceil() as usize;
                    (0..count)
                        .map(|index| (index as f64 * stripe * 2.0, stripe))
                        .collect()
                }
                None => {
                    let width = bar.size.width * 0.3;
                    let phase = (cx.time / INDETERMINATE_PERIOD).fract();
                    vec![(-width + (bar.size.width + width) * phase, width)]
                }
            };
            // Clip the indicator to the rounded track.
            let path = Path::rounded_rect_corners(bar, track.radius.physical(cx.rtl));
            if cx.list.push_clip_path(path).is_err() {
                cx.list.push_clip(bar);
            }
            for (start, width) in segments {
                let x = if cx.rtl {
                    bar.max_x() - start - width
                } else {
                    bar.origin.x + start
                };
                let indicator = Rect::from_xywh(x, bar.origin.y, width, bar.size.height);
                fill_area(cx.list, indicator, cx.rtl, &fill.background);
            }
            let _ = cx.list.pop_clip();
        });
    }

    pub fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::ProgressIndicator,
            name: self.label.clone(),
            description: String::new(),
            value: self.value.map(|(value, maximum)| {
                format!("{}%", u64::from(value) * 100 / u64::from(maximum))
            }),
            state: SemanticState {
                range: self.value.map(|(value, maximum)| SemanticRange {
                    value: value.into(),
                    minimum: 0,
                    maximum: maximum.into(),
                    step: 1,
                }),
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        }
    }
}

/// One indeterminate sweep, in seconds.
pub const INDETERMINATE_PERIOD: f64 = 1.2;

fn range_semantics(
    role: SemanticRole,
    label: &str,
    range: IntegerRange,
    enabled: bool,
) -> SemanticNode {
    SemanticNode {
        role,
        name: label.to_owned(),
        description: String::new(),
        value: Some(range.value().to_string()),
        state: SemanticState {
            disabled: !enabled,
            range: Some(SemanticRange {
                value: range.value().into(),
                minimum: range.minimum().into(),
                maximum: range.maximum().into(),
                step: range.step().into(),
            }),
            ..SemanticState::default()
        },
        relations: Vec::new(),
        actions: if enabled {
            vec![
                SemanticAction::SetValue,
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

/// The integer that an assistive `SetValue` string or a
/// [`ControlValue::Number`](crate::ControlValue::Number) names, rounded
/// and clamped to `i32`. Not-a-number and infinite values name none.
fn integer(value: f64) -> Option<i32> {
    value.is_finite().then(|| {
        value
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
    })
}

/// The integer in an assistive `SetValue` string.
fn parse_integer(value: Option<&str>) -> Option<i32> {
    integer(value?.trim().parse::<f64>().ok()?)
}

/// The reply for a routed event of a range control: presses and
/// releases, and the keys that step the value, stop.
fn range_reply(routed: crate::hosted::Routed) -> choscorui_core::EventReply {
    use crate::hosted::Routed;
    match routed {
        Routed::Pointer(PointerEvent::Down { .. } | PointerEvent::Up { .. })
        | Routed::Key(KeyEvent::Down {
            key:
                Key::ArrowUp
                | Key::ArrowDown
                | Key::ArrowLeft
                | Key::ArrowRight
                | Key::PageUp
                | Key::PageDown
                | Key::Home
                | Key::End,
            ..
        }) => choscorui_core::EventReply::Stop,
        _ => choscorui_core::EventReply::Continue,
    }
}

/// Reports a changed integer value as `Changed(Number)`.
fn emit_number(cx: &mut crate::ControlContext<'_>, changed: Result<Option<i32>, TreeError>) {
    if let Ok(Some(value)) = changed {
        cx.emit(crate::ControlMessage::Changed(crate::ControlValue::Number(
            f64::from(value),
        )));
        cx.request_semantics();
    }
}

impl crate::CustomControl for Slider {
    /// The recipe length along the track and the thumb size across it.
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        let root = cx.style(StyleQuery::new(ControlKind::Slider, Slot::Root));
        let size = match self.orientation {
            SliderOrientation::Horizontal => {
                choscorui_core::Size::new(root.min_width, root.min_height)
            }
            SliderOrientation::Vertical => {
                choscorui_core::Size::new(root.min_height, root.min_width)
            }
        };
        choscorui_core::Measured::new(cx.constraints.constrain(size))
    }

    /// Records the direction and the thumb length, so that the pointer
    /// maps onto the travel that paint draws.
    fn arrange(&mut self, cx: &crate::MeasureContext<'_>, _bounds: Rect) {
        self.direction = cx.direction;
        let thumb = cx.style(StyleQuery::new(ControlKind::Slider, Slot::Thumb));
        self.thumb = match self.orientation {
            SliderOrientation::Horizontal => thumb.min_width,
            SliderOrientation::Vertical => thumb.min_height,
        };
    }

    fn event(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        if *event == crate::ControlEvent::Hover(false) && self.hovered {
            self.hovered = false;
            cx.request_paint();
        }
        let Some(routed) = crate::hosted::route(cx, event) else {
            return choscorui_core::EventReply::Continue;
        };
        let input = match routed {
            crate::hosted::Routed::Pointer(event) => SliderInput::Pointer(event),
            crate::hosted::Routed::Key(event) => SliderInput::Key(event),
            crate::hosted::Routed::Cancel => SliderInput::Cancel,
        };
        let node = cx.node();
        let changed = self.handle(cx.tree_mut(), node, input);
        emit_number(cx, changed);
        cx.request_paint();
        range_reply(routed)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx
            .state
            .set(VisualState::HOVERED, cx.state.hovered() || self.dragging)
            .set(VisualState::PRESSED, self.dragging)
            .set(VisualState::DISABLED, cx.state.disabled() || !self.enabled);
        self.paint_state(cx, state);
    }

    fn semantics(&self) -> SemanticNode {
        range_semantics(SemanticRole::Slider, &self.label, self.range, self.enabled)
    }

    fn perform(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        let semantic = match action {
            SemanticAction::SetValue => match parse_integer(value) {
                Some(value) => SliderSemanticAction::SetValue(value),
                None => return false,
            },
            SemanticAction::Increment => SliderSemanticAction::Increment,
            SemanticAction::Decrement => SliderSemanticAction::Decrement,
            SemanticAction::Focus => {
                cx.request_focus();
                return true;
            }
            _ => return false,
        };
        let node = cx.node();
        let changed = self.handle(cx.tree_mut(), node, SliderInput::Semantic(semantic));
        emit_number(cx, changed);
        cx.request_paint();
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.dragging = false;
            self.hovered = false;
        }
    }

    fn set_value(&mut self, value: &crate::ControlValue) -> bool {
        match value {
            crate::ControlValue::Number(value) => integer(*value).is_some_and(|value| {
                self.range.set_value(value);
                true
            }),
            _ => false,
        }
    }
}

/// The filter of a spin box's text: whole numbers in `locale`, with a
/// minus when the range has negative values.
fn integer_filter(range: IntegerRange, locale: Locale) -> InputFilter {
    NumberInput::new(locale, 0)
        .with_range(
            Some(f64::from(range.minimum())),
            Some(f64::from(range.maximum())),
        )
        .filter()
}

impl SpinBox {
    /// The value of the edited text, clamped to the range, or `None`
    /// when the text is no number.
    fn pending(&self) -> Option<i32> {
        let value = self.locale.parse_integer(self.text.text().trim()).ok()?;
        let value = value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        Some(value.clamp(self.range.minimum(), self.range.maximum()))
    }

    /// Sets the value from the edited text, clamped to the range, and
    /// formats the text. Text that is no number gives way to the value.
    fn commit(&mut self, cx: &mut crate::ControlContext<'_>) {
        if let Some(value) = self.pending()
            && value != self.range.value()
        {
            self.range.set_value(value);
            cx.emit(crate::ControlMessage::Changed(crate::ControlValue::Number(
                f64::from(value),
            )));
        }
        self.text.set(&self.formatted());
        cx.request_paint();
        cx.request_semantics();
    }

    /// Takes the pending text and then steps with `key`. A step past the
    /// end of the range beeps.
    fn step(&mut self, cx: &mut crate::ControlContext<'_>, key: Key) {
        let before = self.range.value();
        if let Some(value) = self.pending() {
            self.range.set_value(value);
        }
        let pending = self.range.value();
        match key {
            Key::ArrowUp => self.range.increment(),
            Key::ArrowDown => self.range.decrement(),
            Key::PageUp => self.range.page_increment(),
            Key::PageDown => self.range.page_decrement(),
            Key::Home => self.range.set_value(self.range.minimum()),
            _ => self.range.set_value(self.range.maximum()),
        };
        if self.range.value() == pending {
            cx.beep();
        }
        if self.range.value() != before {
            cx.emit(crate::ControlMessage::Changed(crate::ControlValue::Number(
                f64::from(self.range.value()),
            )));
        }
        self.text.set(&self.formatted());
        cx.request_paint();
        cx.request_semantics();
    }

    /// The stepper column inside `bounds` for the reading direction.
    fn steppers(bounds: Rect, rtl: bool) -> Rect {
        let width = STEPPER_WIDTH.min(bounds.size.width);
        let x = if rtl {
            bounds.origin.x
        } else {
            bounds.max_x() - width
        };
        Rect::from_xywh(x, bounds.origin.y, width, bounds.size.height)
    }
}

impl crate::CustomControl for SpinBox {
    /// The widest bound as text, the input padding, and the steppers, at
    /// least the input recipe's size.
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root));
        let text = [self.range.minimum(), self.range.maximum()]
            .iter()
            .map(|&bound| {
                cx.text
                    .width(&self.locale.format_integer(bound.into()), &style.text)
            })
            .fold(0.0, f64::max);
        let size = choscorui_core::Size::new(
            (text + style.padding.horizontal() + STEPPER_WIDTH).max(style.min_width),
            (style.text.line_height + style.padding.vertical()).max(style.min_height),
        );
        choscorui_core::Measured::new(cx.constraints.constrain(size))
    }

    fn arrange(&mut self, cx: &crate::MeasureContext<'_>, _bounds: Rect) {
        self.direction = cx.direction;
    }

    /// The text edits like an input, through the integer filter. Focus
    /// selects the whole text. Arrows, Page keys, Home, End, and the
    /// steppers take the pending text and then step; a step past the end
    /// of the range beeps. A held stepper repeats. Return and a loss of
    /// focus commit the text, clamped to the range.
    fn event(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        use crate::ControlEvent;
        use choscorui_core::EventReply;
        if cx.phase() != choscorui_core::EventPhase::Target {
            return EventReply::Continue;
        }
        let rtl = self.direction == LayoutDirection::Rtl;
        let steppers = Self::steppers(cx.bounds(), rtl);
        if let Some(key) = self.text.track_steppers(cx, event, steppers) {
            self.step(cx, key);
            cx.request_focus();
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
                    ) =>
            {
                self.step(cx, *key);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Enter, ..
            }) if cx.is_focused() => {
                self.commit(cx);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Escape, ..
            }) if cx.is_focused() => {
                self.text.set(&self.formatted());
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::FocusChanged(true) => {
                self.text.select_all();
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::FocusChanged(false) => {
                self.commit(cx);
                EventReply::Continue
            }
            _ => {
                let rect = crate::form::step_text::text_rect(cx.bounds(), steppers);
                let (reply, edited) = self.text.event(cx, event, rect);
                if edited {
                    cx.request_semantics();
                }
                reply
            }
        }
    }

    /// The input look: a focused spin box always shows the focus ring.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx
            .state
            .set(VisualState::FOCUS_VISIBLE, cx.state.focused())
            .set(VisualState::DISABLED, cx.state.disabled() || !self.enabled);
        let steppers = Self::steppers(cx.bounds, cx.rtl);
        let range = self.range;
        let ends = (
            range.value() >= range.maximum(),
            range.value() <= range.minimum(),
        );
        self.text.paint(cx, self.text.state(state), steppers, ends);
    }

    fn deadline(&self) -> Option<f64> {
        self.text.deadline()
    }

    fn semantics(&self) -> SemanticNode {
        range_semantics(
            SemanticRole::SpinButton,
            &self.label,
            self.range,
            self.enabled,
        )
    }

    fn perform(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        let semantic = match action {
            SemanticAction::SetValue => match parse_integer(value) {
                Some(value) => SpinBoxSemanticAction::SetValue(value),
                None => return false,
            },
            SemanticAction::Increment => SpinBoxSemanticAction::Increment,
            SemanticAction::Decrement => SpinBoxSemanticAction::Decrement,
            SemanticAction::Focus => {
                cx.request_focus();
                return true;
            }
            _ => return false,
        };
        let node = cx.node();
        let changed = self.handle(cx.tree_mut(), node, SpinBoxInput::Semantic(semantic));
        self.text.set(&self.formatted());
        emit_number(cx, changed);
        cx.request_paint();
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.text.set_enabled(enabled);
    }

    fn set_value(&mut self, value: &crate::ControlValue) -> bool {
        match value {
            crate::ControlValue::Number(value) => integer(*value).is_some_and(|value| {
                self.range.set_value(value);
                self.text.set(&self.formatted());
                true
            }),
            _ => false,
        }
    }
}

impl crate::CustomControl for Progress {
    /// The width it is given, or the width of an input without a width
    /// constraint, by the track height.
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        let track = cx.style(StyleQuery::new(ControlKind::Progress, Slot::Track));
        let width = crate::form::input::default_width(cx, &track);
        let size = choscorui_core::Size::new(width, track.min_height);
        choscorui_core::Measured::new(cx.constraints.constrain(size))
    }

    fn arrange(&mut self, cx: &crate::MeasureContext<'_>, _bounds: Rect) {
        self.motion_ms = cx
            .style(StyleQuery::new(ControlKind::Progress, Slot::Indicator))
            .motion_ms;
    }

    fn event(
        &mut self,
        _cx: &mut crate::ControlContext<'_>,
        _event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        choscorui_core::EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        Progress::paint(self, cx);
    }

    fn semantics(&self) -> SemanticNode {
        Progress::semantics(self)
    }

    fn focusable(&self) -> bool {
        false
    }

    /// Takes a whole `Number` from 0 to the maximum of a determinate bar.
    fn set_value(&mut self, value: &crate::ControlValue) -> bool {
        match (value, self.value) {
            (crate::ControlValue::Number(number), Some((_, maximum)))
                if number.fract() == 0.0 && (0.0..=f64::from(maximum)).contains(number) =>
            {
                if self.value != Some((*number as u32, maximum)) {
                    self.change(*number as u32, maximum);
                }
                true
            }
            _ => false,
        }
    }

    /// An indeterminate bar sweeps while it is mounted and visible. A
    /// determinate fill animates after a change, until it reaches the
    /// value.
    fn animating(&self, time: f64) -> bool {
        let (_, started) = self.fill.get();
        let filling =
            started.is_none_or(|started| (time - started) * 1000.0 < f64::from(self.motion_ms));
        self.needs_frame(true) || filling
    }
}

fn require_node(tree: &Tree, id: NodeId) -> Result<(), TreeError> {
    tree.contains(id)
        .then_some(())
        .ok_or(TreeError::MissingNode)
}
fn invalidate_value(tree: &mut Tree, id: NodeId) -> Result<(), TreeError> {
    tree.invalidate(id, Dirty::PAINT)?;
    tree.invalidate(id, Dirty::SEMANTICS)
}
fn changed_value(
    tree: &mut Tree,
    id: NodeId,
    old: i32,
    new: i32,
) -> Result<Option<i32>, TreeError> {
    if old == new {
        Ok(None)
    } else {
        invalidate_value(tree, id)?;
        Ok(Some(new))
    }
}
