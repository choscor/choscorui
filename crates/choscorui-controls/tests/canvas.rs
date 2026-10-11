use choscorui_controls::{CanvasInput, CanvasView, EstimatedText, MAX_ZOOM, PaintContext, Styles};
use choscorui_core::{
    Color, ColorRole, DesignTokens, DisplayCommand, DisplayList, ItemShape, ItemStyle, Key, Paint,
    Point, Rect, Scene, SceneItem, SceneItemId, SemanticAction, SemanticRole, Size,
};

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(w, h))
}

fn paint(view: &CanvasView, scene: &mut Scene, list: &mut DisplayList) -> usize {
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let mut cx = PaintContext::new(list, view.viewport(), &tokens, &styles, &EstimatedText);
    view.paint(&mut cx, scene).unwrap()
}

fn scene() -> (Scene, Vec<SceneItemId>) {
    let mut scene = Scene::new();
    let ids = (0..3)
        .map(|i| {
            scene
                .add(
                    SceneItem::new(
                        ItemShape::Rect(rect(i as f64 * 100.0, 0.0, 50.0, 50.0)),
                        ItemStyle::filled(Color::rgba8(0, 128, 0, 255)),
                    )
                    .named(format!("Box {i}"), SemanticRole::Button),
                )
                .unwrap()
        })
        .collect();
    (scene, ids)
}

#[test]
fn zoom_keeps_anchor_and_pan_moves_view() {
    let mut view = CanvasView::new("Diagram", rect(10.0, 20.0, 400.0, 300.0));
    let anchor = Point::new(110.0, 120.0);
    let before = view.view_to_scene(anchor);
    assert!(view.zoom_at(anchor, 4.0));
    let after = view.view_to_scene(anchor);
    assert!((before.x - after.x).abs() < 1e-9 && (before.y - after.y).abs() < 1e-9);
    assert!(!view.zoom_at(anchor, 4.0));
    view.zoom_at(anchor, 1e9);
    assert_eq!(view.zoom(), MAX_ZOOM);
    let p = view.scene_to_view(Point::new(5.0, 5.0));
    let back = view.view_to_scene(p);
    assert!((back.x - 5.0).abs() < 1e-9);

    let mut view = CanvasView::new("Diagram", rect(0.0, 0.0, 400.0, 300.0));
    assert!(view.pan(100.0, 50.0));
    assert_eq!(view.visible_scene_rect().origin, Point::new(100.0, 50.0));
    view.fit(rect(0.0, 0.0, 800.0, 100.0), 0.0);
    assert_eq!(view.zoom(), 0.5);
    assert_eq!(view.visible_scene_rect().center(), Point::new(400.0, 50.0));
}

#[test]
fn selection_and_dragging_work_in_zoomed_coordinates() {
    let (mut scene, ids) = scene();
    let mut view = CanvasView::new("Diagram", rect(0.0, 0.0, 400.0, 300.0));
    view.zoom_at(Point::new(0.0, 0.0), 2.0);
    // Box 1 spans scene x 100..150 → view x 200..300.
    let r = view.handle(
        &mut scene,
        CanvasInput::Press {
            at: Point::new(250.0, 50.0),
            extend: false,
        },
    );
    assert!(r.selection_changed);
    assert_eq!(
        view.selection().iter().copied().collect::<Vec<_>>(),
        vec![ids[1]]
    );
    let r = view.handle(
        &mut scene,
        CanvasInput::Drag {
            at: Point::new(270.0, 60.0),
        },
    );
    assert_eq!(
        r.moved,
        Some((vec![ids[1]], 10.0, 5.0)),
        "view delta / zoom"
    );
    view.handle(
        &mut scene,
        CanvasInput::Release {
            at: Point::new(270.0, 60.0),
        },
    );
    assert_eq!(
        scene.world_bounds(ids[1]),
        Some(rect(110.0, 5.0, 50.0, 50.0))
    );

    // ⌘-click adds; clicking empty space clears.
    view.handle(
        &mut scene,
        CanvasInput::Press {
            at: Point::new(20.0, 20.0),
            extend: true,
        },
    );
    assert_eq!(view.selection().len(), 2);
    view.handle(
        &mut scene,
        CanvasInput::Release {
            at: Point::new(20.0, 20.0),
        },
    );
    view.handle(
        &mut scene,
        CanvasInput::Press {
            at: Point::new(390.0, 290.0),
            extend: false,
        },
    );
    assert!(view.selection().is_empty());
    // Rubber band (in view space) over boxes 0 and 1.
    let r = view.handle(
        &mut scene,
        CanvasInput::Drag {
            at: Point::new(-1.0, -1.0),
        },
    );
    assert!(r.selection_changed);
    assert!(view.rubber_band().is_some());
    view.handle(
        &mut scene,
        CanvasInput::Release {
            at: Point::new(-1.0, -1.0),
        },
    );
    assert!(view.rubber_band().is_none());
    assert_eq!(
        view.selection().iter().copied().collect::<Vec<_>>(),
        vec![ids[0], ids[1]]
    );

    let mut list = DisplayList::new();
    let painted = paint(&view, &mut scene, &mut list);
    assert_eq!(
        painted, 2,
        "box 2 at scene x 200 is outside the zoomed viewport"
    );
    assert!(list.is_balanced());
    let primary = DesignTokens::light().color(ColorRole::Primary);
    let outlines = list.commands().iter().filter(|command| {
        matches!(command, DisplayCommand::StrokePath { paint: Paint::Solid(color), .. }
            if *color == primary)
    });
    assert_eq!(outlines.count(), 3, "two selected items and the focus");
}

#[test]
fn keyboard_focus_nudge_delete_and_zoom() {
    let (mut scene, ids) = scene();
    let mut view = CanvasView::new("Diagram", rect(0.0, 0.0, 120.0, 100.0));
    let key = |key| CanvasInput::Key { key, shift: false };
    view.handle(&mut scene, key(Key::Tab));
    assert_eq!(view.focus(), Some(ids[0]));
    view.handle(&mut scene, key(Key::Tab));
    view.handle(&mut scene, key(Key::Tab));
    assert_eq!(view.focus(), Some(ids[2]));
    assert!(
        view.visible_scene_rect()
            .contains_rect(scene.world_bounds(ids[2]).unwrap()),
        "focus scrolls the item into view"
    );
    view.handle(
        &mut scene,
        CanvasInput::Key {
            key: Key::Tab,
            shift: true,
        },
    );
    assert_eq!(view.focus(), Some(ids[1]));
    let r = view.handle(
        &mut scene,
        CanvasInput::Key {
            key: Key::ArrowRight,
            shift: true,
        },
    );
    assert_eq!(r.moved, Some((vec![ids[1]], 10.0, 0.0)));
    assert_eq!(
        view.handle(&mut scene, key(Key::Enter)).activated,
        Some(ids[1])
    );
    assert_eq!(
        view.handle(&mut scene, key(Key::Delete)).delete_requested,
        vec![ids[1]]
    );
    scene.remove(ids[1]);
    assert!(view.prune(&scene));
    assert_eq!(view.focus(), None);
    view.handle(&mut scene, key(Key::Character('+')));
    assert_eq!(view.zoom(), 1.25);
    view.handle(&mut scene, key(Key::Character('0')));
    assert_eq!(view.zoom(), 1.0);
    let r = view.handle(
        &mut scene,
        CanvasInput::Wheel {
            dx: 0.0,
            dy: -69.3147,
            at: Point::new(0.0, 0.0),
            zoom: true,
        },
    );
    assert!(r.repaint);
    assert!((view.zoom() - 2.0).abs() < 1e-3);
}

#[test]
fn accessible_items_follow_viewport_and_actions() {
    let (mut scene, ids) = scene();
    let mut view = CanvasView::new("Diagram", rect(0.0, 0.0, 160.0, 100.0));
    let (node, collection) = view.semantics(&mut scene);
    assert_eq!(node.role, SemanticRole::List);
    assert_eq!(collection.item_count, 3);
    let items = view.item_semantics(&mut scene);
    assert_eq!(items.len(), 2, "box 2 is offscreen and virtualized");
    assert_eq!(items[1].1, 1);
    assert_eq!(items[1].2.name, "Box 1");
    assert_eq!(items[1].3, rect(100.0, 0.0, 50.0, 50.0));
    assert!(view.perform(&scene, ids[2], SemanticAction::Focus));
    let items = view.item_semantics(&mut scene);
    assert!(
        items
            .iter()
            .any(|(id, index, _, _)| *id == ids[2] && *index == 2)
    );
    assert!(view.perform(&scene, ids[2], SemanticAction::Activate));
    let selected = view
        .item_semantics(&mut scene)
        .into_iter()
        .find(|(id, ..)| *id == ids[2])
        .unwrap();
    assert_eq!(selected.2.state.selected, Some(true));
}

#[test]
fn two_views_share_a_scene_with_independent_state() {
    let (mut scene, ids) = scene();
    let mut overview = CanvasView::new("Overview", rect(0.0, 0.0, 100.0, 100.0));
    let mut detail = CanvasView::new("Detail", rect(0.0, 0.0, 400.0, 400.0));
    overview.fit(rect(0.0, 0.0, 250.0, 50.0), 0.0);
    detail.zoom_at(Point::new(0.0, 0.0), 4.0);
    detail.handle(
        &mut scene,
        CanvasInput::Press {
            at: Point::new(100.0, 100.0),
            extend: false,
        },
    );
    assert_eq!(detail.selection().len(), 1);
    assert!(overview.selection().is_empty());
    detail.handle(
        &mut scene,
        CanvasInput::Drag {
            at: Point::new(140.0, 100.0),
        },
    );
    // The overview sees the moved geometry on its next paint.
    let mut list = DisplayList::new();
    assert_eq!(paint(&overview, &mut scene, &mut list), 3);
    assert_eq!(scene.world_bounds(ids[0]).unwrap().origin.x, 10.0);
}
