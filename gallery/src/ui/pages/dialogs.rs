//! Dialogs: a modal alert with ordered buttons beside a modeless note,
//! input dialogs with validation, the color picker, and the font dialog.

use crate::ui::page::{Build, PageCx, column, row};
use choscorui::compose::view::{
    AnyView, Binding, Button, Card, Configured, Corner, Cx, Dynamic, Empty, Fill, FontDialog,
    Frame, HStack, HorizontalAlignment, Input, InputDialog, IntoLive, Text, TextContent, VStack,
    View, ViewExt,
};
use choscorui::controls::{
    AlertDialog, ButtonVariant, ColorChannel, ColorPicker, ControlKind, ControlMessage,
    ControlValue, DialogButton, DialogButtonRole, InputKind, ValidationState, dialog_key_button,
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

fn save_alert() -> AlertDialog {
    let mut dialog = AlertDialog::new(
        "Unsaved changes",
        "Do you want to save the changes you made to “Notes.txt”?",
        vec![
            DialogButton::new("Save", DialogButtonRole::Accept),
            DialogButton::new("Don't Save", DialogButtonRole::Discard),
            DialogButton::new("Cancel", DialogButtonRole::Reject),
            DialogButton::new("?", DialogButtonRole::Help),
        ],
    );
    dialog.detail = Some("Your changes will be lost if you don't save them.".into());
    dialog
}

/// The readout for choosing button `index` of `dialog`.
fn chose(dialog: &AlertDialog, index: usize) -> String {
    format!("Chose “{}”", dialog.buttons[index].title)
}

/// Opens a modal "save changes" alert or a modeless note. While the modal
/// alert is open, input outside it is blocked; Return chooses Save and
/// Escape chooses Cancel.
struct AlertDialogPage;

impl View for AlertDialogPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let last = cx.state("No dialog open.".to_owned());
        let modal = cx.state(false);
        let tip = cx.state(false);
        let alert = Configured::custom(ControlKind::AlertDialog, |_| save_alert().hosted())
            .open(modal.binding())
            .on_item(move |index| last.set(chose(&save_alert(), index)))
            // Dismissing chooses the button that Escape chooses.
            .on_dismiss(move || {
                let dialog = save_alert();
                if let Some(index) = dialog_key_button(&dialog.buttons, Key::Escape) {
                    last.set(chose(&dialog, index));
                }
            });
        let note = Dynamic::new(move || {
            tip.get().then(|| {
                Card::new(
                    "Tip",
                    "This note is modeless: the rest of the page keeps working.",
                )
                .with(|card| card.with_footer("OK", ButtonVariant::Default))
                .on_item(move |_| {
                    last.set("Chose “OK”".to_owned());
                    tip.set(false);
                })
                .frame(Frame::new().max_width(512.0))
            })
        });
        column((
            row((
                Button::new("Close document…", move || {
                    tip.set(false);
                    modal.set(true);
                }),
                Button::new("Show tip", move || tip.set(true)),
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

/// The title, prompt, kind, and starting text of one input dialog.
type Spec = (&'static str, &'static str, InputKind, String);

/// Four input dialogs shown inline: text, integer, decimal, and choice.
/// Each OK button is enabled only while its value is valid.
struct InputDialogPage;

impl InputDialogPage {
    fn specs(cx: &Cx<'_>) -> [Spec; 4] {
        let locale = cx.env().locale();
        [
            (
                "Rename",
                "New name",
                InputKind::Text { allow_empty: false },
                "Notes.txt".into(),
            ),
            (
                "Go to line",
                "Line (1 – 500)",
                InputKind::Integer {
                    minimum: 1,
                    maximum: 500,
                },
                "42".into(),
            ),
            (
                "Price",
                "Price (0 – 1,000,000; 2 decimals)",
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
                "Size",
                "Size (Small, Medium, or Large)",
                InputKind::Choice(vec!["Small".into(), "Medium".into(), "Large".into()]),
                "Medium".into(),
            ),
        ]
    }
}

impl View for InputDialogPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let last = cx.state("Type a value and press Return.".to_owned());
        let mut dialogs = Vec::new();
        for (title, prompt, kind, initial) in Self::specs(cx) {
            let text = cx.state(initial);
            dialogs.push(InputDialog::new(
                title,
                prompt,
                "OK",
                kind,
                text.binding(),
                move |value| last.set(format!("{title}: accepted {value:?}")),
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

const CHANNELS: [&str; 4] = ["Hue", "Saturation", "Brightness", "Opacity"];

/// The picker's starting color: teal.
const TEAL: Color = Color::rgba8(0, 128, 128, 255);

/// The readout of channel `index`, such as "Hue 180°".
fn channel_text(picker: &ColorPicker, index: usize) -> String {
    let fraction = picker.channel(ColorChannel::ALL[index]);
    if index == 0 {
        format!("{} {:.0}°", CHANNELS[index], fraction * 360.0)
    } else {
        format!("{} {:.0}%", CHANNELS[index], fraction * 100.0)
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
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
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
        let picker = Configured::custom(ControlKind::ColorPicker, |_| {
            ColorPicker::new(TEAL).with_name("Current color")
        })
        .value(move || ControlValue::Text(model.with(ColorPicker::hex).unwrap_or_default()))
        .on_message(move |message| match message {
            ControlMessage::Changed(ControlValue::Text(hex)) => {
                model.update(|picker| {
                    picker.set_hex(hex);
                });
                hex_text.set(hex.clone());
                hex_state.set(ValidationState::Valid);
            }
            ControlMessage::Activated => model.update(|picker| {
                picker.accept();
            }),
            _ => {}
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
        let hex = Input::new("Hex color", typed)
            .with(move |mut input| {
                input.set_validation(checked());
                input.with_placeholder("#RRGGBB")
            })
            .frame(Frame::new().width(140.0));
        let channels: Vec<_> = (0..CHANNELS.len())
            .map(|index| {
                readout(move || {
                    model
                        .with(|picker| channel_text(picker, index))
                        .unwrap_or_default()
                })
            })
            .collect();
        column((
            picker,
            HStack::new(channels).spacing(Space::S4),
            row((
                hex,
                Button::new("Use Color", move || {
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

const PREVIEW: &str = "Sphinx of black quartz — Chữ Việt — 日本語";

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
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let style = cx.state(TextStyle::new("Georgia", 22.0));
        let families = FAMILIES.iter().map(|family| (*family).to_owned()).collect();
        column((
            FontDialog::new(PREVIEW, "Size", "Bold", "Italic", families, style.binding()),
            readout(move || {
                style
                    .with(|style| format!("{} {} pt", style.family, style.size))
                    .unwrap_or_default()
            }),
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
        assert_eq!(page.value(super::PREVIEW).as_deref(), Some("Georgia"));
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
