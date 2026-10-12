//! Layout components mounted through `ControlHost`: aspect-ratio boxes,
//! collapsible sections, accordions, and item rows.

use crate::page::{Env, Page};
use crate::pages::display::portrait;
use crate::pages::hosted::{HostedPage, Row};
use choscorui::controls::{
    Accordion, AspectRatio, Avatar, ButtonVariant, Collapsible, ControlSize, CustomControl, Item,
    ItemGroup, ItemMedia, ItemVariant,
};
use choscorui::core::{Icon, NodeId, Tree};

fn boxed(control: impl CustomControl + 'static) -> Box<dyn CustomControl> {
    Box::new(control)
}

pub fn aspect_ratio(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "16 by 9",
            vec![boxed(
                AspectRatio::new("Landscape photo", 16.0 / 9.0).with_image(portrait()),
            )],
        ),
        (
            "Square placeholder",
            vec![boxed(AspectRatio::new("Placeholder", 1.0))],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn collapsible(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let items = || {
        vec![
            "@radix-ui/colors".to_string(),
            "@stitches/react".to_string(),
        ]
    };
    let rows: Vec<Row> = vec![
        (
            "Closed",
            vec![boxed(Collapsible::new(
                "@peduarte starred 3 repositories",
                "Toggle repositories",
                "@radix-ui/primitives",
                items(),
            ))],
        ),
        (
            "Open",
            vec![boxed(
                Collapsible::new(
                    "Order #4189 details",
                    "Toggle details",
                    "Shipped on March 3",
                    vec!["Tracking number 1Z999".to_string()],
                )
                .open(),
            )],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn accordion(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let faq = || {
        [
            (
                "Product information",
                "Our flagship product combines new technology with a sleek design.",
            ),
            (
                "Shipping details",
                "We offer worldwide shipping through trusted courier partners.",
            ),
            (
                "Return policy",
                "We stand behind our products with a 30-day return policy.",
            ),
        ]
        .into_iter()
        .map(|(title, text)| (title.to_string(), text.to_string()))
        .collect::<Vec<_>>()
    };
    let rows: Vec<Row> = vec![
        (
            "Single",
            vec![boxed(Accordion::new("Product FAQ", faq()).with_open(0))],
        ),
        (
            "Multiple",
            vec![boxed(Accordion::new("Store FAQ", faq()).multiple())],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn item(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "Variants",
            vec![boxed(ItemGroup::new(
                "Item variants",
                vec![
                    Item::new("Default variant")
                        .with_description("Transparent background with no border.")
                        .with_action("Open", ButtonVariant::Outline),
                    Item::new("Outline variant")
                        .with_description("Outlined style with a clear border.")
                        .with_variant(ItemVariant::Outline)
                        .with_action("Open", ButtonVariant::Outline),
                    Item::new("Muted variant")
                        .with_description("Muted background for secondary content.")
                        .with_variant(ItemVariant::Muted)
                        .with_action("Open", ButtonVariant::Outline),
                ],
            ))],
        ),
        (
            "Media and links",
            vec![boxed(
                ItemGroup::new(
                    "Item media",
                    vec![
                        Item::new("Your profile has been verified.")
                            .with_media(ItemMedia::Icon(Icon::CircleCheck))
                            .with_variant(ItemVariant::Outline)
                            .with_size(ControlSize::Sm)
                            .as_link(),
                        Item::new("Midnight City")
                            .with_description("Silent Echo")
                            .with_media(ItemMedia::Image(portrait())),
                        Item::new("Ana Lee")
                            .with_description("Last seen 5 months ago")
                            .with_media(ItemMedia::Avatar(Avatar::new("Ana Lee", "AL")))
                            .with_action("Invite", ButtonVariant::Default),
                    ],
                )
                .with_separators(),
            )],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}
