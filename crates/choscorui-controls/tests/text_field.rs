use choscorui_controls::{
    TextField, TextFieldChange, TextFieldInput, TextFieldSemanticAction, TextFieldStyle,
    ValidationKind, ValidationState,
};
use choscorui_core::{
    Color, Dirty, DisplayCommand, DisplayList, Key, KeyEvent, Point, Rect, Size, Tree,
};
use choscorui_core::{SemanticAction as CoreSemanticAction, SemanticRole};

fn setup() -> (Tree, choscorui_core::NodeId, TextField) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(240.0, 80.0)));
    let id = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(10.0, 10.0), Size::new(180.0, 32.0)),
        )
        .unwrap();
    tree.set_focusable(id, true).unwrap();
    (tree, id, TextField::new("Name", ""))
}

#[test]
fn inserted_text_and_backspace_edit_whole_graphemes_and_invalidate() {
    let (mut tree, id, mut field) = setup();
    tree.set_focus(Some(id)).unwrap();
    tree.take_dirty(id).unwrap();
    tree.take_dirty(tree.root()).unwrap();
    assert_eq!(
        field
            .handle(&mut tree, id, TextFieldInput::Text("e\u{301}👩‍💻".into()))
            .unwrap(),
        TextFieldChange::Edited
    );
    assert_eq!(field.value(), "e\u{301}👩‍💻");
    assert_eq!(
        field
            .handle(
                &mut tree,
                id,
                TextFieldInput::Key {
                    event: KeyEvent::Down {
                        key: Key::Backspace,
                        repeat: false
                    },
                    extend: false,
                    word: false
                }
            )
            .unwrap(),
        TextFieldChange::Edited
    );
    assert_eq!(field.value(), "e\u{301}");
    let dirty = tree.take_dirty(tree.root()).unwrap();
    assert!(dirty.contains(Dirty::PAINT));
    assert!(dirty.contains(Dirty::SEMANTICS));
    assert!(!dirty.contains(Dirty::LAYOUT));
}

#[test]
fn keyboard_navigation_commit_and_undo_are_separate_events() {
    let (mut tree, id, mut field) = setup();
    tree.set_focus(Some(id)).unwrap();
    field
        .handle(&mut tree, id, TextFieldInput::SetValue("A👩‍💻B".into()))
        .unwrap();
    field
        .handle(
            &mut tree,
            id,
            TextFieldInput::Key {
                event: KeyEvent::Down {
                    key: Key::Home,
                    repeat: false,
                },
                extend: false,
                word: false,
            },
        )
        .unwrap();
    field
        .handle(
            &mut tree,
            id,
            TextFieldInput::Key {
                event: KeyEvent::Down {
                    key: Key::ArrowRight,
                    repeat: false,
                },
                extend: true,
                word: false,
            },
        )
        .unwrap();
    assert_eq!(field.selection(), 0..1);
    assert_eq!(
        field
            .handle(&mut tree, id, TextFieldInput::Text("Z".into()))
            .unwrap(),
        TextFieldChange::Edited
    );
    assert_eq!(field.value(), "Z👩‍💻B");
    assert_eq!(
        field.handle(&mut tree, id, TextFieldInput::Undo).unwrap(),
        TextFieldChange::Edited
    );
    assert_eq!(field.value(), "A👩‍💻B");
    assert_eq!(
        field
            .handle(
                &mut tree,
                id,
                TextFieldInput::Key {
                    event: KeyEvent::Down {
                        key: Key::Enter,
                        repeat: false
                    },
                    extend: false,
                    word: false
                }
            )
            .unwrap(),
        TextFieldChange::Committed
    );
}

#[test]
fn semantic_value_setting_is_focus_independent_and_disabled_state_blocks_input() {
    let (mut tree, id, mut field) = setup();
    assert_eq!(
        field
            .handle(&mut tree, id, TextFieldInput::SetValue("Xin chào".into()))
            .unwrap(),
        TextFieldChange::Edited
    );
    let semantics = field.semantics(&tree, id);
    assert_eq!(semantics.role, "text field");
    assert_eq!(semantics.name, "Name");
    assert_eq!(semantics.value, "Xin chào");
    assert_eq!(semantics.actions, &[TextFieldSemanticAction::SetValue]);
    let shared = semantics.shared();
    assert_eq!(shared.role, SemanticRole::TextField);
    assert_eq!(shared.value.as_deref(), Some("Xin chào"));
    assert_eq!(
        shared.actions,
        vec![CoreSemanticAction::SetValue, CoreSemanticAction::Focus]
    );
    field.set_enabled(&mut tree, id, false).unwrap();
    tree.set_focus(Some(id)).unwrap();
    assert_eq!(
        field
            .handle(&mut tree, id, TextFieldInput::Text("!".into()))
            .unwrap(),
        TextFieldChange::None
    );
    assert_eq!(field.value(), "Xin chào");
    assert!(field.semantics(&tree, id).actions.is_empty());
}

#[test]
fn single_line_text_normalizes_line_breaks_and_delete_is_grapheme_safe() {
    let (mut tree, id, mut field) = setup();
    tree.set_focus(Some(id)).unwrap();
    field
        .handle(&mut tree, id, TextFieldInput::Text("A\r\n👩‍💻B".into()))
        .unwrap();
    assert_eq!(field.value(), "A👩‍💻B");
    field
        .handle(
            &mut tree,
            id,
            TextFieldInput::Key {
                event: KeyEvent::Down {
                    key: Key::Home,
                    repeat: false,
                },
                extend: false,
                word: false,
            },
        )
        .unwrap();
    field
        .handle(
            &mut tree,
            id,
            TextFieldInput::Key {
                event: KeyEvent::Down {
                    key: Key::ArrowRight,
                    repeat: false,
                },
                extend: false,
                word: false,
            },
        )
        .unwrap();
    assert_eq!(
        field
            .handle(
                &mut tree,
                id,
                TextFieldInput::Key {
                    event: KeyEvent::Down {
                        key: Key::Delete,
                        repeat: false
                    },
                    extend: false,
                    word: false
                }
            )
            .unwrap(),
        TextFieldChange::Edited
    );
    assert_eq!(field.value(), "AB");
}

#[test]
fn visual_recipe_is_swappable_without_changing_text_behavior() {
    let (mut tree, id, mut field) = setup();
    tree.set_focus(Some(id)).unwrap();
    let first = TextFieldStyle {
        background: Color::rgba8(10, 20, 30, 255),
        disabled: Color::rgba8(60, 60, 60, 255),
        focus: Color::rgba8(255, 255, 255, 255),
        intermediate: Color::rgba8(240, 180, 50, 255),
        invalid: Color::rgba8(240, 60, 70, 255),
    };
    let second = TextFieldStyle {
        background: Color::rgba8(100, 110, 120, 255),
        ..first
    };
    let mut before = DisplayList::new();
    first.paint(
        &mut before,
        tree.bounds(id).unwrap(),
        field.visual_state(&tree, id),
    );
    let mut after = DisplayList::new();
    second.paint(
        &mut after,
        tree.bounds(id).unwrap(),
        field.visual_state(&tree, id),
    );
    assert_ne!(before.commands(), after.commands());
    assert!(
        matches!(after.commands()[0], DisplayCommand::FillRect { color, .. } if color == second.background)
    );
    field
        .handle(&mut tree, id, TextFieldInput::Text("é".into()))
        .unwrap();
    assert_eq!(field.value(), "é");
}

#[test]
fn setting_the_same_semantic_value_is_idempotent() {
    let (mut tree, id, mut field) = setup();
    field
        .handle(&mut tree, id, TextFieldInput::SetValue("same".into()))
        .unwrap();
    tree.take_dirty(id).unwrap();
    tree.take_dirty(tree.root()).unwrap();
    assert_eq!(
        field
            .handle(&mut tree, id, TextFieldInput::SetValue("same".into()))
            .unwrap(),
        TextFieldChange::None
    );
    assert_eq!(tree.take_dirty(tree.root()).unwrap(), Dirty::NONE);
    tree.set_focus(Some(id)).unwrap();
    assert_eq!(
        field.handle(&mut tree, id, TextFieldInput::Undo).unwrap(),
        TextFieldChange::Edited
    );
    assert_eq!(field.value(), "");
}

#[test]
fn ime_marked_text_is_provisional_and_commit_is_one_undoable_edit() {
    let (mut tree, id, _) = setup();
    let mut field = TextField::new("Greeting", "Xin ");
    tree.set_focus(Some(id)).unwrap();

    assert_eq!(
        field
            .handle(
                &mut tree,
                id,
                TextFieldInput::SetMarkedText {
                    text: "cha".into(),
                    selected_utf16: 3..3,
                    replacement_utf16: None,
                }
            )
            .unwrap(),
        TextFieldChange::Composition
    );
    assert_eq!(field.value(), "Xin ");
    assert_eq!(field.composition().unwrap().text, "cha");

    field
        .handle(
            &mut tree,
            id,
            TextFieldInput::SetMarkedText {
                text: "chào".into(),
                selected_utf16: 4..4,
                replacement_utf16: None,
            },
        )
        .unwrap();
    assert_eq!(field.value(), "Xin ");
    assert_eq!(field.composition().unwrap().text, "chào");

    assert_eq!(
        field
            .handle(
                &mut tree,
                id,
                TextFieldInput::CommitMarkedText {
                    text: "chào".into(),
                    replacement_utf16: None,
                }
            )
            .unwrap(),
        TextFieldChange::Edited
    );
    assert_eq!(field.value(), "Xin chào");
    assert!(field.composition().is_none());
    field.handle(&mut tree, id, TextFieldInput::Undo).unwrap();
    assert_eq!(field.value(), "Xin ");
}

#[test]
fn ime_cancel_discards_marked_text_without_an_edit() {
    let (mut tree, id, _) = setup();
    let mut field = TextField::new("Name", "A");
    tree.set_focus(Some(id)).unwrap();
    field
        .handle(
            &mut tree,
            id,
            TextFieldInput::SetMarkedText {
                text: "候補".into(),
                selected_utf16: 0..2,
                replacement_utf16: None,
            },
        )
        .unwrap();
    assert_eq!(
        field
            .handle(&mut tree, id, TextFieldInput::CancelMarkedText)
            .unwrap(),
        TextFieldChange::Composition
    );
    assert_eq!(field.value(), "A");
    assert!(field.composition().is_none());
}

#[test]
fn ime_utf16_replacement_range_replaces_whole_emoji_cluster() {
    let (mut tree, id, _) = setup();
    let mut field = TextField::new("Value", "A😀B");
    tree.set_focus(Some(id)).unwrap();
    field
        .handle(
            &mut tree,
            id,
            TextFieldInput::SetMarkedText {
                text: "候補".into(),
                selected_utf16: 2..2,
                replacement_utf16: Some(1..3),
            },
        )
        .unwrap();
    assert_eq!(field.display_value(), "A候補B");
    field
        .handle(
            &mut tree,
            id,
            TextFieldInput::CommitMarkedText {
                text: "仮".into(),
                replacement_utf16: Some(1..3),
            },
        )
        .unwrap();
    assert_eq!(field.value(), "A仮B");
    field.handle(&mut tree, id, TextFieldInput::Undo).unwrap();
    assert_eq!(field.value(), "A😀B");
}

#[test]
fn ime_unmark_finalizes_the_current_marked_text() {
    let (mut tree, id, _) = setup();
    let mut field = TextField::new("Name", "");
    tree.set_focus(Some(id)).unwrap();
    field
        .handle(
            &mut tree,
            id,
            TextFieldInput::SetMarkedText {
                text: "中文".into(),
                selected_utf16: 2..2,
                replacement_utf16: None,
            },
        )
        .unwrap();
    assert_eq!(
        field
            .handle(&mut tree, id, TextFieldInput::UnmarkMarkedText)
            .unwrap(),
        TextFieldChange::Edited
    );
    assert_eq!(field.value(), "中文");
    assert!(field.composition().is_none());
}

#[test]
fn password_field_masks_rendering_and_semantics_and_blocks_copy_cut() {
    let (mut tree, id, _) = setup();
    let mut field = TextField::password("Password", "a👩‍💻b");
    tree.set_focus(Some(id)).unwrap();
    field
        .handle(&mut tree, id, TextFieldInput::SelectAll)
        .unwrap();

    assert_eq!(field.value(), "a👩‍💻b");
    assert_eq!(field.display_value(), "•••");
    assert_eq!(field.selected_text(), "");
    let semantics = field.semantics(&tree, id).shared();
    assert_eq!(semantics.role, SemanticRole::PasswordField);
    assert_eq!(semantics.value.as_deref(), Some("•••"));

    assert_eq!(
        field.handle(&mut tree, id, TextFieldInput::Cut).unwrap(),
        TextFieldChange::None
    );
    assert_eq!(field.value(), "a👩‍💻b");
}

#[test]
fn layout_hit_test_can_place_or_extend_the_caret() {
    let (mut tree, id, mut field) = setup();
    tree.set_focus(Some(id)).unwrap();
    field
        .handle(&mut tree, id, TextFieldInput::SetValue("abcd".into()))
        .unwrap();
    assert_eq!(
        field
            .handle(
                &mut tree,
                id,
                TextFieldInput::SetCaret {
                    caret: 1,
                    extend: false
                }
            )
            .unwrap(),
        TextFieldChange::Selection
    );
    assert_eq!(field.selection(), 1..1);
    field
        .handle(
            &mut tree,
            id,
            TextFieldInput::SetCaret {
                caret: 3,
                extend: true,
            },
        )
        .unwrap();
    assert_eq!(field.selection(), 1..3);
    assert_eq!(
        field
            .handle(
                &mut tree,
                id,
                TextFieldInput::SetCaret {
                    caret: 9,
                    extend: false
                }
            )
            .unwrap(),
        TextFieldChange::None
    );
    assert_eq!(field.selection(), 1..3);
}

#[test]
fn validation_state_is_separate_and_only_valid_value_commits() {
    let (mut tree, id, mut field) = setup();
    tree.set_focus(Some(id)).unwrap();
    assert_eq!(
        field
            .handle(
                &mut tree,
                id,
                TextFieldInput::SetValidation(ValidationState::Intermediate)
            )
            .unwrap(),
        TextFieldChange::ValidationChanged
    );
    assert_eq!(
        field
            .handle(
                &mut tree,
                id,
                TextFieldInput::Key {
                    event: KeyEvent::Down {
                        key: Key::Enter,
                        repeat: false
                    },
                    extend: false,
                    word: false
                }
            )
            .unwrap(),
        TextFieldChange::CommitRejected
    );
    assert_eq!(
        field
            .handle(
                &mut tree,
                id,
                TextFieldInput::SetValidation(ValidationState::Invalid("Name is required".into()))
            )
            .unwrap(),
        TextFieldChange::ValidationChanged
    );
    let semantics = field.semantics(&tree, id);
    assert_eq!(semantics.validation, ValidationKind::Invalid);
    assert_eq!(semantics.announcement.as_deref(), Some("Name is required"));
    assert_eq!(
        field.visual_state(&tree, id).validation,
        ValidationKind::Invalid
    );
    let style = TextFieldStyle {
        background: Color::rgba8(10, 20, 30, 255),
        disabled: Color::rgba8(60, 60, 60, 255),
        focus: Color::rgba8(255, 255, 255, 255),
        intermediate: Color::rgba8(240, 180, 50, 255),
        invalid: Color::rgba8(240, 60, 70, 255),
    };
    let mut list = DisplayList::new();
    style.paint(
        &mut list,
        tree.bounds(id).unwrap(),
        field.visual_state(&tree, id),
    );
    assert!(
        matches!(list.commands().last().unwrap(), DisplayCommand::FillRect { color, .. } if *color == style.invalid)
    );
    assert_eq!(
        field
            .handle(
                &mut tree,
                id,
                TextFieldInput::SetValidation(ValidationState::Valid)
            )
            .unwrap(),
        TextFieldChange::ValidationChanged
    );
    assert_eq!(
        field
            .handle(
                &mut tree,
                id,
                TextFieldInput::Key {
                    event: KeyEvent::Down {
                        key: Key::Enter,
                        repeat: false
                    },
                    extend: false,
                    word: false
                }
            )
            .unwrap(),
        TextFieldChange::Committed
    );
}

#[test]
fn select_all_and_cut_expose_clipboard_command_seams() {
    let (mut tree, id, mut field) = setup();
    tree.set_focus(Some(id)).unwrap();
    field
        .handle(
            &mut tree,
            id,
            TextFieldInput::SetValue("Xin chào 👩‍💻".into()),
        )
        .unwrap();
    assert_eq!(
        field
            .handle(&mut tree, id, TextFieldInput::SelectAll)
            .unwrap(),
        TextFieldChange::Selection
    );
    assert_eq!(field.selected_text(), "Xin chào 👩‍💻");
    assert_eq!(
        field.handle(&mut tree, id, TextFieldInput::Cut).unwrap(),
        TextFieldChange::Edited
    );
    assert_eq!(field.value(), "");
    assert_eq!(
        field.handle(&mut tree, id, TextFieldInput::Undo).unwrap(),
        TextFieldChange::Edited
    );
    assert_eq!(field.value(), "Xin chào 👩‍💻");
}
