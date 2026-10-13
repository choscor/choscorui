use choscorui_compose::view::{Cx, Env, ForEach, Text, VStack, View, ViewError, ViewHost};
use choscorui_compose::{
    ComposeError, Element, Key, KeyedChild, Properties, Property, ReconcileReport, Reconciler,
    Runtime, plan_keyed,
};
use choscorui_controls::{ControlKind, ControlValue, EstimatedText};
use choscorui_core::{Catalog, DesignTokens, Locale, Point, Rect, Size, Tree};
use std::rc::Rc;

fn bounds(x: f64) -> Rect {
    Rect::new(Point::new(x, 0.0), Size::new(10.0, 10.0))
}

fn element(key: &str, x: f64) -> Element {
    Element::new(Key::new(key), bounds(x))
}

#[test]
fn keyed_reconciliation_preserves_identity_across_reorder_and_reparent() {
    let mut tree = Tree::new(bounds(0.0));
    let mut composition = Reconciler::new();
    composition
        .reconcile(
            &mut tree,
            &[
                element("left", 1.0).with_child(element("field", 2.0).focusable(true)),
                element("right", 3.0),
            ],
        )
        .unwrap();
    let field = composition.node_id(&Key::new("field")).unwrap();
    let right = composition.node_id(&Key::new("right")).unwrap();
    tree.set_focusable(field, true).unwrap();
    tree.set_focus(Some(field)).unwrap();

    let report = composition
        .reconcile(
            &mut tree,
            &[
                element("right", 30.0).with_child(element("field", 20.0).focusable(true)),
                element("left", 10.0),
            ],
        )
        .unwrap();

    assert_eq!(composition.node_id(&Key::new("field")), Some(field));
    assert_eq!(tree.parent(field), Some(right));
    assert_eq!(tree.focus(), Some(field));
    assert_eq!(tree.bounds(field), Some(bounds(20.0)));
    assert_eq!(report.created, 0);
    assert_eq!(report.reused, 3);
    assert!(report.moved >= 2);
}

#[test]
fn removal_prunes_subtrees_and_new_keys_never_reuse_ids() {
    let mut tree = Tree::new(bounds(0.0));
    let mut composition = Reconciler::new();
    composition
        .reconcile(
            &mut tree,
            &[element("old", 1.0).with_child(element("leaf", 2.0))],
        )
        .unwrap();
    let old = composition.node_id(&Key::new("old")).unwrap();
    let report = composition
        .reconcile(&mut tree, &[element("new", 3.0)])
        .unwrap();

    let new = composition.node_id(&Key::new("new")).unwrap();
    assert_ne!(old, new);
    assert!(!tree.contains(old));
    assert_eq!(report.removed, 2);
}

#[test]
fn invalid_descriptions_leave_tree_and_key_map_unchanged() {
    let mut tree = Tree::new(bounds(0.0));
    let mut composition = Reconciler::new();
    composition
        .reconcile(&mut tree, &[element("kept", 1.0)])
        .unwrap();
    let kept = composition.node_id(&Key::new("kept")).unwrap();
    let original_children = tree.children(tree.root()).unwrap().to_vec();

    let error = composition
        .reconcile(
            &mut tree,
            &[element("duplicate", 2.0), element("duplicate", 3.0)],
        )
        .unwrap_err();

    assert_eq!(error, ComposeError::DuplicateKey(Key::new("duplicate")));
    assert_eq!(composition.node_id(&Key::new("kept")), Some(kept));
    assert_eq!(tree.children(tree.root()).unwrap(), original_children);
}

#[test]
fn invalid_geometry_is_rejected_before_existing_nodes_change() {
    let mut tree = Tree::new(bounds(0.0));
    let mut composition = Reconciler::new();
    composition
        .reconcile(&mut tree, &[element("kept", 1.0)])
        .unwrap();
    let kept = composition.node_id(&Key::new("kept")).unwrap();

    let error = composition
        .reconcile(
            &mut tree,
            &[Element::new(
                Key::new("bad"),
                Rect::new(Point::new(f64::NAN, 0.0), Size::new(1.0, 1.0)),
            )],
        )
        .unwrap_err();

    assert_eq!(error, ComposeError::InvalidBounds(Key::new("bad")));
    assert_eq!(tree.bounds(kept), Some(bounds(1.0)));
}

#[test]
fn keyed_plan_reuses_creates_removes_and_counts_minimal_moves() {
    let old = ["a", "b", "c", "d"];
    let new = ["d", "a", "b", "e", "a"];
    let plan = plan_keyed(&old, &new, |_, _| true);

    assert_eq!(
        plan.children,
        vec![
            KeyedChild {
                key: "d",
                index: 0,
                reuse: Some(3),
            },
            KeyedChild {
                key: "a",
                index: 1,
                reuse: Some(0),
            },
            KeyedChild {
                key: "b",
                index: 2,
                reuse: Some(1),
            },
            KeyedChild {
                key: "e",
                index: 3,
                reuse: None,
            },
        ]
    );
    assert_eq!(plan.removed, vec![2]);
    assert_eq!(plan.duplicates, vec!["a"]);
    // Only "d" moves; "a" and "b" keep their relative order.
    assert_eq!(
        plan.report,
        ReconcileReport {
            created: 1,
            reused: 3,
            moved: 1,
            removed: 1,
            updated: 0,
        }
    );
}

#[test]
fn keyed_plan_creates_again_when_the_old_child_is_not_reusable() {
    let plan = plan_keyed(&[1, 2], &[1, 2], |old, _| old != 1);

    assert_eq!(plan.children[1].reuse, None);
    assert_eq!(plan.removed, vec![1]);
    assert_eq!(plan.report.created, 1);
    assert_eq!(plan.report.reused, 1);
}

#[test]
fn a_changed_kind_gives_a_new_node_and_changed_properties_keep_it() {
    let mut tree = Tree::new(bounds(0.0));
    let mut composition = Reconciler::new();
    let label = |text: &str| Properties::new().with("label", ControlValue::Text(text.into()));
    composition
        .reconcile(
            &mut tree,
            &[
                element("action", 1.0)
                    .kind(ControlKind::Button)
                    .properties(label("Save")),
                element("toggle", 2.0)
                    .kind(ControlKind::Checkbox)
                    .with_child(element("inner", 3.0)),
            ],
        )
        .unwrap();
    let action = composition.node_id(&Key::new("action")).unwrap();
    let toggle = composition.node_id(&Key::new("toggle")).unwrap();
    let inner = composition.node_id(&Key::new("inner")).unwrap();

    let report = composition
        .reconcile(
            &mut tree,
            &[
                element("action", 1.0)
                    .kind(ControlKind::Button)
                    .properties(label("Save all")),
                element("toggle", 2.0)
                    .kind(ControlKind::Switch)
                    .with_child(element("inner", 3.0)),
            ],
        )
        .unwrap();

    assert_eq!(composition.node_id(&Key::new("action")), Some(action));
    assert_eq!(
        composition
            .properties(&Key::new("action"))
            .and_then(|properties| properties.get("label")),
        Some(&ControlValue::Text("Save all".into()))
    );
    let switch = composition.node_id(&Key::new("toggle")).unwrap();
    assert_ne!(switch, toggle);
    assert!(!tree.contains(toggle));
    assert_eq!(
        composition.kind(&Key::new("toggle")),
        Some(ControlKind::Switch)
    );
    // The child of the replaced node keeps its node under the new node.
    assert_eq!(composition.node_id(&Key::new("inner")), Some(inner));
    assert_eq!(tree.parent(inner), Some(switch));
    assert_eq!(
        tree.children(tree.root()).unwrap(),
        [action, switch].as_slice()
    );
    assert_eq!(report.created, 1);
    assert_eq!(report.removed, 1);
    assert_eq!(report.reused, 2);
    assert_eq!(report.updated, 1);
    assert_eq!(report.moved, 1);
}

#[test]
fn unchanged_descriptions_report_no_work() {
    let mut tree = Tree::new(bounds(0.0));
    let mut composition = Reconciler::new();
    let elements = [
        element("one", 1.0).kind(ControlKind::Label),
        element("two", 2.0),
    ];
    composition.reconcile(&mut tree, &elements).unwrap();

    let report = composition.reconcile(&mut tree, &elements).unwrap();

    assert_eq!(
        report,
        ReconcileReport {
            reused: 2,
            ..ReconcileReport::default()
        }
    );
}

type Items = Property<Vec<(u32, &'static str)>>;

/// A host that shows two labels for each item of `items`.
fn list_host(items: Items, runtime: Runtime) -> ViewHost {
    struct List(Items);
    impl View for List {
        fn body(&self, _cx: &mut Cx) -> impl View {
            let items = self.0.clone();
            VStack::new(ForEach::new(
                move || items.get(),
                |item| item.0,
                |item| {
                    (
                        Text::new(format!("{} title", item.1)),
                        Text::new(format!("{} detail", item.1)),
                    )
                },
            ))
        }
    }
    let en = Locale::parse("en").unwrap();
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        Env::new(Rc::new(Catalog::new()), en),
        runtime,
        Rc::new(EstimatedText),
    );
    host.mount(List(items));
    host.layout();
    host
}

#[test]
fn for_each_keeps_multi_node_items_and_mounts_changed_values_again() {
    let runtime = Runtime::new();
    let items = runtime.property(vec![(1, "a"), (2, "b"), (3, "c")]);
    let mut host = list_host(items.clone(), runtime);
    let a = host.find("a title").unwrap();
    let b = host.find("b title").unwrap();
    let b_detail = host.find("b detail").unwrap();

    // Key 1 keeps its key with a new value; key 2 moves to the front.
    items.set(vec![(2, "b"), (1, "x"), (3, "c")]);
    host.flush();
    host.layout();

    assert_eq!(host.find("b title"), Some(b));
    assert_eq!(host.find("b detail"), Some(b_detail));
    assert_eq!(host.find("a title"), None);
    assert!(host.find("x title").is_some_and(|x| x != a));
    let parent = host.tree().parent(b).unwrap();
    let children = host.tree().children(parent).unwrap().to_vec();
    let order: Vec<String> = children
        .into_iter()
        .filter_map(|node| host.semantic(node).map(|semantic| semantic.name))
        .collect();
    assert_eq!(
        order,
        [
            "b title", "b detail", "x title", "x detail", "c title", "c detail"
        ]
    );
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn for_each_reports_duplicate_keys_and_keeps_the_first_item() {
    let runtime = Runtime::new();
    let items = runtime.property(vec![(1, "a")]);
    let mut host = list_host(items.clone(), runtime);

    items.set(vec![(1, "a"), (2, "b"), (2, "c")]);
    host.flush();

    assert!(host.find("b title").is_some());
    assert_eq!(host.find("c title"), None);
    assert_eq!(
        host.take_errors(),
        vec![ViewError::DuplicateKey("2".to_owned())]
    );
}
