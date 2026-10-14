//! Chart through the control host.

mod hosted;

use choscorui_controls::{Chart, ChartKind, ChartSeries};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, Key, LayoutDirection, Locale, Paint, Point, Rect,
    SemanticRole,
};
use hosted::{Harness, paints, texts};

fn months() -> Vec<String> {
    ["January", "February", "March"].map(String::from).to_vec()
}

fn visitors(kind: ChartKind) -> Chart {
    Chart::new(
        "Visitors",
        kind,
        months(),
        vec![
            ChartSeries::new("Desktop", vec![186.0, 305.0, 237.0]),
            ChartSeries::new("Mobile", vec![80.0, 200.0, 120.0]),
        ],
    )
    .with_legend()
}

fn browsers(kind: ChartKind) -> Chart {
    Chart::new(
        "Browsers",
        kind,
        ["Chrome", "Safari", "Firefox", "Edge"]
            .map(String::from)
            .to_vec(),
        vec![ChartSeries::new(
            "Visitors",
            vec![275.0, 200.0, 187.0, 173.0],
        )],
    )
}

#[test]
fn a_bar_chart_paints_series_colors_axis_labels_and_a_legend() {
    let tokens = DesignTokens::light();
    let mut h = Harness::new(visitors(ChartKind::Bar));
    // 200 pt plot, 22 pt axis labels, 28 pt legend.
    assert_eq!(h.measure().height, 250.0);
    assert_eq!(h.measure().width, 480.0);
    let list = h.paint();
    assert_eq!(
        texts(&list),
        ["January", "February", "March", "Desktop", "Mobile"]
    );
    assert!(paints(&list, tokens.color(ColorRole::Chart1)));
    assert!(paints(&list, tokens.color(ColorRole::Chart2)));
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::Image);
    assert_eq!(node.description, "Desktop, Mobile");
}

#[test]
fn arrow_keys_pick_categories_and_show_the_tooltip() {
    let mut h = Harness::new(visitors(ChartKind::Line));
    h.focus();
    assert_eq!(h.host.get(h.node).unwrap().overlay(), None);
    h.key(Key::ArrowRight);
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("January: Desktop 186, Mobile 80")
    );
    let tooltip = h.host.get(h.node).unwrap().overlay().expect("shown");
    let list = h.paint();
    assert!(texts(&list).contains(&"186".to_string()), "{tooltip:?}");
    h.key(Key::End);
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("March: Desktop 237, Mobile 120")
    );
    h.key(Key::Escape);
    assert_eq!(h.host.get(h.node).unwrap().overlay(), None);

    let mut rtl = Harness::with_direction(visitors(ChartKind::Area), LayoutDirection::Rtl);
    rtl.focus();
    rtl.key(Key::ArrowLeft);
    rtl.key(Key::ArrowLeft);
    assert!(rtl.semantics().value.unwrap().starts_with("February"));
}

#[test]
fn hover_picks_the_category_under_the_pointer() {
    let mut h = Harness::new(visitors(ChartKind::Bar));
    let bounds = h.bounds();
    h.hover(Point::new(bounds.origin.x + 20.0, bounds.origin.y + 100.0));
    assert!(h.semantics().value.unwrap().starts_with("January"));
    let mut rtl = Harness::with_direction(visitors(ChartKind::Bar), LayoutDirection::Rtl);
    rtl.hover(Point::new(bounds.origin.x + 20.0, bounds.origin.y + 100.0));
    assert!(
        rtl.semantics().value.unwrap().starts_with("March"),
        "mirrored"
    );
}

#[test]
fn polar_charts_hit_slices_spokes_and_rings() {
    let mut pie = Harness::new(
        browsers(ChartKind::Pie)
            .donut()
            .with_format(|v| format!("{v} visitors")),
    );
    let center = Point::new(pie.bounds().center().x, pie.bounds().origin.y + 100.0);
    // Just right of the top, inside the ring: the first slice.
    pie.hover(Point::new(center.x + 10.0, center.y - 80.0));
    assert_eq!(
        pie.semantics().value.as_deref(),
        Some("Chrome: 275 visitors")
    );
    pie.hover(center);
    assert_eq!(pie.semantics().value, None, "the donut hole");
    // Just left of the top: the last slice.
    pie.hover(Point::new(center.x - 10.0, center.y - 80.0));
    assert!(pie.semantics().value.unwrap().starts_with("Edge"));

    let mut radar = Harness::new(browsers(ChartKind::Radar));
    radar.hover(Point::new(center.x + 2.0, center.y - 50.0));
    assert!(radar.semantics().value.unwrap().starts_with("Chrome"));
    radar.hover(Point::new(center.x + 50.0, center.y));
    assert!(radar.semantics().value.unwrap().starts_with("Safari"));

    let mut radial = Harness::new(browsers(ChartKind::Radial));
    // The innermost ring starts at 30% of the 100 pt radius.
    radial.hover(Point::new(center.x, center.y - 35.0));
    assert!(radial.semantics().value.unwrap().starts_with("Chrome"));
    radial.hover(Point::new(center.x, center.y - 95.0));
    assert!(radial.semantics().value.unwrap().starts_with("Edge"));
}

#[test]
fn the_tooltip_hides_when_the_pointer_leaves_and_blur_clears_the_highlight() {
    let mut h = Harness::new(visitors(ChartKind::Bar));
    h.focus();
    let bounds = h.bounds();
    h.hover(Point::new(bounds.origin.x + 20.0, bounds.origin.y + 100.0));
    assert!(h.host.get(h.node).unwrap().overlay().is_some());
    h.hover(Point::new(bounds.max_x() + 50.0, bounds.max_y() + 50.0));
    assert_eq!(
        h.host.get(h.node).unwrap().overlay(),
        None,
        "the tooltip hides, though the chart has focus"
    );
    assert!(h.semantics().value.is_some(), "the highlight stays");
    h.key(Key::ArrowRight);
    h.key(Key::Escape);
    assert_eq!(h.semantics().value, None, "Escape clears the highlight");
    h.key(Key::ArrowRight);
    h.host.move_focus(&mut h.tree, None);
    assert_eq!(h.semantics().value, None, "blur clears the highlight");
}

#[test]
fn a_radar_with_two_categories_shows_bars() {
    let radar = Chart::new(
        "Skills",
        ChartKind::Radar,
        vec!["Speed".into(), "Power".into()],
        vec![ChartSeries::new("Me", vec![3.0, 4.0])],
    );
    let h = Harness::new(radar);
    let list = h.paint();
    assert_eq!(texts(&list), ["Speed", "Power"], "the category axis");
    assert!(paints(
        &list,
        DesignTokens::light().color(ColorRole::Chart1)
    ));
}

#[test]
fn negative_values_get_a_baseline_and_values_use_the_locale() {
    let chart = |locale: &str| {
        Chart::new(
            "Profit",
            ChartKind::Bar,
            vec!["Q1".into(), "Q2".into()],
            vec![ChartSeries::new("Profit", vec![-1250.0, 3000.5])],
        )
        .with_locale(Locale::parse(locale).unwrap())
    };
    let mut h = Harness::new(chart("en"));
    let tokens = DesignTokens::light();
    let baseline = tokens.color(ColorRole::MutedForeground);
    let list = h.paint();
    let zero = list.commands().iter().find_map(|command| match command {
        DisplayCommand::FillRect { rect, color } if *color == baseline => Some(*rect),
        _ => None,
    });
    let zero = zero.expect("a zero baseline");
    let bars: Vec<Rect> = list
        .commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::FillPath {
                path,
                paint: Paint::Solid(color),
                ..
            } if *color == tokens.color(ColorRole::Chart1) => Some(path.bounds()),
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 2);
    assert!(bars[0].origin.y >= zero.origin.y - 1.0, "below: {bars:?}");
    assert!(bars[1].max_y() <= zero.max_y() + 1.0, "above: {bars:?}");
    h.focus();
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().value.as_deref(), Some("Q1: Profit -1,250"));
    let mut vi = Harness::new(chart("vi"));
    vi.focus();
    vi.key(Key::End);
    assert_eq!(vi.semantics().value.as_deref(), Some("Q2: Profit 3.000,50"));
}
