//! Card, Badge, and Avatar through the control host.

mod hosted;

use choscorui_controls::{
    Avatar, AvatarGroup, Badge, BadgeVariant, ButtonVariant, Card, ControlMessage, ControlSize,
};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, Image, Key, KeyEvent, LayoutDirection, Point,
    SemanticAction, SemanticRole,
};
use hosted::{Harness, fills, rounded_fills, texts};

fn login() -> Card {
    Card::new("Login to your account", "Enter your email below.")
        .with_action("Sign up", ButtonVariant::Link)
        .with_content("Use the address that your team invited.")
        .with_footer("Login", ButtonVariant::Default)
        .with_footer("Login with Google", ButtonVariant::Outline)
}

fn tab(h: &mut Harness) -> bool {
    h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false,
        },
    )
}

#[test]
fn card_paints_its_sections_and_tabs_through_its_buttons() {
    let mut h = Harness::new(login());
    assert_eq!(h.measure().width, 384.0, "a small container");
    let painted = texts(&h.paint());
    for text in [
        "Login to your account",
        "Enter your email below.",
        "Sign up",
        "Login",
        "Login with Google",
    ] {
        assert!(painted.contains(&text.to_string()), "{text}");
    }
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::Group);
    assert_eq!(node.name, "Login to your account");
    assert_eq!(
        node.value.as_deref(),
        Some("Enter your email below.\nUse the address that your team invited.")
    );
    h.focus();
    assert_eq!(
        h.semantics().description,
        "Sign up",
        "the action comes first"
    );
    assert!(tab(&mut h));
    assert!(tab(&mut h));
    assert_eq!(h.semantics().description, "Login with Google");
    h.key(Key::Enter);
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(2)]);
    assert!(!tab(&mut h), "Tab leaves after the last button");
    assert!(h.perform(SemanticAction::Activate, None));
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(2)]);
}

#[test]
fn card_buttons_sit_at_the_ends_and_mirror() {
    let mut h = Harness::new(login());
    let bounds = h.bounds();
    // The header action is at the top end, the footer at the bottom end.
    h.click(Point::new(bounds.max_x() - 30.0, bounds.origin.y + 36.0));
    h.click(Point::new(bounds.max_x() - 30.0, bounds.max_y() - 36.0));
    assert_eq!(
        h.messages(),
        [
            ControlMessage::ItemActivated(0),
            ControlMessage::ItemActivated(2),
        ]
    );
    let mut rtl = Harness::with_direction(login(), LayoutDirection::Rtl);
    let bounds = rtl.bounds();
    rtl.click(Point::new(bounds.origin.x + 30.0, bounds.origin.y + 36.0));
    assert_eq!(rtl.messages(), [ControlMessage::ItemActivated(0)]);

    let mut small = Harness::new(login().with_size(ControlSize::Sm));
    assert!(small.measure().height < h.measure().height);
    let mut plain = Harness::new(Card::new("Notes", "Nothing to do."));
    assert!(!plain.host.get(plain.node).unwrap().focusable());
    assert!(!plain.perform(SemanticAction::Activate, None));
}

#[test]
fn a_plain_badge_is_text_and_a_link_badge_activates() {
    let tokens = DesignTokens::light();
    let mut h = Harness::new(Badge::new("Secondary").with_variant(BadgeVariant::Secondary));
    assert_eq!(h.measure().height, 20.0);
    assert_eq!(h.semantics().role, SemanticRole::StaticText);
    assert!(!h.host.get(h.node).unwrap().focusable());
    assert!(fills(&h.paint(), tokens.color(ColorRole::Secondary)));
    let hovered_secondary = tokens.color(ColorRole::Secondary).with_alpha(0.8);
    h.hover(h.bounds().center());
    assert!(
        !fills(&h.paint(), hovered_secondary),
        "only a link shows the hover look"
    );

    let mut link = Harness::new(
        Badge::new("Open")
            .with_variant(BadgeVariant::Secondary)
            .as_link(),
    );
    assert_eq!(link.semantics().role, SemanticRole::Link);
    let center = link.bounds().center();
    link.hover(center);
    // The hover color eases in over the transition duration.
    link.paint();
    link.set_time(1.0);
    assert!(fills(&link.paint(), hovered_secondary));
    link.click(center);
    link.focus();
    // A link takes Enter only; Space is for buttons.
    link.key(Key::Space);
    assert_eq!(link.messages(), [ControlMessage::ItemActivated(0)]);
    link.key(Key::Enter);
    assert_eq!(link.messages(), [ControlMessage::ItemActivated(0)]);
}

#[test]
fn avatar_shows_the_image_or_the_fallback_and_a_badge() {
    let tokens = DesignTokens::light();
    for (size, side) in [
        (ControlSize::Sm, 24.0),
        (ControlSize::Default, 32.0),
        (ControlSize::Lg, 40.0),
    ] {
        let mut h = Harness::new(Avatar::new("Ana Lee", "AL").with_size(size));
        assert_eq!(h.measure().width, side);
    }
    let h = Harness::new(Avatar::new("Ana Lee", "AL").with_badge(None));
    assert_eq!(h.semantics().role, SemanticRole::Image);
    assert_eq!(h.semantics().name, "Ana Lee");
    let list = h.paint();
    assert_eq!(texts(&list), ["AL"]);
    let badges = rounded_fills(&list, tokens.color(ColorRole::Primary));
    assert_eq!(
        badges,
        [hosted::rect(32.0, 32.0, 10.0, 10.0)],
        "the badge sits at the bottom end"
    );
    let rtl = Harness::with_direction(
        Avatar::new("Ana Lee", "AL").with_badge(None),
        LayoutDirection::Rtl,
    );
    assert_eq!(
        rounded_fills(&rtl.paint(), tokens.color(ColorRole::Primary)),
        [hosted::rect(10.0, 32.0, 10.0, 10.0)]
    );

    let image = Image::from_rgba8(1, 1, vec![255, 0, 0, 255]).unwrap();
    let pictured = Harness::new(Avatar::new("Ana Lee", "AL").with_image(image));
    let list = pictured.paint();
    assert!(texts(&list).is_empty(), "the image replaces the fallback");
    assert!(list.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::DrawImage { rect, .. } if *rect == hosted::rect(10.0, 10.0, 32.0, 32.0)
    )));
}

#[test]
fn avatar_groups_overlap_and_end_with_a_count() {
    let people = || {
        vec![
            Avatar::new("Ana Lee", "AL"),
            Avatar::new("Bao Tran", "BT"),
            Avatar::new("Chi Vo", "CV"),
        ]
    };
    let mut h = Harness::new(AvatarGroup::new("Team", people()).with_count("+3"));
    assert_eq!(h.measure().width, 4.0 * 32.0 - 3.0 * 8.0);
    assert_eq!(texts(&h.paint()), ["AL", "BT", "CV", "+3"]);
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("Ana Lee, Bao Tran, Chi Vo, +3")
    );
    let mut small = Harness::new(AvatarGroup::new("Team", people()).with_size(ControlSize::Sm));
    assert_eq!(small.measure().width, 3.0 * 24.0 - 2.0 * 8.0);
}
