//! A star-rating control written only against the public extension API.

use choscorui_controls::{ControlContext, ControlEvent, ControlHost, CustomControl};
use choscorui_core::{
    Color, ColorRole, Constraints, DesignTokens, Dirty, DisplayCommand, DisplayList, EventPhase,
    EventReply, Key, KeyEvent, LayoutDirection, Measured, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticChange, SemanticNode, SemanticRole, SemanticState, SemanticTree, Size,
    Tree,
};

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
        context.request_paint();
        context.request_semantics();
        true
    }
}

impl CustomControl for Rating {
    fn measure(&mut self, constraints: Constraints, _: LayoutDirection) -> Measured {
        Measured::new(constraints.constrain(Size::new(100.0, 20.0)))
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

    fn paint(&self, list: &mut DisplayList, bounds: Rect, tokens: &DesignTokens, focused: bool) {
        for star in 0..5 {
            let color = if star < self.value {
                tokens.color(ColorRole::Primary)
            } else {
                Color::rgba8(200, 200, 200, 255)
            };
            list.fill_rect(
                Rect::new(
                    Point::new(bounds.origin.x + star as f64 * 20.0, bounds.origin.y),
                    Size::new(18.0, 18.0),
                ),
                color,
            );
        }
        if focused {
            list.fill_rect(bounds, tokens.color(ColorRole::Primary).with_alpha(0.1));
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
    let measured = host
        .measure(
            node,
            Constraints::loose(Size::new(80.0, 50.0)),
            LayoutDirection::Ltr,
        )
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

    let mut list = DisplayList::new();
    host.paint(&tree, &mut list, &DesignTokens::light());
    let fills = list
        .commands()
        .iter()
        .filter(|command| matches!(command, DisplayCommand::FillRect { .. }))
        .count();
    assert_eq!(fills, 6, "five stars plus the focus highlight");
    assert!(list.is_balanced());

    // Focus leaving notifies the control.
    host.move_focus(&mut tree, None);
    tree.detach(node).unwrap();
    assert_eq!(host.prune(&tree), 1);
    assert!(!host.contains(node));
}
