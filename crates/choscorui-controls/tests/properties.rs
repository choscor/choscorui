//! Seeded property and fuzz tests for data views and structured input.
//! `CHOSCORUI_PROPTEST_CASES` raises the case count.

use choscorui_controls::{DateInput, InputMask, NumberInput, ValidationState};
use choscorui_core::{
    BasicCollator, Collator, EditableModel, ItemKey, ItemModel, Locale, MatchMode, Role,
    SortFilterProxy, SortOrder, StringListModel, TextFilter, Value,
};
use std::collections::BTreeSet;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

fn cases(default: u64) -> u64 {
    std::env::var("CHOSCORUI_PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

const WORDS: &[&str] = &[
    "Hà Nội",
    "ha noi",
    "Huế",
    "HUE",
    "Đà Lạt",
    "da lat",
    "Hội An",
    "Sa Pa",
    "sapa",
    "Cần Thơ",
    "",
    "z",
    "A",
];

fn text(model: &dyn ItemModel, key: ItemKey) -> String {
    match model.data(key, 0, Role::Display) {
        Value::Text(text) => text,
        other => other.to_display_string(),
    }
}

#[test]
fn sort_filter_proxy_matches_brute_force_after_random_edits() {
    for seed in 0..cases(80) {
        let mut rng = Rng::new(seed);
        let mut model = StringListModel::new(1);
        let mut proxy = SortFilterProxy::new();
        for step in 0..60 {
            let len = model.row_count(None);
            match rng.below(5) {
                0 | 1 => {
                    let values: Vec<String> = (0..rng.below(4) + 1)
                        .map(|_| rng.pick(WORDS).to_string())
                        .collect();
                    model.insert(None, rng.below(len + 1), values).unwrap();
                }
                2 if len > 0 => {
                    let key = model.key(None, rng.below(len)).unwrap();
                    model.remove(&[key]).unwrap();
                }
                3 if len > 0 => {
                    let key = model.key(None, rng.below(len)).unwrap();
                    let value = Value::Text(rng.pick(WORDS).to_string());
                    model.set_data(key, 0, Role::Edit, value).unwrap();
                }
                _ => {
                    proxy.set_sort(match rng.below(3) {
                        0 => None,
                        1 => Some((0, SortOrder::Ascending)),
                        _ => Some((0, SortOrder::Descending)),
                    });
                    proxy.set_filter(match rng.below(3) {
                        0 => None,
                        _ => Some(TextFilter {
                            column: None,
                            pattern: rng.pick(&["ha", "hue", "Đ", "a", "x"]).to_string(),
                            mode: MatchMode::Contains,
                        }),
                    });
                }
            }
            proxy.sync(&model);
            let view = proxy.view(&model);
            let shown: Vec<ItemKey> = (0..view.row_count(None))
                .map(|row| view.key(None, row).unwrap())
                .collect();
            let expected: BTreeSet<ItemKey> = (0..model.row_count(None))
                .map(|row| model.key(None, row).unwrap())
                .filter(|key| {
                    proxy
                        .filter()
                        .is_none_or(|filter| filter.matches(&text(&model, *key)))
                })
                .collect();
            assert_eq!(
                shown.iter().copied().collect::<BTreeSet<_>>(),
                expected,
                "seed {seed} step {step}: filtered membership"
            );
            assert_eq!(shown.len(), expected.len(), "seed {seed}: no duplicates");
            if let Some((_, order)) = proxy.sort() {
                for pair in shown.windows(2) {
                    let ordering =
                        BasicCollator.compare(&text(&model, pair[0]), &text(&model, pair[1]));
                    let ordering = match order {
                        SortOrder::Ascending => ordering,
                        SortOrder::Descending => ordering.reverse(),
                    };
                    assert_ne!(
                        ordering,
                        std::cmp::Ordering::Greater,
                        "seed {seed} step {step}: sorted"
                    );
                }
            }
            for key in &shown {
                assert!(proxy.contains(*key), "seed {seed}");
            }
        }
    }
}

const INPUT: &[&str] = &[
    "0", "1", "9", "12", ",", ".", "-", "+", " ", "e", "1e309", "١", "٫", "\u{a0}", "/", "31", "2",
    "29", "02", "a", "Z", "(", ")", "", "∞",
];

#[test]
fn structured_inputs_never_panic_and_valid_values_round_trip() {
    let locales: Vec<Locale> = ["en-US", "vi-VN", "de-DE", "fr-FR", "ar-EG"]
        .iter()
        .map(|tag| Locale::parse(tag).unwrap())
        .collect();
    let masks: Vec<InputMask> = [
        "(999) 000-0000",
        "AAA-9999",
        ">LL<ll",
        "00/00/0000",
        "HH:HH",
    ]
    .iter()
    .filter_map(|pattern| InputMask::parse(pattern).ok())
    .collect();
    assert!(!masks.is_empty());
    for seed in 0..cases(500) {
        let mut rng = Rng::new(seed);
        let input: String = (0..rng.below(10)).map(|_| *rng.pick(INPUT)).collect();
        let locale = rng.pick(&locales).clone();

        let number =
            NumberInput::new(locale.clone(), rng.below(4)).with_range(Some(-1e6), Some(1e6));
        let (state, value) = number.validate(&input);
        if let (ValidationState::Valid, Some(value)) = (&state, value) {
            assert!(
                value.is_finite() && (-1e6..=1e6).contains(&value),
                "seed {seed}"
            );
            let formatted = number.format(value);
            let (again, reparsed) = number.validate(&formatted);
            assert_eq!(again, ValidationState::Valid, "seed {seed}: {formatted:?}");
            assert_eq!(reparsed, Some(value), "seed {seed}: {formatted:?}");
        }

        let date = DateInput::new(locale);
        if let (ValidationState::Valid, Some(parsed)) = date.validate(&input) {
            let formatted = date.format(parsed);
            assert_eq!(date.validate(&formatted).1, Some(parsed), "seed {seed}");
        }

        let mask = rng.pick(&masks);
        let masked = mask.apply(&input);
        let _ = mask.validate(&masked.value);
        assert_eq!(
            mask.apply(&masked.value).value,
            masked.value,
            "seed {seed}: applying a mask is idempotent"
        );
    }
}
