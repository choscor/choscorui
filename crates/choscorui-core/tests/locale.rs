use choscorui_core::{
    BasicCollator, Catalog, CatalogIssue, CivilDate, CivilTime, Collator, LayoutDirection, Locale,
    LocaleError, Localizer, PluralCategory, Weekday, month_grid, weekday_short_name,
};

fn locale(tag: &str) -> Locale {
    Locale::parse(tag).unwrap()
}

#[test]
fn parses_tags_and_builds_fallback_chain() {
    let traditional = locale("zh-Hant-TW");
    assert_eq!(traditional.tag(), "zh-TW");
    assert_eq!(locale("vi_vn").tag(), "vi-VN");
    assert_eq!(locale("es-419").region(), Some("419"));
    assert!(Locale::parse("").is_err());
    assert!(Locale::parse("english").is_err());
    assert!(Locale::parse("en-US-x-private").is_err());
    let chain: Vec<String> = locale("pt-BR")
        .fallback_chain()
        .iter()
        .map(Locale::tag)
        .collect();
    assert_eq!(chain, ["pt-BR", "pt", "und"]);
    assert_eq!(locale("ar").direction(), LayoutDirection::Rtl);
    assert_eq!(locale("he-IL").direction(), LayoutDirection::Rtl);
    assert_eq!(locale("vi").direction(), LayoutDirection::Ltr);
}

#[test]
fn plural_rules_follow_cldr_for_supported_families() {
    use PluralCategory::*;
    let cases: &[(&str, &[(u64, PluralCategory)])] = &[
        ("en", &[(0, Other), (1, One), (2, Other)]),
        ("vi", &[(0, Other), (1, Other), (21, Other)]),
        ("fr", &[(0, One), (1, One), (2, Other)]),
        (
            "ru",
            &[
                (1, One),
                (2, Few),
                (5, Many),
                (11, Many),
                (21, One),
                (22, Few),
                (112, Many),
            ],
        ),
        (
            "pl",
            &[(1, One), (3, Few), (5, Many), (22, Few), (21, Many)],
        ),
        ("cs", &[(1, One), (4, Few), (5, Other)]),
        (
            "ar",
            &[
                (0, Zero),
                (1, One),
                (2, Two),
                (3, Few),
                (11, Many),
                (100, Other),
            ],
        ),
        ("he", &[(1, One), (2, Two), (3, Other)]),
    ];
    for (tag, expectations) in cases {
        let locale = locale(tag);
        for (count, category) in *expectations {
            assert_eq!(locale.plural(*count), *category, "{tag} {count}");
            assert!(
                locale.plural_categories().contains(category),
                "{tag} {category:?}"
            );
        }
    }
}

#[test]
fn catalog_falls_back_substitutes_and_selects_plurals() {
    let en = locale("en");
    let vi = locale("vi");
    let ru = locale("ru");
    let mut catalog = Catalog::new();
    catalog
        .load_str(
            &en,
            "# English\ngreeting = Hello, {name}!\nfiles[one] = {count} file\nfiles[other] = {count} files\nbraces = {{literal}}",
        )
        .unwrap();
    catalog.insert(&vi, "greeting", "Xin chào, {name}!");
    catalog.insert_plural(&vi, "files", [(PluralCategory::Other, "{count} tệp")]);
    catalog.insert_plural(
        &ru,
        "files",
        [
            (PluralCategory::One, "{count} файл"),
            (PluralCategory::Few, "{count} файла"),
            (PluralCategory::Many, "{count} файлов"),
        ],
    );
    assert_eq!(
        catalog
            .text(&locale("en-GB"), "greeting", &[("name", "Ada")])
            .unwrap(),
        "Hello, Ada!"
    );
    assert_eq!(
        catalog.text(&vi, "greeting", &[("name", "Lan")]).unwrap(),
        "Xin chào, Lan!"
    );
    assert_eq!(catalog.text(&en, "braces", &[]).unwrap(), "{literal}");
    assert_eq!(catalog.plural(&en, "files", 1, &[]).unwrap(), "1 file");
    assert_eq!(
        catalog.plural(&en, "files", 1200, &[]).unwrap(),
        "1,200 files"
    );
    assert_eq!(
        catalog.plural(&vi, "files", 1200, &[]).unwrap(),
        "1.200 tệp"
    );
    assert_eq!(catalog.plural(&ru, "files", 22, &[]).unwrap(), "22 файла");
    assert_eq!(catalog.plural(&ru, "files", 25, &[]).unwrap(), "25 файлов");
    assert!(matches!(
        catalog.text(&en, "greeting", &[]),
        Err(LocaleError::MissingArgument { .. })
    ));
    assert!(matches!(
        catalog.text(&locale("fr"), "absent", &[]),
        Err(LocaleError::MissingMessage { .. })
    ));
    assert!(catalog.load_str(&en, "bad line").is_err());
    assert!(catalog.load_str(&en, "x[several] = y").is_err());
}

#[test]
fn validation_reports_missing_messages_placeholders_and_plural_gaps() {
    let en = locale("en");
    let mut catalog = Catalog::new();
    catalog.insert(&en, "greeting", "Hello, {name}");
    catalog.insert_plural(
        &en,
        "files",
        [
            (PluralCategory::One, "{count} file"),
            (PluralCategory::Other, "{count} files"),
        ],
    );
    let de = locale("de");
    catalog.insert(&de, "greeting", "Hallo, {nmae}");
    catalog.insert(&de, "stale", "Alt");
    let ru = locale("ru");
    catalog.insert(&ru, "greeting", "Привет, {name}");
    catalog.insert_plural(&ru, "files", [(PluralCategory::One, "{count} файл")]);
    let issues = catalog.validate(&en);
    assert!(issues.contains(&CatalogIssue::PlaceholderMismatch {
        locale: "de".into(),
        id: "greeting".into()
    }));
    assert!(issues.contains(&CatalogIssue::MissingMessage {
        locale: "de".into(),
        id: "files".into()
    }));
    assert!(issues.contains(&CatalogIssue::UnknownMessage {
        locale: "de".into(),
        id: "stale".into()
    }));
    assert!(issues.contains(&CatalogIssue::MissingPluralCategory {
        locale: "ru".into(),
        id: "files".into(),
        category: PluralCategory::Few
    }));
    assert!(issues.contains(&CatalogIssue::MissingPluralCategory {
        locale: "ru".into(),
        id: "files".into(),
        category: PluralCategory::Many
    }));
    assert!(!issues.iter().any(
        |issue| matches!(issue, CatalogIssue::MissingMessage { locale, .. } if locale == "en")
    ));
}

#[test]
fn numbers_format_and_parse_per_locale() {
    let en = locale("en-US");
    let vi = locale("vi-VN");
    let fr = locale("fr");
    assert_eq!(en.format_integer(-1_234_567), "-1,234,567");
    assert_eq!(vi.format_integer(1_234_567), "1.234.567");
    assert_eq!(fr.format_decimal(1234.5, 2).unwrap(), "1\u{202f}234,50");
    assert_eq!(en.format_decimal(-0.001, 2).unwrap(), "0.00");
    assert_eq!(en.parse_decimal("1,234.5").unwrap(), 1234.5);
    assert_eq!(vi.parse_decimal("1.234,5").unwrap(), 1234.5);
    assert_eq!(fr.parse_decimal("1 234,5").unwrap(), 1234.5);
    assert_eq!(
        fr.parse_decimal("2.5").unwrap(),
        2.5,
        "ASCII point accepted when not a group separator"
    );
    assert_eq!(
        vi.parse_decimal("2.500").unwrap(),
        2500.0,
        "point is a group separator in vi"
    );
    assert_eq!(en.parse_decimal("-12").unwrap(), -12.0);
    for bad in ["", "1,23", "12,,345", "1.2.3", "abc", "1.", "--1"] {
        assert_eq!(
            en.parse_decimal(bad),
            Err(LocaleError::InvalidNumber),
            "{bad}"
        );
    }
    assert_eq!(en.parse_integer("1,000").unwrap(), 1000);
    assert!(en.parse_integer("1.5").is_err());
    assert!(en.format_decimal(f64::NAN, 1).is_err());
}

#[test]
fn clamped_dates_and_times_take_the_nearest_valid_value() {
    let date = |year, month, day| CivilDate::new(year, month, day).unwrap();
    assert_eq!(CivilDate::clamped(2026, 10, 12), date(2026, 10, 12));
    assert_eq!(CivilDate::clamped(2023, 2, 29), date(2023, 2, 28));
    assert_eq!(CivilDate::clamped(2024, 13, 0), date(2024, 12, 1));
    assert_eq!(CivilDate::clamped(0, 0, 40), date(1, 1, 31));
    assert_eq!(CivilDate::clamped(12_000, 6, 31), date(9999, 6, 30));
    assert_eq!(
        CivilTime::clamped(9, 30, 0),
        CivilTime::new(9, 30, 0).unwrap()
    );
    assert_eq!(
        CivilTime::clamped(24, 60, 99),
        CivilTime::new(23, 59, 59).unwrap()
    );
}

#[test]
fn dates_round_trip_through_epoch_and_locale_formats() {
    let date = CivilDate::new(2024, 2, 29).unwrap();
    assert!(CivilDate::new(2023, 2, 29).is_err());
    assert!(CivilDate::new(2024, 13, 1).is_err());
    assert_eq!(CivilDate::new(1970, 1, 1).unwrap().days_since_epoch(), 0);
    assert_eq!(
        CivilDate::from_days_since_epoch(date.days_since_epoch()).unwrap(),
        date
    );
    assert_eq!(date.weekday(), Weekday(4)); // Thursday
    assert_eq!(
        date.add_days(1).unwrap(),
        CivilDate::new(2024, 3, 1).unwrap()
    );
    assert_eq!(
        CivilDate::new(2024, 1, 31).unwrap().add_months(1).unwrap(),
        date
    );
    assert_eq!(
        date.add_months(-14).unwrap(),
        CivilDate::new(2022, 12, 29).unwrap()
    );
    assert_eq!(date.format_short(&locale("en-US")), "02/29/2024");
    assert_eq!(date.format_short(&locale("vi")), "29/02/2024");
    assert_eq!(date.format_short(&locale("de")), "29.02.2024");
    assert_eq!(date.format_short(&locale("ja")), "2024/02/29");
    assert_eq!(date.format_long(&locale("en")), "February 29, 2024");
    assert_eq!(date.format_long(&locale("vi")), "29 tháng 2, 2024");
    for tag in ["en-US", "vi", "de", "ja", "fr", "ar"] {
        let locale = locale(tag);
        assert_eq!(
            CivilDate::parse_short(&date.format_short(&locale), &locale).unwrap(),
            date,
            "{tag}"
        );
    }
    assert_eq!(
        CivilDate::parse_short("1/2/2024", &locale("en-US")).unwrap(),
        CivilDate::new(2024, 1, 2).unwrap()
    );
    assert!(CivilDate::parse_short("31/02/2024", &locale("vi")).is_err());
    assert!(CivilDate::parse_short("1/2/24", &locale("en-US")).is_err());
    assert_eq!(CivilDate::parse_iso("2024-02-29").unwrap(), date);
    assert!(CivilDate::parse_iso("2024-2-29").is_err());
}

#[test]
fn times_format_and_parse_twelve_and_twenty_four_hour_clocks() {
    let time = CivilTime::new(13, 5, 0).unwrap();
    assert_eq!(time.format_short(&locale("en-US")), "1:05 PM");
    assert_eq!(time.format_short(&locale("vi")), "13:05");
    assert_eq!(CivilTime::parse("1:05 pm").unwrap(), time);
    assert_eq!(
        CivilTime::parse("12:00 AM").unwrap(),
        CivilTime::new(0, 0, 0).unwrap()
    );
    assert_eq!(CivilTime::parse("23:59:59").unwrap().second(), 59);
    assert!(CivilTime::parse("24:00").is_err());
    assert!(CivilTime::parse("13:00 PM").is_err());
    assert!(CivilTime::parse("9:5").is_err());
    assert_eq!(
        CivilTime::new(23, 30, 0).unwrap().add_seconds(3600),
        CivilTime::new(0, 30, 0).unwrap()
    );
}

#[test]
fn month_grid_starts_on_locale_first_weekday() {
    let us = month_grid(2024, 9, &locale("en-US")).unwrap();
    assert_eq!(us[0], CivilDate::new(2024, 9, 1).unwrap()); // Sunday
    let vi = month_grid(2024, 9, &locale("vi")).unwrap();
    assert_eq!(vi[0], CivilDate::new(2024, 8, 26).unwrap()); // Monday
    assert_eq!(vi[0].weekday(), Weekday(1));
    assert_eq!(vi[41], CivilDate::new(2024, 10, 6).unwrap());
    assert_eq!(weekday_short_name(&locale("vi"), Weekday(0)), "CN");
}

#[test]
fn basic_collator_orders_vietnamese_and_accented_text_by_base_letter() {
    let collator = BasicCollator;
    let mut words = vec![
        "Zebra",
        "Đà Nẵng",
        "apple",
        "Ánh",
        "ánh",
        "Bình",
        "anh",
        "dao",
    ];
    words.sort_by(|a, b| collator.compare(a, b));
    assert_eq!(
        words,
        [
            "anh",
            "ánh",
            "Ánh",
            "apple",
            "Bình",
            "Đà Nẵng",
            "dao",
            "Zebra"
        ]
    );
}

#[test]
fn localizer_switches_language_and_reports_direction_flip() {
    let mut catalog = Catalog::new();
    catalog.insert(&locale("en"), "ok", "OK");
    catalog.insert(&locale("ar"), "ok", "موافق");
    let mut localizer = Localizer::new(locale("en"), catalog);
    assert_eq!(localizer.text("ok", &[]).unwrap(), "OK");
    assert!(localizer.set_locale(locale("ar-EG")));
    assert_eq!(localizer.revision(), 1);
    assert_eq!(localizer.text("ok", &[]).unwrap(), "موافق");
    assert!(!localizer.set_locale(locale("ar-EG")));
    assert_eq!(localizer.revision(), 1);
}
