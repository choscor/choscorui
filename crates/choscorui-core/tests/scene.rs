use choscorui_core::{
    Color, DisplayCommand, DisplayList, ItemShape, ItemStyle, Point, Rect, Scene, SceneChange,
    SceneError, SceneItem, SemanticRole, Size, Transform,
};
use std::f64::consts::FRAC_PI_4;

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(w, h))
}

fn red() -> ItemStyle {
    ItemStyle::filled(Color::rgba8(255, 0, 0, 255))
}

#[test]
fn hit_testing_respects_transforms_z_order_and_shapes() {
    let mut scene = Scene::new();
    // A 100x20 bar rotated 45° about the origin, then moved to (200, 200).
    let bar = scene
        .add(
            SceneItem::new(ItemShape::Rect(rect(-50.0, -10.0, 100.0, 20.0)), red()).with_transform(
                Transform::rotate(FRAC_PI_4).then(Transform::translate(200.0, 200.0)),
            ),
        )
        .unwrap();
    // Along the rotated axis: inside. Its axis-aligned bounds corner: not.
    assert_eq!(scene.hit_test(Point::new(225.0, 225.0)), Some(bar));
    assert_eq!(scene.hit_test(Point::new(235.0, 165.0)), None);
    let bounds = scene.world_bounds(bar).unwrap();
    assert!(bounds.contains_rect(rect(170.0, 170.0, 60.0, 60.0)));

    let ellipse = scene
        .add(SceneItem::new(ItemShape::Ellipse(rect(180.0, 180.0, 40.0, 40.0)), red()).with_z(5))
        .unwrap();
    assert_eq!(
        scene.hit_test(Point::new(200.0, 200.0)),
        Some(ellipse),
        "higher z wins"
    );
    assert_eq!(
        scene.hit_test(Point::new(182.0, 182.0)),
        Some(bar),
        "ellipse corner misses"
    );

    let line = scene
        .add(SceneItem::new(
            ItemShape::Line {
                from: Point::new(0.0, 0.0),
                to: Point::new(100.0, 0.0),
            },
            ItemStyle::stroked(Color::rgba8(0, 0, 0, 255), 2.0),
        ))
        .unwrap();
    assert_eq!(scene.hit_test(Point::new(50.0, 2.5)), Some(line));
    assert_eq!(scene.hit_test(Point::new(50.0, 6.0)), None);

    scene
        .update(ellipse, |item| item.selectable = false)
        .unwrap();
    assert_eq!(scene.hit_test(Point::new(200.0, 200.0)), Some(bar));
    scene.update(bar, |item| item.visible = false).unwrap();
    assert_eq!(scene.hit_test(Point::new(225.0, 225.0)), None);
    assert_eq!(
        scene.update(bar, |item| item.transform = Transform::scale(0.0, 1.0)),
        Err(SceneError::InvalidTransform)
    );
}

#[test]
fn groups_compose_transforms_clip_and_reparent() {
    let mut scene = Scene::new();
    let mut group = SceneItem::new(
        ItemShape::Rect(rect(0.0, 0.0, 100.0, 100.0)),
        ItemStyle::default(),
    )
    .with_transform(Transform::scale(2.0, 2.0).then(Transform::translate(1000.0, 0.0)));
    group.clip_children = true;
    let group = scene.add(group).unwrap();
    let child = scene
        .add_child(
            group,
            SceneItem::new(ItemShape::Rect(rect(80.0, 10.0, 50.0, 10.0)), red()),
        )
        .unwrap();
    assert_eq!(
        scene.world_bounds(child),
        Some(rect(1160.0, 20.0, 100.0, 20.0))
    );
    assert_eq!(
        scene.world_bounds(group),
        Some(rect(1000.0, 0.0, 200.0, 200.0))
    );
    // Clipped: the part beyond the group's edge does not hit.
    assert_eq!(scene.hit_test(Point::new(1190.0, 30.0)), Some(child));
    assert_eq!(scene.hit_test(Point::new(1230.0, 30.0)), None);

    scene.move_by(&[group, child], 10.0, 0.0);
    assert_eq!(
        scene.world_bounds(child),
        Some(rect(1170.0, 20.0, 100.0, 20.0)),
        "moving a group and its child moves the child once"
    );
    scene.set_parent(child, None).unwrap();
    assert_eq!(
        scene.world_bounds(child),
        Some(rect(1170.0, 20.0, 100.0, 20.0))
    );
    assert_eq!(scene.roots().len(), 2);
    assert_eq!(scene.set_parent(group, Some(group)), Err(SceneError::Cycle));
    scene.set_parent(child, Some(group)).unwrap();
    assert_eq!(scene.set_parent(group, Some(child)), Err(SceneError::Cycle));
    scene.take_changes();
    assert_eq!(scene.remove(group), 2);
    assert!(scene.is_empty());
    let changes = scene.take_changes();
    assert_eq!(
        changes
            .iter()
            .filter(|change| matches!(change, SceneChange::Removed { .. }))
            .count(),
        2
    );
    assert_eq!(scene.index_cells(), 0, "the spatial index empties");
}

#[test]
fn large_scene_queries_scale_with_area() {
    let mut scene = Scene::new();
    for i in 0..100_000 {
        let (x, y) = ((i % 400) as f64 * 30.0, (i / 400) as f64 * 30.0);
        scene
            .add(
                SceneItem::new(ItemShape::Rect(rect(x, y, 20.0, 20.0)), red())
                    .named(format!("Node {i}"), SemanticRole::Button),
            )
            .unwrap();
    }
    let window = rect(3000.0, 3000.0, 300.0, 300.0);
    let visible = scene.items_in_rect(window);
    assert_eq!(visible.len(), 100);
    let start = std::time::Instant::now();
    for _ in 0..100 {
        scene.items_in_rect(window);
    }
    assert!(start.elapsed().as_millis() < 500, "{:?}", start.elapsed());
    let mut list = DisplayList::new();
    assert_eq!(scene.paint(&mut list, window).unwrap(), 100);
    let hit = scene.hit_test(Point::new(3005.0, 3005.0)).unwrap();
    assert_eq!(scene.item(hit).unwrap().name, "Node 40100");
    assert_eq!(scene.accessible_items().len(), 100_000);
}

#[test]
fn painting_nests_opacity_transform_and_exports() {
    let mut scene = Scene::new();
    let mut group = SceneItem::new(ItemShape::Group, ItemStyle::default());
    group.style.opacity = 0.5;
    let group = scene.add(group).unwrap();
    scene
        .add_child(
            group,
            SceneItem::new(
                ItemShape::Text {
                    origin: Point::new(10.0, 10.0),
                    text: "Xin chào".into(),
                    family: "Helvetica".into(),
                    size: 12.0,
                },
                ItemStyle::default(),
            ),
        )
        .unwrap();
    assert!(
        scene.world_bounds(group).is_some(),
        "groups take children's bounds"
    );
    let list = scene.export(rect(0.0, 0.0, 100.0, 50.0), 2.0).unwrap();
    assert!(list.is_balanced());
    let commands = list.commands();
    assert!(matches!(commands[0], DisplayCommand::PushClip(_)));
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, DisplayCommand::PushOpacity(o) if *o == 0.5))
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, DisplayCommand::DrawText { text, .. } if text == "Xin chào"))
    );
    let empty = scene.export(rect(500.0, 500.0, 10.0, 10.0), 1.0).unwrap();
    assert!(
        !empty
            .commands()
            .iter()
            .any(|c| matches!(c, DisplayCommand::DrawText { .. })),
        "culled"
    );
}
