//! The control protocol. A `CustomControl` supplies measure, arrange,
//! event, paint, overlay, and semantics; `ControlHost` mounts it on a
//! retained node and connects it to layout, input routing, hover, focus
//! scopes, painting, accessibility, invalidation, and the message outbox.
//! Built-in controls and application controls use the same protocol. No
//! platform or FFI code is involved.

use crate::style::{
    ControlKind, Cursor, PaintContext, ResolvedStyle, Slot, StyleQuery, Styles, TextMeasure,
    TransitionStats, Transitions, VisualState,
};
use crate::{DialogId, DialogStack, EditCommand, Modality, Shortcut, ShortcutModifiers};
use choscorui_core::{
    AnnouncementPriority, CivilDate, CompositionEvent, Constraints, DesignTokens, Dirty,
    DisplayList, EngineError, EventPhase, EventReply, Feedback, GestureEvent, InputModality, Key,
    KeyEvent, LayoutDirection, LayoutEngine, LeafMeasure, Localizer, Measured, NoFeedback, NodeId,
    Point, PointerButton, PointerDetails, PointerEvent, Rect, SemanticAction, SemanticChange,
    SemanticError, SemanticNode, SemanticParts, SemanticTree, Services, TextInputInfo,
    ThemeResolver, Tree, TreeError,
};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// Input delivered to a control.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlEvent {
    Pointer(PointerEvent),
    Key(KeyEvent),
    /// Committed text (typing or IME) while focused.
    Text(String),
    /// A trackpad gesture over the control, such as a pinch. It goes to
    /// the control under the gesture, then bubbles to its ancestors.
    Gesture(GestureEvent),
    /// Input method composition while focused. A control that does not
    /// use it gets the committed text as [`ControlEvent::Text`].
    Compose(CompositionEvent),
    /// An Edit menu command, or its Command-key chord that no control
    /// used as a key, while focused.
    Edit(EditCommand),
    FocusChanged(bool),
    /// The pointer entered (`true`) or left (`false`) the node, one of its
    /// descendants, or its overlay.
    Hover(bool),
    /// A press landed outside the control and its open overlay, or the
    /// control becomes disabled: close transient popups and emit
    /// [`ControlMessage::Dismissed`]. Reply `Stop` when a popup closed, so
    /// that the host consumes the press.
    Dismiss,
    /// The host time reached the control's [`CustomControl::deadline`].
    Timer,
    /// The window became fully hidden (`false`) or visible again
    /// (`true`). Every control gets it, also a disabled one. A control
    /// pauses the timers that a person must see, such as a toast's time.
    WindowVisible(bool),
    /// The user pressed one of the control's
    /// [`CustomControl::window_shortcuts`], and the focused control did
    /// not use the key.
    Shortcut(Shortcut),
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
    /// The channels of a multi-channel value, each as a fraction of its
    /// range, such as the hue, saturation, value, and alpha of a
    /// `ColorPicker`.
    Channels(Vec<f64>),
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
    /// Action button `action` of row `item` ran, for example in an item
    /// group. Both are indices in the app's lists, so they stay the same
    /// when other rows gain or lose buttons.
    ItemAction { item: usize, action: usize },
    /// The value changed through the user.
    Changed(ControlValue),
    /// A popup, dialog, or disclosure opened or closed.
    OpenChanged(bool),
    /// The user dismissed an overlay, for example with Escape.
    Dismissed,
    /// The item with this index or id closed without its action, for
    /// example a toast that closed by its close button, Escape, or its
    /// timer.
    ItemDismissed(usize),
    /// A menu action ran its command.
    Command(crate::CommandId),
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
    /// True when a focus frame paints the ring of the focused node, so
    /// the node paints none.
    pub framed: bool,
}

impl<'a> HostEnv<'a> {
    pub fn new(themes: &'a ThemeResolver, styles: &'a Styles, text: &'a dyn TextMeasure) -> Self {
        Self {
            themes,
            styles,
            text,
            direction: LayoutDirection::Ltr,
            time: 0.0,
            framed: false,
        }
    }

    /// A focus frame paints the ring of the focused node: the node leaves
    /// it in [`ControlHost::framed_ring`].
    #[must_use]
    pub fn framed(mut self, framed: bool) -> Self {
        self.framed = framed;
        self
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
    /// The root node's bounds: the window area that overlays fit in.
    pub viewport: Rect,
    pub direction: LayoutDirection,
    pub tokens: &'a DesignTokens,
    pub styles: &'a Styles,
    pub text: &'a dyn TextMeasure,
}

impl MeasureContext<'_> {
    /// True when the layout direction is right-to-left.
    pub fn rtl(&self) -> bool {
        self.direction == LayoutDirection::Rtl
    }

    /// The resolved style of `query`, from the cache.
    pub fn style(&self, query: StyleQuery) -> ResolvedStyle {
        self.styles.resolve(self.tokens, query)
    }

    /// The area that a popup of the control fits in, from
    /// [`crate::popup_bounds`].
    pub fn popup_bounds(&self) -> Rect {
        crate::popup_bounds(self.viewport, self.tokens)
    }
}

/// What a control may do while handling an event.
pub struct ControlContext<'a> {
    node: NodeId,
    tree: &'a mut Tree,
    phase: EventPhase,
    time: f64,
    modifiers: ShortcutModifiers,
    pointer: PointerDetails,
    services: &'a Services,
    messages: &'a mut Vec<(NodeId, ControlMessage)>,
    announcements: &'a mut Vec<(String, AnnouncementPriority)>,
    feedback: &'a dyn Feedback,
    strings: &'a Localizer,
    /// The node that had the focus, while a `FocusChanged(true)` event
    /// is delivered.
    focus_from: Option<NodeId>,
}

impl ControlContext<'_> {
    /// The node of the control that gets the event.
    pub fn node(&self) -> NodeId {
        self.node
    }
    /// Capture, target, or bubble; controls usually act on `Target`.
    pub fn phase(&self) -> EventPhase {
        self.phase
    }
    /// The bounds of the control's node, or an empty rectangle.
    pub fn bounds(&self) -> Rect {
        self.tree.bounds(self.node).unwrap_or_default()
    }
    /// True when the control's node has the keyboard focus.
    pub fn is_focused(&self) -> bool {
        self.tree.focus() == Some(self.node)
    }
    /// The bounds of the focused node, for a control that acts for a
    /// focused descendant, such as a context menu that opens at it.
    pub fn focus_bounds(&self) -> Option<Rect> {
        self.tree.focus().and_then(|focus| self.tree.bounds(focus))
    }
    /// The host time in seconds, for delays such as a tooltip's.
    pub fn time(&self) -> f64 {
        self.time
    }
    /// The modifier keys held during the event.
    pub fn modifiers(&self) -> ShortcutModifiers {
        self.modifiers
    }
    /// How the user moves focus now: `Pointer` after a press, `Keyboard`
    /// after a key. A control that keeps an inner focus restores it only
    /// on keyboard focus.
    pub fn input_modality(&self) -> InputModality {
        self.tree.input_modality()
    }
    /// The click count of the pointer event: 2 for a double click and 3
    /// for a triple click, within the system double-click interval.
    pub fn click_count(&self) -> u32 {
        self.pointer.click_count
    }
    /// The desktop services of the backend, which check its
    /// capabilities.
    pub fn services(&self) -> &Services {
        self.services
    }
    /// The text on the clipboard, or `None` when the backend has no
    /// clipboard, the clipboard has no text, or the read fails.
    pub fn clipboard_text(&self) -> Option<String> {
        self.services.clipboard_text().ok().flatten()
    }
    /// Puts `text` on the clipboard. Returns false when the backend has no
    /// clipboard or the write fails.
    pub fn write_clipboard(&mut self, text: &str) -> bool {
        self.services.set_clipboard_text(text).is_ok()
    }
    /// The node that had the focus before this control got it, during a
    /// `FocusChanged(true)` event.
    pub(crate) fn focus_from(&self) -> Option<NodeId> {
        self.focus_from
    }
    /// The tree, for built-in controls that wrap a tree-based model.
    pub(crate) fn tree_mut(&mut self) -> &mut Tree {
        self.tree
    }
    /// Marks the control to paint again in the next frame.
    pub fn request_paint(&mut self) {
        let _ = self.tree.invalidate(self.node, Dirty::PAINT);
    }
    /// Marks the control to measure and lay out again.
    pub fn request_layout(&mut self) {
        let _ = self.tree.invalidate(self.node, Dirty::LAYOUT);
    }
    /// Marks the control to publish its semantics again.
    pub fn request_semantics(&mut self) {
        let _ = self.tree.invalidate(self.node, Dirty::SEMANTICS);
    }
    /// Moves the keyboard focus to the control.
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
    /// The messages of the window locale, for the built-in text of a
    /// message or a description.
    pub fn strings(&self) -> &Localizer {
        self.strings
    }
    /// Plays the system alert sound through the host's [`Feedback`], for
    /// example when the control refuses a key or a paste.
    pub fn beep(&self) {
        self.feedback.beep();
    }
    /// Asks the screen reader to speak `text`, for a live update that no
    /// focused node carries, such as a status message or a new toast.
    /// `High` interrupts the current speech. Empty text does nothing.
    pub fn announce(&mut self, text: impl Into<String>, priority: AnnouncementPriority) {
        let text = text.into();
        if !text.trim().is_empty() {
            self.announcements.push((text, priority));
        }
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
    /// and sets `cx.state` to the hover, focus, disabled, and invalid flags.
    fn paint(&self, cx: &mut PaintContext<'_>);

    /// The rectangle of an open popup in window coordinates. The host
    /// routes pointer events inside it to this control first.
    fn overlay(&self) -> Option<Rect> {
        None
    }

    /// True when the open overlay takes pointer input. A tooltip bubble
    /// returns false, so that the pointer reaches the control under it.
    /// The default is true.
    fn overlay_takes_input(&self) -> bool {
        true
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

    /// Accessible role, name, value, state, and supported actions, with
    /// the built-in English messages.
    fn semantics(&self) -> SemanticNode;

    /// The semantics with the messages of the window locale. The host
    /// publishes this. A control with built-in text in its name or
    /// description overrides it; the default returns
    /// [`semantics`](Self::semantics).
    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        let _ = strings;
        self.semantics()
    }

    /// Adds the parts of the control, such as items, tabs, buttons, or
    /// cells, as child nodes with bounds in window coordinates, and marks
    /// the part that has the keyboard focus. A large collection adds only
    /// the parts in view. The host publishes them with the node. A part
    /// that lists [`SemanticAction::Activate`] gets a press at its center
    /// from [`ControlHost::perform_part`]. The default adds nothing.
    fn semantic_parts(&self, strings: &Localizer, parts: &mut SemanticParts) {
        let _ = (strings, parts);
    }

    /// The selection, the marked text, and the caret rectangle of an
    /// editable text control, for the input method. Other controls return
    /// `None`.
    fn text_input(&self) -> Option<TextInputInfo> {
        None
    }

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

    /// True when `point`, in window coordinates inside the bounds, is on
    /// a part of the control that takes pointer input. The host gives a
    /// point that misses to the parent node. The default is true.
    fn hit(&self, point: Point) -> bool {
        let _ = point;
        true
    }

    /// True when the control can take the keyboard focus. The default is true.
    fn focusable(&self) -> bool {
        true
    }

    /// Shortcuts that reach the control from anywhere in its window, as
    /// [`ControlEvent::Shortcut`]. The focused control gets the key first.
    fn window_shortcuts(&self) -> &[Shortcut] {
        &[]
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

    /// The host time at which the control wants a [`ControlEvent::Timer`],
    /// for delays such as a hover card's.
    fn deadline(&self) -> Option<f64> {
        None
    }

    /// True while the focused control takes every key, before the menu
    /// shortcuts of the window, for example while it records a shortcut.
    fn captures_keys(&self) -> bool {
        false
    }

    /// Takes a value from the application, of the kind that the control
    /// reports in [`ControlMessage::Changed`]. Returns false when the
    /// control has no such value or the value does not fit. The control
    /// emits no message for it.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let _ = value;
        false
    }

    /// Opens or closes the control's popup, dialog, or disclosure at host
    /// `time`, as the user would, but without a message. Returns false
    /// when the control has nothing to open.
    fn set_open(&mut self, open: bool, time: f64) -> bool {
        let _ = (open, time);
        false
    }
}

/// Owns mounted controls by node.
#[derive(Default)]
pub struct ControlHost {
    controls: HashMap<NodeId, Box<dyn CustomControl>>,
    disabled: HashSet<NodeId>,
    /// Controls that a host marks invalid, for example from a form field
    /// with an error.
    invalid: HashSet<NodeId>,
    hovered: Vec<NodeId>,
    messages: Vec<(NodeId, ControlMessage)>,
    /// The announcements that controls asked for, oldest first.
    announcements: Vec<(String, AnnouncementPriority)>,
    time: f64,
    modifiers: ShortcutModifiers,
    /// The details of the pointer event in dispatch.
    pointer: PointerDetails,
    /// The desktop services, for the clipboard of text controls.
    services: Services,
    /// The viewport of the last layout.
    viewport: Option<Rect>,
    /// Controls that are new or changed since the last layout, which
    /// arrange again even when their bounds stay the same.
    unarranged: HashSet<NodeId>,
    /// Running state transitions, and the last styles of hot nodes.
    transitions: Transitions,
    /// The platform signals that controls send, such as the beep.
    feedback: HostFeedback,
    /// The messages of the window locale.
    strings: HostStrings,
    /// The position of the last pointer event.
    pointer_at: Option<Point>,
    /// The cursor of the hovered part, from the last paint of the hovered
    /// control.
    cursor: Cell<Cursor>,
    /// The open modal controls, bottom first, with their dialogs in
    /// `dialogs`, which keep the focus to give back.
    modal_focus: Vec<(NodeId, DialogId)>,
    dialogs: DialogStack,
    /// The ring that the focused node left for its focus frame at its
    /// last paint.
    framed_ring: Cell<Option<crate::style::FramedRing>>,
    /// The node that had the focus, while the host delivers
    /// `FocusChanged(true)`.
    focus_from: Option<NodeId>,
}

/// The messages of a host: the built-in ones of the root locale until the
/// host gets a [`Localizer`].
struct HostStrings(Rc<Localizer>);

impl Default for HostStrings {
    fn default() -> Self {
        Self(Rc::new(Localizer::new(
            choscorui_core::Locale::root(),
            choscorui_core::Catalog::new(),
        )))
    }
}

/// The [`Feedback`] of a host: [`NoFeedback`] until the host gets one.
struct HostFeedback(Rc<dyn Feedback>);

impl Default for HostFeedback {
    fn default() -> Self {
        Self(Rc::new(NoFeedback))
    }
}

impl ControlHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sends the platform signals of the controls, such as the beep,
    /// through `feedback`. Until then the host uses [`NoFeedback`].
    pub fn set_feedback(&mut self, feedback: Rc<dyn Feedback>) {
        self.feedback = HostFeedback(feedback);
    }

    /// Gives the controls the messages of the window locale: the
    /// application catalog, with the built-in messages for the ids that it
    /// does not define. The host paints and publishes every control again.
    pub fn set_strings(&mut self, tree: &mut Tree, strings: Rc<Localizer>) {
        self.strings = HostStrings(strings);
        for node in self.controls.keys() {
            let _ = tree.invalidate(*node, Dirty::PAINT);
            let _ = tree.invalidate(*node, Dirty::SEMANTICS);
        }
    }

    /// The messages of the window locale.
    pub fn strings(&self) -> &Localizer {
        &self.strings.0
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
        self.unarranged.insert(node);
        Ok(node)
    }

    /// Detaches the node and drops its control.
    pub fn unmount(&mut self, tree: &mut Tree, node: NodeId) -> Option<Box<dyn CustomControl>> {
        let control = self.controls.remove(&node)?;
        self.disabled.remove(&node);
        self.invalid.remove(&node);
        self.hovered.retain(|id| *id != node);
        self.unarranged.remove(&node);
        self.transitions.remove(node);
        let _ = tree.detach(node);
        Some(control)
    }

    /// Puts `control` on `node` in place of its control. The node keeps
    /// its identity, focus, and disabled state, for example when a view
    /// rebuilds a control for a new label.
    pub fn replace(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        mut control: Box<dyn CustomControl>,
    ) -> Result<(), TreeError> {
        if !tree.contains(node) || !self.controls.contains_key(&node) {
            return Err(TreeError::MissingNode);
        }
        let enabled = !self.disabled.contains(&node);
        control.set_enabled(enabled);
        tree.set_focusable(node, enabled && control.focusable())?;
        self.controls.insert(node, control);
        self.unarranged.insert(node);
        Self::mark(tree, node);
        Ok(())
    }

    /// Drops controls whose nodes were detached elsewhere.
    pub fn prune(&mut self, tree: &Tree) -> usize {
        let before = self.controls.len();
        self.controls.retain(|node, _| tree.contains(*node));
        self.disabled.retain(|node| tree.contains(*node));
        self.invalid.retain(|node| tree.contains(*node));
        self.hovered.retain(|node| tree.contains(*node));
        self.unarranged.retain(|node| tree.contains(*node));
        self.transitions.retain(|node| tree.contains(node));
        before - self.controls.len()
    }

    pub fn get(&self, node: NodeId) -> Option<&dyn CustomControl> {
        self.controls.get(&node).map(|control| control.as_ref())
    }
    pub fn get_mut(&mut self, node: NodeId) -> Option<&mut (dyn CustomControl + 'static)> {
        let control = self.controls.get_mut(&node)?;
        self.unarranged.insert(node);
        Some(control.as_mut())
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
    /// pointer, key, or text input, and it leaves keyboard traversal. A
    /// control that shows an overlay or holds a modal scope gets
    /// [`ControlEvent::Dismiss`] first, so that it closes and reports it.
    pub fn set_enabled(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        enabled: bool,
    ) -> Result<(), TreeError> {
        let control = self.controls.get(&node).ok_or(TreeError::MissingNode)?;
        let open = control.overlay().is_some() || control.modal();
        if !enabled && open && !self.disabled.contains(&node) {
            self.deliver(tree, node, EventPhase::Target, &ControlEvent::Dismiss);
        }
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
        tree.invalidate(node, Dirty::SEMANTICS)?;
        self.sync_modal(tree);
        Ok(())
    }

    pub fn is_enabled(&self, node: NodeId) -> bool {
        !self.disabled.contains(&node)
    }

    /// Marks a mounted control invalid or valid, for example while the
    /// form field around it shows an error. The control paints with the
    /// invalid state, and its node gets the invalid semantic state. A
    /// control that is not a field ignores the state.
    pub fn set_invalid(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        invalid: bool,
    ) -> Result<(), TreeError> {
        if !self.controls.contains_key(&node) {
            return Err(TreeError::MissingNode);
        }
        let changed = if invalid {
            self.invalid.insert(node)
        } else {
            self.invalid.remove(&node)
        };
        if changed {
            tree.invalidate(node, Dirty::PAINT)?;
            tree.invalidate(node, Dirty::SEMANTICS)?;
        }
        Ok(())
    }

    pub fn is_invalid(&self, node: NodeId) -> bool {
        self.invalid.contains(&node)
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
        let viewport = tree.bounds(tree.root()).unwrap_or_default();
        self.measure_with(node, constraints, env.direction, &tokens, viewport, env)
    }

    fn measure_with(
        &mut self,
        node: NodeId,
        constraints: Constraints,
        direction: LayoutDirection,
        tokens: &DesignTokens,
        viewport: Rect,
        env: &HostEnv<'_>,
    ) -> Option<Measured> {
        let control = self.controls.get_mut(&node)?;
        let cx = MeasureContext {
            constraints,
            viewport,
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
        let viewport = tree.bounds(tree.root()).unwrap_or_default();
        let placements = {
            let tree: &Tree = tree;
            let mut measure = |node: NodeId, constraints: Constraints, direction| {
                let hosted = self.controls.contains_key(&node)
                    && tree.is_within(node, root)
                    && tree.visible(node) == Some(true);
                let measured = hosted.then(|| env.tokens(tree, node)).and_then(|tokens| {
                    self.measure_with(node, constraints, direction, &tokens, viewport, env)
                });
                measured.unwrap_or_else(|| leaves.measure(node, constraints, direction))
            };
            engine.plan(tree, root, bounds, &mut measure)?
        };
        engine.place(tree, placements)?;
        let unarranged = std::mem::take(&mut self.unarranged);
        if self.viewport == Some(viewport) {
            // Controls that kept their bounds and content keep their part
            // geometry.
            let placed = engine
                .placed()
                .iter()
                .filter(|node| !unarranged.contains(node));
            for node in placed.chain(&unarranged) {
                self.arrange_one(tree, *node, viewport, env);
            }
        } else {
            self.viewport = Some(viewport);
            self.arrange(tree, env);
        }
        Ok(())
    }

    /// Lets every visible control record its part geometry for its
    /// current bounds. Call after layout and after theme changes.
    pub fn arrange(&mut self, tree: &Tree, env: &HostEnv<'_>) {
        let viewport = tree.bounds(tree.root()).unwrap_or_default();
        let nodes: Vec<NodeId> = self.controls.keys().copied().collect();
        for node in nodes {
            self.arrange_one(tree, node, viewport, env);
        }
    }

    /// Lets the control on `node`, if it is visible, record its part
    /// geometry for its current bounds.
    fn arrange_one(&mut self, tree: &Tree, node: NodeId, viewport: Rect, env: &HostEnv<'_>) {
        let Some(bounds) = tree.bounds(node) else {
            return;
        };
        if tree.visible(node) != Some(true) {
            return;
        }
        let Some(control) = self.controls.get_mut(&node) else {
            return;
        };
        let tokens = env.tokens(tree, node);
        let cx = MeasureContext {
            constraints: Constraints::tight(bounds.size),
            viewport,
            direction: env.direction,
            tokens: &tokens,
            styles: env.styles,
            text: env.text,
        };
        control.arrange(&cx, bounds);
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
                ControlEvent::FocusChanged(_)
                    | ControlEvent::Hover(_)
                    | ControlEvent::Dismiss
                    | ControlEvent::WindowVisible(_)
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
                    pointer: self.pointer,
                    services: &self.services,
                    messages: &mut self.messages,
                    announcements: &mut self.announcements,
                    feedback: &*self.feedback.0,
                    strings: &self.strings.0,
                    focus_from: self.focus_from,
                };
                control.event(&mut context, event)
            }
            None => EventReply::Continue,
        }
    }

    /// The topmost open modal control, if any.
    pub fn modal(&self, tree: &Tree) -> Option<NodeId> {
        self.layered(tree)
            .into_iter()
            .rev()
            .find(|node| self.controls.get(node).is_some_and(|c| c.modal()))
    }

    /// The control whose open overlay contains `point`, topmost first.
    fn overlay_at(&self, tree: &Tree, point: Point) -> Option<NodeId> {
        self.layered(tree).into_iter().rev().find(|node| {
            self.controls
                .get(node)
                .filter(|control| control.overlay_takes_input())
                .and_then(|control| control.overlay())
                .is_some_and(|rect| rect.contains(point))
        })
    }

    /// Where a pointer event at `point` goes: an open overlay, then the
    /// hit node, kept inside an open modal.
    /// The control that a pointer event at `point` goes to: the captured
    /// control, an open overlay, or the control under the point.
    pub fn pointer_target(&self, tree: &Tree, point: Point) -> Option<NodeId> {
        self.route(tree, point)
    }

    fn route(&self, tree: &Tree, point: Point) -> Option<NodeId> {
        if let Some(capture) = tree.pointer_capture() {
            return Some(capture);
        }
        let mut target = self
            .overlay_at(tree, point)
            .or_else(|| tree.hit_test(point));
        while let Some(node) = target
            && self.controls.get(&node).is_some_and(|c| !c.hit(point))
        {
            target = tree.parent(node);
        }
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
        let before: Vec<(NodeId, VisualState)> = [self.hovered.first(), path.first()]
            .into_iter()
            .flatten()
            .map(|node| (*node, self.visual_state(tree, *node)))
            .collect();
        self.hovered = path;
        self.note_changes(tree, &before);
        for node in left {
            self.deliver(tree, node, EventPhase::Target, &ControlEvent::Hover(false));
            let _ = tree.invalidate(node, Dirty::PAINT);
        }
        for node in entered.into_iter().rev() {
            self.deliver(tree, node, EventPhase::Target, &ControlEvent::Hover(true));
            let _ = tree.invalidate(node, Dirty::PAINT);
        }
    }

    /// Records each node of `before` whose host state changed, so that its
    /// next paint eases from the earlier state. Then drops the records of
    /// nodes that are no longer hot and run nothing.
    fn note_changes(&self, tree: &Tree, before: &[(NodeId, VisualState)]) {
        for (node, was) in before {
            if self.visual_state(tree, *node) != *was {
                self.transitions.note(*node, *was);
            }
        }
        self.transitions.retain(|node| self.is_hot(tree, node));
    }

    /// A node that the pointer is on, that has focus, or that captures the
    /// pointer: its state can change at the next input, so paint keeps its
    /// last styles.
    fn is_hot(&self, tree: &Tree, node: NodeId) -> bool {
        self.hovered.first() == Some(&node)
            || tree.focus() == Some(node)
            || tree.pointer_capture() == Some(node)
    }

    /// The first ring that the focused node left for its focus frame at
    /// its last paint, in the coordinates of the node's parent. A node
    /// paints none in a frame, so the ring shows once.
    pub fn framed_ring(&self) -> Option<crate::style::FramedRing> {
        self.framed_ring.get()
    }

    /// The counters of the transition store.
    pub fn transition_stats(&self) -> TransitionStats {
        self.transitions.stats()
    }

    /// True while the pointer is over the node, a descendant, or its
    /// overlay.
    pub fn is_hovered(&self, node: NodeId) -> bool {
        self.hovered.contains(&node)
    }

    /// Routes a pointer event: an open overlay first, then capture, target,
    /// and bubble through the tree. A press outside an open overlay sends
    /// `Dismiss` to its control. When a control replies `Stop` to that
    /// `Dismiss`, the press closed its popup, and the host consumes the
    /// press: no control gets it, and focus stays. Else pressing a
    /// focusable control focuses it.
    pub fn dispatch_pointer(&mut self, tree: &mut Tree, event: PointerEvent) {
        self.dispatch_pointer_with(tree, event, PointerDetails::default());
    }

    /// Routes a pointer event as [`dispatch_pointer`](Self::dispatch_pointer)
    /// does, with the device details of the event, such as its click
    /// count.
    pub fn dispatch_pointer_with(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        details: PointerDetails,
    ) {
        self.pointer = details;
        self.pointer_at = Some(event.position());
        self.route_pointer(tree, event);
        self.sync_modal(tree);
    }

    fn route_pointer(&mut self, tree: &mut Tree, event: PointerEvent) {
        let target = self.route(tree, event.position());
        if matches!(event, PointerEvent::Move { .. } | PointerEvent::Down { .. }) {
            self.update_hover(tree, target);
        }
        if let PointerEvent::Down { .. } = event {
            tree.set_input_modality(choscorui_core::InputModality::Pointer);
            if self.dismiss_overlays(tree, target).1 {
                // The press closed a popup: it does not reach the control
                // under the pointer.
                return;
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

    /// Routes a trackpad gesture to the control under its position, then
    /// to its ancestors until one stops it. A gesture with no position,
    /// such as `ScrollEnded`, goes to no control. Returns true when a
    /// control stopped the gesture.
    pub fn dispatch_gesture(&mut self, tree: &mut Tree, event: GestureEvent) -> bool {
        let position = match event {
            GestureEvent::Magnify { position, .. }
            | GestureEvent::Rotate { position, .. }
            | GestureEvent::SmartMagnify { position } => position,
            GestureEvent::ScrollEnded => return false,
        };
        let mut node = self.route(tree, position);
        let mut phase = EventPhase::Target;
        while let Some(current) = node.filter(|node| tree.contains(*node)) {
            let reply = self.deliver(tree, current, phase, &ControlEvent::Gesture(event));
            if reply == EventReply::Stop {
                return true;
            }
            phase = EventPhase::Bubble;
            node = tree.parent(current);
        }
        false
    }

    /// Routes a key event to the focused control, through its ancestors.
    /// Focus outside an open modal moves into it first.
    /// Returns true when a control stopped the event.
    pub fn dispatch_key(&mut self, tree: &mut Tree, event: KeyEvent) -> bool {
        self.sync_modal(tree);
        let mut stopped = false;
        tree.dispatch_key(event, |tree, node, phase, event| {
            let reply = self.deliver(tree, node, phase, &ControlEvent::Key(event));
            stopped |= reply == EventReply::Stop;
            reply
        });
        if let KeyEvent::Down { key, repeat } = event
            && !stopped
        {
            // A held key repeats an edit, such as Undo, but not a
            // shortcut. An unused Escape closes a tooltip, a hover card,
            // or a popup wherever the focus is.
            stopped = EditCommand::from_shortcut(key, self.modifiers)
                .is_some_and(|command| self.dispatch_edit(tree, command))
                || (!repeat && self.dispatch_shortcut(tree, key))
                || (key == Key::Escape && self.dismiss_overlays(tree, None).0);
        }
        self.sync_modal(tree);
        stopped
    }

    /// Sends `Dismiss` to each open overlay that is not modal and does not
    /// hold `keep`, such as a popup, a tooltip, or a hover card. Returns
    /// whether one was open, and whether one of them stopped the event.
    fn dismiss_overlays(&mut self, tree: &mut Tree, keep: Option<NodeId>) -> (bool, bool) {
        let open: Vec<NodeId> = self
            .layered(tree)
            .into_iter()
            .filter(|node| {
                self.controls
                    .get(node)
                    .is_some_and(|control| control.overlay().is_some() && !control.modal())
            })
            .filter(|node| keep.is_none_or(|keep| !tree.is_within(keep, *node)))
            .collect();
        let mut stopped = false;
        for node in &open {
            stopped |= self.deliver(tree, *node, EventPhase::Target, &ControlEvent::Dismiss)
                == EventReply::Stop;
        }
        (!open.is_empty(), stopped)
    }

    /// Moves focus into a control that became modal: to its first
    /// focusable node, or to the control itself. When the control is no
    /// longer modal, focus goes back to the node that had it before. A
    /// [`DialogStack`](crate::DialogStack) keeps the saved focus.
    fn sync_modal(&mut self, tree: &mut Tree) {
        let before = tree.focus();
        while let Some(&(node, id)) = self.modal_focus.last() {
            if tree.contains(node) && self.controls.get(&node).is_some_and(|c| c.modal()) {
                break;
            }
            self.modal_focus.pop();
            self.dialogs.close(tree, id);
        }
        if let Some(modal) = self.modal(tree)
            && !self.modal_focus.iter().any(|(node, _)| *node == modal)
        {
            // Focus that is already inside stays where it is.
            let inside = tree.focus().filter(|focus| tree.is_within(*focus, modal));
            let Ok(id) = self.dialogs.open(tree, modal, Modality::Modal, inside) else {
                return;
            };
            self.modal_focus.push((modal, id));
        }
        let modal = self.modal(tree);
        if let Some(modal) = modal
            && !tree
                .focus()
                .is_some_and(|focus| tree.is_within(focus, modal))
        {
            // A modal control without a focusable node takes the focus.
            let _ = tree.set_focus(Some(modal));
        }
        let after = tree.focus();
        if before != after {
            self.notify_focus(tree, before, after);
        }
    }

    /// Sends an Edit menu command to the focused control. Returns true
    /// when the control used it.
    pub fn dispatch_edit(&mut self, tree: &mut Tree, command: EditCommand) -> bool {
        let Some(focus) = tree.focus() else {
            return false;
        };
        self.deliver(
            tree,
            focus,
            EventPhase::Target,
            &ControlEvent::Edit(command),
        ) == EventReply::Stop
    }

    /// Sends input method composition to the focused control. A control
    /// that does not use composition gets a commit as committed text.
    /// Returns true when the control used it.
    pub fn dispatch_composition(&mut self, tree: &mut Tree, event: CompositionEvent) -> bool {
        let Some(focus) = tree.focus() else {
            return false;
        };
        let reply = self.deliver(
            tree,
            focus,
            EventPhase::Target,
            &ControlEvent::Compose(event.clone()),
        );
        match event {
            _ if reply == EventReply::Stop => true,
            CompositionEvent::Commit { text, .. } => self.dispatch_text(tree, &text),
            _ => false,
        }
    }

    /// The input method state of the focused control, when it edits text.
    pub fn text_input(&self, tree: &Tree) -> Option<TextInputInfo> {
        let focus = tree.focus()?;
        if self.disabled.contains(&focus) {
            return None;
        }
        self.controls.get(&focus)?.text_input()
    }

    /// True while the focused control takes every key, before the menu
    /// shortcuts of the window. See [`CustomControl::captures_keys`].
    pub fn captures_keys(&self, tree: &Tree) -> bool {
        tree.focus()
            .and_then(|focus| self.controls.get(&focus))
            .is_some_and(|control| control.captures_keys())
    }

    /// Gives the controls the desktop services of the backend. Text
    /// controls copy, cut, and paste through its clipboard. Without
    /// clipboard support, Copy, Cut, and Paste do nothing.
    pub fn set_services(&mut self, services: Services) {
        self.services = services;
    }

    /// Sends a key that no focused control used to the first visible,
    /// enabled control that declares it in
    /// [`CustomControl::window_shortcuts`]. An open modal limits the
    /// search to its own controls. Returns true when a control stopped it.
    fn dispatch_shortcut(&mut self, tree: &mut Tree, key: Key) -> bool {
        let shortcut = Shortcut {
            key,
            modifiers: self.modifiers,
        };
        let modal = self.modal(tree);
        let mut owners: Vec<(Vec<usize>, NodeId)> = self
            .controls
            .iter()
            .filter(|(_, control)| control.window_shortcuts().contains(&shortcut))
            .map(|(node, _)| *node)
            .filter(|node| !self.disabled.contains(node) && tree.visible(*node) == Some(true))
            .filter(|node| modal.is_none_or(|modal| tree.is_within(*node, modal)))
            .filter_map(|node| Some((tree_path(tree, node)?, node)))
            .collect();
        owners.sort();
        owners.into_iter().any(|(_, node)| {
            self.deliver(
                tree,
                node,
                EventPhase::Target,
                &ControlEvent::Shortcut(shortcut),
            ) == EventReply::Stop
        })
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
        // Focus is already on `next`; before, it had no focus flags. The
        // previous node was hot, so its last styles are known.
        let focus = VisualState::FOCUSED.with(VisualState::FOCUS_VISIBLE);
        let before: Vec<(NodeId, VisualState)> = next
            .map(|node| (node, self.visual_state(tree, node).without(focus)))
            .into_iter()
            .collect();
        self.note_changes(tree, &before);
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
            self.focus_from = previous;
            self.deliver(
                tree,
                next,
                EventPhase::Target,
                &ControlEvent::FocusChanged(true),
            );
            self.focus_from = None;
            let _ = tree.invalidate(next, Dirty::PAINT);
        }
    }

    /// The visible controls that show an overlay, hold a modal scope,
    /// wait for a timer, or animate, in tree (paint) order. It asks each
    /// control but walks the tree only for these few.
    fn layered(&self, tree: &Tree) -> Vec<NodeId> {
        let time = self.time;
        let mut layered: Vec<(Vec<usize>, NodeId)> = self
            .controls
            .iter()
            .filter(|(_, control)| is_active(control.as_ref(), time))
            .filter_map(|(node, _)| Some((tree_path(tree, *node)?, *node)))
            .collect();
        layered.sort();
        layered.into_iter().map(|(_, node)| node).collect()
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

    /// The host's view of a node's state: hover, focus, disabled, and
    /// invalid.
    pub fn visual_state(&self, tree: &Tree, node: NodeId) -> VisualState {
        VisualState::NONE
            .set(VisualState::HOVERED, self.hovered.first() == Some(&node))
            .set(VisualState::FOCUSED, tree.focus() == Some(node))
            .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(node))
            .set(VisualState::DISABLED, self.disabled.contains(&node))
            .set(VisualState::INVALID, self.invalid.contains(&node))
    }

    /// Paints visible mounted controls in tree order, each clipped to its
    /// bounds, then every open overlay above them.
    pub fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &HostEnv<'_>) {
        let order = self.paint_order(tree);
        for node in &order {
            let tokens = env.tokens(tree, *node);
            self.paint_node(tree, *node, list, env, &tokens);
        }
        self.paint_overlays(tree, list, env);
    }

    /// Paints one mounted control with `tokens`, clipped to its bounds.
    /// A host that walks the tree itself, for example to paint
    /// decorations between controls, calls this for each control and then
    /// [`ControlHost::paint_overlays`]. Returns false when `node` has no
    /// control.
    pub fn paint_node(
        &self,
        tree: &Tree,
        node: NodeId,
        list: &mut DisplayList,
        env: &HostEnv<'_>,
        tokens: &DesignTokens,
    ) -> bool {
        let (Some(control), Some(bounds)) = (self.controls.get(&node), tree.bounds(node)) else {
            return false;
        };
        list.push_clip(bounds);
        let state = self.visual_state(tree, node);
        let hot = self.is_hot(tree, node);
        let pointer = self.hover_pointer(tree, node);
        let earlier = self.cursor.replace(Cursor::Auto);
        let mut cx = PaintContext::new(list, bounds, tokens, env.styles, env.text)
            .rtl(env.direction == LayoutDirection::Rtl)
            .state(state)
            .time(env.time)
            .strings(&self.strings.0);
        if let Some(point) = pointer {
            cx = cx.pointer(point, &self.cursor);
        } else {
            self.cursor.set(earlier);
        }
        let eased = self.transitions.begin(node, hot, state, env.time);
        if eased {
            cx = cx.transitions(&self.transitions);
        }
        if env.framed && state.focus_visible() && tree.focus() == Some(node) {
            self.framed_ring.set(None);
            cx = cx.framed(&self.framed_ring);
        }
        // A disabled control fades as one group, not slot by slot, with
        // the opacity of a disabled button. The group eases like a slot.
        let group = cx
            .style(
                StyleQuery::new(ControlKind::Button, Slot::Root)
                    .state(VisualState::NONE.set(VisualState::DISABLED, state.disabled())),
            )
            .opacity;
        let faded = group < 1.0 && cx.list.push_opacity(group.clamp(0.0, 1.0)).is_ok();
        let mut cx = cx.dimmed(state.disabled());
        control.paint(&mut cx);
        if eased {
            self.transitions.end(hot);
        }
        // A drag keeps its cursor while the pointer leaves the part.
        if pointer.is_some()
            && self.cursor.get() == Cursor::Auto
            && tree.pointer_capture() == Some(node)
        {
            self.cursor.set(earlier);
        }
        if faded {
            let _ = list.pop_opacity();
        }
        let _ = list.pop_clip();
        true
    }

    /// The pointer position while `node` is the hovered control.
    fn hover_pointer(&self, tree: &Tree, node: NodeId) -> Option<Point> {
        let hovered = self.hovered.first() == Some(&node) || tree.pointer_capture() == Some(node);
        self.pointer_at.filter(|_| hovered)
    }

    /// The cursor of the part under the pointer, from the last paint: the
    /// cursor of the topmost slot of the hovered control that sets one.
    /// The window shows [`Cursor::shape`] of it.
    pub fn cursor(&self) -> Cursor {
        if self.hovered.is_empty() {
            Cursor::Auto
        } else {
            self.cursor.get()
        }
    }

    /// Paints every open overlay, and every closing one that still
    /// animates, above the controls, in tree order.
    pub fn paint_overlays(&self, tree: &Tree, list: &mut DisplayList, env: &HostEnv<'_>) {
        let rtl = env.direction == LayoutDirection::Rtl;
        for node in self.layered(tree) {
            let (Some(control), Some(bounds)) = (self.controls.get(&node), tree.bounds(node))
            else {
                continue;
            };
            // A closing popup still animates after its overlay is gone.
            if control.overlay().is_none() && !control.animating(env.time) {
                continue;
            }
            let tokens = env.tokens(tree, node);
            let mut cx = PaintContext::new(list, bounds, &tokens, env.styles, env.text)
                .rtl(rtl)
                .state(self.visual_state(tree, node))
                .time(env.time)
                .strings(&self.strings.0);
            // Over its open popup, the cursor comes from the popup.
            let over = self
                .hover_pointer(tree, node)
                .filter(|point| control.overlay().is_some_and(|rect| rect.contains(*point)));
            if let Some(point) = over {
                self.cursor.set(Cursor::Auto);
                cx = cx.pointer(point, &self.cursor);
            }
            control.paint_overlay(&mut cx);
        }
    }

    /// Gives the control a value from the application, as
    /// [`CustomControl::set_value`] describes. When the control takes it,
    /// the node is marked for layout, paint, and semantics.
    pub fn set_value(&mut self, tree: &mut Tree, node: NodeId, value: &ControlValue) -> bool {
        let taken = self
            .controls
            .get_mut(&node)
            .is_some_and(|control| control.set_value(value));
        if taken {
            Self::mark(tree, node);
        }
        taken
    }

    /// Opens or closes the control's popup, dialog, or disclosure from the
    /// application, as [`CustomControl::set_open`] describes.
    pub fn set_open(&mut self, tree: &mut Tree, node: NodeId, open: bool) -> bool {
        let time = self.time;
        let taken = self
            .controls
            .get_mut(&node)
            .is_some_and(|control| control.set_open(open, time));
        if taken {
            Self::mark(tree, node);
            self.sync_modal(tree);
        }
        taken
    }

    fn mark(tree: &mut Tree, node: NodeId) {
        for flag in [Dirty::LAYOUT, Dirty::PAINT, Dirty::SEMANTICS] {
            let _ = tree.invalidate(node, flag);
        }
    }

    /// Sets the host time and sends [`ControlEvent::Timer`] to each
    /// visible control whose deadline is at or before `time`. It also ends
    /// the state transitions that are finished at `time` and marks their
    /// nodes for paint. Returns true when a timer fired or a transition
    /// ended.
    pub fn advance(&mut self, tree: &mut Tree, time: f64) -> bool {
        self.time = time;
        let ended = self
            .transitions
            .finish(time, |node| self.is_hot(tree, node));
        for node in &ended {
            let _ = tree.invalidate(*node, Dirty::PAINT);
        }
        let due: Vec<NodeId> = self
            .layered(tree)
            .into_iter()
            .filter(|node| {
                self.controls
                    .get(node)
                    .and_then(|control| control.deadline())
                    .is_some_and(|deadline| deadline <= time)
            })
            .collect();
        for node in &due {
            self.deliver(tree, *node, EventPhase::Target, &ControlEvent::Timer);
        }
        !due.is_empty() || !ended.is_empty()
    }

    /// Sends [`ControlEvent::WindowVisible`] to every control, for
    /// example when the whole window is covered or minimized.
    pub fn set_window_visible(&mut self, tree: &mut Tree, visible: bool) {
        let mut nodes: Vec<NodeId> = self.controls.keys().copied().collect();
        nodes.sort_unstable();
        let event = ControlEvent::WindowVisible(visible);
        for node in nodes {
            self.deliver(tree, node, EventPhase::Target, &event);
        }
    }

    /// The earliest deadline of a visible control.
    pub fn next_deadline(&self, tree: &Tree) -> Option<f64> {
        self.layered(tree)
            .into_iter()
            .filter_map(|node| self.controls.get(&node)?.deadline())
            .reduce(f64::min)
    }

    /// True while any visible control animates or runs a state
    /// transition at `time`.
    pub fn animating(&self, tree: &Tree, time: f64) -> bool {
        self.controls
            .iter()
            .any(|(node, control)| tree.visible(*node) == Some(true) && control.animating(time))
            || self
                .transitions
                .running(time, |node| tree.visible(node) == Some(true))
    }

    /// The messages controls emitted since the last call, in order.
    pub fn take_messages(&mut self) -> Vec<(NodeId, ControlMessage)> {
        std::mem::take(&mut self.messages)
    }

    /// Returns the announcements that controls asked for with
    /// [`ControlContext::announce`], oldest first. The window speaks each
    /// one, for example with `Window::announce` on macOS.
    pub fn take_announcements(&mut self) -> Vec<(String, AnnouncementPriority)> {
        std::mem::take(&mut self.announcements)
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
                let mut semantic = control.semantics_in(&self.strings.0);
                if self.disabled.contains(node) {
                    semantic.state.disabled = true;
                    semantic.actions.clear();
                }
                semantic.state.invalid |= self.invalid.contains(node);
                let change = semantics.set(*node, semantic, tree)?;
                if change != SemanticChange::None {
                    changes.push(change);
                }
                changes.extend(semantics.set_parts(*node, self.parts(*node))?);
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

    /// The parts that `node` publishes as child nodes, disabled with the
    /// node.
    pub fn parts(&self, node: NodeId) -> SemanticParts {
        let mut parts = SemanticParts::new();
        if let Some(control) = self.controls.get(&node) {
            control.semantic_parts(&self.strings.0, &mut parts);
            if self.disabled.contains(&node) {
                parts.disable();
            }
        }
        parts
    }

    /// Routes an assistive-technology action to part `part` of `node`.
    /// [`SemanticAction::Activate`] presses and releases the primary
    /// button at the center of the part, as a click does. Another action
    /// goes to the control when the part has the focus, such as
    /// `Increment` on the focused thumb. Returns false when the part is
    /// gone or does not list the action.
    pub fn perform_part(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        part: u64,
        action: SemanticAction,
    ) -> bool {
        let parts = self.parts(node);
        let focused = parts.focused().map(|position| parts.parts()[position].key);
        let Some(part) = parts
            .get(part)
            .filter(|part| part.node.actions.contains(&action))
        else {
            return false;
        };
        if action != SemanticAction::Activate {
            return focused == Some(part.key) && self.perform(tree, node, action, None);
        }
        let position = part.bounds.center();
        let button = PointerButton::Primary;
        self.dispatch_pointer(tree, PointerEvent::Down { position, button });
        self.dispatch_pointer(tree, PointerEvent::Up { position, button });
        true
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
            pointer: self.pointer,
            services: &self.services,
            messages: &mut self.messages,
            announcements: &mut self.announcements,
            feedback: &*self.feedback.0,
            strings: &self.strings.0,
            focus_from: None,
        };
        let done = control.perform(&mut context, action, value);
        self.sync_modal(tree);
        done
    }
}

/// Whether `control` shows an overlay, holds a modal scope, waits for a
/// timer, or animates at `time`.
fn is_active(control: &dyn CustomControl, time: f64) -> bool {
    control.overlay().is_some()
        || control.modal()
        || control.deadline().is_some()
        || control.animating(time)
}

/// The child indices from the root down to `node`, which sort in tree
/// order, or `None` when `node` or an ancestor is hidden or detached.
fn tree_path(tree: &Tree, node: NodeId) -> Option<Vec<usize>> {
    let mut path = Vec::new();
    let mut current = node;
    while let Some(parent) = tree.parent(current) {
        if tree.visible(current) != Some(true) {
            return None;
        }
        let index = tree
            .children(parent)?
            .iter()
            .position(|child| *child == current)?;
        path.push(index);
        current = parent;
    }
    if current != tree.root() || tree.visible(current) == Some(false) {
        return None;
    }
    path.reverse();
    Some(path)
}
