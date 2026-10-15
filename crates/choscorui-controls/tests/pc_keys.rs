//! The `Pc` keyboard conventions through the control host: Ctrl runs the
//! `command` shortcuts and Super does not, the PC editing keys, and the
//! PC shortcut labels.

mod hosted;

use choscorui_controls::{
    CommandId, ContextMenu, ControlMessage, EditCommand, Input, MenuItem, Shortcut,
    ShortcutModifiers, Sidebar, SidebarItem, Textarea, shortcut_label, shortcut_text,
};
use choscorui_core::{Icon, Key, KeyConvention, Point, PointerButton, PointerEvent};
use hosted::{Harness, texts};

/// Ctrl, as a backend reports it.
const CTRL: ShortcutModifiers = ShortcutModifiers {
    shift: false,
    control: true,
    option: false,
    command: false,
};

/// The Super key, as a backend reports it.
const SUPER: ShortcutModifiers = ShortcutModifiers {
    shift: false,
    control: false,
    option: false,
    command: true,
};

const SHIFT: ShortcutModifiers = ShortcutModifiers {
    shift: true,
    control: false,
    option: false,
    command: false,
};

const CTRL_SHIFT: ShortcutModifiers = ShortcutModifiers {
    shift: true,
    ..CTRL
};

/// A harness for `control` with the `Pc` conventions, arranged again as
/// a window does after the change.
fn pc(control: impl choscorui_controls::CustomControl + 'static) -> Harness {
    let mut h = Harness::new(control);
    h.host.set_key_convention(&mut h.tree, KeyConvention::Pc);
    let bounds = h.bounds();
    h.place(bounds);
    h
}

fn value(h: &Harness) -> String {
    h.semantics().value.unwrap_or_default()
}

fn sidebar() -> Sidebar {
    Sidebar::new(
        "Navigation",
        "Toggle sidebar",
        vec![(
            "Application".to_string(),
            vec![SidebarItem::new("Inbox", Icon::Inbox)],
        )],
    )
}

#[test]
fn ctrl_runs_a_command_shortcut_and_super_does_not() {
    let mut h = pc(sidebar());
    h.chord(SUPER, Key::Character('b'));
    assert!(h.messages().is_empty(), "Super never matches Command");
    h.chord(CTRL, Key::Character('b'));
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(false)]);
    assert_eq!(h.host.key_convention(), KeyConvention::Pc);
}

#[test]
fn a_new_host_keeps_the_mac_conventions() {
    let mut h = Harness::new(sidebar());
    assert_eq!(h.host.key_convention(), KeyConvention::Mac);
    h.chord(CTRL, Key::Character('b'));
    assert!(h.messages().is_empty(), "Control is not Command on Mac");
    h.chord(SUPER, Key::Character('b'));
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(false)]);
}

#[test]
fn ctrl_arrows_move_by_word_and_home_and_end_go_to_the_line_ends() {
    let mut h = pc(Input::new("Name", "hello world"));
    h.focus();
    h.chord(CTRL, Key::ArrowLeft);
    h.text("big ");
    assert_eq!(value(&h), "hello big world");
    h.key(Key::Home);
    h.text("<");
    h.key(Key::End);
    h.text(">");
    assert_eq!(value(&h), "<hello big world>");
    // Shift extends the word motion: the selection takes "world>".
    h.chord(CTRL_SHIFT, Key::ArrowLeft);
    h.text("all");
    assert_eq!(value(&h), "<hello big all");
    // Shift extends Home: the selection takes the whole line.
    h.chord(SHIFT, Key::Home);
    h.text("new");
    assert_eq!(value(&h), "new");
}

#[test]
fn ctrl_backspace_and_ctrl_delete_delete_a_word() {
    let mut h = pc(Input::new("Name", "one two three"));
    h.focus();
    h.chord(CTRL, Key::Backspace);
    assert_eq!(value(&h), "one two ");
    h.key(Key::Home);
    h.chord(CTRL, Key::Delete);
    assert_eq!(value(&h), " two ");
}

#[test]
fn ctrl_home_and_ctrl_end_go_to_the_document_ends() {
    let mut h = pc(Textarea::new("Notes", "line one\nline two"));
    h.focus();
    h.chord(CTRL, Key::Home);
    h.text("[");
    h.chord(CTRL, Key::End);
    h.text("]");
    assert_eq!(value(&h), "[line one\nline two]");
    // Home goes to the start of the line, not of the document.
    h.key(Key::Home);
    h.text(">");
    assert_eq!(value(&h), "[line one\n>line two]");
    // Shift extends Ctrl+Home to the document start.
    h.chord(CTRL_SHIFT, Key::Home);
    h.text("x");
    assert_eq!(value(&h), "xline two]");
}

#[test]
fn ctrl_z_undoes_and_ctrl_y_and_ctrl_shift_z_redo() {
    let mut h = pc(Input::new("Name", ""));
    h.focus();
    h.text("a");
    h.chord(CTRL, Key::Home);
    h.text("b");
    assert_eq!(value(&h), "ba");
    h.chord(CTRL, Key::Character('z'));
    assert_eq!(value(&h), "a");
    h.chord(CTRL, Key::Character('y'));
    assert_eq!(value(&h), "ba");
    h.chord(CTRL, Key::Character('z'));
    h.chord(CTRL_SHIFT, Key::Character('z'));
    assert_eq!(value(&h), "ba");
    // Super+Z is not an Edit command on Pc.
    h.chord(SUPER, Key::Character('z'));
    assert_eq!(value(&h), "ba");
}

#[test]
fn ctrl_y_redoes_only_on_pc() {
    let command = ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    };
    let y = Key::Character('y');
    assert_eq!(
        EditCommand::from_shortcut(y, command, KeyConvention::Pc),
        Some(EditCommand::Redo)
    );
    assert_eq!(
        EditCommand::from_shortcut(y, command, KeyConvention::Mac),
        None
    );
}

#[test]
fn labels_use_words_on_pc_and_glyphs_on_mac() {
    let chord = |key, modifiers| Shortcut { key, modifiers };
    let command = ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    };
    let command_shift = ShortcutModifiers {
        shift: true,
        ..command
    };
    let option = ShortcutModifiers {
        option: true,
        ..ShortcutModifiers::default()
    };
    let save = chord(Key::Character('s'), command);
    assert_eq!(shortcut_label(save, KeyConvention::Pc), "Ctrl+S");
    assert_eq!(shortcut_label(save, KeyConvention::Mac), "⌘S");
    assert_eq!(
        shortcut_label(save, KeyConvention::Mac),
        shortcut_text(save)
    );
    let save_as = chord(Key::Character('s'), command_shift);
    assert_eq!(shortcut_label(save_as, KeyConvention::Pc), "Ctrl+Shift+S");
    assert_eq!(shortcut_label(save_as, KeyConvention::Mac), "⇧⌘S");
    let close = chord(Key::F(4), option);
    assert_eq!(shortcut_label(close, KeyConvention::Pc), "Alt+F4");
    let start = chord(Key::Home, CTRL);
    assert_eq!(shortcut_label(start, KeyConvention::Pc), "Super+Home");
    let page = chord(Key::PageDown, ShortcutModifiers::default());
    assert_eq!(shortcut_label(page, KeyConvention::Pc), "Page Down");
}

const BACK: CommandId = CommandId(1);
const SAVE: CommandId = CommandId(2);
const NOTE: CommandId = CommandId(3);

fn menu() -> ContextMenu {
    ContextMenu::new(
        "Page actions",
        "Right click here",
        vec![
            MenuItem::action("Back", BACK).with_shortcut("⌘["),
            MenuItem::action("Save As", SAVE).with_shortcut("⇧⌘S"),
            MenuItem::action("Note", NOTE).with_shortcut("Any key"),
        ],
    )
}

fn open_menu(h: &mut Harness, button: PointerButton) {
    let at = h.bounds().center();
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Down {
            position: at,
            button,
        },
    );
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Up {
            position: at,
            button,
        },
    );
}

#[test]
fn a_menu_shows_pc_labels_on_pc() {
    let mut h = pc(menu());
    open_menu(&mut h, PointerButton::Secondary);
    h.set_time(1.0);
    let painted = texts(&h.paint());
    for label in ["Ctrl+[", "Ctrl+Shift+S", "Any key"] {
        assert!(painted.iter().any(|text| text == label), "{painted:?}");
    }
    assert!(
        !painted.iter().any(|text| text.contains('⌘')),
        "{painted:?}"
    );
    let described: Vec<String> = (h.parts().parts().iter())
        .map(|part| part.node.description.clone())
        .filter(|description| !description.is_empty())
        .collect();
    assert_eq!(described, ["Ctrl+[", "Ctrl+Shift+S", "Any key"]);

    let mut mac = Harness::new(menu());
    open_menu(&mut mac, PointerButton::Secondary);
    mac.set_time(1.0);
    let painted = texts(&mac.paint());
    for label in ["⌘[", "⇧⌘S", "Any key"] {
        assert!(painted.iter().any(|text| text == label), "{painted:?}");
    }
}

#[test]
fn a_super_click_does_not_open_a_context_menu_on_pc() {
    let mut h = pc(menu());
    for modifiers in [CTRL, SUPER] {
        h.modifiers(modifiers);
        open_menu(&mut h, PointerButton::Primary);
        h.modifiers(ShortcutModifiers::default());
        assert!(h.host.get(h.node).unwrap().overlay().is_none());
    }
    let at = Point::new(790.0, 590.0);
    h.click(at);
    open_menu(&mut h, PointerButton::Secondary);
    assert!(h.host.get(h.node).unwrap().overlay().is_some());
}
