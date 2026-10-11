//! Range controls: sliders, spin boxes, progress, scroll bars, dials, and
//! numeric displays.

use crate::page::{Align, Clicks, Column, Env, Page, Push, node, pressed, sem, type_into};
use choscorui::controls::{
    ControlKind, DecimalSpinBox, Dial, IntegerRange, NumberBase, NumericDisplay, Progress,
    ScrollBar, ScrollBarInput, ScrollOrientation, ShortcutModifiers, Slider, SliderInput,
    SliderOrientation, SliderSemanticAction, Slot, SpinBox, SpinBoxInput, SpinBoxSemanticAction,
    StyleQuery, TextAlign, ValidationState, VisualState,
};
use choscorui::core::{
    ColorRole, DisplayList, Key, KeyEvent, NodeId, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, Size, Tree,
};

fn range(minimum: i32, maximum: i32, value: i32, step: u32, page: u32) -> IntegerRange {
    IntegerRange::new(minimum, maximum, value, step, page).expect("demo ranges are valid")
}

/// A caption row above a control.
fn caption(tree: &mut Tree, parent: NodeId) -> NodeId {
    node(tree, parent, false)
}

/// Horizontal, stepped, vertical, and disabled sliders.
pub struct SliderPage {
    sliders: Vec<(NodeId, NodeId, Slider, &'static str)>,
}

pub fn slider(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let mut sliders = Vec::new();
    for (label, range, orientation, enabled) in [
        (
            "Volume",
            range(0, 100, 40, 1, 10),
            SliderOrientation::Horizontal,
            true,
        ),
        (
            "Stepped (0–10 by 2)",
            range(0, 10, 4, 2, 2),
            SliderOrientation::Horizontal,
            true,
        ),
        (
            "Vertical",
            range(0, 100, 70, 5, 20),
            SliderOrientation::Vertical,
            true,
        ),
        (
            "Disabled",
            range(0, 100, 25, 1, 10),
            SliderOrientation::Horizontal,
            false,
        ),
    ] {
        let caption = caption(tree, parent);
        let id = node(tree, parent, true);
        let mut slider = Slider::new(label, range, orientation);
        slider.set_direction(env.direction);
        let _ = slider.set_enabled(tree, id, enabled && env.enabled);
        sliders.push((caption, id, slider, label));
    }
    Box::new(SliderPage { sliders })
}

impl Page for SliderPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (caption, id, slider, _) in &self.sliders {
            let _ = tree.set_bounds(*caption, column.row(20.0));
            let rect = match slider.orientation() {
                SliderOrientation::Horizontal => column.cells(16.0, &[320.0])[0],
                SliderOrientation::Vertical => column.cells(140.0, &[16.0])[0],
            };
            let _ = tree.set_bounds(*id, rect);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (caption, id, slider, label) in &self.sliders {
            env.caption(
                list,
                tree.bounds(*caption).unwrap_or_default(),
                &format!("{label}: {}", slider.value()),
            );
            let bounds = tree.bounds(*id).unwrap_or_default();
            slider.paint(&mut env.cx(list, bounds), tree, *id);
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.sliders
            .iter()
            .map(|(_, id, slider, _)| (*id, slider.semantics(tree, *id)))
            .collect()
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        for (_, id, slider, _) in &mut self.sliders {
            let _ = slider.handle(tree, *id, SliderInput::Pointer(event));
        }
        true
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let focus = tree.focus();
        let Some((_, id, slider, _)) = self
            .sliders
            .iter_mut()
            .find(|(_, id, _, _)| Some(*id) == focus)
        else {
            return false;
        };
        let _ = slider.handle(tree, *id, SliderInput::Key(event));
        true
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let Some((_, id, slider, _)) = self.sliders.iter_mut().find(|(_, id, _, _)| *id == node)
        else {
            return false;
        };
        let action = match action {
            SemanticAction::Increment => SliderSemanticAction::Increment,
            SemanticAction::Decrement => SliderSemanticAction::Decrement,
            SemanticAction::SetValue => match value.and_then(|v| v.trim().parse().ok()) {
                Some(value) => SliderSemanticAction::SetValue(value),
                None => return false,
            },
            _ => return false,
        };
        matches!(
            slider.handle(tree, *id, SliderInput::Semantic(action)),
            Ok(Some(_))
        )
    }

    fn native_disabled(&self) -> bool {
        true
    }
}

/// Integer spin boxes with different ranges.
pub struct SpinBoxPage {
    boxes: Vec<(NodeId, NodeId, SpinBox, &'static str)>,
}

pub fn spin_box(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let mut boxes = Vec::new();
    for (label, range, enabled) in [
        ("Copies (1–99)", range(1, 99, 2, 1, 10), true),
        ("Temperature (−20–40)", range(-20, 40, 21, 1, 5), true),
        ("Disabled", range(0, 10, 5, 1, 1), false),
    ] {
        let caption = caption(tree, parent);
        let id = node(tree, parent, true);
        let mut spin = SpinBox::new(label, range);
        spin.set_direction(env.direction);
        let _ = spin.set_enabled(tree, id, enabled && env.enabled);
        boxes.push((caption, id, spin, label));
    }
    Box::new(SpinBoxPage { boxes })
}

impl Page for SpinBoxPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (caption, id, _, _) in &self.boxes {
            let _ = tree.set_bounds(*caption, column.row(20.0));
            let _ = tree.set_bounds(*id, column.cells(36.0, &[140.0])[0]);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (caption, id, spin, label) in &self.boxes {
            env.caption(list, tree.bounds(*caption).unwrap_or_default(), label);
            let bounds = tree.bounds(*id).unwrap_or_default();
            spin.paint(&mut env.cx(list, bounds), tree, *id);
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.boxes
            .iter()
            .map(|(_, id, spin, _)| (*id, spin.semantics(tree, *id)))
            .collect()
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        for (_, id, spin, _) in &mut self.boxes {
            let _ = spin.handle(tree, *id, SpinBoxInput::Pointer(event));
        }
        matches!(event, PointerEvent::Down { .. })
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let focus = tree.focus();
        let Some((_, id, spin, _)) = self
            .boxes
            .iter_mut()
            .find(|(_, id, _, _)| Some(*id) == focus)
        else {
            return false;
        };
        let _ = spin.handle(tree, *id, SpinBoxInput::Key(event));
        true
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let Some((_, id, spin, _)) = self.boxes.iter_mut().find(|(_, id, _, _)| *id == node) else {
            return false;
        };
        let action = match action {
            SemanticAction::Increment => SpinBoxSemanticAction::Increment,
            SemanticAction::Decrement => SpinBoxSemanticAction::Decrement,
            SemanticAction::SetValue => match value.and_then(|v| v.trim().parse().ok()) {
                Some(value) => SpinBoxSemanticAction::SetValue(value),
                None => return false,
            },
            _ => return false,
        };
        matches!(
            spin.handle(tree, *id, SpinBoxInput::Semantic(action)),
            Ok(Some(_))
        )
    }

    fn native_disabled(&self) -> bool {
        true
    }
}

/// Determinate and indeterminate progress, with buttons to drive them.
pub struct ProgressPage {
    determinate: (NodeId, Progress),
    complete: (NodeId, Progress),
    indeterminate: (NodeId, Progress),
    captions: [NodeId; 3],
    advance: Push,
    reset: Push,
}

pub fn progress(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let captions = [
        caption(tree, parent),
        caption(tree, parent),
        caption(tree, parent),
    ];
    let bar = |tree: &mut Tree, value| {
        (
            node(tree, parent, false),
            Progress::new("Download", value, 100).expect("maximum is non-zero"),
        )
    };
    let determinate = bar(tree, 35);
    let complete = bar(tree, 100);
    let indeterminate = (
        node(tree, parent, false),
        Progress::indeterminate("Indexing"),
    );
    Box::new(ProgressPage {
        determinate,
        complete,
        indeterminate,
        captions,
        advance: Push::new(tree, parent, "Advance 10%", env),
        reset: Push::new(tree, parent, "Reset", env),
    })
}

impl ProgressPage {
    fn advance(&mut self, tree: &mut Tree) {
        let (id, bar) = &mut self.determinate;
        let value = bar.value().map_or(0, |(value, _)| value);
        let _ = bar.set_value(tree, *id, (value + 10).min(100), 100);
    }

    fn reset(&mut self, tree: &mut Tree) {
        let (id, bar) = &mut self.determinate;
        let _ = bar.set_value(tree, *id, 0, 100);
    }
}

impl Page for ProgressPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (caption, id) in
            self.captions
                .iter()
                .zip([self.determinate.0, self.complete.0, self.indeterminate.0])
        {
            let _ = tree.set_bounds(*caption, column.row(20.0));
            let _ = tree.set_bounds(id, column.cells(10.0, &[360.0])[0]);
        }
        let cells = column.cells(32.0, &[140.0, 100.0]);
        let _ = tree.set_bounds(self.advance.node, cells[0]);
        let _ = tree.set_bounds(self.reset.node, cells[1]);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (caption, (id, bar)) in
            self.captions
                .iter()
                .zip([&self.determinate, &self.complete, &self.indeterminate])
        {
            let text = match bar.value() {
                Some((value, maximum)) => format!("{value} of {maximum}"),
                None => "Indeterminate (animates while visible)".into(),
            };
            env.caption(list, tree.bounds(*caption).unwrap_or_default(), &text);
            let bounds = tree.bounds(*id).unwrap_or_default();
            bar.paint(&mut env.cx(list, bounds));
        }
        self.advance.paint(tree, list, env);
        self.reset.paint(tree, list, env);
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        vec![
            (self.determinate.0, self.determinate.1.semantics()),
            (self.complete.0, self.complete.1.semantics()),
            (self.indeterminate.0, self.indeterminate.1.semantics()),
            self.advance.semantics(tree),
            self.reset.semantics(tree),
        ]
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if self.advance.pointer(tree, event) {
            self.advance(tree);
        }
        if self.reset.pointer(tree, event) {
            self.reset(tree);
        }
        true
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if self.advance.key(tree, event) {
            self.advance(tree);
        }
        if self.reset.key(tree, event) {
            self.reset(tree);
        }
        true
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        if self.advance.action(tree, node, action) {
            self.advance(tree);
            return true;
        }
        if self.reset.action(tree, node, action) {
            self.reset(tree);
            return true;
        }
        false
    }

    #[cfg(target_os = "macos")]
    fn tick(&mut self, _env: &Env<'_>) -> bool {
        self.indeterminate.1.needs_frame(true)
    }

    fn native_disabled(&self) -> bool {
        true
    }
}

/// Vertical and horizontal scroll bars over a list of numbered lines.
pub struct ScrollBarPage {
    bars: Vec<(NodeId, ScrollBar)>,
    view: NodeId,
    dragging: Option<usize>,
}

const SCROLL_LINES: usize = 60;
const SCROLL_LINE_HEIGHT: f64 = 22.0;
const SCROLL_WIDTH: f64 = 900.0;

pub fn scroll_bar(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let view = node(tree, parent, false);
    let mut vertical = ScrollBar::new("Vertical scroll", ScrollOrientation::Vertical);
    vertical.set_line_step(SCROLL_LINE_HEIGHT);
    let horizontal = ScrollBar::new("Horizontal scroll", ScrollOrientation::Horizontal);
    Box::new(ScrollBarPage {
        bars: vec![
            (node(tree, parent, true), vertical),
            (node(tree, parent, true), horizontal),
        ],
        view,
        dragging: None,
    })
}

impl ScrollBarPage {
    fn along(&self, tree: &Tree, index: usize, point: Point) -> f64 {
        let (id, bar) = &self.bars[index];
        let bounds = tree.bounds(*id).unwrap_or_default();
        match bar.orientation() {
            ScrollOrientation::Vertical => point.y - bounds.origin.y,
            ScrollOrientation::Horizontal => point.x - bounds.origin.x,
        }
    }
}

impl Page for ScrollBarPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, _env: &Env<'_>) {
        let view = Rect::new(
            bounds.origin,
            Size::new(bounds.size.width.min(420.0), bounds.size.height.min(300.0)),
        );
        let _ = tree.set_bounds(self.view, view);
        let vertical = Rect::new(
            Point::new(view.max_x() + 4.0, view.origin.y),
            Size::new(14.0, view.size.height),
        );
        let horizontal = Rect::new(
            Point::new(view.origin.x, view.max_y() + 4.0),
            Size::new(view.size.width, 14.0),
        );
        let _ = tree.set_bounds(self.bars[0].0, vertical);
        let _ = tree.set_bounds(self.bars[1].0, horizontal);
        self.bars[0].1.set_range(
            SCROLL_LINES as f64 * SCROLL_LINE_HEIGHT,
            view.size.height,
            vertical.size.height,
        );
        self.bars[1]
            .1
            .set_range(SCROLL_WIDTH, view.size.width, horizontal.size.width);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let view = tree.bounds(self.view).unwrap_or_default();
        env.panel(list, view);
        list.push_clip(view);
        let top = self.bars[0].1.value();
        let left = self.bars[1].1.value();
        let first = (top / SCROLL_LINE_HEIGHT) as usize;
        for line in first..(first + 16).min(SCROLL_LINES) {
            let y = view.origin.y + line as f64 * SCROLL_LINE_HEIGHT - top;
            let _ = list.draw_text(
                Point::new(view.origin.x + 10.0 - left, y + 16.0),
                &format!(
                    "Line {:02} — a long line that extends past the right edge of the view so the horizontal bar matters",
                    line + 1
                ),
                env.font(),
                env.body(),
                env.tokens.color(ColorRole::Foreground),
            );
        }
        let _ = list.pop_clip();
        for (id, bar) in &self.bars {
            let bounds = tree.bounds(*id).unwrap_or_default();
            bar.paint(&mut env.cx(list, bounds));
            if tree.focus() == Some(*id) {
                env.focus_ring(list, bounds);
            }
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes: Vec<_> = self
            .bars
            .iter()
            .map(|(id, bar)| (*id, bar.semantics()))
            .collect();
        nodes.push((
            self.view,
            sem(
                SemanticRole::ScrollArea,
                "Numbered lines",
                Some(format!("{} lines", SCROLL_LINES)),
            ),
        ));
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => {
                let Some(index) = self
                    .bars
                    .iter()
                    .position(|(id, _)| tree.bounds(*id).is_some_and(|b| b.contains(position)))
                else {
                    return false;
                };
                let at = self.along(tree, index, position);
                self.dragging = Some(index);
                let id = self.bars[index].0;
                let _ = tree.set_focus(Some(id));
                let _ = tree.set_pointer_capture(Some(id));
                self.bars[index].1.handle(ScrollBarInput::Press { at });
                true
            }
            PointerEvent::Move { position } => match self.dragging {
                Some(index) => {
                    let at = self.along(tree, index, position);
                    self.bars[index].1.handle(ScrollBarInput::Drag { at })
                }
                None => false,
            },
            PointerEvent::Up { .. } => match self.dragging.take() {
                Some(index) => {
                    let _ = tree.set_pointer_capture(None);
                    self.bars[index].1.handle(ScrollBarInput::Release);
                    true
                }
                None => false,
            },
            PointerEvent::Down { .. } => false,
            PointerEvent::Wheel { delta, .. } => {
                let v = self.bars[0].1.value();
                let h = self.bars[1].1.value();
                self.bars[0].1.set_value(v + delta.y) | self.bars[1].1.set_value(h + delta.x)
            }
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let Some(key) = pressed(event) else {
            return false;
        };
        match self
            .bars
            .iter_mut()
            .find(|(id, _)| tree.focus() == Some(*id))
        {
            Some((_, bar)) => {
                bar.handle(ScrollBarInput::Key(key));
                true
            }
            None => false,
        }
    }

    fn action(
        &mut self,
        _tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        match self.bars.iter_mut().find(|(id, _)| *id == node) {
            Some((_, bar)) => bar.handle(ScrollBarInput::Semantic(action)),
            None => false,
        }
    }
}

/// A clamped dial and a wrapping (compass) dial.
pub struct DialPage {
    dials: Vec<(NodeId, Dial, &'static str)>,
    dragging: Option<usize>,
}

pub fn dial(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    Box::new(DialPage {
        dials: vec![
            (
                node(tree, parent, true),
                Dial::new(range(0, 100, 30, 1, 10), false),
                "Gain",
            ),
            (
                node(tree, parent, true),
                Dial::new(range(0, 359, 90, 1, 15), true),
                "Heading (wraps)",
            ),
        ],
        dragging: None,
    })
}

impl Page for DialPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(140.0, &[140.0, 140.0]);
        for ((id, _, _), cell) in self.dials.iter().zip(cells) {
            let _ = tree.set_bounds(*id, cell);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (id, dial, label) in &self.dials {
            let bounds = tree.bounds(*id).unwrap_or_default();
            let state = VisualState::NONE
                .set(VisualState::FOCUSED, tree.focus() == Some(*id))
                .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(*id));
            dial.paint(&mut env.cx(list, bounds), state);
            let below = Rect::new(
                Point::new(bounds.origin.x, bounds.max_y() + 4.0),
                Size::new(bounds.size.width, 20.0),
            );
            env.text(
                list,
                below,
                &format!("{label}: {}", dial.value()),
                env.small(),
                env.tokens.color(ColorRole::Foreground),
                Align::Center,
            );
            if tree.focus() == Some(*id) {
                env.focus_ring(list, bounds);
            }
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.dials
            .iter()
            .map(|(id, dial, label)| (*id, dial.semantics(label)))
            .collect()
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => {
                self.dragging = self
                    .dials
                    .iter()
                    .position(|(id, _, _)| tree.bounds(*id).is_some_and(|b| b.contains(position)));
                let Some(index) = self.dragging else {
                    return false;
                };
                let (id, dial, _) = &mut self.dials[index];
                let _ = tree.set_focus(Some(*id));
                let _ = tree.set_pointer_capture(Some(*id));
                dial.point_to(tree.bounds(*id).unwrap_or_default().center(), position)
            }
            PointerEvent::Move { position } => match self.dragging {
                Some(index) => {
                    let (id, dial, _) = &mut self.dials[index];
                    dial.point_to(tree.bounds(*id).unwrap_or_default().center(), position)
                }
                None => false,
            },
            PointerEvent::Up { .. } => {
                if self.dragging.take().is_some() {
                    let _ = tree.set_pointer_capture(None);
                }
                false
            }
            PointerEvent::Wheel { .. } | PointerEvent::Down { .. } => false,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let Some(key) = pressed(event) else {
            return false;
        };
        match self
            .dials
            .iter_mut()
            .find(|(id, _, _)| tree.focus() == Some(*id))
        {
            Some((_, dial, _)) => {
                dial.handle_key(key);
                true
            }
            None => false,
        }
    }

    fn action(
        &mut self,
        _tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let key = match action {
            SemanticAction::Increment => Key::ArrowUp,
            SemanticAction::Decrement => Key::ArrowDown,
            _ => return false,
        };
        match self.dials.iter_mut().find(|(id, _, _)| *id == node) {
            Some((_, dial, _)) => dial.handle_key(key),
            None => false,
        }
    }
}

/// Locale-aware decimal spin boxes; typing shows validation feedback.
pub struct DecimalSpinBoxPage {
    boxes: Vec<(NodeId, NodeId, DecimalSpinBox, &'static str)>,
}

pub fn decimal_spin_box(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let mut boxes = Vec::new();
    for (label, limits, step, digits, value) in [
        (
            "Price (0–10,000, 2 decimals)",
            (0.0, 10_000.0),
            0.25,
            2,
            1_234.5,
        ),
        ("Ratio (0–1, 3 decimals)", (0.0, 1.0), 0.005, 3, 0.618),
    ] {
        let caption = caption(tree, parent);
        let id = node(tree, parent, true);
        boxes.push((
            caption,
            id,
            DecimalSpinBox::new(label, env.locale.clone(), limits, step, digits, value),
            label,
        ));
    }
    Box::new(DecimalSpinBoxPage { boxes })
}

impl Page for DecimalSpinBoxPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (caption, id, _, _) in &self.boxes {
            let _ = tree.set_bounds(*caption, column.row(20.0));
            let _ = tree.set_bounds(*id, column.cells(32.0, &[220.0])[0]);
            column.space(18.0);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (caption, id, spin, label) in &self.boxes {
            env.caption(
                list,
                tree.bounds(*caption).unwrap_or_default(),
                &format!("{label} — value {}", spin.value()),
            );
            let bounds = tree.bounds(*id).unwrap_or_default();
            let message = Rect::new(
                Point::new(bounds.origin.x, bounds.max_y() + 2.0),
                Size::new(bounds.size.width + 200.0, 16.0),
            );
            let host = VisualState::NONE
                .set(VisualState::FOCUSED, tree.focus() == Some(*id))
                .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(*id));
            spin.paint(&mut env.cx(list, bounds), host);
            let (slot, text) = match spin.validation() {
                ValidationState::Invalid(message) => (Slot::Error, message),
                ValidationState::Intermediate => (Slot::Description, "Incomplete".to_owned()),
                ValidationState::Valid => (Slot::Description, "Valid".to_owned()),
            };
            let mut cx = env.cx(list, message);
            let style = cx.style(StyleQuery::new(ControlKind::Field, slot));
            cx.label(message, &text, &style, TextAlign::Start);
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.boxes
            .iter()
            .map(|(_, id, spin, _)| (*id, spin.semantics()))
            .collect()
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let Some(key) = pressed(event) else {
            return false;
        };
        let Some((_, _, spin, _)) = self
            .boxes
            .iter_mut()
            .find(|(_, id, _, _)| tree.focus() == Some(*id))
        else {
            return false;
        };
        if matches!(key, Key::Backspace | Key::Delete) {
            let mut text = spin.text().to_owned();
            text.pop();
            spin.set_text(text);
        } else {
            spin.handle_key(key);
        }
        true
    }

    fn text(&mut self, tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        let Some((_, _, spin, _)) = self
            .boxes
            .iter_mut()
            .find(|(_, id, _, _)| tree.focus() == Some(*id))
        else {
            return false;
        };
        let mut value = spin.text().to_owned();
        if type_into(&mut value, text) {
            spin.set_text(value);
            return true;
        }
        false
    }

    fn action(
        &mut self,
        _tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        match self.boxes.iter_mut().find(|(_, id, _, _)| *id == node) {
            Some((_, _, spin, _)) => spin.handle_semantic(action, value),
            None => false,
        }
    }
}

/// A seven-segment display in four bases.
pub struct NumericDisplayPage {
    display: (NodeId, NumericDisplay),
    bases: Vec<(NodeId, NumberBase, &'static str)>,
    value: i64,
    base: NumberBase,
    clicks: Clicks,
    hint: NodeId,
}

pub fn numeric_display(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let mut display = NumericDisplay::new(8, NumberBase::Decimal);
    display.set_value(2026);
    Box::new(NumericDisplayPage {
        display: (node(tree, parent, true), display),
        bases: [
            (NumberBase::Decimal, "DEC"),
            (NumberBase::Hexadecimal, "HEX"),
            (NumberBase::Octal, "OCT"),
            (NumberBase::Binary, "BIN"),
        ]
        .into_iter()
        .map(|(base, label)| (node(tree, parent, true), base, label))
        .collect(),
        value: 2026,
        base: NumberBase::Decimal,
        clicks: Clicks::default(),
        hint: node(tree, parent, false),
    })
}

impl NumericDisplayPage {
    fn set_value(&mut self, value: i64) -> bool {
        self.value = value;
        self.display.1.set_value(value);
        true
    }
}

impl Page for NumericDisplayPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let _ = tree.set_bounds(self.display.0, column.cells(70.0, &[360.0])[0]);
        let cells = column.cells(28.0, &[64.0, 64.0, 64.0, 64.0]);
        for ((id, _, _), cell) in self.bases.iter().zip(cells) {
            let _ = tree.set_bounds(*id, cell);
        }
        let _ = tree.set_bounds(self.hint, column.row(20.0));
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let (id, display) = &self.display;
        let bounds = tree.bounds(*id).unwrap_or_default();
        let state = VisualState::NONE
            .set(VisualState::FOCUSED, tree.focus() == Some(*id))
            .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(*id));
        display.paint(&mut env.cx(list, bounds), state);
        for (id, base, label) in &self.bases {
            let bounds = tree.bounds(*id).unwrap_or_default();
            let fill = if *base == self.base {
                choscorui::core::lerp_color(
                    env.tokens.color(ColorRole::Card),
                    env.tokens.color(ColorRole::Primary),
                    0.5,
                )
            } else {
                env.mix(0.08)
            };
            list.fill_rect(bounds, fill);
            env.text(
                list,
                bounds,
                label,
                env.small(),
                env.tokens.color(ColorRole::Foreground),
                Align::Center,
            );
            if tree.focus() == Some(*id) {
                env.focus_ring(list, bounds);
            }
        }
        env.caption(
            list,
            tree.bounds(self.hint).unwrap_or_default(),
            "Arrow keys change the value; values wider than 8 digits overflow to dashes",
        );
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = vec![(self.display.0, self.display.1.semantics("Counter"))];
        for (id, base, label) in &self.bases {
            let mut node = sem(SemanticRole::RadioButton, *label, None);
            node.state.checked = Some(*base == self.base);
            node.actions = vec![SemanticAction::Activate];
            nodes.push((*id, node));
        }
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let nodes: Vec<NodeId> = self.bases.iter().map(|(id, _, _)| *id).collect();
        match self.clicks.feed(tree, event, &nodes) {
            Some((id, _)) => self.activate(id),
            None => false,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let Some(key) = pressed(event) else {
            return false;
        };
        let focus = tree.focus();
        if focus == Some(self.display.0) {
            return match key {
                Key::ArrowUp | Key::ArrowRight => self.set_value(self.value.saturating_add(1)),
                Key::ArrowDown | Key::ArrowLeft => self.set_value(self.value.saturating_sub(1)),
                Key::PageUp => self.set_value(self.value.saturating_mul(10)),
                Key::PageDown => self.set_value(self.value / 10),
                _ => false,
            };
        }
        match (focus, key) {
            (Some(id), Key::Space | Key::Enter) => self.activate(id),
            _ => false,
        }
    }

    fn action(
        &mut self,
        _tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        action == SemanticAction::Activate && self.activate(node)
    }
}

impl NumericDisplayPage {
    fn activate(&mut self, id: NodeId) -> bool {
        match self.bases.iter().find(|(node, _, _)| *node == id) {
            Some((_, base, _)) => {
                self.base = *base;
                self.display.1.set_base(*base);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::tests::Harness;

    #[test]
    fn decimal_spin_box_steps_and_clamps() {
        let mut h = Harness::new("decimal-spin-box");
        let spin = &mut h.get::<DecimalSpinBoxPage>().boxes[0].2;
        let start = spin.value();
        spin.handle_key(Key::ArrowUp);
        assert!((spin.value() - (start + 0.25)).abs() < 1e-9);
        for _ in 0..10_000 {
            spin.handle_key(Key::PageUp);
        }
        assert!((spin.value() - 10_000.0).abs() < 1e-9, "{}", spin.value());
    }
}
