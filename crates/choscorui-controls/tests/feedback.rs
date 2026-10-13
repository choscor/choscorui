//! Alert, Spinner, Skeleton, and Empty through the control host.

mod hosted;

use choscorui_controls::{
    Alert, AlertVariant, Avatar, ButtonVariant, ControlMessage, Empty, Skeleton, Spinner,
};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, DisplayList, Icon, Key, KeyEvent, LayoutDirection,
    Point, PointerButton, PointerEvent, SemanticRole, ThemeResolver,
};
use hosted::{Harness, paints, texts};

fn tab(h: &mut Harness) -> bool {
    h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false,
        },
    )
}

fn reduced(h: &mut Harness) {
    let mut tokens = DesignTokens::light();
    tokens.motion.reduced = true;
    h.themes = ThemeResolver::new(tokens);
    let bounds = h.bounds();
    h.place(bounds);
}

fn transforms(list: &DisplayList) -> usize {
    list.commands()
        .iter()
        .filter(|command| matches!(command, DisplayCommand::PushTransform(_)))
        .count()
}

fn opacities(list: &DisplayList) -> Vec<f64> {
    list.commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::PushOpacity(opacity) => Some(*opacity),
            _ => None,
        })
        .collect()
}

#[test]
fn alert_shows_its_text_and_tabs_through_its_actions() {
    let tokens = DesignTokens::light();
    let mut h = Harness::new(
        Alert::new(
            "Unable to process your payment.",
            "Check your card details.",
        )
        .with_variant(AlertVariant::Destructive)
        .with_icon(Icon::CircleAlert)
        .with_action("Retry", ButtonVariant::Outline)
        .with_action("Dismiss", ButtonVariant::Ghost),
    );
    assert_eq!(h.measure().width, 480.0);
    let list = h.paint();
    let painted = texts(&list);
    for text in ["Unable to process your payment.", "Retry", "Dismiss"] {
        assert!(painted.contains(&text.to_string()), "{text}");
    }
    assert!(paints(&list, tokens.color(ColorRole::Destructive)));
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::Group);
    assert_eq!(node.value.as_deref(), Some("Check your card details."));
    h.focus();
    assert_eq!(h.semantics().description, "Retry");
    assert!(tab(&mut h));
    h.key(Key::Enter);
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(1)]);
    assert!(!tab(&mut h));

    let plain = Harness::new(Alert::new("Heads up!", "You can add components."));
    assert!(!plain.host.get(plain.node).unwrap().focusable());
}

#[test]
fn alert_actions_sit_at_the_top_end_and_mirror() {
    let make = || {
        Alert::new("Update", "A new version is ready.")
            .with_action("Install", ButtonVariant::Default)
    };
    let mut h = Harness::new(make());
    let bounds = h.bounds();
    h.click(Point::new(bounds.max_x() - 30.0, bounds.origin.y + 20.0));
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(0)]);
    let mut rtl = Harness::with_direction(make(), LayoutDirection::Rtl);
    let bounds = rtl.bounds();
    rtl.click(Point::new(bounds.origin.x + 30.0, bounds.origin.y + 20.0));
    assert_eq!(rtl.messages(), [ControlMessage::ItemActivated(0)]);
}

#[test]
fn spinner_spins_unless_motion_is_reduced() {
    let mut h = Harness::new(Spinner::new("Loading"));
    assert_eq!(h.measure().width, 16.0);
    assert_eq!(h.semantics().role, SemanticRole::ProgressIndicator);
    assert_eq!(h.semantics().name, "Loading");
    assert!(h.host.animating(&h.tree, 0.0));
    h.set_time(0.25);
    assert_eq!(transforms(&h.paint()), 1);
    reduced(&mut h);
    assert!(!h.host.animating(&h.tree, 0.25), "a still spinner");
}

#[test]
fn skeleton_pulses_unless_motion_is_reduced() {
    let mut h = Harness::new(Skeleton::new("Loading", 250.0, 16.0));
    assert_eq!(h.measure().width, 250.0);
    assert!(!h.host.get(h.node).unwrap().focusable());
    h.set_time(1.0);
    assert_eq!(opacities(&h.paint()), [0.5], "halfway through the pulse");
    reduced(&mut h);
    assert!(opacities(&h.paint()).is_empty());
    assert!(!h.host.animating(&h.tree, 1.0));
    let mut circle = Harness::new(Skeleton::new("Loading", 48.0, 48.0).circle());
    assert_eq!(circle.measure().height, 48.0);
}

#[test]
fn empty_centers_its_parts_and_activates_buttons() {
    let mut h = Harness::new(
        Empty::new(
            "No projects yet",
            "Get started by creating your first project.",
        )
        .with_icon(Icon::Inbox)
        .with_action("Create project", ButtonVariant::Default)
        .with_action("Import project", ButtonVariant::Outline),
    );
    assert_eq!(h.measure().width, 384.0 + 96.0);
    let painted = texts(&h.paint());
    assert_eq!(
        painted,
        [
            "No projects yet",
            "Get started by creating your first project.",
            "Create project",
            "Import project"
        ]
    );
    h.focus();
    assert!(tab(&mut h));
    h.key(Key::Space);
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(1)]);

    let avatar =
        Harness::new(Empty::new("User offline", "").with_avatar(Avatar::new("Ana Lee", "AL")));
    assert_eq!(texts(&avatar.paint()), ["AL", "User offline"]);
}

/// The opacity layers of `list`, in paint order.
fn opacity_layers(list: &DisplayList) -> Vec<f64> {
    list.commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::PushOpacity(opacity) => Some(*opacity),
            _ => None,
        })
        .collect()
}

/// Lets the state transitions finish.
fn settle(h: &mut Harness) {
    let time = h.time + 1.0;
    h.paint();
    h.set_time(time);
}

fn press(h: &mut Harness, down: bool, position: Point) {
    let button = PointerButton::Primary;
    let event = if down {
        PointerEvent::Down { position, button }
    } else {
        PointerEvent::Up { position, button }
    };
    h.host.dispatch_pointer(&mut h.tree, event);
}

#[test]
fn panel_buttons_show_hover_press_while_inside_and_disabled() {
    let tokens = DesignTokens::light();
    let primary = tokens.color(ColorRole::Primary);
    let mut h = Harness::new(
        Alert::new("Update", "A new version is ready.")
            .with_action("Install", ButtonVariant::Default),
    );
    let rest = hosted::rounded_fills(&h.paint(), primary);
    assert_eq!(rest.len(), 1, "one button at rest");
    let button = rest[0];
    let inside = button.center();

    h.hover(inside);
    settle(&mut h);
    let hovered = h.paint();
    assert!(hosted::rounded_fills(&hovered, primary).is_empty());
    assert_eq!(
        hosted::rounded_fills(&hovered, primary.with_alpha(0.8)),
        [button],
        "a Move over the button shows hover"
    );

    press(&mut h, true, inside);
    settle(&mut h);
    let hover = primary.with_alpha(0.8);
    let down = hosted::rounded_fills(&h.paint(), hover);
    assert_eq!(down.len(), 1);
    assert_eq!(down[0].min_y(), button.min_y() + 1.0, "the pressed offset");

    let outside = Point::new(h.bounds().origin.x + 4.0, h.bounds().max_y() - 4.0);
    h.hover(outside);
    settle(&mut h);
    assert_eq!(
        hosted::rounded_fills(&h.paint(), primary),
        [button],
        "dragged off, the button shows no press"
    );
    h.hover(inside);
    settle(&mut h);
    assert_eq!(hosted::rounded_fills(&h.paint(), hover), down);
    press(&mut h, false, outside);
    assert!(h.messages().is_empty(), "a release outside does nothing");

    h.set_enabled(false);
    h.hover(inside);
    settle(&mut h);
    let disabled = h.paint();
    assert_eq!(opacity_layers(&disabled), [0.5], "one group opacity");
    assert_eq!(hosted::rounded_fills(&disabled, primary), [button]);
}
