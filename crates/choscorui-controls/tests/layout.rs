//! AspectRatio, Collapsible, Accordion, and Item through the control host.

mod hosted;

use choscorui_controls::{
    Accordion, AspectRatio, ButtonVariant, Collapsible, ControlMessage, ControlValue, Item,
    ItemGroup, ItemMedia, ItemVariant,
};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, DisplayList, Icon, Image, Key, KeyEvent,
    LayoutDirection, Point, PointerButton, PointerEvent, SemanticAction, SemanticRole,
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

fn images(list: &DisplayList) -> Vec<choscorui_core::Rect> {
    list.commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawImage { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect()
}

fn faq() -> Vec<(String, String)> {
    [
        (
            "Is it accessible?",
            "Yes. It follows the disclosure pattern.",
        ),
        ("Is it styled?", "Yes. It takes its look from the theme."),
        ("Is it animated?", "Yes. It respects reduced motion."),
    ]
    .into_iter()
    .map(|(title, text)| (title.to_string(), text.to_string()))
    .collect()
}

#[test]
fn aspect_ratio_follows_its_width_and_covers_with_the_image() {
    let image = Image::from_rgba8(4, 2, vec![255; 32]).unwrap();
    let mut h = Harness::new(AspectRatio::new("Photo", 16.0 / 9.0).with_image(image));
    let size = h.measure();
    assert_eq!(size.width, 448.0);
    assert_eq!(size.height, 252.0);
    let list = h.paint();
    let drawn = images(&list);
    assert_eq!(drawn.len(), 1);
    // A 2:1 image covers a 16:9 box: it fills the height and overflows the width.
    assert_eq!(drawn[0].size.height, 252.0);
    assert!(drawn[0].size.width > 448.0);
    assert!(paints(&list, DesignTokens::light().color(ColorRole::Muted)));
    assert_eq!(h.semantics().role, SemanticRole::Image);
    assert!(!h.host.get(h.node).unwrap().focusable());
    let mut square = Harness::new(AspectRatio::new("Square", -1.0));
    assert_eq!(square.measure().height, 448.0);
}

#[test]
fn collapsible_shows_items_when_open() {
    let items = vec![
        "@radix-ui/colors".to_string(),
        "@stitches/react".to_string(),
    ];
    let mut h = Harness::new(Collapsible::new(
        "@peduarte starred 3 repositories",
        "Toggle",
        "@radix-ui/primitives",
        items,
    ));
    let closed = h.measure().height;
    assert_eq!(
        texts(&h.paint()),
        ["@peduarte starred 3 repositories", "@radix-ui/primitives"]
    );
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::Group);
    assert_eq!(node.state.expanded, Some(false));
    h.focus();
    h.key(Key::Enter);
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(true)]);
    assert!(h.measure().height > closed);
    let bounds = h.bounds();
    let height = h.measure().height;
    h.place(choscorui_core::Rect::from_xywh(
        bounds.origin.x,
        bounds.origin.y,
        bounds.size.width,
        height,
    ));
    h.set_time(1.0);
    assert_eq!(texts(&h.paint()).len(), 4);
    assert!(h.perform(SemanticAction::Collapse, None));
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(false)]);
}

#[test]
fn collapsible_toggle_sits_at_the_end_and_mirrors() {
    let make = || Collapsible::new("Repositories", "Toggle", "first", vec!["second".into()]);
    let mut h = Harness::new(make());
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 20.0, bounds.origin.y + 10.0));
    assert!(h.messages().is_empty(), "the title is not the toggle");
    h.click(Point::new(bounds.max_x() - 10.0, bounds.origin.y + 10.0));
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(true)]);
    let mut rtl = Harness::with_direction(make(), LayoutDirection::Rtl);
    let bounds = rtl.bounds();
    rtl.click(Point::new(bounds.origin.x + 10.0, bounds.origin.y + 10.0));
    assert_eq!(rtl.messages(), [ControlMessage::OpenChanged(true)]);
}

#[test]
fn accordion_keeps_one_item_open() {
    let mut h = Harness::new(Accordion::new("FAQ", faq()).with_open(0));
    assert_eq!(h.semantics().value.as_deref(), Some("Is it accessible?"));
    h.focus();
    assert_eq!(h.semantics().description, "Is it accessible?");
    h.key(Key::ArrowDown);
    assert_eq!(h.semantics().description, "Is it styled?");
    h.key(Key::Space);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![1]))]
    );
    h.key(Key::End);
    assert_eq!(h.semantics().description, "Is it animated?");
    assert_eq!(h.semantics().state.expanded, Some(false));
    assert!(h.perform(SemanticAction::Expand, None));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![2]))]
    );
    h.key(Key::Home);
    assert_eq!(h.semantics().description, "Is it accessible?");
}

#[test]
fn accordion_headers_are_tab_stops_and_multiple_items_open() {
    let mut h = Harness::new(Accordion::new("FAQ", faq()).multiple());
    let closed = h.measure().height;
    h.focus();
    h.key(Key::Enter);
    assert!(tab(&mut h));
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Changed(ControlValue::Indices(vec![0])),
            ControlMessage::Changed(ControlValue::Indices(vec![0, 1])),
        ]
    );
    // The height grows over the open animation.
    assert!(h.host.advance(&mut h.tree, 1.0));
    assert!(h.measure().height > closed);
    assert!(tab(&mut h));
    assert!(!tab(&mut h), "Tab leaves after the last header");
}

#[test]
fn accordion_click_toggles_and_chevron_mirrors() {
    let mut h = Harness::new(Accordion::new("FAQ", faq()));
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 20.0, bounds.origin.y + 10.0));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![0]))]
    );
    h.click(Point::new(bounds.origin.x + 20.0, bounds.origin.y + 10.0));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![]))]
    );
    let rtl = Harness::with_direction(Accordion::new("FAQ", faq()), LayoutDirection::Rtl);
    assert_eq!(texts(&rtl.paint()).len(), 3);
}

#[test]
fn accordion_height_animates_and_the_last_item_has_no_border() {
    let one = faq().into_iter().take(1).collect();
    let header = Harness::new(Accordion::new("FAQ", one)).measure().height;
    let mut h = Harness::new(Accordion::new("FAQ", faq()));
    let closed = h.measure().height;
    assert_eq!(
        closed,
        header * 3.0 + 2.0,
        "two borders between three items"
    );
    h.focus();
    h.key(Key::Enter);
    assert_eq!(h.measure().height, closed, "the height starts closed");
    let frame = h.host.next_deadline(&h.tree).expect("a frame timer");
    assert!(h.host.advance(&mut h.tree, frame + 0.05));
    let middle = h.measure().height;
    assert!(middle > closed, "{middle} > {closed}");
    while let Some(frame) = h.host.next_deadline(&h.tree) {
        h.host.advance(&mut h.tree, frame);
    }
    let open = h.measure().height;
    assert!(open > middle, "{open} > {middle}");
}

#[test]
fn a_disabled_accordion_item_is_skipped_and_only_primary_releases_toggle() {
    let mut h = Harness::new(Accordion::new("FAQ", faq()).with_disabled(1));
    h.focus();
    h.key(Key::ArrowDown);
    assert_eq!(h.semantics().description, "Is it animated?");
    let bounds = h.bounds();
    let header = bounds.size.height / 3.0;
    h.click(Point::new(
        bounds.origin.x + 20.0,
        bounds.origin.y + header * 1.5,
    ));
    assert!(h.messages().is_empty(), "the disabled item does not toggle");
    let first = Point::new(bounds.origin.x + 20.0, bounds.origin.y + 10.0);
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Down {
            position: first,
            button: PointerButton::Primary,
        },
    );
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Up {
            position: first,
            button: PointerButton::Secondary,
        },
    );
    assert!(
        h.messages().is_empty(),
        "a secondary release does not toggle"
    );
}

#[test]
fn collapsible_items_fade_out_before_they_leave() {
    let mut h = Harness::new(
        Collapsible::new("Repositories", "Toggle", "first", vec!["second".into()]).open(),
    );
    let open = h.measure().height;
    h.focus();
    h.key(Key::Enter);
    assert_eq!(h.measure().height, open, "the item stays while it fades");
    let end = h.host.next_deadline(&h.tree).expect("the fade end");
    assert!(h.host.advance(&mut h.tree, end));
    assert!(h.measure().height < open);
}

#[test]
fn item_group_tabs_through_links_and_actions() {
    let image = Image::from_rgba8(1, 1, vec![255, 0, 0, 255]).unwrap();
    let mut h = Harness::new(ItemGroup::new(
        "Settings",
        vec![
            Item::new("Basic item")
                .with_description("A simple item with a title and a description.")
                .with_variant(ItemVariant::Outline)
                .with_action("Action", ButtonVariant::Outline),
            Item::new("Your profile has been verified.")
                .with_media(ItemMedia::Icon(Icon::CircleCheck))
                .as_link(),
            Item::new("Midnight City")
                .with_description("Silent Echo")
                .with_media(ItemMedia::Image(image)),
        ],
    ));
    assert_eq!(h.measure().width, 480.0);
    let list = h.paint();
    let painted = texts(&list);
    for text in ["Basic item", "Action", "Midnight City", "Silent Echo"] {
        assert!(painted.contains(&text.to_string()), "{text}");
    }
    assert_eq!(images(&list).len(), 1);
    assert!(paints(
        &list,
        DesignTokens::light().color(ColorRole::Border)
    ));
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::List);
    h.focus();
    assert_eq!(h.semantics().description, "Action");
    assert!(tab(&mut h));
    assert_eq!(h.semantics().description, "Your profile has been verified.");
    h.key(Key::Enter);
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(1)]);
    assert!(!tab(&mut h));
}

#[test]
fn item_link_rows_click_and_mirror() {
    let make = || {
        ItemGroup::new(
            "Links",
            vec![
                Item::new("Account").as_link(),
                Item::new("Billing").as_link(),
            ],
        )
        .with_separators()
    };
    let mut h = Harness::new(make());
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 20.0, bounds.max_y() - 10.0));
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(1)]);
    let rtl = Harness::with_direction(make(), LayoutDirection::Rtl);
    assert_eq!(texts(&rtl.paint()), ["Account", "Billing"]);
    let plain = Harness::new(ItemGroup::new("Plain", vec![Item::new("Static")]));
    assert!(!plain.host.get(plain.node).unwrap().focusable());
}

/// The baseline of each text run of `list`, by text.
fn baselines(list: &DisplayList) -> Vec<(String, Point)> {
    list.commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText { text, baseline, .. } => Some((text.clone(), *baseline)),
            _ => None,
        })
        .collect()
}

#[test]
fn item_header_and_footer_lines_frame_the_row() {
    let row = |item: Item| Harness::new(ItemGroup::new("Plans", vec![item]));
    let plain = row(Item::new("Pro plan").with_variant(ItemVariant::Outline));
    let framed = row(Item::new("Pro plan")
        .with_header("Most popular")
        .with_footer("Billed yearly")
        .with_variant(ItemVariant::Outline));
    // Each line adds its text height and one 14 pt row gap.
    let (short, tall) = (plain.bounds().size.height, framed.bounds().size.height);
    assert!(tall > short + 2.0 * 14.0, "{short} {tall}");
    let lines = baselines(&framed.paint());
    let names: Vec<&str> = lines.iter().map(|(text, _)| text.as_str()).collect();
    assert_eq!(names, ["Most popular", "Billed yearly", "Pro plan"]);
    let (header, footer, title) = (lines[0].1.y, lines[1].1.y, lines[2].1.y);
    assert!(header < title && title < footer);
    // Assistive technology reads the header and the footer with the title.
    assert_eq!(
        framed.semantics().value.as_deref(),
        Some("Most popular, Pro plan, Billed yearly")
    );
    let rtl = Harness::with_direction(
        ItemGroup::new("Plans", vec![Item::new("Pro").with_header("New")]),
        LayoutDirection::Rtl,
    );
    let header = baselines(&rtl.paint())
        .into_iter()
        .find(|(text, _)| text == "New")
        .unwrap();
    assert!(header.1.x > rtl.bounds().center().x, "the header mirrors");
}
