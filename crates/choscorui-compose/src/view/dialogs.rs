//! View constructors for the dialog models that are not hosted controls:
//! [`InputDialog`] and [`FontDialog`]. The models in `choscorui-controls`
//! validate the value, handle the keys, and resolve a missing font; the
//! views mount their fields and buttons and connect them to state.

use super::configured::Configured;
use super::dynamic::Dynamic;
use super::env::Fill;
use super::host::Cx;
use super::layout::{HStack, HorizontalAlignment, VStack, WrapRow};
use super::modifier::ViewExt;
use super::state::{Binding, IntoLive};
use super::text::wrap_lines;
use super::text::{Localized, Text, TextContent};
use super::widgets::{Button, Input, Label, Select, Toggle, label, text};
use super::widgets_form::as_number;
use super::{Empty, View};
use crate::reactive::Runtime;
use choscorui_controls as controls;
use choscorui_controls::{
    ButtonVariant, ComboInput, ControlContext, ControlEvent, ControlKind, ControlMessage,
    ControlValue, CustomControl, InputKind, InputValue, ListEntry, MeasureContext, PaintContext,
    ResolvedStyle, Slot, StyleQuery, TextMeasure, ValidationState, paint_box,
};
use choscorui_core::{
    ColorRole, EventReply, FontSlant, FontWeight, Key, Measured, Point, SemanticNode, SemanticRole,
    SemanticState, Size, Space, TextPreset, TextRun, TextStyle, TypeSpec,
};
use std::rc::Rc;

/// A modal dialog for one validated value. While `presented` is true, it
/// shows the title, the prompt, a field that edits `text`, the validation
/// message, and Cancel and accept buttons. A Choice kind uses a `Select`,
/// and the other kinds use an `Input`. The accept button is enabled only
/// while the value is valid, and Return in the field accepts only a valid
/// value. Both run `on_accept` with the value and close the dialog.
/// Cancel, Escape, and a press outside the dialog close it without a
/// value. Focus moves into the dialog when it opens, and returns when it
/// closes. `controls::InputDialog` validates the text for `kind` in the
/// locale of the environment. An Integer or Decimal field rejects keys
/// that cannot make a number. A number below the minimum is incomplete
/// while the user types, and invalid when the user accepts it. Accept
/// writes the value back to `text`: a number in the locale format, and
/// text without the spaces at its ends.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::InputKind;
/// struct Rename;
///
/// impl View for Rename {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let name = cx.state(String::new());
///         let open = cx.state(false);
///         let kind = InputKind::Text { allow_empty: false };
///         VStack::new((
///             Button::new("Rename…", move || open.set(true)),
///             InputDialog::new(
///                 ("Rename", "New name", "Rename"),
///                 kind,
///                 name.binding(),
///                 open.binding(),
///                 |value| println!("{value:?}"),
///             ),
///         ))
///     }
/// }
/// ```
pub struct InputDialog(Rc<InputParts>);

/// What an input dialog shows and edits.
struct InputParts {
    title: Label,
    prompt: Label,
    accept: Label,
    kind: InputKind,
    text: Binding<String>,
    presented: Binding<bool>,
    on_accept: Box<dyn Fn(InputValue)>,
}

impl InputDialog {
    /// An input dialog with a title, a prompt, and an accept button label,
    /// for a value of `kind`, open while `presented` is true.
    pub fn new<M, N, O>(
        (title, prompt, accept): (
            impl IntoLive<TextContent, M>,
            impl IntoLive<TextContent, N>,
            impl IntoLive<TextContent, O>,
        ),
        kind: InputKind,
        text: Binding<String>,
        presented: Binding<bool>,
        on_accept: impl Fn(InputValue) + 'static,
    ) -> Self {
        Self(Rc::new(InputParts {
            title: label(title),
            prompt: label(prompt),
            accept: label(accept),
            kind,
            text,
            presented,
            on_accept: Box::new(on_accept),
        }))
    }
}

impl View for InputDialog {
    fn mount(self, cx: &mut Cx) {
        let parts = self.0;
        let (presented, title) = (parts.presented, parts.title.clone());
        Empty
            .dialog(presented, title, move || InputPanel(parts.clone()))
            .mount(cx);
    }
}

/// The content of an open input dialog.
struct InputPanel(Rc<InputParts>);

impl View for InputPanel {
    fn mount(self, cx: &mut Cx) {
        let parts = self.0;
        let env = cx.env().clone();
        let message_env = env.clone();
        let value = parts.text;
        let model = {
            let parts = parts.clone();
            Rc::new(move || {
                controls::InputDialog::new(
                    text(&parts.title, &env),
                    text(&parts.prompt, &env),
                    parts.kind.clone(),
                    env.locale(),
                    value.try_get().unwrap_or_default(),
                )
            })
        };
        let validation = {
            let model = model.clone();
            cx.memo(move || model().validate().0)
        };
        // The validation of the last accept that failed, until an edit.
        let refused = cx.state(None::<ValidationState>);
        // A memo reads as incomplete after the view unmounts.
        let current = move || match refused.try_get().ok().flatten() {
            Some(state) => state,
            None => validation
                .try_get()
                .unwrap_or(ValidationState::Intermediate),
        };
        let submit = {
            let (model, parts) = (model.clone(), parts.clone());
            move || {
                let model = model();
                match model.commit() {
                    (_, Some(accepted)) => {
                        let shown = match &accepted {
                            InputValue::Integer(number) => {
                                model.number().map(|rule| rule.format(*number as f64))
                            }
                            InputValue::Decimal(number) => {
                                model.number().map(|rule| rule.format(*number))
                            }
                            InputValue::Text(text) => Some(text.clone()),
                            InputValue::Choice(_) => None,
                        };
                        if let Some(shown) = shown {
                            value.set(shown);
                        }
                        (parts.on_accept)(accepted);
                        parts.presented.set(false);
                    }
                    (state, None) => refused.set(Some(state)),
                }
            }
        };
        let field = match &parts.kind {
            InputKind::Choice(options) => {
                choice(parts.prompt.clone(), options, value, refused).into_any()
            }
            _ => {
                let filter = model.clone();
                Input::new(parts.prompt.clone(), value)
                    .with(move |input| {
                        let mut input = input.with_filter(filter().filter());
                        input.set_validation(current());
                        input
                    })
                    .on_message(move |message| {
                        if let ControlMessage::Changed(_) = message {
                            refused.set(None);
                        }
                    })
                    .on_activate(submit.clone())
                    .into_any()
            }
        };
        // The field publishes the message, so the visible copy is hidden.
        let message = Dynamic::new(move || match current() {
            ValidationState::Invalid(error) => Some(
                Text::new(error.message(&message_env.localizer()))
                    .text_style(TextPreset::Small)
                    .foreground(Fill::Role(ColorRole::Destructive))
                    .accessibility_hidden(true),
            ),
            _ => None,
        });
        let presented = parts.presented;
        let cancel = Button::new(Localized::new("choscorui.cancel"), move || {
            presented.set(false)
        });
        let accept = Button::new(parts.accept.clone(), submit)
            .disabled(move || current() != ValidationState::Valid);
        // The dialog carries the title, and the field carries the prompt.
        VStack::new((
            Text::new(parts.title.clone())
                .text_style(TextPreset::Small)
                .font_weight(FontWeight::MEDIUM)
                .accessibility_hidden(true),
            Text::new(parts.prompt.clone())
                .text_style(TextPreset::Small)
                .foreground(ColorRole::MutedForeground)
                .accessibility_hidden(true),
            field,
            message,
            HStack::new((cancel.variant(ButtonVariant::Outline), accept)).spacing(Space::S2),
        ))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S2)
        .mount(cx);
    }
}

/// The `Select` of a Choice dialog over `options`. It writes the chosen
/// option to `value`, and clears a refused accept.
fn choice(
    name: Label,
    options: &[String],
    value: Binding<String>,
    refused: super::state::State<Option<ValidationState>>,
) -> impl View + use<> {
    let entries: Vec<ListEntry> = options.iter().map(ListEntry::option).collect();
    let (shown, chosen): (Rc<[String]>, Rc<[String]>) = (options.into(), options.into());
    let selected = Binding::new(
        move || {
            let typed = value.try_get().unwrap_or_default();
            shown.iter().position(|option| option == typed.trim())
        },
        move |index: Option<usize>| {
            if let Some(option) = index.and_then(|index| chosen.get(index)) {
                value.set(option.clone());
                refused.set(None);
            }
        },
    );
    Select::new(name, entries, selected)
}

/// A font dialog that edits `style`: a family combobox, a size field,
/// bold and italic toggles, and a preview of `sample` in the chosen
/// style. The labels name the family, size, bold, and italic controls.
/// `controls::FontDialog` applies each change. A family that is not in
/// `families` resolves to its fallback, and the size stays from 1 to 4096
/// points, with one decimal. Bold off restores the weight from before
/// bold. The other fields of `style` do not change. The controls wrap to
/// more lines when the width is too small.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_core::TextStyle;
/// struct Fonts;
///
/// impl View for Fonts {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let style = cx.state(TextStyle::new("Helvetica", 13.0));
///         let families = vec!["Helvetica".to_owned(), "Menlo".to_owned()];
///         let labels = ("Font", "Size", "Bold", "Italic");
///         FontDialog::new("The quick fox", labels, families, style.binding())
///     }
/// }
/// ```
pub struct FontDialog {
    sample: Label,
    labels: [Label; 4],
    families: Rc<[String]>,
    style: Binding<TextStyle>,
}

impl FontDialog {
    /// A font dialog with a preview of `sample` that edits `style`. The
    /// labels name the family, size, bold, and italic controls.
    pub fn new<M, N, O, P, Q>(
        sample: impl IntoLive<TextContent, M>,
        (family, size, bold, italic): (
            impl IntoLive<TextContent, N>,
            impl IntoLive<TextContent, O>,
            impl IntoLive<TextContent, P>,
            impl IntoLive<TextContent, Q>,
        ),
        families: Vec<String>,
        style: Binding<TextStyle>,
    ) -> Self {
        Self {
            sample: label(sample),
            labels: [label(family), label(size), label(bold), label(italic)],
            families: families.into(),
            style,
        }
    }
}

/// The font dialog model of `style`.
fn model(families: &[String], style: &TextStyle) -> controls::FontDialog {
    controls::FontDialog::new(families.to_vec(), style)
}

/// The value of `style`, or an empty style after its owner unmounts.
fn read(style: Binding<TextStyle>) -> TextStyle {
    style
        .try_get()
        .unwrap_or_else(|_| TextStyle::new(String::new(), 1.0))
}

/// Applies `change` to the model of `style`, and writes the family, size,
/// weight, and slant of the result back to `style`.
fn apply(
    families: &[String],
    style: Binding<TextStyle>,
    change: impl FnOnce(&mut controls::FontDialog),
) {
    style.update(|current| {
        let mut dialog = model(families, current);
        change(&mut dialog);
        let chosen = dialog.style();
        current.family = chosen.family;
        current.size = chosen.size;
        current.weight = chosen.weight;
        current.slant = chosen.slant;
    });
}

/// Types `family` into the family combobox of the model and commits it.
fn choose_family(dialog: &mut controls::FontDialog, family: String) {
    dialog.family(ComboInput::SetText(family));
    dialog.family(ComboInput::Key {
        key: Key::Enter,
        alt: false,
    });
}

/// A binding to one field of `style`, which the model changes.
fn field<T: Clone + PartialEq + 'static>(
    families: &Rc<[String]>,
    style: Binding<TextStyle>,
    get: impl Fn(&TextStyle) -> T + 'static,
    set: impl Fn(&mut controls::FontDialog, T) + 'static,
) -> Binding<T> {
    let families = families.clone();
    Binding::new(
        move || get(&read(style)),
        move |value| apply(&families, style, |dialog| set(dialog, value)),
    )
}

/// The weight that bold off goes back to: `weight` when it is not bold.
fn plain(weight: FontWeight) -> FontWeight {
    if weight >= FontWeight::SEMIBOLD {
        FontWeight::REGULAR
    } else {
        weight
    }
}

impl View for FontDialog {
    fn mount(self, cx: &mut Cx) {
        let Self {
            sample,
            labels: [family_label, size, bold, italic],
            families,
            style,
        } = self;
        let initial = Runtime::untracked(|| read(style));
        // The family that the user typed or chose. It can be missing.
        let requested = cx.state(initial.family);
        // The weight from before bold went on.
        let regular = cx.state(plain(initial.weight));
        let chooser = {
            let (build, shown, chosen) = (families.clone(), families.clone(), families.clone());
            Configured::custom(ControlKind::Combobox, move |env| {
                let current = Runtime::untracked(|| read(style));
                model(&build, &current)
                    .into_chooser()
                    .with_name(text(&family_label, env))
            })
            // Shows the family of `style`, but keeps a typed family that
            // is missing while `style` has its fallback.
            .value(move || {
                let family = read(style).family;
                let typed = model(&shown, &TextStyle::new(requested.get(), 1.0));
                let missing = typed.chooser().is_missing() && typed.style().family == family;
                let index = shown.iter().position(|item| *item == family);
                ControlValue::Index(index.filter(|_| !missing))
            })
            .on_message(move |message| {
                let family = match message {
                    ControlMessage::Changed(ControlValue::Text(typed)) => typed.clone(),
                    ControlMessage::Changed(ControlValue::Index(Some(index))) => {
                        match chosen.get(*index) {
                            Some(family) => family.clone(),
                            None => return,
                        }
                    }
                    _ => return,
                };
                requested.set(family.clone());
                apply(&chosen, style, |dialog| choose_family(dialog, family));
            })
        };
        let points = field(
            &families,
            style,
            |style| style.size,
            |dialog, points: f64| {
                dialog.set_size(points);
            },
        );
        let spin = Configured::try_custom(ControlKind::Input, move |env| {
            let name = text(&size, env);
            controls::DecimalSpinBox::new(name, env.locale(), (1.0, 4096.0), 1.0, 1, 1.0)
        })
        .bind(points, ControlValue::Number, as_number);
        let heavy = {
            let families = families.clone();
            Binding::new(
                move || read(style).weight >= FontWeight::SEMIBOLD,
                move |on: bool| {
                    let current = read(style).weight;
                    if on && current < FontWeight::SEMIBOLD {
                        regular.set(current);
                    }
                    let weight = if on {
                        FontWeight::BOLD
                    } else {
                        regular.try_get().unwrap_or(FontWeight::REGULAR)
                    };
                    apply(&families, style, |dialog| dialog.set_weight(weight));
                },
            )
        };
        let slanted = field(
            &families,
            style,
            |style| style.slant == FontSlant::Italic,
            |dialog, on: bool| {
                dialog.set_slant(if on {
                    FontSlant::Italic
                } else {
                    FontSlant::Upright
                });
            },
        );
        let preview = Configured::custom(ControlKind::Card, move |env| FontPreview {
            sample: text(&sample, env),
            style: read(style),
        });
        let controls = (
            chooser,
            spin,
            Toggle::new(bold, heavy),
            Toggle::new(italic, slanted),
        );
        VStack::new((WrapRow(controls, Space::S2), preview))
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S3)
            .mount(cx);
    }
}

/// The most lines of the sample that the preview shows.
const PREVIEW_LINES: f64 = 4.0;

/// The sample text of a font dialog in the chosen family, size, weight,
/// and slant, on the card surface. The sample wraps to the width. The
/// preview is at most four lines high, and at most a third of the window
/// high.
struct FontPreview {
    sample: String,
    style: TextStyle,
}

impl FontPreview {
    /// The card style, and the type of the sample in it.
    fn styles(&self, resolve: impl Fn(StyleQuery) -> ResolvedStyle) -> (ResolvedStyle, TypeSpec) {
        let card = resolve(StyleQuery::new(ControlKind::Card, Slot::Root));
        let size = self.style.size;
        let spec = TypeSpec {
            size,
            line_height: size * 1.2,
            weight: self.style.weight,
            slant: self.style.slant,
            tracking: self.style.tracking,
            ..card.text
        };
        (card, spec)
    }

    /// The font of the sample.
    fn run(&self) -> TextRun<'_> {
        TextRun {
            family: &self.style.family,
            size: self.style.size,
            weight: self.style.weight,
            slant: self.style.slant,
            tracking: self.style.tracking,
        }
    }

    /// The lines of the sample in `width`, each with its width.
    fn lines(&self, measure: &dyn TextMeasure, width: f64) -> Vec<(String, f64)> {
        let run = self.run();
        let width_of = |line: &str| measure.run_width(line, &run);
        wrap_lines(&self.sample, width, &width_of)
            .into_iter()
            .map(|line| {
                let width = width_of(&line);
                (line, width)
            })
            .collect()
    }
}

impl CustomControl for FontPreview {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let (card, spec) = self.styles(|query| cx.style(query));
        let inner = cx.constraints.max.width - card.padding.horizontal();
        let lines = self.lines(cx.text, inner);
        let width = lines.iter().map(|(_, width)| *width).fold(0.0, f64::max);
        let shown = (lines.len() as f64).min(PREVIEW_LINES) * spec.line_height;
        let height = shown.min(cx.viewport.size.height / 3.0) + card.padding.vertical();
        let size = Size::new(width + card.padding.horizontal(), height);
        Measured::new(cx.constraints.constrain(size))
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let (card, spec) = self.styles(|query| cx.style(query));
        let bounds = cx.bounds;
        paint_box(cx.list, bounds, &card, cx.rtl);
        let (left, right) = card.padding.left_right(cx.rtl);
        let run = self.run();
        let mut baseline = bounds.origin.y + card.padding.block_start + spec.baseline();
        cx.list.push_clip(bounds);
        for (line, width) in self.lines(cx.text, bounds.size.width - left - right) {
            if baseline - spec.size > bounds.max_y() {
                break;
            }
            let x = if cx.rtl {
                bounds.max_x() - right - width
            } else {
                bounds.origin.x + left
            };
            let origin = Point::new(x, baseline);
            let _ = cx.list.draw_text_run(origin, &line, &run, card.foreground);
            baseline += spec.line_height;
        }
        let _ = cx.list.pop_clip();
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::StaticText,
            name: self.sample.clone(),
            description: String::new(),
            value: Some(self.style.family.clone()),
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        }
    }

    fn focusable(&self) -> bool {
        false
    }
}
