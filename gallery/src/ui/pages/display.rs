//! Data display components: cards, badges, avatars, carousels, the data
//! table, and charts.

use crate::ui::page::{Build, PageCx, column, portrait, row, section, stateful};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Avatar, AvatarGroup, Badge, Card, Carousel, Chart, Configured, Cx, DataTable, Env,
    View, ViewExt,
};
use choscorui::controls::{
    self, BadgeVariant, ButtonVariant, ChartKind, ChartSeries, ControlSize, Orientation,
};
use choscorui::core::{Icon, ItemData, ItemModel, ListStore, Role, Value};

pub fn card(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let (login, reports) = (cx.env().clone(), cx.env().clone());
        column((
            section(
                t!("card.header_action"),
                Card::new(t!("card.login_title"), t!("card.login_description")).with(move |card| {
                    let text = |key| login.text(key, &[]);
                    card.with_action(text("card.sign_up"), ButtonVariant::Link)
                        .with_content(text("card.login_content"))
                        .with_footer(text("card.login_google"), ButtonVariant::Outline)
                        .with_footer(text("card.login"), ButtonVariant::Default)
                }),
            ),
            section(
                t!("card.small"),
                Card::new(t!("card.reports_title"), t!("card.reports_description")).with(
                    move |card| {
                        card.with_content(reports.text("card.reports_content", &[]))
                            .with_footer(reports.text("card.manage", &[]), ButtonVariant::Outline)
                            .with_size(ControlSize::Sm)
                    },
                ),
            ),
        ))
    })
}

pub fn badge(_: &PageCx) -> AnyView {
    let variants = [
        ("badge.default", BadgeVariant::Default),
        ("badge.secondary", BadgeVariant::Secondary),
        ("badge.destructive", BadgeVariant::Destructive),
        ("badge.outline", BadgeVariant::Outline),
        ("badge.ghost", BadgeVariant::Ghost),
        ("badge.link", BadgeVariant::Link),
    ];
    let badge = |key: &'static str, variant: BadgeVariant| {
        Badge::new(t!(key)).with(move |badge| badge.with_variant(variant))
    };
    let iconic = |key: &'static str, variant: BadgeVariant, icon: Icon| {
        badge(key, variant).with(move |badge| badge.with_icon(icon))
    };
    column((
        section(
            t!("badge.variants"),
            row(variants
                .into_iter()
                .map(|(key, variant)| badge(key, variant))
                .collect::<Vec<_>>()),
        ),
        section(
            t!("badge.with_icons"),
            row((
                iconic("badge.verified", BadgeVariant::Secondary, Icon::Check),
                iconic("badge.failed", BadgeVariant::Destructive, Icon::OctagonX),
                iconic("badge.syncing", BadgeVariant::Outline, Icon::LoaderCircle),
            )),
        ),
        section(
            t!("badge.links"),
            row((
                Badge::new(t!("badge.open_issue")).with(|badge| badge.as_link()),
                badge("badge.changelog", BadgeVariant::Outline).with(|badge| badge.as_link()),
            )),
        ),
    ))
}

/// A group of three people at `size`, named by the catalog string `name`.
fn team(name: &'static str, size: ControlSize, count: Option<&'static str>) -> impl View {
    let people: Vec<_> = vec![
        controls::Avatar::new("Ana Lee", "AL").with_image(portrait()),
        controls::Avatar::new("Bao Tran", "BT"),
        controls::Avatar::new("Chi Vo", "CV"),
    ]
    .into_iter()
    .map(|avatar| avatar.with_size(size))
    .collect();
    AvatarGroup::new(t!(name), people).with(move |group| {
        let group = group.with_size(size);
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
            t!("avatar.image_fallback"),
            row((ana(), Avatar::new("Bao Tran", "BT"))),
        ),
        section(
            t!("avatar.sizes"),
            row([ControlSize::Sm, ControlSize::Default, ControlSize::Lg]
                .into_iter()
                .map(|size| Avatar::new("Chi Vo", "CV").with(move |avatar| avatar.with_size(size)))
                .collect::<Vec<_>>()),
        ),
        section(
            t!("avatar.badges"),
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
            t!("avatar.groups"),
            row((
                team("avatar.design_team", ControlSize::Default, Some("+3")),
                team("avatar.small_team", ControlSize::Sm, None),
            )),
        ),
    ))
}

fn numbers(count: usize) -> Vec<String> {
    (1..=count).map(|n| n.to_string()).collect()
}

/// A carousel named by the catalog string `name`.
fn carousel_of(cx: &mut Cx, name: &'static str, count: usize) -> Configured<controls::Carousel> {
    Carousel::new(
        t!(name),
        t!("carousel.previous"),
        t!("carousel.next"),
        numbers(count),
        cx.state(0).binding(),
    )
}

pub fn carousel(_: &PageCx) -> AnyView {
    stateful(|cx| {
        column((
            section(
                t!("carousel.horizontal"),
                carousel_of(cx, "carousel.slides", 5),
            ),
            section(
                t!("carousel.three_per_view"),
                carousel_of(cx, "carousel.thumbnails", 8)
                    .with(|carousel| carousel.with_per_view(3)),
            ),
            section(
                t!("carousel.vertical"),
                carousel_of(cx, "carousel.stacked", 5).with(|carousel| {
                    carousel
                        .with_orientation(Orientation::Vertical)
                        .with_per_view(2)
                }),
            ),
        ))
    })
}

/// One payment row: status, email, and amount.
struct Payment(String, &'static str, f64);

impl ItemData for Payment {
    fn data(&self, column: usize, role: Role) -> Value {
        match (column, role) {
            (0, Role::Display | Role::Sort) => Value::Text(self.0.clone()),
            (1, Role::Display | Role::Sort) => Value::Text(self.1.into()),
            (2, Role::Display) => Value::Text(format!("${:.2}", self.2)),
            (2, Role::Sort) => Value::Float(self.2),
            _ => Value::None,
        }
    }
}

fn payments(env: &Env) -> Box<dyn ItemModel> {
    let text = |key| env.text(key, &[]);
    let mut store = ListStore::new(3);
    store.set_headers([
        text("data-table.status_column"),
        text("data-table.email_column"),
        text("data-table.amount_column"),
    ]);
    let rows: Vec<Payment> = [
        ("data-table.success", "ken99@example.com", 316.0),
        ("data-table.success", "abe45@example.com", 242.0),
        ("data-table.processing", "monserrat44@example.com", 837.0),
        ("data-table.success", "silas22@example.com", 874.0),
        ("data-table.failed", "carmella@example.com", 721.0),
        ("data-table.pending", "jason78@example.com", 450.0),
        ("data-table.success", "sarah23@example.com", 1280.0),
    ]
    .into_iter()
    .map(|(status, email, amount)| Payment(text(status), email, amount))
    .collect();
    store.insert(None, 0, rows).expect("root insert is valid");
    Box::new(store)
}

pub fn data_table(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let (model, options) = (cx.env().clone(), cx.env().clone());
        column(section(
            t!("data-table.features"),
            DataTable::new(
                t!("data-table.payments"),
                move || payments(&model),
                t!("data-table.previous"),
                t!("data-table.next"),
                cx.state(Vec::new()).binding(),
            )
            .with(move |table| {
                let status = options.clone();
                table
                    .with_filter(options.text("data-table.filter", &[]), Some(1))
                    .with_column_menu(options.text("data-table.columns", &[]))
                    .with_empty(options.text("data-table.empty", &[]))
                    .with_status(move |selected, total| {
                        let (selected, total) = (selected.to_string(), total.to_string());
                        status.text(
                            "data-table.status",
                            &[("selected", &selected), ("total", &total)],
                        )
                    })
                    .with_page_size(5)
            }),
        ))
    })
}

fn months(env: &Env) -> Vec<String> {
    [
        "chart.january",
        "chart.february",
        "chart.march",
        "chart.april",
        "chart.may",
        "chart.june",
    ]
    .map(|key| env.text(key, &[]))
    .to_vec()
}

fn visitors(kind: ChartKind) -> impl View {
    Chart::new(t!("chart.visitors_by_month"), kind, months, |env: &Env| {
        vec![
            ChartSeries::new(
                env.text("chart.desktop", &[]),
                vec![186.0, 305.0, 237.0, 73.0, 209.0, 214.0],
            ),
            ChartSeries::new(
                env.text("chart.mobile", &[]),
                vec![80.0, 200.0, 120.0, 190.0, 130.0, 140.0],
            ),
        ]
    })
    .with(|chart| chart.with_legend())
}

/// Browser names are product names; only the rest category translates.
fn browsers(kind: ChartKind) -> Configured<controls::Chart> {
    Chart::new(
        t!("chart.visitors_by_browser"),
        kind,
        |env: &Env| {
            let mut names = ["Chrome", "Safari", "Firefox", "Edge"]
                .map(String::from)
                .to_vec();
            names.push(env.text("chart.other", &[]));
            names
        },
        |env: &Env| {
            vec![ChartSeries::new(
                env.text("chart.visitors", &[]),
                vec![275.0, 200.0, 187.0, 173.0, 90.0],
            )]
        },
    )
    .with(|chart| chart.with_legend())
}

pub fn chart(_: &PageCx) -> AnyView {
    column((
        section(t!("chart.bar"), visitors(ChartKind::Bar)),
        section(t!("chart.area"), visitors(ChartKind::Area)),
        section(t!("chart.line"), visitors(ChartKind::Line)),
        section(
            t!("chart.pie_donut"),
            // The two charts share the width.
            row((
                browsers(ChartKind::Pie).layout_priority(1.0),
                browsers(ChartKind::Pie)
                    .with(|chart| chart.donut())
                    .layout_priority(1.0),
            )),
        ),
        section(t!("chart.radar"), visitors(ChartKind::Radar)),
        section(t!("chart.radial"), browsers(ChartKind::Radial)),
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
