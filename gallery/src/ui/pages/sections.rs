//! Layout components: aspect-ratio boxes, collapsible sections,
//! accordions, and item rows.

use crate::ui::page::{Build, PageCx, column, portrait, section};
use choscorui::compose::view::{
    Accordion, AnyView, AspectRatio, Collapsible, Configured, Cx, Frame, View, ViewExt,
};
use choscorui::controls::{
    self, ButtonVariant, ControlKind, ControlSize, Item, ItemGroup, ItemMedia, ItemVariant,
};
use choscorui::core::Icon;

pub fn aspect_ratio(_: &PageCx) -> AnyView {
    column((
        section(
            "16 by 9",
            AspectRatio::new("Landscape photo", 16.0 / 9.0)
                .with(|ratio| ratio.with_image(portrait()))
                .frame(Frame::new().width(320.0)),
        ),
        section(
            "Square placeholder",
            AspectRatio::new("Placeholder", 1.0).frame(Frame::new().width(160.0)),
        ),
    ))
}

/// One closed and one open collapsible section.
struct CollapsiblePage;

impl View for CollapsiblePage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let (closed, open) = (cx.state(false), cx.state(true));
        column((
            section(
                "Closed",
                Collapsible::new(
                    "@peduarte starred 3 repositories",
                    "Toggle repositories",
                    "@radix-ui/primitives",
                    vec![
                        "@radix-ui/colors".to_string(),
                        "@stitches/react".to_string(),
                    ],
                    closed.binding(),
                )
                .frame(Frame::new().width(320.0)),
            ),
            section(
                "Open",
                Collapsible::new(
                    "Order #4189 details",
                    "Toggle details",
                    "Shipped on March 3",
                    vec!["Tracking number 1Z999".to_string()],
                    open.binding(),
                )
                .frame(Frame::new().width(320.0)),
            ),
        ))
    }
}

pub fn collapsible(_: &PageCx) -> AnyView {
    CollapsiblePage.into_any()
}

fn faq() -> Vec<(String, String)> {
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
    .collect()
}

/// A single accordion with its first section open, and a multiple one.
struct AccordionPage;

impl View for AccordionPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let (single, multiple) = (cx.state(vec![0]), cx.state(Vec::new()));
        column((
            section(
                "Single",
                Accordion::new("Product FAQ", faq(), single.binding())
                    .frame(Frame::new().width(400.0)),
            ),
            section(
                "Multiple",
                Accordion::new("Store FAQ", faq(), multiple.binding())
                    .with(|accordion| accordion.multiple())
                    .frame(Frame::new().width(400.0)),
            ),
        ))
    }
}

pub fn accordion(_: &PageCx) -> AnyView {
    AccordionPage.into_any()
}

pub fn item(_: &PageCx) -> AnyView {
    let group = |build: fn() -> ItemGroup| {
        Configured::custom(ControlKind::Item, move |_| build()).frame(Frame::new().width(420.0))
    };
    column((
        section(
            "Variants",
            group(|| {
                ItemGroup::new(
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
                )
            }),
        ),
        section(
            "Media and links",
            group(|| {
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
                            .with_media(ItemMedia::Avatar(controls::Avatar::new("Ana Lee", "AL")))
                            .with_action("Invite", ButtonVariant::Default),
                    ],
                )
                .with_separators()
            }),
        ),
    ))
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "aspect-ratio" => aspect_ratio,
        "collapsible" => collapsible,
        "accordion" => accordion,
        "item" => item,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::core::SemanticAction;

    fn actions(page: &mut Harness, name: &str) -> Vec<SemanticAction> {
        let node = page.node(name);
        page.host.semantic(node).unwrap().actions
    }

    #[test]
    fn collapsible_sections_open_and_close() {
        let mut page = Harness::new("collapsible");
        let closed = "@peduarte starred 3 repositories";
        assert!(actions(&mut page, closed).contains(&SemanticAction::Expand));
        assert!(page.act(closed, SemanticAction::Expand));
        assert!(actions(&mut page, closed).contains(&SemanticAction::Collapse));
        let open = "Order #4189 details";
        assert!(page.act(open, SemanticAction::Collapse));
        assert!(actions(&mut page, open).contains(&SemanticAction::Expand));
    }

    #[test]
    fn accordions_open_their_sections() {
        let mut page = Harness::new("accordion");
        assert_eq!(
            page.value("Product FAQ").as_deref(),
            Some("Product information")
        );
        assert_eq!(page.value("Store FAQ"), None);
        assert!(page.act("Store FAQ", SemanticAction::Expand));
        assert!(page.value("Store FAQ").is_some());
    }
}
