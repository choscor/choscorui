use choscorui_core::{
    Axis, CrossAlign, Insets, LayoutDirection, LayoutError, LinearLayout, Point, Rect, Size, Tree,
};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn row_measures_minimum_and_arranges_fixed_and_weighted_flex() {
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 100.0));
    let root = tree.root();
    let fixed = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let first = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let second = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let mut row = LinearLayout::new(
        Axis::Horizontal,
        LayoutDirection::Ltr,
        Insets::all(10.0),
        10.0,
        CrossAlign::Center,
    )
    .unwrap();
    row.add_fixed(fixed, 100.0, 20.0).unwrap();
    row.add_flex(first, 1.0, 50.0, 200.0, 30.0).unwrap();
    row.add_flex(second, 2.0, 0.0, f64::INFINITY, 40.0).unwrap();

    assert_eq!(row.measure(), Size::new(190.0, 60.0));
    row.apply(&mut tree, rect(0.0, 0.0, 400.0, 100.0)).unwrap();
    assert_eq!(tree.bounds(fixed), Some(rect(10.0, 40.0, 100.0, 20.0)));
    assert_eq!(tree.bounds(first), Some(rect(120.0, 35.0, 120.0, 30.0)));
    assert_eq!(tree.bounds(second), Some(rect(250.0, 30.0, 140.0, 40.0)));
}

#[test]
fn rtl_row_mirrors_order_without_changing_identity_or_focus() {
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 100.0));
    let root = tree.root();
    let fixed = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let flex = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    tree.set_focus(Some(flex)).unwrap();
    let mut row = LinearLayout::new(
        Axis::Horizontal,
        LayoutDirection::Rtl,
        Insets::all(10.0),
        10.0,
        CrossAlign::Stretch,
    )
    .unwrap();
    row.add_fixed(fixed, 100.0, 20.0).unwrap();
    row.add_flex(flex, 1.0, 50.0, f64::INFINITY, 20.0).unwrap();
    row.apply(&mut tree, rect(0.0, 0.0, 400.0, 100.0)).unwrap();
    assert_eq!(tree.bounds(fixed), Some(rect(290.0, 10.0, 100.0, 80.0)));
    assert_eq!(tree.bounds(flex), Some(rect(10.0, 10.0, 270.0, 80.0)));
    assert_eq!(tree.focus(), Some(flex));
}

#[test]
fn tiny_window_fails_without_partially_moving_children() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 80.0));
    let root = tree.root();
    let first = tree.attach(root, rect(1.0, 2.0, 10.0, 10.0)).unwrap();
    let second = tree.attach(root, rect(20.0, 2.0, 10.0, 10.0)).unwrap();
    let mut row = LinearLayout::new(
        Axis::Horizontal,
        LayoutDirection::Ltr,
        Insets::all(10.0),
        10.0,
        CrossAlign::Start,
    )
    .unwrap();
    row.add_fixed(first, 100.0, 20.0).unwrap();
    row.add_flex(second, 1.0, 50.0, 200.0, 20.0).unwrap();
    assert_eq!(
        row.apply(&mut tree, rect(0.0, 0.0, 100.0, 80.0)),
        Err(LayoutError::InsufficientSpace)
    );
    assert_eq!(tree.bounds(first), Some(rect(1.0, 2.0, 10.0, 10.0)));
    assert_eq!(tree.bounds(second), Some(rect(20.0, 2.0, 10.0, 10.0)));
}

#[test]
fn column_arranges_from_top_and_caps_flex_growth() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 300.0));
    let root = tree.root();
    let top = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let bottom = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let mut column = LinearLayout::new(
        Axis::Vertical,
        LayoutDirection::Ltr,
        Insets::all(10.0),
        10.0,
        CrossAlign::End,
    )
    .unwrap();
    column.add_fixed(top, 50.0, 40.0).unwrap();
    column.add_flex(bottom, 1.0, 30.0, 100.0, 60.0).unwrap();
    column
        .apply(&mut tree, rect(0.0, 0.0, 100.0, 300.0))
        .unwrap();
    assert_eq!(tree.bounds(top), Some(rect(50.0, 10.0, 40.0, 50.0)));
    assert_eq!(tree.bounds(bottom), Some(rect(30.0, 70.0, 60.0, 100.0)));
}

#[test]
fn large_finite_weights_still_share_space() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 20.0));
    let root = tree.root();
    let first = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let second = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let mut row = LinearLayout::new(
        Axis::Horizontal,
        LayoutDirection::Ltr,
        Insets::all(0.0),
        0.0,
        CrossAlign::Stretch,
    )
    .unwrap();
    row.add_flex(first, f64::MAX, 0.0, f64::INFINITY, 20.0)
        .unwrap();
    row.add_flex(second, f64::MAX, 0.0, f64::INFINITY, 20.0)
        .unwrap();
    row.apply(&mut tree, rect(0.0, 0.0, 100.0, 20.0)).unwrap();
    assert_eq!(tree.bounds(first), Some(rect(0.0, 0.0, 50.0, 20.0)));
    assert_eq!(tree.bounds(second), Some(rect(50.0, 0.0, 50.0, 20.0)));
}

#[test]
fn overflowing_arranged_coordinates_are_rejected() {
    let tree = Tree::new(rect(0.0, 0.0, 10.0, 10.0));
    let mut row = LinearLayout::new(
        Axis::Horizontal,
        LayoutDirection::Rtl,
        Insets::all(0.0),
        0.0,
        CrossAlign::Stretch,
    )
    .unwrap();
    row.add_fixed(tree.root(), 1.0, 1.0).unwrap();
    assert_eq!(
        row.arrange(rect(f64::MAX, 0.0, f64::MAX, 10.0)),
        Err(LayoutError::InvalidGeometry)
    );
}
