//! Extension point for application-defined controls. A `CustomControl`
//! supplies measure, event, paint, and semantics; `ControlHost` mounts it
//! on a retained node and connects it to layout, input routing, painting,
//! accessibility, and invalidation. No platform or FFI code is involved.

use choscorui_core::{
    Constraints, DesignTokens, Dirty, DisplayList, EventPhase, EventReply, KeyEvent,
    LayoutDirection, Measured, NodeId, PointerEvent, Rect, SemanticAction, SemanticChange,
    SemanticError, SemanticNode, SemanticTree, Tree, TreeError,
};
use std::collections::HashMap;

/// Input delivered to a custom control.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlEvent {
    Pointer(PointerEvent),
    Key(KeyEvent),
    /// Committed text (typing or IME) while focused.
    Text(String),
    FocusChanged(bool),
}

/// What a control may do while handling an event.
pub struct ControlContext<'a> {
    node: NodeId,
    tree: &'a mut Tree,
    phase: EventPhase,
}

impl ControlContext<'_> {
    pub fn node(&self) -> NodeId {
        self.node
    }
    /// Capture, target, or bubble; controls usually act on `Target`.
    pub fn phase(&self) -> EventPhase {
        self.phase
    }
    pub fn bounds(&self) -> Rect {
        self.tree.bounds(self.node).unwrap_or_default()
    }
    pub fn is_focused(&self) -> bool {
        self.tree.focus() == Some(self.node)
    }
    pub fn request_paint(&mut self) {
        let _ = self.tree.invalidate(self.node, Dirty::PAINT);
    }
    pub fn request_layout(&mut self) {
        let _ = self.tree.invalidate(self.node, Dirty::LAYOUT);
    }
    pub fn request_semantics(&mut self) {
        let _ = self.tree.invalidate(self.node, Dirty::SEMANTICS);
    }
    pub fn request_focus(&mut self) {
        let _ = self.tree.set_focus(Some(self.node));
    }
    /// Routes later pointer events here until released (drags).
    pub fn capture_pointer(&mut self, capture: bool) {
        let _ = self.tree.set_pointer_capture(capture.then_some(self.node));
    }
}

/// An application-defined control.
pub trait CustomControl {
    /// Preferred size within `constraints` (used by the layout engine).
    fn measure(&mut self, constraints: Constraints, direction: LayoutDirection) -> Measured;

    /// Handles routed input. Return `Stop` to consume it.
    fn event(&mut self, context: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply;

    /// Paints into `bounds` with the resolved theme.
    fn paint(&self, list: &mut DisplayList, bounds: Rect, tokens: &DesignTokens, focused: bool);

    /// Accessible role, name, value, state, and supported actions.
    fn semantics(&self) -> SemanticNode;

    /// Performs an assistive action listed in `semantics().actions`.
    fn perform(
        &mut self,
        context: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        let _ = (context, action, value);
        false
    }

    fn focusable(&self) -> bool {
        true
    }
}

/// Owns mounted custom controls by node.
#[derive(Default)]
pub struct ControlHost {
    controls: HashMap<NodeId, Box<dyn CustomControl>>,
}

impl ControlHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a child node of `parent` for `control`.
    pub fn mount(
        &mut self,
        tree: &mut Tree,
        parent: NodeId,
        control: Box<dyn CustomControl>,
    ) -> Result<NodeId, TreeError> {
        let node = tree.attach(parent, Rect::default())?;
        tree.set_focusable(node, control.focusable())?;
        tree.invalidate(node, Dirty::LAYOUT)?;
        tree.invalidate(node, Dirty::SEMANTICS)?;
        self.controls.insert(node, control);
        Ok(node)
    }

    /// Detaches the node and drops its control.
    pub fn unmount(&mut self, tree: &mut Tree, node: NodeId) -> Option<Box<dyn CustomControl>> {
        let control = self.controls.remove(&node)?;
        let _ = tree.detach(node);
        Some(control)
    }

    /// Drops controls whose nodes were detached elsewhere.
    pub fn prune(&mut self, tree: &Tree) -> usize {
        let before = self.controls.len();
        self.controls.retain(|node, _| tree.contains(*node));
        before - self.controls.len()
    }

    pub fn get(&self, node: NodeId) -> Option<&dyn CustomControl> {
        self.controls.get(&node).map(|control| control.as_ref())
    }
    pub fn get_mut(&mut self, node: NodeId) -> Option<&mut (dyn CustomControl + 'static)> {
        self.controls.get_mut(&node).map(|control| control.as_mut())
    }
    pub fn contains(&self, node: NodeId) -> bool {
        self.controls.contains_key(&node)
    }

    /// Measures a mounted node; use from a `LeafMeasure` closure:
    /// `|node, c, d| host.measure(node, c, d).unwrap_or(...)`.
    pub fn measure(
        &mut self,
        node: NodeId,
        constraints: Constraints,
        direction: LayoutDirection,
    ) -> Option<Measured> {
        let control = self.controls.get_mut(&node)?;
        let measured = control.measure(constraints, direction);
        Some(Measured {
            size: constraints.constrain(measured.size),
            baseline: measured.baseline,
        })
    }

    fn deliver(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        phase: EventPhase,
        event: &ControlEvent,
    ) -> EventReply {
        match self.controls.get_mut(&node) {
            Some(control) => {
                let mut context = ControlContext { node, tree, phase };
                control.event(&mut context, event)
            }
            None => EventReply::Continue,
        }
    }

    /// Routes a pointer event through capture/target/bubble. Pressing a
    /// focusable control focuses it (reporting focus changes).
    pub fn dispatch_pointer(&mut self, tree: &mut Tree, event: PointerEvent) {
        if let PointerEvent::Down { .. } = event
            && let Some(target) = tree.hit_test(event.position())
            && self
                .controls
                .get(&target)
                .is_some_and(|control| control.focusable())
        {
            self.move_focus(tree, Some(target));
        }
        tree.dispatch_pointer(event, |tree, node, phase, event| {
            self.deliver(tree, node, phase, &ControlEvent::Pointer(event))
        });
    }

    pub fn dispatch_key(&mut self, tree: &mut Tree, event: KeyEvent) {
        tree.dispatch_key(event, |tree, node, phase, event| {
            self.deliver(tree, node, phase, &ControlEvent::Key(event))
        });
    }

    /// Delivers committed text to the focused control.
    pub fn dispatch_text(&mut self, tree: &mut Tree, text: &str) -> bool {
        let Some(focus) = tree.focus() else {
            return false;
        };
        self.deliver(
            tree,
            focus,
            EventPhase::Target,
            &ControlEvent::Text(text.to_owned()),
        ) == EventReply::Stop
    }

    /// Moves focus, notifying the old and new controls.
    pub fn move_focus(&mut self, tree: &mut Tree, next: Option<NodeId>) {
        let previous = tree.focus();
        if previous == next || tree.set_focus(next).is_err() {
            return;
        }
        if let Some(previous) = previous {
            self.deliver(
                tree,
                previous,
                EventPhase::Target,
                &ControlEvent::FocusChanged(false),
            );
            let _ = tree.invalidate(previous, Dirty::PAINT);
        }
        if let Some(next) = next {
            self.deliver(
                tree,
                next,
                EventPhase::Target,
                &ControlEvent::FocusChanged(true),
            );
            let _ = tree.invalidate(next, Dirty::PAINT);
        }
    }

    /// Paints visible mounted controls in tree order.
    pub fn paint(&self, tree: &Tree, list: &mut DisplayList, tokens: &DesignTokens) {
        let mut stack = vec![tree.root()];
        while let Some(node) = stack.pop() {
            if tree.visible(node) == Some(false) {
                continue;
            }
            if let (Some(control), Some(bounds)) = (self.controls.get(&node), tree.bounds(node)) {
                list.push_clip(bounds);
                control.paint(list, bounds, tokens, tree.focus() == Some(node));
                let _ = list.pop_clip();
            }
            if let Some(children) = tree.children(node) {
                stack.extend(children.iter().rev().copied());
            }
        }
    }

    /// Publishes semantics for controls marked dirty (or all, with
    /// `force`). Returns the resulting changes for platform notification.
    pub fn sync_semantics(
        &self,
        tree: &mut Tree,
        semantics: &mut SemanticTree,
        force: bool,
    ) -> Result<Vec<SemanticChange>, SemanticError> {
        let mut changes = Vec::new();
        for (node, control) in &self.controls {
            let dirty = tree.take_dirty(*node).unwrap_or(Dirty::NONE);
            if force || dirty.contains(Dirty::SEMANTICS) {
                let change = semantics.set(*node, control.semantics(), tree)?;
                if change != SemanticChange::None {
                    changes.push(change);
                }
            }
            // Keep paint/layout requests for the caller's frame loop.
            let rest = [Dirty::LAYOUT, Dirty::PAINT]
                .into_iter()
                .filter(|flag| dirty.contains(*flag));
            for flag in rest {
                let _ = tree.invalidate(*node, flag);
            }
        }
        Ok(changes)
    }

    /// Routes an assistive-technology action (VoiceOver press, increment,
    /// set value) to the control, if it advertises the action.
    pub fn perform(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        let Some(control) = self.controls.get_mut(&node) else {
            return false;
        };
        if !control.semantics().actions.contains(&action) {
            return false;
        }
        let mut context = ControlContext {
            node,
            tree,
            phase: EventPhase::Target,
        };
        control.perform(&mut context, action, value)
    }
}
