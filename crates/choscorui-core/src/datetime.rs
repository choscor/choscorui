//! Proleptic Gregorian civil dates and wall-clock times with locale-aware
//! short formats. Time zones are an application concern.

use crate::{DateOrder, Locale, LocaleError};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CivilDate {
    year: i32,
    month: u8,
    day: u8,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CivilTime {
    hour: u8,
    minute: u8,
    second: u8,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CivilDateTime {
    pub date: CivilDate,
    pub time: CivilTime,
}

/// Day of week, Sunday = 0.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Weekday(pub u8);

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

impl CivilDate {
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, LocaleError> {
        if !(1..=9999).contains(&year) || day == 0 || day > days_in_month(year, month) {
            return Err(LocaleError::InvalidDate);
        }
        Ok(Self { year, month, day })
    }

    /// The nearest valid date: the year clamps to `1..=9999`, the month
    /// to `1..=12`, and the day to the days of that month. Use it for
    /// fixed dates in code; [`CivilDate::new`] reports invalid input.
    pub fn clamped(year: i32, month: u8, day: u8) -> Self {
        let year = year.clamp(1, 9999);
        let month = month.clamp(1, 12);
        let day = day.clamp(1, days_in_month(year, month));
        Self { year, month, day }
    }

    pub fn year(self) -> i32 {
        self.year
    }
    pub fn month(self) -> u8 {
        self.month
    }
    pub fn day(self) -> u8 {
        self.day
    }

    /// Days since 1970-01-01 (Howard Hinnant's civil algorithm).
    pub fn days_since_epoch(self) -> i64 {
        let year = i64::from(self.year) - i64::from(self.month <= 2);
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let month = i64::from(self.month);
        let day_of_year =
            (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    pub fn from_days_since_epoch(days: i64) -> Result<Self, LocaleError> {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let day_of_era = z - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let mp = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        let year = i32::try_from(year).map_err(|_| LocaleError::InvalidDate)?;
        Self::new(year, month as u8, day as u8)
    }

    pub fn weekday(self) -> Weekday {
        // 1970-01-01 was a Thursday.
        Weekday((self.days_since_epoch() + 4).rem_euclid(7) as u8)
    }

    pub fn add_days(self, days: i64) -> Result<Self, LocaleError> {
        Self::from_days_since_epoch(self.days_since_epoch() + days)
    }

    /// Adds months, clamping the day to the target month's length.
    pub fn add_months(self, months: i32) -> Result<Self, LocaleError> {
        let index = self.year * 12 + i32::from(self.month) - 1 + months;
        let year = index.div_euclid(12);
        let month = (index.rem_euclid(12) + 1) as u8;
        let day = self.day.min(days_in_month(year, month));
        Self::new(year, month, day)
    }

    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    pub fn parse_iso(text: &str) -> Result<Self, LocaleError> {
        let mut parts = text.trim().split('-');
        let (Some(year), Some(month), Some(day), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(LocaleError::InvalidDate);
        };
        if year.len() != 4 || month.len() != 2 || day.len() != 2 {
            return Err(LocaleError::InvalidDate);
        }
        Self::new(number(year)?, number(month)?, number(day)?)
    }

    /// Numeric short date in the locale's field order and separator.
    pub fn format_short(self, locale: &Locale) -> String {
        let data = locale.data();
        let separator = data.date_separator;
        let (year, month, day) = (
            format!("{:04}", self.year),
            format!("{:02}", self.month),
            format!("{:02}", self.day),
        );
        let fields = match data.date_order {
            DateOrder::DayMonthYear => [day, month, year],
            DateOrder::MonthDayYear => [month, day, year],
            DateOrder::YearMonthDay => [year, month, day],
        };
        fields.join(&separator.to_string())
    }

    /// Parses a short date. Accepts the locale separator, `/`, `.`, or `-`,
    /// and one- or two-digit day and month fields. Years need four digits.
    pub fn parse_short(text: &str, locale: &Locale) -> Result<Self, LocaleError> {
        let fields: Vec<&str> = text
            .trim()
            .split(|c: char| c == locale.data().date_separator || matches!(c, '/' | '.' | '-'))
            .collect();
        let [first, second, third] = fields.as_slice() else {
            return Err(LocaleError::InvalidDate);
        };
        let (year, month, day) = match locale.data().date_order {
            DateOrder::DayMonthYear => (third, second, first),
            DateOrder::MonthDayYear => (third, first, second),
            DateOrder::YearMonthDay => (first, second, third),
        };
        if year.len() != 4 || !(1..=2).contains(&month.len()) || !(1..=2).contains(&day.len()) {
            return Err(LocaleError::InvalidDate);
        }
        Self::new(number(year)?, number(month)?, number(day)?)
    }

    /// Long format with a localized month name where the toolkit has data.
    pub fn format_long(self, locale: &Locale) -> String {
        let month = month_name(locale, self.month);
        match locale.language() {
            "en" => format!("{month} {}, {}", self.day, self.year),
            "vi" => format!("{} {month}, {}", self.day, self.year),
            "fr" | "es" | "it" | "pt" => format!("{} {month} {}", self.day, self.year),
            "de" => format!("{}. {month} {}", self.day, self.year),
            "ja" | "zh" => format!("{}年{}月{}日", self.year, self.month, self.day),
            _ => self.iso(),
        }
    }
}

impl fmt::Display for CivilDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.iso())
    }
}

pub fn month_name(locale: &Locale, month: u8) -> String {
    const EN: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const FR: [&str; 12] = [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ];
    const DE: [&str; 12] = [
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ];
    const ES: [&str; 12] = [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ];
    let index = usize::from(month.clamp(1, 12) - 1);
    match locale.language() {
        "fr" => FR[index].to_owned(),
        "de" => DE[index].to_owned(),
        "es" => ES[index].to_owned(),
        "vi" => format!("tháng {}", index + 1),
        "ja" | "zh" => format!("{}月", index + 1),
        _ => EN[index].to_owned(),
    }
}

/// Short weekday label for calendar headers.
pub fn weekday_short_name(locale: &Locale, weekday: Weekday) -> &'static str {
    const EN: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const VI: [&str; 7] = ["CN", "T2", "T3", "T4", "T5", "T6", "T7"];
    const FR: [&str; 7] = ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."];
    const DE: [&str; 7] = ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"];
    const JA: [&str; 7] = ["日", "月", "火", "水", "木", "金", "土"];
    let index = usize::from(weekday.0 % 7);
    match locale.language() {
        "vi" => VI[index],
        "fr" => FR[index],
        "de" => DE[index],
        "ja" | "zh" => JA[index],
        _ => EN[index],
    }
}

impl CivilTime {
    pub fn new(hour: u8, minute: u8, second: u8) -> Result<Self, LocaleError> {
        if hour > 23 || minute > 59 || second > 59 {
            return Err(LocaleError::InvalidDate);
        }
        Ok(Self {
            hour,
            minute,
            second,
        })
    }

    /// The nearest valid time: the hour clamps to 23, and the minute and
    /// the second to 59. [`CivilTime::new`] reports invalid input.
    pub fn clamped(hour: u8, minute: u8, second: u8) -> Self {
        Self {
            hour: hour.min(23),
            minute: minute.min(59),
            second: second.min(59),
        }
    }

    pub fn hour(self) -> u8 {
        self.hour
    }
    pub fn minute(self) -> u8 {
        self.minute
    }
    pub fn second(self) -> u8 {
        self.second
    }
    pub fn seconds_since_midnight(self) -> u32 {
        u32::from(self.hour) * 3600 + u32::from(self.minute) * 60 + u32::from(self.second)
    }

    /// Wraps around midnight.
    pub fn add_seconds(self, seconds: i64) -> Self {
        let total = (i64::from(self.seconds_since_midnight()) + seconds).rem_euclid(86_400);
        Self {
            hour: (total / 3600) as u8,
            minute: (total / 60 % 60) as u8,
            second: (total % 60) as u8,
        }
    }

    /// `HH:MM` in 24-hour locales, `h:MM AM` otherwise.
    pub fn format_short(self, locale: &Locale) -> String {
        if locale.data().hour_cycle_24 {
            format!("{:02}:{:02}", self.hour, self.minute)
        } else {
            let suffix = if self.hour < 12 { "AM" } else { "PM" };
            let hour = match self.hour % 12 {
                0 => 12,
                hour => hour,
            };
            format!("{hour}:{:02} {suffix}", self.minute)
        }
    }

    /// Parses `H:MM`, `H:MM:SS`, and optional `AM`/`PM` in any locale.
    pub fn parse(text: &str) -> Result<Self, LocaleError> {
        let text = text.trim();
        let upper = text.to_ascii_uppercase();
        let (clock, meridiem) = if let Some(rest) = upper.strip_suffix("AM") {
            (rest.trim_end().to_owned(), Some(false))
        } else if let Some(rest) = upper.strip_suffix("PM") {
            (rest.trim_end().to_owned(), Some(true))
        } else {
            (upper, None)
        };
        let fields: Vec<&str> = clock.split(':').collect();
        if !(2..=3).contains(&fields.len()) || fields[1..].iter().any(|field| field.len() != 2) {
            return Err(LocaleError::InvalidDate);
        }
        let mut hour: u8 = number(fields[0])?;
        let minute = number(fields[1])?;
        let second = fields.get(2).map_or(Ok(0), |field| number(field))?;
        if let Some(pm) = meridiem {
            if !(1..=12).contains(&hour) {
                return Err(LocaleError::InvalidDate);
            }
            hour = hour % 12 + if pm { 12 } else { 0 };
        }
        Self::new(hour, minute, second)
    }
}

fn number<T: std::str::FromStr>(text: &str) -> Result<T, LocaleError> {
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return Err(LocaleError::InvalidDate);
    }
    text.parse().map_err(|_| LocaleError::InvalidDate)
}

/// Six-week grid of dates for a month view, starting on the locale's first
/// weekday. Days outside `month` are included for a stable layout.
pub fn month_grid(year: i32, month: u8, locale: &Locale) -> Result<[CivilDate; 42], LocaleError> {
    let first = CivilDate::new(year, month, 1)?;
    let offset =
        (i64::from(first.weekday().0) - i64::from(locale.data().first_weekday)).rem_euclid(7);
    let start = first.add_days(-offset)?;
    let mut grid = [start; 42];
    for (index, cell) in grid.iter_mut().enumerate() {
        *cell = start.add_days(index as i64)?;
    }
    Ok(grid)
}
