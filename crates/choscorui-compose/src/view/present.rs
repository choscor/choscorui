//! Presentation: dialogs, sheets, popovers, menus, and hover cards. A
//! presented view mounts above the window content in a layer. In a modal
//! layer, focus moves into it and stays there; Escape or a press on the
//! scrim closes it; closing returns focus to where it was. A layer that
//! is not modal takes no input and no focus, as a hover card. The layer
//! animates in and out as [`Motion`] describes, and unmounts when its
//! close motion ends.

use super::host::{Cx, Inner, ScopeId};
use super::layout::{Alignment, HorizontalAlignment, VerticalAlignment, parent_tokens};
use super::modifier::Styled;
use super::motion::{Motion, Movement};
use super::scope;
use super::state::{Binding, Live};
use super::text::TextContent;
use super::{AnyView, View};
use crate::reactive::Runtime;
use choscorui_controls::{
    ControlContext, ControlEvent, ControlKind, ControlMessage, CustomControl, MeasureContext,
    PaintContext, SheetSide, Slot, StyleQuery, Variant, paint_box,
};
use choscorui_core::{
    Axis, Dirty, EventPhase, EventReply, Insets, ItemAlign, Key, KeyEvent, LayoutKind, LayoutStyle,
    Length, MainAlign, Measured, NodeId, Point, PointerButton, PointerEvent, Rect, SemanticNode,
    SemanticRole, SemanticState, Size,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Where a presented view sits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Placement {
    /// Centered over the window.
    Center,
    /// Along an edge of the window.
    Edge(SheetSide),
    /// Next to the view that presents it.
    Anchored,
}

/// How a presented view looks and sits.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Presentation {
    pub kind: ControlKind,
    pub variant: Variant,
    pub placement: Placement,
    /// Whether the panel paints the surface of its kind's root slot. A
    /// menu paints its own panel.
    pub surface: bool,
    /// Whether the layer keeps focus and input inside. A layer that is
    /// not modal lets the pointer through and takes no focus.
    pub modal: bool,
}

impl Presentation {
    /// The motion of the panel.
    fn movement(self) -> Movement {
        match self.placement {
            Placement::Center => Movement::Zoom,
            Placement::Anchored => Movement::Anchored,
            Placement::Edge(side) => Movement::Edge(side),
        }
    }

    fn query(self) -> StyleQuery {
        StyleQuery::new(self.kind, Slot::Root).variant(self.variant)
    }
}

/// What an anchored panel sits next to.
#[derive(Clone, Debug)]
pub(crate) enum Anchor {
    /// A node: the panel sits below it, or above it when it does not fit.
    Node(NodeId),
    /// A point in the window, such as the pointer of a secondary click:
    /// the panel's top-start corner sits at the point.
    Point(Rc<Cell<Point>>),
}

impl Anchor {
    /// Whether the anchor is still in `tree`.
    pub(crate) fn exists(&self, tree: &choscorui_core::Tree) -> bool {
        match self {
            Self::Node(node) => tree.contains(*node),
            Self::Point(_) => true,
        }
    }
}

/// An open layer: its scope, the node that had focus before, the layer
/// node, and its motion.
struct Open {
    scope: ScopeId,
    returns: Option<NodeId>,
    layer: NodeId,
    motion: Rc<Motion>,
}

/// The distance between a popover and the view that presents it.
const ANCHOR_GAP: f64 = 4.0;

/// The share of a drawer's size that a drag must pass to dismiss it.
const DRAG_DISMISS: f64 = 0.25;

/// The modal layer: it paints the scrim, keeps focus and input inside,
/// and reports Escape and presses outside the panel as `Dismissed`.
/// While it closes, presses go through it to the window content, and it
/// reports `OpenChanged(false)` when the close motion ends.
struct ModalLayer {
    query: StyleQuery,
    scrim: bool,
    modal: bool,
    panel: Rc<Cell<Rect>>,
    motion: Rc<Motion>,
}

impl CustomControl for ModalLayer {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.min)
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if !self.motion.is_open() {
            return match event {
                ControlEvent::Timer => {
                    cx.emit(ControlMessage::OpenChanged(false));
                    EventReply::Stop
                }
                _ => EventReply::Continue,
            };
        }
        if cx.phase() == EventPhase::Capture {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Escape, ..
            }) => {
                cx.emit(ControlMessage::Dismissed);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Down { position, .. })
                if !self.panel.get().contains(*position) =>
            {
                cx.emit(ControlMessage::Dismissed);
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        if !self.scrim {
            return;
        }
        let progress = self.motion.progress(cx.tokens, cx.time);
        if progress <= 0.0 {
            return;
        }
        let style = cx.style(self.query.slot(Slot::Overlay));
        let faded = progress < 1.0 && cx.list.push_opacity(progress).is_ok();
        paint_box(cx.list, cx.bounds, &style, cx.rtl);
        if faded {
            let _ = cx.list.pop_opacity();
        }
    }

    fn modal(&self) -> bool {
        self.modal && self.motion.is_open()
    }

    fn animating(&self, time: f64) -> bool {
        self.motion.animating(time)
    }

    fn deadline(&self) -> Option<f64> {
        self.motion.closes_at()
    }

    fn semantics(&self) -> SemanticNode {
        node(SemanticRole::Group, String::new())
    }

    fn focusable(&self) -> bool {
        false
    }
}

/// A semantic node of `role` named `name`, with nothing else.
fn node(role: SemanticRole, name: String) -> SemanticNode {
    SemanticNode {
        role,
        name,
        description: String::new(),
        value: None,
        state: SemanticState::default(),
        relations: Vec::new(),
        actions: Vec::new(),
        text: None,
    }
}

/// The panel of a presented view: it paints the surface of its control
/// kind's root slot around the content. A drawer panel follows a pointer
/// drag toward its edge, and a drag past a quarter of its size dismisses
/// it.
struct Panel {
    query: StyleQuery,
    bounds: Rc<Cell<Rect>>,
    role: SemanticRole,
    surface: bool,
    /// The accessible name, from the presentation's title.
    name: Rc<RefCell<String>>,
    motion: Rc<Motion>,
    draggable: bool,
    /// Where the current drag started.
    drag: Option<Point>,
}

impl Panel {
    /// How far a drag from `start` to `position` moves toward the edge.
    fn offset(&self, start: Point, position: Point) -> f64 {
        let (ux, uy) = self.motion.outward().unwrap_or((0.0, 1.0));
        ((position.x - start.x) * ux + (position.y - start.y) * uy).max(0.0)
    }

    /// The panel's size along its dismiss direction.
    fn extent(&self) -> f64 {
        let bounds = self.bounds.get();
        match self.motion.outward() {
            Some((ux, _)) if ux != 0.0 => bounds.size.width,
            _ => bounds.size.height,
        }
    }
}

impl CustomControl for Panel {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.min)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.bounds.set(bounds);
        self.motion.set_rtl(cx.rtl());
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if !self.draggable || !self.motion.is_open() || cx.phase() == EventPhase::Capture {
            return EventReply::Continue;
        }
        let ControlEvent::Pointer(pointer) = event else {
            return EventReply::Continue;
        };
        match (*pointer, self.drag) {
            (
                PointerEvent::Down {
                    position,
                    button: PointerButton::Primary,
                },
                _,
            ) => {
                self.drag = Some(position);
                cx.capture_pointer(true);
            }
            (PointerEvent::Move { position }, Some(start)) => {
                self.motion.set_drag(self.offset(start, position));
                cx.request_paint();
            }
            (PointerEvent::Up { position, .. }, Some(start)) => {
                self.drag = None;
                cx.capture_pointer(false);
                if self.offset(start, position) > self.extent() * DRAG_DISMISS {
                    // The close motion goes on from where the drag ended.
                    cx.emit(ControlMessage::Dismissed);
                } else {
                    self.motion.set_drag(0.0);
                }
                cx.request_paint();
            }
            _ => return EventReply::Continue,
        }
        EventReply::Stop
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        if self.surface {
            let style = cx.style(self.query);
            paint_box(cx.list, cx.bounds, &style, cx.rtl);
        }
    }

    fn semantics(&self) -> SemanticNode {
        node(self.role, self.name.borrow().clone())
    }

    fn focusable(&self) -> bool {
        false
    }
}

/// The alignment of the panel in the modal layer.
fn alignment(placement: Placement) -> Alignment {
    match placement {
        Placement::Center => Alignment::CENTER,
        Placement::Anchored => Alignment::TOP_LEADING,
        Placement::Edge(SheetSide::Right) => {
            Alignment::new(HorizontalAlignment::Trailing, VerticalAlignment::Fill)
        }
        Placement::Edge(SheetSide::Left) => {
            Alignment::new(HorizontalAlignment::Leading, VerticalAlignment::Fill)
        }
        Placement::Edge(SheetSide::Top) => {
            Alignment::new(HorizontalAlignment::Fill, VerticalAlignment::Top)
        }
        Placement::Edge(SheetSide::Bottom) => {
            Alignment::new(HorizontalAlignment::Fill, VerticalAlignment::Bottom)
        }
    }
}

/// Mounts the layer with `content` in the root's fragment. The panel
/// records its bounds in `panel_bounds`. Returns the layer node, the
/// panel node, and the motion of both.
fn mount_layer(
    cx: &mut Cx<'_>,
    (presentation, title): (Presentation, Option<Live<TextContent>>),
    panel_bounds: Rc<Cell<Rect>>,
    content: impl View,
) -> (NodeId, NodeId, Rc<Motion>) {
    let query = presentation.query();
    let tokens = parent_tokens(cx);
    let style = cx.inner.styles.resolve(&tokens, query);
    let motion = Rc::new(Motion::opening(
        presentation.movement(),
        style.motion_ms,
        cx.inner.time,
    ));
    let layer = cx.control(Box::new(ModalLayer {
        query,
        scrim: presentation.placement != Placement::Anchored,
        modal: presentation.modal,
        panel: panel_bounds.clone(),
        motion: motion.clone(),
    }));
    let overlay = LayoutStyle {
        kind: LayoutKind::Overlay,
        align_self: Some(ItemAlign::Stretch),
        justify_self: Some(ItemAlign::Stretch),
        ..LayoutStyle::default()
    };
    cx.inner.set_style(layer, overlay);
    {
        let data = cx.inner.data(layer);
        data.child_align = Some(alignment(presentation.placement).items());
    }
    let rtl = cx.env.direction() == choscorui_core::LayoutDirection::Rtl;
    let (left, right) = if rtl {
        (style.padding.inline_end, style.padding.inline_start)
    } else {
        (style.padding.inline_start, style.padding.inline_end)
    };
    // A panel without a surface adds no space around its content.
    let padding = if presentation.surface {
        Insets {
            left,
            top: style.padding.block_start,
            right,
            bottom: style.padding.block_end,
        }
    } else {
        Insets::default()
    };
    let min = if presentation.surface {
        Size::new(style.min_width, style.min_height)
    } else {
        Size::ZERO
    };
    let role = match presentation.kind {
        ControlKind::Dialog
        | ControlKind::AlertDialog
        | ControlKind::Sheet
        | ControlKind::Drawer => SemanticRole::Dialog,
        _ => SemanticRole::Group,
    };
    let env = cx.env.clone();
    let mut inside = cx.within(layer, env);
    let name: Rc<RefCell<String>> = Rc::default();
    let panel = inside.control(Box::new(Panel {
        query,
        bounds: panel_bounds,
        role,
        surface: presentation.surface,
        name: name.clone(),
        motion: motion.clone(),
        draggable: presentation.kind == ControlKind::Drawer,
        drag: None,
    }));
    inside.inner.data(panel).motion = Some(motion.clone());
    if let Some(title) = title {
        let env = inside.env.clone();
        inside.react(move |inner| {
            let text = title.get().resolve(&env);
            if *name.borrow() != text {
                *name.borrow_mut() = text;
                inner.touch(panel, Dirty::SEMANTICS);
            }
        });
    }
    let max_width = if presentation.surface && style.max_width > 0.0 {
        style.max_width
    } else {
        f64::INFINITY
    };
    let column = LayoutStyle {
        kind: LayoutKind::Stack {
            axis: Axis::Vertical,
            spacing: style.gap,
            main_align: MainAlign::Start,
            cross_align: ItemAlign::Stretch,
        },
        padding,
        min,
        max: Size::new(max_width, f64::INFINITY),
        width: if max_width.is_finite() && presentation.placement == Placement::Center {
            Length::Points(max_width)
        } else {
            Length::Auto
        },
        shrink: true,
        ..LayoutStyle::default()
    };
    inside.inner.set_style(panel, column);
    let env = inside.env.clone();
    inside.within(panel, env).mount(content);
    if !presentation.modal {
        // The pointer goes through the layer, and Tab skips its content.
        if let Err(error) = cx.inner.tree.set_hit_testable(layer, false) {
            cx.inner.report(super::host::ViewError::Tree(error));
        }
        unfocus(cx.inner, layer);
    }
    (layer, panel, motion)
}

/// Keeps an anchored panel next to its anchor after layout, inside the
/// window: below the anchor when it fits, otherwise above it.
pub(crate) fn place_anchored(inner: &mut Inner, layer: NodeId, anchor: &Anchor) -> bool {
    let (target, gap) = match anchor {
        Anchor::Node(node) => (inner.tree.bounds(*node), ANCHOR_GAP),
        Anchor::Point(point) => (Some(Rect::new(point.get(), Size::ZERO)), 0.0),
    };
    let (Some(target), Some(window)) = (target, inner.tree.bounds(layer)) else {
        return false;
    };
    let first = inner
        .tree
        .children(layer)
        .and_then(|children| children.first())
        .copied();
    let panel = first
        .and_then(|panel| inner.tree.bounds(panel))
        .map_or(Size::ZERO, |bounds| bounds.size);
    let rtl = inner.direction == choscorui_core::LayoutDirection::Rtl;
    let room = (window.size.width - panel.width).max(0.0);
    let start = if rtl {
        window.max_x() - target.max_x()
    } else {
        target.min_x() - window.min_x()
    };
    let start = start.clamp(0.0, room);
    let below = target.max_y() + gap - window.min_y();
    let fits = below + panel.height <= window.size.height;
    let top = if fits {
        below
    } else {
        (target.min_y() - gap - panel.height - window.min_y()).max(0.0)
    };
    if let Some(motion) = first
        .and_then(|panel| inner.nodes.get(&panel))
        .and_then(|data| data.motion.as_ref())
    {
        motion.set_above(!fits);
    }
    let padding = Insets {
        left: if rtl { 0.0 } else { start },
        top,
        right: if rtl { start } else { 0.0 },
        bottom: 0.0,
    };
    let mut style = inner.engine.style(layer).cloned().unwrap_or_default();
    if style.padding == padding {
        return false;
    }
    style.padding = padding;
    inner.set_style(layer, style);
    true
}

/// Adds a presentation of `content` to a view, shown while `presented`
/// is true. `title` names the panel for assistive technology.
pub(crate) fn present<V: View, C: View>(
    view: Styled<V>,
    presented: Binding<bool>,
    (presentation, title): (Presentation, Option<Live<TextContent>>),
    content: impl Fn() -> C + 'static,
) -> Styled<V> {
    let content: Content = Rc::new(move || content().into_any());
    view.present(move |cx, anchor| {
        mount_presentation(
            cx,
            Anchor::Node(anchor),
            presented,
            (presentation, title.clone()),
            (Rc::default(), content.clone()),
        );
    })
}

/// Makes the content of a presentation each time it opens.
pub(crate) type Content = Rc<dyn Fn() -> AnyView>;

/// Removes the nodes of `fragment`: an open or a closing layer.
fn clear(inner: &mut Inner, fragment: super::fragment::FragmentId) {
    let nodes = inner.fragments.clear(fragment);
    inner.remove_nodes(&nodes);
}

/// Takes `layer` out of hit testing, focus traversal, and the semantic
/// tree while its close motion runs.
fn retire(inner: &mut Inner, layer: NodeId) {
    inner.data(layer).access.hidden = true;
    if let Err(error) = inner.tree.set_hit_testable(layer, false) {
        inner.report(super::host::ViewError::Tree(error));
    }
    unfocus(inner, layer);
}

/// Takes `layer` and its subtree out of focus traversal.
fn unfocus(inner: &mut Inner, layer: NodeId) {
    let mut stack = vec![layer];
    while let Some(node) = stack.pop() {
        if let Err(error) = inner.tree.set_focusable(node, false) {
            inner.report(super::host::ViewError::Tree(error));
        }
        inner.touch(node, Dirty::SEMANTICS);
        if let Some(children) = inner.tree.children(node) {
            stack.extend_from_slice(children);
        }
    }
}

/// Shows `content` in a layer above the window while `presented` is
/// true. An anchored layer sits next to `anchor`. Focus moves into a
/// modal layer when it opens and returns when it closes. The panel
/// records its bounds in `panel_bounds`. The closed layer stays, without
/// input or semantics, until its close motion ends.
pub(crate) fn mount_presentation(
    cx: &mut Cx<'_>,
    anchor: Anchor,
    presented: Binding<bool>,
    (presentation, title): (Presentation, Option<Live<TextContent>>),
    (panel_bounds, content): (Rc<Cell<Rect>>, Content),
) {
    let root = cx.inner.tree.root();
    let root_fragment = cx.inner.fragments.root_of(root);
    let fragment = cx.inner.fragments.child(root_fragment);
    let owner = cx.scope;
    let env = cx.env.clone();
    let open: Rc<RefCell<Option<Open>>> = Rc::default();
    // A layer goes with the view that presents it.
    cx.inner
        .on_dispose(owner, Box::new(move |inner| clear(inner, fragment)));
    cx.react(move |inner| {
        let wanted = presented.try_get().unwrap_or(false);
        Runtime::untracked(|| {
            let current = open.borrow_mut().take();
            match (wanted, current) {
                (true, None) => {
                    // A layer that still closes goes at once.
                    clear(inner, fragment);
                    let returns = inner.tree.focus().filter(|_| presentation.modal);
                    let scope = inner.child_scope(owner);
                    let _owner = scope::enter(inner.owned(scope));
                    let mut layer_cx = Cx {
                        inner: &mut *inner,
                        parent: root,
                        fragment,
                        scope,
                        env: env.clone(),
                    };
                    let (layer, panel, motion) = mount_layer(
                        &mut layer_cx,
                        (presentation, title.clone()),
                        panel_bounds.clone(),
                        content(),
                    );
                    let dismiss: super::host::Handler = Rc::new(move |message: &ControlMessage| {
                        if *message == ControlMessage::Dismissed {
                            presented.set(false);
                        }
                    });
                    inner.data(layer).handlers.push(dismiss.clone());
                    inner.data(panel).handlers.push(dismiss);
                    inner.data(layer).internal = Some(Rc::new(move |inner, message| {
                        if *message == ControlMessage::OpenChanged(false) {
                            clear(inner, fragment);
                        }
                    }));
                    if presentation.placement == Placement::Anchored {
                        inner.anchors.insert(layer, anchor.clone());
                    }
                    if presentation.modal {
                        inner.controls.focus_next(&mut inner.tree, false);
                    }
                    *open.borrow_mut() = Some(Open {
                        scope,
                        returns,
                        layer,
                        motion,
                    });
                }
                (false, Some(layer)) => {
                    inner.dispose_scope(layer.scope);
                    if let Some(node) = layer.returns.filter(|node| inner.tree.contains(*node)) {
                        inner.controls.move_focus(&mut inner.tree, Some(node));
                    }
                    let tokens = inner
                        .themes
                        .resolve(&inner.tree, layer.layer)
                        .unwrap_or_else(|_| inner.themes.application());
                    let duration = inner
                        .styles
                        .resolve(&tokens, presentation.query())
                        .motion_ms;
                    if duration == 0 {
                        clear(inner, fragment);
                    } else {
                        layer.motion.close(duration, inner.time);
                        retire(inner, layer.layer);
                    }
                }
                (_, current) => *open.borrow_mut() = current,
            }
        });
    });
}
