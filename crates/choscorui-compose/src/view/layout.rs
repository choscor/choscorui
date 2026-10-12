//! Layout views. Each one lowers onto the core layout engine: stacks onto
//! stack containers, `ZStack` onto an overlay, `Grid` onto grid tracks,
//! and `ScrollView` onto a scroll container.

use super::View;
use super::env::Metric;
use super::host::{Cx, Flex, Inner};
use choscorui_controls::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
    OverlayScrollBar, PaintContext, ScrollOrientation, Separator, SeparatorOrientation,
};
use choscorui_core::{
    Axis, DesignTokens, Dirty, EventPhase, EventReply, ItemAlign, Key, KeyEvent, LayoutKind,
    LayoutStyle, MainAlign, Measured, NodeId, Point, PointerEvent, Rect, SemanticAction,
    SemanticNode, SemanticRole, SemanticState, Size, Space, Track, WheelUnit,
};
use std::cell::Cell;
use std::rc::Rc;

/// Where children sit across a vertical stack.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HorizontalAlignment {
    /// The start edge: left in left-to-right layouts, right in
    /// right-to-left ones.
    Leading,
    #[default]
    Center,
    Trailing,
    /// Each child takes the full width.
    Fill,
}

impl HorizontalAlignment {
    pub(crate) fn item(self) -> ItemAlign {
        match self {
            Self::Leading => ItemAlign::Start,
            Self::Center => ItemAlign::Center,
            Self::Trailing => ItemAlign::End,
            Self::Fill => ItemAlign::Stretch,
        }
    }
}

/// Where children sit across a horizontal stack.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VerticalAlignment {
    Top,
    #[default]
    Center,
    Bottom,
    /// The first text baselines line up.
    FirstBaseline,
    /// Each child takes the full height.
    Fill,
}

impl VerticalAlignment {
    pub(super) fn item(self) -> ItemAlign {
        match self {
            Self::Top => ItemAlign::Start,
            Self::Center => ItemAlign::Center,
            Self::Bottom => ItemAlign::End,
            Self::FirstBaseline => ItemAlign::Baseline,
            Self::Fill => ItemAlign::Stretch,
        }
    }
}

/// A position in a box along both axes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Alignment {
    pub horizontal: HorizontalAlignment,
    pub vertical: VerticalAlignment,
}

impl Alignment {
    pub const CENTER: Self = Self::new(HorizontalAlignment::Center, VerticalAlignment::Center);
    pub const LEADING: Self = Self::new(HorizontalAlignment::Leading, VerticalAlignment::Center);
    pub const TRAILING: Self = Self::new(HorizontalAlignment::Trailing, VerticalAlignment::Center);
    pub const TOP: Self = Self::new(HorizontalAlignment::Center, VerticalAlignment::Top);
    pub const BOTTOM: Self = Self::new(HorizontalAlignment::Center, VerticalAlignment::Bottom);
    pub const TOP_LEADING: Self = Self::new(HorizontalAlignment::Leading, VerticalAlignment::Top);
    pub const TOP_TRAILING: Self = Self::new(HorizontalAlignment::Trailing, VerticalAlignment::Top);
    pub const BOTTOM_LEADING: Self =
        Self::new(HorizontalAlignment::Leading, VerticalAlignment::Bottom);
    pub const BOTTOM_TRAILING: Self =
        Self::new(HorizontalAlignment::Trailing, VerticalAlignment::Bottom);
    pub const FILL: Self = Self::new(HorizontalAlignment::Fill, VerticalAlignment::Fill);

    pub const fn new(horizontal: HorizontalAlignment, vertical: VerticalAlignment) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }

    /// The overlay alignment of a child: (vertical, horizontal).
    pub(crate) fn items(self) -> (ItemAlign, ItemAlign) {
        (self.vertical.item(), self.horizontal.item())
    }
}

impl Default for Alignment {
    fn default() -> Self {
        Self::CENTER
    }
}

/// The default distance between stacked children.
pub(super) const DEFAULT_SPACING: Space = Space::S2;

/// The tokens that apply where a new node mounts, before it exists.
pub(crate) fn parent_tokens(cx: &Cx<'_>) -> DesignTokens {
    cx.inner
        .themes
        .resolve(&cx.inner.tree, cx.parent)
        .unwrap_or_else(|_| cx.inner.themes.application())
}

/// Mounts `children` in a stack node along `axis`.
fn stack<C: View>(
    cx: &mut Cx<'_>,
    axis: Axis,
    children: C,
    spacing: Metric,
    cross: ItemAlign,
    main: MainAlign,
) -> NodeId {
    let node = stack_node(cx, axis, spacing, cross, main);
    let env = cx.env.clone();
    cx.within(node, env).mount(children);
    node
}

// The node setup of the containers stays out of line, so that each
// content type adds only the call that mounts its children.

/// A new stack node along `axis`.
#[inline(never)]
pub(super) fn stack_node(
    cx: &mut Cx<'_>,
    axis: Axis,
    spacing: Metric,
    cross: ItemAlign,
    main: MainAlign,
) -> NodeId {
    let tokens = parent_tokens(cx);
    let style = LayoutStyle {
        kind: LayoutKind::Stack {
            axis,
            spacing: spacing.resolve(&tokens),
            main_align: main,
            cross_align: cross,
        },
        ..LayoutStyle::default()
    };
    cx.node(style)
}

/// Children top to bottom.
pub struct VStack<C> {
    children: C,
    spacing: Metric,
    alignment: HorizontalAlignment,
    distribution: MainAlign,
}

impl<C: View> VStack<C> {
    pub fn new(children: C) -> Self {
        Self {
            children,
            spacing: DEFAULT_SPACING.into(),
            alignment: HorizontalAlignment::Center,
            distribution: MainAlign::Start,
        }
    }

    /// The distance between children.
    #[must_use]
    pub fn spacing(mut self, spacing: impl Into<Metric>) -> Self {
        self.spacing = spacing.into();
        self
    }

    /// Where children sit across the stack.
    #[must_use]
    pub fn alignment(mut self, alignment: HorizontalAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// How leftover space is shared along the stack.
    #[must_use]
    pub fn distribution(mut self, distribution: MainAlign) -> Self {
        self.distribution = distribution;
        self
    }
}

impl<C: View> View for VStack<C> {
    fn mount(self, cx: &mut Cx<'_>) {
        stack(
            cx,
            Axis::Vertical,
            self.children,
            self.spacing,
            self.alignment.item(),
            self.distribution,
        );
    }
}

/// Children from the leading edge to the trailing edge; mirrored in
/// right-to-left layouts.
pub struct HStack<C> {
    children: C,
    spacing: Metric,
    alignment: VerticalAlignment,
    distribution: MainAlign,
}

impl<C: View> HStack<C> {
    pub fn new(children: C) -> Self {
        Self {
            children,
            spacing: DEFAULT_SPACING.into(),
            alignment: VerticalAlignment::Center,
            distribution: MainAlign::Start,
        }
    }

    #[must_use]
    pub fn spacing(mut self, spacing: impl Into<Metric>) -> Self {
        self.spacing = spacing.into();
        self
    }

    #[must_use]
    pub fn alignment(mut self, alignment: VerticalAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    #[must_use]
    pub fn distribution(mut self, distribution: MainAlign) -> Self {
        self.distribution = distribution;
        self
    }
}

impl<C: View> View for HStack<C> {
    fn mount(self, cx: &mut Cx<'_>) {
        stack(
            cx,
            Axis::Horizontal,
            self.children,
            self.spacing,
            self.alignment.item(),
            self.distribution,
        );
    }
}

/// Children on top of each other, later ones above.
pub struct ZStack<C> {
    children: C,
    alignment: Alignment,
}

impl<C: View> ZStack<C> {
    pub fn new(children: C) -> Self {
        Self {
            children,
            alignment: Alignment::CENTER,
        }
    }

    #[must_use]
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
}

impl<C: View> View for ZStack<C> {
    fn mount(self, cx: &mut Cx<'_>) {
        let node = overlay_node(cx, self.alignment);
        let env = cx.env.clone();
        cx.within(node, env).mount(self.children);
    }
}

/// A new node that places its children on top of each other.
#[inline(never)]
pub(super) fn overlay_node(cx: &mut Cx<'_>, alignment: Alignment) -> NodeId {
    let style = LayoutStyle {
        kind: LayoutKind::Overlay,
        ..LayoutStyle::default()
    };
    let node = cx.node(style);
    cx.inner.data(node).child_align = Some(alignment.items());
    node
}

/// Flexible space that takes what is left along a stack.
#[derive(Clone, Copy, Debug, Default)]
pub struct Spacer {
    min: f64,
}

impl Spacer {
    pub fn new() -> Self {
        Self::default()
    }

    /// The least space it keeps.
    #[must_use]
    pub fn min_length(mut self, min: f64) -> Self {
        self.min = min.max(0.0);
        self
    }
}

impl View for Spacer {
    fn mount(self, cx: &mut Cx<'_>) {
        let style = LayoutStyle {
            min: Size::new(self.min, self.min),
            shrink: true,
            ..LayoutStyle::default()
        };
        let flex = match stack_axis(cx.inner, cx.parent) {
            Some(Axis::Horizontal) => Flex {
                wide: true,
                tall: false,
            },
            Some(Axis::Vertical) => Flex {
                wide: false,
                tall: true,
            },
            None => Flex::default(),
        };
        let node = cx.node(style);
        cx.inner.make_flexible(node, flex);
    }
}

/// The axis of the stack that `parent` lays out, if it is a stack.
fn stack_axis(inner: &Inner, parent: NodeId) -> Option<Axis> {
    match inner.engine.style(parent).map(|style| &style.kind) {
        Some(LayoutKind::Stack { axis, .. }) => Some(*axis),
        _ => None,
    }
}

/// A thin line across a stack: horizontal in a vertical stack, vertical
/// in a horizontal one.
#[derive(Clone, Copy, Debug, Default)]
pub struct Divider;

impl View for Divider {
    fn mount(self, cx: &mut Cx<'_>) {
        divider_node(cx);
    }
}

/// A new separator control across the parent stack. The `Separator`
/// recipe gives its thickness and color.
#[inline(never)]
fn divider_node(cx: &mut Cx<'_>) {
    let vertical = stack_axis(cx.inner, cx.parent) == Some(Axis::Horizontal);
    let orientation = if vertical {
        SeparatorOrientation::Vertical
    } else {
        SeparatorOrientation::Horizontal
    };
    let node = cx.control(Box::new(Separator::new(orientation)));
    let style = LayoutStyle {
        align_self: Some(ItemAlign::Stretch),
        ..LayoutStyle::default()
    };
    if let Err(error) = cx.inner.engine.set_style(&cx.inner.tree, node, style) {
        cx.inner.report(super::host::ViewError::Layout(error));
    }
    // A divider spans its stack, so the stack takes the space offered.
    let flex = Flex {
        wide: !vertical,
        tall: vertical,
    };
    cx.inner.make_flexible(node, flex);
}

/// A column of a [`Grid`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridItem {
    /// A fixed width in points.
    Fixed(f64),
    /// A share of the leftover width, within `min..=max`.
    Flexible { min: f64, max: f64 },
    /// As wide as its widest cell.
    Fit,
}

impl GridItem {
    fn track(self) -> Track {
        match self {
            Self::Fixed(width) => Track::Fixed(width),
            Self::Flexible { min, max } => Track::Flex {
                weight: 1.0,
                min,
                max,
            },
            Self::Fit => Track::Auto,
        }
    }
}

/// Children in rows across the given columns, filled row by row.
pub struct Grid<C> {
    columns: Vec<GridItem>,
    children: C,
    spacing: Metric,
}

impl<C: View> Grid<C> {
    pub fn new(columns: Vec<GridItem>, children: C) -> Self {
        Self {
            columns,
            children,
            spacing: DEFAULT_SPACING.into(),
        }
    }

    #[must_use]
    pub fn spacing(mut self, spacing: impl Into<Metric>) -> Self {
        self.spacing = spacing.into();
        self
    }
}

impl<C: View> View for Grid<C> {
    fn mount(self, cx: &mut Cx<'_>) {
        let node = grid_node(cx, &self.columns, self.spacing);
        let env = cx.env.clone();
        cx.within(node, env).mount(self.children);
    }
}

/// A new grid node with `columns`.
#[inline(never)]
fn grid_node(cx: &mut Cx<'_>, columns: &[GridItem], spacing: Metric) -> NodeId {
    let tokens = parent_tokens(cx);
    let gap = spacing.resolve(&tokens);
    let columns: Vec<Track> = columns.iter().map(|item| item.track()).collect();
    let count = columns.len().max(1);
    let style = LayoutStyle::grid(columns, Vec::new(), gap);
    let node = cx.node(style);
    cx.inner.data(node).grid_columns = Some(count);
    node
}

/// Mounts children without a node of its own, so that a modifier can
/// apply to a group of views.
pub struct Group<C>(C);

impl<C: View> Group<C> {
    pub fn new(children: C) -> Self {
        Self(children)
    }
}

impl<C: View> View for Group<C> {
    fn mount(self, cx: &mut Cx<'_>) {
        cx.mount(self.0);
    }
}

/// Scroll state shared between a scroll view's node and its control.
#[derive(Default)]
pub(crate) struct ScrollShared {
    pub offset: Cell<Point>,
    pub content: Cell<Size>,
}

/// How far one wheel line or arrow key scrolls, in points.
const WHEEL_LINE: f64 = 40.0;

/// The control on a scroll view's node. Its [`OverlayScrollBar`] paints
/// the scroll bar with the `ScrollArea` recipe above the content, and
/// handles the thumb drag, the track paging, and the thumb hover. The
/// control also turns the wheel and, while it has focus, the scroll keys
/// into offsets. It sends each new offset as a `Changed` number.
struct ScrollControl {
    shared: Rc<ScrollShared>,
    axis: Axis,
    bar: OverlayScrollBar,
}

impl ScrollControl {
    /// Gives the bar the content length and the offset of the scroll node.
    fn sync(&mut self) {
        let (content, offset) = self.along();
        self.bar.set_scroll(content, offset);
    }

    /// The content length and the offset of the scroll node along the
    /// axis, as of the last layout or scroll.
    fn along(&self) -> (f64, f64) {
        let (content, offset) = (self.shared.content.get(), self.shared.offset.get());
        match self.axis {
            Axis::Vertical => (content.height, offset.y),
            Axis::Horizontal => (content.width, offset.x),
        }
    }

    /// Asks the scroll view to move to `offset` along its axis.
    fn scroll_to(&self, cx: &mut ControlContext<'_>, offset: f64) {
        let offset = offset.clamp(0.0, self.bar.maximum());
        if offset != self.bar.offset() {
            cx.emit(ControlMessage::Changed(ControlValue::Number(offset)));
        }
    }

    /// The wheel distance along the axis, in points.
    fn wheel(&self, delta: Point, unit: WheelUnit) -> f64 {
        let scale = match unit {
            WheelUnit::Points => 1.0,
            WheelUnit::Lines => WHEEL_LINE,
        };
        let amount = match self.axis {
            Axis::Vertical => delta.y,
            Axis::Horizontal if delta.x == 0.0 => delta.y,
            Axis::Horizontal => delta.x,
        };
        amount * scale
    }

    /// One page: the viewport less one line of overlap.
    fn page(&self) -> f64 {
        (self.bar.length() - WHEEL_LINE).max(WHEEL_LINE)
    }

    /// The offset that `key` moves to, if it scrolls along the axis.
    fn key_target(&self, key: Key) -> Option<f64> {
        let (back, forward) = match self.axis {
            Axis::Vertical => (Key::ArrowUp, Key::ArrowDown),
            Axis::Horizontal => (Key::ArrowLeft, Key::ArrowRight),
        };
        let offset = self.bar.offset();
        match key {
            key if key == back => Some(offset - WHEEL_LINE),
            key if key == forward => Some(offset + WHEEL_LINE),
            Key::PageUp => Some(offset - self.page()),
            Key::PageDown | Key::Space => Some(offset + self.page()),
            Key::Home => Some(0.0),
            Key::End => Some(self.bar.maximum()),
            _ => None,
        }
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, event: &PointerEvent) -> EventReply {
        if let PointerEvent::Wheel { delta, unit, .. } = *event {
            // The wheel scrolls after the content had its turn.
            let amount = self.wheel(delta, unit);
            if cx.phase() == EventPhase::Capture || amount == 0.0 {
                return EventReply::Continue;
            }
            self.scroll_to(cx, self.bar.offset() + amount);
            return EventReply::Stop;
        }
        // The bar is above the content, so it takes the pointer in the
        // capture phase, before the content under it.
        if cx.phase() == EventPhase::Bubble {
            return EventReply::Continue;
        }
        let dragging = self.bar.dragging();
        let reply = self.bar.pointer(event);
        if dragging != self.bar.dragging() {
            cx.capture_pointer(self.bar.dragging());
        }
        if reply.scrolled {
            cx.emit(ControlMessage::Changed(ControlValue::Number(
                self.bar.offset(),
            )));
        }
        if reply.repaint {
            cx.request_paint();
        }
        if reply.consumed {
            EventReply::Stop
        } else {
            EventReply::Continue
        }
    }

    /// Ends a drag and the hover, and gives back the pointer capture.
    fn release(&mut self, cx: &mut ControlContext<'_>) {
        let dragging = self.bar.dragging();
        if self.bar.release() {
            cx.request_paint();
        }
        if dragging {
            cx.capture_pointer(false);
        }
    }
}

impl CustomControl for ScrollControl {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.min)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.bar.arrange(cx, bounds);
        self.sync();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        self.sync();
        match event {
            ControlEvent::Pointer(event) => self.pointer(cx, event),
            ControlEvent::Key(KeyEvent::Down { key, .. })
                if cx.is_focused() && cx.phase() == EventPhase::Target =>
            {
                match self.key_target(*key) {
                    Some(target) => {
                        self.scroll_to(cx, target);
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                }
            }
            ControlEvent::Hover(false) | ControlEvent::Dismiss => {
                self.release(cx);
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    /// The layout updates the content size after it arranges the control,
    /// so paint takes the size and the offset from the scroll node.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let (content, offset) = self.along();
        let mut bar = self.bar.clone();
        bar.set_scroll(content, offset);
        bar.paint(cx);
    }

    /// The value is the position along the axis in whole percent.
    fn semantics(&self) -> SemanticNode {
        let (extent, position) = self.along();
        let range = (extent - self.bar.length()).max(0.0);
        let percent = if range > 0.0 {
            (position / range * 100.0).round()
        } else {
            0.0
        };
        SemanticNode {
            role: SemanticRole::ScrollArea,
            name: String::new(),
            description: String::new(),
            value: Some(format!("{percent}%")),
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Increment, SemanticAction::Decrement],
            text: None,
        }
    }

    /// `Increment` pages forward, and `Decrement` pages back.
    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        self.sync();
        let page = match action {
            SemanticAction::Increment => self.page(),
            SemanticAction::Decrement => -self.page(),
            _ => return false,
        };
        self.scroll_to(cx, self.bar.offset() + page);
        true
    }

    fn focusable(&self) -> bool {
        false
    }
}

/// Content that scrolls along one axis. The wheel and the scroll bar
/// scroll it, and moving focus to a control out of view scrolls that
/// control into view. The scroll bar takes its look from the
/// `ScrollArea` recipe. With `.focusable(true)`, the scroll view takes
/// focus, and the arrow, page, Home, and End keys scroll it.
pub struct ScrollView<C> {
    content: C,
    axis: Axis,
}

impl<C: View> ScrollView<C> {
    /// Vertically scrolling content.
    pub fn new(content: C) -> Self {
        Self {
            content,
            axis: Axis::Vertical,
        }
    }

    /// Content that scrolls along `axis`.
    #[must_use]
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }
}

impl<C: View> View for ScrollView<C> {
    fn mount(self, cx: &mut Cx<'_>) {
        let (node, content) = scroll_nodes(cx, self.axis);
        let env = cx.env.clone();
        let mut inside = cx.within(node, env);
        let env = inside.env.clone();
        inside.within(content, env).mount(self.content);
    }
}

/// A new scroll node along `axis`, and the column node inside it for the
/// content.
#[inline(never)]
fn scroll_nodes(cx: &mut Cx<'_>, axis: Axis) -> (NodeId, NodeId) {
    let shared = Rc::new(ScrollShared::default());
    let orientation = match axis {
        Axis::Vertical => ScrollOrientation::Vertical,
        Axis::Horizontal => ScrollOrientation::Horizontal,
    };
    let node = cx.control(Box::new(ScrollControl {
        shared: shared.clone(),
        axis,
        bar: OverlayScrollBar::new(orientation),
    }));
    let style = LayoutStyle {
        kind: LayoutKind::Scroll { axis },
        shrink: true,
        ..LayoutStyle::default()
    };
    if let Err(error) = cx.inner.engine.set_style(&cx.inner.tree, node, style) {
        cx.inner.report(super::host::ViewError::Layout(error));
    }
    // Like SwiftUI, a scroll view takes the space it is offered along
    // its axis, and a vertical one also takes the full width.
    let flex = match axis {
        Axis::Vertical => Flex::BOTH,
        Axis::Horizontal => Flex {
            wide: true,
            tall: false,
        },
    };
    cx.inner.make_flexible(node, flex);
    let data = cx.inner.data(node);
    data.scroll = Some(shared.clone());
    cx.inner.scrolls.insert(node);
    let data = cx.inner.data(node);
    data.internal = Some(Rc::new(
        move |inner: &mut Inner, message: &ControlMessage| {
            if let ControlMessage::Changed(ControlValue::Number(along)) = message {
                let offset = inner.engine.scroll_offset(node);
                let next = match axis {
                    Axis::Vertical => Point::new(offset.x, *along),
                    Axis::Horizontal => Point::new(*along, offset.y),
                };
                set_scroll(inner, node, next);
            }
        },
    ));
    // The content is one column, as the scroll container lays out its
    // first child only.
    let column = LayoutStyle {
        kind: LayoutKind::Stack {
            axis: Axis::Vertical,
            spacing: 0.0,
            main_align: MainAlign::Start,
            cross_align: ItemAlign::Stretch,
        },
        ..LayoutStyle::default()
    };
    let env = cx.env.clone();
    let content = cx.within(node, env).node(column);
    (node, content)
}

/// Moves the scroll node to `offset`, within its content.
pub(crate) fn set_scroll(inner: &mut Inner, node: NodeId, offset: Point) {
    let bounds = inner.tree.bounds(node).unwrap_or_default();
    let content = inner.engine.content_size(node).unwrap_or(bounds.size);
    let max = Point::new(
        (content.width - bounds.size.width).max(0.0),
        (content.height - bounds.size.height).max(0.0),
    );
    let clamped = Point::new(offset.x.clamp(0.0, max.x), offset.y.clamp(0.0, max.y));
    if clamped != inner.engine.scroll_offset(node) {
        inner.engine.set_scroll_offset(node, clamped);
        // The control reads the new offset before the next layout.
        if let Some(shared) = inner.nodes.get(&node).and_then(|data| data.scroll.as_ref()) {
            shared.offset.set(clamped);
        }
        inner.needs_layout = true;
        inner.touch(node, Dirty::PAINT.with(Dirty::SEMANTICS));
    }
}
