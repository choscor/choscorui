use choscorui_controls::{
    CellEdit, ColumnBrowser, DataMapper, DefaultEditorFactory, EditorKind, HeaderColumn,
    HeaderInput, HeaderView, ItemModifiers, ItemView, ItemViewInput, TextItemDelegate,
    ValidationError, ValidationState, ViewLayout, undo_history, undo_steps_to,
};
use choscorui_controls::{EstimatedText, PaintContext, Styles, VisualState};
use choscorui_core::{
    Catalog, ColorRole, DesignTokens, DisplayCommand, DisplayList, ItemData, ItemKey, ItemModel,
    Key, ListStore, Locale, Localizer, ModelError, NodeId, Paint, Point, Rect, Role, SelectionMode,
    SemanticAction, SemanticRole, SemanticTree, Size, SliceModel, SortFilterProxy, SortOrder,
    StringListModel, TextFilter, Tree, TreeStore, UndoStack, Value,
};
use std::time::Duration;

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(w, h))
}

fn setup() -> (Tree, NodeId) {
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 400.0));
    let id = tree
        .attach(tree.root(), rect(0.0, 0.0, 300.0, 200.0))
        .unwrap();
    tree.set_focusable(id, true).unwrap();
    (tree, id)
}

fn key(key: Key) -> ItemViewInput {
    ItemViewInput::Key {
        key,
        modifiers: ItemModifiers::default(),
    }
}

fn shift(key: Key) -> ItemViewInput {
    ItemViewInput::Key {
        key,
        modifiers: ItemModifiers {
            shift: true,
            command: false,
        },
    }
}

#[test]
fn hundred_thousand_rows_keep_realized_nodes_bounded_while_scrolling() {
    let numbers: Vec<u32> = (0..100_000).collect();
    let model = SliceModel::new(&numbers, 1, |n: &u32, _, role| match role {
        Role::Display => Value::Text(format!("Row {n}")),
        _ => Value::None,
    });
    let (mut tree, id) = setup();
    let mut view = ItemView::new("Rows", SemanticRole::List, 20.0);
    view.sync(&model);
    view.arrange(&mut tree, id, rect(0.0, 0.0, 300.0, 200.0))
        .unwrap();
    let bound = 10 + 2 * 2 + 1;
    assert!(tree.children(id).unwrap().len() <= bound);
    let mut peak = 0;
    for _ in 0..500 {
        let response = view
            .handle(&mut tree, id, &model, key(Key::PageDown))
            .unwrap();
        if response.scrolled {
            view.arrange(&mut tree, id, rect(0.0, 0.0, 300.0, 200.0))
                .unwrap();
        }
        peak = peak.max(tree.children(id).unwrap().len());
    }
    assert!(peak <= bound, "peak realized {peak}");
    view.handle(&mut tree, id, &model, key(Key::End)).unwrap();
    view.arrange(&mut tree, id, rect(0.0, 0.0, 300.0, 200.0))
        .unwrap();
    assert_eq!(view.current(), Some(ItemKey(99_999)));
    let last = view.realized().last().unwrap();
    assert_eq!(last.key, ItemKey(99_999));
    assert!(last.bounds.origin.y + last.bounds.size.height <= 200.0 + 1e-9);
    assert!(tree.children(id).unwrap().len() <= bound);

    let mut list = DisplayList::new();
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    view.paint(
        &mut PaintContext::new(
            &mut list,
            rect(0.0, 0.0, 300.0, 200.0),
            &tokens,
            &styles,
            &EstimatedText,
        ),
        &model,
        &TextItemDelegate::default(),
        &[],
        VisualState::FOCUSED,
    );
    assert!(list.len() < 100, "painting is proportional to the viewport");
}

#[test]
fn selection_survives_filter_and_sort_and_type_ahead_folds_diacritics() {
    let store = StringListModel::from_strings(["Đà Lạt", "Hà Nội", "Huế", "Hội An", "Sa Pa"]);
    let mut proxy = SortFilterProxy::new();
    proxy.sync(&store);
    let (mut tree, id) = setup();
    let mut view = ItemView::new("Cities", SemanticRole::List, 20.0);
    view.set_selection_mode(SelectionMode::Extended);
    view.sync(&proxy.view(&store));
    view.arrange(&mut tree, id, rect(0.0, 0.0, 300.0, 200.0))
        .unwrap();

    let at = |ms| Duration::from_millis(ms);
    view.handle(
        &mut tree,
        id,
        &proxy.view(&store),
        ItemViewInput::Type {
            character: 'h',
            at: at(0),
        },
    )
    .unwrap();
    let ha_noi = store.key(None, 1).unwrap();
    assert_eq!(view.current(), Some(ha_noi));
    for (ms, ch) in [(100, 'o'), (200, 'i')] {
        view.handle(
            &mut tree,
            id,
            &proxy.view(&store),
            ItemViewInput::Type {
                character: ch,
                at: at(ms),
            },
        )
        .unwrap();
    }
    let hoi_an = store.key(None, 3).unwrap();
    assert_eq!(view.current(), Some(hoi_an), "\"hoi\" matches Hội An");
    view.handle(&mut tree, id, &proxy.view(&store), shift(Key::ArrowUp))
        .unwrap();
    assert_eq!(view.selection().len(), 2);

    proxy.set_sort(Some((0, SortOrder::Descending)));
    proxy.set_filter(Some(TextFilter::contains("h")));
    proxy.sync(&store);
    assert!(view.sync(&proxy.view(&store)));
    assert!(view.selection().is_selected(hoi_an));
    assert_eq!(view.order().len(), 3);
    let hue = store.key(None, 2).unwrap();
    assert_eq!(view.current(), Some(hue), "Shift-Up moved to Huế");
    // After a pause the search restarts from the item after Huế; a stale
    // "hoih" buffer would match nothing and leave Huế current.
    view.handle(
        &mut tree,
        id,
        &proxy.view(&store),
        ItemViewInput::Type {
            character: 'h',
            at: at(5_000),
        },
    )
    .unwrap();
    assert_eq!(view.current(), Some(hoi_an));
}

#[test]
fn tree_keys_expand_collapse_and_request_lazy_fetch() {
    let mut store: TreeStore<String> = TreeStore::new(1);
    let fruits = store.push(None, "Fruits".into()).unwrap();
    let apple = store.push(Some(fruits), "Apple".into()).unwrap();
    store.push(None, "Vegetables".into()).unwrap();
    let (mut tree, id) = setup();
    let mut view = ItemView::new("Food", SemanticRole::Tree, 20.0);
    view.sync(&store);
    view.arrange(&mut tree, id, rect(0.0, 0.0, 300.0, 200.0))
        .unwrap();
    assert_eq!(view.len(), 2);
    view.handle(&mut tree, id, &store, key(Key::Home)).unwrap();
    let response = view
        .handle(&mut tree, id, &store, key(Key::ArrowRight))
        .unwrap();
    assert!(response.expansion_changed);
    assert_eq!(view.len(), 3);
    view.handle(&mut tree, id, &store, key(Key::ArrowRight))
        .unwrap();
    assert_eq!(view.current(), Some(apple), "second Right enters the child");
    view.handle(&mut tree, id, &store, key(Key::ArrowLeft))
        .unwrap();
    assert_eq!(
        view.current(),
        Some(fruits),
        "Left on a leaf goes to the parent"
    );
    view.handle(&mut tree, id, &store, key(Key::ArrowLeft))
        .unwrap();
    assert_eq!(view.len(), 2);

    view.arrange(&mut tree, id, rect(0.0, 0.0, 300.0, 200.0))
        .unwrap();
    let mut semantics = SemanticTree::new();
    let changes = view
        .sync_semantics(&mut semantics, &tree, id, &store)
        .unwrap();
    assert!(!changes.is_empty());
    let snapshot = semantics.snapshot(&tree);
    assert_eq!(snapshot[0].node.role, SemanticRole::Tree);
    assert_eq!(snapshot[0].collection.unwrap().item_count, 2);
    assert_eq!(snapshot[1].node.role, SemanticRole::TreeItem);
    assert_eq!(snapshot[1].node.state.expanded, Some(false));
    assert_eq!(snapshot[1].item_index, Some(0));
    assert!(snapshot[1].node.actions.contains(&SemanticAction::Expand));

    // Assistive expand goes through the same path.
    let response = view
        .handle(
            &mut tree,
            id,
            &store,
            ItemViewInput::Semantic {
                key: fruits,
                action: SemanticAction::Expand,
            },
        )
        .unwrap();
    assert!(response.expansion_changed);

    // Removing the expanded parent drops it from expansion and selection.
    store.remove(&[fruits]).unwrap();
    view.sync(&store);
    assert!(!view.is_expanded(fruits));
    assert_eq!(view.selection().len(), 0);
}

#[test]
fn grid_layout_moves_by_rows_and_hit_tests_cells() {
    let names: Vec<String> = (0..20).map(|n| format!("Icon {n}")).collect();
    let model = StringListModel::from_strings(names);
    let (mut tree, id) = setup();
    let mut view = ItemView::new("Icons", SemanticRole::List, 80.0);
    view.set_layout(ViewLayout::Grid { item_width: 100.0 });
    view.sync(&model);
    view.arrange(&mut tree, id, rect(0.0, 0.0, 300.0, 200.0))
        .unwrap();
    view.handle(&mut tree, id, &model, key(Key::Home)).unwrap();
    view.handle(&mut tree, id, &model, key(Key::ArrowDown))
        .unwrap();
    assert_eq!(view.current(), model.key(None, 3), "three per row");
    view.handle(&mut tree, id, &model, key(Key::ArrowRight))
        .unwrap();
    assert_eq!(view.current(), model.key(None, 4));
    let response = view
        .handle(
            &mut tree,
            id,
            &model,
            ItemViewInput::Press {
                position: Point::new(250.0, 10.0),
                modifiers: ItemModifiers::default(),
                clicks: 2,
            },
        )
        .unwrap();
    assert_eq!(response.activated, model.key(None, 2));
    assert_eq!(tree.focus(), Some(id));
}

#[test]
fn header_sorts_resizes_and_exposes_semantics() {
    let mut header = HeaderView::new(vec![
        HeaderColumn {
            title: "Tên".into(),
            width: 120.0,
            min_width: 40.0,
        },
        HeaderColumn {
            title: "Size".into(),
            width: 80.0,
            min_width: 40.0,
        },
    ]);
    assert!(header.handle(HeaderInput::Press { x: 150.0 }).sort_changed);
    assert_eq!(header.sort(), Some((1, SortOrder::Ascending)));
    header.handle(HeaderInput::Press { x: 150.0 });
    assert_eq!(header.sort(), Some((1, SortOrder::Descending)));
    let english = header.column_semantics(1, Localizer::builtin()).unwrap();
    assert_eq!(english.value.as_deref(), Some("descending"));
    let vietnamese = Localizer::new(Locale::parse("vi").unwrap(), Catalog::new());
    let column = header.column_semantics(1, &vietnamese).unwrap();
    assert_eq!(column.value.as_deref(), Some("giảm dần"));
    header.handle(HeaderInput::Press { x: 120.0 });
    assert_eq!(
        header.handle(HeaderInput::Drag { x: 60.0 }).resized,
        Some(0)
    );
    assert_eq!(header.widths()[0], 60.0);
    header.handle(HeaderInput::Drag { x: -100.0 });
    assert_eq!(header.widths()[0], 40.0, "minimum width");
    header.handle(HeaderInput::Release);
    header.handle(HeaderInput::Key(Key::ArrowLeft));
    header.handle(HeaderInput::Key(Key::Character('+')));
    assert_eq!(header.widths()[0], 50.0);
    header.handle(HeaderInput::Key(Key::Enter));
    assert_eq!(header.sort(), Some((0, SortOrder::Ascending)));
    header.restore_sort(Some((1, SortOrder::Descending)));
    assert_eq!(header.sort(), Some((1, SortOrder::Descending)));
    header.restore_sort(Some((2, SortOrder::Ascending)));
    assert_eq!(
        header.sort(),
        None,
        "a column past the last clears the sort"
    );
}

#[test]
fn header_paints_cells_sort_icon_and_ring_from_the_table_recipe() {
    let mut header = HeaderView::new(vec![
        HeaderColumn {
            title: "Name".into(),
            width: 120.0,
            min_width: 40.0,
        },
        HeaderColumn {
            title: "Size".into(),
            width: 80.0,
            min_width: 40.0,
        },
    ]);
    header.handle(HeaderInput::Press { x: 150.0 });
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let paint = |focus_visible| {
        let mut list = DisplayList::new();
        header.paint(
            &mut PaintContext::new(
                &mut list,
                rect(0.0, 0.0, 200.0, 40.0),
                &tokens,
                &styles,
                &EstimatedText,
            ),
            focus_visible,
        );
        list
    };
    let plain = paint(false);
    let texts: Vec<_> = plain
        .commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["Name", "Size"]);
    // The sort chevron is a muted path in the sorted column.
    let muted = tokens.color(ColorRole::MutedForeground);
    assert!(plain.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::StrokePath { paint: Paint::Solid(color), .. } if *color == muted
    )));
    let ringed = paint(true);
    assert!(
        ringed.commands().len() > plain.commands().len(),
        "focus ring"
    );
}

#[derive(Clone)]
struct Item {
    name: String,
    quantity: i64,
}

impl ItemData for Item {
    fn data(&self, column: usize, role: Role) -> Value {
        match (column, role) {
            (0, Role::Display | Role::Edit) => Value::Text(self.name.clone()),
            (1, Role::Display) => Value::Text(self.quantity.to_string()),
            (1, Role::Edit | Role::Sort) => Value::Int(self.quantity),
            _ => Value::None,
        }
    }
    fn set_data(&mut self, column: usize, _: Role, value: Value) -> Result<(), ModelError> {
        match (column, value) {
            (0, Value::Text(text)) => self.name = text,
            (1, Value::Int(quantity)) if quantity >= 0 => self.quantity = quantity,
            _ => return Err(ModelError::InvalidValue),
        }
        Ok(())
    }
    fn editable(&self, _: usize) -> bool {
        true
    }
}

#[test]
fn cell_editors_validate_locale_numbers_and_mapper_keeps_pending_text() {
    let mut store = ListStore::new(2);
    let first = store
        .push(
            None,
            Item {
                name: "Bút".into(),
                quantity: 1200,
            },
        )
        .unwrap();
    let second = store
        .push(
            None,
            Item {
                name: "Vở".into(),
                quantity: 3,
            },
        )
        .unwrap();
    let vi = Locale::parse("vi-VN").unwrap();
    let mut edit = CellEdit::begin(&store, &DefaultEditorFactory, first, 1, vi).unwrap();
    assert_eq!(edit.kind, EditorKind::Integer);
    assert_eq!(edit.text, "1.200");
    edit.set_text("abc");
    assert!(matches!(edit.validation(), ValidationState::Invalid(_)));
    assert!(edit.commit(&mut store).is_err());
    assert_eq!(store.data(first, 1, Role::Edit), Value::Int(1200));
    edit.set_text("2.500");
    edit.commit(&mut store).unwrap();
    assert_eq!(store.data(first, 1, Role::Edit), Value::Int(2500));

    let mut mapper = DataMapper::new(vec![0, 1]);
    mapper.set_current(Some(first));
    mapper.edit(&store, 1, "-4");
    mapper.set_current(Some(second));
    assert_eq!(mapper.values(&store)[1].1, "3");
    mapper.set_current(Some(first));
    assert_eq!(mapper.values(&store)[1].1, "-4", "pending text survives");
    let parse = |column: usize, text: &str| -> Result<Value, ValidationError> {
        if column == 1 {
            text.parse::<i64>()
                .map(Value::Int)
                .map_err(|e| e.to_string().into())
        } else {
            Ok(Value::Text(text.into()))
        }
    };
    mapper.edit(&store, 0, "Bút chì");
    let errors = mapper.submit(&mut store, parse).unwrap_err();
    assert_eq!(errors.len(), 1, "negative quantity rejected by the model");
    assert_eq!(
        store.data(first, 0, Role::Display).to_display_string(),
        "Bút chì"
    );
    assert!(mapper.has_pending(first));
    store.remove(&[first]).unwrap();
    mapper.retain_existing(&store);
    assert_eq!(mapper.current(), None);
}

#[test]
fn mapper_formats_numbers_marks_one_field_and_keeps_errors_with_their_text() {
    let mut store = ListStore::new(2);
    let first = store
        .push(
            None,
            Item {
                name: "Bút".into(),
                quantity: 1200,
            },
        )
        .unwrap();
    let second = store
        .push(
            None,
            Item {
                name: "Vở".into(),
                quantity: 3,
            },
        )
        .unwrap();
    let vi = Locale::parse("vi-VN").unwrap();
    let parse = |column: usize, text: &str| -> Result<Value, ValidationError> {
        match column {
            1 => vi
                .parse_integer(text)
                .map(Value::Int)
                .map_err(|_| ValidationError::Number(choscorui_controls::NumberError::NotWhole)),
            _ => Ok(Value::Text(text.trim().into())),
        }
    };
    let mut mapper = DataMapper::new(vec![0, 1]).with_locale(vi.clone());
    mapper.set_current(Some(first));
    assert_eq!(
        mapper.values(&store)[1].1,
        "1.200",
        "locale group separator"
    );
    // Text equal to the model value is not pending.
    mapper.edit(&store, 1, " 1.200 ");
    assert!(!mapper.is_pending(1));
    mapper.edit(&store, 1, "x");
    assert!(
        mapper.is_pending(1) && !mapper.is_pending(0),
        "only the edited field"
    );
    assert!(mapper.submit(&mut store, parse).is_err());
    assert!(mapper.error(1).is_some());
    // The error stays with its text on another row and back.
    mapper.set_current(Some(second));
    assert!(mapper.error(1).is_none());
    mapper.set_current(Some(first));
    assert!(mapper.error(1).is_some());
    // An edit keeps the error until a check finds the text valid.
    mapper.edit(&store, 1, "7");
    assert!(mapper.error(1).is_some());
    mapper.check(1, parse);
    assert!(mapper.error(1).is_none());
    mapper.submit(&mut store, parse).unwrap();
    assert_eq!(store.data(first, 1, Role::Edit), Value::Int(7));

    // A cell editor with a minimum of 0 refuses a minus sign.
    let edit = CellEdit::begin(&store, &DefaultEditorFactory, second, 1, vi)
        .unwrap()
        .with_range(Some(0.0), None);
    assert!(edit.filter().accept("", 0..0, "-4").is_none());
}

#[test]
fn column_browser_and_undo_history() {
    let mut store: TreeStore<String> = TreeStore::new(1);
    let docs = store.push(None, "Docs".into()).unwrap();
    let work = store.push(Some(docs), "Work".into()).unwrap();
    store.push(Some(work), "plan.txt".into()).unwrap();
    store.push(None, "Music".into()).unwrap();
    let mut browser = ColumnBrowser::new();
    assert!(browser.handle_key(&store, Key::ArrowDown));
    assert_eq!(browser.path(), &[docs]);
    assert!(browser.handle_key(&store, Key::ArrowRight));
    assert_eq!(browser.path(), &[docs, work]);
    assert_eq!(browser.columns(&store), vec![None, Some(docs), Some(work)]);
    assert!(browser.handle_key(&store, Key::ArrowLeft));
    assert_eq!(browser.path(), &[docs]);
    store.remove(&[docs]).unwrap();
    browser.retain_existing(&store);
    assert!(browser.path().is_empty());

    let mut stack = UndoStack::new(10);
    stack.push("Type", 1);
    stack.push("Bold", 2);
    stack.undo();
    let rows = undo_history(&stack, "<empty>");
    assert_eq!(rows.len(), 3);
    assert!(rows[1].current && rows[2].redoable);
    assert_eq!(undo_steps_to(&stack, 0), -1);
    assert_eq!(undo_steps_to(&stack, 2), 1);
}

#[test]
fn column_browser_entries_paint_with_the_list_row_look() {
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let paint = |selected, active| {
        let mut list = DisplayList::new();
        let cx = &mut PaintContext::new(
            &mut list,
            Rect::new(Point::new(0.0, 0.0), Size::new(180.0, 32.0)),
            &tokens,
            &styles,
            &EstimatedText,
        );
        ColumnBrowser::paint_entry(cx, "Documents", true, selected, active);
        list
    };
    let fills = |list: &DisplayList, role| {
        let color = tokens.color(role);
        list.commands().iter().any(|command| {
            matches!(command, DisplayCommand::FillRoundedRect { color: c, .. } if *c == color)
        })
    };
    assert!(fills(&paint(true, false), ColorRole::Muted));
    assert!(fills(&paint(true, true), ColorRole::Accent));
    let plain = paint(false, false);
    assert!(!fills(&plain, ColorRole::Muted));
    assert!(plain.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::DrawText { text, .. } if text == "Documents"
    )));
}
