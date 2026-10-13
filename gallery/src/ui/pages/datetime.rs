//! Dates and times: the month calendar and sectioned date/time editors.

use crate::ui::page::{Build, PageCx, caption, column, section};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Binding, Calendar, Cx, DateTimeEdit, Dynamic, FieldText, Frame, HorizontalAlignment,
    Text, VStack, View, ViewExt,
};
use choscorui::controls::{CalendarMode, ControlMessage, ControlValue, DateTimeKind, FieldPart};
use choscorui::core::{CivilDate, CivilDateTime, CivilTime, Space, TextPreset};

fn date(year: i32, month: u8, day: u8) -> CivilDate {
    CivilDate::clamped(year, month, day)
}

pub(crate) fn date_time(day: CivilDate, hour: u8, minute: u8) -> CivilDateTime {
    CivilDateTime {
        date: day,
        time: CivilTime::clamped(hour, minute, 0),
    }
}

/// The day the demo treats as today.
pub(crate) fn today() -> CivilDate {
    date(2026, 10, 12)
}

/// A month calendar limited to 2026, with previous and next month
/// buttons. A click, or Return on the selected day, chooses a day. Below
/// it, calendars choose several days, choose a range, and pick the month
/// and year from dropdowns.
struct CalendarPage;

impl View for CalendarPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let selected = cx.state(today());
        let chosen = cx.state(None::<CivilDate>);
        let locale = cx.env().locale();
        let name = cx.env().text("calendar.name", &[]);
        let meetings = cx.state(days(&[8, 15, 22]));
        let trip = cx.state(days(&[6, 10]));
        let birthday = cx.state(today());
        let calendar = Calendar::new(selected.binding())
            .with(move |mut calendar| {
                calendar.set_range(Some(date(2026, 1, 1)), Some(date(2026, 12, 31)));
                calendar.with_name(name.clone()).with_today(today())
            })
            .on_message(move |message| match message {
                ControlMessage::Changed(ControlValue::Date(Some(day))) => chosen.set(Some(*day)),
                ControlMessage::Activated => chosen.set(selected.try_get().ok()),
                _ => {}
            });
        column((
            calendar,
            Dynamic::new(move || {
                chosen.get().map(|day| {
                    Text::new(t!("calendar.chosen", date = day.format_long(&locale)))
                        .text_style(TextPreset::Small)
                })
            }),
            section(
                t!("calendar.multiple"),
                chooser(
                    "calendar.meetings",
                    CalendarMode::Multiple,
                    meetings.binding(),
                ),
            ),
            section(
                t!("calendar.range"),
                chooser("calendar.trip", CalendarMode::Range, trip.binding()),
            ),
            section(
                t!("calendar.dropdown"),
                Calendar::new(birthday.binding()).with_env(|calendar, env| {
                    calendar
                        .with_name(env.text("calendar.birthday", &[]))
                        .with_today(today())
                        .with_dropdown_caption(1926, 2026)
                }),
            ),
        ))
    }
}

/// The October 2026 `days`.
fn days(days: &[u8]) -> Vec<CivilDate> {
    days.iter().map(|day| date(2026, 10, *day)).collect()
}

/// A calendar named by the catalog string `name` that chooses `chosen`
/// days as `mode` says.
fn chooser(name: &'static str, mode: CalendarMode, chosen: Binding<Vec<CivilDate>>) -> impl View {
    Calendar::dates(today(), mode, chosen)
        .with_env(move |calendar, env| calendar.with_name(env.text(name, &[])).with_today(today()))
}

pub fn calendar(_: &PageCx) -> AnyView {
    CalendarPage.into_any()
}

/// The English label of the range-limited editor, `date-time-edit.limited`
/// in the catalogs; scripted checks find it by this name.
pub(crate) const LIMITED: &str = "Meeting (Oct 2026, 09:00 – 17:00 on the 12th)";

/// The office hours that the limited editor allows.
fn limits() -> (CivilDateTime, CivilDateTime) {
    (date_time(today(), 9, 0), date_time(today(), 17, 0))
}

/// The value in the ISO 8601 text that a date and time editor reports,
/// `YYYY-MM-DDTHH:MM:SS`.
fn parse_iso(text: &str) -> Option<CivilDateTime> {
    let (day, time) = text.split_once('T')?;
    Some(CivilDateTime {
        date: CivilDate::parse_iso(day).ok()?,
        time: CivilTime::parse(time).ok()?,
    })
}

/// One editor of `kind`, named by the catalog string `key`, that edits
/// `value`.
fn editor(key: &'static str, kind: DateTimeKind, value: Binding<CivilDateTime>) -> DateTimeEdit {
    DateTimeEdit::new(t!(key), kind, value).with(move |mut edit| {
        if kind == DateTimeKind::DateTime {
            let (minimum, maximum) = limits();
            edit.set_range(Some(minimum), Some(maximum));
        }
        edit
    })
}

/// An editor under its caption. The editor has the same name, so the
/// caption stays out of the accessibility tree.
fn labeled(key: &'static str, content: impl View) -> impl View {
    VStack::new((caption(t!(key)).accessibility_hidden(true), content))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S2)
}

/// Date, time, and date-time editors; the last is limited to office hours
/// on the demo day and reports out-of-range values until Return clamps
/// them.
struct DateTimeEditPage;

impl View for DateTimeEditPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let outside = cx.state(false);
        // Each editor starts at 09:30 on the demo day.
        let mut start = || cx.state(date_time(today(), 9, 30)).binding();
        let (day, time, meeting) = (start(), start(), start());
        let width = || Frame::new().width(260.0);
        let limited = editor("date-time-edit.limited", DateTimeKind::DateTime, meeting).on_message(
            move |message| {
                if let ControlMessage::Changed(ControlValue::Text(text)) = message
                    && let Some(value) = parse_iso(text)
                {
                    let (minimum, maximum) = limits();
                    outside.set(value < minimum || value > maximum);
                }
            },
        );
        column((
            labeled(
                "date-time-edit.date",
                editor("date-time-edit.date", DateTimeKind::Date, day).frame(width()),
            ),
            labeled(
                "date-time-edit.time",
                editor("date-time-edit.time", DateTimeKind::Time, time).frame(width()),
            ),
            labeled(
                "date-time-edit.limited",
                VStack::new((
                    limited.frame(width()),
                    FieldText::new(FieldPart::Error, t!("date-time-edit.outside"))
                        .hidden(move || !outside.get()),
                ))
                .alignment(HorizontalAlignment::Leading)
                .spacing(Space::S1),
            ),
        ))
    }
}

pub fn date_time_edit(_: &PageCx) -> AnyView {
    DateTimeEditPage.into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "calendar" => calendar,
        "date-time-edit" => date_time_edit,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{LIMITED, date_time, today};
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::{DateTimeEdit, DateTimeKind, ShortcutModifiers};
    use choscorui::core::{Key, KeyEvent, Locale, Localizer, SemanticAction};

    fn press(page: &mut Harness, key: Key) {
        let none = ShortcutModifiers::default();
        page.host.key(KeyEvent::Down { key, repeat: false }, none);
        page.host.key(KeyEvent::Up { key }, none);
        page.host.layout();
    }

    /// What the limited editor reads at `hour`:00 on the demo day.
    fn reads(hour: u8) -> Option<String> {
        let locale = Locale::parse("en").unwrap();
        let at = date_time(today(), hour, 0);
        DateTimeEdit::new(LIMITED, locale, DateTimeKind::DateTime, at)
            .semantics(Localizer::builtin())
            .value
    }

    fn invalid(page: &mut Harness) -> bool {
        let node = page.node(LIMITED);
        page.host.semantic(node).unwrap().state.invalid
    }

    #[test]
    fn the_limited_editor_clamps_hours_outside_office_hours() {
        let mut page = Harness::new("date-time-edit");
        let message = "Outside the allowed range; Return clamps it";
        assert!(page.host.find(message).is_none());
        assert!(page.act(LIMITED, SemanticAction::Focus));
        for _ in 0..5 {
            let node = page.node(LIMITED);
            let description = page.host.semantic(node).unwrap().description;
            if description == "Hour field" {
                break;
            }
            press(&mut page, Key::ArrowRight);
        }
        // Stepping stops at the start of office hours.
        press(&mut page, Key::ArrowDown);
        press(&mut page, Key::ArrowDown);
        assert_eq!(page.value(LIMITED), reads(9));
        assert!(!invalid(&mut page));
        assert!(page.act(LIMITED, SemanticAction::Decrement));
        assert_eq!(page.value(LIMITED), reads(9));
        // A typed hour outside the range is invalid until Return clamps it.
        assert!(page.host.text("8"));
        page.host.layout();
        assert!(invalid(&mut page));
        assert!(page.host.find(message).is_some());
        press(&mut page, Key::Enter);
        assert_eq!(page.value(LIMITED), reads(9));
        assert!(!invalid(&mut page));
        assert!(page.host.find(message).is_none());
    }

    #[test]
    fn choosing_a_day_shows_it() {
        let mut page = Harness::new("calendar");
        assert!(page.act("Calendar", SemanticAction::Focus));
        press(&mut page, Key::ArrowRight);
        assert!(page.host.find("Chosen: October 13, 2026").is_some());
    }
}
