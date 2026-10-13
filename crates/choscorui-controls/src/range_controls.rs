use crate::style::fill_area;
use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState, fill_shape,
};
use choscorui_core::{
    Dirty, Icon, Key, KeyEvent, LayoutDirection, NodeId, Path, Point, PointerButton, PointerEvent,
    Rect, SemanticAction, SemanticNode, SemanticRole, SemanticState, Tree, TreeError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeError {
    Reversed,
    ZeroStep,
    InvalidMaximum,
    MissingNode,
}

impl std::fmt::Display for RangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reversed => f.write_str("the minimum is greater than the maximum"),
            Self::ZeroStep => f.write_str("the step must be positive"),
            Self::InvalidMaximum => f.write_str("the maximum must be greater than zero"),
            Self::MissingNode => f.write_str("the control's node is not in the tree"),
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

    fn add(&mut self, delta: i64) -> i32 {
        let candidate = i64::from(self.value) + delta;
        self.value = candidate.clamp(i64::from(self.minimum), i64::from(self.maximum)) as i32;
        self.value
    }

    fn fraction(&self) -> f64 {
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
        }
    }

    /// Runs a horizontal slider from the right in a right-to-left layout,
    /// for pointer input. Paint reads the direction from its context.
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

    fn set_from_point(&mut self, bounds: Rect, point: Point) {
        let fraction = match self.orientation {
            SliderOrientation::Horizontal if bounds.size.width > 0.0 => {
                let fraction = (point.x - bounds.origin.x) / bounds.size.width;
                match self.direction {
                    LayoutDirection::Ltr => fraction,
                    LayoutDirection::Rtl => 1.0 - fraction,
                }
            }
            SliderOrientation::Vertical if bounds.size.height > 0.0 => {
                1.0 - (point.y - bounds.origin.y) / bounds.size.height
            }
            _ => 0.0,
        };
        self.range.set_value(self.range.value_at_fraction(fraction));
    }

    fn handle_key(&mut self, key: Key) {
        match key {
            Key::ArrowRight | Key::ArrowUp => {
                self.range.increment();
            }
            Key::ArrowLeft | Key::ArrowDown => {
                self.range.decrement();
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
                self.range.set_value(value);
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

    pub fn semantics(&self, _tree: &Tree, _id: NodeId) -> SemanticNode {
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
        let fraction = self.range.fraction().clamp(0.0, 1.0);
        let fraction = if cx.rtl && self.orientation == SliderOrientation::Horizontal {
            1.0 - fraction
        } else {
            fraction
        };
        let (track_rect, range_rect, center) = match self.orientation {
            SliderOrientation::Horizontal => {
                let t = track.min_height;
                let y = bounds.origin.y + (bounds.size.height - t) / 2.0;
                let x = bounds.origin.x + bounds.size.width * fraction;
                let (start, end) = if cx.rtl {
                    (x, bounds.max_x())
                } else {
                    (bounds.origin.x, x)
                };
                (
                    Rect::from_xywh(bounds.origin.x, y, bounds.size.width, t),
                    Rect::from_xywh(start, y, end - start, t),
                    Point::new(x, bounds.origin.y + bounds.size.height / 2.0),
                )
            }
            SliderOrientation::Vertical => {
                let t = track.min_width;
                let x = bounds.origin.x + (bounds.size.width - t) / 2.0;
                let height = bounds.size.height * fraction;
                let y = bounds.max_y() - height;
                (
                    Rect::from_xywh(x, bounds.origin.y, t, bounds.size.height),
                    Rect::from_xywh(x, y, t, height),
                    Point::new(bounds.origin.x + bounds.size.width / 2.0, y),
                )
            }
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
        // Keep the thumb inside the bounds at both ends.
        let (w, h) = (thumb.min_width, thumb.min_height);
        let x =
            (center.x - w / 2.0).clamp(bounds.origin.x, (bounds.max_x() - w).max(bounds.origin.x));
        let y =
            (center.y - h / 2.0).clamp(bounds.origin.y, (bounds.max_y() - h).max(bounds.origin.y));
        cx.slot(Rect::from_xywh(x, y, w, h), &thumb, |_, _| {});
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

/// An integer field with up and down steppers at its end.
pub struct SpinBox {
    label: String,
    range: IntegerRange,
    enabled: bool,
    direction: LayoutDirection,
}

impl SpinBox {
    pub fn new(label: impl Into<String>, range: IntegerRange) -> Self {
        Self {
            label: label.into(),
            range,
            enabled: true,
            direction: LayoutDirection::Ltr,
        }
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
    pub fn semantics(&self, _tree: &Tree, _id: NodeId) -> SemanticNode {
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
        paint_stepper_field(
            cx,
            &self.range.value().to_string(),
            state,
            self.stepper_rect(cx.bounds),
        );
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
    let icon = cx.style(StyleQuery::new(ControlKind::Input, Slot::Icon).state(state));
    let bounds = cx.bounds;
    cx.slot(bounds, &style, |cx, rect| {
        let width = (rect.size.width - steppers.size.width).max(0.0);
        let x = if cx.rtl {
            rect.max_x() - width
        } else {
            rect.origin.x
        };
        let text_rect = Rect::from_xywh(x, rect.origin.y, width, rect.size.height);
        cx.label(text_rect, text, &style, TextAlign::Start);
        let half = steppers.size.height / 2.0;
        let up = Rect::from_xywh(
            steppers.origin.x,
            steppers.origin.y,
            steppers.size.width,
            half,
        );
        let down = up.translate(0.0, half);
        let mut chevron = icon.clone();
        chevron.icon_size = icon.icon_size.min(half + 2.0);
        cx.icon(up, Icon::ChevronUp, &chevron);
        cx.icon(down, Icon::ChevronDown, &chevron);
    });
}

/// The old name of [`Progress`].
#[deprecated(since = "0.1.0", note = "renamed to `Progress`")]
pub type ProgressBar = Progress;

/// A determinate or indeterminate progress bar.
pub struct Progress {
    label: String,
    value: Option<(u32, u32)>,
}
impl Progress {
    pub fn new(label: impl Into<String>, value: u32, maximum: u32) -> Result<Self, RangeError> {
        if maximum == 0 {
            return Err(RangeError::InvalidMaximum);
        }
        Ok(Self {
            label: label.into(),
            value: Some((value.min(maximum), maximum)),
        })
    }
    pub fn indeterminate(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: None,
        }
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
            self.value = next;
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
    /// [`INDETERMINATE_PERIOD`] at `cx.time`, and holds it still when the
    /// tokens ask for reduced motion.
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
        cx.slot(bar, &track, |cx, _| {
            let (start, width) = match self.fraction() {
                Some(fraction) => (0.0, bar.size.width * fraction.clamp(0.0, 1.0)),
                None => {
                    let width = bar.size.width * 0.3;
                    let phase = if reduced {
                        0.5
                    } else {
                        (cx.time / INDETERMINATE_PERIOD).fract()
                    };
                    (-width + (bar.size.width + width) * phase, width)
                }
            };
            let x = if cx.rtl {
                bar.max_x() - start - width
            } else {
                bar.origin.x + start
            };
            // Clip the indicator to the rounded track.
            let path = Path::rounded_rect_corners(bar, track.radius.physical(cx.rtl));
            if cx.list.push_clip_path(path).is_err() {
                cx.list.push_clip(bar);
            }
            let indicator = Rect::from_xywh(x, bar.origin.y, width, bar.size.height);
            fill_area(cx.list, indicator, cx.rtl, &fill.background);
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
            state: SemanticState::default(),
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
        description: format!("Range {} to {}", range.minimum(), range.maximum()),
        value: Some(range.value().to_string()),
        state: SemanticState {
            disabled: !enabled,
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
                Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight | Key::Home | Key::End,
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

    fn arrange(&mut self, cx: &crate::MeasureContext<'_>, _bounds: Rect) {
        self.direction = cx.direction;
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

impl SpinBox {
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
            .map(|bound| cx.text.width(&bound.to_string(), &style.text))
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

    fn event(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        let Some(routed) = crate::hosted::route(cx, event) else {
            return choscorui_core::EventReply::Continue;
        };
        let input = match routed {
            crate::hosted::Routed::Pointer(event) => SpinBoxInput::Pointer(event),
            crate::hosted::Routed::Key(event) => SpinBoxInput::Key(event),
            // A spin box has no press to cancel.
            crate::hosted::Routed::Cancel => return choscorui_core::EventReply::Continue,
        };
        let node = cx.node();
        let changed = self.handle(cx.tree_mut(), node, input);
        emit_number(cx, changed);
        cx.request_paint();
        range_reply(routed)
    }

    /// The input look: a focused spin box always shows the focus ring.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx
            .state
            .set(VisualState::FOCUS_VISIBLE, cx.state.focused())
            .set(VisualState::DISABLED, cx.state.disabled() || !self.enabled);
        let steppers = Self::steppers(cx.bounds, cx.rtl);
        paint_stepper_field(cx, &self.range.value().to_string(), state, steppers);
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
        emit_number(cx, changed);
        cx.request_paint();
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
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

impl crate::CustomControl for Progress {
    /// The width it is given, or the width of an input without a width
    /// constraint, by the track height.
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        let track = cx.style(StyleQuery::new(ControlKind::Progress, Slot::Track));
        let width = crate::form::input::default_width(cx, &track);
        let size = choscorui_core::Size::new(width, track.min_height);
        choscorui_core::Measured::new(cx.constraints.constrain(size))
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
                self.value = Some((*number as u32, maximum));
                true
            }
            _ => false,
        }
    }

    /// An indeterminate bar sweeps while it is mounted and visible.
    fn animating(&self, _time: f64) -> bool {
        self.needs_frame(true)
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
