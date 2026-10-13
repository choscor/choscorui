//! Tooltips. The `tooltip` modifier wraps a view in a control that
//! watches hover and keyboard focus inside it. All tooltips of a host
//! share one `TooltipController`, so the delay, the grace time between
//! targets, and the hiding on a press or a key follow the controls.
//! The bubble fades and zooms in and out. While it shows, a node of the
//! wrapper publishes the `Tooltip` semantic node.

use super::host::{Cx, Inner};
use super::modifier::{Modifier, Trigger};
use super::state::Live;
use super::text::TextContent;
use choscorui_controls::{
    ControlContext, ControlEvent, ControlKind, CustomControl, MeasureContext, PaintContext,
    Placement, PopupPlacement, Reveal, Slot, StyleQuery, TooltipController, VisibleTooltip,
};
use choscorui_core::{
    Dirty, EventPhase, EventReply, ItemAlign, Key, KeyEvent, LayoutStyle, Measured, NodeId,
    PointerEvent, Rect, SemanticNode, SemanticRelation, SemanticRole, SemanticState,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// The tooltips of a host: the shared controller, and when the shown
/// tooltip showed and the last one hid, for the show and hide motion.
#[derive(Default)]
pub(crate) struct Tooltips {
    pub controller: TooltipController,
    shown: Option<(VisibleTooltip, f64)>,
    hidden: Option<(VisibleTooltip, f64)>,
}

impl Tooltips {
    /// Records at host `time` a change of the tooltip that shows.
    fn sync(&mut self, time: f64) {
        let now = self.controller.visible();
        let before = self.shown.as_ref().map(|(tip, _)| tip.target);
        if now.map(|tip| tip.target) == before {
            return;
        }
        let now = now.cloned();
        if let Some((tip, _)) = self.shown.take() {
            self.hidden = Some((tip, time));
        }
        self.shown = now.map(|tip| (tip, time));
    }

    /// The show or hide motion of the tooltip of `node`, and the tooltip.
    fn reveal(&self, node: NodeId) -> Option<(Reveal, &VisibleTooltip)> {
        of(&self.shown, node, true).or_else(|| of(&self.hidden, node, false))
    }

    /// The `Tooltip` node of the tooltip of `wrapper` while it shows,
    /// with a `DescribedBy` relation to `view`.
    pub(crate) fn semantics(&self, wrapper: NodeId, view: NodeId) -> Option<SemanticNode> {
        let shows = self
            .controller
            .visible()
            .is_some_and(|tip| tip.target == wrapper);
        let mut node = self.controller.semantics().filter(|_| shows)?;
        node.relations = vec![SemanticRelation::DescribedBy(view)];
        Some(node)
    }

    /// The wrapper whose tooltip shows.
    pub(crate) fn showing(&self) -> Option<NodeId> {
        self.controller.visible().map(|tip| tip.target)
    }
}

/// The motion of `entry` as open or closed, if it is the tooltip of
/// `node`.
fn of(
    entry: &Option<(VisibleTooltip, f64)>,
    node: NodeId,
    open: bool,
) -> Option<(Reveal, &VisibleTooltip)> {
    let (tip, changed_at) = entry.as_ref().filter(|(tip, _)| tip.target == node)?;
    let reveal = Reveal {
        open,
        changed_at: *changed_at,
    };
    Some((reveal, tip))
}

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
    tooltips: Rc<RefCell<Tooltips>>,
    /// The wrapper node, known once the wrap is done.
    node: Rc<Cell<Option<NodeId>>>,
    /// Where the bubble goes, from the last arrange.
    bubble: Option<PopupPlacement>,
    /// The duration of the show and hide motion, from the last arrange.
    motion_ms: u32,
}

impl TooltipTrigger {
    fn shows(&self) -> bool {
        let tooltips = self.tooltips.borrow();
        self.node
            .get()
            .is_some_and(|node| tooltips.showing() == Some(node))
    }
}

impl CustomControl for TooltipTrigger {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.min)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.motion_ms = cx
            .style(StyleQuery::new(ControlKind::Tooltip, Slot::Root))
            .motion_ms;
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
        let time = cx.time();
        let now = duration(time);
        let mut tooltips = self.tooltips.borrow_mut();
        let controller = &mut tooltips.controller;
        let reply = match event {
            ControlEvent::Hover(true) => {
                let text = self.text.borrow();
                controller.hover(node, &text, cx.bounds(), now);
                EventReply::Continue
            }
            ControlEvent::Hover(false) | ControlEvent::Dismiss => {
                release(controller, node, now);
                EventReply::Continue
            }
            ControlEvent::Timer => {
                controller.poll(now);
                EventReply::Stop
            }
            // Input inside the view hides the tooltip; Escape hides it
            // and goes no further.
            ControlEvent::Key(KeyEvent::Down { key, .. })
                if cx.phase() == EventPhase::Capture && owns(controller, node) =>
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
                if cx.phase() != EventPhase::Bubble && owns(controller, node) =>
            {
                controller.interrupt(now);
                EventReply::Continue
            }
            _ => return EventReply::Continue,
        };
        tooltips.sync(time);
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
        let tooltips = self.tooltips.borrow();
        let (Some(bubble), Some(node)) = (self.bubble, self.node.get()) else {
            return;
        };
        let Some((reveal, tip)) = tooltips.reveal(node) else {
            return;
        };
        let progress = reveal.progress(cx.tokens, self.motion_ms, cx.time);
        if progress <= 0.0 {
            return;
        }
        // The bubble slides in from its anchor.
        let distance = cx.tokens.spacing * 2.0;
        let slide = if bubble.placement == Placement::Below {
            (0.0, -distance)
        } else {
            (0.0, distance)
        };
        Reveal::paint(cx, bubble.rect, progress, slide, |cx| {
            tip.paint(&mut cx.at(bubble.rect), bubble.placement);
        });
    }

    fn animating(&self, time: f64) -> bool {
        let tooltips = self.tooltips.borrow();
        self.node
            .get()
            .and_then(|node| tooltips.reveal(node))
            .is_some_and(|(reveal, _)| reveal.animating(self.motion_ms, time))
    }

    fn deadline(&self) -> Option<f64> {
        let node = self.node.get()?;
        let (target, due) = self.tooltips.borrow().controller.pending()?;
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
                tooltips: inner.tooltips.clone(),
                node: control_node,
                bubble: None,
                motion_ms: 0,
            })
        }),
        mount: Box::new(move |cx: &mut Cx, wrapper, view| {
            node.set(Some(wrapper));
            // The node that publishes the tooltip covers the view, but
            // presses go through it.
            let env = cx.env.clone();
            let bubble = cx.within(wrapper, env).node(LayoutStyle {
                align_self: Some(ItemAlign::Stretch),
                justify_self: Some(ItemAlign::Stretch),
                ..LayoutStyle::default()
            });
            if let Err(error) = cx.inner.tree.set_hit_testable(bubble, false) {
                cx.inner.report(super::host::ViewError::Tree(error));
            }
            cx.inner.data(bubble).describes = Some((wrapper, view));
            let data = cx.inner.data(wrapper);
            data.tip = Some(shared.clone());
            data.bubble = Some(bubble);
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
    let shared = inner.tooltips.clone();
    let mut tooltips = shared.borrow_mut();
    if let Some(left) = before {
        release(&mut tooltips.controller, left, now);
        inner.touch(left, Dirty::PAINT);
    }
    if let (Some(entered), Some(focus)) = (after, new)
        && inner.tree.focus_visible(focus)
    {
        let text = inner
            .nodes
            .get(&entered)
            .and_then(|data| data.tip.as_ref())
            .map(|text| text.borrow().clone())
            .unwrap_or_default();
        let anchor = inner.tree.bounds(entered).unwrap_or_default();
        tooltips.controller.hover(entered, &text, anchor, now);
        inner.touch(entered, Dirty::PAINT);
    }
    tooltips.sync(inner.time);
}
