//! The editing commands and the key maps that the text controls share.
//! Each [`KeyConvention`] has a key map. On `Mac`, Option moves and deletes
//! by word, and Command moves and deletes to the line or document edge.
//! On `Pc`, Ctrl moves and deletes by word, Home and End go to the line
//! edge, and Ctrl with Home or End goes to the document edge. Shift
//! extends the selection. The modifiers are the ones that
//! [`ShortcutModifiers::logical`] gives, so `command` is Ctrl on `Pc`.

use crate::ShortcutModifiers;
use crate::custom::ControlContext;
use choscorui_core::{Key, KeyConvention};

/// A command of the Edit menu. The control host sends it to the focused
/// control as [`ControlEvent::Edit`](crate::ControlEvent::Edit), from the
/// menu or from its Command-key chord.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditCommand {
    Cut,
    Copy,
    Paste,
    SelectAll,
    Undo,
    Redo,
}

impl EditCommand {
    /// The command of a Command-key chord: Command with X, C, V, or A,
    /// Command+Z for undo, and Shift+Command+Z for redo. On
    /// [`KeyConvention::Pc`], Command is Ctrl, and Ctrl+Y also redoes.
    /// Other chords give `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use choscorui_controls::{EditCommand, ShortcutModifiers};
    /// use choscorui_core::{Key, KeyConvention};
    ///
    /// let command = ShortcutModifiers { command: true, ..ShortcutModifiers::default() };
    /// let y = Key::Character('y');
    /// assert_eq!(EditCommand::from_shortcut(y, command, KeyConvention::Pc), Some(EditCommand::Redo));
    /// assert_eq!(EditCommand::from_shortcut(y, command, KeyConvention::Mac), None);
    /// ```
    pub fn from_shortcut(
        key: Key,
        modifiers: ShortcutModifiers,
        convention: KeyConvention,
    ) -> Option<Self> {
        if !modifiers.command || modifiers.control || modifiers.option {
            return None;
        }
        Some(match (key, modifiers.shift) {
            (Key::Character('x'), false) => Self::Cut,
            (Key::Character('c'), false) => Self::Copy,
            (Key::Character('v'), false) => Self::Paste,
            (Key::Character('a'), false) => Self::SelectAll,
            (Key::Character('z'), false) => Self::Undo,
            (Key::Character('z'), true) => Self::Redo,
            (Key::Character('y'), false) if convention == KeyConvention::Pc => Self::Redo,
            _ => return None,
        })
    }
}

/// Where a caret motion of an editing key goes. A single-line control
/// treats the line and the document as the same.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CaretMotion {
    Left,
    Right,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    Up,
    Down,
    PageUp,
    PageDown,
    DocumentStart,
    DocumentEnd,
}

/// What an editing key does in a text control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditingKey {
    /// Moves the caret, or extends the selection with `extend`.
    Move {
        motion: CaretMotion,
        extend: bool,
    },
    /// Deletes the selection, or the text from the caret to where the
    /// motion goes.
    Delete(CaretMotion),
    Enter,
    Tab,
}

/// The editing action of `key` with `modifiers` in the key map of
/// `convention`, or `None` for a key that the text control leaves to its
/// ancestors and to the shortcuts.
pub(crate) fn editing_key(
    key: Key,
    modifiers: ShortcutModifiers,
    convention: KeyConvention,
) -> Option<EditingKey> {
    if modifiers.control {
        return None;
    }
    match convention {
        KeyConvention::Mac => mac_editing_key(key, modifiers),
        KeyConvention::Pc => pc_editing_key(key, modifiers),
    }
}

/// The macOS key map: Option goes by word, and Command goes to the line
/// or the document edge.
fn mac_editing_key(key: Key, modifiers: ShortcutModifiers) -> Option<EditingKey> {
    let (command, option) = (modifiers.command, modifiers.option);
    let horizontal = |plain, word, line| match (command, option) {
        (true, _) => line,
        (false, true) => word,
        (false, false) => plain,
    };
    let motion = match key {
        Key::ArrowLeft => horizontal(
            CaretMotion::Left,
            CaretMotion::WordLeft,
            CaretMotion::LineStart,
        ),
        Key::ArrowRight => horizontal(
            CaretMotion::Right,
            CaretMotion::WordRight,
            CaretMotion::LineEnd,
        ),
        Key::ArrowUp if command => CaretMotion::DocumentStart,
        Key::ArrowDown if command => CaretMotion::DocumentEnd,
        Key::ArrowUp => CaretMotion::Up,
        Key::ArrowDown => CaretMotion::Down,
        Key::Home if option => CaretMotion::DocumentStart,
        Key::End if option => CaretMotion::DocumentEnd,
        Key::Home if !command => CaretMotion::LineStart,
        Key::End if !command => CaretMotion::LineEnd,
        Key::PageUp if !command => CaretMotion::PageUp,
        Key::PageDown if !command => CaretMotion::PageDown,
        Key::Backspace => {
            return Some(EditingKey::Delete(horizontal(
                CaretMotion::Left,
                CaretMotion::WordLeft,
                CaretMotion::LineStart,
            )));
        }
        Key::Delete => {
            return Some(EditingKey::Delete(horizontal(
                CaretMotion::Right,
                CaretMotion::WordRight,
                CaretMotion::LineEnd,
            )));
        }
        Key::Enter if !command => return Some(EditingKey::Enter),
        Key::Tab if !command && !option => return Some(EditingKey::Tab),
        _ => return None,
    };
    Some(EditingKey::Move {
        motion,
        extend: modifiers.shift,
    })
}

/// The PC key map: Ctrl (`command`) goes by word, Home and End go to the
/// line edge, and Ctrl with Home or End goes to the document edge. The
/// text control leaves Alt chords to the shortcuts.
fn pc_editing_key(key: Key, modifiers: ShortcutModifiers) -> Option<EditingKey> {
    if modifiers.option {
        return None;
    }
    let ctrl = modifiers.command;
    let by_word = |plain, word| if ctrl { word } else { plain };
    let motion = match key {
        Key::ArrowLeft => by_word(CaretMotion::Left, CaretMotion::WordLeft),
        Key::ArrowRight => by_word(CaretMotion::Right, CaretMotion::WordRight),
        Key::ArrowUp if !ctrl => CaretMotion::Up,
        Key::ArrowDown if !ctrl => CaretMotion::Down,
        Key::Home => by_word(CaretMotion::LineStart, CaretMotion::DocumentStart),
        Key::End => by_word(CaretMotion::LineEnd, CaretMotion::DocumentEnd),
        Key::PageUp if !ctrl => CaretMotion::PageUp,
        Key::PageDown if !ctrl => CaretMotion::PageDown,
        Key::Backspace => {
            return Some(EditingKey::Delete(by_word(
                CaretMotion::Left,
                CaretMotion::WordLeft,
            )));
        }
        Key::Delete => {
            return Some(EditingKey::Delete(by_word(
                CaretMotion::Right,
                CaretMotion::WordRight,
            )));
        }
        Key::Enter if !ctrl => return Some(EditingKey::Enter),
        Key::Tab if !ctrl => return Some(EditingKey::Tab),
        _ => return None,
    };
    Some(EditingKey::Move {
        motion,
        extend: modifiers.shift,
    })
}

/// True for a key that types text without a Command or Control chord.
/// The text arrives as committed text, so a text control stops the key,
/// and an ancestor such as a scroll view does not act on it.
pub(crate) fn is_typing_key(key: Key, modifiers: ShortcutModifiers) -> bool {
    !modifiers.command
        && !modifiers.control
        && matches!(key, Key::Character(_) | Key::Space | Key::Other(_))
}

/// The text change of an Edit command, after its clipboard part ran.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TextEdit {
    /// Delete the selection, which is on the clipboard now.
    Cut,
    /// Insert this text from the clipboard in place of the selection.
    Paste(String),
    SelectAll,
    Undo,
    Redo,
}

/// Runs the clipboard part of `command` for a text control whose
/// selection holds `selected`, or `None` for a password field, and returns
/// the change the control makes to its text. Copy and Cut of a password
/// field do nothing. Without a clipboard, Copy, Cut, and Paste do nothing,
/// so Cut never deletes text that it could not copy.
pub(crate) fn text_edit(
    cx: &mut ControlContext<'_>,
    command: EditCommand,
    selected: Option<&str>,
) -> Option<TextEdit> {
    match command {
        EditCommand::Copy => {
            let text = selected.filter(|text| !text.is_empty())?;
            cx.write_clipboard(text);
            None
        }
        EditCommand::Cut => {
            let text = selected.filter(|text| !text.is_empty())?;
            cx.write_clipboard(text).then_some(TextEdit::Cut)
        }
        EditCommand::Paste => cx
            .clipboard_text()
            .filter(|text| !text.is_empty())
            .map(TextEdit::Paste),
        EditCommand::SelectAll => Some(TextEdit::SelectAll),
        EditCommand::Undo => Some(TextEdit::Undo),
        EditCommand::Redo => Some(TextEdit::Redo),
    }
}

/// The value change of an Edit command in a field that has no text
/// selection, such as a spin box or a code input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ValueEdit {
    /// Clear the value, which is on the clipboard now.
    Cut,
    /// Take this clipboard text as typed text.
    Paste(String),
}

/// Runs the clipboard part of `command` for a field without a text
/// selection: Copy and Cut take the whole `value`. Cut gives
/// [`ValueEdit::Cut`] only when `clearable` is true and the copy
/// succeeded; otherwise it copies only.
pub(crate) fn value_edit(
    cx: &mut ControlContext<'_>,
    command: EditCommand,
    value: &str,
    clearable: bool,
) -> Option<ValueEdit> {
    match command {
        EditCommand::Copy | EditCommand::Cut if !value.is_empty() => {
            let copied = cx.write_clipboard(value);
            (copied && clearable && command == EditCommand::Cut).then_some(ValueEdit::Cut)
        }
        EditCommand::Paste => cx
            .clipboard_text()
            .filter(|text| !text.is_empty())
            .map(ValueEdit::Paste),
        _ => None,
    }
}
