use choscorui_core::{Dirty, LayoutError, Point, Rect, ScrollViewport, Size, Tree, WheelUnit};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn line_scroll_moves_subtree_and_clips_hit_testing_to_viewport() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let viewport = tree.attach(root, rect(10.0, 10.0, 50.0, 40.0)).unwrap();
    let content = tree
        .attach(viewport, rect(10.0, 10.0, 200.0, 100.0))
        .unwrap();
    let item = tree.attach(content, rect(30.0, 20.0, 20.0, 20.0)).unwrap();
    tree.set_focus(Some(item)).unwrap();
    let mut scroll = ScrollViewport::new(
        viewport,
        content,
        Size::new(200.0, 100.0),
        Size::new(10.0, 20.0),
    )
    .unwrap();
    scroll
        .arrange(&mut tree, rect(10.0, 10.0, 50.0, 40.0))
        .unwrap();
    scroll
        .scroll_by(&mut tree, Point::new(2.0, 1.0), WheelUnit::Lines)
        .unwrap();
    assert_eq!(scroll.offset(), Point::new(20.0, 20.0));
    assert_eq!(tree.bounds(content), Some(rect(-10.0, -10.0, 200.0, 100.0)));
    assert_eq!(tree.bounds(item), Some(rect(10.0, 0.0, 20.0, 20.0)));
    assert_eq!(tree.hit_test(Point::new(15.0, 15.0)), Some(item));
    assert_eq!(tree.hit_test(Point::new(15.0, 5.0)), Some(root));
    assert_eq!(tree.focus(), Some(item));
}

#[test]
fn point_scroll_clamps_and_resize_restores_valid_offset() {
    let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 200.0));
    let root = tree.root();
    let viewport = tree.attach(root, rect(10.0, 10.0, 50.0, 40.0)).unwrap();
    let content = tree
        .attach(viewport, rect(10.0, 10.0, 200.0, 100.0))
        .unwrap();
    let mut scroll = ScrollViewport::new(
        viewport,
        content,
        Size::new(200.0, 100.0),
        Size::new(10.0, 20.0),
    )
    .unwrap();
    scroll
        .arrange(&mut tree, rect(10.0, 10.0, 50.0, 40.0))
        .unwrap();
    scroll
        .scroll_by(&mut tree, Point::new(500.0, 500.0), WheelUnit::Points)
        .unwrap();
    assert_eq!(scroll.offset(), Point::new(150.0, 60.0));
    assert_eq!(
        tree.bounds(content),
        Some(rect(-140.0, -50.0, 200.0, 100.0))
    );
    scroll
        .arrange(&mut tree, rect(0.0, 0.0, 250.0, 150.0))
        .unwrap();
    assert_eq!(scroll.offset(), Point::new(0.0, 0.0));
    assert_eq!(tree.bounds(content), Some(rect(0.0, 0.0, 200.0, 100.0)));
}

#[test]
fn invalid_scroll_does_not_change_offset_or_geometry() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let viewport = tree.attach(root, rect(0.0, 0.0, 50.0, 40.0)).unwrap();
    let content = tree.attach(viewport, rect(0.0, 0.0, 200.0, 100.0)).unwrap();
    let mut scroll = ScrollViewport::new(
        viewport,
        content,
        Size::new(200.0, 100.0),
        Size::new(10.0, 20.0),
    )
    .unwrap();
    scroll
        .arrange(&mut tree, rect(0.0, 0.0, 50.0, 40.0))
        .unwrap();
    assert_eq!(
        scroll.scroll_by(&mut tree, Point::new(f64::INFINITY, 1.0), WheelUnit::Points),
        Err(LayoutError::InvalidGeometry)
    );
    assert_eq!(scroll.offset(), Point::new(0.0, 0.0));
    assert_eq!(tree.bounds(content), Some(rect(0.0, 0.0, 200.0, 100.0)));
}

#[test]
fn shrinking_content_clamps_existing_offset_and_preserves_subtree_positions() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let viewport = tree.attach(root, rect(10.0, 10.0, 50.0, 40.0)).unwrap();
    let content = tree
        .attach(viewport, rect(10.0, 10.0, 200.0, 100.0))
        .unwrap();
    let item = tree.attach(content, rect(30.0, 20.0, 10.0, 10.0)).unwrap();
    let mut scroll = ScrollViewport::new(
        viewport,
        content,
        Size::new(200.0, 100.0),
        Size::new(10.0, 20.0),
    )
    .unwrap();
    scroll
        .arrange(&mut tree, rect(10.0, 10.0, 50.0, 40.0))
        .unwrap();
    scroll
        .scroll_by(&mut tree, Point::new(500.0, 500.0), WheelUnit::Points)
        .unwrap();
    scroll
        .set_content_size(&mut tree, Size::new(40.0, 30.0))
        .unwrap();
    assert_eq!(scroll.offset(), Point::new(0.0, 0.0));
    assert_eq!(tree.bounds(content), Some(rect(10.0, 10.0, 40.0, 30.0)));
    assert_eq!(tree.bounds(item), Some(rect(30.0, 20.0, 10.0, 10.0)));
}

#[test]
fn scrolling_requests_paint_and_semantics_without_remeasurement() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let viewport = tree.attach(root, rect(0.0, 0.0, 50.0, 40.0)).unwrap();
    let content = tree.attach(viewport, rect(0.0, 0.0, 200.0, 100.0)).unwrap();
    let mut scroll = ScrollViewport::new(
        viewport,
        content,
        Size::new(200.0, 100.0),
        Size::new(10.0, 20.0),
    )
    .unwrap();
    scroll
        .arrange(&mut tree, rect(0.0, 0.0, 50.0, 40.0))
        .unwrap();
    tree.take_dirty(root).unwrap();
    scroll
        .scroll_by(&mut tree, Point::new(0.0, 10.0), WheelUnit::Points)
        .unwrap();
    let dirty = tree.take_dirty(root).unwrap();
    assert!(dirty.contains(Dirty::PAINT));
    assert!(dirty.contains(Dirty::SEMANTICS));
    assert!(!dirty.contains(Dirty::LAYOUT));
}

#[test]
fn reveal_uses_minimum_scroll_to_keep_content_rect_visible() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let viewport = tree
        .attach(tree.root(), rect(10.0, 10.0, 50.0, 40.0))
        .unwrap();
    let content = tree
        .attach(viewport, rect(10.0, 10.0, 200.0, 120.0))
        .unwrap();
    let mut scroll = ScrollViewport::new(
        viewport,
        content,
        Size::new(200.0, 120.0),
        Size::new(10.0, 20.0),
    )
    .unwrap();
    scroll
        .arrange(&mut tree, rect(10.0, 10.0, 50.0, 40.0))
        .unwrap();
    scroll
        .reveal(&mut tree, rect(5.0, 80.0, 10.0, 10.0))
        .unwrap();
    assert_eq!(scroll.offset(), Point::new(0.0, 50.0));
    scroll
        .reveal(&mut tree, rect(5.0, 20.0, 10.0, 5.0))
        .unwrap();
    assert_eq!(scroll.offset(), Point::new(0.0, 20.0));
    scroll
        .reveal(&mut tree, rect(5.0, 22.0, 10.0, 2.0))
        .unwrap();
    assert_eq!(scroll.offset(), Point::new(0.0, 20.0));
}
