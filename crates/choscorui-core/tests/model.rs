use choscorui_core::{
    EditableModel, ItemKey, ItemModel, ModelChange, ModelError, Role, Selection, SelectionCommand,
    SelectionMode, StringListModel, TreeStore, Value, visible_keys,
};

fn texts(model: &StringListModel) -> Vec<String> {
    (0..model.row_count(None))
        .map(|row| {
            let key = model.key(None, row).unwrap();
            model.data(key, 0, Role::Display).to_display_string()
        })
        .collect()
}

#[test]
fn keys_survive_insertion_removal_and_sorting() {
    let mut model = StringListModel::from_strings(["pear", "apple", "fig"]);
    let apple = model.key(None, 1).unwrap();
    let start = model.revision();
    model.insert(None, 0, ["kiwi".to_owned()]).unwrap();
    assert_eq!(model.row(apple), Some(2));
    model.sort_children_by(None, |text| text.clone());
    assert_eq!(texts(&model), ["apple", "fig", "kiwi", "pear"]);
    assert_eq!(model.row(apple), Some(0));
    let fig = model.key(None, 1).unwrap();
    model.remove(&[fig]).unwrap();
    assert_eq!(model.row(fig), None);
    let changes = model.changes_since(start);
    assert!(matches!(
        changes[0],
        ModelChange::Inserted { first_row: 0, .. }
    ));
    assert_eq!(changes[1], ModelChange::Reordered { parent: None });
    assert_eq!(
        changes[2],
        ModelChange::Removed {
            parent: None,
            keys: vec![fig]
        }
    );
    assert!(model.changes_since(model.revision()).is_empty());
}

#[test]
fn stale_consumers_receive_reset_from_bounded_log() {
    let mut model = StringListModel::new(1);
    let start = model.revision();
    for index in 0..300 {
        model.push(None, index.to_string()).unwrap();
    }
    assert_eq!(model.changes_since(start), [ModelChange::Reset]);
    assert_eq!(model.changes_since(model.revision() - 1).len(), 1);
}

#[test]
fn editing_validates_column_value_and_reports_display_change() {
    let mut model = StringListModel::from_strings(["a"]);
    let key = model.key(None, 0).unwrap();
    let revision = model.revision();
    model
        .set_data(key, 0, Role::Edit, Value::Text("b".into()))
        .unwrap();
    assert_eq!(model.data(key, 0, Role::Display), Value::Text("b".into()));
    assert_eq!(
        model.changes_since(revision),
        [ModelChange::DataChanged {
            key,
            roles: vec![Role::Edit, Role::Display]
        }]
    );
    assert_eq!(
        model.set_data(key, 0, Role::Edit, Value::Int(3)),
        Err(ModelError::InvalidValue)
    );
    assert_eq!(
        model.set_data(key, 1, Role::Edit, Value::Text("x".into())),
        Err(ModelError::InvalidColumn)
    );
    assert_eq!(
        model.set_data(ItemKey(999), 0, Role::Edit, Value::Text("x".into())),
        Err(ModelError::MissingItem)
    );
}

#[test]
fn tree_store_moves_reparents_and_removes_descendants() {
    let mut model: TreeStore<String> = TreeStore::new(1);
    let docs = model.push(None, "docs".into()).unwrap();
    let src = model.push(None, "src".into()).unwrap();
    let readme = model.push(Some(docs), "readme".into()).unwrap();
    let lib = model.push(Some(src), "lib".into()).unwrap();
    assert_eq!(model.parent(readme), Some(docs));
    assert_eq!(visible_keys(&model, |key| key == docs), [docs, readme, src]);
    assert_eq!(visible_keys(&model, |_| true), [docs, readme, src, lib]);
    model.move_item(lib, Some(docs), 0).unwrap();
    assert_eq!(model.parent(lib), Some(docs));
    assert_eq!(model.row(lib), Some(0));
    assert_eq!(
        model.move_item(docs, Some(lib), 0),
        Err(ModelError::InvalidRow)
    );
    model.remove(&[docs]).unwrap();
    assert_eq!(model.len(), 1);
    assert_eq!(model.row(readme), None);
    assert_eq!(
        model.insert(Some(docs), 0, ["x".into()]),
        Err(ModelError::MissingItem)
    );
}

#[test]
fn extended_selection_ranges_survive_reorder_and_removal() {
    let mut model = StringListModel::from_strings(["a", "b", "c", "d", "e"]);
    let order: Vec<ItemKey> = (0..5).map(|row| model.key(None, row).unwrap()).collect();
    let mut selection = Selection::new();
    let mode = SelectionMode::Extended;
    assert!(selection.apply(mode, SelectionCommand::Replace, order[1], &order));
    assert!(selection.apply(mode, SelectionCommand::ExtendTo, order[3], &order));
    assert_eq!(selection.ranges(&order), vec![(1..4)]);
    selection.apply(mode, SelectionCommand::Toggle, order[2], &order);
    assert_eq!(selection.ranges(&order), [1..2, 3..4]);
    // Reverse the model; the same items stay selected.
    model.sort_children_by(None, |text| std::cmp::Reverse(text.clone()));
    let reversed: Vec<ItemKey> = (0..5).map(|row| model.key(None, row).unwrap()).collect();
    assert_eq!(selection.ranges(&reversed), [1..2, 3..4]);
    assert!(selection.is_selected(order[1]) && selection.is_selected(order[3]));
    model.remove(&[order[3]]).unwrap();
    let removed = selection.retain_existing(|key| model.row(key).is_some());
    assert_eq!(removed, [order[3]]);
    assert_eq!(selection.len(), 1);
    assert!(!selection.apply(mode, SelectionCommand::CurrentOnly, order[0], &order));
    assert_eq!(selection.current(), Some(order[0]));
}

#[test]
fn an_indexed_range_looks_up_only_the_anchor_and_the_key() {
    let order: Vec<ItemKey> = (0..1_000_000).map(ItemKey).collect();
    let lookups = std::cell::Cell::new(0);
    let position = |key: ItemKey| {
        lookups.set(lookups.get() + 1);
        usize::try_from(key.0).ok()
    };
    let mut selection = Selection::new();
    let mode = SelectionMode::Extended;
    selection.apply_indexed(
        mode,
        SelectionCommand::Replace,
        order[999_990],
        &order,
        position,
    );
    assert!(selection.apply_indexed(
        mode,
        SelectionCommand::ExtendTo,
        order[999_995],
        &order,
        position,
    ));
    assert_eq!(lookups.get(), 2, "one lookup each for the anchor and key");
    assert_eq!(selection.len(), 6);
    assert_eq!(selection.anchor(), Some(order[999_990]));
    // A stale lookup past the order selects only the key.
    selection.apply_indexed(mode, SelectionCommand::ExtendTo, order[3], &order, |_| {
        Some(2_000_000)
    });
    assert_eq!(selection.keys().collect::<Vec<_>>(), [order[3]]);
}

#[test]
fn single_and_multi_modes() {
    let order = [ItemKey(1), ItemKey(2), ItemKey(3)];
    let mut single = Selection::new();
    single.apply(
        SelectionMode::Single,
        SelectionCommand::Toggle,
        order[0],
        &order,
    );
    single.apply(
        SelectionMode::Single,
        SelectionCommand::ExtendTo,
        order[2],
        &order,
    );
    assert_eq!(single.keys().collect::<Vec<_>>(), [order[2]]);
    let mut multi = Selection::new();
    multi.apply(
        SelectionMode::Multi,
        SelectionCommand::Replace,
        order[0],
        &order,
    );
    multi.apply(
        SelectionMode::Multi,
        SelectionCommand::Replace,
        order[2],
        &order,
    );
    multi.apply(
        SelectionMode::Multi,
        SelectionCommand::Replace,
        order[0],
        &order,
    );
    assert_eq!(multi.keys().collect::<Vec<_>>(), [order[2]]);
    let mut none = Selection::new();
    assert!(!none.apply(
        SelectionMode::None,
        SelectionCommand::Replace,
        order[0],
        &order
    ));
    assert!(none.select_all(&order));
    assert_eq!(none.len(), 3);
}

#[test]
fn values_sort_with_a_total_order() {
    use std::cmp::Ordering::*;
    assert_eq!(Value::Int(2).compare(&Value::Float(2.5)), Less);
    assert_eq!(Value::Float(f64::NAN).compare(&Value::Float(1.0)), Greater);
    assert_eq!(Value::None.compare(&Value::Text("a".into())), Less);
    assert_eq!(
        Value::Text("b".into()).compare(&Value::Text("a".into())),
        Greater
    );
}
