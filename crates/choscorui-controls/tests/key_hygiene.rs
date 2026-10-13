//! Space on key-up, repeats, and Command or Control chords that go on to
//! the shortcuts, through the control host.

mod hosted;

use choscorui_controls::{
    ControlMessage, ControlValue, KeySequenceEdit, ShortcutModifiers, Tabs, Toggle, ToggleGroup,
    ToggleItem,
};
use choscorui_core::{Key, KeyEvent};
use hosted::Harness;

fn down(h: &mut Harness, key: Key, repeat: bool) -> bool {
    h.host
        .dispatch_key(&mut h.tree, KeyEvent::Down { key, repeat })
}

fn up(h: &mut Harness, key: Key) -> bool {
    h.host.dispatch_key(&mut h.tree, KeyEvent::Up { key })
}

const COMMAND: ShortcutModifiers = ShortcutModifiers {
    shift: false,
    control: false,
    option: false,
    command: true,
};

#[test]
fn a_button_like_control_activates_on_space_up_and_never_on_a_repeat() {
    let mut h = Harness::new(Toggle::new("Bold", false));
    h.focus();
    assert!(down(&mut h, Key::Space, false));
    assert!(h.messages().is_empty(), "Space down only presses");
    assert!(down(&mut h, Key::Space, true), "a repeat is consumed");
    assert!(up(&mut h, Key::Space));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Bool(true))]
    );
    // A held Enter toggles once.
    assert!(down(&mut h, Key::Enter, false));
    assert!(down(&mut h, Key::Enter, true));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Bool(false))]
    );
    // Command+Enter goes on to the shortcuts.
    h.modifiers(COMMAND);
    assert!(!down(&mut h, Key::Enter, false));
    assert!(h.messages().is_empty());
}

#[test]
fn a_held_key_toggles_a_group_item_once() {
    let items = vec![ToggleItem::new("Bold"), ToggleItem::new("Italic")];
    let mut h = Harness::new(ToggleGroup::new("Style", items, true));
    h.focus();
    assert!(down(&mut h, Key::Space, false));
    assert!(down(&mut h, Key::Space, true));
    assert!(down(&mut h, Key::Space, true));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![0]))]
    );
    h.modifiers(COMMAND);
    assert!(!down(&mut h, Key::ArrowRight, false), "a chord goes on");
}

#[test]
fn a_tab_list_lets_command_chords_go_to_the_shortcuts() {
    let tabs = Tabs::hosted("Sections", ["Account", "Password"], 0).unwrap();
    let mut h = Harness::new(tabs);
    h.focus();
    h.modifiers(COMMAND);
    assert!(!down(&mut h, Key::ArrowRight, false));
    assert_eq!(h.semantics().value.as_deref(), Some("Account"));
    h.modifiers(ShortcutModifiers::default());
    assert!(down(&mut h, Key::ArrowRight, false));
    assert_eq!(h.semantics().value.as_deref(), Some("Password"));
}

#[test]
fn a_recording_key_sequence_edit_takes_repeats_and_releases() {
    let mut h = Harness::new(KeySequenceEdit::new(1).with_name("Shortcut"));
    h.focus();
    h.key(Key::Enter);
    assert!(down(&mut h, Key::Tab, true), "a repeat does not move focus");
    assert!(up(&mut h, Key::Tab));
}
