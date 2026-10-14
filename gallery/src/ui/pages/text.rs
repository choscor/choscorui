//! Text entry: single-line fields, the multi-line editor, and structured
//! inputs (numbers, masks, dates) with validation feedback.

use crate::ui::page::{Build, PageCx, column, section};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Binding, Cx, Dynamic, Env, Frame, HorizontalAlignment, Input, Text, TextEditor,
    VStack, View, ViewExt,
};
use choscorui::controls::{self, DateInput, InputMask, NumberInput, ValidationState};
use choscorui::core::{CivilDate, ColorRole, Space, TextPreset};
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

fn validate_required(env: &Env, value: &str) -> Option<String> {
    if value.is_empty() {
        Some(env.text("text-field.email_required", &[]))
    } else if !value.contains('@') {
        Some(env.text(
            "text-field.email_example",
            &[("example", "name@example.com")],
        ))
    } else {
        None
    }
}

/// One sample text field: its label, first value, and kind.
#[derive(Clone)]
struct Sample {
    label: String,
    value: String,
    password: bool,
    /// Return checks the value; an edit clears the error.
    required: bool,
    enabled: bool,
}

/// Plain, password, required, and disabled text fields.
struct TextFieldPage;

impl View for TextFieldPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let env = cx.env().clone();
        let text = |key| env.text(key, &[]);
        let samples = [
            Sample {
                label: text("text-field.name"),
                value: "Nguyễn Thị Lựu".into(),
                password: false,
                required: false,
                enabled: true,
            },
            Sample {
                label: text("text-field.password"),
                value: "hunter2".into(),
                password: true,
                required: false,
                enabled: true,
            },
            Sample {
                label: text("text-field.email"),
                value: String::new(),
                password: false,
                required: true,
                enabled: true,
            },
            Sample {
                label: text("text-field.disabled"),
                value: text("text-field.read_only"),
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

fn sample_field(cx: &mut Cx, sample: Sample) -> AnyView {
    let env = cx.env().clone();
    let Sample {
        label,
        value,
        password,
        required,
        enabled,
    } = sample;
    let value = cx.state(value);
    // The message of a failed check; `None` while the value may be valid.
    let error = cx.state(None::<String>);
    let typed = Binding::new(
        move || value.get(),
        move |text: String| {
            value.set(text);
            if required {
                error.set(None);
            }
        },
    );
    let name = label.clone();
    let input = if password {
        Input::password(name, typed)
    } else {
        Input::new(name, typed)
    }
    .with(move |mut input| {
        input.set_validation(match error.get() {
            Some(message) => ValidationState::Invalid(message.into()),
            None => ValidationState::Valid,
        });
        input
    })
    .on_activate(move || {
        if required {
            error.set(validate_required(&env, &value.get()));
        }
    })
    .frame(Frame::new().width(340.0))
    .disabled(!enabled);
    VStack::new((
        field_caption(move || match error.get() {
            Some(message) => format!("{label} — {message}"),
            None => label.clone(),
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

/// The multi-line editor.
struct TextEditorPage;

impl View for TextEditorPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let sample = cx.env().text("text-editor.sample", &[]);
        let text = cx.state(sample);
        let notes = cx.state(cx.env().text("text-editor.locked_text", &[]));
        column((
            TextEditor::new(t!("text-editor.document"), text.binding())
                .frame(Frame::size(620.0, 320.0)),
            section(
                t!("text-editor.disabled"),
                TextEditor::new(t!("text-editor.locked"), notes.binding())
                    .frame(Frame::size(620.0, 120.0))
                    .disabled(true),
            ),
        ))
    }
}

pub fn text_editor(_: &PageCx) -> AnyView {
    TextEditorPage.into_any()
}

/// How a structured field checks its text.
type Rule = Rc<dyn Fn(&str) -> ValidationState>;

/// What a structured field gives its input: a filter, and the check and
/// format of Return and blur.
type Make = Rc<dyn Fn(controls::Input) -> controls::Input>;

/// One structured-input field.
#[derive(Clone)]
struct RuleField {
    /// The page id, which prefixes the verdict strings.
    page: &'static str,
    label: String,
    raw: String,
    placeholder: String,
    rule: Rule,
    make: Make,
}

/// The line under a field of page `page`: whether its value is valid,
/// or why not.
fn verdict(env: &Env, page: &str, validation: &ValidationState) -> String {
    match validation {
        ValidationState::Valid => env.text(&format!("{page}.valid"), &[]),
        ValidationState::Intermediate => env.text(&format!("{page}.incomplete"), &[]),
        ValidationState::Invalid(error) => error.message(&env.localizer()),
    }
}

/// Structured fields, each validated as the user types. The field takes
/// only the keys that its rule allows, is invalid with the rule's
/// message, and describes its verdict to assistive technology.
struct RuleFields(Vec<RuleField>);

impl View for RuleFields {
    fn body(&self, cx: &mut Cx) -> impl View {
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

/// The caption, the field, and the verdict line of a structured field.
fn verdict_column(
    env: Env,
    page: &'static str,
    label: String,
    input: impl View,
    checked: impl Fn() -> ValidationState + Clone + 'static,
) -> AnyView {
    let invalid = {
        let checked = checked.clone();
        move || matches!(checked(), ValidationState::Invalid(_))
    };
    let message = Text::new(move || verdict(&env, page, &checked()))
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

fn rule_field(cx: &mut Cx, field: RuleField) -> AnyView {
    let RuleField {
        page,
        label,
        raw,
        placeholder,
        rule,
        make,
    } = field;
    let env = cx.env().clone();
    let raw = cx.state(raw);
    let checked = move || rule(&raw.get());
    let validated = {
        let checked = checked.clone();
        move |input: controls::Input| {
            let mut input = make(input.with_placeholder(&placeholder));
            input.set_validation(checked());
            input
        }
    };
    let input = Input::new(label.clone(), raw.binding())
        .with(validated)
        .accessibility_hint({
            let (checked, env) = (checked.clone(), env.clone());
            move || verdict(&env, page, &checked())
        })
        .frame(Frame::new().width(300.0));
    verdict_column(env, page, label, input, checked)
}

/// A page of structured fields that `fields` makes for the environment's
/// locale and catalog. The fields are made again when the locale changes.
struct RulePage(fn(&Env) -> Vec<RuleField>);

impl View for RulePage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let env = cx.env().clone();
        let fields = self.0;
        column(Dynamic::new(move || RuleFields(fields(&env))))
    }
}

fn number_fields(env: &Env) -> Vec<RuleField> {
    let locale = env.locale();
    let amount = NumberInput::new(locale.clone(), 2).with_range(Some(0.0), Some(1e9));
    let quantity = NumberInput::new(locale.clone(), 0).with_range(Some(1.0), Some(10_000.0));
    let sample = locale
        .format_decimal(1234.5, 2)
        .unwrap_or_else(|_| "1234.50".into());
    let field = |label: String, raw: String, placeholder: String, input: NumberInput| {
        let make = input.clone();
        RuleField {
            page: "number-input",
            label,
            raw,
            placeholder,
            rule: Rc::new(move |text| input.validate(text).0),
            make: Rc::new(move |field| field.with_number(make.clone())),
        }
    };
    vec![
        field(
            env.text("number-input.amount", &[("locale", &locale.tag())]),
            sample.clone(),
            sample,
            amount,
        ),
        field(
            env.text("number-input.quantity", &[]),
            "12".into(),
            "12".into(),
            quantity,
        ),
    ]
}

fn date_fields(env: &Env) -> Vec<RuleField> {
    let locale = env.locale();
    let any = DateInput::new(locale.clone());
    let placeholder = any.placeholder();
    let ranged = DateInput::new(locale.clone()).with_range(
        CivilDate::new(2026, 1, 1).ok(),
        CivilDate::new(2026, 12, 31).ok(),
    );
    let today = CivilDate::clamped(2026, 10, 12);
    let sample = any.format(today);
    let field = |label: String, raw: String, input: DateInput| {
        let make = input.clone();
        RuleField {
            page: "date-input",
            label,
            raw,
            placeholder: placeholder.clone(),
            rule: Rc::new(move |text| input.validate(text).0),
            make: Rc::new(move |field| field.with_date(make.clone())),
        }
    };
    vec![
        field(
            env.text("date-input.any", &[("format", &placeholder)]),
            sample,
            any,
        ),
        field(
            env.text("date-input.in_year", &[("year", "2026")]),
            String::new(),
            ranged,
        ),
    ]
}

/// The masked fields: a required phone number and an uppercased licence
/// plate. Each stores its value with the mask's literals. A pattern that
/// does not parse leaves its field out.
struct MaskPage;

impl View for MaskPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let env = cx.env().clone();
        let fields = [
            ("input-mask.phone", "0905", "(000) 000-0000"),
            ("input-mask.plate", "", ">AA-99999"),
        ];
        column(
            VStack::new(
                fields
                    .into_iter()
                    .filter_map(|(key, raw, pattern)| mask_field(cx, &env, key, raw, pattern))
                    .collect::<Vec<_>>(),
            )
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S4),
        )
    }
}

fn mask_field(cx: &mut Cx, env: &Env, key: &str, raw: &str, pattern: &str) -> Option<AnyView> {
    let mask = InputMask::parse(pattern).ok()?;
    let label = env.text(key, &[("mask", pattern)]);
    let value = cx.state(mask.apply(raw).display);
    let checked = {
        let mask = mask.clone();
        move || mask.validate(&value.get())
    };
    let input = Input::masked(label.clone(), mask, value.binding())
        .accessibility_hint({
            let (checked, env) = (checked.clone(), env.clone());
            move || verdict(&env, "input-mask", &checked())
        })
        .frame(Frame::new().width(300.0));
    Some(verdict_column(
        env.clone(),
        "input-mask",
        label,
        input,
        checked,
    ))
}

pub fn number_input(_: &PageCx) -> AnyView {
    RulePage(number_fields).into_any()
}

pub fn input_mask(_: &PageCx) -> AnyView {
    MaskPage.into_any()
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
    const PHONE: &str = "Phone, mask (000) 000-0000";
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

    /// Sets the field's value as assistive technology would, expects the
    /// field's filter to refuse it, and returns the semantics.
    fn refuse(page: &mut Harness, name: &str, text: &str) -> SemanticNode {
        let before = semantic(page, name).value;
        let node = page.node(name);
        page.host
            .perform(node, SemanticAction::SetValue, Some(text));
        page.host.layout();
        let field = semantic(page, name);
        assert_eq!(field.value, before, "a refused value changes nothing");
        field
    }

    /// Types `text` one character at a time into the focused field.
    fn type_text(page: &mut Harness, text: &str) {
        for ch in text.chars() {
            page.host.text(&ch.to_string());
            page.host.layout();
        }
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
            ("input-mask", [PHONE, PLATE]),
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
        // The field refuses each letter, so typing `12abc` leaves `12`.
        enter(&mut page, AMOUNT, "");
        page.act(AMOUNT, SemanticAction::Focus);
        type_text(&mut page, "12abc");
        let letters = semantic(&mut page, AMOUNT);
        assert_eq!(letters.value.as_deref(), Some("12"));
        assert!(!letters.state.invalid, "{letters:?}");
        refuse(&mut page, AMOUNT, "1-2");
        refuse(&mut page, AMOUNT, "1.2.3");
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
        // The plate needs letters first, so no digit fits and the field
        // refuses them.
        let digits = refuse(&mut page, PLATE, "12");
        assert_eq!(digits.value.as_deref(), Some(""));
        let partial = enter(&mut page, PLATE, "a");
        assert!(rejected(&partial) && !partial.state.invalid, "{partial:?}");
        assert_eq!(partial.description, "Incomplete");
        assert_eq!(partial.value.as_deref(), Some("A"), "`>` uppercases");
        let full = enter(&mut page, PLATE, "ab12345");
        assert!(!rejected(&full), "{full:?}");
        assert_eq!(full.value.as_deref(), Some("AB-12345"));
    }

    #[test]
    fn typing_into_a_mask_fills_it_and_backspace_removes_input() {
        let mut page = Harness::new("input-mask");
        let phone = PHONE;
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
        refuse(&mut page, ANY_DATE, "not a date");
        refuse(&mut page, ANY_DATE, "13/");
        let text = enter(&mut page, ANY_DATE, "2/30/2026");
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
