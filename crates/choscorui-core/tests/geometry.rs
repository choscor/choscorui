use choscorui_core::{Point, Rect, Size};

#[test]
fn rectangle_contains_its_near_edges_but_not_far_edges() {
    let rect = Rect::new(Point::new(10.0, 20.0), Size::new(30.0, 40.0));
    assert!(rect.contains(Point::new(10.0, 20.0)));
    assert!(rect.contains(Point::new(39.9, 59.9)));
    assert!(!rect.contains(Point::new(40.0, 60.0)));
    assert!(!rect.contains(Point::new(9.9, 30.0)));
}

#[test]
fn empty_rectangle_never_contains_a_point() {
    let rect = Rect::new(Point::new(10.0, 20.0), Size::new(0.0, 40.0));
    assert!(!rect.contains(Point::new(10.0, 20.0)));
}
