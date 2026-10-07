use choscorui_core::{GridLayout, Insets, LayoutDirection, LayoutError, Point, Rect, Size, Tree};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn grid_measures_tracks_and_arranges_spanning_child() {
    let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 200.0));
    let root = tree.root();
    let label = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let input = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let footer = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let mut grid = GridLayout::new(LayoutDirection::Ltr, Insets::all(10.0), 5.0, 5.0).unwrap();
    grid.add_column_fixed(80.0).unwrap();
    grid.add_column_flex(1.0, 100.0, f64::INFINITY).unwrap();
    grid.add_row_fixed(30.0).unwrap();
    grid.add_row_flex(1.0, 40.0, f64::INFINITY).unwrap();
    grid.place(label, 0, 0, 1, 1).unwrap();
    grid.place(input, 0, 1, 1, 1).unwrap();
    grid.place(footer, 1, 0, 1, 2).unwrap();

    assert_eq!(grid.measure(), Size::new(205.0, 95.0));
    grid.apply(&mut tree, rect(0.0, 0.0, 300.0, 200.0)).unwrap();
    assert_eq!(tree.bounds(label), Some(rect(10.0, 10.0, 80.0, 30.0)));
    assert_eq!(tree.bounds(input), Some(rect(95.0, 10.0, 195.0, 30.0)));
    assert_eq!(tree.bounds(footer), Some(rect(10.0, 45.0, 280.0, 145.0)));
}

#[test]
fn rtl_grid_mirrors_columns_and_preserves_spans() {
    let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 100.0));
    let root = tree.root();
    let first = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let second = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let span = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let mut grid = GridLayout::new(LayoutDirection::Rtl, Insets::all(10.0), 5.0, 5.0).unwrap();
    grid.add_column_fixed(80.0).unwrap();
    grid.add_column_flex(1.0, 100.0, f64::INFINITY).unwrap();
    grid.add_row_fixed(30.0).unwrap();
    grid.add_row_fixed(40.0).unwrap();
    grid.place(first, 0, 0, 1, 1).unwrap();
    grid.place(second, 0, 1, 1, 1).unwrap();
    grid.place(span, 1, 0, 1, 2).unwrap();
    grid.apply(&mut tree, rect(0.0, 0.0, 300.0, 100.0)).unwrap();
    assert_eq!(tree.bounds(first), Some(rect(210.0, 10.0, 80.0, 30.0)));
    assert_eq!(tree.bounds(second), Some(rect(10.0, 10.0, 195.0, 30.0)));
    assert_eq!(tree.bounds(span), Some(rect(10.0, 45.0, 280.0, 40.0)));
}

#[test]
fn invalid_span_and_tiny_bounds_leave_tree_unchanged() {
    let mut tree = Tree::new(rect(0.0, 0.0, 180.0, 100.0));
    let root = tree.root();
    let child = tree.attach(root, rect(3.0, 4.0, 10.0, 10.0)).unwrap();
    let mut grid = GridLayout::new(LayoutDirection::Ltr, Insets::all(10.0), 5.0, 5.0).unwrap();
    grid.add_column_fixed(80.0).unwrap();
    grid.add_column_flex(1.0, 100.0, f64::INFINITY).unwrap();
    grid.add_row_fixed(30.0).unwrap();
    assert_eq!(
        grid.place(child, 0, 1, 1, 2),
        Err(LayoutError::InvalidPlacement)
    );
    grid.place(child, 0, 1, 1, 1).unwrap();
    assert_eq!(
        grid.apply(&mut tree, rect(0.0, 0.0, 180.0, 100.0)),
        Err(LayoutError::InsufficientSpace)
    );
    assert_eq!(tree.bounds(child), Some(rect(3.0, 4.0, 10.0, 10.0)));
}

#[test]
fn vertical_span_includes_internal_gap_and_rejects_overlap() {
    let mut tree = Tree::new(rect(0.0, 0.0, 80.0, 80.0));
    let root = tree.root();
    let span = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let overlap = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let mut grid = GridLayout::new(LayoutDirection::Ltr, Insets::all(5.0), 4.0, 0.0).unwrap();
    grid.add_column_fixed(60.0).unwrap();
    grid.add_row_fixed(20.0).unwrap();
    grid.add_row_fixed(30.0).unwrap();
    grid.place(span, 0, 0, 2, 1).unwrap();
    assert_eq!(
        grid.place(overlap, 1, 0, 1, 1),
        Err(LayoutError::InvalidPlacement)
    );
    grid.apply(&mut tree, rect(0.0, 0.0, 80.0, 80.0)).unwrap();
    assert_eq!(tree.bounds(span), Some(rect(5.0, 5.0, 60.0, 54.0)));
}
