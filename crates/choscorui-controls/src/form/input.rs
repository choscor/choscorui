//! A single-line text input mounted through the control host.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::style::{ControlKind, PaintContext, ResolvedStyle, Slot, StyleQuery};
use crate::text_field::{TextField, ValidationState};
use crate::{CharClass, DateInput, InputFilter, InputMask, NumberInput, ValidationError};
use choscorui_core::{
    EventPhase, EventReply, Key, KeyEvent, Localizer, Measured, SemanticAction, SemanticNode, Size,
    TextInputInfo,
};
use std::rc::Rc;

/// The kind of text that an [`Input`] takes. A kind sets the filter where
/// it has one, and the check that Return and blur run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InputType {
    /// Any text.
    #[default]
    Text,
    /// An email address, such as `name@example.com`.
    Email,
    /// A web address, such as `https://example.com`.
    Url,
    /// A phone number: digits, spaces, and `+ - ( ) .`.
    Tel,
    /// A search query: any text.
    Search,
    /// A number: digits, one leading minus, and one `.`.
    Number,
}

impl InputType {
    /// The filter of the kind, if it has one.
    fn filter(self) -> Option<InputFilter> {
        match self {
            Self::Tel => Some(InputFilter::new().chars(CharClass::Custom(Rc::new(|ch| {
                ch.is_ascii_digit() || matches!(ch, ' ' | '+' | '-' | '(' | ')' | '.')
            })))),
            Self::Number => Some(InputFilter::new().rule(|text| {
                let digits = text.strip_prefix('-').unwrap_or(text);
                digits.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
                    && digits.matches('.').count() <= 1
            })),
            Self::Text | Self::Email | Self::Url | Self::Search => None,
        }
    }

    /// The error of `text` for this kind, or `None` when it fits. Empty
    /// text fits every kind.
    fn check(self, text: &str) -> Option<ValidationError> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let fits = match self {
            Self::Text | Self::Search => true,
            Self::Email => text.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty()
                    && !domain.contains('@')
                    && domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
                    && !text.contains(char::is_whitespace)
            }),
            Self::Url => text.split_once("://").is_some_and(|(scheme, rest)| {
                !scheme.is_empty()
                    && scheme
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '+')
                    && !rest.is_empty()
                    && !rest.contains(char::is_whitespace)
            }),
            Self::Tel => text.chars().filter(char::is_ascii_digit).count() >= 3,
            Self::Number => text.parse::<f64>().is_ok_and(f64::is_finite),
        };
        (!fits).then_some(match self {
            Self::Email => ValidationError::Email,
            Self::Url => ValidationError::Url,
            Self::Tel => ValidationError::Tel,
            _ => ValidationError::Number(crate::NumberError::NotANumber),
        })
    }
}

/// A locale rule that an [`Input`] applies to its text.
#[derive(Clone, Debug)]
enum Rule {
    None,
    Number(NumberInput),
    Date(DateInput),
}

/// A text input: the `Input` recipe box around one line of editable text.
/// It fills the width it is given.
///
/// A filter rejects keys, pastes, and set values that it does not allow:
/// the text stays as it was, and the input beeps. A kind or a rule also
/// checks the text when the user presses Return or moves focus away.
pub struct Input {
    editor: TextField,
    kind: InputType,
    rule: Rule,
    /// The validation that the application sets.
    validation: ValidationState,
    /// The validation from the kind or the rule.
    own: ValidationState,
}

impl Input {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self::with_editor(TextField::new(name, value))
    }

    /// An input that shows bullets in place of its characters. Cut and
    /// copy do nothing in it.
    pub fn password(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self::with_editor(TextField::password(name, value))
    }

    fn with_editor(editor: TextField) -> Self {
        Self {
            editor,
            kind: InputType::Text,
            rule: Rule::None,
            validation: ValidationState::Valid,
            own: ValidationState::Valid,
        }
    }

    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.editor = self.editor.with_placeholder(placeholder);
        self
    }

    /// Lets the user type, paste, and set only what `filter` accepts.
    #[must_use]
    pub fn with_filter(mut self, filter: InputFilter) -> Self {
        self.editor.set_filter(filter);
        self
    }

    /// Allows at most `length` grapheme clusters. A paste longer than the
    /// room left is cut to the room.
    #[must_use]
    pub fn max_length(self, length: usize) -> Self {
        let filter = self.editor.filter().clone().max_length(length);
        self.with_filter(filter)
    }

    /// Allows only the characters of `class`.
    #[must_use]
    pub fn filter(self, class: CharClass) -> Self {
        let filter = self.editor.filter().clone().chars(class);
        self.with_filter(filter)
    }

    /// Fits the text into `mask`. The value is the text with the mask's
    /// literals. The caret keeps its place among the typed characters.
    #[must_use]
    pub fn masked(self, mask: InputMask) -> Self {
        let value = mask.apply(self.value()).display;
        let mut input = self.with_filter(InputFilter::masked(mask));
        input.editor.replace_value(&value);
        input
    }

    /// Takes text of `kind`: it sets the kind's filter, if any, and checks
    /// the text on Return and blur.
    #[must_use]
    pub fn with_type(mut self, kind: InputType) -> Self {
        self.kind = kind;
        match kind.filter() {
            Some(filter) => self.with_filter(filter),
            None => self,
        }
    }

    /// Takes a number of `rule`: the filter rejects text that cannot
    /// become a number. Return and blur format a valid number in the
    /// locale, and mark a value outside the range invalid.
    #[must_use]
    pub fn with_number(mut self, rule: NumberInput) -> Self {
        self.own = rule.validate(self.value()).0;
        let filter = rule.filter();
        self.rule = Rule::Number(rule);
        self.sync();
        self.with_filter(filter)
    }

    /// Takes a date of `rule`: the filter rejects text that cannot become
    /// a date. Return and blur format a valid date in the locale.
    #[must_use]
    pub fn with_date(mut self, rule: DateInput) -> Self {
        self.own = rule.validate(self.value()).0;
        let filter = rule.filter();
        self.rule = Rule::Date(rule);
        self.sync();
        self.with_filter(filter)
    }

    /// The text model: value, selection, and composition.
    pub fn field(&self) -> &TextField {
        &self.editor
    }

    pub fn value(&self) -> &str {
        self.editor.value()
    }

    /// Marks the input invalid with an error, or valid. An error of the
    /// kind or the rule shows while the application's validation is not
    /// invalid.
    pub fn set_validation(&mut self, validation: ValidationState) {
        self.validation = validation;
        self.sync();
    }

    /// The validation that the input shows.
    pub fn validation(&self) -> ValidationState {
        match (&self.validation, &self.own) {
            (ValidationState::Invalid(_), _) | (_, ValidationState::Valid) => {
                self.validation.clone()
            }
            (_, own) => own.clone(),
        }
    }

    /// Gives the field the validation that it shows.
    fn sync(&mut self) {
        let validation = self.validation();
        self.editor.set_validation_state(validation);
    }

    /// The validation of the text while the user types.
    fn typed(&self) -> ValidationState {
        let text = self.value();
        match &self.rule {
            Rule::Number(rule) => rule.validate(text).0,
            Rule::Date(rule) => rule.validate(text).0,
            Rule::None => ValidationState::Valid,
        }
    }

    /// Checks the text as the user commits it. A valid number or date
    /// gets its locale format, reported as a change. Returns true when
    /// the text is valid.
    fn commit(&mut self, cx: &mut ControlContext<'_>) -> bool {
        let text = self.value().to_owned();
        let (state, formatted) = match &self.rule {
            Rule::Number(rule) => {
                let (state, value) = rule.commit(&text);
                (state, value.map(|value| rule.format(value)))
            }
            Rule::Date(rule) => {
                let (state, date) = rule.commit(&text);
                (state, date.map(|(_, text)| text))
            }
            Rule::None => match self.kind.check(&text) {
                Some(error) => (ValidationState::Invalid(error), None),
                None => (ValidationState::Valid, None),
            },
        };
        if let Some(formatted) = formatted.filter(|formatted| *formatted != text) {
            self.editor.replace_value(&formatted);
            cx.emit(ControlMessage::Changed(ControlValue::Text(formatted)));
        }
        let valid = state == ValidationState::Valid;
        if state != self.own {
            self.own = state;
            self.sync();
            cx.request_paint();
            cx.request_semantics();
        }
        valid
    }

    fn style(&self, style: impl Fn(StyleQuery) -> ResolvedStyle) -> ResolvedStyle {
        style(StyleQuery::new(ControlKind::Input, Slot::Root))
    }
}

/// The width an input asks for without a width constraint: 60 spacing
/// units.
pub(crate) fn default_width(cx: &MeasureContext<'_>, style: &ResolvedStyle) -> f64 {
    let natural = (cx.tokens.spacing * 60.0).max(style.min_width);
    let max = cx.constraints.max.width;
    if max.is_finite() {
        max.max(style.min_width)
    } else {
        natural
    }
}

impl CustomControl for Input {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = self.style(|query| cx.style(query));
        let height = (style.text.line_height + style.padding.vertical()).max(style.min_height);
        Measured {
            size: Size::new(default_width(cx, &style), height),
            baseline: None,
        }
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let checks = !matches!(self.rule, Rule::None) || self.kind != InputType::Text;
        match event {
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Enter, ..
            }) if checks && cx.is_focused() => {
                if self.commit(cx) && self.validation == ValidationState::Valid {
                    cx.emit(ControlMessage::Activated);
                }
                return EventReply::Stop;
            }
            ControlEvent::FocusChanged(false) if checks => {
                self.commit(cx);
            }
            _ => {}
        }
        let before = self.value().to_owned();
        let bounds = cx.bounds();
        let reply = self.editor.hosted_event(cx, event, bounds);
        if checks && self.value() != before {
            self.own = self.typed();
            self.sync();
        }
        reply
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = self.editor.hosted_state(cx.state);
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root).state(state));
        let bounds = cx.bounds;
        let editor = &self.editor;
        cx.slot(bounds, &style, |cx, inner| {
            editor.paint_hosted_text(cx, inner, state)
        });
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        self.editor.hosted_semantics(strings)
    }

    fn text_input(&self) -> Option<TextInputInfo> {
        self.editor.hosted_text_input()
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::SetValue => {
                let before = self.value().to_owned();
                let set = self.editor.hosted_set_value(cx, value.unwrap_or_default());
                if self.value() != before {
                    self.own = self.typed();
                    self.sync();
                }
                set
            }
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.editor.set_enabled_state(enabled);
    }

    /// Takes `Text`.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let taken = self.editor.take_text(value);
        if taken && !matches!(self.rule, Rule::None) {
            self.own = self.typed();
            self.sync();
        }
        taken
    }
}
