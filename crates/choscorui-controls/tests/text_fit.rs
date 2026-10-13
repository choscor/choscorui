//! Text that does not fit: one-line labels truncate with an ellipsis that
//! follows the reading order, paragraphs wrap and grow taller, and
//! measures honor their constraints.

mod hosted;

use choscorui_controls::{
    Accordion, AlertDialog, Avatar, Breadcrumb, Card, Chart, ChartKind, ChartSeries, CommandId,
    CommandLink, ControlHost, ControlValue, CustomControl, DataTable, Dialog, DialogButton,
    DialogButtonRole, ELLIPSIS, EstimatedText, HostEnv, ListEntry, PaintContext, Select, Sidebar,
    SidebarItem, Styles, TextLine, TextMeasure, TextOverflow, VisibleTooltip, draw_text,
    truncate_end,
};
use choscorui_core::{
    Constraints, DesignTokens, DisplayCommand, DisplayList, FontWeight, Icon, ItemData, Key,
    LayoutDirection, ListStore, Point, Rect, Role, Size, TextSize, ThemeResolver, Tree, TypeSpec,
    Value,
};
use hosted::{Harness, rect, texts};

/// The type of a default label: 14 pt, regular.
fn spec() -> TypeSpec {
    DesignTokens::light().text(TextSize::Sm, FontWeight::NORMAL)
}

/// The text runs of `list` with their baseline x.
fn runs(list: &DisplayList) -> Vec<(String, f64)> {
    list.commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText { text, baseline, .. } => Some((text.clone(), baseline.x)),
            _ => None,
        })
        .collect()
}

#[test]
fn text_that_fits_comes_back_whole() {
    let spec = spec();
    let width = EstimatedText.width("Apple", &spec);
    let (text, drawn) = truncate_end("Apple", &spec, width, &EstimatedText);
    assert_eq!(text, "Apple");
    assert_eq!(drawn, width);
}

#[test]
fn long_text_ends_in_an_ellipsis_inside_the_width() {
    let spec = spec();
    let long = "Dragon fruit from the southern provinces";
    let (text, drawn) = truncate_end(long, &spec, 100.0, &EstimatedText);
    assert!(text.ends_with(ELLIPSIS), "{text}");
    assert!(long.starts_with(text.trim_end_matches(ELLIPSIS)));
    assert!(drawn <= 100.0);
    // One more grapheme does not fit.
    let kept = text.trim_end_matches(ELLIPSIS).chars().count();
    let next: String = long.chars().take(kept + 1).collect();
    let wider = format!("{}{ELLIPSIS}", next.trim_end());
    assert!(EstimatedText.width(&wider, &spec) > 100.0 || next.ends_with(' '));
    // No space before the ellipsis, and cuts fall between graphemes.
    assert!(!text.contains(&format!(" {ELLIPSIS}")));
    let (cut, _) = truncate_end("Hà Nội và Đà Lạt", &spec, 50.0, &EstimatedText);
    assert!(cut.ends_with(ELLIPSIS), "{cut}");
}

#[test]
fn text_with_no_room_is_an_ellipsis_alone() {
    let spec = spec();
    let (text, _) = truncate_end("Apple", &spec, 1.0, &EstimatedText);
    assert_eq!(text, ELLIPSIS);
}

#[test]
fn the_ellipsis_mirrors_in_right_to_left_text_and_a_clip_cuts_at_the_edges() {
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let style = styles.resolve(
        &tokens,
        choscorui_controls::StyleQuery::new(
            choscorui_controls::ControlKind::Label,
            choscorui_controls::Slot::Root,
        ),
    );
    let bounds = rect(10.0, 0.0, 80.0, 20.0);
    let long = "Notifications from every project";
    let draw = |rtl: bool, overflow: TextOverflow| {
        let mut list = DisplayList::new();
        let line = TextLine::new(long, &style, rtl).overflow(overflow);
        let drawn = draw_text(&mut list, bounds, &line, &EstimatedText);
        (list, drawn)
    };
    // Start-aligned text sits at the start in each direction, so the
    // ellipsis is at the left end in right-to-left text.
    let (ltr, drawn) = draw(false, TextOverflow::Ellipsis);
    assert_eq!(runs(&ltr)[0].1, bounds.origin.x);
    assert!(drawn.max_x() <= bounds.max_x());
    let (rtl, drawn) = draw(true, TextOverflow::Ellipsis);
    assert!(runs(&rtl)[0].0.ends_with(ELLIPSIS));
    assert!((drawn.max_x() - bounds.max_x()).abs() < 1e-9);
    // A clip keeps every character and cuts the run at the rectangle.
    let (clipped, drawn) = draw(false, TextOverflow::Clip);
    assert_eq!(runs(&clipped)[0].0, long);
    assert!(clipped.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::PushClip(clip) if clip.origin.x == bounds.origin.x
            && clip.size.width == bounds.size.width
    )));
    assert_eq!(drawn.size.width, bounds.size.width);
}

#[test]
fn a_long_select_value_truncates_before_the_chevron() {
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        let select = Select::new(
            "Fruit",
            vec![ListEntry::option(
                "Dragon fruit from the southern provinces",
            )],
        );
        let mut h = Harness::with_direction(select, direction);
        assert!(
            h.host
                .set_value(&mut h.tree, h.node, &ControlValue::Index(Some(0)))
        );
        let trigger = rect(10.0, 10.0, 160.0, 36.0);
        h.place(trigger);
        let painted = runs(&h.paint());
        let (value, x) = &painted[0];
        assert!(value.ends_with(ELLIPSIS), "{value}");
        let width = EstimatedText.width(value, &spec());
        // The text stays clear of the chevron, which takes the end.
        if direction == LayoutDirection::Ltr {
            assert!(x + width <= trigger.max_x() - 16.0 - 12.0);
        } else {
            assert!(*x >= trigger.origin.x + 16.0 + 12.0);
        }
    }
}

#[test]
fn a_long_sidebar_label_truncates_before_its_badge() {
    let sidebar = Sidebar::new(
        "Navigation",
        "Toggle sidebar",
        vec![(
            "Application".to_string(),
            vec![
                SidebarItem::new("Notifications from every project", Icon::Inbox).with_badge("128"),
            ],
        )],
    );
    let mut h = Harness::new(sidebar);
    let height = h.bounds().size.height;
    h.place(rect(10.0, 10.0, 200.0, height));
    let painted = runs(&h.paint());
    let badge = painted.iter().find(|(text, _)| text == "128").unwrap();
    let label = painted
        .iter()
        .find(|(text, _)| text.starts_with("Notif"))
        .unwrap();
    assert!(label.0.ends_with(ELLIPSIS), "{}", label.0);
    let end = label.1 + EstimatedText.width(&label.0, &spec());
    assert!(end < badge.1, "{end} before the badge at {}", badge.1);
}

struct Email(&'static str);

impl ItemData for Email {
    fn data(&self, _column: usize, role: Role) -> Value {
        match role {
            Role::Display | Role::Sort => Value::Text(self.0.into()),
            _ => Value::None,
        }
    }
}

#[test]
fn a_long_data_table_cell_truncates() {
    let mut store = ListStore::new(1);
    store.set_headers(["Email"]);
    store
        .insert(None, 0, vec![Email("monserrat.de.la.cruz.44@example.com")])
        .unwrap();
    let table = DataTable::new("Payments", Box::new(store), "Previous", "Next");
    let mut h = Harness::new(table);
    let height = h.bounds().size.height;
    h.place(rect(10.0, 10.0, 220.0, height));
    let painted = texts(&h.paint());
    assert!(
        painted
            .iter()
            .any(|text| text.starts_with("monserrat") && text.ends_with(ELLIPSIS)),
        "{painted:?}"
    );
}

#[test]
fn long_tooltip_text_wraps_and_the_measured_height_grows() {
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let tip = |text: &str| VisibleTooltip {
        target: Tree::new(rect(0.0, 0.0, 1.0, 1.0)).root(),
        text: text.into(),
        anchor: rect(300.0, 300.0, 80.0, 24.0),
    };
    let short = tip("Save").size(&styles, &tokens, &EstimatedText);
    let long = tip(
        "Saves the document, and keeps a copy of each earlier version so that you can go back to it later",
    );
    let size = long.size(&styles, &tokens, &EstimatedText);
    assert_eq!(size.width, 320.0, "the maximum width");
    let line = 16.0;
    assert_eq!(size.height, short.height + 2.0 * line, "three lines");
    let placed = long.place(
        rect(0.0, 0.0, 800.0, 600.0),
        &styles,
        &tokens,
        &EstimatedText,
        LayoutDirection::Ltr,
    );
    let mut list = DisplayList::new();
    long.paint(
        &mut PaintContext::new(&mut list, placed.rect, &tokens, &styles, &EstimatedText),
        placed.placement,
    );
    let lines = runs(&list);
    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(|(text, _)| !text.ends_with(ELLIPSIS)));
}

#[test]
fn a_long_alert_message_wraps_and_the_measured_height_grows() {
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let buttons = || {
        vec![
            DialogButton::new("Delete", DialogButtonRole::Destructive),
            DialogButton::new("Cancel", DialogButtonRole::Reject),
        ]
    };
    let available = Size::new(800.0, 600.0);
    let short = AlertDialog::new("Delete?", "You cannot undo it.", buttons());
    let long = AlertDialog {
        detail: Some(
            "The members get an email about it, and the project leaves their lists.".into(),
        ),
        ..AlertDialog::new(
            "Delete?",
            "This deletes the project, its files, and its history for every member of the team, and you cannot undo it.",
            buttons(),
        )
    };
    let short_size = short.measure(available, &styles, &tokens, &EstimatedText);
    let long_size = long.measure(available, &styles, &tokens, &EstimatedText);
    assert_eq!(short_size.width, long_size.width);
    assert!(long_size.height > short_size.height + 20.0);
    let mut list = DisplayList::new();
    long.paint(&mut PaintContext::new(
        &mut list,
        Rect::new(Point::new(0.0, 0.0), long_size),
        &tokens,
        &styles,
        &EstimatedText,
    ));
    assert!(runs(&list).len() >= 4, "{:?}", runs(&list));
}

/// The measured size of `control` in `constraints`.
fn measure_in(control: impl CustomControl + 'static, constraints: Constraints) -> Size {
    let themes = ThemeResolver::new(DesignTokens::light());
    let styles = Styles::new();
    let env = HostEnv::new(&themes, &styles, &EstimatedText);
    let mut tree = Tree::new(rect(0.0, 0.0, 800.0, 600.0));
    let mut host = ControlHost::new();
    let root = tree.root();
    let node = host.mount(&mut tree, root, Box::new(control)).unwrap();
    host.measure(&tree, node, constraints, &env).unwrap().size
}

#[test]
fn command_link_breadcrumb_and_card_measures_honor_their_constraints() {
    let narrow = Constraints::loose(Size::new(120.0, 400.0));
    let link = CommandLink::new(
        "Open the project settings",
        "Change the name, the members, and the storage",
    )
    .hosted(CommandId(1));
    assert!(measure_in(link, narrow).width <= 120.0);
    let trail = Breadcrumb::new(
        "Path",
        ["Home", "Documents", "Projects", "Quarterly report"]
            .map(String::from)
            .to_vec(),
    );
    assert!(measure_in(trail, narrow).width <= 120.0);
    let card = Card::new("Team", "The members of the team").with_content("Ana, Ben, and Chi.");
    let tight = Constraints::tight(Size::new(240.0, 100.0));
    assert_eq!(measure_in(card, tight), Size::new(240.0, 100.0));
}

#[test]
fn a_narrow_breadcrumb_truncates_its_labels_inside_its_bounds() {
    let trail = Breadcrumb::new(
        "Path",
        ["Home", "Documents", "Quarterly report"]
            .map(String::from)
            .to_vec(),
    );
    let mut h = Harness::new(trail);
    let height = h.bounds().size.height;
    let bounds = rect(10.0, 10.0, 120.0, height);
    h.place(bounds);
    let painted = runs(&h.paint());
    assert!(painted.iter().any(|(text, _)| text.ends_with(ELLIPSIS)));
    for (text, x) in &painted {
        assert!(*x >= bounds.origin.x);
        assert!(x + EstimatedText.width(text, &spec()) <= bounds.max_x() + 0.5);
    }
}

/// The text runs of `list` with their font size and their end.
fn sized_runs(list: &DisplayList) -> Vec<(String, f64, f64)> {
    list.commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText {
                text,
                baseline,
                font_size,
                ..
            } => {
                let spec = TypeSpec {
                    size: *font_size,
                    ..spec()
                };
                let end = baseline.x + EstimatedText.width(text, &spec);
                Some((text.clone(), *font_size, end))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_dialog_title_wraps_clear_of_the_close_button() {
    let title = "Edit the profile of every member of the team";
    let open_title_ends = |close: bool| {
        let mut dialog = Dialog::new("Edit", title, "Make changes here.");
        if close {
            dialog = dialog.with_close("Close");
        }
        let mut h = Harness::new(dialog);
        h.focus();
        h.key(Key::Enter);
        h.set_time(1.0);
        let runs = sized_runs(&h.paint());
        runs.into_iter()
            .filter(|(text, ..)| title.contains(text.as_str()) && text != "Edit")
            .map(|(_, _, end)| end)
            .collect::<Vec<_>>()
    };
    let plain = open_title_ends(false);
    let closable = open_title_ends(true);
    assert!(
        closable.len() >= plain.len() && closable.len() >= 2,
        "{closable:?}"
    );
    let widest = |ends: &[f64]| ends.iter().copied().fold(0.0, f64::max);
    assert!(widest(&closable) < widest(&plain), "{closable:?} {plain:?}");
}

#[test]
fn a_long_accordion_title_wraps_and_the_item_grows() {
    let height = |title: &str| {
        let accordion = Accordion::new("FAQ", vec![(title.to_string(), "Yes.".to_string())]);
        measure_in(accordion, Constraints::loose(Size::new(240.0, 600.0))).height
    };
    let short = height("Is it styled?");
    let long = height("Is it styled to match the other components of the design system?");
    assert!(long > short + 10.0, "{long} after {short}");
}

#[test]
fn a_narrow_chart_puts_its_legend_in_more_rows() {
    let chart = || {
        Chart::new(
            "Visitors",
            ChartKind::Bar,
            ["January", "February"].map(String::from).to_vec(),
            vec![
                ChartSeries::new("Desktop computers", vec![186.0, 305.0]),
                ChartSeries::new("Mobile phones", vec![80.0, 200.0]),
                ChartSeries::new("Tablets", vec![20.0, 40.0]),
            ],
        )
        .with_legend()
    };
    let wide = measure_in(chart(), Constraints::loose(Size::new(600.0, 900.0))).height;
    let narrow = measure_in(chart(), Constraints::loose(Size::new(160.0, 900.0))).height;
    assert!(narrow > wide + 20.0, "{narrow} after {wide}");
}

#[test]
fn a_long_avatar_fallback_shrinks_to_fit_the_circle() {
    let painted = |fallback: &str| {
        let h = Harness::new(Avatar::new("Ana Lee", fallback));
        let bounds = h.bounds();
        let run = sized_runs(&h.paint()).remove(0);
        (run, bounds)
    };
    let ((text, size, _), _) = painted("AL");
    assert_eq!(text, "AL");
    let ((text, smaller, end), bounds) = painted("ANA LEE");
    assert_eq!(text, "ANA LEE", "a fallback never truncates");
    assert!(smaller < size);
    assert!(end <= bounds.max_x() + 0.5);
}
