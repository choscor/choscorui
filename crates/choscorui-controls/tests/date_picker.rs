//! DatePicker through the control host, and the calendar's choice modes.

mod hosted;

use choscorui_controls::{
    Calendar, CalendarMode, ControlMessage, ControlValue, DatePicker, VisualState,
};
use choscorui_core::{
    CivilDate, ColorRole, Constraints, DesignTokens, Key, Locale, Point, SemanticAction,
    SemanticRole, Size,
};
use hosted::{Harness, paints, rounded_fills, texts};

fn date(day: u8) -> CivilDate {
    CivilDate::new(2026, 10, day).unwrap()
}

fn picker(mode: CalendarMode) -> DatePicker {
    DatePicker::new("Due date", Locale::parse("en").unwrap(), date(12))
        .with_mode(mode)
        .with_placeholder("Pick a date")
        .with_today(date(12))
}

fn frame(h: &Harness) -> choscorui_core::Rect {
    h.host
        .get(h.node)
        .unwrap()
        .overlay()
        .expect("the calendar is open")
}

#[test]
fn a_click_on_a_day_chooses_it_and_closes() {
    let mut h = Harness::new(picker(CalendarMode::Single));
    assert_eq!(h.measure().height, 36.0);
    assert_eq!(texts(&h.paint()), ["Pick a date"]);
    let bounds = h.bounds();
    h.click(bounds.center());
    let frame = frame(&h);
    assert_eq!(frame.origin.y, bounds.max_y() + 4.0);
    // October 2026 starts on a Thursday; weeks start on Sunday in "en".
    // Row 2 is the first week: Oct 1 is the fifth cell. The grid starts
    // 12 pt inside the frame, after the caption and weekday rows.
    let cell = 32.0;
    let first = choscorui_core::Point::new(
        frame.origin.x + 12.0 + 4.0 * cell + cell / 2.0,
        frame.origin.y + 12.0 + 2.0 * cell + cell / 2.0,
    );
    h.messages();
    h.click(first);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Changed(ControlValue::Date(Some(date(1)))),
            ControlMessage::OpenChanged(false),
        ]
    );
    // The calendar fades out over its motion time.
    assert!(texts(&h.paint()).contains(&"October 2026".to_string()));
    h.set_time(1.0);
    assert_eq!(texts(&h.paint()), ["October 1, 2026"]);
    assert_eq!(h.semantics().value.as_deref(), Some("October 1, 2026"));
}

#[test]
fn keys_move_the_cursor_without_choosing_until_enter() {
    let mut h = Harness::new(picker(CalendarMode::Single));
    h.focus();
    h.key(Key::Enter);
    assert_eq!(h.semantics().state.expanded, Some(true));
    h.messages();
    h.key(Key::ArrowRight);
    h.key(Key::ArrowDown);
    assert!(h.messages().is_empty(), "the cursor moves without a choice");
    assert_eq!(h.focused_part(), "October 20, 2026");
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Changed(ControlValue::Date(Some(date(20)))),
            ControlMessage::OpenChanged(false),
        ]
    );
    h.key(Key::Space);
    h.key(Key::Escape);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(true),
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed,
        ]
    );
}

#[test]
fn a_range_picker_chooses_a_start_and_an_end_and_stays_open() {
    let mut h = Harness::new(picker(CalendarMode::Range));
    h.focus();
    h.key(Key::Enter);
    h.key(Key::Enter);
    for _ in 0..3 {
        h.key(Key::ArrowRight);
    }
    h.key(Key::Enter);
    let messages = h.messages();
    assert_eq!(
        messages.last(),
        Some(&ControlMessage::Changed(ControlValue::Dates(vec![
            date(12),
            date(15)
        ])))
    );
    assert_eq!(
        h.semantics().state.expanded,
        Some(true),
        "a range stays open"
    );
    h.set_time(1.0);
    let list = h.paint();
    let tokens = DesignTokens::light();
    let ends = rounded_fills(&list, tokens.color(ColorRole::Primary));
    assert_eq!(ends.len(), 2, "the two ends fill primary");
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("October 12, 2026 – October 15, 2026")
    );
}

#[test]
fn calendar_modes_choose_days_as_documented() {
    let locale = Locale::parse("en").unwrap();
    let mut multiple = Calendar::new(locale.clone(), date(12)).with_mode(CalendarMode::Multiple);
    assert!(multiple.dates().is_empty());
    multiple.activate(date(14));
    multiple.activate(date(3));
    multiple.activate(date(14));
    assert_eq!(multiple.dates(), [date(3)]);
    assert!(
        multiple.handle_key(Key::Enter),
        "Enter activates the cursor"
    );
    assert_eq!(
        multiple.dates(),
        [date(3), date(14)],
        "the cursor is on the 14th"
    );

    let mut range = Calendar::new(locale, date(12)).with_mode(CalendarMode::Range);
    range.activate(date(10));
    range.activate(date(5));
    assert_eq!(range.dates(), [date(5), date(10)], "an earlier day ends it");
    range.activate(date(9));
    assert_eq!(range.dates(), [date(9)], "a day inside starts again");
    range.activate(date(20));
    assert_eq!(range.dates(), [date(9), date(20)]);
    range.activate(date(3));
    assert_eq!(
        range.dates(),
        [date(3), date(20)],
        "an earlier day extends it"
    );
    range.activate(date(25));
    assert_eq!(range.dates(), [date(3), date(25)], "a later day extends it");

    let node = range.semantics("Stay");
    assert_eq!(node.role, SemanticRole::Table);
}

#[test]
fn assistive_actions_open_and_close_the_calendar() {
    let mut h = Harness::new(picker(CalendarMode::Multiple));
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::Button);
    assert_eq!(node.value.as_deref(), Some("Pick a date"));
    assert!(h.perform(SemanticAction::Expand, None));
    assert!(h.host.get(h.node).unwrap().overlay().is_some());
    assert!(h.perform(SemanticAction::Collapse, None));
    assert!(h.host.get(h.node).unwrap().overlay().is_none());
}

#[test]
fn an_invalid_picker_shows_the_destructive_border_and_reports_it() {
    let destructive = DesignTokens::light().color(ColorRole::Destructive);
    let h = Harness::new(picker(CalendarMode::Single));
    assert!(!h.semantics().state.invalid);
    assert!(!paints(&h.paint(), destructive));

    let mut invalid = picker(CalendarMode::Single).with_invalid(true);
    assert!(invalid.visual_state(VisualState::NONE).invalid());
    invalid.set_invalid(false);
    assert!(!invalid.visual_state(VisualState::NONE).invalid());
    invalid.set_invalid(true);
    let h = Harness::new(invalid);
    assert!(h.semantics().state.invalid);
    assert!(paints(&h.paint(), destructive), "the trigger border");
}

#[test]
fn near_the_bottom_edge_the_calendar_opens_above() {
    let mut h = Harness::new(picker(CalendarMode::Single));
    let size = h.bounds().size;
    h.place(hosted::rect(10.0, 550.0, size.width, size.height));
    h.click(h.bounds().center());
    assert_eq!(frame(&h).max_y(), 546.0, "4 pt above the trigger");
}

#[test]
fn closing_without_a_choice_reports_dismissed_and_disabling_closes() {
    let mut h = Harness::new(picker(CalendarMode::Single));
    h.click(h.bounds().center());
    h.messages();
    h.click(choscorui_core::Point::new(790.0, 590.0));
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
    h.click(h.bounds().center());
    h.messages();
    h.set_enabled(false);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
    assert!(h.host.get(h.node).unwrap().overlay().is_none());
}

#[test]
fn month_buttons_respect_the_range_and_page_the_cursor() {
    let mut calendar = Calendar::new(Locale::parse("en").unwrap(), date(15)).with_name("Due");
    calendar.set_range(Some(CivilDate::new(2026, 9, 10).unwrap()), Some(date(20)));
    assert!(!calendar.can_show_month(1), "November is past the range");
    let mut h = Harness::new(calendar);
    h.focus();
    // The previous-month button is the first stop. The disabled
    // next-month button is no stop.
    h.key(Key::Enter);
    assert_eq!(h.semantics().description, "September 2026");
    // The cursor moved into September. The previous-month button is now
    // disabled, so focus moved to the grid, where Enter chooses.
    h.key(Key::Enter);
    let september = CivilDate::new(2026, 9, 15).unwrap();
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Changed(ControlValue::Date(Some(september))),
            ControlMessage::Activated,
        ]
    );
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 28.0, bounds.origin.y + 28.0));
    assert_eq!(h.semantics().description, "September 2026", "disabled");
}

#[test]
fn a_multiple_picker_counts_its_days_and_a_key_clears_them() {
    let mut h = Harness::new(picker(CalendarMode::Multiple));
    let node = h.node;
    let dates = ControlValue::Dates(vec![date(3), date(9)]);
    assert!(h.host.set_value(&mut h.tree, node, &dates));
    assert_eq!(texts(&h.paint()), ["2 dates"]);
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("October 3, 2026, October 9, 2026")
    );
    h.focus();
    h.key(Key::Delete);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Dates(Vec::new()))]
    );
    assert_eq!(texts(&h.paint()), ["Pick a date"]);
}

#[test]
fn arrow_up_opens_and_choosing_the_chosen_day_again_clears_it() {
    let mut h = Harness::new(picker(CalendarMode::Single));
    let node = h.node;
    let chosen = ControlValue::Date(Some(date(12)));
    assert!(h.host.set_value(&mut h.tree, node, &chosen));
    h.focus();
    h.key(Key::ArrowUp);
    assert_eq!(h.semantics().state.expanded, Some(true));
    h.messages();
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Changed(ControlValue::Date(None)),
            ControlMessage::OpenChanged(false),
        ]
    );
}

#[test]
fn a_picker_takes_a_range_and_disabled_days_and_truncates_to_its_width() {
    let picker = picker(CalendarMode::Single)
        .with_range(Some(date(5)), Some(date(25)))
        .with_disabled_days(|day| day.day() == 13);
    let mut h = Harness::new(picker);
    h.focus();
    h.key(Key::Enter);
    h.messages();
    h.key(Key::ArrowRight);
    h.key(Key::Enter);
    assert!(h.messages().is_empty(), "the 13th is disabled");
    h.key(Key::ArrowRight);
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Changed(ControlValue::Date(Some(date(14)))),
            ControlMessage::OpenChanged(false),
        ]
    );
    let narrow = h.measure_in(Constraints::loose(Size::new(120.0, 100.0)));
    assert_eq!(narrow.width, 120.0);
    h.place(hosted::rect(10.0, 10.0, narrow.width, narrow.height));
    h.set_time(1.0);
    assert!(
        texts(&h.paint())
            .iter()
            .any(|text| text.ends_with('\u{2026}')),
        "the label truncates"
    );
}
