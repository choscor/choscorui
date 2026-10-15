//! The keyboard conventions of a control host: the shortcut labels of a
//! menu and a shortcut field, and the editing keys of a text input and a
//! text area. Each record plays the same physical keys on `Mac` and on
//! `Pc`, so the golden file shows how the two conventions differ.

use super::{mount_hosted, write_parts};
use choscorui::controls::{
    CommandId, ContextMenu, ControlHost, ControlValue, CustomControl, EstimatedText, HostEnv,
    Input, KeySequenceEdit, MenuItem, ShortcutModifiers, Styles, Textarea,
};
use choscorui::core::{
    DesignTokens, DisplayCommand, DisplayList, InputModality, Key, KeyConvention, KeyEvent,
    LayoutDirection, NodeId, ThemeResolver, Tree,
};
use std::fmt::Write;

const CONVENTIONS: [KeyConvention; 2] = [KeyConvention::Mac, KeyConvention::Pc];

/// The modifiers that a backend reports: `control` is Ctrl, `option` is
/// Alt or Option, and `command` is Command or the Super key.
const fn held(shift: bool, control: bool, option: bool, command: bool) -> ShortcutModifiers {
    ShortcutModifiers {
        shift,
        control,
        option,
        command,
    }
}

const NONE: ShortcutModifiers = held(false, false, false, false);
const SHIFT: ShortcutModifiers = held(true, false, false, false);
const CTRL: ShortcutModifiers = held(false, true, false, false);
const CTRL_SHIFT: ShortcutModifiers = held(true, true, false, false);
const ALT: ShortcutModifiers = held(false, false, true, false);
const META: ShortcutModifiers = held(false, false, false, true);
const META_SHIFT: ShortcutModifiers = held(true, false, false, true);

/// A step: its name, the physical modifiers, and the key or the typed
/// text.
type Step = (&'static str, ShortcutModifiers, Play);

/// A key press or typed text.
#[derive(Clone, Copy)]
enum Play {
    Key(Key),
    Text(&'static str),
}

/// The keys that a single-line input plays, from the end of its text.
const LINE_STEPS: &[Step] = &[
    ("Ctrl+Left", CTRL, Play::Key(Key::ArrowLeft)),
    ("Alt+Left", ALT, Play::Key(Key::ArrowLeft)),
    ("Meta+Left", META, Play::Key(Key::ArrowLeft)),
    ("End", NONE, Play::Key(Key::End)),
    ("Ctrl+Shift+Left", CTRL_SHIFT, Play::Key(Key::ArrowLeft)),
    ("Home", NONE, Play::Key(Key::Home)),
    ("Shift+End", SHIFT, Play::Key(Key::End)),
    ("Right", NONE, Play::Key(Key::ArrowRight)),
    ("Ctrl+Backspace", CTRL, Play::Key(Key::Backspace)),
    ("Alt+Backspace", ALT, Play::Key(Key::Backspace)),
    ("Meta+Backspace", META, Play::Key(Key::Backspace)),
    ("type", NONE, Play::Text("!")),
    ("Ctrl+Z", CTRL, Play::Key(Key::Character('z'))),
    ("Meta+Z", META, Play::Key(Key::Character('z'))),
    ("Ctrl+Y", CTRL, Play::Key(Key::Character('y'))),
    ("Ctrl+Shift+Z", CTRL_SHIFT, Play::Key(Key::Character('z'))),
    ("Meta+Shift+Z", META_SHIFT, Play::Key(Key::Character('z'))),
    ("Ctrl+A", CTRL, Play::Key(Key::Character('a'))),
    ("Meta+A", META, Play::Key(Key::Character('a'))),
];

/// The keys that a text area plays, from the start of its text.
const AREA_STEPS: &[Step] = &[
    ("Ctrl+End", CTRL, Play::Key(Key::End)),
    ("Meta+Down", META, Play::Key(Key::ArrowDown)),
    ("Home", NONE, Play::Key(Key::Home)),
    ("Ctrl+Home", CTRL, Play::Key(Key::Home)),
    ("Alt+Home", ALT, Play::Key(Key::Home)),
    ("Ctrl+Shift+End", CTRL_SHIFT, Play::Key(Key::End)),
    ("Meta+Shift+Down", META_SHIFT, Play::Key(Key::ArrowDown)),
    ("Right", NONE, Play::Key(Key::ArrowRight)),
    ("Ctrl+Backspace", CTRL, Play::Key(Key::Backspace)),
    ("Alt+Backspace", ALT, Play::Key(Key::Backspace)),
];

/// Mounts the control that `build` makes with `convention` and settles
/// it again, as a window does after the change.
fn mount(
    build: impl FnOnce(&mut Tree) -> Box<dyn CustomControl>,
    convention: KeyConvention,
    themes: &ThemeResolver,
    styles: &Styles,
) -> (Tree, ControlHost, NodeId) {
    let (mut tree, mut host, node) = mount_hosted(build, themes, styles, LayoutDirection::Ltr);
    host.set_key_convention(&mut tree, convention);
    let env = HostEnv::new(themes, styles, &EstimatedText);
    host.arrange(&tree, &env);
    (tree, host, node)
}

/// The text runs that `host` paints, with their places.
/// The time is past the opening motion of a menu.
fn write_text(out: &mut String, tree: &Tree, host: &ControlHost, env: &HostEnv<'_>) {
    let mut list = DisplayList::new();
    host.paint(tree, &mut list, &env.time(10.0));
    for command in list.commands() {
        if let DisplayCommand::DrawText { .. } = command {
            writeln!(out, "  {command:?}").unwrap();
        }
    }
}

/// A context menu with shortcut texts, open, and a shortcut field with a
/// sequence: the painted labels and the semantic parts.
fn labels(out: &mut String, themes: &ThemeResolver, styles: &Styles) {
    let env = HostEnv::new(themes, styles, &EstimatedText);
    for convention in CONVENTIONS {
        writeln!(out, "# menu labels {convention:?}").unwrap();
        let (mut tree, mut host, node) = mount(
            |_| {
                Box::new(ContextMenu::new(
                    "Area",
                    "Right click here",
                    vec![
                        MenuItem::action("Save", CommandId(1)).with_shortcut("⌘S"),
                        MenuItem::action("Save As", CommandId(2)).with_shortcut("⇧⌘S"),
                        MenuItem::action("Close", CommandId(3)).with_shortcut("⌥F4"),
                        MenuItem::action("Full Screen", CommandId(4)).with_shortcut("⌃⌘F"),
                        MenuItem::action("Go", CommandId(5)).with_shortcut("⌘K, ⌘G"),
                        MenuItem::action("Note", CommandId(6)).with_shortcut("Any key"),
                    ],
                ))
            },
            convention,
            themes,
            styles,
        );
        assert!(host.set_open(&mut tree, node, true));
        host.arrange(&tree, &env);
        host.advance(&mut tree, 10.0);
        host.arrange(&tree, &env);
        write_text(out, &tree, &host, &env);
        write_parts(out, &host.parts(node));

        writeln!(out, "# shortcut field {convention:?}").unwrap();
        let (mut tree, mut host, node) = mount(
            |_| Box::new(KeySequenceEdit::new(2).with_name("Shortcut")),
            convention,
            themes,
            styles,
        );
        let sequence = ControlValue::Text("⌘K, ⇧⌘Space".to_owned());
        assert!(host.set_value(&mut tree, node, &sequence));
        host.arrange(&tree, &env);
        write_text(out, &tree, &host, &env);
        let semantic = host.get(node).unwrap().semantics_in(host.strings());
        writeln!(out, "  value {:?}", semantic.value).unwrap();
    }
}

/// Plays `steps` in the focused text control that `build` makes, and
/// writes the value and the selection after each step.
fn editing(
    out: &mut String,
    name: &str,
    build: fn(&mut Tree) -> Box<dyn CustomControl>,
    steps: &[Step],
    themes: &ThemeResolver,
    styles: &Styles,
) {
    let env = HostEnv::new(themes, styles, &EstimatedText);
    for convention in CONVENTIONS {
        writeln!(out, "# {name} editing {convention:?}").unwrap();
        let (mut tree, mut host, node) = mount(build, convention, themes, styles);
        tree.set_input_modality(InputModality::Keyboard);
        host.move_focus(&mut tree, Some(node));
        host.arrange(&tree, &env);
        for (step, modifiers, input) in steps {
            host.set_modifiers(*modifiers);
            match input {
                Play::Key(key) => {
                    let down = KeyEvent::Down {
                        key: *key,
                        repeat: false,
                    };
                    host.dispatch_key(&mut tree, down);
                    host.dispatch_key(&mut tree, KeyEvent::Up { key: *key });
                }
                Play::Text(text) => {
                    host.dispatch_text(&mut tree, text);
                }
            }
            host.set_modifiers(NONE);
            host.arrange(&tree, &env);
            let semantic = host.get(node).unwrap().semantics();
            let selection = semantic.text.map(|text| text.selection_utf16);
            writeln!(
                out,
                "  {step}: value {:?} selection {selection:?}",
                semantic.value.unwrap_or_default()
            )
            .unwrap();
        }
    }
}

/// Writes the label records and the editing records.
pub fn record(out: &mut String) {
    let themes = ThemeResolver::new(DesignTokens::light());
    let styles = Styles::new();
    labels(out, &themes, &styles);
    editing(
        out,
        "input",
        |_| Box::new(Input::new("Name", "one two three")),
        LINE_STEPS,
        &themes,
        &styles,
    );
    editing(
        out,
        "textarea",
        |_| Box::new(Textarea::new("Notes", "first line\nsecond line")),
        AREA_STEPS,
        &themes,
        &styles,
    );
}
