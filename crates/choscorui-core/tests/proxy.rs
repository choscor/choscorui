use choscorui_core::{
    ConcatModel, DirectoryState, EditableModel, FileSystemModel, ItemData, ItemKey, ItemModel,
    ListStore, MappedModel, MatchMode, ModelChange, Role, Selection, SelectionCommand,
    SelectionMode, SliceModel, SortFilterProxy, SortOrder, StringListModel, TextFilter, TreeStore,
    Value, visible_keys,
};
use std::time::Duration;

#[derive(Clone)]
struct City {
    name: &'static str,
    population: i64,
}

impl ItemData for City {
    fn data(&self, column: usize, role: Role) -> Value {
        match (column, role) {
            (0, Role::Display | Role::Sort) => Value::Text(self.name.into()),
            (1, Role::Display) => Value::Text(format!("{}k", self.population / 1000)),
            (1, Role::Sort) => Value::Int(self.population),
            _ => Value::None,
        }
    }
}

fn cities() -> ListStore<City> {
    let mut store = ListStore::new(2);
    for (name, population) in [
        ("Đà Nẵng", 1_200_000),
        ("Hà Nội", 8_400_000),
        ("Huế", 650_000),
        ("Cần Thơ", 1_250_000),
        ("hải phòng", 2_100_000),
    ] {
        store.push(None, City { name, population }).unwrap();
    }
    store
}

fn names(model: &dyn ItemModel) -> Vec<String> {
    (0..model.row_count(None))
        .map(|row| {
            model
                .data(model.key(None, row).unwrap(), 0, Role::Display)
                .to_display_string()
        })
        .collect()
}

#[test]
fn locale_sort_filter_and_selection_survive_source_edits() {
    let mut store = cities();
    let mut proxy = SortFilterProxy::new();
    proxy.set_sort(Some((0, SortOrder::Ascending)));
    assert!(proxy.sync(&store));
    // Collation ignores case and Vietnamese diacritics at the primary level.
    assert_eq!(
        names(&proxy.view(&store)),
        ["Cần Thơ", "Đà Nẵng", "Hà Nội", "hải phòng", "Huế"]
    );
    proxy.set_sort(Some((1, SortOrder::Descending)));
    proxy.sync(&store);
    assert_eq!(names(&proxy.view(&store))[0], "Hà Nội");

    let mut selection = Selection::new();
    let hue = store.key(None, 2).unwrap();
    let order = |proxy: &SortFilterProxy, store: &ListStore<City>| -> Vec<ItemKey> {
        visible_keys(&proxy.view(store), |_| false)
    };
    selection.apply(
        SelectionMode::Extended,
        SelectionCommand::Replace,
        hue,
        &order(&proxy, &store),
    );

    // Diacritic-insensitive filter keeps the selected key reachable.
    proxy.set_filter(Some(TextFilter::contains("hue")));
    proxy.sync(&store);
    assert_eq!(names(&proxy.view(&store)), ["Huế"]);
    assert_eq!(proxy.view(&store).row(hue), Some(0));

    // A source insertion that matches arrives as a keyed insert.
    let before = proxy.view(&store).revision();
    store
        .push(
            None,
            City {
                name: "Huế Mới",
                population: 1,
            },
        )
        .unwrap();
    assert!(proxy.sync(&store));
    let changes = proxy.view(&store).changes_since(before);
    assert!(matches!(
        changes[..],
        [ModelChange::Inserted { first_row: 1, .. }]
    ));
    assert!(selection.is_selected(hue));
    assert_eq!(proxy.view(&store).row(hue), Some(0));

    // Data changes outside the sort/filter roles skip the rebuild.
    let quiet = proxy.view(&store).revision();
    store.update(hue, vec![Role::ToolTip], |_| {}).unwrap();
    proxy.sync(&store);
    assert_eq!(
        proxy.view(&store).changes_since(quiet),
        vec![ModelChange::DataChanged {
            key: hue,
            roles: vec![Role::ToolTip]
        }]
    );
}

#[test]
fn tree_filter_keeps_ancestors_of_matches_and_prefix_mode() {
    let mut tree: TreeStore<String> = TreeStore::new(1);
    let docs = tree.push(None, "Documents".into()).unwrap();
    let music = tree.push(None, "Music".into()).unwrap();
    let report = tree.push(Some(docs), "Báo cáo.txt".into()).unwrap();
    tree.push(Some(music), "song.mp3".into()).unwrap();
    let mut proxy = SortFilterProxy::new();
    proxy.set_filter(Some(TextFilter {
        column: Some(0),
        pattern: "bao".into(),
        mode: MatchMode::Prefix,
    }));
    proxy.sync(&tree);
    let view = proxy.view(&tree);
    assert_eq!(visible_keys(&view, |_| true), vec![docs, report]);
    assert_eq!(view.parent(report), Some(docs));
    proxy.set_recursive(false);
    proxy.sync(&tree);
    assert_eq!(proxy.len(None), 0);
}

#[test]
fn mapped_concat_and_slice_adapters_keep_reversible_keys() {
    let store = cities();
    let upper = MappedModel::new(&store, |_, _, role, value| match (role, value) {
        (Role::Display, Value::Text(text)) => Value::Text(text.to_uppercase()),
        (_, value) => value,
    });
    assert_eq!(names(&upper)[1], "HÀ NỘI");

    let extra = StringListModel::from_strings(["Vinh", "Quy Nhơn"]);
    let concat = ConcatModel::new(vec![&store, &extra]);
    assert_eq!(concat.row_count(None), 7);
    let vinh = concat.key(None, 5).unwrap();
    assert_eq!(
        ConcatModel::source_key(vinh),
        (1, extra.key(None, 0).unwrap())
    );
    assert_eq!(concat.row(vinh), Some(5));
    assert_eq!(names(&concat)[6], "Quy Nhơn");

    // Composition: sort the concatenation.
    let mut proxy = SortFilterProxy::new();
    proxy.set_sort(Some((0, SortOrder::Ascending)));
    proxy.sync(&concat);
    assert_eq!(names(&proxy.view(&concat))[0], "Cần Thơ");
    assert_eq!(proxy.view(&concat).row(vinh), Some(6));

    let numbers: Vec<u32> = (0..100_000).collect();
    let slice = SliceModel::new(&numbers, 1, |n: &u32, _, role| match role {
        Role::Display => Value::Text(n.to_string()),
        Role::Sort => Value::Int(i64::from(*n)),
        _ => Value::None,
    });
    assert_eq!(slice.row_count(None), 100_000);
    assert_eq!(
        slice
            .data(ItemKey(99_999), 0, Role::Display)
            .to_display_string(),
        "99999"
    );
    assert_eq!(slice.data(ItemKey(100_000), 0, Role::Display), Value::None);
}

#[test]
fn filesystem_model_lists_lazily_and_refresh_keeps_keys() {
    let root = std::env::temp_dir().join(format!("choscorui-fs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("thư mục")).unwrap();
    std::fs::write(root.join("a.txt"), b"hello").unwrap();
    std::fs::write(root.join(".hidden"), b"").unwrap();

    let mut model = FileSystemModel::new(&root);
    assert!(model.can_fetch_more(None));
    assert_eq!(model.row_count(None), 0, "nothing read before fetch");
    model.fetch_more(None);
    assert_eq!(model.state(None), DirectoryState::Loading);
    model.wait(Duration::from_secs(5));
    assert_eq!(model.state(None), DirectoryState::Loaded);
    assert_eq!(names(&model), ["thư mục", "a.txt"], "folders first");
    let folder = model.key(None, 0).unwrap();
    let file = model.key(None, 1).unwrap();
    assert!(model.has_children(folder));
    assert!(model.can_fetch_more(Some(folder)));
    assert_eq!(
        model.data(file, 1, Role::Display).to_display_string(),
        "5 B"
    );

    std::fs::write(root.join("b.txt"), b"").unwrap();
    std::fs::remove_dir(root.join("thư mục")).unwrap();
    model.refresh(None).unwrap();
    model.wait(Duration::from_secs(5));
    assert_eq!(names(&model), ["a.txt", "b.txt"]);
    assert_eq!(
        model.key(None, 0),
        Some(file),
        "surviving entry keeps its key"
    );

    let missing = FileSystemModel::new(root.join("missing"));
    let mut missing = missing;
    missing.fetch_more(None);
    missing.wait(Duration::from_secs(5));
    assert!(matches!(missing.state(None), DirectoryState::Failed(_)));
    std::fs::remove_dir_all(&root).unwrap();
}
