//! Remaining basic widgets: standalone scroll bar, decimal spin box, dial,
//! tool and command-link buttons, button groups, toolbox (accordion),
//! numeric (LCD) display, focus frame, size grip, and key-sequence capture.
//! Each documents its value, keyboard, and accessibility contract.

use crate::range_controls::paint_stepper_field;
use crate::style::{
    Content, ControlKind, ItemVariant, Orientation, PaintContext, Slot, StyleQuery, Styles,
    TextAlign, ToggleVariant, VisualState, content_rect, fill_shape, join_segment,
};
use crate::{
    CommandId, CommandRegistry, IntegerRange, NumberInput, STEPPER_WIDTH, Shortcut,
    ShortcutModifiers, ValidationState,
};
use choscorui_core::{
    Color, ColorRole, DesignTokens, DisplayList, Icon, Key, Locale, NodeId, Path, Point,
    RadiusScale, Rect, SemanticAction, SemanticNode, SemanticRole, SemanticState, Size, Tree,
};
use std::f64::consts::PI;

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
        if !self.is_needed() {
            return;
        }
        let bounds = cx.bounds;
        let query =
            |slot| StyleQuery::new(ControlKind::ScrollArea, slot).state(self.visual_state());
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
        fill_shape(cx.list, rect, &thumb.radius, cx.rtl, thumb.background);
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
        let stroke = |list: &mut DisplayList, path: Path, color: Color, width: f64| {
            let _ = list.stroke_path(
                path,
                choscorui_core::Paint::Solid(color),
                choscorui_core::StrokeStyle {
                    cap: choscorui_core::LineCap::Round,
                    ..choscorui_core::StrokeStyle::new(width)
                },
            );
        };
        stroke(
            cx.list,
            arc(start, start + self.sweep()),
            track.background,
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
                range.background,
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
}

// ------------------------------------------------------ Tool and link buttons

/// What a tool button does on press.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolButtonMenu {
    None,
    /// The whole button opens the menu.
    Instant,
    /// Click runs the command; the arrow (or Down key) opens the menu.
    Split,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolButtonAction {
    None,
    Activated(CommandId),
    Toggled(CommandId, bool),
    OpenMenu,
}

/// Toolbar button: optionally checkable (the command's checked state) and
/// optionally with a menu.
#[derive(Clone, Debug)]
pub struct ToolButton {
    pub label: String,
    pub command: CommandId,
    pub checkable: bool,
    pub menu: ToolButtonMenu,
    /// Width of the split-menu arrow at the trailing edge.
    pub arrow_width: f64,
}

impl ToolButton {
    pub fn new(label: impl Into<String>, command: CommandId) -> Self {
        Self {
            label: label.into(),
            command,
            checkable: false,
            menu: ToolButtonMenu::None,
            arrow_width: 14.0,
        }
    }

    fn run(&self, commands: &mut CommandRegistry) -> ToolButtonAction {
        let Ok(state) = commands.state(self.command) else {
            return ToolButtonAction::None;
        };
        if !state.enabled {
            return ToolButtonAction::None;
        }
        if self.checkable {
            let checked = !state.checked;
            let _ = commands.set_checked(self.command, checked);
            ToolButtonAction::Toggled(self.command, checked)
        } else {
            ToolButtonAction::Activated(self.command)
        }
    }

    /// A completed click at `x` within `bounds`.
    pub fn click(&self, bounds: Rect, x: f64, commands: &mut CommandRegistry) -> ToolButtonAction {
        match self.menu {
            ToolButtonMenu::Instant => ToolButtonAction::OpenMenu,
            ToolButtonMenu::Split
                if x >= bounds.origin.x + bounds.size.width - self.arrow_width =>
            {
                ToolButtonAction::OpenMenu
            }
            _ => self.run(commands),
        }
    }

    pub fn handle_key(&self, key: Key, commands: &mut CommandRegistry) -> ToolButtonAction {
        match key {
            Key::Space | Key::Enter if self.menu == ToolButtonMenu::Instant => {
                ToolButtonAction::OpenMenu
            }
            Key::Space | Key::Enter => self.run(commands),
            Key::ArrowDown if self.menu != ToolButtonMenu::None => ToolButtonAction::OpenMenu,
            _ => ToolButtonAction::None,
        }
    }

    pub fn semantics(&self, commands: &CommandRegistry) -> SemanticNode {
        let state = commands.state(self.command).unwrap_or_default();
        let mut actions = Vec::new();
        if state.enabled {
            actions.push(SemanticAction::Activate);
            if self.menu != ToolButtonMenu::None {
                actions.push(SemanticAction::ShowMenu);
            }
        }
        SemanticNode {
            role: if self.checkable {
                SemanticRole::Checkbox
            } else {
                SemanticRole::Button
            },
            name: self.label.clone(),
            description: String::new(),
            value: None,
            state: SemanticState {
                disabled: !state.enabled,
                checked: self.checkable.then_some(state.checked),
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions,
            text: None,
        }
    }

    /// The flags the toggle recipe reads: the host's flags in `host`, with
    /// the command's checked and disabled state.
    pub fn visual_state(&self, commands: &CommandRegistry, host: VisualState) -> VisualState {
        let state = commands.state(self.command).unwrap_or_default();
        host.set(VisualState::CHECKED, self.checkable && state.checked)
            .set(VisualState::DISABLED, !state.enabled)
    }

    /// Paints the button with the `Toggle` look: transparent, and `muted`
    /// on hover or while checked. A menu adds a chevron at the end; a split
    /// menu also adds a divider before the arrow.
    pub fn paint(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        let style = cx.style(StyleQuery::new(ControlKind::Toggle, Slot::Root).state(state));
        let rule = cx.style(StyleQuery::new(ControlKind::ButtonGroup, Slot::Separator));
        let bounds = cx.bounds;
        let arrow = match self.menu {
            ToolButtonMenu::None => 0.0,
            ToolButtonMenu::Instant => style.icon_size,
            ToolButtonMenu::Split => self.arrow_width,
        }
        .min(bounds.size.width);
        // Instant menus keep the chevron inside the padding; a split arrow
        // spans the button's end edge.
        let split = self.menu == ToolButtonMenu::Split;
        let (label_bounds, arrow_rect) = if arrow == 0.0 {
            (bounds, None)
        } else if split {
            let x = if cx.rtl {
                bounds.origin.x
            } else {
                bounds.max_x() - arrow
            };
            let label_x = if cx.rtl {
                bounds.origin.x + arrow
            } else {
                bounds.origin.x
            };
            (
                Rect::from_xywh(
                    label_x,
                    bounds.origin.y,
                    bounds.size.width - arrow,
                    bounds.size.height,
                ),
                Some(Rect::from_xywh(
                    x,
                    bounds.origin.y,
                    arrow,
                    bounds.size.height,
                )),
            )
        } else {
            (bounds, None)
        };
        cx.slot(bounds, &style, |cx, inner| {
            let mut label = content_rect(label_bounds, &style, cx.rtl);
            label.size.height = inner.size.height;
            label.origin.y = inner.origin.y;
            if !split && arrow > 0.0 {
                let x = if cx.rtl {
                    inner.origin.x
                } else {
                    inner.max_x() - arrow
                };
                cx.icon(
                    Rect::from_xywh(x, inner.origin.y, arrow, inner.size.height),
                    Icon::ChevronDown,
                    &style,
                );
                label.size.width = (inner.size.width - arrow - style.gap).max(0.0);
                if cx.rtl {
                    label.origin.x = inner.max_x() - label.size.width;
                }
            }
            cx.label(label, &self.label, &style, TextAlign::Center);
        });
        if let Some(rect) = arrow_rect {
            let rule_x = if cx.rtl { rect.max_x() } else { rect.origin.x };
            let inset = style.padding.block_start.max(rect.size.height / 4.0);
            let line = Rect::from_xywh(
                rule_x - rule.min_width / 2.0,
                rect.origin.y + inset,
                rule.min_width,
                (rect.size.height - 2.0 * inset).max(0.0),
            );
            cx.list.fill_rect(line, rule.background);
            let mut chevron = style;
            chevron.icon_size = style.icon_size.min(arrow);
            cx.icon(rect, Icon::ChevronDown, &chevron);
        }
    }
}

/// Exclusive or non-exclusive group of checkable commands (for example
/// text alignment buttons). Exclusive groups keep exactly one checked.
#[derive(Clone, Debug)]
pub struct ButtonGroup {
    members: Vec<CommandId>,
    exclusive: bool,
    orientation: Orientation,
}

impl ButtonGroup {
    pub fn new(members: Vec<CommandId>, exclusive: bool) -> Self {
        Self {
            members,
            exclusive,
            orientation: Orientation::Horizontal,
        }
    }

    /// Joins the members in a row (the default) or a column.
    pub fn with_orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    pub fn members(&self) -> &[CommandId] {
        &self.members
    }

    /// The flags the member recipe reads: the host's focus and hover flags
    /// in `host`, with the command's checked and disabled state.
    pub fn member_state(
        &self,
        commands: &CommandRegistry,
        member: CommandId,
        host: VisualState,
    ) -> VisualState {
        let state = commands.state(member).unwrap_or_default();
        host.set(VisualState::CHECKED, state.checked)
            .set(VisualState::DISABLED, !state.enabled)
    }

    /// Paints member `index` in the context's bounds with the outline
    /// toggle look. Inner corners are square, and each member after the
    /// first overlaps the border before it, so the members read as one
    /// control.
    pub fn paint_member(
        &self,
        cx: &mut PaintContext<'_>,
        index: usize,
        label: &str,
        state: VisualState,
    ) {
        let query = StyleQuery::new(ControlKind::ToggleGroup, Slot::Item)
            .variant(ToggleVariant::Outline)
            .state(state);
        let mut style = cx.style(query);
        let mut rect = cx.bounds;
        join_segment(
            &mut style,
            &mut rect,
            index,
            self.members.len(),
            self.orientation,
            cx.rtl,
        );
        cx.slot(rect, &style, |cx, inner| {
            cx.label(inner, label, &style, TextAlign::Center);
        });
    }
    pub fn checked(&self, commands: &CommandRegistry) -> Vec<CommandId> {
        self.members
            .iter()
            .copied()
            .filter(|id| commands.state(*id).is_ok_and(|state| state.checked))
            .collect()
    }
    /// Applies a click on `member`. Exclusive groups ignore unchecking the
    /// checked member.
    pub fn toggle(&self, commands: &mut CommandRegistry, member: CommandId) -> bool {
        if !self.members.contains(&member)
            || !commands.state(member).is_ok_and(|state| state.enabled)
        {
            return false;
        }
        let checked = commands.state(member).is_ok_and(|state| state.checked);
        if self.exclusive {
            if checked {
                return false;
            }
            for id in &self.members {
                let _ = commands.set_checked(*id, *id == member);
            }
        } else {
            let _ = commands.set_checked(member, !checked);
        }
        true
    }
    /// Arrow-key movement within an exclusive group.
    pub fn step(&self, commands: &mut CommandRegistry, forward: bool) -> Option<CommandId> {
        let current = self.checked(commands).first().copied();
        let index = current.and_then(|id| self.members.iter().position(|member| *member == id));
        let count = self.members.len();
        if count == 0 {
            return None;
        }
        let mut next = match (index, forward) {
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
            (None, _) => 0,
        };
        for _ in 0..count {
            let id = self.members[next];
            if commands.state(id).is_ok_and(|state| state.enabled) {
                self.toggle(commands, id);
                return Some(id);
            }
            next = if forward {
                (next + 1) % count
            } else {
                (next + count - 1) % count
            };
        }
        None
    }
}

/// The look of a command link: an arrow, a title, and a description line
/// in an item row. Behavior comes from a [`crate::Button`] on the same
/// node.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandLink {
    pub title: String,
    pub description: String,
}

impl CommandLink {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
        }
    }

    /// Paints the link in `state`; hover and press highlight the row.
    pub fn paint(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        let query = StyleQuery::new(ControlKind::Item, Slot::Root)
            .variant(ItemVariant::Outline)
            .state(state.set(VisualState::HIGHLIGHTED, true));
        let root = cx.style(query);
        let media = cx.style(query.slot(Slot::Media));
        let mut title = cx.style(query.slot(Slot::Title));
        title.foreground = cx.tokens.color(ColorRole::Primary);
        let description = cx.style(query.slot(Slot::Description));
        let bounds = cx.bounds;
        cx.slot(bounds, &root, |cx, inner| {
            let icon = media.icon_size;
            let (left, right) = if cx.rtl {
                (inner.max_x() - icon, inner.origin.x)
            } else {
                (inner.origin.x, inner.origin.x + icon + root.gap)
            };
            let arrow = if cx.rtl {
                Icon::ArrowLeft
            } else {
                Icon::ArrowRight
            };
            let mut arrow_style = media;
            arrow_style.foreground = title.foreground;
            cx.icon(
                Rect::from_xywh(left, inner.origin.y, icon, title.text.line_height),
                arrow,
                &arrow_style,
            );
            let text = Rect::from_xywh(
                right,
                inner.origin.y,
                (inner.size.width - icon - root.gap).max(0.0),
                title.text.line_height,
            );
            cx.label(text, &self.title, &title, TextAlign::Start);
            let below = text.translate(0.0, title.text.line_height + 4.0);
            cx.label(below, &self.description, &description, TextAlign::Start);
        });
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
}

impl ToolBox {
    pub fn new(titles: Vec<String>) -> Self {
        Self {
            titles,
            current: 0,
            focused: 0,
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
        let (on, off) = (value.foreground, track.background);
        cx.slot(cx.bounds, &root, |cx, bounds| {
            self.paint_segments(cx.list, bounds, on, off)
        });
    }

    fn paint_segments(&self, list: &mut DisplayList, bounds: Rect, on: Color, off: Color) {
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
                list.fill_rect(rect, if lit { on } else { off });
            }
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
}

impl SizeGrip {
    pub fn new(minimum: Size, maximum: Size, rtl: bool) -> Self {
        Self {
            minimum,
            maximum,
            rtl,
            start: None,
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
}

// ----------------------------------------------------------- KeySequenceEdit

/// Records up to `max_chords` shortcuts. Presses of keys without a
/// `Key` mapping (`Key::Other`, which includes modifier-only presses) are
/// ignored; Backspace with no modifiers clears; Escape cancels and keeps
/// the previous sequence; recording finishes when full or on
/// [`KeySequenceEdit::finish`] (for example after a pause or focus loss).
#[derive(Clone, Debug, PartialEq)]
pub struct KeySequenceEdit {
    committed: Vec<Shortcut>,
    recording: Option<Vec<Shortcut>>,
    max_chords: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeySequenceChange {
    None,
    Recording,
    Committed,
    Cancelled,
    Cleared,
}

impl KeySequenceEdit {
    pub fn new(max_chords: usize) -> Self {
        Self {
            committed: Vec::new(),
            recording: None,
            max_chords: max_chords.clamp(1, 4),
        }
    }
    pub fn sequence(&self) -> &[Shortcut] {
        &self.committed
    }
    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }
    pub fn start(&mut self) {
        self.recording = Some(Vec::new());
    }
    pub fn press(&mut self, key: Key, modifiers: ShortcutModifiers) -> KeySequenceChange {
        let Some(recording) = &mut self.recording else {
            return KeySequenceChange::None;
        };
        let plain = modifiers == ShortcutModifiers::default();
        match key {
            Key::Escape if plain => {
                self.recording = None;
                KeySequenceChange::Cancelled
            }
            Key::Backspace | Key::Delete if plain && recording.is_empty() => {
                self.committed.clear();
                self.recording = None;
                KeySequenceChange::Cleared
            }
            Key::Other(_) => KeySequenceChange::None,
            _ => {
                recording.push(Shortcut { key, modifiers });
                if recording.len() >= self.max_chords {
                    self.finish()
                } else {
                    KeySequenceChange::Recording
                }
            }
        }
    }
    pub fn finish(&mut self) -> KeySequenceChange {
        match self.recording.take() {
            Some(recording) if !recording.is_empty() => {
                self.committed = recording;
                KeySequenceChange::Committed
            }
            Some(_) => KeySequenceChange::Cancelled,
            None => KeySequenceChange::None,
        }
    }
    /// macOS-style text such as "⌃⌥⇧⌘S, ⌘K".
    pub fn text(&self) -> String {
        self.committed
            .iter()
            .map(|shortcut| shortcut_text(*shortcut))
            .collect::<Vec<_>>()
            .join(", ")
    }
    pub fn semantics(&self, name: &str, node: NodeId) -> SemanticNode {
        let _ = node;
        SemanticNode {
            role: SemanticRole::TextField,
            name: name.to_owned(),
            description: if self.is_recording() {
                "Recording shortcut; press Escape to cancel".into()
            } else {
                String::new()
            },
            value: Some(self.text()),
            state: SemanticState {
                read_only: true,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: vec![SemanticAction::Activate, SemanticAction::Focus],
            text: None,
        }
    }

    /// Paints the input box with one `Kbd` key cap per chord, or
    /// `placeholder` when the sequence is empty. While recording, the
    /// chords so far show. `host` carries focus flags.
    pub fn paint(&self, cx: &mut PaintContext<'_>, host: VisualState, placeholder: &str) {
        let input = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root).state(host));
        let muted = cx.style(StyleQuery::new(ControlKind::Input, Slot::Placeholder));
        let cap = cx.style(StyleQuery::new(ControlKind::Kbd, Slot::Root));
        let group = cx.style(StyleQuery::new(ControlKind::Kbd, Slot::Group));
        let chords = self.recording.as_ref().unwrap_or(&self.committed);
        let bounds = cx.bounds;
        cx.slot(bounds, &input, |cx, inner| {
            if chords.is_empty() {
                cx.label(inner, placeholder, &muted, TextAlign::Start);
                return;
            }
            let mut x = if cx.rtl {
                inner.max_x()
            } else {
                inner.origin.x
            };
            let height = cap.min_height.min(inner.size.height);
            let y = inner.origin.y + (inner.size.height - height) / 2.0;
            for chord in chords {
                let text = shortcut_text(*chord);
                let width = Content::label(&text).size(&cap, cx.text).width;
                let left = if cx.rtl { x - width } else { x };
                let rect = Rect::from_xywh(left, y, width, height);
                cx.slot(rect, &cap, |cx, inner| {
                    cx.label(inner, &text, &cap, TextAlign::Center);
                });
                x = if cx.rtl {
                    x - width - group.gap
                } else {
                    x + width + group.gap
                };
            }
        });
    }
}

/// macOS modifier glyphs in canonical order, then the key.
pub fn shortcut_text(shortcut: Shortcut) -> String {
    let mut text = String::new();
    let modifiers = shortcut.modifiers;
    if modifiers.control {
        text.push('⌃');
    }
    if modifiers.option {
        text.push('⌥');
    }
    if modifiers.shift {
        text.push('⇧');
    }
    if modifiers.command {
        text.push('⌘');
    }
    let key = match shortcut.key {
        Key::Character(character) => character.to_uppercase().to_string(),
        Key::Enter => "↩".into(),
        Key::Tab => "⇥".into(),
        Key::Space => "Space".into(),
        Key::Escape => "⎋".into(),
        Key::Backspace => "⌫".into(),
        Key::Delete => "⌦".into(),
        Key::ArrowUp => "↑".into(),
        Key::ArrowDown => "↓".into(),
        Key::ArrowLeft => "←".into(),
        Key::ArrowRight => "→".into(),
        Key::Home => "↖".into(),
        Key::End => "↘".into(),
        Key::PageUp => "⇞".into(),
        Key::PageDown => "⇟".into(),
        other => format!("{other:?}"),
    };
    text.push_str(&key);
    text
}
