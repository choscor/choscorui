//! Buttons and other momentary or toggled inputs.

use crate::ui::page::{Build, PageCx, caption, column, row, section, size_key};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Button, ButtonGroup, Checkbox, Cx, Env, Frame, HStack, HorizontalAlignment,
    KeySequenceEdit, Localized, State, Text, ToolButton, VStack, View, ViewExt,
};
use choscorui::controls::{
    self, ButtonGroupMember, ButtonVariant, CommandId, ControlMessage, ControlSize, ControlValue,
    Orientation, Shortcut, ShortcutModifiers, ToolButtonMenu, shortcut_text,
};
use choscorui::core::{ColorRole, Icon, Key, Space, TextPreset};

/// A small muted line whose catalog string follows state.
fn readout(text: Localized) -> impl View {
    Text::new(text)
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
}

/// Buttons in several variants, one that a checkbox enables, a window
/// shortcut, and each variant in each size.
struct ButtonPage;

impl View for ButtonPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let activations = cx.state(0_usize);
        let archive = cx.state(false);
        let count = move || activations.update(|count| *count += 1);
        let width = || Frame::new().width(260.0);
        let command = ShortcutModifiers {
            command: true,
            ..ShortcutModifiers::default()
        };
        column((
            Button::new(t!("button.hello"), count).frame(width()),
            Button::new(t!("button.outline"), count)
                .variant(ButtonVariant::Outline)
                .icon(Icon::Send)
                .frame(width()),
            Button::new(t!("button.archive"), count)
                .variant(ButtonVariant::Destructive)
                .disabled(move || !archive.get())
                .frame(width()),
            Checkbox::new(t!("button.enable_archive"), archive.binding())
                .frame(Frame::new().width(320.0)),
            readout(t!("button.activations", count = activations)),
            VStack::new(
                VARIANTS
                    .into_iter()
                    .map(|(variant, key)| section(t!(key), sizes(variant, count)))
                    .collect::<Vec<_>>(),
            )
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S4),
        ))
        .keyboard_shortcut(Key::Enter, command, count)
    }
}

/// Each button variant with the catalog key of its name.
const VARIANTS: [(ButtonVariant, &str); 6] = [
    (ButtonVariant::Default, "badge.default"),
    (ButtonVariant::Destructive, "badge.destructive"),
    (ButtonVariant::Outline, "badge.outline"),
    (ButtonVariant::Secondary, "badge.secondary"),
    (ButtonVariant::Ghost, "badge.ghost"),
    (ButtonVariant::Link, "badge.link"),
];

/// A button of `variant` in each size, labelled with the size; the icon
/// sizes show only an icon.
fn sizes(variant: ButtonVariant, action: impl Fn() + Copy + 'static) -> impl View {
    row(ControlSize::ALL
        .into_iter()
        .map(|size| {
            let button = Button::new(t!(size_key(size)), action)
                .variant(variant)
                .size(size);
            if size.is_icon() {
                button.icon(Icon::Send)
            } else {
                button
            }
        })
        .collect::<Vec<_>>())
}

pub fn button(_: &PageCx) -> AnyView {
    ButtonPage.into_any()
}

/// Checked, unchecked, mixed, invalid, and permanently disabled
/// checkboxes.
struct CheckboxPage;

impl View for CheckboxPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        // `None` is the mixed state.
        let mut row = |key: &'static str, checked: Option<bool>, invalid: bool, enabled: bool| {
            let checked: State<Option<bool>> = cx.state(checked);
            Checkbox::mixed(t!(key), checked.binding())
                .with(move |checkbox| checkbox.with_invalid(invalid))
                .disabled(!enabled)
                .frame(Frame::new().width(320.0))
        };
        column((
            row("checkbox.line_numbers", Some(true), false, true),
            row("checkbox.wrap", Some(false), false, true),
            row("checkbox.select_all", None, false, true),
            row("checkbox.terms", Some(false), true, true),
            row("checkbox.sync", Some(true), false, false),
        ))
    }
}

pub fn checkbox(_: &PageCx) -> AnyView {
    CheckboxPage.into_any()
}

/// Plain, checkable, instant-menu, and split-menu tool buttons.
struct ToolButtonPage;

impl View for ToolButtonPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let env = cx.env().clone();
        let last = cx.state(env.text("tool-button.none", &[]));
        let mut tool = |key: &'static str, checkable: bool, menu, width: f64| {
            let checked = cx.state(false);
            let name = env.text(key, &[]);
            let reported = env.clone();
            let report = move |key: &str| last.set(reported.text(key, &[("name", &name)]));
            let activated = report.clone();
            let button = ToolButton::new(t!(key), move || activated("tool-button.activated"))
                .menu(menu)
                .on_message(move |message| match message {
                    ControlMessage::Changed(ControlValue::Bool(true)) => report("tool-button.on"),
                    ControlMessage::Changed(ControlValue::Bool(false)) => report("tool-button.off"),
                    ControlMessage::OpenChanged(true) => report("tool-button.menu_opened"),
                    _ => {}
                });
            if checkable {
                button.checked(checked.binding())
            } else {
                button
            }
            .frame(Frame::new().width(width))
        };
        column((
            HStack::new((
                tool("tool-button.cut", false, ToolButtonMenu::None, 72.0),
                tool("tool-button.bold", true, ToolButtonMenu::None, 72.0),
                tool("tool-button.recent", false, ToolButtonMenu::Instant, 96.0),
                tool("tool-button.undo", false, ToolButtonMenu::Split, 96.0),
            ))
            .spacing(Space::S2),
            readout(t!("tool-button.last", action = last)),
        ))
    }
}

pub fn tool_button(_: &PageCx) -> AnyView {
    ToolButtonPage.into_any()
}

/// A joined group of `members` named by the catalog strings `labels`; a
/// separator has an empty key. `checked` edits the checked toggles, and
/// the members join along `orientation`.
fn group(
    members: Vec<ButtonGroupMember>,
    labels: &'static [&'static str],
    exclusive: bool,
    checked: State<Vec<usize>>,
    orientation: Orientation,
) -> ButtonGroup {
    let names = move |env: &Env| -> Vec<String> {
        labels
            .iter()
            .map(|key| match *key {
                "" => String::new(),
                key => env.text(key, &[]),
            })
            .collect()
    };
    let group =
        controls::ButtonGroup::from_members(members, exclusive).with_orientation(orientation);
    ButtonGroup::new(group, names, checked.binding())
}

/// `count` toggle members, each 92 × 36 pt, and the frame that holds them
/// along `orientation`.
fn toggles(count: u64, orientation: Orientation) -> (Vec<ButtonGroupMember>, Frame) {
    let members = (1..=count)
        .map(|id| ButtonGroupMember::Toggle(CommandId(id)))
        .collect();
    let frame = match orientation {
        Orientation::Horizontal => Frame::new().width(92.0 * count as f64),
        Orientation::Vertical => Frame::new().width(92.0).height(36.0 * count as f64),
    };
    (members, frame)
}

/// A group under its caption; assistive technology reads the caption as
/// `name`.
fn titled(title: &'static str, name: String, content: impl View) -> impl View {
    VStack::new((caption(t!(title)).accessibility_label(name), content))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S2)
}

/// An exclusive alignment group, a non-exclusive style group, a vertical
/// group, and a text size group of outline buttons with a separator and a
/// text part.
struct ButtonGroupPage;

impl View for ButtonGroupPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let alignment = cx.state(vec![0]);
        let style = cx.state(vec![0]);
        let vertical = cx.state(vec![1]);
        let size = cx.state(14_u32);
        let text = |key: &str| cx.env().text(key, &[]);
        let (alignment_name, style_name, vertical_name, size_name) = (
            text("button-group.alignment"),
            text("button-group.style"),
            text("button-group.vertical_name"),
            text("button-group.size"),
        );
        let (across, down) = (Orientation::Horizontal, Orientation::Vertical);
        let (four, four_frame) = toggles(4, across);
        let (three, three_frame) = toggles(3, across);
        let (stack, stack_frame) = toggles(3, down);
        let outline = |id| ButtonGroupMember::Button(CommandId(id), ButtonVariant::Outline);
        let parts = vec![
            ButtonGroupMember::Text,
            outline(1),
            ButtonGroupMember::Separator,
            outline(2),
        ];
        column((
            titled(
                "button-group.exclusive",
                alignment_name,
                group(
                    four,
                    &[
                        "button-group.left",
                        "button-group.center",
                        "button-group.right",
                        "button-group.justify",
                    ],
                    true,
                    alignment,
                    across,
                )
                .with(|group| group.with_enabled(3, false))
                .frame(four_frame),
            ),
            titled(
                "button-group.non_exclusive",
                style_name,
                group(
                    three,
                    &[
                        "tool-button.bold",
                        "button-group.italic",
                        "button-group.underline",
                    ],
                    false,
                    style,
                    across,
                )
                .frame(three_frame),
            ),
            titled(
                "button-group.vertical",
                vertical_name,
                group(
                    stack,
                    &[
                        "button-group.top",
                        "button-group.middle",
                        "button-group.bottom",
                    ],
                    true,
                    vertical,
                    down,
                )
                .frame(stack_frame),
            ),
            titled(
                "button-group.parts",
                size_name,
                group(
                    parts,
                    &[
                        "button-group.size",
                        "button-group.smaller",
                        "",
                        "button-group.larger",
                    ],
                    false,
                    cx.state(Vec::new()),
                    across,
                )
                .on_item(move |index| {
                    size.update(|size| {
                        *size = if index == 1 {
                            size.saturating_sub(1).max(8)
                        } else {
                            (*size + 1).min(32)
                        }
                    })
                }),
            ),
            readout(t!("button-group.size_value", size = size)),
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
    fn body(&self, cx: &mut Cx) -> impl View {
        let env = cx.env().clone();
        let last = cx.state(env.text("key-sequence.none", &[]));
        let sequence = cx.state(String::new());
        column((
            KeySequenceEdit::new(t!("key-sequence.shortcut"), 2, sequence.binding())
                .with_env(|edit, env| {
                    edit.with_placeholder(env.text("key-sequence.placeholder", &[]))
                })
                .on_message(move |message| {
                    if let ControlMessage::Changed(ControlValue::Text(text)) = message {
                        let key = if text.is_empty() {
                            "key-sequence.cleared"
                        } else {
                            "key-sequence.committed"
                        };
                        last.set(env.text(key, &[]));
                    }
                })
                .frame(Frame::new().width(320.0)),
            readout(t!("key-sequence.last", change = last)),
            caption(t!("key-sequence.examples", examples = examples()))
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
    fn the_text_size_group_steps_the_size_and_skips_its_parts() {
        let mut page = Harness::new("button-group");
        assert!(page.host.find("Text size: 14 pt").is_some());
        assert!(page.act("Smaller", SemanticAction::Activate));
        assert!(page.host.find("Text size: 13 pt").is_some());
        let smaller = page.node("Smaller");
        assert_eq!(
            page.host.semantic(smaller).unwrap().description,
            "Text size"
        );
        assert!(page.act("Smaller", SemanticAction::Focus));
        // The arrow key passes the separator to the other button.
        press(&mut page, Key::ArrowRight);
        press(&mut page, Key::Enter);
        press(&mut page, Key::Enter);
        assert!(page.host.find("Text size: 15 pt").is_some());
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
