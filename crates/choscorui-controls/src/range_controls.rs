use choscorui_core::{
    Color, Dirty, DisplayList, Key, KeyEvent, NodeId, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, SemanticState, Size, Tree, TreeError,
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

    pub const fn minimum(&self) -> i32 {
        self.minimum
    }
    pub const fn maximum(&self) -> i32 {
        self.maximum
    }
    pub const fn value(&self) -> i32 {
        self.value
    }
    pub const fn step(&self) -> u32 {
        self.step
    }
    pub const fn page_step(&self) -> u32 {
        self.page_step
    }

    pub fn set_value(&mut self, value: i32) -> i32 {
        self.value = value.clamp(self.minimum, self.maximum);
        self.value
    }

    pub fn increment(&mut self) -> i32 {
        self.add(self.step as i64)
    }
    pub fn decrement(&mut self) -> i32 {
        self.add(-(self.step as i64))
    }
    pub fn page_increment(&mut self) -> i32 {
        self.add(self.page_step as i64)
    }
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderVisualState {
    pub enabled: bool,
    pub focused: bool,
    pub dragging: bool,
    pub fraction: f64,
    pub orientation: SliderOrientation,
}

pub struct Slider {
    label: String,
    range: IntegerRange,
    orientation: SliderOrientation,
    enabled: bool,
    dragging: bool,
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
        }
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
        if self.enabled == enabled {
            return Ok(());
        }
        self.enabled = enabled;
        if !enabled {
            self.dragging = false;
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
                tree.set_focus(Some(id))?;
                tree.set_pointer_capture(Some(id))?;
                self.dragging = true;
                self.set_from_point(bounds, position);
            }
            SliderInput::Pointer(PointerEvent::Move { position })
                if self.dragging && tree.pointer_capture() == Some(id) =>
            {
                self.set_from_point(bounds, position)
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
                (point.x - bounds.origin.x) / bounds.size.width
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

    pub fn visual_state(&self, tree: &Tree, id: NodeId) -> SliderVisualState {
        SliderVisualState {
            enabled: self.enabled,
            focused: tree.focus() == Some(id),
            dragging: self.dragging,
            fraction: self.range.fraction(),
            orientation: self.orientation,
        }
    }

    pub fn semantics(&self, _tree: &Tree, _id: NodeId) -> SemanticNode {
        range_semantics(SemanticRole::Slider, &self.label, self.range, self.enabled)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SliderStyle {
    pub track: Color,
    pub fill: Color,
    pub thumb: Color,
    pub disabled: Color,
    pub focus: Color,
    pub track_thickness: f64,
    pub thumb_size: f64,
}

impl SliderStyle {
    pub fn paint(&self, list: &mut DisplayList, bounds: Rect, state: SliderVisualState) {
        let track_color = if state.enabled {
            self.track
        } else {
            self.disabled
        };
        let t = self.track_thickness.max(0.0);
        let fraction = state.fraction.clamp(0.0, 1.0);
        let (track, fill, thumb) = match state.orientation {
            SliderOrientation::Horizontal => {
                let y = bounds.origin.y + (bounds.size.height - t) / 2.0;
                let x = bounds.origin.x + bounds.size.width * fraction;
                (
                    Rect::new(
                        Point::new(bounds.origin.x, y),
                        Size::new(bounds.size.width, t),
                    ),
                    Rect::new(
                        Point::new(bounds.origin.x, y),
                        Size::new(bounds.size.width * fraction, t),
                    ),
                    Rect::new(
                        Point::new(
                            x - self.thumb_size / 2.0,
                            bounds.origin.y + (bounds.size.height - self.thumb_size) / 2.0,
                        ),
                        Size::new(self.thumb_size, self.thumb_size),
                    ),
                )
            }
            SliderOrientation::Vertical => {
                let x = bounds.origin.x + (bounds.size.width - t) / 2.0;
                let height = bounds.size.height * fraction;
                let y = bounds.origin.y + bounds.size.height - height;
                (
                    Rect::new(
                        Point::new(x, bounds.origin.y),
                        Size::new(t, bounds.size.height),
                    ),
                    Rect::new(Point::new(x, y), Size::new(t, height)),
                    Rect::new(
                        Point::new(
                            bounds.origin.x + (bounds.size.width - self.thumb_size) / 2.0,
                            y - self.thumb_size / 2.0,
                        ),
                        Size::new(self.thumb_size, self.thumb_size),
                    ),
                )
            }
        };
        list.fill_rect(track, track_color);
        if state.enabled {
            list.fill_rect(fill, self.fill);
        }
        list.fill_rect(
            thumb,
            if state.enabled {
                self.thumb
            } else {
                self.disabled
            },
        );
        if state.focused {
            paint_focus(list, bounds, self.focus);
        }
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpinBoxVisualState {
    pub enabled: bool,
    pub focused: bool,
}

pub struct SpinBox {
    label: String,
    range: IntegerRange,
    enabled: bool,
}

impl SpinBox {
    pub fn new(label: impl Into<String>, range: IntegerRange) -> Self {
        Self {
            label: label.into(),
            range,
            enabled: true,
        }
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
                tree.set_focus(Some(id))?;
                if position.x >= bounds.origin.x + bounds.size.width * 0.75 {
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
    pub fn visual_state(&self, tree: &Tree, id: NodeId) -> SpinBoxVisualState {
        SpinBoxVisualState {
            enabled: self.enabled,
            focused: tree.focus() == Some(id),
        }
    }
    pub fn semantics(&self, _tree: &Tree, _id: NodeId) -> SemanticNode {
        range_semantics(
            SemanticRole::SpinButton,
            &self.label,
            self.range,
            self.enabled,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SpinBoxStyle {
    pub background: Color,
    pub buttons: Color,
    pub disabled: Color,
    pub focus: Color,
    pub button_width: f64,
}
impl SpinBoxStyle {
    pub fn paint(&self, list: &mut DisplayList, bounds: Rect, state: SpinBoxVisualState) {
        list.fill_rect(
            bounds,
            if state.enabled {
                self.background
            } else {
                self.disabled
            },
        );
        if state.enabled {
            let width = self.button_width.clamp(0.0, bounds.size.width);
            list.fill_rect(
                Rect::new(
                    Point::new(bounds.origin.x + bounds.size.width - width, bounds.origin.y),
                    Size::new(width, bounds.size.height),
                ),
                self.buttons,
            );
        }
        if state.focused {
            paint_focus(list, bounds, self.focus);
        }
    }
}

pub struct ProgressBar {
    label: String,
    value: Option<(u32, u32)>,
}
impl ProgressBar {
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
    pub fn visual_state(&self) -> ProgressVisualState {
        ProgressVisualState {
            fraction: self
                .value
                .map(|(value, maximum)| f64::from(value) / f64::from(maximum)),
        }
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressVisualState {
    pub fraction: Option<f64>,
}
#[derive(Clone, Copy, Debug)]
pub struct ProgressStyle {
    pub track: Color,
    pub fill: Color,
    pub indeterminate: Color,
}
impl ProgressStyle {
    pub fn paint(&self, list: &mut DisplayList, bounds: Rect, state: ProgressVisualState) {
        list.fill_rect(bounds, self.track);
        let (offset, fraction, color) = match state.fraction {
            Some(fraction) => (0.0, fraction.clamp(0.0, 1.0), self.fill),
            None => (0.35, 0.3, self.indeterminate),
        };
        list.fill_rect(
            Rect::new(
                Point::new(
                    bounds.origin.x + bounds.size.width * offset,
                    bounds.origin.y,
                ),
                Size::new(bounds.size.width * fraction, bounds.size.height),
            ),
            color,
        );
    }
}

/// One indeterminate sweep, in seconds.
pub const INDETERMINATE_PERIOD: f64 = 1.2;

impl ProgressStyle {
    /// Paints the moving indeterminate segment at monotonic time `now`.
    /// With reduced motion, pass a fixed `now` to draw a static bar.
    pub fn paint_indeterminate(
        &self,
        list: &mut DisplayList,
        bounds: Rect,
        now: std::time::Duration,
    ) {
        list.fill_rect(bounds, self.track);
        let phase = (now.as_secs_f64() / INDETERMINATE_PERIOD).fract();
        let width = bounds.size.width * 0.3;
        let x = bounds.origin.x - width + (bounds.size.width + width) * phase;
        list.push_clip(bounds);
        list.fill_rect(
            Rect::new(
                Point::new(x, bounds.origin.y),
                Size::new(width, bounds.size.height),
            ),
            self.indeterminate,
        );
        let _ = list.pop_clip();
    }
}

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
fn paint_focus(list: &mut DisplayList, bounds: Rect, color: Color) {
    list.fill_rect(
        Rect::new(
            Point::new(bounds.origin.x, bounds.origin.y + bounds.size.height - 2.0),
            Size::new(bounds.size.width, 2.0),
        ),
        color,
    );
}
