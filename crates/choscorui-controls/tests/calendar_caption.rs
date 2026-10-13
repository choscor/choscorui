//! The dropdown caption of Calendar and DatePicker through the control
//! host: focus order, keys, pointer, the range, semantics, and mirroring.

mod hosted;

use choscorui_controls::{Calendar, ControlMessage, DatePicker, ShortcutModifiers};
use choscorui_core::{
    CivilDate, DisplayCommand, DisplayList, Key, LayoutDirection, Locale, Point, Rect,
    SemanticAction,
};
use hosted::{Harness, texts};

fn date(year: i32, month: u8, day: u8) -> CivilDate {
    CivilDate::new(year, month, day).unwrap()
}

fn calendar() -> Calendar {
    Calendar::new(Locale::parse("en").unwrap(), date(2026, 10, 12))
        .with_name("Due")
        .with_dropdown_caption(2020, 2030)
}

fn overlay(h: &Harness) -> Option<Rect> {
    h.host.get(h.node).unwrap().overlay()
}

/// The baseline start of the first text run that reads `text`, whole or
/// truncated with an ellipsis.
fn text_x(list: &DisplayList, text: &str) -> f64 {
    list.commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawText {
                baseline, text: t, ..
            } if t == text
                || t.strip_suffix('\u{2026}')
                    .is_some_and(|start| !start.is_empty() && text.starts_with(start)) =>
            {
                Some(baseline.x)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text {text}"))
}

fn shift_tab(h: &mut Harness) {
    h.modifiers(ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    });
    h.key(Key::Tab);
    h.modifiers(ShortcutModifiers::default());
}

#[test]
fn the_caption_shows_month_and_year_dropdowns_in_place_of_the_title() {
    let h = Harness::new(calendar());
    let painted = texts(&h.paint());
    assert!(painted.contains(&"October".to_string()));
    assert!(painted.contains(&"2026".to_string()));
    assert!(!painted.contains(&"October 2026".to_string()));
    assert_eq!(h.semantics().description, "October 2026");

    // The plain caption keeps the title.
    let plain = Harness::new(Calendar::new(
        Locale::parse("en").unwrap(),
        date(2026, 10, 12),
    ));
    assert!(texts(&plain.paint()).contains(&"October 2026".to_string()));
}

#[test]
fn the_dropdowns_are_tab_stops_before_the_grid() {
    let mut h = Harness::new(calendar());
    h.focus();
    let node = h.semantics();
    assert_eq!(node.description, "October", "Tab enters at the month");
    assert_eq!(node.state.expanded, Some(false));
    assert!(node.actions.contains(&SemanticAction::ShowMenu));
    h.key(Key::Tab);
    assert_eq!(h.semantics().description, "2026");
    h.key(Key::Tab);
    assert_eq!(h.semantics().description, "October 2026", "the grid");
    assert_eq!(h.semantics().state.expanded, None);
    shift_tab(&mut h);
    assert_eq!(h.semantics().description, "2026");
    shift_tab(&mut h);
    shift_tab(&mut h);
    assert_eq!(
        h.semantics().description,
        "October",
        "focus left at the start"
    );
}

#[test]
fn keys_open_a_dropdown_and_a_choice_shows_its_month() {
    let mut h = Harness::new(calendar());
    h.focus();
    for open in [Key::Enter, Key::Space, Key::ArrowDown] {
        h.key(open);
        assert_eq!(h.semantics().state.expanded, Some(true), "{open:?}");
        assert!(overlay(&h).is_some());
        h.key(Key::Escape);
        assert_eq!(h.semantics().state.expanded, Some(false));
        assert!(overlay(&h).is_none());
    }
    h.key(Key::Enter);
    h.key(Key::ArrowDown);
    assert_eq!(h.semantics().description, "November", "the highlight");
    h.key(Key::Enter);
    assert_eq!(h.semantics().state.expanded, Some(false));
    assert_eq!(h.semantics().description, "November");

    h.key(Key::Tab);
    h.key(Key::Space);
    assert_eq!(h.semantics().description, "2026");
    h.key(Key::ArrowUp);
    h.key(Key::Enter);
    h.key(Key::Tab);
    assert_eq!(h.semantics().description, "November 2025");
    assert!(h.messages().is_empty(), "showing a month changes no choice");

    // Type-ahead finds a year in the open list.
    shift_tab(&mut h);
    h.key(Key::Enter);
    h.text("2029");
    assert_eq!(h.semantics().description, "2029");
    h.key(Key::Enter);
    h.key(Key::Tab);
    assert_eq!(h.semantics().description, "November 2029");
}

#[test]
fn a_press_on_a_trigger_opens_its_list_and_a_press_on_an_option_chooses() {
    let mut h = Harness::new(calendar());
    h.set_time(1.0);
    let painted = h.paint();
    let month = text_x(&painted, "October");
    let bounds = h.bounds();
    // The caption row is the first 32 pt row inside the 12 pt padding.
    let caption_y = bounds.origin.y + 12.0 + 16.0;
    h.click(Point::new(month + 4.0, caption_y));
    let list = overlay(&h).expect("the month list is open");
    assert!(
        list.origin.y >= bounds.origin.y + 12.0 + 32.0,
        "below the caption"
    );
    assert_eq!(h.semantics().description, "October");
    assert_eq!(h.semantics().state.expanded, Some(true));

    // Eight 32 pt rows show, scrolled so that October is the last.
    let painted = texts(&h.paint());
    assert!(painted.contains(&"March".to_string()));
    assert!(!painted.contains(&"January".to_string()));
    assert!(!painted.contains(&"December".to_string()));
    // Resting on the scroll-down button scrolls the list over time.
    h.hover(Point::new(list.center().x, list.max_y() - 4.0 - 2.0));
    assert!(h.host.next_deadline(&h.tree).is_some());
    // The fourth visible row is June.
    let june = Point::new(list.center().x, list.origin.y + 4.0 + 3.0 * 32.0 + 16.0);
    h.hover(june);
    h.click(june);
    assert!(overlay(&h).is_none());
    assert_eq!(h.semantics().description, "June");

    // A press outside the open list closes it.
    h.click(Point::new(month + 4.0, caption_y));
    assert!(overlay(&h).is_some());
    h.click(Point::new(bounds.center().x, bounds.max_y() - 20.0));
    assert!(overlay(&h).is_none());
    assert!(
        h.messages()
            .iter()
            .all(|message| !matches!(message, ControlMessage::OpenChanged(_)))
    );
}

#[test]
fn months_and_years_outside_the_range_are_disabled() {
    let mut calendar = calendar();
    calendar.set_range(Some(date(2026, 10, 5)), Some(date(2027, 2, 1)));
    let mut h = Harness::new(calendar);
    h.focus();
    h.key(Key::Enter);
    h.key(Key::Home);
    assert_eq!(
        h.semantics().description,
        "October",
        "earlier months are disabled"
    );
    h.key(Key::End);
    assert_eq!(h.semantics().description, "December");
    h.key(Key::Escape);
    h.key(Key::Tab);
    h.key(Key::Enter);
    h.key(Key::Home);
    assert_eq!(h.semantics().description, "2026");
    h.key(Key::End);
    assert_eq!(h.semantics().description, "2027");
}

#[test]
fn the_year_list_takes_its_ends_in_either_order() {
    let calendar = Calendar::new(Locale::parse("en").unwrap(), date(2026, 10, 12))
        .with_dropdown_caption(2030, 2024);
    assert!(calendar.has_dropdown_caption());
    let mut h = Harness::new(calendar);
    h.focus();
    h.key(Key::Tab);
    h.key(Key::Enter);
    h.key(Key::Home);
    assert_eq!(h.semantics().description, "2024");
    h.key(Key::End);
    assert_eq!(h.semantics().description, "2030");
}

#[test]
fn assistive_actions_open_and_close_the_focused_dropdown() {
    let mut h = Harness::new(calendar());
    h.focus();
    assert!(h.perform(SemanticAction::ShowMenu, None));
    assert_eq!(h.semantics().state.expanded, Some(true));
    assert!(h.semantics().actions.contains(&SemanticAction::Collapse));
    assert!(h.perform(SemanticAction::Collapse, None));
    assert_eq!(h.semantics().state.expanded, Some(false));
    h.key(Key::Tab);
    h.key(Key::Tab);
    assert!(
        !h.perform(SemanticAction::ShowMenu, None),
        "the grid has no menu"
    );
}

#[test]
fn right_to_left_puts_the_month_at_the_right() {
    let ltr = Harness::new(calendar());
    let painted = ltr.paint();
    assert!(text_x(&painted, "October") < text_x(&painted, "2026"));
    let rtl = Harness::with_direction(calendar(), LayoutDirection::Rtl);
    let painted = rtl.paint();
    assert!(text_x(&painted, "October") > text_x(&painted, "2026"));
}

#[test]
fn the_dropdowns_stay_between_the_month_buttons() {
    let calendar = Calendar::new(Locale::parse("en").unwrap(), date(2026, 9, 12))
        .with_dropdown_caption(2020, 2030);
    let h = Harness::new(calendar);
    let painted = h.paint();
    // The narrowed month trigger truncates its long name.
    assert!(!texts(&painted).contains(&"September".to_string()));
    let bounds = h.bounds();
    let (start, end) = (bounds.origin.x + 12.0 + 32.0, bounds.max_x() - 12.0 - 32.0);
    let month = text_x(&painted, "September");
    let year = text_x(&painted, "2026");
    assert!(month >= start, "{month} after {start}");
    assert!(year > month && year < end, "{year} before {end}");
}

#[test]
fn a_date_picker_passes_the_dropdown_caption_to_its_calendar() {
    let picker = DatePicker::new("Due date", Locale::parse("en").unwrap(), date(2026, 10, 12))
        .with_dropdown_caption(2020, 2030);
    assert!(picker.calendar().has_dropdown_caption());
    let mut h = Harness::new(picker);
    h.focus();
    h.key(Key::Enter);
    h.set_time(1.0);
    let painted = texts(&h.paint());
    assert!(painted.contains(&"October".to_string()));
    assert!(painted.contains(&"2026".to_string()));
    assert_eq!(h.semantics().description, "October 12, 2026", "the grid");

    // Shift+Tab reaches the year, then the month.
    shift_tab(&mut h);
    assert_eq!(h.semantics().description, "2026");
    shift_tab(&mut h);
    assert_eq!(h.semantics().description, "October");
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    assert_eq!(h.semantics().description, "November");
    // Escape closes the list and keeps the popover open.
    h.key(Key::Escape);
    assert_eq!(h.semantics().state.expanded, Some(true));
    h.key(Key::Enter);
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(h.semantics().description, "November");
    assert_eq!(h.semantics().state.expanded, Some(true));
    assert!(overlay(&h).is_some());

    // Tab goes to the year, the grid, then out, which closes the popover.
    h.key(Key::Tab);
    assert_eq!(h.semantics().description, "2026");
    h.key(Key::Tab);
    assert_ne!(h.semantics().description, "2026", "the grid");
    h.messages();
    h.key(Key::Tab);
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(false)]);
}
