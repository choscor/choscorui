use choscorui_controls::{
    Checkbox, CheckboxInput, ControlKind, EstimatedText, PaintContext, Recipe, ResolvedStyle,
    SemanticAction, Slot, StyleQuery, Styles,
};
use choscorui_core::{
    Color, ColorRole, DesignTokens, Dirty, DisplayCommand, DisplayList, Key, KeyEvent, Point,
    PointerButton, PointerEvent, RecipeSet, Rect, Size, Tree,
};
use choscorui_core::{SemanticAction as CoreSemanticAction, SemanticRole};
use std::rc::Rc;

fn setup() -> (Tree, choscorui_core::NodeId, Checkbox) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0)));
    let id = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(10.0, 10.0), Size::new(80.0, 40.0)),
        )
        .unwrap();
    (tree, id, Checkbox::new("Remember", false))
}

#[test]
fn pointer_release_inside_toggles_and_outside_cancels() {
    let (mut tree, id, mut checkbox) = setup();
    let inside = Point::new(20.0, 20.0);
    let outside = Point::new(150.0, 20.0);
    checkbox
        .handle(
            &mut tree,
            id,
            CheckboxInput::Pointer(PointerEvent::Down {
                position: inside,
                button: PointerButton::Primary,
            }),
        )
        .unwrap();
    assert_eq!(tree.pointer_capture(), Some(id));
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Pointer(PointerEvent::Up {
                    position: outside,
                    button: PointerButton::Primary
                })
            )
            .unwrap(),
        None
    );
    assert!(!checkbox.value());
    checkbox
        .handle(
            &mut tree,
            id,
            CheckboxInput::Pointer(PointerEvent::Down {
                position: inside,
                button: PointerButton::Primary,
            }),
        )
        .unwrap();
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Pointer(PointerEvent::Up {
                    position: inside,
                    button: PointerButton::Primary
                })
            )
            .unwrap(),
        Some(true)
    );
    assert_eq!(tree.pointer_capture(), None);
    assert!(checkbox.semantics(&tree, id).checked);
}

#[test]
fn space_and_semantic_set_value_share_state() {
    let (mut tree, id, mut checkbox) = setup();
    tree.set_focus(Some(id)).unwrap();
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Key(KeyEvent::Down {
                    key: Key::Space,
                    repeat: false
                })
            )
            .unwrap(),
        None
    );
    assert!(checkbox.visual_state(&tree, id).pressed());
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Key(KeyEvent::Up { key: Key::Space })
            )
            .unwrap(),
        Some(true)
    );
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Semantic(SemanticAction::SetValue(true))
            )
            .unwrap(),
        None
    );
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Semantic(SemanticAction::SetValue(false))
            )
            .unwrap(),
        Some(false)
    );
    assert_eq!(checkbox.semantics(&tree, id).name, "Remember");
    let shared = checkbox.semantics(&tree, id).shared();
    assert_eq!(shared.role, SemanticRole::Checkbox);
    assert_eq!(shared.state.checked, Some(false));
    assert_eq!(
        shared.actions,
        vec![
            CoreSemanticAction::Activate,
            CoreSemanticAction::SetValue,
            CoreSemanticAction::Focus
        ]
    );
}

#[test]
fn disabled_checkbox_releases_capture_and_blocks_changes() {
    let (mut tree, id, mut checkbox) = setup();
    checkbox
        .handle(
            &mut tree,
            id,
            CheckboxInput::Pointer(PointerEvent::Down {
                position: Point::new(20.0, 20.0),
                button: PointerButton::Primary,
            }),
        )
        .unwrap();
    checkbox.set_enabled(&mut tree, id, false).unwrap();
    assert_eq!(tree.pointer_capture(), None);
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Semantic(SemanticAction::SetValue(true))
            )
            .unwrap(),
        None
    );
    assert!(!checkbox.semantics(&tree, id).enabled);
}

#[test]
fn value_change_invalidates_paint_and_semantics_without_layout() {
    let (mut tree, id, mut checkbox) = setup();
    tree.take_dirty(id).unwrap();
    tree.take_dirty(tree.root()).unwrap();
    checkbox
        .handle(
            &mut tree,
            id,
            CheckboxInput::Semantic(SemanticAction::SetValue(true)),
        )
        .unwrap();
    let dirty = tree.take_dirty(tree.root()).unwrap();
    assert!(dirty.contains(Dirty::PAINT));
    assert!(dirty.contains(Dirty::SEMANTICS));
    assert!(!dirty.contains(Dirty::LAYOUT));
}

#[test]
fn recipe_swap_preserves_checked_value_and_keyboard_behavior() {
    struct Red;
    impl Recipe for Red {
        fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
            (query.slot == Slot::Root).then(|| ResolvedStyle {
                background: Color::rgba8(200, 30, 20, 255),
                min_width: 20.0,
                min_height: 20.0,
                ..ResolvedStyle::new(tokens)
            })
        }
    }
    let (mut tree, id, mut checkbox) = setup();
    tree.set_focus(Some(id)).unwrap();
    let tokens = DesignTokens::light();
    let default = Styles::new();
    let mut custom = Styles::new();
    custom.set_recipe(RecipeSet::DEFAULT, ControlKind::Checkbox, Rc::new(Red));
    let bounds = tree.bounds(id).unwrap();
    let paint = |styles: &Styles, checkbox: &Checkbox, tree: &Tree| {
        let mut list = DisplayList::new();
        let mut cx = PaintContext::new(&mut list, bounds, &tokens, styles, &EstimatedText);
        checkbox.paint(&mut cx, tree, id);
        list
    };
    let before = paint(&default, &checkbox, &tree);
    let after = paint(&custom, &checkbox, &tree);
    assert_ne!(before.commands(), after.commands());
    assert!(after.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::FillRect { color, .. } if *color == Color::rgba8(200, 30, 20, 255)
    )));
    let semantics = checkbox.semantics(&tree, id);
    checkbox
        .handle(
            &mut tree,
            id,
            CheckboxInput::Key(KeyEvent::Down {
                key: Key::Space,
                repeat: false,
            }),
        )
        .unwrap();
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Key(KeyEvent::Up { key: Key::Space })
            )
            .unwrap(),
        Some(true),
        "the recipe does not change activation"
    );
    assert_eq!(semantics.name, checkbox.semantics(&tree, id).name);
}

#[test]
fn indeterminate_shows_mixed_until_the_next_change() {
    let (mut tree, id, mut checkbox) = setup();
    checkbox.set_indeterminate(&mut tree, id, true).unwrap();
    let state = checkbox.visual_state(&tree, id);
    assert!(state.indeterminate() && !state.checked());
    let node = checkbox.semantics(&tree, id).shared();
    assert_eq!(node.value.as_deref(), Some("mixed"));
    assert_eq!(node.state.checked, None);
    assert_eq!(
        checkbox
            .handle(
                &mut tree,
                id,
                CheckboxInput::Semantic(SemanticAction::SetValue(false))
            )
            .unwrap(),
        Some(false),
        "setting the current value still clears the mixed state"
    );
    assert!(!checkbox.is_indeterminate());

    checkbox.set_invalid(&mut tree, id, true).unwrap();
    assert!(checkbox.visual_state(&tree, id).invalid());
    assert!(checkbox.semantics(&tree, id).shared().state.invalid);
}

#[test]
fn checkbox_paints_the_recipe_box_check_and_label() {
    let (mut tree, id, mut checkbox) = setup();
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let bounds = tree.bounds(id).unwrap();
    checkbox
        .handle(
            &mut tree,
            id,
            CheckboxInput::Semantic(SemanticAction::Activate),
        )
        .unwrap();
    let mut list = DisplayList::new();
    checkbox.paint(
        &mut PaintContext::new(&mut list, bounds, &tokens, &styles, &EstimatedText),
        &tree,
        id,
    );
    let primary = tokens.color(ColorRole::Primary);
    assert!(list.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::FillRoundedRect { rect, color, radius }
            if *color == primary && rect.size == Size::new(16.0, 16.0) && *radius == 4.0
    )));
    assert!(list.commands().iter().any(
        |command| matches!(command, DisplayCommand::DrawText { text, .. } if text == "Remember")
    ));
    let size = checkbox.measure(&styles, &tokens, &EstimatedText);
    assert!(size.width > 16.0 + 8.0 && size.height == 16.0);
}
