//! The input filters: what fields let the user type, paste, commit from
//! an input method, or set, and the beep of a refused edit.

mod hosted;

use choscorui_controls::{
    Accepted, CellEdit, CharClass, ControlMessage, ControlValue, DateInput, DecimalSpinBox,
    DefaultEditorFactory, EditorKind, Input, InputDialog, InputFilter, InputKind, InputMask,
    InputType, IntegerRange, NumberError, NumberInput, RangeError, SpinBox, TextField,
    TextFieldChange, TextFieldInput, ValidationError, ValidationState,
};
use choscorui_core::{
    Catalog, Feedback, ItemData, ItemModel, Key, ListStore, Locale, Localizer, ModelError, Point,
    Rect, Role, SemanticRole, Size, Tree, Value,
};
use hosted::Harness;
use std::cell::Cell;
use std::rc::Rc;

fn locale(tag: &str) -> Locale {
    Locale::parse(tag).unwrap()
}

/// A feedback backend that counts the beeps.
#[derive(Default)]
struct Beeps(Cell<usize>);

impl Feedback for Beeps {
    fn beep(&self) {
        self.0.set(self.0.get() + 1);
    }
}

/// A harness for `control` whose host counts the beeps.
fn beeping(control: impl choscorui_controls::CustomControl + 'static) -> (Harness, Rc<Beeps>) {
    let mut h = Harness::new(control);
    let beeps = Rc::new(Beeps::default());
    h.host.set_feedback(beeps.clone());
    (h, beeps)
}

fn insert(text: &str) -> Option<Accepted> {
    Some(Accepted::Insert(text.to_owned()))
}

#[test]
fn a_filter_takes_an_edit_as_one_unit() {
    let digits = InputFilter::new().chars(CharClass::Digits);
    assert_eq!(digits.accept("12", 2..2, "3"), insert("3"));
    assert_eq!(digits.accept("12", 2..2, "a"), None);
    assert_eq!(
        digits.accept("12", 2..2, "3a"),
        None,
        "a paste is refused whole"
    );
    assert_eq!(
        digits.accept("12", 0..1, ""),
        insert(""),
        "a deletion is allowed"
    );

    let short = InputFilter::new().max_length(3);
    assert_eq!(
        short.accept("ab", 2..2, "cde"),
        insert("c"),
        "cut to the room"
    );
    assert_eq!(short.accept("abc", 3..3, "d"), None, "no room");
    assert_eq!(
        short.accept("abc", 1..2, "x"),
        insert("x"),
        "a replacement fits"
    );
    assert_eq!(
        short.accept("", 0..0, "Lựu🇻🇳!"),
        insert("Lựu"),
        "the length counts grapheme clusters"
    );

    let vowels = InputFilter::new().chars(CharClass::Custom(Rc::new(|ch| "aeiou".contains(ch))));
    assert_eq!(vowels.accept("", 0..0, "ae"), insert("ae"));
    assert_eq!(vowels.accept("", 0..0, "b"), None);
    let letters = InputFilter::new().chars(CharClass::AsciiAlphanumeric);
    assert_eq!(letters.accept("", 0..0, "é"), None);

    let even = InputFilter::new().rule(|text| text.len() % 2 == 0);
    assert_eq!(even.accept("a", 1..1, "b"), insert("b"));
    assert_eq!(even.accept("ab", 2..2, "c"), None);
    assert!(InputFilter::new().is_open());
    assert!(!even.is_open());
}

#[test]
fn a_mask_filter_keeps_the_characters_that_fit() {
    let phone = InputFilter::masked(InputMask::parse("(000) 000-0000").unwrap());
    let Some(Accepted::Value { text, .. }) = phone.accept("", 0..0, "09a05") else {
        panic!("the digits fit the mask");
    };
    assert_eq!(text, "(090) 5", "the paste keeps the digits");
    assert_eq!(
        phone.accept("(090) 5", 7..7, "x"),
        None,
        "no slot takes a letter"
    );
    let plate = InputFilter::masked(InputMask::parse(">AA-99999").unwrap());
    let Some(Accepted::Value { text, .. }) = plate.accept("", 0..0, "ab1") else {
        panic!("the letters fit the mask");
    };
    assert_eq!(text, "AB-1", "`>` uppercases");
    let lower = InputMask::parse("<AA").unwrap();
    assert_eq!(lower.apply("XY").display, "xy", "`<` lowercases");
}

#[test]
fn the_number_rule_refuses_impossible_text() {
    let any = NumberInput::new(locale("en-US"), 2);
    for text in ["-1,234.5", "12", "-", "0.", "1,2"] {
        assert!(any.accepts(text), "{text}");
    }
    for text in ["1-2", "1.2.3", "--", "1.234", "12a", "1.5-"] {
        assert!(!any.accepts(text), "{text}");
    }
    let whole = NumberInput::new(locale("en-US"), 0).with_range(Some(1.0), Some(500.0));
    assert!(!whole.accepts("1.5"), "no fraction");
    assert!(!whole.accepts("-1"), "the minimum is not negative");
    assert_eq!(whole.validate("0").0, ValidationState::Intermediate);
    assert_eq!(
        whole.commit("0").0,
        ValidationState::Invalid(NumberError::BelowMinimum(1.0).into())
    );
    assert_eq!(
        whole.validate("501").0,
        ValidationState::Invalid(NumberError::AboveMaximum(500.0).into())
    );
}

#[test]
fn the_date_rule_refuses_impossible_partials() {
    let us = DateInput::new(locale("en-US"));
    for text in ["1", "12/", "12/31/2026", "1/5/"] {
        assert!(us.accepts(text), "{text}");
    }
    for text in ["13/", "1234/", "12/32", "12-31", "12/31/20267", "a"] {
        assert!(!us.accepts(text), "{text}");
    }
    for tag in ["und", "ja"] {
        let ymd = DateInput::new(locale(tag));
        assert!(
            matches!(ymd.validate("2026-13-45").0, ValidationState::Invalid(_)),
            "{tag}"
        );
    }
    let root = DateInput::new(Locale::root());
    assert!(root.validate("2026-12-31").1.is_some());
}

fn field_tree() -> (Tree, choscorui_core::NodeId) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(240.0, 80.0)));
    let id = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(10.0, 10.0), Size::new(180.0, 32.0)),
        )
        .unwrap();
    tree.set_focusable(id, true).unwrap();
    tree.set_focus(Some(id)).unwrap();
    (tree, id)
}

#[test]
fn a_text_field_filters_keys_pastes_input_methods_and_set_value() {
    let (mut tree, id) = field_tree();
    let mut field =
        TextField::new("Code", "").with_filter(InputFilter::new().chars(CharClass::Digits));
    let mut send = |field: &mut TextField, input| field.handle(&mut tree, id, input).unwrap();
    assert_eq!(
        send(&mut field, TextFieldInput::Text("12".into())),
        TextFieldChange::Edited
    );
    assert_eq!(
        send(&mut field, TextFieldInput::Text("x".into())),
        TextFieldChange::Rejected
    );
    assert_eq!(
        send(&mut field, TextFieldInput::Text("3y".into())),
        TextFieldChange::Rejected,
        "a paste with a refused character"
    );
    assert_eq!(
        send(
            &mut field,
            TextFieldInput::CommitMarkedText {
                text: "ă".into(),
                replacement_utf16: None,
            }
        ),
        TextFieldChange::Rejected,
        "an input method commit"
    );
    assert_eq!(
        send(&mut field, TextFieldInput::SetValue("abc".into())),
        TextFieldChange::Rejected
    );
    assert_eq!(field.value(), "12");
    // A refused edit adds no undo step: undo removes the accepted edit.
    send(&mut field, TextFieldInput::Undo);
    assert_eq!(field.value(), "");
}

#[test]
fn a_password_field_does_not_cut() {
    let (mut tree, id) = field_tree();
    let mut field = TextField::password("Password", "secret");
    field
        .handle(&mut tree, id, TextFieldInput::SelectAll)
        .unwrap();
    field.handle(&mut tree, id, TextFieldInput::Cut).unwrap();
    assert_eq!(field.value(), "secret");
}

#[test]
fn a_refused_key_beeps_through_the_host_feedback() {
    let (mut h, beeps) = beeping(Input::new("Code", "").filter(CharClass::Digits));
    h.focus();
    h.text("1");
    h.text("a");
    assert_eq!(h.semantics().value.as_deref(), Some("1"));
    assert_eq!(beeps.0.get(), 1);
    let (mut h, beeps) = beeping(Input::new("Initials", "").max_length(2));
    h.focus();
    h.text("abc");
    assert_eq!(h.semantics().value.as_deref(), Some("ab"));
    assert_eq!(beeps.0.get(), 0, "a paste cut to the room does not beep");
    h.text("d");
    assert_eq!(beeps.0.get(), 1);
}

#[test]
fn an_input_kind_sets_its_filter_and_role() {
    let search = Harness::new(Input::new("Find", "").with_type(InputType::Search));
    assert_eq!(search.semantics().role, SemanticRole::SearchField);
    let (mut tel, beeps) = beeping(Input::new("Phone", "").with_type(InputType::Tel));
    tel.focus();
    tel.text("+84 (90) 555-01.23");
    tel.text("x");
    assert_eq!(tel.semantics().value.as_deref(), Some("+84 (90) 555-01.23"));
    assert_eq!(beeps.0.get(), 1);
    let (mut email, _) = beeping(Input::new("Email", "").with_type(InputType::Email));
    email.focus();
    email.text("luu");
    email.key(Key::Enter);
    assert!(email.semantics().state.invalid, "Return checks the address");
}

#[test]
fn a_spin_box_takes_typed_numbers_and_clamps_them_on_return() {
    let range = IntegerRange::new(0, 100, 5, 1, 10).unwrap();
    let (mut h, beeps) = beeping(SpinBox::new("Quantity", range));
    h.focus();
    h.text("87");
    h.key(Key::Enter);
    assert_eq!(h.semantics().value.as_deref(), Some("87"));
    assert!(
        h.messages()
            .contains(&ControlMessage::Changed(ControlValue::Number(87.0)))
    );
    h.text("x");
    assert_eq!(beeps.0.get(), 1, "the integer rule refuses letters");
    h.key(Key::End);
    assert_eq!(h.semantics().value.as_deref(), Some("100"));
    h.key(Key::ArrowUp);
    assert_eq!(beeps.0.get(), 2, "a step past the bound beeps");
    h.focus();
    h.text("150");
    h.key(Key::Enter);
    assert_eq!(h.semantics().value.as_deref(), Some("100"), "clamped");
}

#[test]
fn a_decimal_spin_box_reports_a_bad_range() {
    let new = |range, step| DecimalSpinBox::new("Amount", locale("en"), range, step, 1, 0.0);
    assert!(matches!(
        new((f64::NAN, 1.0), 0.5),
        Err(RangeError::NotANumber)
    ));
    assert!(matches!(new((5.0, 1.0), 0.5), Err(RangeError::Reversed)));
    assert!(matches!(new((0.0, 1.0), 0.0), Err(RangeError::ZeroStep)));
    assert!(matches!(new((0.0, 1.0), -1.0), Err(RangeError::ZeroStep)));
    // The value is rounded first and then clamped, so it stays in range.
    let spin = DecimalSpinBox::new("Amount", locale("en"), (0.0, 9.99), 0.1, 1, 9.99).unwrap();
    assert!(spin.value() <= 9.99, "{}", spin.value());
}

#[test]
fn a_decimal_spin_box_commits_typed_text_before_a_step_and_restores_on_blur() {
    let spin = DecimalSpinBox::new("Amount", locale("en"), (0.0, 10.0), 0.5, 1, 2.0).unwrap();
    let mut h = Harness::new(spin);
    h.focus();
    h.text("4");
    h.key(Key::ArrowUp);
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("4.5"),
        "4 committed, then a step"
    );
    h.focus();
    h.text("99");
    assert!(h.semantics().state.invalid, "above the maximum");
    h.host.move_focus(&mut h.tree, None);
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("4.5"),
        "restored on blur"
    );
    assert!(!h.semantics().state.invalid);
}

#[test]
fn an_input_dialog_words_integer_and_decimal_errors_alike() {
    let en = locale("en");
    let mut integer = InputDialog::new(
        "Line",
        "Line:",
        InputKind::Integer {
            minimum: 10,
            maximum: 500,
        },
        en.clone(),
        "",
    );
    let mut decimal = InputDialog::new(
        "Price",
        "Price:",
        InputKind::Decimal {
            minimum: 10.0,
            maximum: 500.0,
            fraction_digits: 2,
        },
        en,
        "",
    );
    let strings = Localizer::builtin();
    let message = |state: ValidationState| match state {
        ValidationState::Invalid(error) => error.message(strings),
        state => panic!("expected an error, not {state:?}"),
    };
    integer.set_text("501");
    decimal.set_text("501");
    assert_eq!(message(integer.validate().0), message(decimal.validate().0));
    integer.set_text("5");
    decimal.set_text("5");
    assert_eq!(integer.validate().0, ValidationState::Intermediate);
    assert_eq!(decimal.validate().0, ValidationState::Intermediate);
    assert_eq!(
        message(integer.commit().0),
        message(decimal.commit().0),
        "below the minimum is invalid when the user accepts"
    );
    assert!(!integer.filter().is_open(), "the number rule filters keys");
}

#[test]
fn validation_errors_speak_the_user_language() {
    let error = ValidationError::from(NumberError::TooManyDecimals(1));
    assert_eq!(
        error.message(Localizer::builtin()),
        "Use at most 1 decimal place"
    );
    let vietnamese = Localizer::new(locale("vi"), Catalog::new());
    assert_eq!(error.message(&vietnamese), "Dùng tối đa 1 chữ số thập phân");
    assert_eq!(
        ValidationError::from("Choose a name").message(&vietnamese),
        "Choose a name",
        "an application message stays as it is"
    );
}

/// A to-do row with one bool column.
struct Done(bool);

impl ItemData for Done {
    fn data(&self, _: usize, role: Role) -> Value {
        match role {
            Role::Display | Role::Edit => Value::Bool(self.0),
            _ => Value::None,
        }
    }
    fn set_data(&mut self, _: usize, _: Role, value: Value) -> Result<(), ModelError> {
        let Value::Bool(done) = value else {
            return Err(ModelError::InvalidValue);
        };
        self.0 = done;
        Ok(())
    }
    fn editable(&self, _: usize) -> bool {
        true
    }
}

#[test]
fn a_bool_cell_edits_with_a_checkbox() {
    let mut store = ListStore::new(1);
    let key = store.push(None, Done(true)).unwrap();
    let Some(mut edit) = CellEdit::begin(&store, &DefaultEditorFactory, key, 0, locale("en"))
    else {
        panic!("a bool column has an editor");
    };
    assert_eq!(edit.kind, EditorKind::Bool);
    assert_eq!(edit.checked(), Some(true));
    assert_eq!(edit.filter().accept("", 0..0, "x"), None, "no typed text");
    edit.set_checked(false);
    assert_eq!(edit.checked(), Some(false));
    edit.commit(&mut store).unwrap();
    assert_eq!(store.data(key, 0, Role::Edit), Value::Bool(false));
}
