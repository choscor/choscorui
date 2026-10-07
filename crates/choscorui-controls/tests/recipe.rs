use choscorui_controls::{
    Button, ButtonDecoration, ButtonInput, ButtonRecipe, ButtonStyle, ButtonVisualState, CommandId,
    ContentAlignment, PillButtonRecipe, SemanticAction, Slot, SlotContext, SlotLayout,
    paint_button,
};
use choscorui_core::{
    Color, DesignTokens, DisplayCommand, DisplayList, Key, KeyEvent, NodeId, Point, Rect, Size,
    ThemeInvalidation, ThemeResolver, Tree,
};
use std::cell::RefCell;

fn setup() -> (Tree, NodeId, Button) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(300.0, 100.0)));
    let id = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(10.0, 10.0), Size::new(200.0, 44.0)),
        )
        .unwrap();
    tree.set_focusable(id, true).unwrap();
    (tree, id, Button::new("Run", CommandId(7)))
}

fn flat() -> ButtonStyle {
    ButtonStyle {
        normal: Color::rgba8(40, 140, 230, 255),
        hover: Color::rgba8(80, 170, 245, 255),
        pressed: Color::rgba8(20, 95, 175, 255),
        disabled: Color::rgba8(90, 90, 90, 255),
        focus: Color::rgba8(255, 255, 255, 255),
        decoration: ButtonDecoration::LeadingRail {
            width: 6.0,
            color: Color::rgba8(230, 70, 90, 255),
        },
    }
}

fn kinds(list: &DisplayList) -> Vec<&'static str> {
    list.commands()
        .iter()
        .map(|command| match command {
            DisplayCommand::FillRect { .. } => "rect",
            DisplayCommand::FillRoundedRect { .. } => "rounded",
            DisplayCommand::StrokeRoundedRect { .. } => "ring",
            DisplayCommand::DrawShadow { .. } => "shadow",
            DisplayCommand::DrawText { .. } => "text",
            _ => "other",
        })
        .collect()
}

fn paint(
    recipe: &dyn ButtonRecipe,
    state: ButtonVisualState,
    tokens: &DesignTokens,
) -> DisplayList {
    let mut list = DisplayList::new();
    let bounds = Rect::new(Point::new(10.0, 10.0), Size::new(200.0, 44.0));
    paint_button(recipe, &mut list, bounds, state, tokens, |list, layout| {
        list.draw_text(
            Point::new(layout.content.origin.x, layout.content.origin.y + 20.0),
            "Run",
            "Helvetica",
            tokens.font_size,
            layout.content_color,
        )
        .unwrap();
    });
    list
}

#[test]
fn swapping_recipes_mid_interaction_keeps_activation_focus_keyboard_and_semantics() {
    let (mut tree, id, mut button) = setup();
    let mut themes = ThemeResolver::new(DesignTokens::dark());
    let pill = PillButtonRecipe::default();
    let style = flat();
    let mut recipe: &dyn ButtonRecipe = &style;

    tree.set_focus(Some(id)).unwrap();
    let before = button.semantics(&tree, id);
    let down = KeyEvent::Down {
        key: Key::Space,
        repeat: false,
    };
    button
        .handle(&mut tree, id, ButtonInput::Key(down))
        .unwrap();
    let state = button.visual_state(&tree, id);
    assert!(state.pressed && state.focused);
    let flat_list = paint(recipe, state, &themes.resolve(&tree, id).unwrap());

    // Switch theme and recipe while the key is still held.
    assert_eq!(
        themes
            .set_application(&mut tree, DesignTokens::light())
            .unwrap(),
        ThemeInvalidation::Layout
    );
    recipe = &pill;
    let tokens = themes.resolve(&tree, id).unwrap();
    let pill_list = paint(recipe, button.visual_state(&tree, id), &tokens);
    assert_ne!(kinds(&flat_list), kinds(&pill_list));
    assert!(!kinds(&flat_list).contains(&"rounded"));
    assert!(kinds(&pill_list).contains(&"ring"), "focus ring indicator");
    assert!(kinds(&pill_list).contains(&"shadow"), "elevation");

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
fn slots_paint_in_order_and_layouts_differ_structurally() {
    struct Recorder(RefCell<Vec<Slot>>);
    impl ButtonRecipe for Recorder {
        fn layout(&self, bounds: Rect, _: ButtonVisualState, tokens: &DesignTokens) -> SlotLayout {
            SlotLayout {
                content: bounds,
                alignment: ContentAlignment::Leading,
                icon: None,
                content_color: tokens.content,
            }
        }
        fn paint_slot(&self, slot: Slot, _: &mut DisplayList, _: &SlotContext<'_>) {
            self.0.borrow_mut().push(slot);
        }
    }
    let recorder = Recorder(RefCell::new(Vec::new()));
    let state = ButtonVisualState {
        enabled: true,
        hovered: false,
        pressed: false,
        focused: false,
    };
    let list = paint(&recorder, state, &DesignTokens::dark());
    assert_eq!(*recorder.0.borrow(), Slot::ORDER);
    assert_eq!(kinds(&list), vec!["text"], "label drawn by the caller");

    let tokens = DesignTokens::light();
    let bounds = Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 44.0));
    let pill = PillButtonRecipe::default().layout(bounds, state, &tokens);
    let flat_layout = flat().layout(bounds, state, &tokens);
    assert_eq!(pill.alignment, ContentAlignment::Center);
    assert_eq!(flat_layout.alignment, ContentAlignment::Leading);
    assert!(pill.icon.is_some() && flat_layout.icon.is_none());
    assert!(pill.content.size.width < flat_layout.content.size.width);

    // Disabled pill adds an overlay; unfocused flat has no indicator bars.
    let disabled = ButtonVisualState {
        enabled: false,
        ..state
    };
    let pill_disabled = paint(&PillButtonRecipe::default(), disabled, &tokens);
    assert_eq!(kinds(&pill_disabled).last(), Some(&"rounded"));
    assert!(!kinds(&pill_disabled).contains(&"shadow"));
    assert_eq!(
        kinds(&paint(&flat(), state, &tokens)),
        vec!["rect", "rect", "text"]
    );
}
