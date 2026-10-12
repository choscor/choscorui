//! Layout components: aspect-ratio boxes, collapsible sections,
//! accordions, and item rows.

use crate::ui::page::{Build, PageCx, column, portrait, section};
use choscorui::compose::t;
use choscorui::compose::view::{
    Accordion, AnyView, AspectRatio, Collapsible, Configured, Cx, Env, Frame, View, ViewExt,
};
use choscorui::controls::{
    self, ButtonVariant, ControlKind, ControlSize, Item, ItemGroup, ItemMedia, ItemVariant,
};
use choscorui::core::Icon;

pub fn aspect_ratio(_: &PageCx) -> AnyView {
    column((
        section(
            t!("aspect-ratio.sixteen_by_nine"),
            AspectRatio::new(t!("aspect-ratio.landscape"), 16.0 / 9.0)
                .with(|ratio| ratio.with_image(portrait()))
                .frame(Frame::new().width(320.0)),
        ),
        section(
            t!("aspect-ratio.square"),
            AspectRatio::new(t!("aspect-ratio.placeholder"), 1.0).frame(Frame::new().width(160.0)),
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
                t!("collapsible.closed"),
                Collapsible::new(
                    t!("collapsible.starred", user = "@peduarte", count = 3),
                    t!("collapsible.toggle_repositories"),
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
                t!("collapsible.open"),
                Collapsible::new(
                    t!("collapsible.order", number = 4189),
                    t!("collapsible.toggle_details"),
                    t!("collapsible.shipped"),
                    |env: &Env| vec![env.text("collapsible.tracking", &[("code", "1Z999")])],
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

fn faq(env: &Env) -> Vec<(String, String)> {
    [
        ("accordion.product", "accordion.product_text"),
        ("accordion.shipping", "accordion.shipping_text"),
        ("accordion.returns", "accordion.returns_text"),
    ]
    .into_iter()
    .map(|(title, text)| (env.text(title, &[]), env.text(text, &[])))
    .collect()
}

/// A single accordion with its first section open, and a multiple one.
struct AccordionPage;

impl View for AccordionPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let (single, multiple) = (cx.state(vec![0]), cx.state(Vec::new()));
        column((
            section(
                t!("accordion.single"),
                Accordion::new(t!("accordion.product_faq"), faq, single.binding())
                    .frame(Frame::new().width(400.0)),
            ),
            section(
                t!("accordion.multiple"),
                Accordion::new(t!("accordion.store_faq"), faq, multiple.binding())
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
    let group = |build: fn(&Env) -> ItemGroup| {
        Configured::custom(ControlKind::Item, build).frame(Frame::new().width(420.0))
    };
    column((
        section(
            t!("item.variants"),
            group(|env| {
                let text = |key| env.text(key, &[]);
                ItemGroup::new(
                    text("item.variants_name"),
                    vec![
                        Item::new(text("item.default"))
                            .with_description(text("item.default_text"))
                            .with_action(text("item.open"), ButtonVariant::Outline),
                        Item::new(text("item.outline"))
                            .with_description(text("item.outline_text"))
                            .with_variant(ItemVariant::Outline)
                            .with_action(text("item.open"), ButtonVariant::Outline),
                        Item::new(text("item.muted"))
                            .with_description(text("item.muted_text"))
                            .with_variant(ItemVariant::Muted)
                            .with_action(text("item.open"), ButtonVariant::Outline),
                    ],
                )
            }),
        ),
        section(
            t!("item.media_links"),
            group(|env| {
                let text = |key| env.text(key, &[]);
                ItemGroup::new(
                    text("item.media_name"),
                    vec![
                        Item::new(text("item.verified"))
                            .with_media(ItemMedia::Icon(Icon::CircleCheck))
                            .with_variant(ItemVariant::Outline)
                            .with_size(ControlSize::Sm)
                            .as_link(),
                        // A song and its artist stand for user data.
                        Item::new("Midnight City")
                            .with_description("Silent Echo")
                            .with_media(ItemMedia::Image(portrait())),
                        Item::new("Ana Lee")
                            .with_description(text("item.last_seen"))
                            .with_media(ItemMedia::Avatar(controls::Avatar::new("Ana Lee", "AL")))
                            .with_action(text("item.invite"), ButtonVariant::Default),
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
