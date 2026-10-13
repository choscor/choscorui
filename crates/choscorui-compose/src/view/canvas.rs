//! Custom drawing: [`Canvas`] draws with display-list commands on a node.
//! It is built on `CanvasView`, which gives the zoom, the pan, and the
//! transform from canvas coordinates to the window.

use super::View;
use super::host::Cx;
use choscorui_controls::{
    CanvasInput, CanvasView, ControlContext, ControlEvent, CustomControl, MeasureContext,
    PaintContext,
};
use choscorui_core::{
    DesignTokens, DisplayList, EventPhase, EventReply, KeyEvent, Measured, Point, PointerButton,
    PointerEvent, Rect, Scene, SemanticAction, SemanticNode, SemanticRole, SemanticState, Size,
    WheelUnit,
};
use std::borrow::Cow;
use std::rc::Rc;

/// Points that one wheel line pans, as in a hosted canvas.
const WHEEL_LINE: f64 = 20.0;

/// What a [`Canvas`] draws with. The display list draws in canvas
/// coordinates: the point (0, 0) is the top-left corner of the node at
/// zoom 1 and with no pan.
pub struct CanvasContext<'a> {
    pub list: &'a mut DisplayList,
    /// The part of the canvas in view, in canvas coordinates. Draw only
    /// what touches it.
    pub bounds: Rect,
    pub tokens: &'a DesignTokens,
    pub rtl: bool,
    /// The scale from canvas coordinates to points. It is 1 unless the
    /// canvas is zoomable and the user zoomed.
    pub zoom: f64,
}

type Draw = Rc<dyn Fn(&mut CanvasContext<'_>, Size)>;

/// The control of a [`Canvas`]: a `CanvasView` that has no scene items.
struct CanvasLeaf {
    draw: Draw,
    view: CanvasView,
    /// An empty scene, because `CanvasView::handle` takes one.
    scene: Scene,
    zoomable: bool,
    /// The last pointer position of a pan drag.
    drag: Option<Point>,
}

impl CanvasLeaf {
    /// Stops a pan drag.
    fn end_drag(&mut self, cx: &mut ControlContext<'_>) {
        if self.drag.take().is_some() {
            cx.capture_pointer(false);
        }
    }
}

impl CustomControl for CanvasLeaf {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let max = cx.constraints.max;
        let min = cx.constraints.min;
        Measured::new(Size::new(
            if max.width.is_finite() {
                max.width
            } else {
                min.width
            },
            if max.height.is_finite() {
                max.height
            } else {
                min.height
            },
        ))
    }

    fn arrange(&mut self, _cx: &MeasureContext<'_>, bounds: Rect) {
        self.view.set_viewport(bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if !self.zoomable || cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let modifiers = cx.modifiers();
        let input = match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => {
                self.drag = Some(*position);
                cx.capture_pointer(true);
                return EventReply::Stop;
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                let Some(last) = self.drag.replace(*position) else {
                    return EventReply::Continue;
                };
                // The content follows the pointer.
                if self.view.pan(last.x - position.x, last.y - position.y) {
                    cx.request_paint();
                }
                return EventReply::Stop;
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) if self.drag.is_some() => {
                self.end_drag(cx);
                return EventReply::Stop;
            }
            ControlEvent::Pointer(PointerEvent::Wheel {
                position,
                delta,
                unit,
            }) => {
                let scale = match unit {
                    WheelUnit::Points => 1.0,
                    WheelUnit::Lines => WHEEL_LINE,
                };
                CanvasInput::Wheel {
                    dx: delta.x * scale,
                    dy: delta.y * scale,
                    at: *position,
                    zoom: modifiers.command,
                }
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => CanvasInput::Key {
                key: *key,
                shift: modifiers.shift,
            },
            ControlEvent::FocusChanged(false) | ControlEvent::Dismiss => {
                self.end_drag(cx);
                return EventReply::Continue;
            }
            _ => return EventReply::Continue,
        };
        // With no scene items, the input can only pan or zoom.
        if self.view.handle(&mut self.scene, input).repaint {
            cx.request_paint();
            EventReply::Stop
        } else {
            EventReply::Continue
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let view = if self.view.viewport() == cx.bounds {
            Cow::Borrowed(&self.view)
        } else {
            let mut view = self.view.clone();
            view.set_viewport(cx.bounds);
            Cow::Owned(view)
        };
        // Paint has no error channel. The push fails only for a transform
        // that is not finite, and then the canvas draws nothing.
        if cx.list.push_transform(view.transform()).is_err() {
            return;
        }
        let mut canvas = CanvasContext {
            list: cx.list,
            bounds: view.visible_scene_rect(),
            tokens: cx.tokens,
            rtl: cx.rtl,
            zoom: view.zoom(),
        };
        (self.draw)(&mut canvas, cx.bounds.size);
        let _ = cx.list.pop_transform();
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Image,
            name: self.view.name.clone(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: if self.zoomable {
                vec![SemanticAction::Focus]
            } else {
                Vec::new()
            },
            text: None,
        }
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if !self.zoomable || action != SemanticAction::Focus {
            return false;
        }
        cx.request_focus();
        true
    }

    fn focusable(&self) -> bool {
        self.zoomable
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.drag = None;
        }
    }
}

/// Custom drawing: `Canvas::new(|ctx, size| …)` draws with display-list
/// commands in the space the canvas gets. It fills the space it is
/// offered; give it a size with `.frame(...)`.
///
/// The canvas wraps a `CanvasView`. The closure draws in canvas
/// coordinates, which start at the top-left corner of the node, and
/// `size` is the size of the node in points.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_core::{Color, Rect};
/// let bars = Canvas::new(|ctx, size| {
///     for (index, value) in [0.4, 0.9, 0.6].into_iter().enumerate() {
///         let height = size.height * value;
///         let x = index as f64 * 24.0;
///         let bar = Rect::from_xywh(x, size.height - height, 16.0, height);
///         ctx.list.fill_rect(bar, Color::rgb8(0x25, 0x63, 0xeb));
///     }
/// })
/// .label("Sales by month");
/// ```
pub struct Canvas {
    draw: Draw,
    label: String,
    zoomable: bool,
}

impl Canvas {
    /// A canvas that `draw` paints, again after each change that it read.
    pub fn new(draw: impl Fn(&mut CanvasContext<'_>, Size) + 'static) -> Self {
        Self {
            draw: Rc::new(draw),
            label: String::new(),
            zoomable: false,
        }
    }

    /// The name that assistive technology reads for the drawing.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Lets the user pan and zoom the drawing, as `CanvasView` does:
    ///
    /// - A drag or the wheel pans. Command with the wheel zooms about the
    ///   pointer.
    /// - The arrow keys pan. `+`, `-`, and `0` zoom about the center.
    ///
    /// A zoomable canvas takes focus. The default is `false`.
    #[must_use]
    pub fn zoomable(mut self, zoomable: bool) -> Self {
        self.zoomable = zoomable;
        self
    }
}

impl View for Canvas {
    fn mount(self, cx: &mut Cx) {
        let hidden = self.label.is_empty();
        let node = cx.control(Box::new(CanvasLeaf {
            draw: self.draw,
            view: CanvasView::new(self.label, Rect::default()),
            scene: Scene::new(),
            zoomable: self.zoomable,
            drag: None,
        }));
        if hidden {
            cx.inner.data(node).access.hidden = true;
        }
    }
}
