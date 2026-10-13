//! View constructors for the dialog models that are not hosted controls:
//! [`InputDialog`] and [`FontDialog`]. The models in `choscorui-controls`
//! validate the value, handle the keys, and resolve a missing font; the
//! views mount their fields and buttons and connect them to state.

use super::View;
use super::builtins::{as_whole, to_number};
use super::configured::Configured;
use super::dynamic::Dynamic;
use super::env::Fill;
use super::host::Cx;
use super::layout::{HStack, HorizontalAlignment, VStack};
use super::modifier::ViewExt;
use super::state::{Binding, IntoLive};
use super::text::{Text, TextContent};
use super::widgets::{Button, Input, Label, Toggle, label, text};
use crate::reactive::Runtime;
use choscorui_controls as controls;
use choscorui_controls::{
    ComboInput, ControlContext, ControlEvent, ControlKind, ControlMessage, ControlValue,
    CustomControl, InputKind, InputValue, IntegerRange, MeasureContext, PaintContext,
    ResolvedStyle, Slot, StyleQuery, ValidationState, paint_box,
};
use choscorui_core::{
    ColorRole, EventReply, FontSlant, FontWeight, Key, Measured, Point, SemanticNode, SemanticRole,
    SemanticState, Size, Space, TextPreset, TextRun, TextStyle, TypeSpec,
};
use std::rc::Rc;

/// A dialog for one validated value, shown inline: the title, the prompt,
/// a field that edits `text`, an accept button, and the validation
/// message. The button is enabled only while the value is valid, and
/// Return in the field accepts only a valid value. Both run `on_accept`
/// with the value. `controls::InputDialog` validates the text for `kind`
/// in the locale of the environment.
pub struct InputDialog {
    title: Label,
    prompt: Label,
    accept: Label,
    kind: InputKind,
    text: Binding<String>,
    on_accept: Rc<dyn Fn(InputValue)>,
}

impl InputDialog {
    pub fn new<M, N, O>(
        title: impl IntoLive<TextContent, M>,
        prompt: impl IntoLive<TextContent, N>,
        accept: impl IntoLive<TextContent, O>,
        kind: InputKind,
        text: Binding<String>,
        on_accept: impl Fn(InputValue) + 'static,
    ) -> Self {
        Self {
            title: label(title),
            prompt: label(prompt),
            accept: label(accept),
            kind,
            text,
            on_accept: Rc::new(on_accept),
        }
    }
}

impl View for InputDialog {
    fn mount(self, cx: &mut Cx) {
        let Self {
            title,
            prompt,
            accept,
            kind,
            text: value,
            on_accept,
        } = self;
        let env = cx.env().clone();
        let label_env = env.clone();
        let model = {
            let (title, prompt) = (title.clone(), prompt.clone());
            Rc::new(move || {
                controls::InputDialog::new(
                    text(&title, &env),
                    text(&prompt, &env),
                    kind.clone(),
                    env.locale(),
                    value.try_get().unwrap_or_default(),
                )
            })
        };
        let validation = {
            let model = model.clone();
            cx.memo(move || model().validate().0)
        };
        // A memo reads as incomplete after the view unmounts.
        let current = move || {
            validation
                .try_get()
                .unwrap_or(ValidationState::Intermediate)
        };
        let submit = move || {
            if let Some(Ok(value)) = model().handle_key(Key::Enter) {
                on_accept(value);
            }
        };
        let field = Input::new(prompt.clone(), value)
            .with(move |mut input| {
                input.set_validation(current());
                input
            })
            .on_activate(submit.clone());
        let button =
            Button::new(accept, submit).disabled(move || current() != ValidationState::Valid);
        let message = Dynamic::new(move || match current() {
            ValidationState::Invalid(message) => Some(
                Text::new(message)
                    .text_style(TextPreset::Small)
                    .foreground(Fill::Role(ColorRole::Destructive)),
            ),
            _ => None,
        });
        // The group carries the title, and the field carries the prompt.
        VStack::new((
            Text::new(title.clone())
                .text_style(TextPreset::Small)
                .font_weight(FontWeight::MEDIUM)
                .accessibility_hidden(true),
            Text::new(prompt)
                .text_style(TextPreset::Small)
                .foreground(ColorRole::MutedForeground)
                .accessibility_hidden(true),
            HStack::new((field, button)).spacing(Space::S2),
            message,
        ))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S1)
        .accessibility_label(move || text(&title, &label_env))
        .mount(cx);
    }
}

/// A font dialog that edits `style`: a family combobox, a size field,
/// bold and italic toggles, and a preview of `sample` in the chosen
/// style. `controls::FontDialog` applies each change. A family that is
/// not in `families` resolves to its fallback, and the size stays from 1
/// to 4096 points. The other fields of `style` do not change.
pub struct FontDialog {
    sample: Label,
    size: Label,
    bold: Label,
    italic: Label,
    families: Rc<[String]>,
    style: Binding<TextStyle>,
}

impl FontDialog {
    pub fn new<M, N, O, P>(
        sample: impl IntoLive<TextContent, M>,
        size: impl IntoLive<TextContent, N>,
        bold: impl IntoLive<TextContent, O>,
        italic: impl IntoLive<TextContent, P>,
        families: Vec<String>,
        style: Binding<TextStyle>,
    ) -> Self {
        Self {
            sample: label(sample),
            size: label(size),
            bold: label(bold),
            italic: label(italic),
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

impl View for FontDialog {
    fn mount(self, cx: &mut Cx) {
        let Self {
            sample,
            size,
            bold,
            italic,
            families,
            style,
        } = self;
        // The family that the user typed or chose. It can be missing.
        let requested = cx.state(Runtime::untracked(|| read(style)).family);
        let chooser = {
            let (build, shown, chosen) = (families.clone(), families.clone(), families.clone());
            Configured::custom(ControlKind::Combobox, move |_| {
                let current = Runtime::untracked(|| read(style));
                model(&build, &current).into_chooser()
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
            |style| as_whole(&ControlValue::Number(style.size)).unwrap_or(1),
            |dialog, points: i32| {
                dialog.set_size(f64::from(points));
            },
        );
        let spin = Configured::try_custom(ControlKind::Input, move |env| {
            IntegerRange::new(1, 4096, 1, 1, 10)
                .map(|range| controls::SpinBox::new(text(&size, env), range))
        })
        .bind(points, to_number, as_whole);
        let heavy = field(
            &families,
            style,
            |style| style.weight >= FontWeight::SEMIBOLD,
            |dialog, on: bool| {
                dialog.set_weight(if on {
                    FontWeight::BOLD
                } else {
                    FontWeight::REGULAR
                });
            },
        );
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
        VStack::new((
            HStack::new((
                chooser,
                spin,
                Toggle::new(bold, heavy),
                Toggle::new(italic, slanted),
            ))
            .spacing(Space::S2),
            preview,
        ))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S3)
        .mount(cx);
    }
}

/// The sample text of a font dialog in the chosen family, size, and
/// weight, on the card surface.
struct FontPreview {
    sample: String,
    style: TextStyle,
}

impl FontPreview {
    /// The card style, and the type of the sample in it. The theme font
    /// stands in for the chosen family when the sample is measured.
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
}

impl CustomControl for FontPreview {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let (card, spec) = self.styles(|query| cx.style(query));
        let width = cx.text.width(&self.sample, &spec) + card.padding.horizontal();
        let height = spec.line_height + card.padding.vertical();
        Measured::new(cx.constraints.constrain(Size::new(width, height)))
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let (card, spec) = self.styles(|query| cx.style(query));
        let bounds = cx.bounds;
        paint_box(cx.list, bounds, &card, cx.rtl);
        let (left, right) = card.padding.left_right(cx.rtl);
        let width = cx.text.width(&self.sample, &spec);
        let x = if cx.rtl {
            bounds.max_x() - right - width
        } else {
            bounds.origin.x + left
        };
        let baseline = bounds.origin.y + card.padding.block_start + spec.baseline();
        let run = TextRun {
            family: &self.style.family,
            size: spec.size,
            weight: spec.weight,
            tracking: spec.tracking,
        };
        cx.list.push_clip(bounds);
        let _ = cx
            .list
            .draw_text_run(Point::new(x, baseline), &self.sample, &run, card.foreground);
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
