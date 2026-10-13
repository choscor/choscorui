//! Clipboard commands, multiple clicks, and input method composition in
//! the hosted text controls.

mod hosted;

use choscorui_controls::{
    ComboItem, Combobox, ControlMessage, ControlValue, DecimalSpinBox, EditCommand, Input,
    InputOtp, IntegerRange, ShortcutModifiers, SpinBox, TextEditor, Textarea,
};
use choscorui_core::{CompositionEvent, DisplayCommand, DisplayList, ItemKey, Key, Locale, Point};
use hosted::{FakeClipboard, Harness};

const COMMAND: ShortcutModifiers = ShortcutModifiers {
    shift: false,
    control: false,
    option: false,
    command: true,
};

fn command_shift() -> ShortcutModifiers {
    ShortcutModifiers {
        shift: true,
        ..COMMAND
    }
}

fn value(h: &Harness) -> String {
    h.semantics().value.unwrap_or_default()
}

/// The start of the first text run that `list` draws.
fn text_start(list: &DisplayList) -> Point {
    list.commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawText { baseline, .. } => Some(*baseline),
            _ => None,
        })
        .expect("a text run")
}

/// A point on the first grapheme of the painted text.
fn first_grapheme(h: &Harness) -> Point {
    let start = text_start(&h.paint());
    Point::new(start.x + 1.0, start.y - 2.0)
}

#[test]
fn command_c_x_and_v_copy_cut_and_paste_in_an_input() {
    let clipboard = FakeClipboard::with("");
    let mut h = Harness::new(Input::new("Name", "hello world"));
    h.clipboard(&clipboard);
    h.focus();
    h.chord(COMMAND, Key::Character('a'));
    h.chord(COMMAND, Key::Character('c'));
    assert_eq!(clipboard.get().as_deref(), Some("hello world"));
    assert_eq!(value(&h), "hello world", "copy keeps the text");

    *clipboard.text.borrow_mut() = None;
    h.chord(COMMAND, Key::Character('x'));
    assert_eq!(clipboard.get().as_deref(), Some("hello world"));
    assert_eq!(value(&h), "");
    h.chord(COMMAND, Key::Character('v'));
    h.chord(COMMAND, Key::Character('v'));
    assert_eq!(value(&h), "hello worldhello world");
    h.chord(COMMAND, Key::Character('z'));
    h.chord(COMMAND, Key::Character('z'));
    assert_eq!(value(&h), "", "each paste is one undo step");
    h.chord(COMMAND, Key::Character('z'));
    assert_eq!(value(&h), "hello world", "the cut is one undo step");
    h.chord(command_shift(), Key::Character('z'));
    assert_eq!(value(&h), "");
    let edits = h
        .messages()
        .into_iter()
        .filter(|message| matches!(message, ControlMessage::Changed(_)))
        .count();
    assert_eq!(edits, 7, "each edit reports the new text");
}

#[test]
fn the_edit_menu_commands_reach_the_focused_input() {
    let clipboard = FakeClipboard::with("pasted");
    let mut h = Harness::new(Input::new("Name", ""));
    h.clipboard(&clipboard);
    h.focus();
    assert!(h.host.dispatch_edit(&mut h.tree, EditCommand::Paste));
    assert_eq!(value(&h), "pasted");
    assert!(h.host.dispatch_edit(&mut h.tree, EditCommand::SelectAll));
    assert!(h.host.dispatch_edit(&mut h.tree, EditCommand::Cut));
    assert_eq!(value(&h), "");
    h.host.move_focus(&mut h.tree, None);
    assert!(
        !h.host.dispatch_edit(&mut h.tree, EditCommand::Paste),
        "nothing has focus"
    );
}

#[test]
fn a_password_input_blocks_copy_and_cut_but_takes_a_paste() {
    let clipboard = FakeClipboard::with("old");
    let mut h = Harness::new(Input::password("Password", "secret"));
    h.clipboard(&clipboard);
    h.focus();
    h.chord(COMMAND, Key::Character('a'));
    h.chord(COMMAND, Key::Character('c'));
    h.chord(COMMAND, Key::Character('x'));
    assert_eq!(clipboard.get().as_deref(), Some("old"));
    assert_eq!(
        h.host.get(h.node).unwrap().semantics().value.as_deref(),
        Some("••••••")
    );
    h.chord(COMMAND, Key::Character('v'));
    assert_eq!(h.semantics().value.as_deref(), Some("•••"));
}

#[test]
fn without_a_clipboard_cut_keeps_the_text() {
    let mut h = Harness::new(Input::new("Name", "keep"));
    h.focus();
    h.chord(COMMAND, Key::Character('a'));
    h.chord(COMMAND, Key::Character('x'));
    h.chord(COMMAND, Key::Character('v'));
    assert_eq!(value(&h), "keep");
    assert!(h.messages().is_empty());
}

#[test]
fn a_textarea_and_a_text_editor_copy_cut_and_paste() {
    for mut h in [
        Harness::new(Textarea::new("Notes", "line one\nline two")),
        Harness::new(TextEditor::new("Notes", "line one\nline two")),
    ] {
        let clipboard = FakeClipboard::with("");
        h.clipboard(&clipboard);
        h.focus();
        h.chord(COMMAND, Key::Character('a'));
        h.chord(COMMAND, Key::Character('x'));
        assert_eq!(clipboard.get().as_deref(), Some("line one\nline two"));
        assert_eq!(value(&h), "");
        h.chord(COMMAND, Key::Character('v'));
        h.chord(COMMAND, Key::Character('v'));
        assert_eq!(value(&h), "line one\nline twoline one\nline two");
        h.chord(COMMAND, Key::ArrowUp);
        h.chord(command_shift(), Key::ArrowRight);
        h.chord(COMMAND, Key::Character('c'));
        assert_eq!(clipboard.get().as_deref(), Some("line one"));
    }
}

#[test]
fn a_read_only_text_editor_copies_and_does_not_cut_or_paste() {
    let clipboard = FakeClipboard::with("new");
    let mut editor = TextEditor::new("Log", "fixed");
    editor.set_read_only(true);
    let mut h = Harness::new(editor);
    h.clipboard(&clipboard);
    h.focus();
    h.chord(COMMAND, Key::Character('v'));
    h.chord(COMMAND, Key::Character('a'));
    h.chord(COMMAND, Key::Character('x'));
    assert_eq!(value(&h), "fixed");
    h.chord(COMMAND, Key::Character('c'));
    assert_eq!(clipboard.get().as_deref(), Some("fixed"));
}

#[test]
fn an_editable_combobox_copies_cuts_and_pastes_its_text() {
    let mut combo = Combobox::new("Fruit", true);
    combo.set_items(vec![ComboItem {
        key: ItemKey(1),
        text: "Banana".into(),
    }]);
    let clipboard = FakeClipboard::with("Ban");
    let mut h = Harness::new(combo);
    h.clipboard(&clipboard);
    h.focus();
    h.chord(COMMAND, Key::Character('v'));
    assert_eq!(value(&h), "Ban");
    *clipboard.text.borrow_mut() = None;
    h.chord(COMMAND, Key::Character('x'));
    assert_eq!(
        clipboard.get(),
        None,
        "Cut with no selection copies nothing"
    );
    h.chord(COMMAND, Key::Character('a'));
    h.chord(COMMAND, Key::Character('x'));
    assert_eq!(clipboard.get().as_deref(), Some("Ban"));
    assert_eq!(value(&h), "");
}

#[test]
fn value_fields_copy_their_value_and_take_a_paste() {
    let clipboard = FakeClipboard::with("12ab34");
    let mut h = Harness::new(InputOtp::new("Code", 6));
    h.clipboard(&clipboard);
    h.focus();
    h.text("9");
    h.chord(COMMAND, Key::Character('v'));
    assert_eq!(value(&h), "1234", "a paste fills the slots from the first");

    let mut h = Harness::new(SpinBox::new(
        "Count",
        IntegerRange::new(0, 100, 5, 1, 10).unwrap(),
    ));
    h.clipboard(&clipboard);
    h.focus();
    h.chord(COMMAND, Key::Character('c'));
    assert_eq!(
        clipboard.get().as_deref(),
        Some("5"),
        "focus selects the text"
    );
    *clipboard.text.borrow_mut() = Some("42".into());
    h.chord(COMMAND, Key::Character('a'));
    h.chord(COMMAND, Key::Character('v'));
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Number(42.0))]
    );
    *clipboard.text.borrow_mut() = Some("many".into());
    h.chord(COMMAND, Key::Character('a'));
    h.chord(COMMAND, Key::Character('v'));
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("42"),
        "the filter rejects a paste that is no number"
    );
    h.chord(COMMAND, Key::Character('x'));
    assert_eq!(
        clipboard.get().as_deref(),
        Some("42"),
        "cut copies the text"
    );

    let spin = DecimalSpinBox::new("Price", Locale::root(), (0.0, 10.0), 0.5, 2, 1.5).unwrap();
    let mut h = Harness::new(spin);
    h.clipboard(&clipboard);
    h.focus();
    h.chord(COMMAND, Key::Character('c'));
    assert_eq!(clipboard.get().as_deref(), Some("1.50"));
    *clipboard.text.borrow_mut() = Some("2.5".into());
    h.chord(COMMAND, Key::Character('v'));
    h.key(Key::Enter);
    assert_eq!(
        h.host.get(h.node).unwrap().semantics().value.as_deref(),
        Some("2.50")
    );
}

#[test]
fn double_and_triple_clicks_select_a_word_and_the_line_in_an_input() {
    let clipboard = FakeClipboard::with("");
    let mut h = Harness::new(Input::new("Name", "hello world"));
    h.clipboard(&clipboard);
    h.focus();
    let at = first_grapheme(&h);
    h.double_click(at);
    h.chord(COMMAND, Key::Character('c'));
    assert_eq!(clipboard.get().as_deref(), Some("hello"));
    h.triple_click(at);
    h.chord(COMMAND, Key::Character('c'));
    assert_eq!(clipboard.get().as_deref(), Some("hello world"));
}

#[test]
fn double_and_triple_clicks_select_a_word_and_a_paragraph_in_multiline_text() {
    for mut h in [
        Harness::new(Textarea::new("Notes", "first words\nsecond line")),
        Harness::new(TextEditor::new("Notes", "first words\nsecond line")),
    ] {
        let clipboard = FakeClipboard::with("");
        h.clipboard(&clipboard);
        h.focus();
        let at = first_grapheme(&h);
        h.double_click(at);
        h.chord(COMMAND, Key::Character('c'));
        assert_eq!(clipboard.get().as_deref(), Some("first"));
        h.triple_click(at);
        h.chord(COMMAND, Key::Character('c'));
        assert_eq!(clipboard.get().as_deref(), Some("first words"));
    }
}

#[test]
fn an_input_shows_marked_text_and_commits_it() {
    let mut h = Harness::new(Input::new("Name", "Xin "));
    h.focus();
    let mark = |text: &str| CompositionEvent::Mark {
        text: text.into(),
        selected_utf16: text.encode_utf16().count()..text.encode_utf16().count(),
        replacement_utf16: None,
    };
    assert!(h.host.dispatch_composition(&mut h.tree, mark("cha")));
    assert!(h.host.dispatch_composition(&mut h.tree, mark("chà")));
    let painted = hosted::texts(&h.paint());
    assert_eq!(painted, ["Xin chà"], "the marked text shows in place");
    assert_eq!(value(&h), "Xin ", "marked text is not the value");
    let input = h.host.text_input(&h.tree).expect("an editing control");
    assert_eq!(input.marked_utf16, Some(4..7));
    h.key(Key::Backspace);
    assert_eq!(value(&h), "Xin ", "the input method owns the keys");
    let commit = CompositionEvent::Commit {
        text: "chào".into(),
        replacement_utf16: None,
    };
    assert!(h.host.dispatch_composition(&mut h.tree, commit));
    assert_eq!(value(&h), "Xin chào");
    assert_eq!(h.host.text_input(&h.tree).unwrap().selected_utf16, 8..8);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Text(
            "Xin chào".into()
        ))]
    );
}

#[test]
fn a_text_editor_unmarks_its_composition_on_blur() {
    let mut h = Harness::new(TextEditor::new("Notes", ""));
    h.focus();
    let mark = CompositionEvent::Mark {
        text: "にほん".into(),
        selected_utf16: 3..3,
        replacement_utf16: None,
    };
    assert!(h.host.dispatch_composition(&mut h.tree, mark));
    assert_eq!(value(&h), "");
    h.host.move_focus(&mut h.tree, None);
    assert_eq!(value(&h), "にほん", "a blur keeps the marked text");
}

#[test]
fn a_control_without_composition_gets_the_commit_as_text() {
    let mut h = Harness::new(InputOtp::new("Code", 3));
    h.focus();
    let commit = CompositionEvent::Commit {
        text: "12".into(),
        replacement_utf16: None,
    };
    assert!(h.host.dispatch_composition(&mut h.tree, commit));
    assert_eq!(value(&h), "12");
    assert!(h.host.text_input(&h.tree).is_none());
}
