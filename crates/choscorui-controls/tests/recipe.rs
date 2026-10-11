//! A recipe changes only how a control paints. Swapping recipes or themes
//! in the middle of an interaction leaves activation, focus, keyboard
//! handling, and semantics unchanged.

use choscorui_controls::{
    Button, ButtonInput, ButtonVariant, CommandId, ControlKind, Corners, EstimatedText,
    PaintContext, Recipe, ResolvedStyle, SemanticAction, Slot, StyleQuery, Styles,
};
use choscorui_core::{
    Color, DesignTokens, DisplayCommand, DisplayList, Key, KeyEvent, NodeId, Point, RecipeSet,
    Rect, ShadowLayers, Size, ThemeInvalidation, ThemeOverride, ThemeResolver, Tree,
};
use std::rc::Rc;

fn setup() -> (Tree, NodeId, Button) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(300.0, 100.0)));
    let id = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(10.0, 10.0), Size::new(200.0, 36.0)),
        )
        .unwrap();
    tree.set_focusable(id, true).unwrap();
    (tree, id, Button::new("Run", CommandId(7)))
}

/// A flat look: square corners, no shadow, a thick leading rail as border.
struct Flat;

impl Recipe for Flat {
    fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
        let mut style = ResolvedStyle::new(tokens);
        style.background = Color::rgba8(40, 140, 230, 255);
        style.radius = Corners::ZERO;
        style.shadow = ShadowLayers::NONE;
        style.min_height = 44.0;
        if query.state.pressed() {
            style.background = Color::rgba8(20, 95, 175, 255);
        }
        Some(style)
    }
}

fn kinds(list: &DisplayList) -> Vec<&'static str> {
    list.commands()
        .iter()
        .map(|command| match command {
            DisplayCommand::FillRect { .. } => "rect",
            DisplayCommand::FillRoundedRect { .. } => "rounded",
            DisplayCommand::StrokeRoundedRect { .. } => "stroke",
            DisplayCommand::DrawShadow { .. } => "shadow",
            DisplayCommand::DrawText { .. } => "text",
            _ => "other",
        })
        .collect()
}

fn paint(
    button: &Button,
    tree: &Tree,
    id: NodeId,
    tokens: &DesignTokens,
    styles: &Styles,
) -> DisplayList {
    let mut list = DisplayList::new();
    let mut cx = PaintContext::new(
        &mut list,
        tree.bounds(id).unwrap(),
        tokens,
        styles,
        &EstimatedText,
    );
    button.paint(&mut cx, tree, id);
    list
}

#[test]
fn swapping_recipes_mid_interaction_keeps_activation_focus_keyboard_and_semantics() {
    let (mut tree, id, button) = setup();
    let mut button = button.with_variant(ButtonVariant::Outline);
    let mut themes = ThemeResolver::new(DesignTokens::dark());
    let mut styles = Styles::new();

    tree.focus_next(false).unwrap();
    assert!(tree.focus_visible(id));
    let before = button.semantics(&tree, id);
    let down = KeyEvent::Down {
        key: Key::Space,
        repeat: false,
    };
    button
        .handle(&mut tree, id, ButtonInput::Key(down))
        .unwrap();
    let state = button.visual_state(&tree, id);
    assert!(state.pressed() && state.focus_visible());
    let default_list = paint(
        &button,
        &tree,
        id,
        &themes.resolve(&tree, id).unwrap(),
        &styles,
    );
    assert!(
        kinds(&default_list).contains(&"shadow"),
        "outline elevation"
    );
    assert!(kinds(&default_list).contains(&"stroke"), "focus ring");

    // Switch theme and recipe while the key is still held.
    assert_eq!(
        themes
            .set_application(&mut tree, DesignTokens::light())
            .unwrap(),
        ThemeInvalidation::Paint
    );
    styles.set_recipe(RecipeSet::DEFAULT, ControlKind::Button, Rc::new(Flat));
    let flat_list = paint(
        &button,
        &tree,
        id,
        &themes.resolve(&tree, id).unwrap(),
        &styles,
    );
    assert_ne!(kinds(&default_list), kinds(&flat_list));
    assert!(!kinds(&flat_list).contains(&"shadow"));

    // The held Space still activates exactly once after the swap.
    let up = KeyEvent::Up { key: Key::Space };
    assert_eq!(
        button.handle(&mut tree, id, ButtonInput::Key(up)).unwrap(),
        Some(CommandId(7))
    );
    assert_eq!(tree.focus(), Some(id));
    assert_eq!(button.semantics(&tree, id), before);
    assert_eq!(
        button
            .handle(
                &mut tree,
                id,
                ButtonInput::Semantic(SemanticAction::Activate)
            )
            .unwrap(),
        Some(CommandId(7))
    );
}

#[test]
fn a_scope_selects_its_recipe_set() {
    let (mut tree, id, button) = setup();
    let mut themes = ThemeResolver::new(DesignTokens::light());
    let mut styles = Styles::new();
    let set = RecipeSet(3);
    styles.set_recipe(set, ControlKind::Button, Rc::new(Flat));
    let outside = paint(
        &button,
        &tree,
        id,
        &themes.resolve(&tree, id).unwrap(),
        &styles,
    );
    themes
        .set_scope(
            &mut tree,
            id,
            ThemeOverride {
                recipes: Some(set),
                ..ThemeOverride::default()
            },
        )
        .unwrap();
    let inside = paint(
        &button,
        &tree,
        id,
        &themes.resolve(&tree, id).unwrap(),
        &styles,
    );
    assert!(kinds(&outside).contains(&"rounded"));
    assert_eq!(kinds(&inside).first(), Some(&"rect"), "square fill");
    // The query the recipe saw holds only visual keys.
    let query = button.query(button.visual_state(&tree, id));
    assert_eq!(query.slot, Slot::Root);
    assert_eq!(query.kind, ControlKind::Button);
}
