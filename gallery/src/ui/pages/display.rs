//! Data display components: cards, badges, avatars, carousels, the data
//! table, and charts.

use crate::ui::page::{Build, PageCx, column, section};
use crate::ui::pages::form::{row, stateful};
use choscorui::compose::view::{
    AnyView, Avatar, Badge, Card, Carousel, Chart, Configured, Cx, DataTable, View, ViewExt,
};
use choscorui::controls::{
    self, AvatarGroup, BadgeVariant, ButtonVariant, ChartKind, ChartSeries, ControlKind,
    ControlSize, Orientation,
};
use choscorui::core::{Icon, Image, ItemData, ItemModel, ListStore, Role, Value};

pub fn card(_: &PageCx) -> AnyView {
    column((
        section(
            "Header action and footer",
            Card::new(
                "Login to your account",
                "Enter your email below to login to your account.",
            )
            .with(|card| {
                card.with_action("Sign Up", ButtonVariant::Link)
                    .with_content("Use the email address that your team invited.")
                    .with_footer("Login with Google", ButtonVariant::Outline)
                    .with_footer("Login", ButtonVariant::Default)
            }),
        ),
        section(
            "Small",
            Card::new("Scheduled reports", "Weekly summary of your workspace.").with(|card| {
                card.with_content("The next report goes out on Monday at 9:00.")
                    .with_footer("Manage", ButtonVariant::Outline)
                    .with_size(ControlSize::Sm)
            }),
        ),
    ))
}

pub fn badge(_: &PageCx) -> AnyView {
    let variants = [
        ("Default", BadgeVariant::Default),
        ("Secondary", BadgeVariant::Secondary),
        ("Destructive", BadgeVariant::Destructive),
        ("Outline", BadgeVariant::Outline),
        ("Ghost", BadgeVariant::Ghost),
        ("Link", BadgeVariant::Link),
    ];
    let badge = |label: &'static str, variant: BadgeVariant| {
        Badge::new(label).with(move |badge| badge.with_variant(variant))
    };
    let iconic = |label: &'static str, variant: BadgeVariant, icon: Icon| {
        badge(label, variant).with(move |badge| badge.with_icon(icon))
    };
    column((
        section(
            "Variants",
            row(variants
                .into_iter()
                .map(|(label, variant)| badge(label, variant))
                .collect::<Vec<_>>()),
        ),
        section(
            "With icons",
            row((
                iconic("Verified", BadgeVariant::Secondary, Icon::Check),
                iconic("Failed", BadgeVariant::Destructive, Icon::OctagonX),
                iconic("Syncing", BadgeVariant::Outline, Icon::LoaderCircle),
            )),
        ),
        section(
            "Links",
            row((
                Badge::new("Open issue").with(|badge| badge.as_link()),
                badge("Changelog", BadgeVariant::Outline).with(|badge| badge.as_link()),
            )),
        ),
    ))
}

/// A small diagonal gradient that stands in for a photo.
pub(crate) fn portrait() -> Image {
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

/// A group of three people at `size`.
fn team(name: &'static str, size: ControlSize, count: Option<&'static str>) -> impl View {
    Configured::custom(ControlKind::Avatar, move |_| {
        let people = vec![
            controls::Avatar::new("Ana Lee", "AL").with_image(portrait()),
            controls::Avatar::new("Bao Tran", "BT"),
            controls::Avatar::new("Chi Vo", "CV"),
        ]
        .into_iter()
        .map(|avatar| avatar.with_size(size))
        .collect();
        let group = AvatarGroup::new(name, people).with_size(size);
        match count {
            Some(count) => group.with_count(count),
            None => group,
        }
    })
}

pub fn avatar(_: &PageCx) -> AnyView {
    let ana = || Avatar::new("Ana Lee", "AL").with(|avatar| avatar.with_image(portrait()));
    column((
        section(
            "Image and fallback",
            row((ana(), Avatar::new("Bao Tran", "BT"))),
        ),
        section(
            "Sizes",
            row([ControlSize::Sm, ControlSize::Default, ControlSize::Lg]
                .into_iter()
                .map(|size| Avatar::new("Chi Vo", "CV").with(move |avatar| avatar.with_size(size)))
                .collect::<Vec<_>>()),
        ),
        section(
            "Badges",
            row((
                Avatar::new("Ana Lee", "AL").with(|avatar| avatar.with_badge(None)),
                Avatar::new("Bao Tran", "BT").with(|avatar| {
                    avatar
                        .with_size(ControlSize::Lg)
                        .with_badge(Some(Icon::Plus))
                }),
            )),
        ),
        section(
            "Groups",
            row((
                team("Design team", ControlSize::Default, Some("+3")),
                team("Small team", ControlSize::Sm, None),
            )),
        ),
    ))
}

fn numbers(count: usize) -> Vec<String> {
    (1..=count).map(|n| n.to_string()).collect()
}

fn carousel_of(
    cx: &mut Cx<'_>,
    name: &'static str,
    count: usize,
) -> Configured<controls::Carousel> {
    Carousel::new(
        name,
        "Previous slide",
        "Next slide",
        numbers(count),
        cx.state(0).binding(),
    )
}

pub fn carousel(_: &PageCx) -> AnyView {
    stateful(|cx| {
        column((
            section("Horizontal", carousel_of(cx, "Slides", 5)),
            section(
                "Three per view",
                carousel_of(cx, "Thumbnails", 8).with(|carousel| carousel.with_per_view(3)),
            ),
            section(
                "Vertical",
                carousel_of(cx, "Stacked slides", 5).with(|carousel| {
                    carousel
                        .with_orientation(Orientation::Vertical)
                        .with_per_view(2)
                }),
            ),
        ))
    })
}

/// One payment row: status, email, and amount.
struct Payment(&'static str, &'static str, f64);

impl ItemData for Payment {
    fn data(&self, column: usize, role: Role) -> Value {
        match (column, role) {
            (0, Role::Display | Role::Sort) => Value::Text(self.0.into()),
            (1, Role::Display | Role::Sort) => Value::Text(self.1.into()),
            (2, Role::Display) => Value::Text(format!("${:.2}", self.2)),
            (2, Role::Sort) => Value::Float(self.2),
            _ => Value::None,
        }
    }
}

fn payments() -> Box<dyn ItemModel> {
    let mut store = ListStore::new(3);
    store.set_headers(["Status", "Email", "Amount"]);
    let rows = vec![
        Payment("success", "ken99@example.com", 316.0),
        Payment("success", "abe45@example.com", 242.0),
        Payment("processing", "monserrat44@example.com", 837.0),
        Payment("success", "silas22@example.com", 874.0),
        Payment("failed", "carmella@example.com", 721.0),
        Payment("pending", "jason78@example.com", 450.0),
        Payment("success", "sarah23@example.com", 1280.0),
    ];
    store.insert(None, 0, rows).expect("root insert is valid");
    Box::new(store)
}

pub fn data_table(_: &PageCx) -> AnyView {
    stateful(|cx| {
        column(section(
            "Sort, filter, columns, selection, and pages",
            DataTable::new(
                "Payments",
                payments,
                "Previous",
                "Next",
                cx.state(Vec::new()).binding(),
            )
            .with(|table| {
                table
                    .with_filter("Filter emails...", Some(1))
                    .with_column_menu("Columns")
                    .with_empty("No results.")
                    .with_status(|selected, total| {
                        format!("{selected} of {total} row(s) selected.")
                    })
                    .with_page_size(5)
            }),
        ))
    })
}

fn months() -> Vec<String> {
    ["January", "February", "March", "April", "May", "June"]
        .map(String::from)
        .to_vec()
}

fn visitors(kind: ChartKind) -> impl View {
    Chart::new(
        "Visitors by month",
        kind,
        months(),
        vec![
            ChartSeries::new("Desktop", vec![186.0, 305.0, 237.0, 73.0, 209.0, 214.0]),
            ChartSeries::new("Mobile", vec![80.0, 200.0, 120.0, 190.0, 130.0, 140.0]),
        ],
    )
    .with(|chart| chart.with_legend())
}

fn browsers(kind: ChartKind) -> Configured<controls::Chart> {
    Chart::new(
        "Visitors by browser",
        kind,
        ["Chrome", "Safari", "Firefox", "Edge", "Other"]
            .map(String::from)
            .to_vec(),
        vec![ChartSeries::new(
            "Visitors",
            vec![275.0, 200.0, 187.0, 173.0, 90.0],
        )],
    )
    .with(|chart| chart.with_legend())
}

pub fn chart(_: &PageCx) -> AnyView {
    column((
        section("Bar", visitors(ChartKind::Bar)),
        section("Area", visitors(ChartKind::Area)),
        section("Line", visitors(ChartKind::Line)),
        section(
            "Pie and donut",
            // The two charts share the width.
            row((
                browsers(ChartKind::Pie).layout_priority(1.0),
                browsers(ChartKind::Pie)
                    .with(|chart| chart.donut())
                    .layout_priority(1.0),
            )),
        ),
        section("Radar", visitors(ChartKind::Radar)),
        section("Radial", browsers(ChartKind::Radial)),
    ))
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "card" => card,
        "badge" => badge,
        "avatar" => avatar,
        "carousel" => carousel,
        "data-table" => data_table,
        "chart" => chart,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, SemanticAction, SemanticRole};

    fn press(page: &mut Harness, key: Key) -> bool {
        let handled = page.host.key(
            KeyEvent::Down { key, repeat: false },
            ShortcutModifiers::default(),
        );
        page.host.layout();
        handled
    }

    fn role(page: &mut Harness, name: &str) -> SemanticRole {
        let node = page.node(name);
        page.host.semantic(node).unwrap().role
    }

    #[test]
    fn card_page_buttons_take_keys() {
        let mut page = Harness::new("card");
        assert!(page.act("Login to your account", SemanticAction::Focus));
        assert!(press(&mut page, Key::Tab));
        assert!(press(&mut page, Key::Enter));
    }

    #[test]
    fn carousel_page_moves_with_assistive_actions() {
        let mut page = Harness::new("carousel");
        assert!(page.act("Slides", SemanticAction::Increment));
        assert_eq!(page.value("Slides").as_deref(), Some("2"));
    }

    #[test]
    fn data_table_page_pages_with_assistive_actions() {
        let mut page = Harness::new("data-table");
        assert_eq!(role(&mut page, "Payments"), SemanticRole::Table);
        assert!(page.act("Payments", SemanticAction::Increment));
        assert!(
            !page.act("Payments", SemanticAction::Increment),
            "two pages"
        );
    }

    #[test]
    fn chart_page_charts_take_focus() {
        let mut page = Harness::new("chart");
        assert_eq!(role(&mut page, "Visitors by month"), SemanticRole::Image);
        assert!(page.act("Visitors by month", SemanticAction::Focus));
        assert!(press(&mut page, Key::ArrowRight));
    }

    #[test]
    fn avatar_page_names_the_people() {
        let mut page = Harness::new("avatar");
        assert_eq!(role(&mut page, "Ana Lee"), SemanticRole::Image);
        assert_eq!(role(&mut page, "Design team"), SemanticRole::Group);
    }
}
