use choscorui_core::{
    DragEvent, DragFormat, DragItem, DragOperation, DragOperations, DropRouter, HoverTracker,
    Point, PointerDetails, PointerKind, Rect, ScaleFactor, Transform, Tree, negotiate_drag,
};

fn tree() -> (Tree, [choscorui_core::NodeId; 3]) {
    let mut tree = Tree::new(Rect::from_xywh(0.0, 0.0, 200.0, 100.0));
    let left = tree
        .attach(tree.root(), Rect::from_xywh(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    let inner = tree
        .attach(left, Rect::from_xywh(10.0, 10.0, 20.0, 20.0))
        .unwrap();
    let right = tree
        .attach(tree.root(), Rect::from_xywh(100.0, 0.0, 100.0, 100.0))
        .unwrap();
    (tree, [left, inner, right])
}

#[test]
fn hover_reports_leave_deepest_first_and_enter_outermost_first() {
    let (mut tree, [left, inner, right]) = tree();
    let root = tree.root();
    let mut hover = HoverTracker::new();
    let first = hover.update(&tree, Some(Point::new(15.0, 15.0)));
    assert_eq!(first.entered, [root, left, inner]);
    assert!(first.left.is_empty());
    let moved = hover.update(&tree, Some(Point::new(150.0, 50.0)));
    assert_eq!(moved.left, [inner, left]);
    assert_eq!(moved.entered, [right]);
    assert!(
        hover
            .update(&tree, Some(Point::new(160.0, 50.0)))
            .is_empty()
    );
    // Capture freezes hover so a dragged slider does not hover siblings.
    tree.set_pointer_capture(Some(right)).unwrap();
    assert!(hover.update(&tree, Some(Point::new(15.0, 15.0))).is_empty());
    tree.set_pointer_capture(None).unwrap();
    // Detached nodes leave silently; pointer exit leaves the remainder.
    tree.detach(right).unwrap();
    let exit = hover.update(&tree, None);
    assert_eq!(exit.left, [root]);
    assert!(hover.path().is_empty());
}

#[test]
fn drop_router_retargets_with_leave_and_enter_and_cancels_on_detach() {
    let (mut tree, [left, _, right]) = tree();
    let mut router = DropRouter::new();
    let formats = vec![DragFormat::Files, DragFormat::Text];
    let enter = router.route(
        &tree,
        DragEvent::Enter {
            position: Point::new(50.0, 50.0),
            formats: formats.clone(),
            allowed: DragOperations::ALL,
        },
    );
    assert!(matches!(&enter[..], [(id, DragEvent::Enter { .. })] if *id == left));
    let over = router.route(
        &tree,
        DragEvent::Over {
            position: Point::new(150.0, 50.0),
            allowed: DragOperations::ALL,
        },
    );
    assert_eq!(over.len(), 2);
    assert_eq!(over[0], (left, DragEvent::Leave));
    assert!(
        matches!(&over[1], (id, DragEvent::Enter { formats: f, .. }) if *id == right && *f == formats)
    );
    let drop = router.route(
        &tree,
        DragEvent::Drop {
            position: Point::new(150.0, 50.0),
            items: vec![DragItem::Text("hi".into())],
            operation: DragOperation::Copy,
        },
    );
    assert_eq!(drop[0].0, right);
    assert_eq!(router.target(), None);
    router.route(
        &tree,
        DragEvent::Enter {
            position: Point::new(150.0, 50.0),
            formats,
            allowed: DragOperations::ALL,
        },
    );
    tree.detach(right).unwrap();
    assert!(router.route(&tree, DragEvent::Leave).is_empty());
    assert_eq!(DragItem::Files(vec![]).format(), DragFormat::Files);
}

#[test]
fn drag_negotiation_honors_modifiers_and_source_limits() {
    let all = DragOperations::ALL;
    assert_eq!(negotiate_drag(all, false, false, true), DragOperation::Move);
    assert_eq!(
        negotiate_drag(all, false, false, false),
        DragOperation::Copy
    );
    assert_eq!(negotiate_drag(all, true, false, true), DragOperation::Copy);
    assert_eq!(negotiate_drag(all, true, true, true), DragOperation::Link);
    let copy_only = DragOperations {
        copy: true,
        ..DragOperations::NONE
    };
    assert_eq!(
        negotiate_drag(copy_only, true, true, true),
        DragOperation::Copy
    );
    assert_eq!(
        negotiate_drag(DragOperations::NONE, false, false, true),
        DragOperation::None
    );
    assert!(DragOperations::NONE.is_empty());
}

#[test]
fn transforms_compose_invert_and_bound_rectangles() {
    let t = Transform::scale(2.0, 3.0).then(Transform::translate(10.0, 20.0));
    assert_eq!(t.apply(Point::new(1.0, 1.0)), Point::new(12.0, 23.0));
    let back = t.invert().unwrap().apply(Point::new(12.0, 23.0));
    assert!((back.x - 1.0).abs() < 1e-9 && (back.y - 1.0).abs() < 1e-9);
    assert!(Transform::scale(0.0, 1.0).invert().is_none());
    let rotated = Transform::rotate(std::f64::consts::FRAC_PI_2)
        .apply_rect(Rect::from_xywh(0.0, 0.0, 10.0, 4.0));
    assert!((rotated.size.width - 4.0).abs() < 1e-9 && (rotated.size.height - 10.0).abs() < 1e-9);
}

#[test]
fn rectangles_intersect_union_and_mirror() {
    let a = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
    let b = Rect::from_xywh(5.0, 5.0, 10.0, 10.0);
    assert_eq!(a.intersection(b), Some(Rect::from_xywh(5.0, 5.0, 5.0, 5.0)));
    assert_eq!(a.intersection(Rect::from_xywh(10.0, 0.0, 5.0, 5.0)), None);
    assert_eq!(a.union(b), Rect::from_xywh(0.0, 0.0, 15.0, 15.0));
    assert_eq!(a.union(Rect::default()), a);
    let container = Rect::from_xywh(0.0, 0.0, 100.0, 10.0);
    assert_eq!(
        Rect::from_xywh(10.0, 0.0, 20.0, 10.0)
            .mirrored_in(container)
            .origin
            .x,
        70.0
    );
    assert_eq!(a.inset(6.0).size.width, 0.0);
}

#[test]
fn logical_geometry_is_scale_independent() {
    // The same logical tree hit-tests identically at every display scale;
    // only the pixel coverage of a rectangle changes.
    let (tree, [_, inner, _]) = tree();
    for scale in [1.0, 1.5, 2.0, 3.0] {
        let factor = ScaleFactor::new(scale).unwrap();
        let device = factor.to_physical(Point::new(15.0, 15.0));
        assert_eq!(tree.hit_test(factor.to_logical(device)), Some(inner));
        let pixels = factor.cover_pixels(Rect::from_xywh(10.25, 10.25, 20.0, 20.0));
        assert!(pixels.width as f64 >= 20.0 * scale);
    }
    assert_eq!(ScaleFactor::new(2.0).unwrap().snap(10.3), 10.5);
    assert!(ScaleFactor::new(0.0).is_none());
    let details = PointerDetails::default();
    assert_eq!((details.kind, details.pressure), (PointerKind::Mouse, 1.0));
}
