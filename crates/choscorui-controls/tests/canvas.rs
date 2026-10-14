mod hosted;

use choscorui_controls::{
    BandMode, CanvasInput, CanvasResponse, CanvasView, ControlMessage, ControlValue,
    DRAG_THRESHOLD, EditCommand, EstimatedText, MAX_ZOOM, PaintContext, ShortcutModifiers, Styles,
    VisualState,
};
use choscorui_core::{
    Color, ColorRole, DesignTokens, DisplayCommand, DisplayList, GestureEvent, ItemShape,
    ItemStyle, Key, Localizer, Paint, Point, Rect, Scene, SceneItem, SceneItemId, SemanticAction,
    SemanticRole, Size,
};

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(w, h))
}

fn paint(view: &CanvasView, scene: &mut Scene, list: &mut DisplayList) -> usize {
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    // The canvas has focus, so the focused item shows its outline.
    let mut cx = PaintContext::new(list, view.viewport(), &tokens, &styles, &EstimatedText)
        .state(VisualState::FOCUSED);
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
    let (node, collection) = view.semantics(&mut scene, Localizer::builtin());
    assert_eq!(node.role, SemanticRole::List);
    assert_eq!(node.description, "Zoom 100%");
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

fn press(view: &mut CanvasView, scene: &mut Scene, at: Point, extend: bool) -> CanvasResponse {
    view.handle(scene, CanvasInput::Press { at, extend })
}

fn key(key: Key) -> CanvasInput {
    CanvasInput::Key { key, shift: false }
}

#[test]
fn tab_leaves_at_the_ends_and_cycling_wraps() {
    let (mut scene, ids) = scene();
    let mut view = CanvasView::new("Diagram", rect(0.0, 0.0, 400.0, 100.0));
    for _ in 0..3 {
        view.handle(&mut scene, key(Key::Tab));
    }
    assert_eq!(view.focus(), Some(ids[2]));
    let past = view.handle(&mut scene, key(Key::Tab));
    assert_eq!(past, CanvasResponse::default(), "Tab leaves the canvas");
    assert_eq!(view.focus(), Some(ids[2]));
    view.handle(&mut scene, CanvasInput::Cycle { forward: true });
    assert_eq!(view.focus(), Some(ids[0]), "the cycle wraps");
    let back = view.handle(
        &mut scene,
        CanvasInput::Key {
            key: Key::Tab,
            shift: true,
        },
    );
    assert!(back.handled);
    assert_eq!(view.focus(), None, "back to the canvas itself");
    let out = view.handle(
        &mut scene,
        CanvasInput::Key {
            key: Key::Tab,
            shift: true,
        },
    );
    assert_eq!(
        out,
        CanvasResponse::default(),
        "Shift+Tab leaves the canvas"
    );
}

#[test]
fn a_click_in_a_selection_selects_one_item_and_a_drag_waits_for_the_threshold() {
    let (mut scene, ids) = scene();
    let mut view = CanvasView::new("Diagram", rect(0.0, 0.0, 400.0, 100.0));
    view.handle(&mut scene, CanvasInput::SelectAll);
    assert_eq!(view.selection().len(), 3);
    let at = Point::new(120.0, 20.0);
    press(&mut view, &mut scene, at, false);
    let small = Point::new(at.x + DRAG_THRESHOLD - 1.0, at.y);
    assert_eq!(
        view.handle(&mut scene, CanvasInput::Drag { at: small })
            .moved,
        None
    );
    let release = view.handle(&mut scene, CanvasInput::Release { at: small });
    assert!(release.selection_changed);
    assert_eq!(
        view.selection().iter().copied().collect::<Vec<_>>(),
        [ids[1]]
    );
}

#[test]
fn escape_cancels_a_drag_and_a_rubber_band() {
    let (mut scene, ids) = scene();
    let mut view = CanvasView::new("Diagram", rect(0.0, 0.0, 400.0, 100.0));
    press(&mut view, &mut scene, Point::new(20.0, 20.0), false);
    view.handle(
        &mut scene,
        CanvasInput::Drag {
            at: Point::new(60.0, 30.0),
        },
    );
    assert_eq!(scene.world_bounds(ids[0]).unwrap().origin.x, 40.0);
    let cancel = view.handle(&mut scene, key(Key::Escape));
    assert_eq!(cancel.moved, Some((vec![ids[0]], -40.0, -10.0)));
    assert_eq!(scene.world_bounds(ids[0]), Some(rect(0.0, 0.0, 50.0, 50.0)));
    assert_eq!(view.selection().len(), 1, "the selection stays");

    press(&mut view, &mut scene, Point::new(390.0, 90.0), true);
    view.handle(
        &mut scene,
        CanvasInput::Drag {
            at: Point::new(-1.0, -1.0),
        },
    );
    assert_eq!(view.selection().len(), 2, "the band toggles all three");
    view.handle(&mut scene, key(Key::Escape));
    assert_eq!(
        view.selection().iter().copied().collect::<Vec<_>>(),
        [ids[0]]
    );
    assert!(view.rubber_band().is_none());
}

#[test]
fn a_nudge_keeps_the_item_in_view_and_outlines_stay_in_view() {
    let (mut scene, ids) = scene();
    let mut view = CanvasView::new("Diagram", rect(0.0, 0.0, 160.0, 100.0));
    press(&mut view, &mut scene, Point::new(120.0, 20.0), false);
    for _ in 0..3 {
        view.handle(
            &mut scene,
            CanvasInput::Key {
                key: Key::ArrowRight,
                shift: true,
            },
        );
    }
    let bounds = scene.world_bounds(ids[1]).unwrap();
    assert!(
        view.visible_scene_rect().contains_rect(bounds),
        "{bounds:?}"
    );

    // Many selected items out of view paint no outline.
    let mut scene = Scene::new();
    for i in 0..2000 {
        let shape = ItemShape::Rect(rect(f64::from(i) * 60.0, 0.0, 50.0, 50.0));
        scene
            .add(SceneItem::new(
                shape,
                ItemStyle::filled(Color::rgba8(0, 0, 0, 255)),
            ))
            .unwrap();
    }
    let mut view = CanvasView::new("Diagram", rect(0.0, 0.0, 160.0, 100.0));
    view.handle(&mut scene, CanvasInput::SelectAll);
    let mut list = DisplayList::new();
    paint(&view, &mut scene, &mut list);
    let strokes = list
        .commands()
        .iter()
        .filter(|command| matches!(command, DisplayCommand::StrokePath { .. }))
        .count();
    assert_eq!(strokes, 3, "the three items in view");
}

#[test]
fn a_hosted_canvas_zooms_with_a_pinch_cycles_with_option_and_selects_all() {
    let (scene, ids) = scene();
    let canvas = CanvasView::new("Diagram", rect(0.0, 0.0, 400.0, 100.0)).hosted(scene);
    let mut h = hosted::Harness::new(canvas);
    let at = h.bounds().center();
    let gesture = GestureEvent::Magnify {
        position: at,
        scale: 2.0,
    };
    assert!(h.host.dispatch_gesture(&mut h.tree, gesture));
    assert_eq!(h.semantics().description, "Zoom 200%");
    h.focus();
    h.chord(
        ShortcutModifiers {
            option: true,
            ..ShortcutModifiers::default()
        },
        Key::ArrowLeft,
    );
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![
            ids[2].0 as usize
        ]))],
        "Option+ArrowLeft wraps to the last item"
    );
    assert!(h.host.dispatch_edit(&mut h.tree, EditCommand::SelectAll));
    let all = ids.iter().map(|id| id.0 as usize).collect();
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(all))]
    );
}

#[test]
fn an_intersect_band_selects_the_items_it_touches() {
    // The band covers box 0 and the start of box 1.
    let band = |view: &mut CanvasView, scene: &mut Scene| {
        view.handle(
            scene,
            CanvasInput::Press {
                at: Point::new(125.0, 70.0),
                extend: false,
            },
        );
        view.handle(
            scene,
            CanvasInput::Drag {
                at: Point::new(-5.0, -5.0),
            },
        );
        view.selection().iter().copied().collect::<Vec<_>>()
    };
    let (mut scene, ids) = scene();
    let mut view = CanvasView::new("Diagram", rect(0.0, 0.0, 400.0, 300.0));
    assert_eq!(view.band_mode(), BandMode::Contain);
    let contained = band(&mut view, &mut scene);
    assert_eq!(contained, vec![ids[0]]);
    let mut view = CanvasView::new("Diagram", rect(0.0, 0.0, 400.0, 300.0))
        .with_band_mode(BandMode::Intersect);
    assert_eq!(band(&mut view, &mut scene), vec![ids[0], ids[1]]);
}
