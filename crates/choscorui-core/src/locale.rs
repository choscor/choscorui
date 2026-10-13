//! Locales, plural rules, message catalogs, and locale-aware numbers.
//!
//! The toolkit ships formatting data for a documented set of languages and
//! falls back to the root locale (ISO dates, `.` decimal separator) for the
//! rest. Applications supply translations as [`Catalog`] entries.

use crate::LayoutDirection;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::sync::{Arc, OnceLock};

/// A parsed BCP 47 language tag, reduced to language and optional region.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Locale {
    language: String,
    region: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocaleError {
    InvalidTag(String),
    MissingMessage { locale: String, id: String },
    MissingArgument { id: String, name: String },
    InvalidNumber,
    InvalidDate,
}

impl fmt::Display for LocaleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTag(tag) => write!(formatter, "invalid locale tag `{tag}`"),
            Self::MissingMessage { locale, id } => {
                write!(formatter, "message `{id}` missing for `{locale}`")
            }
            Self::MissingArgument { id, name } => {
                write!(formatter, "message `{id}` needs argument `{name}`")
            }
            Self::InvalidNumber => formatter.write_str("invalid number"),
            Self::InvalidDate => formatter.write_str("invalid date"),
        }
    }
}

impl std::error::Error for LocaleError {}

/// Order of numeric date fields in short dates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DateOrder {
    DayMonthYear,
    MonthDayYear,
    YearMonthDay,
}

/// Formatting conventions resolved for a locale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocaleData {
    pub decimal: char,
    /// Grouping separator; `None` disables grouping.
    pub group: Option<char>,
    pub date_order: DateOrder,
    pub date_separator: char,
    pub hour_cycle_24: bool,
    pub direction: LayoutDirection,
    /// 0 = Sunday, 1 = Monday.
    pub first_weekday: u8,
}

const ROOT_DATA: LocaleData = LocaleData {
    decimal: '.',
    group: Some(','),
    date_order: DateOrder::YearMonthDay,
    date_separator: '-',
    hour_cycle_24: true,
    direction: LayoutDirection::Ltr,
    first_weekday: 1,
};

impl Locale {
    /// Parses tags such as `en`, `en-US`, `vi_VN`, or `zh-Hant-TW`. Script
    /// subtags are accepted and ignored; extensions are rejected.
    pub fn parse(tag: &str) -> Result<Self, LocaleError> {
        let invalid = || LocaleError::InvalidTag(tag.to_owned());
        let mut parts = tag.split(['-', '_']);
        let language = parts
            .next()
            .filter(|part| !part.is_empty())
            .ok_or_else(invalid)?;
        if !(2..=3).contains(&language.len()) || !language.chars().all(|c| c.is_ascii_alphabetic())
        {
            return Err(invalid());
        }
        let mut region = None;
        for part in parts {
            let alphabetic = part.chars().all(|c| c.is_ascii_alphabetic());
            let numeric = part.chars().all(|c| c.is_ascii_digit());
            match part.len() {
                4 if alphabetic && region.is_none() => {} // script
                2 if alphabetic && region.is_none() => region = Some(part.to_ascii_uppercase()),
                3 if numeric && region.is_none() => region = Some(part.to_owned()),
                _ => return Err(invalid()),
            }
        }
        Ok(Self {
            language: language.to_ascii_lowercase(),
            region,
        })
    }

    /// The root locale, `und`, with no language data.
    pub fn root() -> Self {
        Self {
            language: "und".into(),
            region: None,
        }
    }

    /// The lowercase language code, for example `en`.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// The uppercase region code, for example `US`, when the locale has one.
    pub fn region(&self) -> Option<&str> {
        self.region.as_deref()
    }

    /// The BCP 47 tag, for example `en-US`.
    pub fn tag(&self) -> String {
        match &self.region {
            Some(region) => format!("{}-{region}", self.language),
            None => self.language.clone(),
        }
    }

    /// Lookup chain from most to least specific, ending at the root.
    pub fn fallback_chain(&self) -> Vec<Locale> {
        let mut chain = vec![self.clone()];
        if self.region.is_some() {
            chain.push(Self {
                language: self.language.clone(),
                region: None,
            });
        }
        if self.language != "und" {
            chain.push(Self::root());
        }
        chain
    }

    /// The layout direction of the language.
    pub fn direction(&self) -> LayoutDirection {
        self.data().direction
    }

    /// The number, date, and calendar conventions of the locale.
    pub fn data(&self) -> LocaleData {
        let region = self.region.as_deref();
        let dmy = |decimal, group, separator| LocaleData {
            decimal,
            group,
            date_order: DateOrder::DayMonthYear,
            date_separator: separator,
            hour_cycle_24: true,
            direction: LayoutDirection::Ltr,
            first_weekday: 1,
        };
        match self.language.as_str() {
            "en" => match region {
                Some("US") | None => LocaleData {
                    date_order: DateOrder::MonthDayYear,
                    date_separator: '/',
                    hour_cycle_24: false,
                    first_weekday: 0,
                    ..ROOT_DATA
                },
                _ => LocaleData {
                    hour_cycle_24: region != Some("AU"),
                    ..dmy('.', Some(','), '/')
                },
            },
            "vi" => dmy(',', Some('.'), '/'),
            "de" | "da" | "nb" | "fi" => dmy(',', Some('.'), '.'),
            "ru" | "uk" | "pl" | "cs" => dmy(',', Some('\u{a0}'), '.'),
            "fr" => dmy(',', Some('\u{202f}'), '/'),
            "es" | "it" | "pt" => dmy(',', Some('.'), '/'),
            "nl" => dmy(',', Some('.'), '-'),
            "ja" | "zh" => LocaleData {
                date_separator: '/',
                first_weekday: 0,
                ..ROOT_DATA
            },
            "ko" => LocaleData {
                date_separator: '.',
                first_weekday: 0,
                hour_cycle_24: false,
                ..ROOT_DATA
            },
            "ar" => LocaleData {
                direction: LayoutDirection::Rtl,
                first_weekday: 6,
                ..dmy('.', Some(','), '/')
            },
            "he" | "fa" | "ur" => LocaleData {
                direction: LayoutDirection::Rtl,
                first_weekday: 0,
                ..dmy('.', Some(','), '/')
            },
            _ => ROOT_DATA,
        }
    }

    /// The plural category of `count` in the language.
    pub fn plural(&self, count: u64) -> PluralCategory {
        plural_category(&self.language, count)
    }

    /// Categories a complete translation must provide for this language.
    pub fn plural_categories(&self) -> BTreeSet<PluralCategory> {
        use PluralCategory::*;
        match plural_family(&self.language) {
            PluralFamily::Other => [Other].into(),
            PluralFamily::OneOther | PluralFamily::ZeroOneOther => [One, Other].into(),
            PluralFamily::EastSlavic | PluralFamily::Polish => [One, Few, Many, Other].into(),
            PluralFamily::Czech => [One, Few, Other].into(),
            PluralFamily::Arabic => [Zero, One, Two, Few, Many, Other].into(),
            PluralFamily::Hebrew => [One, Two, Other].into(),
        }
    }

    /// Formats a signed integer with locale grouping.
    pub fn format_integer(&self, value: i64) -> String {
        let data = self.data();
        let digits = value.unsigned_abs().to_string();
        let mut grouped = group_digits(&digits, data.group);
        if value < 0 {
            grouped.insert(0, '-');
        }
        grouped
    }

    /// Formats a decimal with exactly `fraction_digits` digits, rounding
    /// half away from zero.
    pub fn format_decimal(
        &self,
        value: f64,
        fraction_digits: usize,
    ) -> Result<String, LocaleError> {
        if !value.is_finite() || fraction_digits > 15 {
            return Err(LocaleError::InvalidNumber);
        }
        let data = self.data();
        let text = format!("{:.*}", fraction_digits, value.abs());
        let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
        let mut out = String::new();
        if value < 0.0 && text.chars().any(|c| c.is_ascii_digit() && c != '0') {
            out.push('-');
        }
        out.push_str(&group_digits(whole, data.group));
        if !fraction.is_empty() {
            out.push(data.decimal);
            out.push_str(fraction);
        }
        Ok(out)
    }

    /// Parses user input. Group separators are optional but, when present,
    /// must separate groups of three digits. ASCII `.` is accepted as the
    /// decimal separator only when it is not this locale's group separator.
    pub fn parse_decimal(&self, input: &str) -> Result<f64, LocaleError> {
        let data = self.data();
        let text = input.trim();
        let (negative, body) = match text.strip_prefix(['-', '\u{2212}']) {
            Some(rest) => (true, rest),
            None => (false, text.strip_prefix('+').unwrap_or(text)),
        };
        let decimal_alias = (data.group != Some('.') && data.decimal != '.').then_some('.');
        let split = body.find(|c| c == data.decimal || Some(c) == decimal_alias);
        let (whole, fraction) = match split {
            Some(index) => {
                let separator_len = body[index..].chars().next().map_or(1, char::len_utf8);
                (&body[..index], Some(&body[index + separator_len..]))
            }
            None => (body, None),
        };
        let is_group = |c: char| {
            Some(c) == data.group
                || (data.group.is_some_and(|g| g.is_whitespace()) && c.is_whitespace())
        };
        let groups: Vec<&str> = whole.split(is_group).collect();
        if groups
            .iter()
            .any(|group| group.is_empty() || !group.chars().all(|c| c.is_ascii_digit()))
        {
            return Err(LocaleError::InvalidNumber);
        }
        if groups.len() > 1
            && (groups[0].len() > 3 || groups[1..].iter().any(|group| group.len() != 3))
        {
            return Err(LocaleError::InvalidNumber);
        }
        let mut canonical: String = groups.concat();
        if let Some(fraction) = fraction {
            if fraction.is_empty() || !fraction.chars().all(|c| c.is_ascii_digit()) {
                return Err(LocaleError::InvalidNumber);
            }
            canonical.push('.');
            canonical.push_str(fraction);
        }
        let value: f64 = canonical.parse().map_err(|_| LocaleError::InvalidNumber)?;
        Ok(if negative { -value } else { value })
    }

    /// Parses an integer in this locale, rejecting fractions.
    pub fn parse_integer(&self, input: &str) -> Result<i64, LocaleError> {
        let value = self.parse_decimal(input)?;
        if value.fract() != 0.0 || value.abs() > 9.007_199_254_740_992e15 {
            return Err(LocaleError::InvalidNumber);
        }
        Ok(value as i64)
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.tag())
    }
}

fn group_digits(digits: &str, group: Option<char>) -> String {
    let Some(group) = group else {
        return digits.to_owned();
    };
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(group);
        }
        out.push(digit);
    }
    out
}

/// CLDR plural category for cardinal integer counts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

enum PluralFamily {
    Other,
    OneOther,
    ZeroOneOther,
    EastSlavic,
    Polish,
    Czech,
    Arabic,
    Hebrew,
}

fn plural_family(language: &str) -> PluralFamily {
    match language {
        "vi" | "ja" | "zh" | "ko" | "th" | "id" | "ms" | "lo" | "my" => PluralFamily::Other,
        "fr" | "pt" | "hi" | "fa" => PluralFamily::ZeroOneOther,
        "ru" | "uk" | "be" => PluralFamily::EastSlavic,
        "pl" => PluralFamily::Polish,
        "cs" | "sk" => PluralFamily::Czech,
        "ar" => PluralFamily::Arabic,
        "he" => PluralFamily::Hebrew,
        _ => PluralFamily::OneOther,
    }
}

fn plural_category(language: &str, n: u64) -> PluralCategory {
    use PluralCategory::*;
    let (n10, n100) = (n % 10, n % 100);
    match plural_family(language) {
        PluralFamily::Other => Other,
        PluralFamily::OneOther => {
            if n == 1 {
                One
            } else {
                Other
            }
        }
        PluralFamily::ZeroOneOther => {
            if n <= 1 {
                One
            } else {
                Other
            }
        }
        PluralFamily::EastSlavic => {
            if n10 == 1 && n100 != 11 {
                One
            } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
                Few
            } else {
                Many
            }
        }
        PluralFamily::Polish => {
            if n == 1 {
                One
            } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
                Few
            } else {
                Many
            }
        }
        PluralFamily::Czech => match n {
            1 => One,
            2..=4 => Few,
            _ => Other,
        },
        PluralFamily::Arabic => match n {
            0 => Zero,
            1 => One,
            2 => Two,
            _ if (3..=10).contains(&n100) => Few,
            _ if (11..=99).contains(&n100) => Many,
            _ => Other,
        },
        PluralFamily::Hebrew => match n {
            1 => One,
            2 => Two,
            _ => Other,
        },
    }
}

/// One translatable message, optionally with plural variants.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Text(String),
    Plural(BTreeMap<PluralCategory, String>),
}

/// A problem found by [`Catalog::validate`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogIssue {
    MissingMessage {
        locale: String,
        id: String,
    },
    PlaceholderMismatch {
        locale: String,
        id: String,
    },
    MissingPluralCategory {
        locale: String,
        id: String,
        category: PluralCategory,
    },
    UnknownMessage {
        locale: String,
        id: String,
    },
}

/// Translations keyed by locale tag and message id.
///
/// Messages use `{name}` placeholders. `{{` and `}}` are literal braces.
/// Plural messages select a variant with [`Catalog::plural`] and receive the
/// formatted count as `{count}`.
/// A clone shares the messages until one of the catalogs changes.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    messages: Arc<HashMap<String, BTreeMap<String, Message>>>,
}

impl Catalog {
    /// An empty catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// The built-in messages of the toolkit, with `choscorui.*` ids, in
    /// English and Vietnamese. The English messages are also the messages
    /// of the root locale, so every other locale falls back to them.
    /// [`Localizer`] uses them for each id that the application catalog
    /// does not define.
    pub fn builtin() -> &'static Catalog {
        static BUILTIN: OnceLock<Catalog> = OnceLock::new();
        BUILTIN.get_or_init(|| {
            const ENGLISH: &str = include_str!("../resources/strings/en.lang");
            const VIETNAMESE: &str = include_str!("../resources/strings/vi.lang");
            let mut catalog = Catalog::new();
            for (tag, source) in [("und", ENGLISH), ("en", ENGLISH), ("vi", VIETNAMESE)] {
                if let Ok(locale) = Locale::parse(tag) {
                    // The files are part of the crate, and a test loads
                    // each one without an error.
                    let _ = catalog.load_str(&locale, source);
                }
            }
            catalog
        })
    }

    /// Adds the text of message `id` in `locale`.
    pub fn insert(&mut self, locale: &Locale, id: impl Into<String>, text: impl Into<String>) {
        Arc::make_mut(&mut self.messages)
            .entry(locale.tag())
            .or_default()
            .insert(id.into(), Message::Text(text.into()));
    }

    /// Adds the plural variants of message `id` in `locale`.
    pub fn insert_plural(
        &mut self,
        locale: &Locale,
        id: impl Into<String>,
        variants: impl IntoIterator<Item = (PluralCategory, impl Into<String>)>,
    ) {
        let variants = variants
            .into_iter()
            .map(|(category, text)| (category, text.into()))
            .collect();
        Arc::make_mut(&mut self.messages)
            .entry(locale.tag())
            .or_default()
            .insert(id.into(), Message::Plural(variants));
    }

    /// Parses `id = text` lines. Blank lines and `#` comments are ignored.
    /// Plural variants use `id[one] = text`. Values may use `\n` escapes.
    pub fn load_str(&mut self, locale: &Locale, source: &str) -> Result<usize, (usize, String)> {
        let mut count = 0;
        for (index, raw) in source.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| (index + 1, "expected `id = text`".to_owned()))?;
            let key = key.trim();
            let value = value.trim().replace("\\n", "\n");
            if let Some((id, category)) = key.strip_suffix(']').and_then(|k| k.split_once('[')) {
                let category = match category {
                    "zero" => PluralCategory::Zero,
                    "one" => PluralCategory::One,
                    "two" => PluralCategory::Two,
                    "few" => PluralCategory::Few,
                    "many" => PluralCategory::Many,
                    "other" => PluralCategory::Other,
                    other => return Err((index + 1, format!("unknown plural category `{other}`"))),
                };
                let entry = Arc::make_mut(&mut self.messages)
                    .entry(locale.tag())
                    .or_default()
                    .entry(id.to_owned())
                    .or_insert_with(|| Message::Plural(BTreeMap::new()));
                match entry {
                    Message::Plural(variants) => {
                        variants.insert(category, value);
                    }
                    Message::Text(_) => {
                        return Err((index + 1, format!("`{id}` is not a plural message")));
                    }
                }
            } else if key.is_empty() || key.contains(char::is_whitespace) {
                return Err((index + 1, "invalid message id".to_owned()));
            } else {
                self.insert(locale, key, value);
            }
            count += 1;
        }
        Ok(count)
    }

    fn lookup(&self, locale: &Locale, id: &str) -> Option<(&Message, Locale)> {
        locale.fallback_chain().into_iter().find_map(|candidate| {
            self.messages
                .get(&candidate.tag())
                .and_then(|messages| messages.get(id))
                .map(|message| (message, candidate))
        })
    }

    /// Resolves a plain message through the locale fallback chain.
    pub fn text(
        &self,
        locale: &Locale,
        id: &str,
        args: &[(&str, &str)],
    ) -> Result<String, LocaleError> {
        let missing = || LocaleError::MissingMessage {
            locale: locale.tag(),
            id: id.to_owned(),
        };
        let (message, _) = self.lookup(locale, id).ok_or_else(missing)?;
        let template = match message {
            Message::Text(text) => text.as_str(),
            Message::Plural(variants) => {
                variants.get(&PluralCategory::Other).ok_or_else(missing)?
            }
        };
        substitute(id, template, args)
    }

    /// Resolves a plural message for `count` using the plural rules of the
    /// locale that supplied the translation.
    pub fn plural(
        &self,
        locale: &Locale,
        id: &str,
        count: u64,
        args: &[(&str, &str)],
    ) -> Result<String, LocaleError> {
        let missing = || LocaleError::MissingMessage {
            locale: locale.tag(),
            id: id.to_owned(),
        };
        let (message, source) = self.lookup(locale, id).ok_or_else(missing)?;
        let template = match message {
            Message::Text(text) => text.as_str(),
            Message::Plural(variants) => variants
                .get(&source.plural(count))
                .or_else(|| variants.get(&PluralCategory::Other))
                .ok_or_else(missing)?,
        };
        let formatted = locale.format_integer(count.min(i64::MAX as u64) as i64);
        let mut all: Vec<(&str, &str)> = vec![("count", formatted.as_str())];
        all.extend_from_slice(args);
        substitute(id, template, &all)
    }

    /// The tags of the locales that have messages.
    pub fn locales(&self) -> impl Iterator<Item = &str> {
        self.messages.keys().map(String::as_str)
    }

    /// Compares every locale against `reference`: each reference id must
    /// exist with the same placeholders, plural messages must cover the
    /// language's categories, and extra ids are reported as unknown.
    pub fn validate(&self, reference: &Locale) -> Vec<CatalogIssue> {
        let mut issues = Vec::new();
        let empty = BTreeMap::new();
        let base = self.messages.get(&reference.tag()).unwrap_or(&empty);
        let mut tags: Vec<&String> = self.messages.keys().collect();
        tags.sort();
        for tag in tags {
            let messages = &self.messages[tag];
            let Ok(locale) = Locale::parse(tag) else {
                continue;
            };
            for (id, reference_message) in base {
                let Some(message) = messages.get(id) else {
                    if *tag != reference.tag() {
                        issues.push(CatalogIssue::MissingMessage {
                            locale: tag.clone(),
                            id: id.clone(),
                        });
                    }
                    continue;
                };
                if placeholders(reference_message) != placeholders(message) {
                    issues.push(CatalogIssue::PlaceholderMismatch {
                        locale: tag.clone(),
                        id: id.clone(),
                    });
                }
                if let Message::Plural(variants) = message {
                    for category in locale.plural_categories() {
                        if !variants.contains_key(&category) {
                            issues.push(CatalogIssue::MissingPluralCategory {
                                locale: tag.clone(),
                                id: id.clone(),
                                category,
                            });
                        }
                    }
                }
            }
            for id in messages.keys() {
                if !base.contains_key(id) {
                    issues.push(CatalogIssue::UnknownMessage {
                        locale: tag.clone(),
                        id: id.clone(),
                    });
                }
            }
        }
        issues
    }
}

fn placeholders(message: &Message) -> BTreeSet<String> {
    let texts: Vec<&str> = match message {
        Message::Text(text) => vec![text],
        Message::Plural(variants) => variants.values().map(String::as_str).collect(),
    };
    let mut names = BTreeSet::new();
    for text in texts {
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            if rest[start..].starts_with("{{") {
                rest = &rest[start + 2..];
                continue;
            }
            let Some(end) = rest[start..].find('}') else {
                break;
            };
            names.insert(rest[start + 1..start + end].to_owned());
            rest = &rest[start + end + 1..];
        }
    }
    names.remove("count");
    names
}

fn substitute(id: &str, template: &str, args: &[(&str, &str)]) -> Result<String, LocaleError> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find(['{', '}']) {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push(tail.as_bytes()[0] as char);
            rest = &tail[2..];
            continue;
        }
        if let Some(after) = tail.strip_prefix('}') {
            out.push('}');
            rest = after;
            continue;
        }
        let Some(end) = tail.find('}') else {
            out.push_str(tail);
            rest = "";
            break;
        };
        let name = &tail[1..end];
        let value = args
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
            .ok_or_else(|| LocaleError::MissingArgument {
                id: id.to_owned(),
                name: name.to_owned(),
            })?;
        out.push_str(value);
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Locale-aware string ordering for model views.
pub trait Collator {
    fn compare(&self, a: &str, b: &str) -> std::cmp::Ordering;
}

/// Portable collator: compares base letters first (ignoring case and Latin
/// diacritics, including Vietnamese), then diacritics, then case, then code
/// points. Platform backends may provide a full CLDR collator.
#[derive(Clone, Copy, Debug, Default)]
pub struct BasicCollator;

impl Collator for BasicCollator {
    fn compare(&self, a: &str, b: &str) -> std::cmp::Ordering {
        let primary = |s: &str| -> Vec<char> { s.chars().map(|c| fold_base(c).0).collect() };
        let secondary = |s: &str| -> Vec<bool> { s.chars().map(|c| fold_base(c).1).collect() };
        let tertiary = |s: &str| -> Vec<bool> { s.chars().map(char::is_uppercase).collect() };
        primary(a)
            .cmp(&primary(b))
            .then_with(|| secondary(a).cmp(&secondary(b)))
            .then_with(|| tertiary(a).cmp(&tertiary(b)))
            .then_with(|| a.cmp(b))
    }
}

/// Lowercased text with Latin diacritics removed, for search and filtering
/// ("Việt Nam" folds to "viet nam").
pub fn fold_for_search(text: &str) -> String {
    text.chars().map(|c| fold_base(c).0).collect()
}

/// Lowercase base letter and whether the character carried a diacritic.
fn fold_base(c: char) -> (char, bool) {
    const GROUPS: &[(char, &str)] = &[
        ('a', "àáâãäåāăąǎạảấầẩẫậắằẳẵặ"),
        ('c', "çćĉċč"),
        ('d', "ďđ"),
        ('e', "èéêëēĕėęěẹẻẽếềểễệ"),
        ('i', "ìíîïĩīĭįıǐỉị"),
        ('n', "ñńņňŉ"),
        ('o', "òóôõöøōŏőơǒọỏốồổỗộớờởỡợ"),
        ('s', "śŝşšș"),
        ('t', "ţťț"),
        ('u', "ùúûüũūŭůűųưǔụủứừửữự"),
        ('y', "ýÿŷỳỵỷỹ"),
        ('z', "źżž"),
    ];
    let lower = c.to_lowercase().next().unwrap_or(c);
    for (base, variants) in GROUPS {
        if variants.contains(lower) {
            return (*base, true);
        }
    }
    (lower, false)
}

/// The active language plus the catalog, switched at runtime.
pub struct Localizer {
    locale: Locale,
    catalog: Catalog,
    revision: u64,
}

impl Localizer {
    pub fn new(locale: Locale, catalog: Catalog) -> Self {
        Self {
            locale,
            catalog,
            revision: 0,
        }
    }

    pub fn locale(&self) -> &Locale {
        &self.locale
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// Increments on every language switch so views can re-resolve text.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Switches language. Returns true when the direction also changed,
    /// which requires mirrored layout in addition to new measurements.
    pub fn set_locale(&mut self, locale: Locale) -> bool {
        let flipped = locale.direction() != self.locale.direction();
        if locale != self.locale {
            self.locale = locale;
            self.revision += 1;
        }
        flipped
    }

    /// A localizer of the root locale with no application messages: it
    /// gives the built-in English messages.
    pub fn builtin() -> &'static Localizer {
        static BUILTIN: OnceLock<Localizer> = OnceLock::new();
        BUILTIN.get_or_init(|| Localizer::new(Locale::root(), Catalog::new()))
    }

    /// The message `id` of the application catalog, or else of
    /// [`Catalog::builtin`]. A message that neither catalog has is
    /// [`LocaleError::MissingMessage`].
    pub fn text(&self, id: &str, args: &[(&str, &str)]) -> Result<String, LocaleError> {
        match self.catalog.text(&self.locale, id, args) {
            Err(LocaleError::MissingMessage { .. }) => {
                Catalog::builtin().text(&self.locale, id, args)
            }
            result => result,
        }
    }

    /// The plural message `id` for `count`, from the application catalog
    /// or else from [`Catalog::builtin`], as [`text`](Self::text) does.
    pub fn plural(
        &self,
        id: &str,
        count: u64,
        args: &[(&str, &str)],
    ) -> Result<String, LocaleError> {
        match self.catalog.plural(&self.locale, id, count, args) {
            Err(LocaleError::MissingMessage { .. }) => {
                Catalog::builtin().plural(&self.locale, id, count, args)
            }
            result => result,
        }
    }

    /// The message `id`, or `id` itself when no catalog has it. Controls
    /// show their built-in messages with it.
    pub fn message(&self, id: &str, args: &[(&str, &str)]) -> String {
        self.text(id, args).unwrap_or_else(|_| id.to_owned())
    }
}
