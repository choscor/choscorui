//! Regression tests of the model views through the control host:
//! ItemView, HeaderView, and ColumnBrowser.

mod hosted;

use choscorui_controls::{
    ControlMessage, ControlValue, HeaderColumn, HeaderInput, HeaderView, ItemView, TextItemDelegate,
};
use choscorui_core::{
    DisplayCommand, DisplayList, Key, Point, SemanticRole, SortOrder, StringListModel, TreeStore,
};
use hosted::{Harness, texts};

fn rows(count: usize) -> ItemView {
    let model = StringListModel::from_strings((0..count).map(|row| format!("Row {row}")));
    ItemView::new("Rows", SemanticRole::List, 20.0)
        .with_model(Box::new(model), Box::new(TextItemDelegate::default()))
}

/// The start of the text run `label` in `list`.
fn text_x(list: &DisplayList, label: &str) -> f64 {
    list.commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawText { text, baseline, .. } if text == label => Some(baseline.x),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text {label:?}"))
}

#[test]
fn a_flat_list_reserves_no_disclosure_gap() {
    let flat = Harness::new(rows(3));
    let mut store = TreeStore::new(1);
    let parent = store.push(None, "Parent".to_owned()).unwrap();
    store.push(Some(parent), "Child".to_owned()).unwrap();
    store.push(None, "Row 0".to_owned()).unwrap();
    let tree = Harness::new(
        ItemView::new("Rows", SemanticRole::Tree, 20.0)
            .with_model(Box::new(store), Box::new(TextItemDelegate::default())),
    );
    let gap = text_x(&tree.paint(), "Row 0") - text_x(&flat.paint(), "Row 0");
    assert_eq!(gap, 16.0, "only the tree reserves the disclosure");
}

#[test]
fn focus_shows_a_current_row_and_space_does_not_type_ahead() {
    let mut h = Harness::new(rows(30));
    h.focus();
    // The first row is current, so Space selects it.
    h.key(Key::Space);
    assert!(h.text(" "));
    h.text("r");
    h.text("o");
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Changed(ControlValue::Indices(vec![0])),
            ControlMessage::Changed(ControlValue::Indices(vec![1])),
        ],
        "the search is \"ro\", not \" ro\""
    );
}

#[test]
fn a_click_on_empty_space_clears_the_selection_and_an_empty_view_says_so() {
    let mut h = Harness::new(rows(2));
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 20.0, bounds.origin.y + 10.0));
    h.click(Point::new(bounds.origin.x + 20.0, bounds.max_y() - 10.0));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Changed(ControlValue::Indices(vec![0])),
            ControlMessage::Changed(ControlValue::Indices(vec![])),
        ]
    );
    let empty = Harness::new(rows(0));
    assert_eq!(texts(&empty.paint()), vec!["No results"]);
}

fn header() -> HeaderView {
    let column = |title: &str| HeaderColumn {
        title: title.into(),
        width: 100.0,
        min_width: 40.0,
    };
    HeaderView::new(vec![column("Name"), column("Size"), column("Kind")])
}

#[test]
fn a_header_sorts_on_release_in_the_same_column_and_has_a_none_step() {
    let mut header = header();
    header.handle(HeaderInput::Press { x: 50.0 });
    header.handle(HeaderInput::Release { x: 150.0 });
    assert_eq!(header.sort(), None, "the release left the column");
    for order in [
        Some(SortOrder::Ascending),
        Some(SortOrder::Descending),
        None,
    ] {
        header.handle(HeaderInput::Press { x: 50.0 });
        header.handle(HeaderInput::Release { x: 60.0 });
        assert_eq!(header.sort(), order.map(|order| (0, order)));
    }
}

#[test]
fn a_header_goes_home_and_end_and_escape_restores_a_width() {
    let mut header = header();
    header.handle(HeaderInput::Key(Key::End));
    assert_eq!(header.focused_column(), 2);
    header.handle(HeaderInput::Key(Key::Home));
    assert_eq!(header.focused_column(), 0);
    header.handle(HeaderInput::Press { x: 100.0 });
    header.handle(HeaderInput::Drag { x: 160.0 });
    assert_eq!(header.widths()[0], 160.0);
    let response = header.handle(HeaderInput::Key(Key::Escape));
    assert_eq!(response.resized, Some(0));
    assert_eq!(header.widths()[0], 100.0);
    header.handle(HeaderInput::Drag { x: 200.0 });
    assert_eq!(header.widths()[0], 100.0, "the resize ended");
}
