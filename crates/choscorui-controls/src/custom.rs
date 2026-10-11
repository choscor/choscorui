//! The control protocol. A `CustomControl` supplies measure, arrange,
//! event, paint, overlay, and semantics; `ControlHost` mounts it on a
//! retained node and connects it to layout, input routing, hover, focus
//! scopes, painting, accessibility, invalidation, and the message outbox.
//! Built-in controls and application controls use the same protocol. No
//! platform or FFI code is involved.

use crate::ShortcutModifiers;
use crate::style::{PaintContext, ResolvedStyle, StyleQuery, Styles, TextMeasure, VisualState};
use choscorui_core::{
    CivilDate, Constraints, DesignTokens, Dirty, DisplayList, EngineError, EventPhase, EventReply,
    KeyEvent, LayoutDirection, LayoutEngine, LeafMeasure, Measured, NodeId, Point, PointerEvent,
    Rect, SemanticAction, SemanticChange, SemanticError, SemanticNode, SemanticTree, ThemeResolver,
    Tree, TreeError,
};
use std::collections::{HashMap, HashSet};

/// Input delivered to a control.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlEvent {
    Pointer(PointerEvent),
    Key(KeyEvent),
    /// Committed text (typing or IME) while focused.
    Text(String),
    FocusChanged(bool),
    /// The pointer entered (`true`) or left (`false`) the node, one of its
    /// descendants, or its overlay.
    Hover(bool),
    /// A press landed outside the control and its open overlay, or a modal
    /// overlay opened above it: close transient popups.
    Dismiss,
}

/// A value a control reports in [`ControlMessage::Changed`].
#[derive(Clone, Debug, PartialEq)]
pub enum ControlValue {
    Bool(bool),
    /// A checkbox state that can be mixed.
    Mixed(Option<bool>),
    Number(f64),
    Range(f64, f64),
    Text(String),
    Index(Option<usize>),
    Indices(Vec<usize>),
    Date(Option<CivilDate>),
    Dates(Vec<CivilDate>),
}

/// What a control tells the application. Read them with
/// [`ControlHost::take_messages`] after dispatching input.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlMessage {
    /// A button-like control ran its action.
    Activated,
    /// The item at this index ran its action, for example a menu item.
    ItemActivated(usize),
    /// The value changed through the user.
    Changed(ControlValue),
    /// A popup, dialog, or disclosure opened or closed.
    OpenChanged(bool),
    /// The user dismissed an overlay, for example with Escape.
    Dismissed,
}

/// The theme, styles, text measurement, direction, and time that the host
/// passes to measure, arrange, and paint.
#[derive(Clone, Copy)]
pub struct HostEnv<'a> {
    pub themes: &'a ThemeResolver,
    pub styles: &'a Styles,
    pub text: &'a dyn TextMeasure,
    pub direction: LayoutDirection,
    /// Monotonic time in seconds.
    pub time: f64,
}

impl<'a> HostEnv<'a> {
    pub fn new(themes: &'a ThemeResolver, styles: &'a Styles, text: &'a dyn TextMeasure) -> Self {
        Self {
            themes,
            styles,
            text,
            direction: LayoutDirection::Ltr,
            time: 0.0,
        }
    }

    #[must_use]
    pub fn direction(mut self, direction: LayoutDirection) -> Self {
        self.direction = direction;
        self
    }

    #[must_use]
    pub fn time(mut self, time: f64) -> Self {
        self.time = time;
        self
    }

    fn tokens(&self, tree: &Tree, node: NodeId) -> DesignTokens {
        self.themes
            .resolve(tree, node)
            .unwrap_or_else(|_| self.themes.application())
    }
}

/// What a control reads while it measures or arranges.
pub struct MeasureContext<'a> {
    pub constraints: Constraints,
    pub direction: LayoutDirection,
    pub tokens: &'a DesignTokens,
    pub styles: &'a Styles,
    pub text: &'a dyn TextMeasure,
}

impl MeasureContext<'_> {
    pub fn rtl(&self) -> bool {
        self.direction == LayoutDirection::Rtl
    }

    /// The resolved style of `query`, from the cache.
    pub fn style(&self, query: StyleQuery) -> ResolvedStyle {
        self.styles.resolve(self.tokens, query)
    }
}

/// What a control may do while handling an event.
pub struct ControlContext<'a> {
    node: NodeId,
    tree: &'a mut Tree,
    phase: EventPhase,
    time: f64,
    modifiers: ShortcutModifiers,
    messages: &'a mut Vec<(NodeId, ControlMessage)>,
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
    /// The host time in seconds, for delays such as a tooltip's.
    pub fn time(&self) -> f64 {
        self.time
    }
    /// The modifier keys held during the event.
    pub fn modifiers(&self) -> ShortcutModifiers {
        self.modifiers
    }
    /// The tree, for built-in controls that wrap a tree-based model.
    pub(crate) fn tree_mut(&mut self) -> &mut Tree {
        self.tree
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
    /// Reports `message` to the application.
    pub fn emit(&mut self, message: ControlMessage) {
        self.messages.push((self.node, message));
    }
}

/// A control mounted in a [`ControlHost`].
pub trait CustomControl {
    /// Preferred size within `cx.constraints` (used by the layout engine).
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured;

    /// Records the geometry of the control's parts for `bounds`, so event
    /// handling hits the same parts that paint draws. Called by
    /// [`ControlHost::arrange`] after layout and theme changes.
    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let _ = (cx, bounds);
    }

    /// Handles routed input. Return `Stop` to consume it.
    fn event(&mut self, context: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply;

    /// Paints into `cx.bounds`. The host clips the control to its bounds
    /// and sets `cx.state` to the hover, focus, and disabled flags.
    fn paint(&self, cx: &mut PaintContext<'_>);

    /// The rectangle of an open popup in window coordinates. The host
    /// routes pointer events inside it to this control first.
    fn overlay(&self) -> Option<Rect> {
        None
    }

    /// Paints the open popup above every control, without a clip. The
    /// host also calls it while the control animates, so a popup can
    /// animate its close.
    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        let _ = cx;
    }

    /// True while a modal overlay is open: focus and pointer input stay in
    /// the control's subtree.
    fn modal(&self) -> bool {
        false
    }

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

    /// Enables or disables the control. A disabled control gets no input
    /// and paints with the disabled state.
    fn set_enabled(&mut self, enabled: bool) {
        let _ = enabled;
    }

    /// True while the control animates at `time` and needs another frame.
    fn animating(&self, time: f64) -> bool {
        let _ = time;
        false
    }
}

/// Owns mounted controls by node.
#[derive(Default)]
pub struct ControlHost {
    controls: HashMap<NodeId, Box<dyn CustomControl>>,
    disabled: HashSet<NodeId>,
    hovered: Vec<NodeId>,
    messages: Vec<(NodeId, ControlMessage)>,
    time: f64,
    modifiers: ShortcutModifiers,
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
        self.disabled.remove(&node);
        self.hovered.retain(|id| *id != node);
        let _ = tree.detach(node);
        Some(control)
    }

    /// Drops controls whose nodes were detached elsewhere.
    pub fn prune(&mut self, tree: &Tree) -> usize {
        let before = self.controls.len();
        self.controls.retain(|node, _| tree.contains(*node));
        self.disabled.retain(|node| tree.contains(*node));
        self.hovered.retain(|node| tree.contains(*node));
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

    /// Sets the time that controls read during events, in seconds.
    pub fn set_time(&mut self, time: f64) {
        self.time = time;
    }

    /// Sets the modifier keys that controls see with later events.
    pub fn set_modifiers(&mut self, modifiers: ShortcutModifiers) {
        self.modifiers = modifiers;
    }

    /// Enables or disables a mounted control. A disabled control gets no
    /// pointer, key, or text input, and it leaves keyboard traversal.
    pub fn set_enabled(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        enabled: bool,
    ) -> Result<(), TreeError> {
        let control = self.controls.get_mut(&node).ok_or(TreeError::MissingNode)?;
        control.set_enabled(enabled);
        let focusable = enabled && control.focusable();
        if enabled {
            self.disabled.remove(&node);
        } else {
            self.disabled.insert(node);
            if tree.pointer_capture() == Some(node) {
                tree.set_pointer_capture(None)?;
            }
        }
        tree.set_focusable(node, focusable)?;
        tree.invalidate(node, Dirty::PAINT)?;
        tree.invalidate(node, Dirty::SEMANTICS)
    }

    pub fn is_enabled(&self, node: NodeId) -> bool {
        !self.disabled.contains(&node)
    }

    /// Measures a mounted node outside a layout pass.
    pub fn measure(
        &mut self,
        tree: &Tree,
        node: NodeId,
        constraints: Constraints,
        env: &HostEnv<'_>,
    ) -> Option<Measured> {
        let tokens = env.tokens(tree, node);
        self.measure_with(node, constraints, env.direction, &tokens, env)
    }

    fn measure_with(
        &mut self,
        node: NodeId,
        constraints: Constraints,
        direction: LayoutDirection,
        tokens: &DesignTokens,
        env: &HostEnv<'_>,
    ) -> Option<Measured> {
        let control = self.controls.get_mut(&node)?;
        let cx = MeasureContext {
            constraints,
            direction,
            tokens,
            styles: env.styles,
            text: env.text,
        };
        let measured = control.measure(&cx);
        Some(Measured {
            size: constraints.constrain(measured.size),
            baseline: measured.baseline,
        })
    }

    /// Lays out `root` within `bounds` with `engine`, then arranges the
    /// controls. Mounted controls measure themselves; `leaves` measures
    /// every other leaf node.
    pub fn layout(
        &mut self,
        engine: &mut LayoutEngine,
        tree: &mut Tree,
        root: NodeId,
        bounds: Rect,
        env: &HostEnv<'_>,
        leaves: &mut dyn LeafMeasure,
    ) -> Result<(), EngineError> {
        // The engine borrows the tree mutably, so resolve tokens first.
        let tokens: HashMap<NodeId, DesignTokens> = self
            .controls
            .keys()
            .filter(|node| tree.is_within(**node, root) && tree.visible(**node) == Some(true))
            .map(|node| (*node, env.tokens(tree, *node)))
            .collect();
        let mut measure = |node: NodeId, constraints: Constraints, direction| match tokens
            .get(&node)
            .and_then(|tokens| self.measure_with(node, constraints, direction, tokens, env))
        {
            Some(measured) => measured,
            None => leaves.measure(node, constraints, direction),
        };
        engine.layout(tree, root, bounds, &mut measure)?;
        self.arrange(tree, env);
        Ok(())
    }

    /// Lets every visible control record its part geometry for its
    /// current bounds. Call after layout and after theme changes.
    pub fn arrange(&mut self, tree: &Tree, env: &HostEnv<'_>) {
        for (node, control) in &mut self.controls {
            let Some(bounds) = tree.bounds(*node) else {
                continue;
            };
            if tree.visible(*node) != Some(true) {
                continue;
            }
            let tokens = env.tokens(tree, *node);
            let cx = MeasureContext {
                constraints: Constraints::tight(bounds.size),
                direction: env.direction,
                tokens: &tokens,
                styles: env.styles,
                text: env.text,
            };
            control.arrange(&cx, bounds);
        }
    }

    fn deliver(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        phase: EventPhase,
        event: &ControlEvent,
    ) -> EventReply {
        if self.disabled.contains(&node)
            && !matches!(
                event,
                ControlEvent::FocusChanged(_) | ControlEvent::Hover(_) | ControlEvent::Dismiss
            )
        {
            return EventReply::Continue;
        }
        match self.controls.get_mut(&node) {
            Some(control) => {
                let mut context = ControlContext {
                    node,
                    tree,
                    phase,
                    time: self.time,
                    modifiers: self.modifiers,
                    messages: &mut self.messages,
                };
                control.event(&mut context, event)
            }
            None => EventReply::Continue,
        }
    }

    /// The topmost open modal control, if any.
    pub fn modal(&self, tree: &Tree) -> Option<NodeId> {
        self.paint_order(tree)
            .into_iter()
            .rev()
            .find(|node| self.controls.get(node).is_some_and(|c| c.modal()))
    }

    /// The control whose open overlay contains `point`, topmost first.
    fn overlay_at(&self, tree: &Tree, point: Point) -> Option<NodeId> {
        self.paint_order(tree).into_iter().rev().find(|node| {
            self.controls
                .get(node)
                .and_then(|control| control.overlay())
                .is_some_and(|rect| rect.contains(point))
        })
    }

    /// Where a pointer event at `point` goes: an open overlay, then the
    /// hit node, kept inside an open modal.
    fn route(&self, tree: &Tree, point: Point) -> Option<NodeId> {
        if let Some(capture) = tree.pointer_capture() {
            return Some(capture);
        }
        let target = self
            .overlay_at(tree, point)
            .or_else(|| tree.hit_test(point));
        match (self.modal(tree), target) {
            (Some(modal), Some(target)) if !tree.is_within(target, modal) => Some(modal),
            (Some(modal), None) => Some(modal),
            (_, target) => target,
        }
    }

    fn update_hover(&mut self, tree: &mut Tree, target: Option<NodeId>) {
        let mut path = Vec::new();
        let mut current = target;
        while let Some(node) = current {
            if self.controls.contains_key(&node) {
                path.push(node);
            }
            current = tree.parent(node);
        }
        let left: Vec<NodeId> = self
            .hovered
            .iter()
            .copied()
            .filter(|node| !path.contains(node))
            .collect();
        let entered: Vec<NodeId> = path
            .iter()
            .copied()
            .filter(|node| !self.hovered.contains(node))
            .collect();
        self.hovered = path;
        for node in left {
            self.deliver(tree, node, EventPhase::Target, &ControlEvent::Hover(false));
            let _ = tree.invalidate(node, Dirty::PAINT);
        }
        for node in entered.into_iter().rev() {
            self.deliver(tree, node, EventPhase::Target, &ControlEvent::Hover(true));
            let _ = tree.invalidate(node, Dirty::PAINT);
        }
    }

    /// True while the pointer is over the node, a descendant, or its
    /// overlay.
    pub fn is_hovered(&self, node: NodeId) -> bool {
        self.hovered.contains(&node)
    }

    /// Routes a pointer event: an open overlay first, then capture, target,
    /// and bubble through the tree. A press outside an open overlay sends
    /// `Dismiss` to its control. Pressing a focusable control focuses it.
    pub fn dispatch_pointer(&mut self, tree: &mut Tree, event: PointerEvent) {
        let target = self.route(tree, event.position());
        if matches!(event, PointerEvent::Move { .. } | PointerEvent::Down { .. }) {
            self.update_hover(tree, target);
        }
        if let PointerEvent::Down { .. } = event {
            tree.set_input_modality(choscorui_core::InputModality::Pointer);
            let open: Vec<NodeId> = self
                .paint_order(tree)
                .into_iter()
                .filter(|node| {
                    self.controls
                        .get(node)
                        .is_some_and(|control| control.overlay().is_some() && !control.modal())
                })
                .filter(|node| target.is_none_or(|target| !tree.is_within(target, *node)))
                .collect();
            for node in open {
                self.deliver(tree, node, EventPhase::Target, &ControlEvent::Dismiss);
            }
            if let Some(target) = target
                && tree.contains(target)
                && self.controls.get(&target).is_some_and(|c| c.focusable())
                && !self.disabled.contains(&target)
            {
                self.move_focus(tree, Some(target));
            }
        }
        let Some(target) = target.filter(|target| tree.contains(*target)) else {
            return;
        };
        let overlay = self
            .controls
            .get(&target)
            .and_then(|control| control.overlay())
            .is_some_and(|rect| rect.contains(event.position()));
        if overlay || tree.pointer_capture() == Some(target) {
            self.deliver(
                tree,
                target,
                EventPhase::Target,
                &ControlEvent::Pointer(event),
            );
            return;
        }
        tree.dispatch_pointer_to(target, event, |tree, node, phase, event| {
            self.deliver(tree, node, phase, &ControlEvent::Pointer(event))
        });
    }

    /// Routes a key event to the focused control, through its ancestors.
    /// Focus outside an open modal moves into it first.
    /// Returns true when a control stopped the event.
    pub fn dispatch_key(&mut self, tree: &mut Tree, event: KeyEvent) -> bool {
        if let Some(modal) = self.modal(tree)
            && !tree
                .focus()
                .is_some_and(|focus| tree.is_within(focus, modal))
        {
            let next = tree.focus_next_within(modal, false);
            if next.is_none() {
                self.move_focus(tree, Some(modal));
            }
        }
        let mut stopped = false;
        tree.dispatch_key(event, |tree, node, phase, event| {
            let reply = self.deliver(tree, node, phase, &ControlEvent::Key(event));
            stopped |= reply == EventReply::Stop;
            reply
        });
        stopped
    }

    /// Moves keyboard focus forward or back, inside an open modal.
    pub fn focus_next(&mut self, tree: &mut Tree, reverse: bool) -> Option<NodeId> {
        let previous = tree.focus();
        let scope = self.modal(tree).unwrap_or(tree.root());
        let next = tree.focus_next_within(scope, reverse);
        if previous != next {
            self.notify_focus(tree, previous, next);
        }
        next
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
        self.notify_focus(tree, previous, next);
    }

    fn notify_focus(&mut self, tree: &mut Tree, previous: Option<NodeId>, next: Option<NodeId>) {
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

    /// Visible mounted nodes in tree (paint) order.
    fn paint_order(&self, tree: &Tree) -> Vec<NodeId> {
        let mut order = Vec::new();
        let mut stack = vec![tree.root()];
        while let Some(node) = stack.pop() {
            if tree.visible(node) == Some(false) {
                continue;
            }
            if self.controls.contains_key(&node) {
                order.push(node);
            }
            if let Some(children) = tree.children(node) {
                stack.extend(children.iter().rev().copied());
            }
        }
        order
    }

    /// The host's view of a node's state: hover, focus, and disabled.
    pub fn visual_state(&self, tree: &Tree, node: NodeId) -> VisualState {
        VisualState::NONE
            .set(VisualState::HOVERED, self.hovered.first() == Some(&node))
            .set(VisualState::FOCUSED, tree.focus() == Some(node))
            .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(node))
            .set(VisualState::DISABLED, self.disabled.contains(&node))
    }

    /// Paints visible mounted controls in tree order, each clipped to its
    /// bounds, then every open overlay above them.
    pub fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &HostEnv<'_>) {
        let rtl = env.direction == LayoutDirection::Rtl;
        let order = self.paint_order(tree);
        for node in &order {
            let (Some(control), Some(bounds)) = (self.controls.get(node), tree.bounds(*node))
            else {
                continue;
            };
            let tokens = env.tokens(tree, *node);
            list.push_clip(bounds);
            let mut cx = PaintContext::new(list, bounds, &tokens, env.styles, env.text)
                .rtl(rtl)
                .state(self.visual_state(tree, *node))
                .time(env.time);
            control.paint(&mut cx);
            let _ = list.pop_clip();
        }
        for node in &order {
            let (Some(control), Some(bounds)) = (self.controls.get(node), tree.bounds(*node))
            else {
                continue;
            };
            // A closing popup still animates after its overlay is gone.
            if control.overlay().is_none() && !control.animating(env.time) {
                continue;
            }
            let tokens = env.tokens(tree, *node);
            let mut cx = PaintContext::new(list, bounds, &tokens, env.styles, env.text)
                .rtl(rtl)
                .state(self.visual_state(tree, *node))
                .time(env.time);
            control.paint_overlay(&mut cx);
        }
    }

    /// True while any visible control animates at `time`.
    pub fn animating(&self, tree: &Tree, time: f64) -> bool {
        self.controls
            .iter()
            .any(|(node, control)| tree.visible(*node) == Some(true) && control.animating(time))
    }

    /// The messages controls emitted since the last call, in order.
    pub fn take_messages(&mut self) -> Vec<(NodeId, ControlMessage)> {
        std::mem::take(&mut self.messages)
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
                let mut semantic = control.semantics();
                if self.disabled.contains(node) {
                    semantic.state.disabled = true;
                    semantic.actions.clear();
                }
                let change = semantics.set(*node, semantic, tree)?;
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
        if self.disabled.contains(&node) {
            return false;
        }
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
            time: self.time,
            modifiers: self.modifiers,
            messages: &mut self.messages,
        };
        control.perform(&mut context, action, value)
    }
}
