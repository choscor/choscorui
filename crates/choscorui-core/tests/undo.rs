use choscorui_core::UndoStack;

/// Each entry is (index, old, new) applied to a Vec<i32> document.
type Edit = (usize, i32, i32);

fn apply_undo(doc: &mut [i32], stack: &mut UndoStack<Edit>) -> bool {
    let Some(group) = stack.undo() else {
        return false;
    };
    for (index, old, _) in group.entries.iter().rev() {
        doc[*index] = *old;
    }
    true
}

fn apply_redo(doc: &mut [i32], stack: &mut UndoStack<Edit>) -> bool {
    let Some(group) = stack.redo() else {
        return false;
    };
    for (index, _, new) in &group.entries {
        doc[*index] = *new;
    }
    true
}

#[test]
fn groups_undo_and_redo_together_and_new_edits_drop_redo() {
    let mut doc = vec![0, 0, 0];
    let mut stack = UndoStack::new(10);
    doc[0] = 1;
    stack.push("Set A", (0, 0, 1));
    stack.begin_group("Fill");
    doc[1] = 2;
    stack.push("ignored", (1, 0, 2));
    stack.begin_group("nested");
    doc[2] = 3;
    stack.push("ignored", (2, 0, 3));
    stack.end_group();
    assert!(!stack.can_undo(), "no undo while a group is open");
    stack.end_group();
    assert_eq!(stack.undo_label(), Some("Fill"));
    assert!(apply_undo(&mut doc, &mut stack));
    assert_eq!(doc, vec![1, 0, 0]);
    assert_eq!(stack.redo_label(), Some("Fill"));
    assert!(apply_redo(&mut doc, &mut stack));
    assert_eq!(doc, vec![1, 2, 3]);
    assert!(apply_undo(&mut doc, &mut stack));
    doc[2] = 9;
    stack.push("Set C", (2, 0, 9));
    assert!(!stack.can_redo());
    assert!(apply_undo(&mut doc, &mut stack));
    assert!(apply_undo(&mut doc, &mut stack));
    assert_eq!(doc, vec![0, 0, 0]);
    assert!(!apply_undo(&mut doc, &mut stack));
    stack.begin_group("Empty");
    stack.end_group();
    assert!(!stack.can_undo());
}

#[test]
fn merging_coalesces_typing_until_broken() {
    let mut stack: UndoStack<String> = UndoStack::new(10);
    let merge = |previous: &mut String, next: &String| {
        previous.push_str(next);
        true
    };
    stack.push_merging("Typing", "a".into(), merge);
    stack.push_merging("Typing", "b".into(), merge);
    stack.push_merging("Delete", "x".into(), merge);
    stack.push_merging("Delete", "y".into(), merge);
    stack.break_merge();
    stack.push_merging("Delete", "z".into(), merge);
    assert_eq!(stack.undo().unwrap().entries, vec!["z".to_string()]);
    assert_eq!(stack.undo().unwrap().entries, vec!["xy".to_string()]);
    assert_eq!(stack.undo().unwrap().entries, vec!["ab".to_string()]);
    // After undoing, a new edit does not merge into redo history.
    stack.push_merging("Typing", "c".into(), merge);
    assert!(!stack.can_redo());
    assert_eq!(stack.undo().unwrap().entries, vec!["c".to_string()]);
}

#[test]
fn clean_state_tracks_saves_and_survives_limit_trimming() {
    let mut stack: UndoStack<u8> = UndoStack::new(2);
    assert!(stack.is_clean());
    stack.push("1", 1);
    assert!(!stack.is_clean());
    stack.set_clean();
    assert!(stack.is_clean());
    // A save point blocks merging so the saved state stays reachable.
    stack.push_merging("1", 2, |_, _| true);
    assert!(!stack.is_clean());
    stack.undo();
    assert!(stack.is_clean());
    stack.redo();
    stack.push("3", 3);
    stack.push("4", 4);
    // The saved state was trimmed away by the limit.
    assert!(!stack.is_clean());
    stack.undo();
    stack.undo();
    assert!(!stack.can_undo());
    assert!(!stack.is_clean());
    stack.clear();
    assert!(stack.is_clean());
}
