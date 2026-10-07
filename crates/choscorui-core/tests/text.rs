use choscorui_core::{TextBuffer, TextError, TextMotion};

#[test]
fn maps_graphemes_bytes_and_utf16_across_combining_and_joined_sequences() {
    let text = TextBuffer::new("A🇻🇳e\u{301}👩‍💻");
    assert_eq!(text.grapheme_count(), 4);
    assert_eq!(text.grapheme_to_byte(0), Some(0));
    assert_eq!(text.grapheme_to_byte(1), Some(1));
    assert_eq!(text.grapheme_to_byte(2), Some(9));
    assert_eq!(text.grapheme_to_byte(3), Some(12));
    assert_eq!(text.grapheme_to_byte(4), Some(23));
    assert_eq!(text.grapheme_to_byte(5), None);
    assert_eq!(text.byte_to_grapheme(9), Some(2));
    assert_eq!(text.byte_to_grapheme(2), None);
    assert_eq!(text.byte_to_utf16(9), Some(5));
    assert_eq!(text.byte_to_utf16(23), Some(12));
    assert_eq!(text.utf16_to_byte(5), Some(9));
    assert_eq!(text.utf16_to_byte(2), None);
    assert_eq!(text.utf16_to_byte(12), Some(23));
}

#[test]
fn empty_text_has_one_valid_caret_boundary() {
    let text = TextBuffer::new("");
    assert_eq!(text.grapheme_count(), 0);
    assert_eq!(text.grapheme_to_byte(0), Some(0));
    assert_eq!(text.byte_to_utf16(0), Some(0));
    assert_eq!(text.utf16_to_byte(0), Some(0));
}

#[test]
fn selection_replacement_and_backspace_keep_whole_graphemes() {
    let mut text = TextBuffer::new("A👩‍💻B");
    text.set_selection(1, 2).unwrap();
    assert_eq!(text.selection(), 1..2);
    text.replace_selection("e\u{301}");
    assert_eq!(text.as_str(), "Ae\u{301}B");
    assert_eq!(text.caret(), 2);
    text.delete_backward();
    assert_eq!(text.as_str(), "AB");
    assert_eq!(text.caret(), 1);
    text.delete_backward();
    assert_eq!(text.as_str(), "B");
    assert_eq!(text.caret(), 0);
}

#[test]
fn invalid_selection_does_not_change_text_or_caret() {
    let mut text = TextBuffer::new("hi");
    assert_eq!(text.set_selection(0, 3), Err(TextError::InvalidSelection));
    assert_eq!(text.as_str(), "hi");
    assert_eq!(text.caret(), 0);
}

#[test]
fn undo_redo_restore_text_and_selection_then_branch_new_history() {
    let mut text = TextBuffer::new("A👩‍💻B");
    text.set_selection(1, 2).unwrap();
    text.replace_selection("é");
    assert_eq!(text.as_str(), "AéB");
    assert_eq!(text.selection(), 2..2);
    assert!(text.undo());
    assert_eq!(text.as_str(), "A👩‍💻B");
    assert_eq!(text.selection(), 1..2);
    assert!(text.redo());
    assert_eq!(text.as_str(), "AéB");
    assert_eq!(text.selection(), 2..2);
    assert!(text.undo());
    text.replace_selection("x");
    assert_eq!(text.as_str(), "AxB");
    assert!(!text.redo());
}

#[test]
fn grapheme_and_word_motion_extend_and_collapse_selection() {
    let mut text = TextBuffer::new("A thế giới!");
    assert_eq!(text.move_caret(TextMotion::WordRight, false), 1);
    assert_eq!(text.move_caret(TextMotion::WordRight, false), 5);
    assert_eq!(text.move_caret(TextMotion::WordRight, true), 10);
    assert_eq!(text.selection(), 5..10);
    assert_eq!(text.move_caret(TextMotion::Left, false), 5);
    assert_eq!(text.selection(), 5..5);
    assert_eq!(text.move_caret(TextMotion::WordLeft, false), 2);
    assert_eq!(text.move_caret(TextMotion::Start, true), 0);
    assert_eq!(text.selection(), 0..2);
    assert_eq!(text.move_caret(TextMotion::End, false), 11);
}

#[test]
fn undo_backspace_restores_original_collapsed_caret() {
    let mut text = TextBuffer::new("A👩‍💻B");
    text.set_selection(2, 2).unwrap();
    text.delete_backward();
    assert_eq!(text.as_str(), "AB");
    assert!(text.undo());
    assert_eq!(text.as_str(), "A👩‍💻B");
    assert_eq!(text.selection(), 2..2);
}

#[test]
fn copy_cut_paste_and_mask_preserve_grapheme_boundaries() {
    let mut text = TextBuffer::new("a👩‍💻b");
    assert_eq!(text.password_display(), "•••");
    text.set_selection(1, 2).unwrap();
    assert_eq!(text.selected_text(), "👩‍💻");
    let copied = text.cut_selection();
    assert_eq!(copied, "👩‍💻");
    assert_eq!(text.as_str(), "ab");
    text.paste(&copied);
    assert_eq!(text.as_str(), "a👩‍💻b");
    assert_eq!(text.caret(), 2);
    assert_eq!(text.password_display(), "•••");
}

#[test]
fn setting_visual_caret_can_preserve_or_replace_selection_anchor() {
    let mut text = TextBuffer::new("abcd");
    text.set_selection(1, 3).unwrap();
    text.set_caret(4, true).unwrap();
    assert_eq!(text.selection(), 1..4);
    text.set_caret(2, false).unwrap();
    assert_eq!(text.selection(), 2..2);
    assert_eq!(text.set_caret(5, true), Err(TextError::InvalidSelection));
    assert_eq!(text.selection(), 2..2);
}

#[test]
fn forward_delete_removes_one_whole_grapheme_and_undo_restores_caret() {
    let mut text = TextBuffer::new("A👩‍💻B");
    text.set_selection(1, 1).unwrap();
    text.delete_forward();
    assert_eq!(text.as_str(), "AB");
    assert_eq!(text.caret(), 1);
    assert!(text.undo());
    assert_eq!(text.as_str(), "A👩‍💻B");
    assert_eq!(text.selection(), 1..1);
}
