use choscorui_controls::{TextEditor, TextEditorChange, TextEditorInput};
use choscorui_core::Key;
use choscorui_core::{
    CaretStop, KeyEvent, LayoutLine, NodeId, Point, Rect, SemanticAction, Size, TextLayout,
    TextLayoutError, TextShaper, TextStyle, Tree, TreeError, grapheme_boundaries,
};

/// Monospace: 10 points per grapheme, 10-point lines, wraps at `max_width`.
struct Mono;

impl TextShaper for Mono {
    fn shape(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
    ) -> Result<TextLayout, TextLayoutError> {
        let all = grapheme_boundaries(text);
        let mut lines = Vec::new();
        let mut top = 0.0;
        let mut start = 0;
        for raw in text.split('\n') {
            let first = all.binary_search(&start).unwrap();
            let local = grapheme_boundaries(raw);
            let count = local.len() - 1;
            let per_line = max_width.map_or(count.max(1), |w| ((w / 10.0) as usize).max(1));
            let mut g = 0;
            loop {
                let end = (g + per_line).min(count);
                let span = end - g;
                lines.push(LayoutLine {
                    bytes: start + local[g]..start + local[end],
                    graphemes: first + g..first + end,
                    baseline: top + 8.0,
                    ascent: 8.0,
                    descent: 2.0,
                    leading: 0.0,
                    width: span as f64 * 10.0,
                    offset_x: 0.0,
                    right_to_left: false,
                    runs: Vec::new(),
                    carets: (0..=span)
                        .map(|i| CaretStop {
                            primary: i as f64 * 10.0,
                            secondary: i as f64 * 10.0,
                        })
                        .collect(),
                    extents: (0..span)
                        .map(|i| (i as f64 * 10.0, (i + 1) as f64 * 10.0))
                        .collect(),
                });
                top += 10.0;
                g = end;
                if g >= count {
                    break;
                }
            }
            start += raw.len() + 1;
        }
        TextLayout::from_lines(text, style.clone(), max_width, lines)
    }
}

fn layout(editor: &TextEditor) -> TextLayout {
    Mono.shape(
        &editor.layout_text(),
        &TextStyle::new("Mono", 10.0),
        Some(60.0),
    )
    .unwrap()
}

fn setup(text: &str) -> (Tree, NodeId, TextEditor) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 200.0)));
    let id = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(0.0, 0.0), Size::new(60.0, 100.0)),
        )
        .unwrap();
    tree.set_focusable(id, true).unwrap();
    tree.set_focus(Some(id)).unwrap();
    (tree, id, TextEditor::new("Notes", text))
}

fn send(
    tree: &mut Tree,
    id: NodeId,
    editor: &mut TextEditor,
    input: TextEditorInput,
) -> TextEditorChange {
    let current = layout(editor);
    editor.handle(tree, id, &current, input).unwrap()
}

fn key(tree: &mut Tree, id: NodeId, editor: &mut TextEditor, key: Key, extend: bool) {
    send(
        tree,
        id,
        editor,
        TextEditorInput::Key {
            event: KeyEvent::Down { key, repeat: false },
            extend,
            word: false,
        },
    );
}

#[test]
fn enter_inserts_newlines_and_vertical_motion_keeps_column() {
    let (mut tree, id, mut editor) = setup("");
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::Text("abcdef".into()),
    );
    key(&mut tree, id, &mut editor, Key::Enter, false);
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::Text("xy\r\nz".into()),
    );
    assert_eq!(editor.value(), "abcdef\nxy\nz");
    // From the end of "abcdef" (column 6) down to "xy" clamps to its end,
    // then back up returns to column 6 thanks to the preferred x.
    editor
        .handle(
            &mut tree,
            id,
            &layout(&editor),
            TextEditorInput::SetCaret {
                caret: 6,
                extend: false,
            },
        )
        .unwrap();
    key(&mut tree, id, &mut editor, Key::ArrowDown, false);
    assert_eq!(editor.caret(), 9);
    key(&mut tree, id, &mut editor, Key::ArrowUp, false);
    assert_eq!(editor.caret(), 6);
    key(&mut tree, id, &mut editor, Key::Home, false);
    assert_eq!(editor.caret(), 0);
    key(&mut tree, id, &mut editor, Key::End, true);
    assert_eq!(editor.selection(), 0..6);
    assert_eq!(editor.selected_text(), "abcdef");
    let rects = editor.selection_rects(&layout(&editor));
    assert_eq!(
        rects,
        vec![Rect::new(Point::new(0.0, 0.0), Size::new(60.0, 10.0))]
    );
}

#[test]
fn wrapped_lines_page_and_home_end_follow_visual_lines() {
    let text = "a".repeat(30);
    let (mut tree, id, mut editor) = setup(&text);
    editor.set_page_height(20.0);
    // Six graphemes per visual line: Home/End stay within the visual line.
    editor
        .handle(
            &mut tree,
            id,
            &layout(&editor),
            TextEditorInput::SetCaret {
                caret: 8,
                extend: false,
            },
        )
        .unwrap();
    key(&mut tree, id, &mut editor, Key::Home, false);
    assert_eq!(editor.caret(), 6);
    key(&mut tree, id, &mut editor, Key::End, false);
    assert_eq!(editor.caret(), 12);
    key(&mut tree, id, &mut editor, Key::PageDown, false);
    assert_eq!(editor.caret(), 24);
    key(&mut tree, id, &mut editor, Key::PageUp, false);
    assert_eq!(editor.caret(), 12);
    let caret = editor.caret_rect(&layout(&editor)).unwrap();
    assert_eq!(caret.origin, Point::new(0.0, 20.0));
}

#[test]
fn pointer_selects_by_drag_word_and_paragraph() {
    let (mut tree, id, mut editor) = setup("one two\nthree");
    let down = |position, clicks| TextEditorInput::PointerDown {
        position,
        extend: false,
        clicks,
    };
    send(&mut tree, id, &mut editor, down(Point::new(11.0, 5.0), 1));
    assert_eq!(editor.caret(), 1);
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::PointerDrag {
            // "one two" wraps after "one tw"; "three" is the third line.
            position: Point::new(500.0, 25.0),
        },
    );
    assert_eq!(editor.selection(), 1..13);
    send(&mut tree, id, &mut editor, TextEditorInput::PointerUp);
    send(&mut tree, id, &mut editor, down(Point::new(51.0, 5.0), 2));
    assert_eq!(editor.selected_text(), "two");
    send(&mut tree, id, &mut editor, down(Point::new(21.0, 25.0), 3));
    assert_eq!(editor.selected_text(), "three");
}

#[test]
fn composition_survives_redraws_commits_on_click_and_undo_skips_marked_text() {
    let (mut tree, id, mut editor) = setup("ab");
    editor
        .handle(
            &mut tree,
            id,
            &layout(&editor),
            TextEditorInput::SetCaret {
                caret: 2,
                extend: false,
            },
        )
        .unwrap();
    let change = send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::SetMarkedText {
            text: "にほ".into(),
            selected_utf16: 2..2,
            replacement_utf16: None,
        },
    );
    assert_eq!(change, TextEditorChange::Composition);
    assert_eq!(editor.layout_text(), "abにほ");
    assert_eq!(editor.marked_range_utf16(), Some(2..4));
    // A redraw (re-layout) does not disturb composition.
    let _ = layout(&editor);
    assert_eq!(editor.caret_rect(&layout(&editor)).unwrap().origin.x, 40.0);
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::SetMarkedText {
            text: "日本".into(),
            selected_utf16: 0..2,
            replacement_utf16: None,
        },
    );
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::CommitMarkedText {
            text: "日本".into(),
            replacement_utf16: None,
        },
    );
    assert_eq!(editor.value(), "ab日本");
    assert!(editor.composition().is_none());
    // Dead key: marked accent then committed composed letter.
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::SetMarkedText {
            text: "´".into(),
            selected_utf16: 1..1,
            replacement_utf16: None,
        },
    );
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::CommitMarkedText {
            text: "é".into(),
            replacement_utf16: None,
        },
    );
    assert_eq!(editor.value(), "ab日本é");
    send(&mut tree, id, &mut editor, TextEditorInput::Undo);
    assert_eq!(editor.value(), "ab日本");
    send(&mut tree, id, &mut editor, TextEditorInput::Undo);
    assert_eq!(editor.value(), "ab");
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::SetMarkedText {
            text: "x".into(),
            selected_utf16: 1..1,
            replacement_utf16: None,
        },
    );
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::PointerDown {
            position: Point::new(0.0, 0.0),
            extend: false,
            clicks: 1,
        },
    );
    assert_eq!(editor.value(), "abx");
}

#[test]
fn removed_node_rejects_edits_and_read_only_blocks_changes() {
    let (mut tree, id, mut editor) = setup("keep");
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::SetMarkedText {
            text: "ん".into(),
            selected_utf16: 1..1,
            replacement_utf16: None,
        },
    );
    let current = layout(&editor);
    tree.detach(id).unwrap();
    assert_eq!(
        editor.handle(&mut tree, id, &current, TextEditorInput::Text("x".into())),
        Err(TreeError::MissingNode)
    );
    assert_eq!(editor.value(), "keep");
    assert!(
        editor.composition().is_none(),
        "composition dropped with the node"
    );

    let (mut tree, id, mut editor) = setup("fixed");
    editor.set_read_only(true);
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::Text("x".into()),
    );
    key(&mut tree, id, &mut editor, Key::Backspace, false);
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::Semantic(SemanticAction::SetValue, Some("y".into())),
    );
    assert_eq!(editor.value(), "fixed");
    let semantics = editor.semantics();
    assert!(semantics.state.read_only);
    assert!(semantics.text.unwrap().multiline);
    assert_eq!(semantics.actions, vec![SemanticAction::Focus]);
}

#[test]
fn tab_inserts_only_when_enabled_and_accessibility_can_replace_text() {
    let (mut tree, id, mut editor) = setup("");
    key(&mut tree, id, &mut editor, Key::Tab, false);
    assert_eq!(editor.value(), "");
    editor.set_tab_inserts(true);
    key(&mut tree, id, &mut editor, Key::Tab, false);
    assert_eq!(editor.value(), "\t");
    send(
        &mut tree,
        id,
        &mut editor,
        TextEditorInput::Semantic(SemanticAction::SetValue, Some("Xin chào\nthế giới".into())),
    );
    assert_eq!(editor.value(), "Xin chào\nthế giới");
    assert_eq!(editor.semantics().text.unwrap().character_count_utf16, 17);
}
