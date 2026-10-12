use choscorui_core::{
    Constraints, EngineError, GridPlacement, Insets, ItemAlign, LayoutDirection, LayoutEngine,
    LayoutKind, LayoutStyle, Length, MainAlign, Measured, NodeId, Point, Rect, Size, Track, Tree,
};
use std::collections::HashMap;

/// Deterministic text stand-in: each character is `7 * scale` wide, lines
/// are `16 * scale` tall with the baseline at 80%, and text wraps at the
/// offered maximum width.
struct FakeText {
    texts: HashMap<NodeId, String>,
    scale: f64,
    calls: usize,
}

impl FakeText {
    fn new(scale: f64) -> Self {
        Self {
            texts: HashMap::new(),
            scale,
            calls: 0,
        }
    }

    fn measure(&mut self, node: NodeId, constraints: Constraints) -> Measured {
        self.calls += 1;
        let Some(text) = self.texts.get(&node) else {
            return Measured::new(constraints.min);
        };
        let char_w = 7.0 * self.scale;
        let line_h = 16.0 * self.scale;
        let natural = text.chars().count() as f64 * char_w;
        let width = natural.min(constraints.max.width).max(char_w.min(natural));
        let per_line = (width / char_w).floor().max(1.0);
        let lines = (text.chars().count() as f64 / per_line).ceil().max(1.0);
        Measured {
            size: constraints.constrain(Size::new(width, lines * line_h)),
            baseline: Some(line_h * 0.8),
        }
    }
}

fn setup() -> (Tree, LayoutEngine, FakeText) {
    let tree = Tree::new(Rect::from_xywh(0.0, 0.0, 800.0, 600.0));
    let mut engine = LayoutEngine::new();
    engine.set_strict(false);
    (tree, engine, FakeText::new(1.0))
}

fn add(tree: &mut Tree, engine: &mut LayoutEngine, parent: NodeId, style: LayoutStyle) -> NodeId {
    let node = tree.attach(parent, Rect::default()).unwrap();
    engine.set_style(tree, node, style).unwrap();
    node
}

fn run(tree: &mut Tree, engine: &mut LayoutEngine, text: &mut FakeText, size: Size) {
    let root = tree.root();
    let mut measure = |node, c, _| text.measure(node, c);
    engine
        .layout(
            tree,
            root,
            Rect::new(Point::new(0.0, 0.0), size),
            &mut measure,
        )
        .unwrap();
}

#[test]
fn nested_flexible_rows_and_columns_share_space_by_weight() {
    let (mut tree, mut engine, mut text) = setup();
    let root = tree.root();
    engine
        .set_style(
            &tree,
            root,
            LayoutStyle::column(10.0).with_padding(Insets::all(5.0)),
        )
        .unwrap();
    let toolbar = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf().with_size(Length::Auto, Length::Points(30.0)),
    );
    let body = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::row(4.0).with_grow(1.0),
    );
    let sidebar = add(
        &mut tree,
        &mut engine,
        body,
        LayoutStyle::leaf().with_size(Length::Points(120.0), Length::Auto),
    );
    let main = add(
        &mut tree,
        &mut engine,
        body,
        LayoutStyle::leaf().with_grow(2.0),
    );
    let inspector = add(
        &mut tree,
        &mut engine,
        body,
        LayoutStyle::leaf()
            .with_grow(1.0)
            .with_max(Size::new(100.0, f64::INFINITY)),
    );
    run(&mut tree, &mut engine, &mut text, Size::new(800.0, 600.0));
    assert_eq!(
        tree.bounds(toolbar).unwrap(),
        Rect::from_xywh(5.0, 5.0, 790.0, 30.0)
    );
    assert_eq!(
        tree.bounds(body).unwrap(),
        Rect::from_xywh(5.0, 45.0, 790.0, 550.0)
    );
    assert_eq!(tree.bounds(sidebar).unwrap().size.width, 120.0);
    // 790 - 120 - 8 = 662; inspector caps at 100, main takes the rest.
    assert_eq!(tree.bounds(inspector).unwrap().size.width, 100.0);
    assert_eq!(
        tree.bounds(main).unwrap(),
        Rect::from_xywh(129.0, 45.0, 562.0, 550.0)
    );
}

#[test]
fn tiny_window_shrinks_to_minimums_then_reports_overflow() {
    let (mut tree, mut engine, mut text) = setup();
    let root = tree.root();
    engine
        .set_style(&tree, root, LayoutStyle::row(0.0))
        .unwrap();
    let a = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf()
            .with_grow(1.0)
            .with_min(Size::new(50.0, 0.0)),
    );
    let label = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf()
            .with_shrink(true)
            .with_min(Size::new(20.0, 0.0)),
    );
    text.texts.insert(label, "a fairly long label".into()); // 133 wide
    run(&mut tree, &mut engine, &mut text, Size::new(100.0, 20.0));
    assert_eq!(tree.bounds(a).unwrap().size.width, 50.0);
    assert_eq!(tree.bounds(label).unwrap().size.width, 50.0);
    assert!(engine.stats().overflowed.is_empty());
    run(&mut tree, &mut engine, &mut text, Size::new(40.0, 20.0));
    assert_eq!(engine.stats().overflowed, [root]);
    // Strict (debug default) fails predictably instead of overflowing.
    engine.set_strict(true);
    engine.invalidate_all();
    let mut measure = |node, c, _| text.measure(node, c);
    assert_eq!(
        engine
            .layout(
                &mut tree,
                root,
                Rect::from_xywh(0.0, 0.0, 40.0, 20.0),
                &mut measure
            )
            .err(),
        Some(EngineError::Unsatisfiable(root))
    );
}

#[test]
fn huge_window_keeps_fixed_content_and_aligns_main_axis() {
    let (mut tree, mut engine, mut text) = setup();
    let root = tree.root();
    let mut style = LayoutStyle::row(10.0);
    if let LayoutKind::Stack {
        main_align,
        cross_align,
        ..
    } = &mut style.kind
    {
        *main_align = MainAlign::Center;
        *cross_align = ItemAlign::Center;
    }
    engine.set_style(&tree, root, style).unwrap();
    let ok = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf().with_size(Length::Points(80.0), Length::Points(24.0)),
    );
    let cancel = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf().with_size(Length::Points(80.0), Length::Points(24.0)),
    );
    run(
        &mut tree,
        &mut engine,
        &mut text,
        Size::new(100_000.0, 50_000.0),
    );
    let ok_rect = tree.bounds(ok).unwrap();
    assert_eq!(ok_rect, Rect::from_xywh(49_915.0, 24_988.0, 80.0, 24.0));
    assert_eq!(tree.bounds(cancel).unwrap().origin.x, 50_005.0);
}

#[test]
fn long_labels_wrap_at_the_offered_width() {
    let (mut tree, mut engine, mut text) = setup();
    let root = tree.root();
    engine
        .set_style(&tree, root, LayoutStyle::column(0.0))
        .unwrap();
    let paragraph = add(&mut tree, &mut engine, root, LayoutStyle::leaf());
    text.texts.insert(paragraph, "x".repeat(100)); // 700 natural
    run(&mut tree, &mut engine, &mut text, Size::new(140.0, 600.0));
    // 20 characters per line → 5 lines.
    assert_eq!(
        tree.bounds(paragraph).unwrap(),
        Rect::from_xywh(0.0, 0.0, 140.0, 80.0)
    );
}

#[test]
fn rtl_mirrors_rows_and_grids_and_is_inherited() {
    let (mut tree, mut engine, mut text) = setup();
    let root = tree.root();
    engine
        .set_style(
            &tree,
            root,
            LayoutStyle::column(0.0).with_direction(LayoutDirection::Rtl),
        )
        .unwrap();
    let row = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::row(10.0).with_size(Length::Auto, Length::Points(20.0)),
    );
    let first = add(
        &mut tree,
        &mut engine,
        row,
        LayoutStyle::leaf().with_size(Length::Points(50.0), Length::Auto),
    );
    let second = add(
        &mut tree,
        &mut engine,
        row,
        LayoutStyle::leaf().with_size(Length::Points(30.0), Length::Auto),
    );
    let ltr_row = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::row(0.0)
            .with_size(Length::Auto, Length::Points(20.0))
            .with_direction(LayoutDirection::Ltr),
    );
    let ltr_child = add(
        &mut tree,
        &mut engine,
        ltr_row,
        LayoutStyle::leaf().with_size(Length::Points(50.0), Length::Auto),
    );
    let form = add(&mut tree, &mut engine, root, LayoutStyle::form(1, 8.0, 4.0));
    let label = add(&mut tree, &mut engine, form, LayoutStyle::form_label(0));
    let field = add(&mut tree, &mut engine, form, LayoutStyle::form_field(0));
    text.texts.insert(label, "Name".into()); // 28 wide
    run(&mut tree, &mut engine, &mut text, Size::new(400.0, 300.0));
    assert_eq!(tree.bounds(first).unwrap().origin.x, 350.0);
    assert_eq!(tree.bounds(second).unwrap().origin.x, 310.0);
    assert_eq!(tree.bounds(ltr_child).unwrap().origin.x, 0.0);
    // RTL form: label column on the right, field fills the left.
    assert_eq!(tree.bounds(label).unwrap().origin.x, 372.0);
    assert_eq!(tree.bounds(field).unwrap().origin.x, 0.0);
    assert_eq!(tree.bounds(field).unwrap().size.width, 364.0);
}

#[test]
fn form_rows_align_baselines_and_labels_trail_toward_fields() {
    let (mut tree, mut engine, mut text) = setup();
    let root = tree.root();
    engine
        .set_style(&tree, root, LayoutStyle::form(2, 8.0, 6.0))
        .unwrap();
    let short = add(&mut tree, &mut engine, root, LayoutStyle::form_label(0));
    let tall_field = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::form_field(0).with_padding(Insets {
            left: 0.0,
            top: 10.0,
            right: 0.0,
            bottom: 4.0,
        }),
    );
    let long = add(&mut tree, &mut engine, root, LayoutStyle::form_label(1));
    let field = add(&mut tree, &mut engine, root, LayoutStyle::form_field(1));
    text.texts.insert(short, "Id".into());
    text.texts.insert(tall_field, "value".into());
    text.texts.insert(long, "Description".into()); // 77 wide → column width
    text.texts.insert(field, "v".into());
    run(&mut tree, &mut engine, &mut text, Size::new(400.0, 200.0));
    let short_rect = tree.bounds(short).unwrap();
    assert_eq!(
        short_rect.origin.x,
        77.0 - 14.0,
        "label trails toward the field"
    );
    assert_eq!(tree.bounds(long).unwrap().origin.x, 0.0);
    assert_eq!(tree.bounds(field).unwrap().origin.x, 85.0);
    // Field baseline = 10 + 12.8; label baseline = 12.8 → label moves down 10.
    assert_eq!(short_rect.origin.y, 10.0);
    assert_eq!(tree.bounds(tall_field).unwrap().origin.y, 0.0);
    // Row 0 height 30 + gap 6.
    assert_eq!(tree.bounds(long).unwrap().origin.y, 36.0);
}

#[test]
fn row_baseline_alignment_lines_up_text_of_different_sizes() {
    let (mut tree, mut engine, mut text) = setup();
    let root = tree.root();
    let mut style = LayoutStyle::row(4.0);
    if let LayoutKind::Stack { cross_align, .. } = &mut style.kind {
        *cross_align = ItemAlign::Baseline;
    }
    engine.set_style(&tree, root, style).unwrap();
    let small = add(&mut tree, &mut engine, root, LayoutStyle::leaf());
    let big = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf().with_padding(Insets {
            left: 0.0,
            top: 8.0,
            right: 0.0,
            bottom: 0.0,
        }),
    );
    text.texts.insert(small, "a".into());
    text.texts.insert(big, "b".into());
    run(&mut tree, &mut engine, &mut text, Size::new(200.0, 100.0));
    let small_baseline = tree.bounds(small).unwrap().origin.y + 12.8;
    let big_baseline = tree.bounds(big).unwrap().origin.y + 8.0 + 12.8;
    assert_eq!(small_baseline, big_baseline);
}

#[test]
fn grid_auto_tracks_size_to_content_and_spans_widen_them() {
    let (mut tree, mut engine, mut text) = setup();
    let root = tree.root();
    engine
        .set_style(
            &tree,
            root,
            LayoutStyle::grid(
                vec![
                    Track::Auto,
                    Track::Auto,
                    Track::Flex {
                        weight: 1.0,
                        min: 0.0,
                        max: f64::INFINITY,
                    },
                ],
                vec![Track::Fixed(20.0), Track::Auto],
                10.0,
            ),
        )
        .unwrap();
    let a = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf().with_cell(GridPlacement::cell(0, 0)),
    );
    let b = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf().with_cell(GridPlacement::cell(0, 1)),
    );
    let span = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf().with_cell(GridPlacement::span(1, 0, 1, 2)),
    );
    let rest = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf().with_cell(GridPlacement::cell(1, 2)),
    );
    text.texts.insert(a, "aa".into()); // 14
    text.texts.insert(b, "bbbb".into()); // 28
    text.texts.insert(span, "s".repeat(12)); // 84 > 14 + 10 + 28
    run(&mut tree, &mut engine, &mut text, Size::new(300.0, 100.0));
    // Spanning cell adds 32 split across both auto columns: 30, 44.
    assert_eq!(tree.bounds(a).unwrap().size.width, 30.0);
    assert_eq!(
        tree.bounds(b).unwrap(),
        Rect::from_xywh(40.0, 0.0, 44.0, 16.0)
    );
    assert_eq!(
        tree.bounds(span).unwrap(),
        Rect::from_xywh(0.0, 30.0, 84.0, 16.0)
    );
    assert_eq!(tree.bounds(rest).unwrap().origin.x, 94.0);
    assert_eq!(tree.bounds(rest).unwrap().size.width, 206.0);
    let bad = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf().with_cell(GridPlacement::cell(5, 0)),
    );
    let mut measure = |node, c, _| text.measure(node, c);
    assert_eq!(
        engine
            .layout(
                &mut tree,
                root,
                Rect::from_xywh(0.0, 0.0, 300.0, 100.0),
                &mut measure
            )
            .err(),
        Some(EngineError::InvalidPlacement(bad))
    );
}

#[test]
fn dynamic_type_scaling_remeasures_only_after_invalidation() {
    let (mut tree, mut engine, mut text) = setup();
    let root = tree.root();
    engine
        .set_style(&tree, root, LayoutStyle::column(0.0))
        .unwrap();
    let title = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf().with_align(ItemAlign::Start),
    );
    let body = add(
        &mut tree,
        &mut engine,
        root,
        LayoutStyle::leaf().with_align(ItemAlign::Start),
    );
    text.texts.insert(title, "Title".into());
    text.texts.insert(body, "Body".into());
    run(&mut tree, &mut engine, &mut text, Size::new(400.0, 300.0));
    assert_eq!(tree.bounds(title).unwrap().size, Size::new(35.0, 16.0));
    let calls = text.calls;
    // Unchanged inputs: a second layout is served entirely from cache.
    run(&mut tree, &mut engine, &mut text, Size::new(400.0, 300.0));
    assert_eq!(text.calls, calls);
    assert!(engine.stats().cache_hits > 0);
    // Larger text without invalidation keeps the stale cached size...
    text.scale = 1.5;
    run(&mut tree, &mut engine, &mut text, Size::new(400.0, 300.0));
    assert_eq!(tree.bounds(title).unwrap().size, Size::new(35.0, 16.0));
    // ...and a metric change invalidates everything.
    engine.invalidate_all();
    run(&mut tree, &mut engine, &mut text, Size::new(400.0, 300.0));
    assert_eq!(tree.bounds(title).unwrap().size, Size::new(52.5, 24.0));
    assert_eq!(tree.bounds(body).unwrap().origin.y, 24.0);
    // Changing one label's content invalidates only that path.
    text.texts.insert(body, "Longer body".into());
    engine.invalidate(&tree, body);
    let before = text.calls;
    run(&mut tree, &mut engine, &mut text, Size::new(400.0, 300.0));
    assert!(text.calls - before <= 2, "title stays cached");
    assert_eq!(tree.bounds(body).unwrap().size.width, 11.0 * 7.0 * 1.5);
}

#[test]
fn scroll_content_measures_unbounded_and_offset_clamps() {
    let (mut tree, mut engine, mut text) = setup();
    let root = tree.root();
    engine
        .set_style(
            &tree,
            root,
            LayoutStyle {
                kind: LayoutKind::Scroll {
                    axis: choscorui_core::Axis::Vertical,
                },
                ..LayoutStyle::default()
            },
        )
        .unwrap();
    let list = add(&mut tree, &mut engine, root, LayoutStyle::column(0.0));
    let rows: Vec<NodeId> = (0..50)
        .map(|_| {
            add(
                &mut tree,
                &mut engine,
                list,
                LayoutStyle::leaf().with_size(Length::Auto, Length::Points(20.0)),
            )
        })
        .collect();
    run(&mut tree, &mut engine, &mut text, Size::new(200.0, 100.0));
    assert_eq!(engine.content_size(root), Some(Size::new(200.0, 1000.0)));
    engine.set_scroll_offset(root, Point::new(0.0, 5000.0));
    let calls = text.calls;
    run(&mut tree, &mut engine, &mut text, Size::new(200.0, 100.0));
    assert_eq!(text.calls, calls, "scrolling does not remeasure");
    assert_eq!(engine.scroll_offset(root), Point::new(0.0, 900.0));
    assert_eq!(tree.bounds(rows[45]).unwrap().origin.y, 0.0);
}

#[test]
fn styles_and_bounds_are_validated() {
    let (mut tree, mut engine, _) = setup();
    let root = tree.root();
    assert_eq!(
        engine.set_style(&tree, root, LayoutStyle::row(-1.0)),
        Err(EngineError::InvalidStyle)
    );
    assert_eq!(
        engine.set_style(
            &tree,
            root,
            LayoutStyle::leaf()
                .with_min(Size::new(10.0, 0.0))
                .with_max(Size::new(5.0, 5.0))
        ),
        Err(EngineError::InvalidStyle)
    );
    let child = tree.attach(root, Rect::default()).unwrap();
    tree.detach(child).unwrap();
    assert_eq!(
        engine.set_style(&tree, child, LayoutStyle::leaf()),
        Err(EngineError::MissingNode(child))
    );
    engine.prune(&tree);
    let mut nested = |_node, _c, _d| Measured::new(Size::ZERO);
    assert!(
        engine
            .measure(&tree, root, Constraints::UNBOUNDED, &mut nested)
            .is_ok()
    );
    assert_eq!(
        engine
            .layout(
                &mut tree,
                root,
                Rect::from_xywh(0.0, 0.0, f64::NAN, 1.0),
                &mut nested
            )
            .err(),
        Some(EngineError::InvalidBounds)
    );
}

#[test]
fn layout_places_only_what_changed() {
    let (mut tree, mut engine, mut text) = setup();
    let root = tree.root();
    engine
        .set_style(&tree, root, LayoutStyle::column(4.0))
        .unwrap();
    let rows: Vec<NodeId> = (0..100)
        .map(|index| {
            let row = add(&mut tree, &mut engine, root, LayoutStyle::row(4.0));
            let label = add(&mut tree, &mut engine, row, LayoutStyle::leaf());
            text.texts.insert(label, format!("Row {index}"));
            label
        })
        .collect();
    run(&mut tree, &mut engine, &mut text, Size::new(400.0, 3000.0));
    assert_eq!(engine.placed().len(), 201);
    // Nothing changed, so nothing moves.
    run(&mut tree, &mut engine, &mut text, Size::new(400.0, 3000.0));
    assert!(engine.placed().is_empty());
    // A longer label in the last row moves only that row's nodes and the
    // root.
    text.texts.insert(rows[99], "Row ninety-nine".into());
    engine.invalidate(&tree, rows[99]);
    run(&mut tree, &mut engine, &mut text, Size::new(400.0, 3000.0));
    assert!(engine.placed().len() <= 3, "{:?}", engine.placed());
    assert_eq!(tree.bounds(rows[99]).unwrap().size.width, 105.0);
    // A metric change arranges everything again.
    engine.invalidate_all();
    run(&mut tree, &mut engine, &mut text, Size::new(400.0, 3000.0));
    assert_eq!(engine.placed().len(), 201);
}
