//! Data display components mounted through `ControlHost`: cards,
//! badges, and avatars.

use crate::page::{Env, Page};
use crate::pages::hosted::{HostedPage, Row};
use choscorui::controls::{
    Avatar, AvatarGroup, Badge, BadgeVariant, ButtonVariant, Card, ControlSize, CustomControl,
};
use choscorui::core::{Icon, Image, NodeId, Tree};

fn boxed(control: impl CustomControl + 'static) -> Box<dyn CustomControl> {
    Box::new(control)
}

pub fn card(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "Header action and footer",
            vec![boxed(
                Card::new(
                    "Login to your account",
                    "Enter your email below to login to your account.",
                )
                .with_action("Sign Up", ButtonVariant::Link)
                .with_content("Use the email address that your team invited.")
                .with_footer("Login with Google", ButtonVariant::Outline)
                .with_footer("Login", ButtonVariant::Default),
            )],
        ),
        (
            "Small",
            vec![boxed(
                Card::new("Scheduled reports", "Weekly summary of your workspace.")
                    .with_content("The next report goes out on Monday at 9:00.")
                    .with_footer("Manage", ButtonVariant::Outline)
                    .with_size(ControlSize::Sm),
            )],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn badge(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let variants = [
        ("Default", BadgeVariant::Default),
        ("Secondary", BadgeVariant::Secondary),
        ("Destructive", BadgeVariant::Destructive),
        ("Outline", BadgeVariant::Outline),
        ("Ghost", BadgeVariant::Ghost),
        ("Link", BadgeVariant::Link),
    ];
    let rows: Vec<Row> = vec![
        (
            "Variants",
            variants
                .iter()
                .map(|(label, variant)| boxed(Badge::new(*label).with_variant(*variant)))
                .collect(),
        ),
        (
            "With icons",
            vec![
                boxed(
                    Badge::new("Verified")
                        .with_variant(BadgeVariant::Secondary)
                        .with_icon(Icon::Check),
                ),
                boxed(
                    Badge::new("Failed")
                        .with_variant(BadgeVariant::Destructive)
                        .with_icon(Icon::OctagonX),
                ),
                boxed(
                    Badge::new("Syncing")
                        .with_variant(BadgeVariant::Outline)
                        .with_icon(Icon::LoaderCircle),
                ),
            ],
        ),
        (
            "Links",
            vec![
                boxed(Badge::new("Open issue").as_link()),
                boxed(
                    Badge::new("Changelog")
                        .with_variant(BadgeVariant::Outline)
                        .as_link(),
                ),
            ],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

/// A small diagonal gradient that stands in for a photo.
fn portrait() -> Image {
    let side = 32u32;
    let pixels = (0..side * side)
        .flat_map(|index| {
            let (x, y) = (index % side, index / side);
            let shade = ((x + y) * 255 / (2 * side - 2)) as u8;
            [shade, 96, 255 - shade, 255]
        })
        .collect();
    Image::from_rgba8(side, side, pixels).expect("the pixel count matches the size")
}

pub fn avatar(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let people = |size: ControlSize| {
        vec![
            Avatar::new("Ana Lee", "AL").with_image(portrait()),
            Avatar::new("Bao Tran", "BT"),
            Avatar::new("Chi Vo", "CV"),
        ]
        .into_iter()
        .map(|avatar| avatar.with_size(size))
        .collect::<Vec<_>>()
    };
    let rows: Vec<Row> = vec![
        (
            "Image and fallback",
            vec![
                boxed(Avatar::new("Ana Lee", "AL").with_image(portrait())),
                boxed(Avatar::new("Bao Tran", "BT")),
            ],
        ),
        (
            "Sizes",
            [ControlSize::Sm, ControlSize::Default, ControlSize::Lg]
                .into_iter()
                .map(|size| boxed(Avatar::new("Chi Vo", "CV").with_size(size)))
                .collect(),
        ),
        (
            "Badges",
            vec![
                boxed(Avatar::new("Ana Lee", "AL").with_badge(None)),
                boxed(
                    Avatar::new("Bao Tran", "BT")
                        .with_size(ControlSize::Lg)
                        .with_badge(Some(Icon::Plus)),
                ),
            ],
        ),
        (
            "Groups",
            vec![
                boxed(
                    AvatarGroup::new("Design team", people(ControlSize::Default)).with_count("+3"),
                ),
                boxed(
                    AvatarGroup::new("Small team", people(ControlSize::Sm))
                        .with_size(ControlSize::Sm),
                ),
            ],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

#[cfg(test)]
mod tests {
    use crate::pages::tests::Harness;
    use choscorui::core::{Key, SemanticAction, SemanticRole};

    #[test]
    fn card_page_buttons_take_keys() {
        let mut h = Harness::new("card");
        let node = h.node("Login to your account", Some(SemanticRole::Group));
        assert!(h.act(node, SemanticAction::Focus));
        assert!(h.press(Key::Tab));
        assert!(h.press(Key::Enter));
    }

    #[test]
    fn avatar_page_names_the_people() {
        let h = Harness::new("avatar");
        h.node("Ana Lee", Some(SemanticRole::Image));
        h.node("Design team", Some(SemanticRole::Group));
    }
}
