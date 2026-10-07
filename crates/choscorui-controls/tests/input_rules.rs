use choscorui_controls::{DateInput, InputMask, MaskError, NumberInput, ValidationState};
use choscorui_core::{CivilDate, Locale};

fn locale(tag: &str) -> Locale {
    Locale::parse(tag).unwrap()
}

#[test]
fn masks_insert_literals_skip_optional_slots_and_report_completion() {
    let phone = InputMask::parse("(000) 000-0000").unwrap();
    assert_eq!(phone.placeholder('_'), "(___) ___-____");
    let partial = phone.apply("0912");
    assert_eq!(partial.display, "(091) 2");
    assert_eq!(partial.value, "0912");
    assert!(!partial.complete);
    assert_eq!(phone.validate("0912"), ValidationState::Intermediate);
    let typed_literals = phone.apply("(091) 234-5678");
    assert_eq!(typed_literals.display, "(091) 234-5678");
    assert!(typed_literals.complete);
    assert_eq!(phone.validate("0912345678"), ValidationState::Valid);
    assert!(matches!(phone.validate("09a"), ValidationState::Invalid(_)));

    let plate = InputMask::parse("AA-9999").unwrap();
    assert_eq!(plate.apply("HN12").display, "HN-12");
    assert!(!plate.apply("H").complete);
    assert!(plate.apply("HN").complete, "the digits are optional");
    assert_eq!(plate.apply("HN").display, "HN-");
    assert_eq!(plate.apply("HN-").display, "HN-");
    let escaped = InputMask::parse(r"\A-00").unwrap();
    assert_eq!(escaped.apply("12").display, "A-12");
    assert_eq!(InputMask::parse("--"), Err(MaskError::Empty));
    assert_eq!(InputMask::parse("00\\"), Err(MaskError::TrailingEscape));
    let letters = InputMask::parse("AAA").unwrap();
    assert_eq!(letters.apply("Đức").display, "Đức");
}

#[test]
fn numbers_follow_locale_separators_bounds_and_precision() {
    let vi = NumberInput::new(locale("vi"), 2).with_range(Some(10.0), Some(1_000_000.0));
    assert_eq!(
        vi.validate("1.234,5"),
        (ValidationState::Valid, Some(1234.5))
    );
    assert_eq!(vi.validate("").0, ValidationState::Intermediate);
    assert_eq!(vi.validate("-").0, ValidationState::Intermediate);
    assert_eq!(
        vi.validate("1").0,
        ValidationState::Intermediate,
        "may reach 10"
    );
    assert!(matches!(vi.validate("1,5").0, ValidationState::Invalid(_)));
    assert!(
        matches!(vi.validate("1,234").0, ValidationState::Invalid(_)),
        "3 decimals"
    );
    assert!(matches!(
        vi.validate("2.000.000").0,
        ValidationState::Invalid(_)
    ));
    assert!(matches!(vi.validate("abc").0, ValidationState::Invalid(_)));
    assert_eq!(vi.format(1234.5), "1.234,50");
    let us = NumberInput::new(locale("en-US"), 0);
    assert_eq!(us.validate("1,234"), (ValidationState::Valid, Some(1234.0)));
}

#[test]
fn dates_follow_locale_field_order() {
    let vi = DateInput::new(locale("vi"));
    assert_eq!(vi.placeholder(), "dd/mm/yyyy");
    let date = CivilDate::new(2026, 12, 31).unwrap();
    assert_eq!(
        vi.validate("31/12/2026"),
        (ValidationState::Valid, Some(date))
    );
    assert_eq!(vi.format(date), "31/12/2026");
    assert_eq!(vi.validate("31/12").0, ValidationState::Intermediate);
    assert_eq!(vi.validate("31/12/20").0, ValidationState::Intermediate);
    assert!(matches!(
        vi.validate("31/02/2026").0,
        ValidationState::Invalid(_)
    ));
    assert!(matches!(
        vi.validate("hôm nay").0,
        ValidationState::Invalid(_)
    ));
    let us =
        DateInput::new(locale("en-US")).with_range(Some(CivilDate::new(2026, 1, 1).unwrap()), None);
    assert_eq!(us.placeholder(), "mm/dd/yyyy");
    assert_eq!(us.validate("12/31/2026").1, Some(date));
    assert!(matches!(
        us.validate("12/31/2025").0,
        ValidationState::Invalid(_)
    ));
}
