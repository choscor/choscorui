//! The customization recipes in docs/theming.md, compiled and checked. Each
//! test is one recipe; keep the document's snippets in sync with these bodies.

use choscorui::controls::{
    Button, ButtonInput, ButtonRecipe, ButtonVisualState, CommandId, CommandRegistry, CommandState,
    ContentAlignment, ItemDelegate, ItemState, ItemView, PillButtonRecipe, Shortcut,
    ShortcutModifiers, ShortcutScope, Slot, SlotContext, SlotLayout, TextField, TextFieldInput,
    ValidationKind, ValidationState, paint_button,
};
use choscorui::core::{
    Color, ColorRole, DesignTokens, DisplayCommand, DisplayList, ItemKey, ItemModel, Key, Locale,
    Point, Rect, Role, SemanticRole, Size, StringListModel, ThemeOverride, ThemeResolver, Tree,
    Value,
};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

// Recipe: a brand theme.
fn brand(dark: bool) -> DesignTokens {
    let base = if dark {
        DesignTokens::dark()
    } else {
        DesignTokens::light()
    };
    DesignTokens {
        palette: base
            .palette
            .with(ColorRole::Primary, Color::rgba8(0, 128, 128, 255)),
        radius: 8.0,
        ..base
    }
}

#[test]
fn brand_theme_switches_at_runtime() {
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
    let root = tree.root();
    let mut themes = ThemeResolver::new(brand(false));
    assert_eq!(themes.resolve(&tree, root).unwrap().radius, 8.0);
    themes.set_application(&mut tree, brand(true)).unwrap();
    assert_eq!(
        themes
            .resolve(&tree, root)
            .unwrap()
            .color(ColorRole::Background),
        DesignTokens::dark().color(ColorRole::Background)
    );
}

// Recipe: restyle one subtree with a scoped override.
#[test]
fn scoped_override_restyles_one_subtree() {
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
    let root = tree.root();
    let danger_zone = tree.attach(root, rect(0.0, 200.0, 400.0, 100.0)).unwrap();
    let delete = tree
        .attach(danger_zone, rect(10.0, 210.0, 120.0, 32.0))
        .unwrap();
    let mut themes = ThemeResolver::new(brand(false));
    let tokens = themes.resolve(&tree, root).unwrap();
    themes
        .set_scope(
            &mut tree,
            danger_zone,
            ThemeOverride::role(ColorRole::Primary, tokens.color(ColorRole::Destructive)),
        )
        .unwrap();
    assert_eq!(
        themes
            .resolve(&tree, delete)
            .unwrap()
            .color(ColorRole::Primary),
        tokens.color(ColorRole::Destructive)
    );
    assert_eq!(
        themes
            .resolve(&tree, root)
            .unwrap()
            .color(ColorRole::Primary),
        tokens.color(ColorRole::Primary)
    );
}

// Recipe: a custom button recipe (outline style).
struct OutlineRecipe;

impl ButtonRecipe for OutlineRecipe {
    fn layout(&self, bounds: Rect, state: ButtonVisualState, tokens: &DesignTokens) -> SlotLayout {
        SlotLayout {
            content: bounds.inset(tokens.spacing),
            alignment: ContentAlignment::Center,
            icon: None,
            content_color: if state.enabled {
                tokens.color(ColorRole::Primary)
            } else {
                tokens.color(ColorRole::Foreground).with_alpha(0.4)
            },
        }
    }

    fn paint_slot(&self, slot: Slot, list: &mut DisplayList, context: &SlotContext<'_>) {
        let tokens = context.tokens;
        match slot {
            Slot::Background if context.state.pressed => {
                let _ = list.fill_rounded_rect(
                    context.bounds,
                    tokens.radius,
                    tokens.color(ColorRole::Primary).with_alpha(0.15),
                );
            }
            Slot::Indicator => {
                let width = if context.state.focused {
                    tokens.border_width * 2.0
                } else {
                    tokens.border_width
                };
                let _ = list.stroke_rounded_rect(
                    context.bounds,
                    tokens.radius,
                    context.layout.content_color,
                    width,
                );
            }
            _ => {}
        }
    }
}

#[test]
fn custom_recipe_changes_paint_but_not_behavior() {
    let tokens = brand(false);
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
    let node = tree
        .attach(tree.root(), rect(10.0, 10.0, 120.0, 32.0))
        .unwrap();
    tree.set_focusable(node, true).unwrap();
    let mut button = Button::new("Save", CommandId(1));
    let state = button.visual_state(&tree, node);
    let mut outline = DisplayList::new();
    let mut pill = DisplayList::new();
    paint_button(
        &OutlineRecipe,
        &mut outline,
        rect(10.0, 10.0, 120.0, 32.0),
        state,
        &tokens,
        |_, _| {},
    );
    paint_button(
        &PillButtonRecipe::default(),
        &mut pill,
        rect(10.0, 10.0, 120.0, 32.0),
        state,
        &tokens,
        |_, _| {},
    );
    assert_ne!(outline.commands(), pill.commands());
    // Behavior belongs to the control, whatever paints it.
    tree.set_focus(Some(node)).unwrap();
    let enter = choscorui::core::KeyEvent::Down {
        key: Key::Enter,
        repeat: false,
    };
    assert_eq!(
        button
            .handle(&mut tree, node, ButtonInput::Key(enter))
            .unwrap(),
        Some(CommandId(1))
    );
}

// Recipe: a custom item delegate.
struct BadgeDelegate;

impl ItemDelegate for BadgeDelegate {
    fn paint_cell(
        &self,
        list: &mut DisplayList,
        rect: Rect,
        model: &dyn ItemModel,
        key: ItemKey,
        column: usize,
        state: ItemState,
    ) {
        let tokens = DesignTokens::light();
        if state.selected {
            list.fill_rect(rect, tokens.color(ColorRole::Primary).with_alpha(0.2));
        }
        if let Value::Text(text) = model.data(key, column, Role::Display) {
            let _ = list.draw_text(
                Point::new(rect.origin.x + 28.0, rect.origin.y + rect.size.height * 0.7),
                &text,
                "Helvetica",
                13.0,
                tokens.color(ColorRole::Foreground),
            );
            let badge = rect.size.height - 8.0;
            let _ = list.fill_rounded_rect(
                Rect::new(
                    Point::new(rect.origin.x + 4.0, rect.origin.y + 4.0),
                    Size::new(badge, badge),
                ),
                badge / 2.0,
                tokens.color(ColorRole::Primary),
            );
        }
    }
}

#[test]
fn custom_delegate_paints_realized_rows() {
    let model = StringListModel::from_strings(["Hà Nội", "Huế", "Đà Lạt"]);
    let mut tree = Tree::new(rect(0.0, 0.0, 200.0, 100.0));
    let node = tree
        .attach(tree.root(), rect(0.0, 0.0, 200.0, 66.0))
        .unwrap();
    let mut view = ItemView::new("Cities", SemanticRole::List, 22.0);
    view.sync(&model);
    view.arrange(&mut tree, node, rect(0.0, 0.0, 200.0, 66.0))
        .unwrap();
    let mut list = DisplayList::new();
    view.paint(&mut list, &model, &BadgeDelegate, &[], false);
    let texts = list
        .commands()
        .iter()
        .filter(|command| matches!(command, DisplayCommand::DrawText { .. }))
        .count();
    assert_eq!(texts, 3);
}

// Recipe: custom validation for a text field.
fn validate_amount(
    field: &mut TextField,
    tree: &mut Tree,
    node: choscorui::core::NodeId,
    locale: &Locale,
) {
    let state = match locale.parse_decimal(field.value()) {
        _ if field.value().is_empty() => ValidationState::Intermediate,
        Ok(value) if value > 0.0 => ValidationState::Valid,
        Ok(_) => ValidationState::Invalid("the amount must be positive".into()),
        Err(_) => ValidationState::Invalid("enter a number such as 1.234,50".into()),
    };
    field
        .handle(tree, node, TextFieldInput::SetValidation(state))
        .unwrap();
}

#[test]
fn custom_validation_follows_the_locale() {
    let vietnamese = Locale::parse("vi").unwrap();
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
    let node = tree
        .attach(tree.root(), rect(10.0, 10.0, 200.0, 30.0))
        .unwrap();
    tree.set_focusable(node, true).unwrap();
    tree.set_focus(Some(node)).unwrap();
    let mut field = TextField::new("Amount", "");
    for (text, expected) in [
        ("1.234,50", ValidationKind::Valid),
        ("-3", ValidationKind::Invalid),
        ("abc", ValidationKind::Invalid),
    ] {
        field
            .handle(&mut tree, node, TextFieldInput::SetValue(text.into()))
            .unwrap();
        validate_amount(&mut field, &mut tree, node, &vietnamese);
        assert_eq!(
            field.visual_state(&tree, node).validation,
            expected,
            "{text}"
        );
    }
    let semantics = field.semantics(&tree, node);
    assert_eq!(
        semantics.announcement.as_deref(),
        Some("enter a number such as 1.234,50")
    );
}

// Recipe: a focused-scope shortcut that overrides an application one.
#[test]
fn focused_shortcut_wins_over_application_shortcut() {
    let (find, find_in_list) = (CommandId(1), CommandId(2));
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
    let list = tree
        .attach(tree.root(), rect(0.0, 0.0, 200.0, 300.0))
        .unwrap();
    let mut commands = CommandRegistry::new();
    commands.insert(find, CommandState::default()).unwrap();
    commands
        .insert(find_in_list, CommandState::default())
        .unwrap();
    let command_f = Shortcut {
        key: Key::Character('f'),
        modifiers: ShortcutModifiers {
            command: true,
            ..ShortcutModifiers::default()
        },
    };
    commands
        .bind_shortcut(ShortcutScope::Application, command_f, find)
        .unwrap();
    commands
        .bind_shortcut(
            ShortcutScope::Focused {
                window: 1,
                node: list,
            },
            command_f,
            find_in_list,
        )
        .unwrap();
    assert_eq!(commands.resolve_shortcut(command_f, 1, None), Some(find));
    assert_eq!(
        commands.resolve_shortcut(command_f, 1, Some(list)),
        Some(find_in_list)
    );
}
