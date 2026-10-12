//! Tooltips. The `tooltip` modifier wraps a view in a control that
//! watches hover and keyboard focus inside it. All tooltips of a host
//! share one `TooltipController`, so the delay, the grace time between
//! targets, and the hiding on a press or a key follow the controls.

use super::host::{Cx, Inner};
use super::modifier::{Modifier, Trigger};
use super::state::Live;
use super::text::TextContent;
use choscorui_controls::{
    ControlContext, ControlEvent, CustomControl, MeasureContext, PaintContext, PopupPlacement,
    TooltipController, VisibleTooltip,
};
use choscorui_core::{
    Dirty, EventPhase, EventReply, Key, KeyEvent, Measured, NodeId, PointerEvent, Rect,
    SemanticNode, SemanticRole, SemanticState,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// The host time in seconds as the controller's time.
fn duration(time: f64) -> Duration {
    Duration::from_secs_f64(time.max(0.0))
}

/// Whether the shown or waiting tooltip of `controller` belongs to `node`.
fn owns(controller: &TooltipController, node: NodeId) -> bool {
    controller.visible().is_some_and(|tip| tip.target == node)
        || controller
            .pending()
            .is_some_and(|(target, _)| target == node)
}

/// Ends the hover of `node`: hides its tooltip, and forgets a hover
/// that a press or a key ended. A tooltip of another view stays.
fn release(controller: &mut TooltipController, node: NodeId, now: Duration) {
    if controller.target() == Some(node) {
        controller.leave(now);
    }
}

/// The control that wraps a view with a tooltip. It paints only the
/// bubble, above every control, and takes no focus.
struct TooltipTrigger {
    text: Rc<RefCell<String>>,
    controller: Rc<RefCell<TooltipController>>,
    /// The wrapper node, known once the wrap is done.
    node: Rc<Cell<Option<NodeId>>>,
    /// Where the bubble goes, from the last arrange.
    bubble: Option<PopupPlacement>,
}

impl TooltipTrigger {
    fn shows(&self) -> bool {
        let controller = self.controller.borrow();
        self.node
            .get()
            .is_some_and(|node| controller.visible().is_some_and(|tip| tip.target == node))
    }
}

impl CustomControl for TooltipTrigger {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.min)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let text = self.text.borrow();
        self.bubble = match self.node.get() {
            Some(target) if !text.is_empty() => {
                let tip = VisibleTooltip {
                    target,
                    text: text.clone(),
                    anchor: bounds,
                };
                Some(tip.place(cx.viewport, cx.styles, cx.tokens, cx.text, cx.direction))
            }
            _ => None,
        };
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let node = cx.node();
        let now = duration(cx.time());
        let mut controller = self.controller.borrow_mut();
        let reply = match event {
            ControlEvent::Hover(true) => {
                let text = self.text.borrow();
                controller.hover(node, &text, cx.bounds(), now);
                EventReply::Continue
            }
            ControlEvent::Hover(false) | ControlEvent::Dismiss => {
                release(&mut controller, node, now);
                EventReply::Continue
            }
            ControlEvent::Timer => {
                controller.poll(now);
                EventReply::Stop
            }
            // Input inside the view hides the tooltip; Escape hides it
            // and goes no further.
            ControlEvent::Key(KeyEvent::Down { key, .. })
                if cx.phase() == EventPhase::Capture && owns(&controller, node) =>
            {
                let shown = controller.visible().is_some();
                controller.interrupt(now);
                if *key == Key::Escape && shown {
                    EventReply::Stop
                } else {
                    EventReply::Continue
                }
            }
            ControlEvent::Pointer(PointerEvent::Down { .. } | PointerEvent::Wheel { .. })
                if cx.phase() != EventPhase::Bubble && owns(&controller, node) =>
            {
                controller.interrupt(now);
                EventReply::Continue
            }
            _ => return EventReply::Continue,
        };
        cx.request_paint();
        reply
    }

    fn paint(&self, _cx: &mut PaintContext<'_>) {}

    fn overlay(&self) -> Option<Rect> {
        self.bubble
            .filter(|_| self.shows())
            .map(|bubble| bubble.rect)
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        let controller = self.controller.borrow();
        let (Some(bubble), Some(tip)) = (self.bubble, controller.visible()) else {
            return;
        };
        if self.node.get() == Some(tip.target) {
            tip.paint(&mut cx.at(bubble.rect), bubble.placement);
        }
    }

    fn deadline(&self) -> Option<f64> {
        let node = self.node.get()?;
        let (target, due) = self.controller.borrow().pending()?;
        (target == node).then_some(due.as_secs_f64())
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

/// The modifier that adds `text` as the tooltip of a view.
pub(super) fn tooltip(text: Live<TextContent>) -> Modifier {
    let shared: Rc<RefCell<String>> = Rc::default();
    let node: Rc<Cell<Option<NodeId>>> = Rc::default();
    let (control_text, control_node) = (shared.clone(), node.clone());
    Modifier::Trigger(Trigger {
        control: Box::new(move |inner: &mut Inner| {
            Box::new(TooltipTrigger {
                text: control_text,
                controller: inner.tooltips.clone(),
                node: control_node,
                bubble: None,
            })
        }),
        mount: Box::new(move |cx: &mut Cx<'_>, wrapper, view| {
            node.set(Some(wrapper));
            cx.inner.data(wrapper).tip = Some(shared.clone());
            let env = cx.env.clone();
            cx.react(move |inner| {
                let text = text.get().resolve(&env);
                if *shared.borrow() == text {
                    return;
                }
                shared.borrow_mut().clone_from(&text);
                inner.data(view).access.tooltip = Some(text);
                inner.touch(view, Dirty::SEMANTICS);
                // The bubble's size follows the text at the next arrange.
                let _ = inner.controls.get_mut(wrapper);
                inner.needs_layout = true;
            });
        }),
    })
}

/// The nearest tooltip wrapper at or above `node`.
fn trigger_of(inner: &Inner, node: NodeId) -> Option<NodeId> {
    let mut current = Some(node);
    while let Some(id) = current {
        if inner.nodes.get(&id).is_some_and(|data| data.tip.is_some()) {
            return Some(id);
        }
        current = inner.tree.parent(id);
    }
    None
}

/// Starts the tooltip of the view that keyboard focus moved into, and
/// hides the one of the view that it left.
pub(super) fn follow_focus(inner: &mut Inner, old: Option<NodeId>, new: Option<NodeId>) {
    let before = old.and_then(|node| trigger_of(inner, node));
    let after = new.and_then(|node| trigger_of(inner, node));
    if before == after {
        return;
    }
    let now = duration(inner.time);
    let controllers = inner.tooltips.clone();
    let mut controller = controllers.borrow_mut();
    if let Some(left) = before {
        release(&mut controller, left, now);
        inner.touch(left, Dirty::PAINT);
    }
    let (Some(entered), Some(focus)) = (after, new) else {
        return;
    };
    if !inner.tree.focus_visible(focus) {
        return;
    }
    let text = inner
        .nodes
        .get(&entered)
        .and_then(|data| data.tip.as_ref())
        .map(|text| text.borrow().clone())
        .unwrap_or_default();
    let anchor = inner.tree.bounds(entered).unwrap_or_default();
    controller.hover(entered, &text, anchor, now);
    inner.touch(entered, Dirty::PAINT);
}
