//! Regression tests of the model views through the control host:
//! ItemView, HeaderView, and ColumnBrowser.

mod hosted;

use choscorui_controls::{
    ColumnBrowser, ControlMessage, ControlValue, HeaderColumn, HeaderInput, HeaderView,
    HostedColumnBrowser, ItemView, TextItemDelegate,
};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, DisplayList, EditableModel, ItemKey, ItemModel, Key,
    ModelChange, ModelError, Point, PointerEvent, Role, SemanticRole, SortOrder, StringListModel,
    TreeStore, Value, WheelUnit,
};
use hosted::{Harness, rect, texts};
use std::cell::Cell;
use std::rc::Rc;

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

fn numbered(count: usize) -> HostedColumnBrowser {
    let mut store = TreeStore::new(1);
    for row in 0..count {
        store.push(None, format!("Row {row}")).unwrap();
    }
    ColumnBrowser::new().hosted("Rows", Box::new(store))
}

#[test]
fn a_column_browser_scrolls_with_the_wheel_and_shows_a_scroll_bar() {
    let mut h = Harness::new(numbered(50));
    let thumb = DesignTokens::light().color(ColorRole::Border);
    assert!(
        !hosted::rounded_fills(&h.paint(), thumb).is_empty(),
        "a bar"
    );
    let at = Point::new(h.bounds().origin.x + 20.0, h.bounds().center().y);
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Wheel {
            position: at,
            delta: Point::new(0.0, 5.0),
            unit: WheelUnit::Lines,
        },
    );
    assert_eq!(texts(&h.paint()).first().map(String::as_str), Some("Row 5"));
    assert!(h.messages().is_empty());
}

#[test]
fn a_column_browser_pages_types_and_activates() {
    let mut h = Harness::new(numbered(50));
    h.focus();
    h.key(Key::End);
    h.key(Key::Home);
    h.key(Key::PageDown);
    h.text("row 5");
    h.key(Key::Enter);
    let page = (h.bounds().size.height / 32.0).floor() as usize;
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Changed(ControlValue::Indices(vec![49])),
            ControlMessage::Changed(ControlValue::Indices(vec![0])),
            ControlMessage::Changed(ControlValue::Indices(vec![page])),
            ControlMessage::Changed(ControlValue::Indices(vec![5])),
            ControlMessage::ItemActivated(5),
        ]
    );
    assert!(texts(&h.paint()).contains(&"Row 5".to_string()));
}

#[test]
fn a_column_browser_keeps_the_focused_column_in_view() {
    let mut store = TreeStore::new(1);
    let mut parent = None;
    for level in 0..4 {
        parent = Some(store.push(parent, format!("Level {level}")).unwrap());
    }
    let mut h = Harness::new(ColumnBrowser::new().hosted("Levels", Box::new(store)));
    let bounds = rect(10.0, 10.0, 300.0, 200.0);
    h.place(bounds);
    h.focus();
    for _ in 0..4 {
        h.key(Key::ArrowDown);
        h.key(Key::ArrowRight);
    }
    let painted = h.paint();
    let deepest = painted
        .commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawText { text, baseline, .. } if text == "Level 3" => Some(*baseline),
            _ => None,
        })
        .expect("the deepest level paints");
    assert!(bounds.contains(deepest), "{deepest:?} in {bounds:?}");
}

/// A folder whose children load only when a view fetches them.
struct Lazy {
    store: TreeStore<String>,
    folder: ItemKey,
    fetched: Rc<Cell<usize>>,
}

impl ItemModel for Lazy {
    fn row_count(&self, parent: Option<ItemKey>) -> usize {
        self.store.row_count(parent)
    }
    fn key(&self, parent: Option<ItemKey>, row: usize) -> Option<ItemKey> {
        self.store.key(parent, row)
    }
    fn parent(&self, key: ItemKey) -> Option<ItemKey> {
        self.store.parent(key)
    }
    fn row(&self, key: ItemKey) -> Option<usize> {
        self.store.row(key)
    }
    fn data(&self, key: ItemKey, column: usize, role: Role) -> Value {
        self.store.data(key, column, role)
    }
    fn revision(&self) -> u64 {
        self.store.revision()
    }
    fn changes_since(&self, revision: u64) -> Vec<ModelChange> {
        self.store.changes_since(revision)
    }
    fn can_fetch_more(&self, parent: Option<ItemKey>) -> bool {
        parent == Some(self.folder) && self.fetched.get() == 0
    }
}

impl EditableModel for Lazy {
    fn set_data(&mut self, _: ItemKey, _: usize, _: Role, _: Value) -> Result<(), ModelError> {
        Err(ModelError::ReadOnly)
    }
    fn fetch_more(&mut self, parent: Option<ItemKey>) {
        if self.can_fetch_more(parent) {
            self.fetched.set(self.fetched.get() + 1);
            self.store.push(parent, "Loaded".to_owned()).unwrap();
        }
    }
}

#[test]
fn a_lazy_column_fetches_its_children_when_it_opens() {
    let mut store = TreeStore::new(1);
    let folder = store.push(None, "Folder".to_owned()).unwrap();
    let fetched = Rc::new(Cell::new(0));
    let model = Lazy {
        store,
        folder,
        fetched: fetched.clone(),
    };
    let mut h = Harness::new(ColumnBrowser::new().hosted_lazy("Files", Box::new(model)));
    let origin = h.bounds().origin;
    h.click(Point::new(origin.x + 20.0, origin.y + 10.0));
    assert_eq!(fetched.get(), 1);
    assert!(texts(&h.paint()).contains(&"Loaded".to_string()));
}
