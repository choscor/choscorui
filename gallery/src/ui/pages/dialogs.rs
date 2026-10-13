//! Dialogs: a modal alert with ordered buttons beside a modeless note,
//! input dialogs with validation, the color picker, and the font dialog.

use crate::ui::page::{Build, PageCx, column, row};
use choscorui::compose::t;
use choscorui::compose::view::{
    self, AlertDialog, AnyView, Binding, Button, Card, Corner, Cx, Dynamic, Empty, Env, Fill,
    FontDialog, Frame, HStack, HorizontalAlignment, Input, InputDialog, IntoLive, Text,
    TextContent, VStack, View, ViewExt,
};
use choscorui::controls::{
    ButtonVariant, ColorChannel, ColorPicker, DialogButton, DialogButtonRole, InputKind,
    ValidationState, dialog_key_button,
};
use choscorui::core::{Color, ColorRole, Key, RadiusScale, Space, TextPreset, TextStyle};

/// A small caption in `muted-foreground`, for headings and readouts.
fn readout<M>(text: impl IntoLive<TextContent, M>) -> impl View {
    Text::new(text)
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
}

// ---------------------------------------------------------------------------
// Alert dialog

/// The catalog string `key` in `env`.
fn text(env: &Env, key: &str) -> String {
    env.text(key, &[])
}

/// The buttons of the "save changes" alert, in order.
fn save_buttons(env: &Env) -> Vec<DialogButton> {
    let text = |key: &str| text(env, key);
    vec![
        DialogButton::new(text("alert-dialog.save"), DialogButtonRole::Accept),
        DialogButton::new(text("alert-dialog.dont_save"), DialogButtonRole::Discard),
        DialogButton::new(text("alert-dialog.cancel"), DialogButtonRole::Reject),
        DialogButton::new("?", DialogButtonRole::Help),
    ]
}

/// The readout for choosing button `index` of `buttons`.
fn chose(env: &Env, buttons: &[DialogButton], index: usize) -> String {
    let button = &buttons[index].title;
    env.text("alert-dialog.chose", &[("button", button)])
}

/// Opens a modal "save changes" alert or a modeless note. While the modal
/// alert is open, input outside it is blocked; Return chooses Save and
/// Escape chooses Cancel.
struct AlertDialogPage;

impl View for AlertDialogPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let last = cx.state(text(cx.env(), "alert-dialog.none"));
        let modal = cx.state(false);
        let tip = cx.state(false);
        let (item_env, dismiss_env, tip_env) =
            (cx.env().clone(), cx.env().clone(), cx.env().clone());
        let alert = AlertDialog::new(
            t!("alert-dialog.title"),
            t!("alert-dialog.message", file = "Notes.txt"),
            save_buttons,
            modal.binding(),
        )
        .detail(t!("alert-dialog.detail"))
        .on_item(move |index| last.set(chose(&item_env, &save_buttons(&item_env), index)))
        // Dismissing chooses the button that Escape chooses.
        .on_dismiss(move || {
            let buttons = save_buttons(&dismiss_env);
            if let Some(index) = dialog_key_button(&buttons, Key::Escape) {
                last.set(chose(&dismiss_env, &buttons, index));
            }
        });
        let note = Dynamic::new(move || {
            tip.get().then(|| {
                let ok = text(&tip_env, "alert-dialog.ok");
                let chose_ok = tip_env.text("alert-dialog.chose", &[("button", &ok)]);
                Card::new(t!("alert-dialog.tip"), t!("alert-dialog.tip_text"))
                    .with(move |card| card.with_footer(ok.clone(), ButtonVariant::Default))
                    .on_item(move |_| {
                        last.set(chose_ok.clone());
                        tip.set(false);
                    })
                    .frame(Frame::new().max_width(512.0))
            })
        });
        column((
            row((
                Button::new(t!("alert-dialog.close_document"), move || {
                    tip.set(false);
                    modal.set(true);
                }),
                Button::new(t!("alert-dialog.show_tip"), move || tip.set(true)),
            )),
            readout(move || last.get()),
            note,
            alert,
        ))
    }
}

pub fn alert_dialog(_: &PageCx) -> AnyView {
    AlertDialogPage.into_any()
}

// ---------------------------------------------------------------------------
// Input dialog

/// The title and prompt keys, kind, and starting text of one input
/// dialog.
type Spec = (&'static str, &'static str, InputKind, String);

/// Four input dialogs shown inline: text, integer, decimal, and choice.
/// Each OK button is enabled only while its value is valid.
struct InputDialogPage;

impl InputDialogPage {
    fn specs(cx: &Cx) -> [Spec; 4] {
        let env = cx.env();
        let locale = env.locale();
        let size = |key: &str| text(env, key);
        [
            (
                "input-dialog.rename",
                "input-dialog.new_name",
                InputKind::Text { allow_empty: false },
                "Notes.txt".into(),
            ),
            (
                "input-dialog.go_to_line",
                "input-dialog.line",
                InputKind::Integer {
                    minimum: 1,
                    maximum: 500,
                },
                "42".into(),
            ),
            (
                "input-dialog.price",
                "input-dialog.price_prompt",
                InputKind::Decimal {
                    minimum: 0.0,
                    maximum: 1e6,
                    fraction_digits: 2,
                },
                locale
                    .format_decimal(9.5, 2)
                    .unwrap_or_else(|_| "9.50".into()),
            ),
            (
                "input-dialog.size",
                "input-dialog.size_prompt",
                InputKind::Choice(vec![
                    size("input-dialog.small"),
                    size("input-dialog.medium"),
                    size("input-dialog.large"),
                ]),
                size("input-dialog.medium"),
            ),
        ]
    }
}

impl View for InputDialogPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let last = cx.state(text(cx.env(), "input-dialog.start"));
        let mut dialogs = Vec::new();
        for (title, prompt, kind, initial) in Self::specs(cx) {
            let typed = cx.state(initial);
            let env = cx.env().clone();
            dialogs.push(InputDialog::new(
                t!(title),
                t!(prompt),
                t!("input-dialog.ok"),
                kind,
                typed.binding(),
                move |value| {
                    let title = text(&env, title);
                    let value = format!("{value:?}");
                    let args = [("title", title.as_str()), ("value", value.as_str())];
                    last.set(env.text("input-dialog.accepted", &args));
                },
            ));
        }
        column((
            VStack::new(dialogs)
                .alignment(HorizontalAlignment::Leading)
                .spacing(Space::S4),
            readout(move || last.get()),
        ))
    }
}

pub fn input_dialog(_: &PageCx) -> AnyView {
    InputDialogPage.into_any()
}

// ---------------------------------------------------------------------------
// Color picker

const CHANNELS: [&str; 4] = [
    "color-picker.hue",
    "color-picker.saturation",
    "color-picker.brightness",
    "color-picker.opacity",
];

/// The picker's starting color: teal.
const TEAL: Color = Color::rgba8(0, 128, 128, 255);

/// The readout of channel `index` named `name`, such as "Hue 180°".
fn channel_text(picker: &ColorPicker, index: usize, name: &str) -> String {
    let fraction = picker.channel(ColorChannel::ALL[index]);
    if index == 0 {
        format!("{name} {:.0}°", fraction * 360.0)
    } else {
        format!("{name} {:.0}%", fraction * 100.0)
    }
}

/// A recent color.
fn swatch(color: Color) -> impl View {
    Empty
        .frame(Frame::size(20.0, 20.0))
        .background(Fill::Color(color))
        .border(ColorRole::Input, 1.0)
        .corner_radius(Corner::Scale(RadiusScale::Sm))
}

/// The hosted picker with its channel readouts, a hex field, and the
/// recent colors chosen with Use Color.
struct ColorPickerPage;

impl View for ColorPickerPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let model = cx.state(ColorPicker::new(TEAL));
        let hex_text = cx.state(ColorPicker::new(TEAL).hex());
        let hex_state = cx.state(ValidationState::Valid);
        // The state reads as valid after the page unmounts.
        let checked = move || hex_state.try_get().unwrap_or(ValidationState::Valid);
        let recent = cx.memo(move || {
            model
                .with(|picker| picker.recent().to_vec())
                .unwrap_or_default()
        });
        let chosen = Binding::new(
            move || model.with(ColorPicker::hex).unwrap_or_default(),
            move |hex: String| {
                model.update(|picker| {
                    picker.set_hex(&hex);
                });
                hex_text.set(hex);
                hex_state.set(ValidationState::Valid);
            },
        );
        let picker = view::ColorPicker::new(t!("color-picker.current"), chosen)
            .on_activate(move || {
                model.update(|picker| {
                    picker.accept();
                })
            })
            .frame(Frame::new().width(300.0));
        let typed = Binding::new(
            move || hex_text.get(),
            move |text: String| {
                let mut state = ValidationState::Valid;
                model.update(|picker| state = picker.set_hex(&text));
                hex_text.set(text);
                hex_state.set(state);
            },
        );
        let hex = Input::new(t!("color-picker.hex"), typed)
            .with(move |mut input| {
                input.set_validation(checked());
                input.with_placeholder("#RRGGBB")
            })
            .frame(Frame::new().width(140.0));
        let channels: Vec<_> = CHANNELS
            .iter()
            .enumerate()
            .map(|(index, key)| {
                let name = text(cx.env(), key);
                readout(move || {
                    model
                        .with(|picker| channel_text(picker, index, &name))
                        .unwrap_or_default()
                })
            })
            .collect();
        column((
            picker,
            HStack::new(channels).spacing(Space::S4),
            row((
                hex,
                Button::new(t!("color-picker.use_color"), move || {
                    model.update(|picker| {
                        picker.accept();
                    })
                }),
            )),
            Dynamic::new(move || {
                HStack::new(recent.get().into_iter().map(swatch).collect::<Vec<_>>())
                    .spacing(Space::S1)
            }),
            Dynamic::new(move || match checked() {
                ValidationState::Invalid(message) => Some(
                    Text::new(message)
                        .text_style(TextPreset::Small)
                        .foreground(ColorRole::Destructive),
                ),
                _ => None,
            }),
        ))
    }
}

pub fn color_picker(_: &PageCx) -> AnyView {
    ColorPickerPage.into_any()
}

// ---------------------------------------------------------------------------
// Font dialog

const FAMILIES: [&str; 8] = [
    "Helvetica",
    "Times New Roman",
    "Courier New",
    "Georgia",
    "Menlo",
    "Avenir Next",
    "Gill Sans",
    "Futura",
];

/// The font dialog over the sample families, with a readout of the
/// chosen family and size.
struct FontDialogPage;

impl View for FontDialogPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let style = cx.state(TextStyle::new("Georgia", 22.0));
        let families = FAMILIES.iter().map(|family| (*family).to_owned()).collect();
        column((
            FontDialog::new(
                t!("font-dialog.preview"),
                t!("font-dialog.size"),
                t!("font-dialog.bold"),
                t!("font-dialog.italic"),
                families,
                style.binding(),
            ),
            readout(t!(
                "font-dialog.readout",
                family = move || style.with(|style| style.family.clone()).unwrap_or_default(),
                size = move || style.with(|style| style.size).unwrap_or_default(),
            )),
        ))
    }
}

pub fn font_dialog(_: &PageCx) -> AnyView {
    FontDialogPage.into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "alert-dialog" => alert_dialog,
        "input-dialog" => input_dialog,
        "color-picker" => color_picker,
        "font-dialog" => font_dialog,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, NodeId, SemanticAction};

    fn press(page: &mut Harness, key: Key) -> bool {
        let handled = page.host.key(
            KeyEvent::Down { key, repeat: false },
            ShortcutModifiers::default(),
        );
        page.host.layout();
        handled
    }

    fn hidden(page: &mut Harness, name: &str) -> bool {
        let node = page.node(name);
        page.host.semantic(node).unwrap().state.hidden
    }

    #[test]
    fn alert_dialog_opens_modally_and_escape_cancels_it() {
        let mut page = Harness::new("alert-dialog");
        assert!(hidden(&mut page, "Unsaved changes"));
        assert!(page.act("Close document…", SemanticAction::Activate));
        assert!(!hidden(&mut page, "Unsaved changes"));
        assert!(press(&mut page, Key::Escape));
        assert!(hidden(&mut page, "Unsaved changes"));
        assert!(page.host.find("Chose “Cancel”").is_some());
    }

    #[test]
    fn alert_dialog_return_saves() {
        let mut page = Harness::new("alert-dialog");
        page.act("Close document…", SemanticAction::Activate);
        assert!(press(&mut page, Key::Enter));
        assert!(page.host.find("Chose “Save”").is_some());
    }

    #[test]
    fn the_tip_is_modeless() {
        let mut page = Harness::new("alert-dialog");
        assert!(page.act("Show tip", SemanticAction::Activate));
        assert!(page.host.find("Tip").is_some());
        // The rest of the page keeps working while the tip shows.
        assert!(page.act("Close document…", SemanticAction::Activate));
        assert!(page.host.find("Tip").is_none());
    }

    /// The OK button of the input dialog titled `title`.
    fn ok_button(page: &mut Harness, title: &str) -> NodeId {
        let mut pending = vec![page.node(title)];
        while let Some(node) = pending.pop() {
            if page
                .host
                .semantic(node)
                .is_some_and(|semantic| semantic.name == "OK")
            {
                return node;
            }
            pending.extend_from_slice(page.host.tree().children(node).unwrap_or_default());
        }
        panic!("the {title:?} dialog has no OK button");
    }

    fn ok_disabled(page: &mut Harness, title: &str) -> bool {
        let node = ok_button(page, title);
        page.host.semantic(node).unwrap().state.disabled
    }

    #[test]
    fn input_dialogs_validate_their_kind() {
        let mut page = Harness::new("input-dialog");
        assert!(!ok_disabled(&mut page, "Rename"));
        page.act("New name", SemanticAction::SetValue);
        assert!(ok_disabled(&mut page, "Rename"), "a name is required");
        let mut enter = |text: &str| {
            let node = page.node("Line (1 – 500)");
            page.host
                .perform(node, SemanticAction::SetValue, Some(text));
            page.host.layout();
            !ok_disabled(&mut page, "Go to line")
        };
        assert!(enter("42"));
        assert!(!enter("501"));
        assert!(!enter("forty"));
        assert!(enter("7"));
        let ok = ok_button(&mut page, "Go to line");
        assert!(page.host.perform(ok, SemanticAction::Activate, None));
        page.host.layout();
        assert!(page.host.find("Go to line: accepted Integer(7)").is_some());
    }

    #[test]
    fn color_picker_follows_the_hex_field() {
        let mut page = Harness::new("color-picker");
        assert_eq!(page.value("Current color").as_deref(), Some("#008080"));
        assert!(page.host.find("Hue 180°").is_some());
        let hex = page.node("Hex color");
        page.host
            .perform(hex, SemanticAction::SetValue, Some("#FF0000"));
        page.host.layout();
        assert_eq!(page.value("Current color").as_deref(), Some("#FF0000"));
        assert!(page.host.find("Hue 0°").is_some());
        page.host.perform(hex, SemanticAction::SetValue, Some("zz"));
        page.host.layout();
        assert!(page.host.semantic(hex).unwrap().state.invalid);
        assert_eq!(page.value("Current color").as_deref(), Some("#FF0000"));
    }

    #[test]
    fn font_dialog_previews_the_chosen_style() {
        let mut page = Harness::new("font-dialog");
        assert!(page.host.find("Georgia 22 pt").is_some());
        assert_eq!(
            page.value("Sphinx of black quartz — Chữ Việt — 日本語")
                .as_deref(),
            Some("Georgia")
        );
        assert!(page.act("Size", SemanticAction::Increment));
        assert!(page.host.find("Georgia 23 pt").is_some());
        assert!(page.act("Bold", SemanticAction::Activate));
        let bold = page.node("Bold");
        assert_eq!(page.host.semantic(bold).unwrap().state.checked, Some(true));
        let font = page.node("Font");
        page.host
            .perform(font, SemanticAction::SetValue, Some("Comic Neue"));
        page.host.layout();
        assert!(page.host.find("Helvetica 23 pt").is_some(), "the fallback");
        assert!(page.host.semantic(font).unwrap().state.invalid);
    }
}
