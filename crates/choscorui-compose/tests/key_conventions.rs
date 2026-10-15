//! The keyboard conventions of a view host: a view shortcut that declares
//! `command` runs with Command on `Mac` and with Ctrl on `Pc`, and the
//! text views follow the editing keys of the conventions.

use choscorui_compose::Runtime;
use choscorui_compose::view::{Cx, Env, Input, Text, VStack, View, ViewExt, ViewHost};
use choscorui_controls::{EstimatedText, ShortcutModifiers};
use choscorui_core::{
    Catalog, DesignTokens, Key, KeyConvention, KeyEvent, Locale, SemanticAction, Size,
};
use std::cell::Cell;
use std::rc::Rc;

/// Ctrl, as a backend reports it.
const CTRL: ShortcutModifiers = ShortcutModifiers {
    shift: false,
    control: true,
    option: false,
    command: false,
};

/// Command on a Mac, or the Super key on a PC, as a backend reports it.
const COMMAND: ShortcutModifiers = ShortcutModifiers {
    shift: false,
    control: false,
    option: false,
    command: true,
};

fn host(view: impl View, convention: Option<KeyConvention>) -> ViewHost {
    let mut host = ViewHost::new(
        Size::new(600.0, 400.0),
        DesignTokens::light(),
        Env::new(Rc::new(Catalog::new()), Locale::parse("en").unwrap()),
        Runtime::new(),
        Rc::new(EstimatedText),
    );
    if let Some(convention) = convention {
        host.set_key_convention(convention);
    }
    host.mount(view);
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    host
}

fn chord(host: &mut ViewHost, modifiers: ShortcutModifiers, key: Key) -> bool {
    let handled = host.key(KeyEvent::Down { key, repeat: false }, modifiers);
    host.key(KeyEvent::Up { key }, modifiers);
    host.layout();
    handled
}

/// A text with a Command+S shortcut that counts its runs.
fn saving(count: Rc<Cell<u32>>) -> impl View {
    Text::new("Document").keyboard_shortcut(Key::Character('s'), COMMAND, move || {
        count.set(count.get() + 1);
    })
}

#[test]
fn ctrl_s_runs_a_command_shortcut_on_pc_and_super_s_does_not() {
    let count = Rc::new(Cell::new(0));
    let mut pc = host(saving(count.clone()), Some(KeyConvention::Pc));
    assert_eq!(pc.key_convention(), KeyConvention::Pc);
    assert!(!chord(&mut pc, COMMAND, Key::Character('s')));
    assert_eq!(count.get(), 0, "Super never matches Command");
    assert!(chord(&mut pc, CTRL, Key::Character('s')));
    assert_eq!(count.get(), 1);
}

#[test]
fn a_new_host_runs_command_shortcuts_with_command() {
    let count = Rc::new(Cell::new(0));
    let mut mac = host(saving(count.clone()), None);
    assert_eq!(mac.key_convention(), KeyConvention::Mac);
    assert!(!chord(&mut mac, CTRL, Key::Character('s')));
    assert_eq!(count.get(), 0);
    assert!(chord(&mut mac, COMMAND, Key::Character('s')));
    assert_eq!(count.get(), 1);
}

/// An input whose text starts as "hello world".
struct Name;

impl View for Name {
    fn body(&self, cx: &mut Cx) -> impl View {
        let text = cx.state(String::from("hello world"));
        VStack::new((Input::new("Name", text.binding()),))
    }
}

fn value(host: &mut ViewHost) -> String {
    let node = host.find("Name").expect("the input");
    host.semantic(node).unwrap().value.unwrap_or_default()
}

#[test]
fn an_input_view_follows_the_pc_editing_keys() {
    let mut host = host(Name, Some(KeyConvention::Pc));
    let node = host.find("Name").expect("the input");
    assert!(host.perform(node, SemanticAction::Focus, None));
    chord(&mut host, CTRL, Key::ArrowLeft);
    host.text("big ");
    host.layout();
    assert_eq!(value(&mut host), "hello big world");
    chord(&mut host, ShortcutModifiers::default(), Key::Home);
    host.text(">");
    host.layout();
    assert_eq!(value(&mut host), ">hello big world");
    chord(&mut host, CTRL, Key::Character('z'));
    assert_eq!(value(&mut host), "hello big world");
    chord(&mut host, CTRL, Key::Character('y'));
    assert_eq!(value(&mut host), ">hello big world");
}
