//! Buttons and other momentary or toggled inputs.

use crate::ui::page::{Build, PageCx, caption, column};
use choscorui::compose::view::{
    AnyView, Button, Checkbox, Configured, Cx, Frame, HStack, HorizontalAlignment, State, Text,
    VStack, View, ViewExt,
};
use choscorui::controls::{
    self, ButtonGroup, ButtonVariant, CommandId, ControlKind, ControlMessage, ControlValue,
    KeySequenceEdit, Shortcut, ShortcutModifiers, ToolButton, ToolButtonMenu, shortcut_text,
};
use choscorui::core::{ColorRole, Icon, Key, Space, TextPreset};

/// A small muted line whose text follows state.
fn readout(text: impl Fn() -> String + 'static) -> impl View {
    Text::new(text)
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
}

/// Buttons in several variants, one that a checkbox enables, and a
/// window shortcut.
struct ButtonPage;

impl View for ButtonPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let activations = cx.state(0_usize);
        let archive = cx.state(false);
        let count = move || activations.update(|count| *count += 1);
        let width = || Frame::new().width(260.0);
        let command = ShortcutModifiers {
            command: true,
            ..ShortcutModifiers::default()
        };
        column((
            Button::new("Say hello (⌘↩)", count).frame(width()),
            Button::new("Outline hello", count)
                .variant(ButtonVariant::Outline)
                .icon(Icon::Send)
                .frame(width()),
            Button::new("Archive (command disabled)", count)
                .variant(ButtonVariant::Destructive)
                .disabled(move || !archive.get())
                .frame(width()),
            Checkbox::new("Enable the Archive command", archive.binding())
                .frame(Frame::new().width(320.0)),
            readout(move || format!("Activations: {}", activations.get())),
        ))
        .keyboard_shortcut(Key::Enter, command, count)
    }
}

pub fn button(_: &PageCx) -> AnyView {
    ButtonPage.into_any()
}

/// Checked, unchecked, mixed, invalid, and permanently disabled
/// checkboxes.
struct CheckboxPage;

impl View for CheckboxPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        // `None` is the mixed state.
        let mut row = |label: &'static str, checked: Option<bool>, invalid: bool, enabled: bool| {
            let checked: State<Option<bool>> = cx.state(checked);
            Configured::custom(ControlKind::Checkbox, move |_| {
                controls::Checkbox::new(label, false).with_invalid(invalid)
            })
            .value(move || ControlValue::Mixed(checked.get()))
            .on_message(move |message| {
                if let ControlMessage::Changed(ControlValue::Bool(value)) = message {
                    checked.set(Some(*value));
                }
            })
            .disabled(!enabled)
            .frame(Frame::new().width(320.0))
        };
        column((
            row("Show line numbers", Some(true), false, true),
            row("Wrap long lines", Some(false), false, true),
            row("Select all files", None, false, true),
            row("Accept the terms", Some(false), true, true),
            row("Sync settings (unavailable)", Some(true), false, false),
        ))
    }
}

pub fn checkbox(_: &PageCx) -> AnyView {
    CheckboxPage.into_any()
}

/// Plain, checkable, instant-menu, and split-menu tool buttons.
struct ToolButtonPage;

impl View for ToolButtonPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let last = cx.state(String::from("None yet"));
        let tool = |index: u64, label: &'static str, checkable: bool, menu, width: f64| {
            Configured::custom(ControlKind::Toggle, move |_| {
                let mut button = ToolButton::new(label, CommandId(index));
                button.checkable = checkable;
                button.menu = menu;
                button.hosted()
            })
            .on_message(move |message| match message {
                ControlMessage::Command(_) => last.set(format!("{label} activated")),
                ControlMessage::Changed(ControlValue::Bool(on)) => {
                    last.set(format!("{label} {}", if *on { "on" } else { "off" }));
                }
                ControlMessage::OpenChanged(true) => last.set(format!("{label} menu opened")),
                _ => {}
            })
            .frame(Frame::new().width(width))
        };
        column((
            HStack::new((
                tool(1, "Cut", false, ToolButtonMenu::None, 72.0),
                tool(2, "Bold", true, ToolButtonMenu::None, 72.0),
                tool(3, "Recent", false, ToolButtonMenu::Instant, 96.0),
                tool(4, "Undo", false, ToolButtonMenu::Split, 96.0),
            ))
            .spacing(Space::S2),
            readout(move || format!("Last action: {}", last.get())),
        ))
    }
}

pub fn tool_button(_: &PageCx) -> AnyView {
    ToolButtonPage.into_any()
}

/// A joined group of `labels` with the members in `checked` on; members
/// in `disabled` take no input.
fn group(
    labels: &'static [&'static str],
    exclusive: bool,
    disabled: Option<usize>,
    checked: State<Vec<usize>>,
) -> impl View {
    Configured::custom(ControlKind::ToggleGroup, move |_| {
        let members = (1..=labels.len() as u64).map(CommandId).collect();
        let names = labels.iter().map(|label| label.to_string()).collect();
        let group = ButtonGroup::new(members, exclusive).hosted(names);
        match disabled {
            Some(index) => group.with_enabled(index, false),
            None => group,
        }
    })
    .value(move || ControlValue::Indices(checked.get()))
    .on_message(move |message| {
        if let ControlMessage::Changed(ControlValue::Indices(indices)) = message {
            checked.set(indices.clone());
        }
    })
    // Each member is 92 pt wide.
    .frame(Frame::new().width(92.0 * labels.len() as f64))
}

/// A group under its caption; assistive technology reads the caption as
/// `name`.
fn titled(title: &'static str, name: &'static str, content: impl View) -> impl View {
    VStack::new((caption(title).accessibility_label(name.to_owned()), content))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S2)
}

/// An exclusive alignment group and a non-exclusive style group.
struct ButtonGroupPage;

impl View for ButtonGroupPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let alignment = cx.state(vec![0]);
        let style = cx.state(vec![0]);
        column((
            titled(
                "Exclusive (arrow keys move the choice; Justify is disabled)",
                "Alignment",
                group(
                    &["Left", "Center", "Right", "Justify"],
                    true,
                    Some(3),
                    alignment,
                ),
            ),
            titled(
                "Non-exclusive (each toggles)",
                "Style",
                group(&["Bold", "Italic", "Underline"], false, None, style),
            ),
        ))
    }
}

pub fn button_group(_: &PageCx) -> AnyView {
    ButtonGroupPage.into_any()
}

/// `shortcut_text` for a few shortcuts.
fn examples() -> String {
    let all = ShortcutModifiers {
        shift: true,
        control: true,
        option: true,
        command: true,
    };
    let command = ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    };
    [
        Shortcut {
            key: Key::Character('s'),
            modifiers: command,
        },
        Shortcut {
            key: Key::ArrowLeft,
            modifiers: all,
        },
        Shortcut {
            key: Key::Enter,
            modifiers: ShortcutModifiers::default(),
        },
    ]
    .map(shortcut_text)
    .join("   ")
}

/// Records a shortcut of up to two chords.
struct KeySequencePage;

impl View for KeySequencePage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let last = cx.state(String::from("None"));
        column((
            Configured::custom(ControlKind::Input, |_| {
                KeySequenceEdit::new(2)
                    .with_name("Shortcut")
                    .with_placeholder("Click or press Return to record")
            })
            .on_message(move |message| {
                if let ControlMessage::Changed(ControlValue::Text(text)) = message {
                    last.set(
                        if text.is_empty() {
                            "Cleared"
                        } else {
                            "Committed"
                        }
                        .into(),
                    );
                }
            })
            .frame(Frame::new().width(320.0)),
            readout(move || format!("Last change: {}", last.get())),
            caption(format!("shortcut_text examples: {}", examples()))
                .accessibility_label(examples()),
        ))
    }
}

pub fn key_sequence(_: &PageCx) -> AnyView {
    KeySequencePage.into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "button" => button,
        "checkbox" => checkbox,
        "tool-button" => tool_button,
        "button-group" => button_group,
        "key-sequence" => key_sequence,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, SemanticAction};

    fn press_with(page: &mut Harness, key: Key, modifiers: ShortcutModifiers) {
        page.host
            .key(KeyEvent::Down { key, repeat: false }, modifiers);
        page.host.key(KeyEvent::Up { key }, modifiers);
        page.host.layout();
    }

    fn press(page: &mut Harness, key: Key) {
        press_with(page, key, ShortcutModifiers::default());
    }

    fn command() -> ShortcutModifiers {
        ShortcutModifiers {
            command: true,
            ..ShortcutModifiers::default()
        }
    }

    #[test]
    fn buttons_count_activations_and_the_checkbox_enables_archive() {
        let mut page = Harness::new("button");
        assert!(page.act("Say hello (⌘↩)", SemanticAction::Activate));
        assert!(page.host.find("Activations: 1").is_some());
        assert!(!page.act("Archive (command disabled)", SemanticAction::Activate));
        assert!(page.act("Enable the Archive command", SemanticAction::Activate));
        assert!(page.act("Archive (command disabled)", SemanticAction::Activate));
        assert!(page.host.find("Activations: 2").is_some());
        press_with(&mut page, Key::Enter, command());
        assert!(page.host.find("Activations: 3").is_some());
    }

    #[test]
    fn checkboxes_show_mixed_invalid_and_disabled_states() {
        let mut page = Harness::new("checkbox");
        assert_eq!(page.value("Select all files").as_deref(), Some("mixed"));
        let terms = page.node("Accept the terms");
        assert!(page.host.semantic(terms).unwrap().state.invalid);
        let sync = page.node("Sync settings (unavailable)");
        assert!(page.host.semantic(sync).unwrap().state.disabled);
        assert!(page.act("Select all files", SemanticAction::Activate));
        assert_ne!(page.value("Select all files").as_deref(), Some("mixed"));
    }

    #[test]
    fn tool_buttons_report_the_last_action() {
        let mut page = Harness::new("tool-button");
        assert!(page.host.find("Last action: None yet").is_some());
        assert!(page.act("Bold", SemanticAction::Activate));
        assert!(page.host.find("Last action: Bold on").is_some());
        assert!(page.act("Recent", SemanticAction::ShowMenu));
        assert!(page.host.find("Last action: Recent menu opened").is_some());
        assert!(page.act("Cut", SemanticAction::Activate));
        assert!(page.host.find("Last action: Cut activated").is_some());
    }

    #[test]
    fn the_alignment_group_moves_its_check_past_justify() {
        let mut page = Harness::new("button-group");
        assert!(page.host.find("Alignment").is_some());
        assert!(page.act("Left", SemanticAction::Focus));
        for name in ["Center", "Right", "Left"] {
            press(&mut page, Key::ArrowRight);
            let node = page.node(name);
            assert_eq!(page.host.semantic(node).unwrap().state.checked, Some(true));
        }
    }

    #[test]
    fn the_shortcut_field_records_two_chords() {
        let mut page = Harness::new("key-sequence");
        assert!(page.act("Shortcut", SemanticAction::Focus));
        assert!(page.act("Shortcut", SemanticAction::Activate));
        press_with(&mut page, Key::Character('k'), command());
        press_with(&mut page, Key::Character('s'), command());
        assert_eq!(page.value("Shortcut").as_deref(), Some("⌘K, ⌘S"));
        assert!(page.host.find("Last change: Committed").is_some());
    }
}
