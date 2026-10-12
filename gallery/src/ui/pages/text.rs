//! Text entry: single-line fields, the multi-line editor, and structured
//! inputs (numbers, masks, dates) with validation feedback.

use crate::ui::page::{Build, PageCx, column};
use choscorui::compose::view::{
    AnyView, Configured, Cx, Dynamic, Frame, HorizontalAlignment, Text, TextEditor, VStack, View,
    ViewExt,
};
use choscorui::controls::{
    self, ControlKind, ControlMessage, ControlValue, DateInput, InputMask, NumberInput,
    ValidationState,
};
use choscorui::core::{CivilDate, ColorRole, Locale, Space, TextPreset};
use std::rc::Rc;

/// A small muted caption that follows `text`. Captions repeat the name of
/// the field below them, so assistive technology skips them and reads
/// the field.
fn field_caption(text: impl Fn() -> String + 'static) -> impl View {
    Text::new(text)
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
        .accessibility_hidden(true)
}

fn validate_required(value: &str) -> Option<String> {
    if value.is_empty() {
        Some("An email address is required".into())
    } else if !value.contains('@') {
        Some("Enter an address like name@example.com".into())
    } else {
        None
    }
}

/// One sample text field: its label, first value, and kind.
#[derive(Clone, Copy)]
struct Sample {
    label: &'static str,
    value: &'static str,
    password: bool,
    /// Return checks the value; an edit clears the error.
    required: bool,
    enabled: bool,
}

/// Plain, password, required, and disabled text fields.
struct TextFieldPage;

impl View for TextFieldPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let samples = [
            Sample {
                label: "Name",
                value: "Nguyễn Thị Lựu",
                password: false,
                required: false,
                enabled: true,
            },
            Sample {
                label: "Password",
                value: "hunter2",
                password: true,
                required: false,
                enabled: true,
            },
            Sample {
                label: "Email (required; Return validates)",
                value: "",
                password: false,
                required: true,
                enabled: true,
            },
            Sample {
                label: "Disabled",
                value: "Read only",
                password: false,
                required: false,
                enabled: false,
            },
        ];
        column(
            samples
                .into_iter()
                .map(|sample| sample_field(cx, sample))
                .collect::<Vec<_>>(),
        )
    }
}

fn sample_field(cx: &mut Cx<'_>, sample: Sample) -> AnyView {
    let value = cx.state(sample.value.to_owned());
    // The message of a failed check; `None` while the value may be valid.
    let error = cx.state(None::<String>);
    let label = sample.label;
    let input = Configured::custom(ControlKind::Input, move |_| {
        let mut input = if sample.password {
            controls::Input::password(label, "")
        } else {
            controls::Input::new(label, "")
        };
        input.set_validation(match error.get() {
            Some(message) => ValidationState::Invalid(message),
            None => ValidationState::Valid,
        });
        input
    })
    .value(move || ControlValue::Text(value.get()))
    .on_message(move |message| match message {
        ControlMessage::Changed(ControlValue::Text(text)) => {
            value.set(text.clone());
            if sample.required {
                error.set(None);
            }
        }
        ControlMessage::Activated if sample.required => {
            error.set(validate_required(&value.get()));
        }
        _ => {}
    })
    .frame(Frame::new().width(340.0))
    .disabled(!sample.enabled);
    VStack::new((
        field_caption(move || match error.get() {
            Some(message) => format!("{label} — {message}"),
            None => label.to_owned(),
        }),
        input,
    ))
    .alignment(HorizontalAlignment::Leading)
    .spacing(Space::S1)
    .into_any()
}

pub fn text_field(_: &PageCx) -> AnyView {
    TextFieldPage.into_any()
}

const EDITOR_TEXT: &str = "Multilingual editing: Tiếng Việt có dấu, 日本語, 中文, and emoji 👩‍💻.\n\
Click to place the caret, drag to select, and use the arrow keys with Shift and Option.\n\
⌘A selects all; ⌘Z undoes and ⇧⌘Z redoes.";

/// The multi-line editor.
struct TextEditorPage;

impl View for TextEditorPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let text = cx.state(EDITOR_TEXT.to_owned());
        column(TextEditor::new("Document", text.binding()).frame(Frame::size(620.0, 320.0)))
    }
}

pub fn text_editor(_: &PageCx) -> AnyView {
    TextEditorPage.into_any()
}

/// How a structured field turns raw text into what it shows and whether
/// it is valid.
type Rule = Rc<dyn Fn(&str) -> (String, ValidationState)>;

/// One structured-input field.
#[derive(Clone)]
struct RuleField {
    label: String,
    raw: String,
    placeholder: String,
    rule: Rule,
}

/// The line under a field: whether its value is valid, or why not.
fn verdict(validation: &ValidationState) -> String {
    match validation {
        ValidationState::Valid => "Valid".into(),
        ValidationState::Intermediate => "Incomplete".into(),
        ValidationState::Invalid(message) => message.clone(),
    }
}

/// Structured fields, each validated as the user types. The field shows
/// what its rule makes of the raw text, is invalid with the rule's
/// message, and describes its verdict to assistive technology.
struct RuleFields(Vec<RuleField>);

impl View for RuleFields {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        VStack::new(
            self.0
                .iter()
                .map(|field| rule_field(cx, field.clone()))
                .collect::<Vec<_>>(),
        )
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S4)
    }
}

fn rule_field(cx: &mut Cx<'_>, field: RuleField) -> AnyView {
    let RuleField {
        label,
        raw,
        placeholder,
        rule,
    } = field;
    let raw = cx.state(raw);
    let checked = {
        let rule = rule.clone();
        move || rule(&raw.get())
    };
    let shown = {
        let checked = checked.clone();
        move || checked().0
    };
    let name = label.clone();
    let build = {
        let checked = checked.clone();
        move |_: &_| {
            let mut input = controls::Input::new(name.clone(), "").with_placeholder(&placeholder);
            input.set_validation(checked().1);
            input
        }
    };
    let typed = {
        let shown = shown.clone();
        move |message: &ControlMessage| {
            let ControlMessage::Changed(ControlValue::Text(text)) = message else {
                return;
            };
            let before = shown();
            if text.len() < before.len() && before.starts_with(text.as_str()) {
                // A deletion at the end removes raw characters until the
                // shown text is no longer than what is left, so a mask
                // does not put a deleted literal back.
                raw.update(
                    |raw| {
                        while rule(raw).0.len() > text.len() && raw.pop().is_some() {}
                    },
                );
            } else {
                raw.set(text.clone());
            }
        }
    };
    let input = Configured::custom(ControlKind::Input, build)
        .value(move || ControlValue::Text(shown()))
        .on_message(typed)
        .accessibility_hint({
            let checked = checked.clone();
            move || verdict(&checked().1)
        })
        .frame(Frame::new().width(300.0));
    let invalid = {
        let checked = checked.clone();
        move || matches!(checked().1, ValidationState::Invalid(_))
    };
    let message = Text::new(move || verdict(&checked().1))
        .text_style(TextPreset::Small)
        .foreground(move || {
            if invalid() {
                ColorRole::Destructive
            } else {
                ColorRole::MutedForeground
            }
        })
        .accessibility_hidden(true);
    VStack::new((field_caption(move || label.clone()), input, message))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S1)
        .into_any()
}

/// A page of structured fields that `fields` makes for the locale. The
/// fields are made again when the locale changes.
struct RulePage(fn(&Locale) -> Vec<RuleField>);

impl View for RulePage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let env = cx.env().clone();
        let fields = self.0;
        column(Dynamic::new(move || RuleFields(fields(&env.locale()))))
    }
}

fn number_fields(locale: &Locale) -> Vec<RuleField> {
    let amount = NumberInput::new(locale.clone(), 2).with_range(Some(0.0), Some(1e9));
    let quantity = NumberInput::new(locale.clone(), 0).with_range(Some(1.0), Some(10_000.0));
    let sample = locale
        .format_decimal(1234.5, 2)
        .unwrap_or_else(|_| "1234.50".into());
    vec![
        RuleField {
            label: format!(
                "Amount (0 – 1,000,000,000; 2 decimals, locale {})",
                locale.tag()
            ),
            raw: sample.clone(),
            placeholder: sample,
            rule: Rc::new(move |text| (text.to_owned(), amount.validate(text).0)),
        },
        RuleField {
            label: "Quantity (1 – 10,000; whole numbers)".into(),
            raw: "12".into(),
            placeholder: "12".into(),
            rule: Rc::new(move |text| (text.to_owned(), quantity.validate(text).0)),
        },
    ]
}

fn mask_fields(_: &Locale) -> Vec<RuleField> {
    let masked = |label: &str, raw: &str, pattern: &str| {
        let mask = InputMask::parse(pattern).expect("demo masks are valid");
        RuleField {
            label: label.into(),
            raw: raw.into(),
            placeholder: mask.placeholder('_'),
            rule: Rc::new(move |raw| (mask.apply(raw).display, mask.validate(raw))),
        }
    };
    vec![
        masked("Phone, mask (999) 999-9999", "0905", "(999) 999-9999"),
        masked(
            "Licence plate, mask >AA-99999 (uppercased)",
            "",
            ">AA-99999",
        ),
    ]
}

fn date_fields(locale: &Locale) -> Vec<RuleField> {
    let any = DateInput::new(locale.clone());
    let placeholder = any.placeholder();
    let ranged = DateInput::new(locale.clone()).with_range(
        CivilDate::new(2026, 1, 1).ok(),
        CivilDate::new(2026, 12, 31).ok(),
    );
    let today = CivilDate::new(2026, 10, 12).expect("valid date");
    let sample = any.format(today);
    vec![
        RuleField {
            label: format!("Any date ({placeholder})"),
            raw: sample,
            placeholder: placeholder.clone(),
            rule: Rc::new(move |text| (text.to_owned(), any.validate(text).0)),
        },
        RuleField {
            label: "A date in 2026".into(),
            raw: String::new(),
            placeholder,
            rule: Rc::new(move |text| (text.to_owned(), ranged.validate(text).0)),
        },
    ]
}

pub fn number_input(_: &PageCx) -> AnyView {
    RulePage(number_fields).into_any()
}

pub fn input_mask(_: &PageCx) -> AnyView {
    RulePage(mask_fields).into_any()
}

pub fn date_input(_: &PageCx) -> AnyView {
    RulePage(date_fields).into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "text-field" => text_field,
        "text-editor" => text_editor,
        "number-input" => number_input,
        "input-mask" => input_mask,
        "date-input" => date_input,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::core::{Key, KeyEvent, SemanticAction, SemanticNode, SemanticRole};

    const AMOUNT: &str = "Amount (0 – 1,000,000,000; 2 decimals, locale en)";
    const QUANTITY: &str = "Quantity (1 – 10,000; whole numbers)";
    const PLATE: &str = "Licence plate, mask >AA-99999 (uppercased)";
    const ANY_DATE: &str = "Any date (mm/dd/yyyy)";
    const DATE_2026: &str = "A date in 2026";

    fn semantic(page: &mut Harness, name: &str) -> SemanticNode {
        let node = page.node(name);
        page.host.semantic(node).expect("the field is published")
    }

    /// Sets the field's value as assistive technology would and returns
    /// its semantics.
    fn enter(page: &mut Harness, name: &str, text: &str) -> SemanticNode {
        let node = page.node(name);
        assert!(
            page.host
                .perform(node, SemanticAction::SetValue, Some(text))
        );
        page.host.layout();
        semantic(page, name)
    }

    /// Rejected: the field is invalid, or its verdict is not "Valid".
    fn rejected(node: &SemanticNode) -> bool {
        node.state.invalid || node.description != "Valid"
    }

    fn press(page: &mut Harness, key: Key) {
        page.host
            .key(KeyEvent::Down { key, repeat: false }, Default::default());
        page.host.layout();
    }

    #[test]
    fn structured_input_pages_hold_two_fields_each() {
        for (id, names) in [
            ("number-input", [AMOUNT, QUANTITY]),
            ("input-mask", ["Phone, mask (999) 999-9999", PLATE]),
            ("date-input", [ANY_DATE, DATE_2026]),
        ] {
            let mut page = Harness::new(id);
            for name in names {
                let field = semantic(&mut page, name);
                assert_eq!(field.role, SemanticRole::TextField, "{id}: {name}");
            }
        }
    }

    #[test]
    fn number_input_rejects_letters_and_out_of_range_values() {
        let mut page = Harness::new("number-input");
        assert_eq!(
            semantic(&mut page, AMOUNT).value.as_deref(),
            Some("1,234.50")
        );
        let letters = enter(&mut page, AMOUNT, "12abc");
        assert!(letters.state.invalid, "{letters:?}");
        assert_eq!(letters.description, "Enter a number");
        // Zero is below the minimum of one; more digits could still reach
        // it, so the field reports it incomplete rather than invalid.
        let low = enter(&mut page, QUANTITY, "0");
        assert!(rejected(&low), "{low:?}");
        assert_eq!(low.description, "Incomplete");
        let high = enter(&mut page, QUANTITY, "20000");
        assert!(high.state.invalid, "{high:?}");
        assert_eq!(high.description, "Must be at most 10,000");
        let ok = enter(&mut page, QUANTITY, "12");
        assert!(!rejected(&ok), "{ok:?}");
        assert_eq!(ok.value.as_deref(), Some("12"));
    }

    #[test]
    fn input_mask_rejects_characters_its_format_forbids() {
        let mut page = Harness::new("input-mask");
        let digits = enter(&mut page, PLATE, "12");
        assert!(digits.state.invalid, "{digits:?}");
        // The plate needs letters first, so the digits never show.
        assert_eq!(digits.value.as_deref(), Some(""));
        let partial = enter(&mut page, PLATE, "a");
        assert!(rejected(&partial) && !partial.state.invalid, "{partial:?}");
        assert_eq!(partial.description, "Incomplete");
        assert_eq!(partial.value.as_deref(), Some(">a"));
        let full = enter(&mut page, PLATE, "ab12345");
        assert!(!rejected(&full), "{full:?}");
        assert_eq!(full.value.as_deref(), Some(">ab-12345"));
    }

    #[test]
    fn typing_into_a_mask_fills_it_and_backspace_removes_input() {
        let mut page = Harness::new("input-mask");
        let phone = "Phone, mask (999) 999-9999";
        assert_eq!(semantic(&mut page, phone).value.as_deref(), Some("(090) 5"));
        page.act(phone, SemanticAction::Focus);
        assert!(page.host.text("1"));
        page.host.layout();
        assert_eq!(
            semantic(&mut page, phone).value.as_deref(),
            Some("(090) 51")
        );
        press(&mut page, Key::Backspace);
        press(&mut page, Key::Backspace);
        assert_eq!(semantic(&mut page, phone).value.as_deref(), Some("(090) "));
        press(&mut page, Key::Backspace);
        assert_eq!(semantic(&mut page, phone).value.as_deref(), Some("(09"));
    }

    #[test]
    fn date_input_rejects_text_and_dates_outside_2026() {
        let mut page = Harness::new("date-input");
        assert_eq!(
            semantic(&mut page, ANY_DATE).value.as_deref(),
            Some("10/12/2026")
        );
        let text = enter(&mut page, ANY_DATE, "not a date");
        assert!(text.state.invalid, "{text:?}");
        assert_eq!(text.description, "Enter a valid date");
        let outside = enter(&mut page, DATE_2026, "1/1/2025");
        assert!(outside.state.invalid, "{outside:?}");
        assert_eq!(outside.description, "Date is outside the allowed range");
        let inside = enter(&mut page, DATE_2026, "3/4/2026");
        assert!(!rejected(&inside), "{inside:?}");
    }

    #[test]
    fn required_email_validates_on_return_and_clears_on_edit() {
        let mut page = Harness::new("text-field");
        let email = "Email (required; Return validates)";
        page.act(email, SemanticAction::Focus);
        press(&mut page, Key::Enter);
        let empty = semantic(&mut page, email);
        assert!(empty.state.invalid, "{empty:?}");
        assert_eq!(empty.description, "An email address is required");
        assert!(page.host.text("luu"));
        page.host.layout();
        assert!(!semantic(&mut page, email).state.invalid);
        press(&mut page, Key::Enter);
        assert_eq!(
            semantic(&mut page, email).description,
            "Enter an address like name@example.com"
        );
        assert!(page.host.text("@example.com"));
        press(&mut page, Key::Enter);
        let valid = semantic(&mut page, email);
        assert!(!valid.state.invalid, "{valid:?}");
        assert_eq!(valid.value.as_deref(), Some("luu@example.com"));
    }

    #[test]
    fn text_fields_keep_their_samples_and_the_disabled_one_refuses_edits() {
        let mut page = Harness::new("text-field");
        assert_eq!(page.value("Name").as_deref(), Some("Nguyễn Thị Lựu"));
        let disabled = semantic(&mut page, "Disabled");
        assert!(disabled.state.disabled);
        assert_eq!(disabled.value.as_deref(), Some("Read only"));
        let node = page.node("Disabled");
        assert!(!page.host.perform(node, SemanticAction::SetValue, Some("x")));
        assert_eq!(page.value("Disabled").as_deref(), Some("Read only"));
    }

    #[test]
    fn the_editor_takes_text_through_semantics() {
        let mut page = Harness::new("text-editor");
        assert!(
            page.value("Document")
                .unwrap()
                .starts_with("Multilingual editing")
        );
        let node = page.node("Document");
        assert!(
            page.host
                .perform(node, SemanticAction::SetValue, Some("Xin chào"))
        );
        assert_eq!(page.value("Document").as_deref(), Some("Xin chào"));
    }
}
