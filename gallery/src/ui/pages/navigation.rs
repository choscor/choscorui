//! Keyboard navigation: mnemonics and default/cancel dialog keys.

use crate::ui::page::{Build, PageCx, column};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Button, Cx, Env, Frame, HStack, HorizontalAlignment, Input, State, Text, TextContent,
    Textarea, VStack, View, ViewExt,
};
use choscorui::controls::{
    ButtonVariant, CommandId, CommandRegistry, CommandState, DialogKeyRoute, DialogKeys,
    ShortcutModifiers, parse_mnemonic,
};
use choscorui::core::{ColorRole, Key, KeyEvent, Space, TextPreset};

/// The translation of `key`, for strings that state keeps.
fn text(env: &Env, key: &str) -> String {
    env.text(key, &[])
}

/// Catalog keys of labels with mnemonics; in English `&P` is shared, so
/// pressing it cycles focus.
const MNEMONIC_LABELS: [&str; 5] = [
    "mnemonics.save",
    "mnemonics.save_as",
    "mnemonics.print",
    "mnemonics.preview",
    "mnemonics.fish",
];

/// The chord that presses a mnemonic.
const MNEMONIC_CHORD: ShortcutModifiers = ShortcutModifiers {
    shift: false,
    control: true,
    option: true,
    command: false,
};

/// A small muted line that follows `text`.
fn status(text: State<String>) -> impl View {
    Text::new(move || text.get())
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
}

/// Buttons reachable with Control-Option and their mnemonic letter. A
/// letter that one button has activates it; a shared letter moves focus
/// to the next button that has it.
struct MnemonicsPage;

/// One mnemonic button: its display label, letter, and focus.
#[derive(Clone)]
struct Mnemonic {
    label: String,
    letter: Option<char>,
    focused: State<bool>,
}

/// The status after activating the button labeled `label`.
fn activated(env: &Env, label: &str) -> String {
    env.text("mnemonics.activated", &[("label", label)])
}

/// Presses Control-Option-`letter` among `buttons`: the status says what
/// it activated or focused.
fn press(env: &Env, buttons: &[Mnemonic], letter: char, last: State<String>) {
    let matches: Vec<&Mnemonic> = buttons
        .iter()
        .filter(|button| button.letter == Some(letter))
        .collect();
    let (hit, activate) = match matches.as_slice() {
        [] => return,
        [only] => (*only, true),
        many => {
            let next = many
                .iter()
                .position(|button| button.focused.get())
                .map_or(0, |index| (index + 1) % many.len());
            (many[next], false)
        }
    };
    hit.focused.set(true);
    last.set(if activate {
        activated(env, &hit.label)
    } else {
        env.text("mnemonics.focused", &[("label", &hit.label)])
    });
}

impl View for MnemonicsPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let env = cx.env().clone();
        let last = cx.state(text(&env, "mnemonics.prompt"));
        let buttons: Vec<Mnemonic> = MNEMONIC_LABELS
            .iter()
            .map(|key| {
                let (label, letter) = parse_mnemonic(&text(&env, key));
                Mnemonic {
                    label,
                    letter,
                    focused: cx.state(false),
                }
            })
            .collect();
        let row = HStack::new(
            buttons
                .iter()
                .map(|button| {
                    let (env, label) = (env.clone(), button.label.clone());
                    Button::new(button.label.clone(), move || {
                        last.set(activated(&env, &label));
                    })
                    .variant(ButtonVariant::Outline)
                    .focused(button.focused.binding())
                    .accessibility_hint(
                        button
                            .letter
                            .map(|letter| format!("Control-Option-{}", letter.to_uppercase()))
                            .unwrap_or_default(),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .spacing(Space::S2);
        let mut letters: Vec<char> = buttons.iter().filter_map(|button| button.letter).collect();
        letters.sort_unstable();
        letters.dedup();
        let page = column((row, status(last)));
        letters.into_iter().fold(page.into_any(), |page, letter| {
            let (env, buttons) = (env.clone(), buttons.clone());
            page.keyboard_shortcut(Key::Character(letter), MNEMONIC_CHORD, move || {
                press(&env, &buttons, letter, last);
            })
            .into_any()
        })
    }
}

pub fn mnemonics(_: &PageCx) -> AnyView {
    MnemonicsPage.into_any()
}

const OK: CommandId = CommandId(1);
const CANCEL: CommandId = CommandId(2);

/// A small form where Return runs the default button and Escape (or
/// ⌘-period) cancels. The multi-line note uses Return itself, so the
/// default button does not run there.
struct DialogKeysPage;

/// The form's state.
#[derive(Clone, Copy)]
struct Form {
    name: State<String>,
    note: State<String>,
    ok_enabled: State<bool>,
    last: State<String>,
}

impl Form {
    /// Routes a key that the focused control refused, as the dialog
    /// would: the default or cancel command runs, and a disabled OK
    /// consumes Return. The status is in `env`.
    fn route(self, env: &Env, key: Key, modifiers: ShortcutModifiers) {
        let mut commands = CommandRegistry::new();
        for command in [OK, CANCEL] {
            let _ = commands.insert(command, CommandState::default());
        }
        let _ = commands.set_enabled(OK, self.ok_enabled.get());
        let keys = DialogKeys {
            default: Some(OK),
            cancel: Some(CANCEL),
        };
        let event = KeyEvent::Down { key, repeat: false };
        match keys.route(event, modifiers, false, &commands) {
            DialogKeyRoute::Run(command) => self.run(env, command),
            DialogKeyRoute::Disabled(_) => {
                self.last.set(text(env, "dialog-keys.return_ignored"));
            }
            DialogKeyRoute::Ignored => {}
        }
    }

    fn run(self, env: &Env, command: CommandId) {
        self.last.set(if command == OK {
            env.text("dialog-keys.saved", &[("name", &self.name.get())])
        } else {
            text(env, "dialog-keys.cancelled")
        });
    }

    fn toggle_ok(self, env: &Env) {
        let enabled = !self.ok_enabled.get();
        self.ok_enabled.set(enabled);
        self.last.set(if enabled {
            text(env, "dialog-keys.ok_enabled")
        } else {
            text(env, "dialog-keys.ok_disabled")
        });
    }
}

/// A caption above a field. It repeats the field's name, so assistive
/// technology skips it.
fn field_caption(text: impl Into<TextContent>) -> impl View {
    Text::new(text.into())
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
        .accessibility_hidden(true)
}

fn labeled(caption: impl Into<TextContent>, field: impl View) -> impl View {
    VStack::new((field_caption(caption), field))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S1)
}

impl View for DialogKeysPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let env = cx.env().clone();
        let form = Form {
            name: cx.state("Lựu".to_owned()),
            note: cx.state(String::new()),
            ok_enabled: cx.state(true),
            last: cx.state(text(&env, "dialog-keys.prompt")),
        };
        let fields = (
            labeled(
                t!("dialog-keys.name"),
                Input::new(t!("dialog-keys.name"), form.name.binding())
                    .frame(Frame::new().width(320.0)),
            ),
            labeled(
                t!("dialog-keys.note_caption"),
                Textarea::new(t!("dialog-keys.note"), form.note.binding())
                    .frame(Frame::size(320.0, 80.0)),
            ),
        );
        let ok_enabled = form.ok_enabled;
        let on = |action: fn(Form, &Env)| {
            let env = env.clone();
            move || action(form, &env)
        };
        let buttons = HStack::new((
            Button::new(
                t!("dialog-keys.cancel"),
                on(|form, env| form.run(env, CANCEL)),
            )
            .variant(ButtonVariant::Outline)
            .accessibility_hint(text(&env, "dialog-keys.cancel_hint")),
            // The default button takes the primary look while it can run.
            Button::new(t!("dialog-keys.ok"), on(|form, env| form.run(env, OK)))
                .variant(move || {
                    if ok_enabled.get() {
                        ButtonVariant::Default
                    } else {
                        ButtonVariant::Outline
                    }
                })
                .disabled(move || !ok_enabled.get())
                .accessibility_hint(text(&env, "dialog-keys.ok_hint")),
            Button::new(
                move || {
                    if ok_enabled.get() {
                        t!("dialog-keys.disable_ok")
                    } else {
                        t!("dialog-keys.enable_ok")
                    }
                },
                on(Form::toggle_ok),
            )
            .variant(ButtonVariant::Outline),
        ))
        .spacing(Space::S2);
        let plain = ShortcutModifiers::default();
        let command = ShortcutModifiers {
            command: true,
            ..plain
        };
        let route = |key: Key, modifiers: ShortcutModifiers| {
            let env = env.clone();
            move || form.route(&env, key, modifiers)
        };
        column((fields, buttons, status(form.last)))
            .keyboard_shortcut(Key::Enter, plain, route(Key::Enter, plain))
            .keyboard_shortcut(Key::Escape, plain, route(Key::Escape, plain))
            .keyboard_shortcut(
                Key::Character('.'),
                command,
                route(Key::Character('.'), command),
            )
    }
}

pub fn dialog_keys(_: &PageCx) -> AnyView {
    DialogKeysPage.into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "mnemonics" => mnemonics,
        "dialog-keys" => dialog_keys,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::MNEMONIC_CHORD;
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, SemanticAction};

    fn press(page: &mut Harness, key: Key, modifiers: ShortcutModifiers) -> bool {
        let handled = page
            .host
            .key(KeyEvent::Down { key, repeat: false }, modifiers);
        page.host.layout();
        handled
    }

    fn focused(page: &mut Harness, name: &str) -> bool {
        let node = page.node(name);
        page.host.tree().focus() == Some(node)
    }

    #[test]
    fn a_unique_mnemonic_activates_and_a_shared_one_cycles_focus() {
        let mut page = Harness::new("mnemonics");
        let save = page.node("Save");
        let description = page.host.semantic(save).unwrap().description;
        assert_eq!(description, "Control-Option-S");
        assert!(page.host.find("Fish & Chips").is_some());
        assert!(press(&mut page, Key::Character('a'), MNEMONIC_CHORD));
        assert!(page.host.find("Activated “Save As…”").is_some());
        assert!(focused(&mut page, "Save As…"));
        assert!(press(&mut page, Key::Character('p'), MNEMONIC_CHORD));
        assert!(focused(&mut page, "Print"));
        assert!(
            page.host
                .find("Focused “Print”; press again for the next match")
                .is_some()
        );
        assert!(press(&mut page, Key::Character('p'), MNEMONIC_CHORD));
        assert!(focused(&mut page, "Preview"));
        assert!(press(&mut page, Key::Character('p'), MNEMONIC_CHORD));
        assert!(focused(&mut page, "Print"));
        page.act("Save", SemanticAction::Activate);
        assert!(page.host.find("Activated “Save”").is_some());
    }

    #[test]
    fn return_runs_the_default_button_except_in_the_note() {
        let mut page = Harness::new("dialog-keys");
        let plain = ShortcutModifiers::default();
        page.act("Name", SemanticAction::Focus);
        assert!(press(&mut page, Key::Enter, plain));
        assert!(page.host.find("OK: saved “Lựu”").is_some());
        assert!(press(&mut page, Key::Escape, plain));
        assert!(page.host.find("Cancelled").is_some());
        // The note uses Return before the shortcut: it breaks the line at
        // the caret, and the default button does not run.
        page.act("Note", SemanticAction::Focus);
        assert!(page.host.text("Hi"));
        assert!(press(&mut page, Key::ArrowLeft, plain));
        assert!(press(&mut page, Key::Enter, plain));
        assert_eq!(page.value("Note").as_deref(), Some("H\ni"));
        assert!(page.host.find("Cancelled").is_some());
        let command = ShortcutModifiers {
            command: true,
            ..plain
        };
        page.act("Name", SemanticAction::Focus);
        assert!(press(&mut page, Key::Enter, plain));
        assert!(press(&mut page, Key::Character('.'), command));
        assert!(page.host.find("Cancelled").is_some());
    }

    #[test]
    fn a_disabled_ok_consumes_return() {
        let mut page = Harness::new("dialog-keys");
        page.act("Disable OK", SemanticAction::Activate);
        let ok = page.node("OK");
        assert!(page.host.semantic(ok).unwrap().state.disabled);
        assert!(
            page.host
                .find("OK disabled: Return is consumed but does nothing")
                .is_some()
        );
        assert!(press(&mut page, Key::Enter, ShortcutModifiers::default()));
        assert!(page.host.find("Return ignored: OK is disabled").is_some());
        page.act("Enable OK", SemanticAction::Activate);
        assert!(!page.host.semantic(ok).unwrap().state.disabled);
    }
}
