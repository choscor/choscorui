//! A star-rating control written only against the public extension API.

use choscorui_controls::{
    ControlContext, ControlEvent, ControlHost, ControlMessage, ControlValue, CustomControl,
    EstimatedText, HostEnv, MeasureContext, PaintContext, Styles, VisualState,
};
use choscorui_core::{
    Color, ColorRole, Constraints, DesignTokens, Dirty, DisplayCommand, DisplayList, EventPhase,
    EventReply, Key, KeyEvent, LayoutDirection, LayoutEngine, LayoutStyle, Measured, NodeId, Point,
    PointerButton, PointerEvent, Rect, SemanticAction, SemanticChange, SemanticNode, SemanticRole,
    SemanticState, SemanticTree, Size, ThemeResolver, Tree,
};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
struct Rating {
    value: u8,
    focused: bool,
    typed: String,
}

impl Rating {
    fn set(&mut self, context: &mut ControlContext<'_>, value: u8) -> bool {
        let value = value.min(5);
        if value == self.value {
            return false;
        }
        self.value = value;
        context.emit(ControlMessage::Changed(ControlValue::Number(f64::from(
            value,
        ))));
        context.request_paint();
        context.request_semantics();
        true
    }
}

impl CustomControl for Rating {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.constrain(Size::new(100.0, 20.0)))
    }

    fn event(&mut self, context: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if context.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down { position, .. }) => {
                let bounds = context.bounds();
                let star = ((position.x - bounds.origin.x) / 20.0).floor() as u8 + 1;
                self.set(context, star);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::ArrowRight,
                ..
            }) => {
                let next = self.value + 1;
                self.set(context, next);
                EventReply::Stop
            }
            ControlEvent::Text(text) => {
                self.typed.push_str(text);
                if let Some(digit) = text.chars().next().and_then(|c| c.to_digit(10)) {
                    self.set(context, digit as u8);
                }
                EventReply::Stop
            }
            ControlEvent::FocusChanged(focused) => {
                self.focused = *focused;
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let (bounds, tokens) = (cx.bounds, cx.tokens);
        for star in 0..5 {
            let color = if star < self.value {
                tokens.color(ColorRole::Primary)
            } else {
                Color::rgba8(200, 200, 200, 255)
            };
            cx.list.fill_rect(
                Rect::new(
                    Point::new(bounds.origin.x + star as f64 * 20.0, bounds.origin.y),
                    Size::new(18.0, 18.0),
                ),
                color,
            );
        }
        if cx.state.focused() {
            cx.list
                .fill_rect(bounds, tokens.color(ColorRole::Primary).with_alpha(0.1));
        }
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Slider,
            name: "Rating".into(),
            description: String::new(),
            value: Some(format!("{} of 5 stars", self.value)),
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Increment, SemanticAction::Decrement],
            text: None,
        }
    }

    fn perform(
        &mut self,
        context: &mut ControlContext<'_>,
        action: SemanticAction,
        _: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Increment => {
                let next = self.value + 1;
                self.set(context, next)
            }
            SemanticAction::Decrement => {
                let next = self.value.saturating_sub(1);
                self.set(context, next)
            }
            _ => false,
        }
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(w, h))
}

#[test]
fn custom_control_participates_in_layout_input_paint_and_semantics() {
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
    let mut host = ControlHost::new();
    let root = tree.root();
    let node = host
        .mount(&mut tree, root, Box::new(Rating::default()))
        .unwrap();
    let themes = ThemeResolver::new(DesignTokens::light());
    let styles = Styles::new();
    let env = HostEnv::new(&themes, &styles, &EstimatedText);
    let measured = host
        .measure(&tree, node, Constraints::loose(Size::new(80.0, 50.0)), &env)
        .unwrap();
    assert_eq!(
        measured.size,
        Size::new(80.0, 20.0),
        "constrained by the host"
    );
    tree.set_bounds(node, rect(10.0, 10.0, 100.0, 20.0))
        .unwrap();

    let mut semantics = SemanticTree::new();
    let changes = host
        .sync_semantics(&mut tree, &mut semantics, false)
        .unwrap();
    assert_eq!(
        changes,
        vec![SemanticChange::Tree],
        "mount marks semantics dirty"
    );

    // Clicking the third star focuses the control and sets the value.
    host.dispatch_pointer(
        &mut tree,
        PointerEvent::Down {
            position: Point::new(55.0, 15.0),
            button: PointerButton::Primary,
        },
    );
    assert_eq!(tree.focus(), Some(node));
    let changes = host
        .sync_semantics(&mut tree, &mut semantics, false)
        .unwrap();
    assert_eq!(changes, vec![SemanticChange::Value(node)]);
    assert!(
        tree.take_dirty(node).unwrap().contains(Dirty::PAINT),
        "paint requests survive semantics sync"
    );

    host.dispatch_key(
        &mut tree,
        KeyEvent::Down {
            key: Key::ArrowRight,
            repeat: false,
        },
    );
    assert!(host.dispatch_text(&mut tree, "5"));
    assert!(host.perform(&mut tree, node, SemanticAction::Decrement, None));
    assert!(
        !host.perform(&mut tree, node, SemanticAction::Activate, None),
        "not advertised"
    );
    host.sync_semantics(&mut tree, &mut semantics, false)
        .unwrap();
    let snapshot = semantics.snapshot(&tree);
    let entry = snapshot.iter().find(|entry| entry.id == node).unwrap();
    assert_eq!(entry.node.value.as_deref(), Some("4 of 5 stars"));
    let values: Vec<_> = host
        .take_messages()
        .into_iter()
        .map(|(from, message)| {
            assert_eq!(from, node);
            message
        })
        .collect();
    assert_eq!(
        values,
        [3.0, 4.0, 5.0, 4.0].map(|v| ControlMessage::Changed(ControlValue::Number(v))),
        "every user change is reported once, in order"
    );
    assert!(host.take_messages().is_empty());

    let mut list = DisplayList::new();
    host.paint(&tree, &mut list, &env);
    let fills = list
        .commands()
        .iter()
        .filter(|command| matches!(command, DisplayCommand::FillRect { .. }))
        .count();
    assert_eq!(fills, 6, "five stars plus the focus highlight");
    assert!(
        matches!(list.commands().first(), Some(DisplayCommand::PushClip(_))),
        "the host clips each control to its bounds"
    );
    assert!(list.is_balanced());

    // Focus leaving notifies the control.
    host.move_focus(&mut tree, None);
    tree.detach(node).unwrap();
    assert_eq!(host.prune(&tree), 1);
    assert!(!host.contains(node));
}

/// What the test reads back from a mounted [`Popup`].
#[derive(Default)]
struct PopupState {
    open: bool,
    modal: bool,
    hovered: bool,
    presses: usize,
    bounds: Rect,
}

/// A trigger that opens a popup below itself; modal when `modal` is set.
#[derive(Clone, Default)]
struct Popup(Rc<RefCell<PopupState>>);

impl Popup {
    fn modal() -> Self {
        let popup = Self::default();
        popup.0.borrow_mut().modal = true;
        popup
    }
}

impl CustomControl for Popup {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.constrain(Size::new(40.0, 20.0)))
    }

    fn arrange(&mut self, _: &MeasureContext<'_>, bounds: Rect) {
        self.0.borrow_mut().bounds = bounds;
    }

    fn event(&mut self, context: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if context.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down { .. }) => {
                let mut state = self.0.borrow_mut();
                state.presses += 1;
                if !state.open {
                    state.open = true;
                    context.emit(ControlMessage::OpenChanged(true));
                }
                EventReply::Stop
            }
            ControlEvent::Dismiss => {
                self.0.borrow_mut().open = false;
                context.emit(ControlMessage::Dismissed);
                EventReply::Stop
            }
            ControlEvent::Hover(hovered) => {
                self.0.borrow_mut().hovered = *hovered;
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        cx.list
            .fill_rect(cx.bounds, cx.tokens.color(ColorRole::Primary));
    }

    fn overlay(&self) -> Option<Rect> {
        let state = self.0.borrow();
        state
            .open
            .then(|| rect(state.bounds.origin.x, state.bounds.max_y(), 100.0, 60.0))
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        if let Some(overlay) = self.overlay() {
            cx.list
                .fill_rect(overlay, cx.tokens.color(ColorRole::Popover));
        }
    }

    fn modal(&self) -> bool {
        let state = self.0.borrow();
        state.modal && state.open
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Button,
            name: "Open".into(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Activate],
            text: None,
        }
    }
}

fn down(x: f64, y: f64) -> PointerEvent {
    PointerEvent::Down {
        position: Point::new(x, y),
        button: PointerButton::Primary,
    }
}

struct Fixture {
    tree: Tree,
    host: ControlHost,
    themes: ThemeResolver,
    styles: Styles,
}

impl Fixture {
    fn new() -> Self {
        Self {
            tree: Tree::new(rect(0.0, 0.0, 400.0, 300.0)),
            host: ControlHost::new(),
            themes: ThemeResolver::new(DesignTokens::light()),
            styles: Styles::new(),
        }
    }

    fn mount(&mut self, parent: NodeId, control: &Popup, bounds: Rect) -> NodeId {
        let node = self
            .host
            .mount(&mut self.tree, parent, Box::new(control.clone()))
            .unwrap();
        self.tree.set_bounds(node, bounds).unwrap();
        node
    }

    fn arrange(&mut self) {
        let env = HostEnv::new(&self.themes, &self.styles, &EstimatedText);
        self.host.arrange(&self.tree, &env);
    }

    fn paint(&self) -> DisplayList {
        let env = HostEnv::new(&self.themes, &self.styles, &EstimatedText)
            .direction(LayoutDirection::Rtl)
            .time(2.0);
        let mut list = DisplayList::new();
        self.host.paint(&self.tree, &mut list, &env);
        list
    }
}

#[test]
fn hover_enters_and_leaves_every_control_on_the_path() {
    let mut f = Fixture::new();
    let root = f.tree.root();
    let (outer, inner) = (Popup::default(), Popup::default());
    let outer_node = f.mount(root, &outer, rect(0.0, 0.0, 200.0, 100.0));
    let inner_node = f.mount(outer_node, &inner, rect(10.0, 10.0, 40.0, 20.0));
    f.arrange();

    f.host.dispatch_pointer(
        &mut f.tree,
        PointerEvent::Move {
            position: Point::new(20.0, 15.0),
        },
    );
    assert!(outer.0.borrow().hovered && inner.0.borrow().hovered);
    assert!(f.host.is_hovered(outer_node) && f.host.is_hovered(inner_node));
    let state = f.host.visual_state(&f.tree, inner_node);
    assert!(state.hovered(), "the innermost control paints as hovered");
    assert!(!f.host.visual_state(&f.tree, outer_node).hovered());

    f.host.dispatch_pointer(
        &mut f.tree,
        PointerEvent::Move {
            position: Point::new(150.0, 50.0),
        },
    );
    assert!(outer.0.borrow().hovered, "still inside the container");
    assert!(!inner.0.borrow().hovered, "left the inner control");

    f.host.dispatch_pointer(
        &mut f.tree,
        PointerEvent::Move {
            position: Point::new(390.0, 290.0),
        },
    );
    assert!(!outer.0.borrow().hovered);
}

#[test]
fn overlays_get_hits_first_and_outside_presses_dismiss_them() {
    let mut f = Fixture::new();
    let root = f.tree.root();
    let (menu, other) = (Popup::default(), Popup::default());
    let menu_node = f.mount(root, &menu, rect(0.0, 0.0, 40.0, 20.0));
    let other_node = f.mount(root, &other, rect(0.0, 40.0, 40.0, 20.0));
    f.arrange();

    f.host.dispatch_pointer(&mut f.tree, down(10.0, 10.0));
    assert!(menu.0.borrow().open);
    assert_eq!(
        f.host.take_messages(),
        vec![(menu_node, ControlMessage::OpenChanged(true))]
    );

    // The overlay covers the other control, so the press goes to the menu.
    f.host.dispatch_pointer(&mut f.tree, down(20.0, 45.0));
    assert_eq!(menu.0.borrow().presses, 2);
    assert_eq!(other.0.borrow().presses, 0);
    assert!(menu.0.borrow().open, "a press inside the overlay keeps it");

    let list = f.paint();
    let last = list.commands().last().unwrap();
    assert!(
        matches!(last, DisplayCommand::FillRect { color, .. }
            if *color == DesignTokens::light().color(ColorRole::Popover)),
        "overlays paint after every control"
    );
    assert!(list.is_balanced());

    // A press outside the overlay dismisses it and still reaches the target.
    f.host.dispatch_pointer(&mut f.tree, down(300.0, 200.0));
    assert!(!menu.0.borrow().open);
    assert_eq!(
        f.host.take_messages(),
        vec![(menu_node, ControlMessage::Dismissed)]
    );
    f.host.dispatch_pointer(&mut f.tree, down(10.0, 50.0));
    assert_eq!(other.0.borrow().presses, 1);
    assert_eq!(f.tree.focus(), Some(other_node));
}

#[test]
fn a_modal_overlay_keeps_pointer_input_and_focus_inside() {
    let mut f = Fixture::new();
    let root = f.tree.root();
    let (dialog, child, outside) = (Popup::modal(), Popup::default(), Popup::default());
    let dialog_node = f.mount(root, &dialog, rect(100.0, 100.0, 200.0, 100.0));
    let child_node = f.mount(dialog_node, &child, rect(110.0, 110.0, 40.0, 20.0));
    let outside_node = f.mount(root, &outside, rect(0.0, 0.0, 40.0, 20.0));
    f.arrange();
    f.host.move_focus(&mut f.tree, Some(outside_node));

    f.host.dispatch_pointer(&mut f.tree, down(200.0, 180.0));
    assert!(dialog.0.borrow().open);
    assert_eq!(f.host.modal(&f.tree), Some(dialog_node));

    // A press outside goes to the modal, not to the control under it.
    f.host.dispatch_pointer(&mut f.tree, down(10.0, 10.0));
    assert_eq!(outside.0.borrow().presses, 0);
    assert_eq!(dialog.0.borrow().presses, 2);
    assert!(
        dialog.0.borrow().open,
        "a modal is not dismissed by a press"
    );

    // Traversal cycles inside the modal subtree.
    f.host.move_focus(&mut f.tree, Some(outside_node));
    let mut seen = Vec::new();
    for _ in 0..4 {
        seen.push(f.host.focus_next(&mut f.tree, false).unwrap());
    }
    assert!(
        seen.iter()
            .all(|node| [dialog_node, child_node].contains(node))
    );
    assert!(seen.contains(&child_node));

    // Keys while focus sits outside move it into the modal first.
    f.host.move_focus(&mut f.tree, Some(outside_node));
    f.host.dispatch_key(
        &mut f.tree,
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false,
        },
    );
    let focus = f.tree.focus().unwrap();
    assert!(f.tree.is_within(focus, dialog_node));
}

#[test]
fn disabled_controls_get_no_input_and_paint_disabled() {
    let mut f = Fixture::new();
    let root = f.tree.root();
    let button = Popup::default();
    let node = f.mount(root, &button, rect(0.0, 0.0, 40.0, 20.0));
    f.arrange();
    f.host.set_enabled(&mut f.tree, node, false).unwrap();
    assert!(!f.host.is_enabled(node));

    f.host.dispatch_pointer(&mut f.tree, down(10.0, 10.0));
    assert_eq!(button.0.borrow().presses, 0);
    assert_eq!(
        f.tree.focus(),
        None,
        "a disabled control does not take focus"
    );
    assert_eq!(f.host.focus_next(&mut f.tree, false), None);
    assert!(
        f.host
            .visual_state(&f.tree, node)
            .contains(VisualState::DISABLED)
    );
    assert!(
        !f.host
            .perform(&mut f.tree, node, SemanticAction::Activate, None)
    );

    let mut semantics = SemanticTree::new();
    f.host
        .sync_semantics(&mut f.tree, &mut semantics, true)
        .unwrap();
    let snapshot = semantics.snapshot(&f.tree);
    let entry = snapshot.iter().find(|entry| entry.id == node).unwrap();
    assert!(entry.node.state.disabled);
    assert!(entry.node.actions.is_empty());

    f.host.set_enabled(&mut f.tree, node, true).unwrap();
    f.host.dispatch_pointer(&mut f.tree, down(10.0, 10.0));
    assert_eq!(button.0.borrow().presses, 1);
    assert_eq!(f.tree.focus(), Some(node));
}

#[test]
fn host_layout_measures_controls_and_other_leaves_then_arranges() {
    let mut f = Fixture::new();
    let root = f.tree.root();
    let popup = Popup::default();
    let control = f.mount(root, &popup, Rect::default());
    let leaf = f.tree.attach(root, Rect::default()).unwrap();
    let mut engine = LayoutEngine::new();
    engine
        .set_style(&f.tree, root, LayoutStyle::column(4.0))
        .unwrap();
    let env = HostEnv::new(&f.themes, &f.styles, &EstimatedText);
    let mut others = Vec::new();
    f.host
        .layout(
            &mut engine,
            &mut f.tree,
            root,
            rect(0.0, 0.0, 400.0, 300.0),
            &env,
            &mut |node, _, _| {
                others.push(node);
                Measured::new(Size::new(10.0, 30.0))
            },
        )
        .unwrap();
    assert!(others.iter().all(|node| *node == leaf));
    assert_eq!(f.tree.bounds(control).unwrap().size.height, 20.0);
    assert_eq!(f.tree.bounds(leaf).unwrap().origin.y, 24.0);
    assert_eq!(
        popup.0.borrow().bounds,
        f.tree.bounds(control).unwrap(),
        "arranged after layout"
    );
}
