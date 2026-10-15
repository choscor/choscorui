//! The combobox and the font chooser through the control host: the
//! editing model of the text, commits and reverts, the popup rows, and
//! the missing-family state.

mod hosted;

use choscorui_controls::{
    ComboItem, Combobox, ControlMessage, ControlValue, CustomControl, EstimatedText, FontChooser,
    MeasureContext, ShortcutModifiers, Styles, TextMeasure,
};
use choscorui_core::{
    Constraints, DesignTokens, ItemKey, Key, LayoutDirection, Localizer, Point, Rect, TypeSpec,
};
use hosted::{Harness, texts};
use std::cell::Cell;

const COMMAND: ShortcutModifiers = ShortcutModifiers {
    shift: false,
    control: false,
    option: false,
    command: true,
};

fn fruits(editable: bool) -> Combobox {
    let mut combo = Combobox::new("Fruit", editable);
    let items = ["Apple", "Banana", "Cherry"]
        .iter()
        .enumerate()
        .map(|(index, text)| ComboItem {
            key: ItemKey(index as u64 + 1),
            text: (*text).to_string(),
        })
        .collect();
    combo.set_items(items);
    combo
}

fn value(h: &Harness) -> String {
    h.semantics().value.unwrap_or_default()
}

fn index(index: Option<usize>) -> ControlMessage {
    ControlMessage::Changed(ControlValue::Index(index))
}

fn is_open(h: &Harness) -> bool {
    h.host.get(h.node).unwrap().overlay().is_some()
}

fn blur(h: &mut Harness) {
    h.host.move_focus(&mut h.tree, None);
}

fn font_chooser() -> FontChooser {
    let families = vec!["Helvetica".to_string(), "Menlo".to_string()];
    FontChooser::new(families, "Menlo", "Helvetica")
}

#[test]
fn editable_text_uses_the_caret_selection_and_editing_keys() {
    let mut h = Harness::new(fruits(true));
    h.focus();
    h.text("bn");
    h.key(Key::ArrowLeft);
    h.text("a");
    assert_eq!(value(&h), "ban", "typing goes in at the caret");
    h.key(Key::Delete);
    assert_eq!(value(&h), "ba");
    h.chord(COMMAND, Key::Character('a'));
    h.text("c");
    assert_eq!(value(&h), "c", "Command+A selects the text to replace");
}

#[test]
fn escape_restores_the_text_from_before_the_first_typed_character() {
    let mut h = Harness::new(fruits(true));
    h.focus();
    h.text("ch");
    h.key(Key::Enter);
    assert_eq!(value(&h), "Cherry");
    h.messages();
    h.key(Key::Backspace);
    h.text("x");
    assert!(h.messages().contains(&index(None)));
    h.key(Key::Escape);
    assert_eq!(value(&h), "Cherry");
    assert!(h.messages().contains(&index(Some(2))));
    assert!(!is_open(&h));
}

#[test]
fn a_blur_commits_a_match_or_reverts() {
    let mut h = Harness::new(fruits(true));
    h.focus();
    h.text("apple");
    blur(&mut h);
    assert_eq!(value(&h), "Apple", "a blur commits the matching item");
    assert!(h.messages().contains(&index(Some(0))));
    h.focus();
    h.text("z");
    blur(&mut h);
    assert_eq!(value(&h), "Apple", "a blur without a match reverts");
    assert!(h.messages().contains(&index(Some(0))));
}

#[test]
fn the_popup_shows_a_no_results_row() {
    let mut h = Harness::new(fruits(true));
    h.place(hosted::rect(10.0, 10.0, 280.0, 36.0));
    h.focus();
    h.text("zz");
    assert!(is_open(&h), "no match keeps the popup open");
    h.set_time(1.0);
    assert!(
        texts(&h.paint()).contains(&"No results".to_string()),
        "{:?}",
        texts(&h.paint())
    );
}

#[test]
fn tab_accepts_the_highlighted_row() {
    let mut h = Harness::new(fruits(true));
    h.focus();
    h.text("b");
    h.messages();
    h.key(Key::Tab);
    assert_eq!(value(&h), "Banana");
    assert!(h.messages().contains(&index(Some(1))));
}

#[test]
fn a_click_in_editable_text_does_not_toggle_the_popup() {
    let mut h = Harness::new(fruits(true));
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 12.0, bounds.center().y));
    assert!(!is_open(&h), "a click in the text places the caret");
    h.click(Point::new(bounds.max_x() - 14.0, bounds.center().y));
    assert!(is_open(&h), "a click on the chevron opens the popup");
}

#[test]
fn a_combobox_shows_its_placeholder() {
    let h = Harness::new(fruits(false).with_placeholder("Pick a fruit"));
    assert!(texts(&h.paint()).contains(&"Pick a fruit".to_string()));
}

#[test]
fn typed_text_does_not_change_the_resolved_family_until_committed() {
    let mut chooser = font_chooser();
    chooser.handle(choscorui_controls::ComboInput::SetText("Zapf".into()));
    assert_eq!(chooser.resolved_family(), "Menlo");
    assert!(!chooser.is_missing());
    let mut h = Harness::new(font_chooser());
    h.focus();
    h.text("x");
    blur(&mut h);
    assert_eq!(
        value(&h),
        "Menlo",
        "a blur reverts text that is not committed"
    );
}

#[test]
fn an_empty_font_family_means_the_default() {
    let mut chooser = font_chooser();
    chooser.handle(choscorui_controls::ComboInput::SetText(String::new()));
    chooser.handle(choscorui_controls::ComboInput::Key {
        key: Key::Enter,
        alt: false,
    });
    assert!(!chooser.is_missing());
    assert_eq!(chooser.resolved_family(), "Helvetica");
    assert!(!chooser.semantics(Localizer::builtin()).state.invalid);
    let mut h = Harness::new(font_chooser());
    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Index(None))
    );
    assert_eq!(value(&h), "", "no index clears the text");
}

#[test]
fn the_host_invalid_state_shows_on_an_installed_family() {
    let mut h = Harness::new(font_chooser());
    let valid = format!("{:?}", h.paint().commands());
    h.host.set_invalid(&mut h.tree, h.node, true).unwrap();
    assert_ne!(format!("{:?}", h.paint().commands()), valid);
}

/// Text measure that counts its calls.
struct Counting(Cell<usize>);

impl TextMeasure for Counting {
    fn width(&self, text: &str, spec: &TypeSpec) -> f64 {
        self.0.set(self.0.get() + 1);
        EstimatedText.width(text, spec)
    }
}

#[test]
fn a_font_chooser_measures_its_families_once() {
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let counting = Counting(Cell::new(0));
    let families = (0..50).map(|n| format!("Family {n}")).collect();
    let mut chooser = FontChooser::new(families, "Family 1", "Family 0");
    let cx = MeasureContext {
        constraints: Constraints::UNBOUNDED,
        viewport: Rect::default(),
        direction: LayoutDirection::Ltr,
        tokens: &tokens,
        styles: &styles,
        text: &counting,
        key_convention: choscorui_core::KeyConvention::Mac,
    };
    chooser.measure(&cx);
    let first = counting.0.get();
    chooser.measure(&cx);
    assert!(
        counting.0.get() - first <= 2,
        "a second layout measures only the text and the placeholder, not the 50 families"
    );
}
