//! Presentation: dialogs, sheets, and popovers. A presented view mounts
//! above the window content in a modal layer. Focus moves into it and
//! stays there; Escape or a press on the scrim closes it; closing returns
//! focus to where it was.

use super::View;
use super::host::{Cx, Inner};
use super::layout::{Alignment, HorizontalAlignment, VerticalAlignment, parent_tokens};
use super::modifier::Styled;
use super::scope;
use super::state::Binding;
use crate::reactive::Runtime;
use choscorui_controls::{
    ControlContext, ControlEvent, ControlKind, ControlMessage, CustomControl, MeasureContext,
    PaintContext, SheetSide, Slot, StyleQuery, Variant, paint_box,
};
use choscorui_core::{
    Axis, EventPhase, EventReply, Insets, ItemAlign, Key, KeyEvent, LayoutKind, LayoutStyle,
    Length, MainAlign, Measured, NodeId, PointerEvent, Rect, SemanticNode, SemanticRole,
    SemanticState, Size,
};
use std::cell::Cell;
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
}

/// The scope of an open layer, and the node that had focus before.
type Mounted = (super::host::ScopeId, Option<NodeId>);

/// The distance between a popover and the view that presents it.
const ANCHOR_GAP: f64 = 4.0;

/// The modal layer: it paints the scrim, keeps focus and input inside,
/// and reports Escape and presses outside the panel as `Dismissed`.
struct ModalLayer {
    query: StyleQuery,
    scrim: bool,
    panel: Rc<Cell<Rect>>,
}

impl CustomControl for ModalLayer {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.min)
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
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
        if self.scrim {
            let style = cx.style(self.query.slot(Slot::Overlay));
            paint_box(cx.list, cx.bounds, &style, cx.rtl);
        }
    }

    fn modal(&self) -> bool {
        true
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Group,
            name: String::new(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        }
    }

    fn focusable(&self) -> bool {
        false
    }
}

/// The panel of a presented view: it paints the surface of its control
/// kind's root slot around the content.
struct Panel {
    query: StyleQuery,
    bounds: Rc<Cell<Rect>>,
    role: SemanticRole,
}

impl CustomControl for Panel {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.min)
    }

    fn arrange(&mut self, _cx: &MeasureContext<'_>, bounds: Rect) {
        self.bounds.set(bounds);
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let style = cx.style(self.query);
        paint_box(cx.list, cx.bounds, &style, cx.rtl);
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: self.role,
            name: String::new(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        }
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

/// Mounts the modal layer with `content` in the root's fragment, and
/// returns the layer node.
fn mount_layer(cx: &mut Cx<'_>, presentation: Presentation, content: impl View) -> NodeId {
    let query = StyleQuery::new(presentation.kind, Slot::Root).variant(presentation.variant);
    let panel_bounds: Rc<Cell<Rect>> = Rc::default();
    let layer = cx.control(Box::new(ModalLayer {
        query,
        scrim: presentation.placement != Placement::Anchored,
        panel: panel_bounds.clone(),
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
    let tokens = parent_tokens(cx);
    let style = cx.inner.styles.resolve(&tokens, query);
    let rtl = cx.env.direction() == choscorui_core::LayoutDirection::Rtl;
    let (left, right) = if rtl {
        (style.padding.inline_end, style.padding.inline_start)
    } else {
        (style.padding.inline_start, style.padding.inline_end)
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
    let panel = inside.control(Box::new(Panel {
        query,
        bounds: panel_bounds,
        role,
    }));
    let max_width = if style.max_width > 0.0 {
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
        padding: Insets {
            left,
            top: style.padding.block_start,
            right,
            bottom: style.padding.block_end,
        },
        min: Size::new(style.min_width, style.min_height),
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
    layer
}

/// Keeps an anchored panel next to its anchor after layout, inside the
/// window: below the anchor when it fits, otherwise above it.
pub(crate) fn place_anchored(inner: &mut Inner, layer: NodeId, anchor: NodeId) -> bool {
    let (Some(target), Some(window)) = (inner.tree.bounds(anchor), inner.tree.bounds(layer)) else {
        return false;
    };
    let panel = inner
        .tree
        .children(layer)
        .and_then(|children| children.first())
        .and_then(|panel| inner.tree.bounds(*panel))
        .map_or(Size::ZERO, |bounds| bounds.size);
    let rtl = inner.direction == choscorui_core::LayoutDirection::Rtl;
    let room = (window.size.width - panel.width).max(0.0);
    let start = if rtl {
        window.max_x() - target.max_x()
    } else {
        target.min_x() - window.min_x()
    };
    let start = start.clamp(0.0, room);
    let below = target.max_y() + ANCHOR_GAP - window.min_y();
    let top = if below + panel.height <= window.size.height {
        below
    } else {
        (target.min_y() - ANCHOR_GAP - panel.height - window.min_y()).max(0.0)
    };
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
/// is true.
pub(crate) fn present<V: View, C: View>(
    view: Styled<V>,
    presented: Binding<bool>,
    presentation: Presentation,
    content: impl Fn() -> C + 'static,
) -> Styled<V> {
    let content = Rc::new(content);
    view.present(move |cx, anchor| {
        let root = cx.inner.tree.root();
        let root_fragment = cx.inner.fragments.root_of(root);
        let fragment = cx.inner.fragments.child(root_fragment);
        let owner = cx.scope;
        let env = cx.env.clone();
        let content = content.clone();
        // The scope of the open layer, and the focus to return on close.
        let mounted: Rc<Cell<Option<Mounted>>> = Rc::default();
        cx.react(move |inner| {
            let open = presented.try_get().unwrap_or(false);
            Runtime::untracked(|| match (open, mounted.get()) {
                (true, None) => {
                    let returns = inner.tree.focus();
                    let scope = inner.child_scope(owner);
                    let _owner = scope::enter(inner.owned(scope));
                    let mut layer_cx = Cx {
                        inner: &mut *inner,
                        parent: root,
                        fragment,
                        scope,
                        env: env.clone(),
                    };
                    let layer = mount_layer(&mut layer_cx, presentation, content());
                    let data = inner.data(layer);
                    data.handlers.push(Rc::new(move |message: &ControlMessage| {
                        if *message == ControlMessage::Dismissed {
                            presented.set(false);
                        }
                    }));
                    if presentation.placement == Placement::Anchored {
                        inner.anchors.insert(layer, anchor);
                    }
                    inner.controls.focus_next(&mut inner.tree, false);
                    mounted.set(Some((scope, returns)));
                }
                (false, Some((scope, returns))) => {
                    inner.dispose_scope(scope);
                    let nodes = inner.fragments.clear(fragment);
                    inner.remove_nodes(&nodes);
                    if let Some(node) = returns.filter(|node| inner.tree.contains(*node)) {
                        inner.controls.move_focus(&mut inner.tree, Some(node));
                    }
                    mounted.set(None);
                }
                _ => {}
            });
        });
    })
}
