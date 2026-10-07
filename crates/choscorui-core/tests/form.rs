use choscorui_core::{FormLayout, Insets, LayoutDirection, LayoutError, Point, Rect, Size, Tree};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn form_aligns_labels_to_fields_and_grows_field_column() {
    let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 120.0));
    let root = tree.root();
    let name_label = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let name_field = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let address_label = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let address_field = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let mut form = FormLayout::new(LayoutDirection::Ltr, Insets::all(10.0), 6.0, 8.0).unwrap();
    form.add_row(name_label, name_field, 40.0, 80.0, 30.0)
        .unwrap();
    form.add_row(address_label, address_field, 70.0, 100.0, 40.0)
        .unwrap();
    assert_eq!(form.measure(), Size::new(198.0, 96.0));
    form.apply(&mut tree, rect(0.0, 0.0, 300.0, 120.0)).unwrap();
    assert_eq!(tree.bounds(name_label), Some(rect(40.0, 10.0, 40.0, 30.0)));
    assert_eq!(tree.bounds(name_field), Some(rect(88.0, 10.0, 202.0, 30.0)));
    assert_eq!(
        tree.bounds(address_label),
        Some(rect(10.0, 46.0, 70.0, 40.0))
    );
    assert_eq!(
        tree.bounds(address_field),
        Some(rect(88.0, 46.0, 202.0, 40.0))
    );
}

#[test]
fn rtl_form_places_label_column_on_right_and_aligns_labels_toward_fields() {
    let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 100.0));
    let root = tree.root();
    let label = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let field = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let long_label = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let other_field = tree.attach(root, rect(0.0, 0.0, 0.0, 0.0)).unwrap();
    let mut form = FormLayout::new(LayoutDirection::Rtl, Insets::all(10.0), 6.0, 8.0).unwrap();
    form.add_row(label, field, 40.0, 100.0, 30.0).unwrap();
    form.add_row(long_label, other_field, 70.0, 100.0, 30.0)
        .unwrap();
    form.apply(&mut tree, rect(0.0, 0.0, 300.0, 100.0)).unwrap();
    assert_eq!(tree.bounds(label), Some(rect(220.0, 10.0, 40.0, 30.0)));
    assert_eq!(tree.bounds(field), Some(rect(10.0, 10.0, 202.0, 30.0)));
    assert_eq!(tree.bounds(long_label), Some(rect(220.0, 46.0, 70.0, 30.0)));
}

#[test]
fn form_rejects_small_window_without_partial_mutation() {
    let mut tree = Tree::new(rect(0.0, 0.0, 180.0, 100.0));
    let root = tree.root();
    let label = tree.attach(root, rect(1.0, 2.0, 10.0, 10.0)).unwrap();
    let field = tree.attach(root, rect(20.0, 2.0, 10.0, 10.0)).unwrap();
    let mut form = FormLayout::new(LayoutDirection::Ltr, Insets::all(10.0), 6.0, 8.0).unwrap();
    form.add_row(label, field, 70.0, 100.0, 30.0).unwrap();
    assert_eq!(
        form.apply(&mut tree, rect(0.0, 0.0, 180.0, 100.0)),
        Err(LayoutError::InsufficientSpace)
    );
    assert_eq!(tree.bounds(label), Some(rect(1.0, 2.0, 10.0, 10.0)));
    assert_eq!(tree.bounds(field), Some(rect(20.0, 2.0, 10.0, 10.0)));
}

#[test]
fn empty_form_still_enforces_its_padding_minimum() {
    let form = FormLayout::new(LayoutDirection::Ltr, Insets::all(10.0), 6.0, 8.0).unwrap();
    assert_eq!(form.measure(), Size::new(20.0, 20.0));
    assert_eq!(
        form.arrange(rect(0.0, 0.0, 10.0, 10.0)),
        Err(LayoutError::InsufficientSpace)
    );
    assert_eq!(form.arrange(rect(0.0, 0.0, 20.0, 20.0)), Ok(Vec::new()));
}
