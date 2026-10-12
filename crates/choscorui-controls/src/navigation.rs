//! Scroll container, mnemonics, and dialog default/cancel keys.

use super::{CommandId, CommandRegistry, ShortcutModifiers};
use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, VisualState, content_rect, fill_shape,
};
use choscorui_core::{
    Dirty, Key, KeyEvent, LayoutError, NodeId, Point, PointerButton, PointerEvent, Rect,
    ScrollViewport, SemanticAction, SemanticNode, SemanticRole, SemanticState, Size, Tree,
};

/// Keyboard and assistive-technology scroll requests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollAction {
    LineUp,
    LineDown,
    LineLeft,
    LineRight,
    PageUp,
    PageDown,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollAreaInput {
    Pointer(PointerEvent),
    Key(KeyEvent),
    Action(ScrollAction),
    /// Accessibility increment/decrement map to page down/up.
    Semantic(SemanticAction),
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarGeometry {
    pub track: Rect,
    pub thumb: Rect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Bar {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug)]
struct ThumbDrag {
    bar: Bar,
    /// Pointer position minus thumb origin along the bar's axis.
    grab: f64,
}

/// Scroll container behavior over a core [`ScrollViewport`]: overlay
/// scrollbars, wheel input, keyboard paging, and thumb dragging that keeps
/// pointer capture outside the viewport.
pub struct ScrollArea {
    viewport: ScrollViewport,
    thickness: f64,
    enabled: bool,
    drag: Option<ThumbDrag>,
    thumb_hovered: bool,
    /// The accessible name in a control host.
    name: String,
    /// The bounds from the last host arrange.
    bounds: Rect,
}

const MIN_THUMB: f64 = 20.0;

impl ScrollArea {
    pub fn new(viewport: ScrollViewport) -> Self {
        Self {
            viewport,
            thickness: 10.0,
            enabled: true,
            drag: None,
            thumb_hovered: false,
            name: String::new(),
            bounds: Rect::default(),
        }
    }

    /// Sets the accessible name that the area reports in a control host.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn viewport(&self) -> &ScrollViewport {
        &self.viewport
    }
    pub fn viewport_mut(&mut self) -> &mut ScrollViewport {
        &mut self.viewport
    }
    pub fn node(&self) -> NodeId {
        self.viewport.viewport()
    }

    pub fn set_enabled(&mut self, tree: &mut Tree, enabled: bool) -> Result<(), LayoutError> {
        if self.enabled == enabled {
            return Ok(());
        }
        self.enabled = enabled;
        if !enabled {
            self.release(tree, self.node())?;
        }
        self.invalidate(tree)
    }

    fn geometry(&self, tree: &Tree, bar: Bar) -> Option<ScrollbarGeometry> {
        self.geometry_in(tree.bounds(self.node())?, bar)
    }

    fn geometry_in(&self, bounds: Rect, bar: Bar) -> Option<ScrollbarGeometry> {
        let content = self.viewport.content_size();
        let offset = self.viewport.offset();
        let (visible, total, position) = match bar {
            Bar::Vertical => (bounds.size.height, content.height, offset.y),
            Bar::Horizontal => (bounds.size.width, content.width, offset.x),
        };
        if total <= visible + 0.5 || visible <= 0.0 {
            return None;
        }
        let t = self.thickness;
        let track = match bar {
            Bar::Vertical => Rect::new(
                Point::new(bounds.max_x() - t, bounds.origin.y),
                Size::new(t, bounds.size.height),
            ),
            Bar::Horizontal => Rect::new(
                Point::new(bounds.origin.x, bounds.max_y() - t),
                Size::new(bounds.size.width, t),
            ),
        };
        let length = match bar {
            Bar::Vertical => track.size.height,
            Bar::Horizontal => track.size.width,
        };
        let thumb_length = (length * visible / total).clamp(MIN_THUMB.min(length), length);
        let travel = length - thumb_length;
        let start = travel * (position / (total - visible)).clamp(0.0, 1.0);
        let thumb = match bar {
            Bar::Vertical => Rect::new(
                Point::new(track.origin.x, track.origin.y + start),
                Size::new(t, thumb_length),
            ),
            Bar::Horizontal => Rect::new(
                Point::new(track.origin.x + start, track.origin.y),
                Size::new(thumb_length, t),
            ),
        };
        Some(ScrollbarGeometry { track, thumb })
    }

    pub fn vertical_scrollbar(&self, tree: &Tree) -> Option<ScrollbarGeometry> {
        self.geometry(tree, Bar::Vertical)
    }
    pub fn horizontal_scrollbar(&self, tree: &Tree) -> Option<ScrollbarGeometry> {
        self.geometry(tree, Bar::Horizontal)
    }

    fn contains_node(tree: &Tree, owner: NodeId, node: Option<NodeId>) -> bool {
        let mut current = node;
        while let Some(id) = current {
            if id == owner {
                return true;
            }
            current = tree.parent(id);
        }
        false
    }

    /// Handles one input; returns whether the scroll offset changed.
    pub fn handle(&mut self, tree: &mut Tree, input: ScrollAreaInput) -> Result<bool, LayoutError> {
        self.handle_for(tree, self.node(), input)
    }

    /// Handles one input for `owner`, the node that takes focus and
    /// pointer capture: the viewport node, or the control host's node.
    fn handle_for(
        &mut self,
        tree: &mut Tree,
        owner: NodeId,
        input: ScrollAreaInput,
    ) -> Result<bool, LayoutError> {
        if !tree.contains(self.node()) {
            return Err(LayoutError::MissingNode);
        }
        if !self.enabled {
            return Ok(false);
        }
        let before = self.viewport.offset();
        let visual = self.visual_state(tree);
        match input {
            ScrollAreaInput::Pointer(event) => self.pointer(tree, owner, event)?,
            ScrollAreaInput::Key(KeyEvent::Down { key, .. }) if tree.focus() == Some(owner) => {
                let action = match key {
                    Key::ArrowUp => Some(ScrollAction::LineUp),
                    Key::ArrowDown => Some(ScrollAction::LineDown),
                    Key::ArrowLeft => Some(ScrollAction::LineLeft),
                    Key::ArrowRight => Some(ScrollAction::LineRight),
                    Key::PageUp => Some(ScrollAction::PageUp),
                    Key::PageDown | Key::Space => Some(ScrollAction::PageDown),
                    Key::Home => Some(ScrollAction::Top),
                    Key::End => Some(ScrollAction::Bottom),
                    _ => None,
                };
                if let Some(action) = action {
                    self.perform(tree, action)?;
                }
            }
            ScrollAreaInput::Action(action) => self.perform(tree, action)?,
            ScrollAreaInput::Semantic(SemanticAction::Increment) => {
                self.perform(tree, ScrollAction::PageDown)?;
            }
            ScrollAreaInput::Semantic(SemanticAction::Decrement) => {
                self.perform(tree, ScrollAction::PageUp)?;
            }
            ScrollAreaInput::Semantic(SemanticAction::Focus) => {
                tree.set_focus(Some(owner))
                    .map_err(|_| LayoutError::MissingNode)?;
            }
            ScrollAreaInput::Cancel => self.release(tree, owner)?,
            _ => {}
        }
        let changed = self.viewport.offset() != before;
        if changed || self.visual_state(tree) != visual {
            self.invalidate(tree)?;
        }
        Ok(changed)
    }

    fn perform(&mut self, tree: &mut Tree, action: ScrollAction) -> Result<(), LayoutError> {
        let bounds = tree.bounds(self.node()).ok_or(LayoutError::MissingNode)?;
        let step = self.viewport.line_step();
        let offset = self.viewport.offset();
        let content = self.viewport.content_size();
        // A page keeps one line of context visible.
        let page = (bounds.size.height - step.height).max(step.height);
        let target = match action {
            ScrollAction::LineUp => Point::new(offset.x, offset.y - step.height),
            ScrollAction::LineDown => Point::new(offset.x, offset.y + step.height),
            ScrollAction::LineLeft => Point::new(offset.x - step.width, offset.y),
            ScrollAction::LineRight => Point::new(offset.x + step.width, offset.y),
            ScrollAction::PageUp => Point::new(offset.x, offset.y - page),
            ScrollAction::PageDown => Point::new(offset.x, offset.y + page),
            ScrollAction::Top => Point::new(offset.x, 0.0),
            ScrollAction::Bottom => Point::new(offset.x, content.height),
        };
        self.viewport.scroll_to(tree, target)
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        owner: NodeId,
        event: PointerEvent,
    ) -> Result<(), LayoutError> {
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => {
                for bar in [Bar::Vertical, Bar::Horizontal] {
                    let Some(geometry) = self.geometry(tree, bar) else {
                        continue;
                    };
                    if geometry.thumb.contains(position) {
                        let grab = match bar {
                            Bar::Vertical => position.y - geometry.thumb.origin.y,
                            Bar::Horizontal => position.x - geometry.thumb.origin.x,
                        };
                        self.drag = Some(ThumbDrag { bar, grab });
                        tree.set_pointer_capture(Some(owner))
                            .map_err(|_| LayoutError::MissingNode)?;
                        return Ok(());
                    }
                    if geometry.track.contains(position) {
                        let before = match bar {
                            Bar::Vertical => position.y < geometry.thumb.origin.y,
                            Bar::Horizontal => position.x < geometry.thumb.origin.x,
                        };
                        let action = match (bar, before) {
                            (Bar::Vertical, true) => ScrollAction::PageUp,
                            (Bar::Vertical, false) => ScrollAction::PageDown,
                            (Bar::Horizontal, true) => ScrollAction::LineLeft,
                            (Bar::Horizontal, false) => ScrollAction::LineRight,
                        };
                        return self.perform(tree, action);
                    }
                }
            }
            PointerEvent::Move { position } => {
                self.thumb_hovered = [Bar::Vertical, Bar::Horizontal].iter().any(|bar| {
                    self.geometry(tree, *bar)
                        .is_some_and(|g| g.thumb.contains(position))
                });
                if let Some(drag) = self.drag {
                    self.drag_to(tree, drag, position)?;
                }
            }
            PointerEvent::Up {
                button: PointerButton::Primary,
                ..
            } if self.drag.is_some() => self.release(tree, owner)?,
            PointerEvent::Wheel {
                position,
                delta,
                unit,
            } if Self::contains_node(tree, owner, tree.pointer_capture())
                || Self::contains_node(tree, owner, tree.hit_test(position)) =>
            {
                self.viewport.scroll_by(tree, delta, unit)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn drag_to(
        &mut self,
        tree: &mut Tree,
        drag: ThumbDrag,
        position: Point,
    ) -> Result<(), LayoutError> {
        let Some(geometry) = self.geometry(tree, drag.bar) else {
            return Ok(());
        };
        let bounds = tree.bounds(self.node()).ok_or(LayoutError::MissingNode)?;
        let content = self.viewport.content_size();
        let offset = self.viewport.offset();
        let (track_start, track_len, thumb_len, pointer, visible, total) = match drag.bar {
            Bar::Vertical => (
                geometry.track.origin.y,
                geometry.track.size.height,
                geometry.thumb.size.height,
                position.y,
                bounds.size.height,
                content.height,
            ),
            Bar::Horizontal => (
                geometry.track.origin.x,
                geometry.track.size.width,
                geometry.thumb.size.width,
                position.x,
                bounds.size.width,
                content.width,
            ),
        };
        let travel = (track_len - thumb_len).max(1.0);
        let fraction = ((pointer - drag.grab - track_start) / travel).clamp(0.0, 1.0);
        let along = fraction * (total - visible);
        let target = match drag.bar {
            Bar::Vertical => Point::new(offset.x, along),
            Bar::Horizontal => Point::new(along, offset.y),
        };
        self.viewport.scroll_to(tree, target)
    }

    fn release(&mut self, tree: &mut Tree, owner: NodeId) -> Result<(), LayoutError> {
        self.drag = None;
        if tree.pointer_capture() == Some(owner) {
            tree.set_pointer_capture(None)
                .map_err(|_| LayoutError::MissingNode)?;
        }
        Ok(())
    }

    fn invalidate(&self, tree: &mut Tree) -> Result<(), LayoutError> {
        tree.invalidate(self.node(), Dirty::PAINT)
            .and_then(|_| tree.invalidate(self.node(), Dirty::SEMANTICS))
            .map_err(|_| LayoutError::MissingNode)
    }

    /// The flags the scroll area recipe reads. Hover and press apply to
    /// the thumb.
    pub fn visual_state(&self, tree: &Tree) -> VisualState {
        VisualState::NONE
            .set(VisualState::DISABLED, !self.enabled)
            .set(VisualState::FOCUSED, tree.focus() == Some(self.node()))
            .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(self.node()))
            .set(VisualState::HOVERED, self.thumb_hovered)
            .set(VisualState::PRESSED, self.drag.is_some())
    }

    /// Scroll area semantics; the value is the vertical position in percent.
    pub fn semantics(&self, tree: &Tree, name: impl Into<String>) -> SemanticNode {
        self.semantics_in(tree.bounds(self.node()).unwrap_or_default(), name.into())
    }

    fn semantics_in(&self, bounds: Rect, name: String) -> SemanticNode {
        let range = (self.viewport.content_size().height - bounds.size.height).max(0.0);
        let percent = if range > 0.0 {
            (self.viewport.offset().y / range * 100.0).round()
        } else {
            0.0
        };
        SemanticNode {
            role: SemanticRole::ScrollArea,
            name,
            description: String::new(),
            value: Some(format!("{percent}%")),
            state: SemanticState {
                disabled: !self.enabled,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: if self.enabled {
                vec![
                    SemanticAction::Increment,
                    SemanticAction::Decrement,
                    SemanticAction::Focus,
                ]
            } else {
                Vec::new()
            },
            text: None,
        }
    }

    /// Paints the overlay scrollbars above the content, and the focus
    /// ring around the area. Paint it after the content.
    pub fn paint(&self, cx: &mut PaintContext<'_>, tree: &Tree) {
        if let Some(bounds) = tree.bounds(self.node()) {
            self.paint_in(cx, bounds, self.visual_state(tree));
        }
    }

    fn paint_in(&self, cx: &mut PaintContext<'_>, bounds: Rect, state: VisualState) {
        let query = |slot| StyleQuery::new(ControlKind::ScrollArea, slot).state(state);
        let (bar, thumb) = (
            cx.style(query(Slot::Scrollbar)),
            cx.style(query(Slot::Thumb)),
        );
        for geometry in [
            self.geometry_in(bounds, Bar::Vertical),
            self.geometry_in(bounds, Bar::Horizontal),
        ]
        .into_iter()
        .flatten()
        {
            let rect = content_rect(geometry.thumb, &bar, cx.rtl);
            fill_shape(cx.list, rect, &thumb.radius, cx.rtl, &thumb.background);
        }
        let root = cx.style(query(Slot::Root));
        if root.ring.is_some() {
            cx.slot(bounds, &root, |_, _| {});
        }
    }
}

/// In a control host the area's node holds focus and pointer capture.
/// Before each input it keeps the viewport node on its own bounds, so the
/// view layer places the content node inside the viewport node and the
/// area scrolls it there.
impl crate::CustomControl for ScrollArea {
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        // A scroll area takes the offered size, and its content size when
        // unbounded.
        let max = cx.constraints.max;
        let content = self.viewport.content_size();
        let size = Size::new(
            if max.width.is_finite() {
                max.width
            } else {
                content.width
            },
            if max.height.is_finite() {
                max.height
            } else {
                content.height
            },
        );
        choscorui_core::Measured::new(cx.constraints.constrain(size))
    }

    fn arrange(&mut self, _cx: &crate::MeasureContext<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn event(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        use choscorui_core::{EventPhase, EventReply};
        // The wheel also scrolls while the pointer is over the content.
        let input = match (cx.phase(), event) {
            (
                EventPhase::Target | EventPhase::Bubble,
                crate::ControlEvent::Pointer(event @ PointerEvent::Wheel { .. }),
            ) => ScrollAreaInput::Pointer(*event),
            _ => match crate::hosted::route(cx, event) {
                Some(crate::hosted::Routed::Pointer(event)) => ScrollAreaInput::Pointer(event),
                Some(crate::hosted::Routed::Key(event)) => ScrollAreaInput::Key(event),
                Some(crate::hosted::Routed::Cancel) => ScrollAreaInput::Cancel,
                None => {
                    if let crate::ControlEvent::FocusChanged(_) = event {
                        cx.request_paint();
                    }
                    return EventReply::Continue;
                }
            },
        };
        let (node, bounds) = (cx.node(), cx.bounds());
        let tree = cx.tree_mut();
        if tree.bounds(self.node()) != Some(bounds) && self.viewport.arrange(tree, bounds).is_err()
        {
            return EventReply::Continue;
        }
        let (drag, hovered) = (self.drag.is_some(), self.thumb_hovered);
        let Ok(scrolled) = self.handle_for(tree, node, input) else {
            return EventReply::Continue;
        };
        if scrolled {
            cx.request_semantics();
        }
        if scrolled || drag != self.drag.is_some() || hovered != self.thumb_hovered {
            cx.request_paint();
        }
        let consumed = match input {
            ScrollAreaInput::Pointer(PointerEvent::Wheel { .. }) => scrolled,
            ScrollAreaInput::Pointer(PointerEvent::Down { .. }) => scrolled || self.drag.is_some(),
            ScrollAreaInput::Pointer(PointerEvent::Up { .. }) => drag,
            ScrollAreaInput::Pointer(PointerEvent::Move { .. }) => drag,
            ScrollAreaInput::Key(KeyEvent::Down { key, .. }) => matches!(
                key,
                Key::ArrowUp
                    | Key::ArrowDown
                    | Key::ArrowLeft
                    | Key::ArrowRight
                    | Key::PageUp
                    | Key::PageDown
                    | Key::Space
                    | Key::Home
                    | Key::End
            ),
            _ => false,
        };
        if consumed {
            EventReply::Stop
        } else {
            EventReply::Continue
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx
            .state
            .set(VisualState::HOVERED, self.thumb_hovered)
            .set(VisualState::PRESSED, self.drag.is_some());
        let bounds = cx.bounds;
        self.paint_in(cx, bounds, state);
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(self.bounds, self.name.clone())
    }

    fn perform(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        let input = match action {
            SemanticAction::Focus => {
                cx.request_focus();
                return true;
            }
            SemanticAction::Increment | SemanticAction::Decrement => {
                ScrollAreaInput::Semantic(action)
            }
            _ => return false,
        };
        let (node, bounds) = (cx.node(), cx.bounds());
        let tree = cx.tree_mut();
        if tree.bounds(self.node()) != Some(bounds) && self.viewport.arrange(tree, bounds).is_err()
        {
            return false;
        }
        if let Ok(true) = self.handle_for(tree, node, input) {
            cx.request_paint();
            cx.request_semantics();
        }
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.drag = None;
            self.thumb_hovered = false;
        }
    }
}

/// Splits `&Save` into the display label `Save` and mnemonic `s`. `&&`
/// is a literal ampersand; a trailing `&` is kept literally.
pub fn parse_mnemonic(label: &str) -> (String, Option<char>) {
    let mut display = String::with_capacity(label.len());
    let mut mnemonic = None;
    let mut chars = label.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '&' {
            display.push(ch);
            continue;
        }
        match chars.next() {
            Some('&') => display.push('&'),
            Some(next) => {
                if mnemonic.is_none() && next.is_alphanumeric() {
                    mnemonic = next.to_lowercase().next();
                }
                display.push(next);
            }
            None => display.push('&'),
        }
    }
    (display, mnemonic)
}

/// Result of pressing a mnemonic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MnemonicHit {
    pub node: NodeId,
    /// A unique mnemonic activates; a shared one only moves focus so the
    /// user can press it again to reach the next control.
    pub activate: bool,
}

/// Mnemonics within one window or dialog. On macOS, AppKit has no
/// mnemonic convention; applications opt in and choose the chord (the
/// gallery uses Control+Option+letter) so it never shadows Option text
/// input or Command shortcuts.
#[derive(Default)]
pub struct MnemonicTable {
    entries: Vec<(char, NodeId)>,
}

impl MnemonicTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a label's mnemonic for `node` and returns the display text.
    pub fn register(&mut self, label: &str, node: NodeId) -> String {
        let (display, mnemonic) = parse_mnemonic(label);
        self.entries.retain(|(_, existing)| *existing != node);
        if let Some(ch) = mnemonic {
            self.entries.push((ch, node));
        }
        display
    }

    pub fn remove(&mut self, node: NodeId) {
        self.entries.retain(|(_, existing)| *existing != node);
    }

    /// Nodes sharing `ch`, in registration order.
    pub fn conflicts(&self, ch: char) -> Vec<NodeId> {
        let ch = ch.to_lowercase().next().unwrap_or(ch);
        self.entries
            .iter()
            .filter(|(c, _)| *c == ch)
            .map(|(_, node)| *node)
            .collect()
    }

    /// Resolves a mnemonic press. Hidden, disabled, or detached nodes are
    /// skipped through `available`; shared mnemonics cycle after `focus`.
    pub fn resolve(
        &self,
        ch: char,
        focus: Option<NodeId>,
        available: impl Fn(NodeId) -> bool,
    ) -> Option<MnemonicHit> {
        let candidates: Vec<NodeId> = self
            .conflicts(ch)
            .into_iter()
            .filter(|node| available(*node))
            .collect();
        match candidates.as_slice() {
            [] => None,
            [only] => Some(MnemonicHit {
                node: *only,
                activate: true,
            }),
            many => {
                let next = focus
                    .and_then(|f| many.iter().position(|n| *n == f))
                    .map_or(0, |i| (i + 1) % many.len());
                Some(MnemonicHit {
                    node: many[next],
                    activate: false,
                })
            }
        }
    }
}

/// Default (Return) and cancel (Escape, Command-period) commands for a
/// window or dialog.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DialogKeys {
    pub default: Option<CommandId>,
    pub cancel: Option<CommandId>,
}

impl DialogKeys {
    /// Routes a key not consumed by the focused control. Pass
    /// `focus_uses_enter` for multiline editors and other controls that
    /// treat Return as input. Disabled commands consume the key.
    pub fn route(
        &self,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        focus_uses_enter: bool,
        commands: &CommandRegistry,
    ) -> Option<CommandId> {
        let KeyEvent::Down { key, repeat: false } = event else {
            return None;
        };
        let plain = !modifiers.command && !modifiers.control && !modifiers.option;
        let command = match key {
            Key::Enter if plain && !focus_uses_enter => self.default?,
            Key::Escape if plain => self.cancel?,
            Key::Character('.') if modifiers.command && !modifiers.control => self.cancel?,
            _ => return None,
        };
        commands.activate(command).ok().flatten()
    }
}
