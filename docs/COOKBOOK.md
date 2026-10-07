# Customization cookbook

These are short recipes for common customizations. Each snippet below comes from a test in [`crates/choscorui/tests/cookbook.rs`](../crates/choscorui/tests/cookbook.rs), so it compiles against the current API, and the test checks its claim. Background is in [THEMING.md](THEMING.md) and the [API guide](GUIDE.md).

The rule behind every recipe: **controls own behavior, you own appearance**. Recipes, delegates, and tokens change what is painted. They never change input handling, focus, or accessibility semantics.

## A brand theme

Start from a stock theme and override the tokens that make up your brand:

```rust
fn brand(dark: bool) -> DesignTokens {
    let base = if dark { DesignTokens::dark() } else { DesignTokens::light() };
    DesignTokens {
        accent: Color::rgba8(0, 128, 128, 255),
        radius: 8.0,
        ..base
    }
}

let mut themes = ThemeResolver::new(brand(false));
themes.set_application(&mut tree, brand(true))?; // switch at runtime
```

`set_application` returns the `ThemeInvalidation` it caused. A color-only change marks subtrees paint-dirty; a metric change also marks them layout-dirty. For several windows, keep one `ThemeResolver` per window tree and apply the same tokens to each, as `samples/notes` does.

To follow the system, read the appearance and the accessibility preferences at launch:

```rust
let dark = choscorui::platform::system_prefers_dark().unwrap_or(false);
let mut themes = ThemeResolver::new(brand(dark));
if let Ok(preferences) = choscorui::platform::system_accessibility_preferences() {
    themes.set_accessibility_preferences(&mut tree, preferences)?;
}
```

Preferences apply on top of every theme. Increase Contrast forces `Contrast::High`, Reduce Motion zeroes animation durations, and text scale multiplies `font_size`.

## Restyle one subtree

Give part of the UI different tokens, such as a "danger zone" whose accent is the danger color, with a scoped override:

```rust
let tokens = themes.resolve(&tree, root)?;
themes.set_scope(&mut tree, danger_zone, ThemeOverride {
    accent: Some(tokens.danger),
    ..ThemeOverride::default()
})?;
// Descendants of `danger_zone` resolve the danger accent; the rest of the tree is unchanged.
```

`set_instance` overrides a single node the same way. Invalid results, such as a negative radius or a non-finite size, are rejected with `ThemeError`, and the previous theme stays.

## A custom button look

Implement `ButtonRecipe` to replace how a button is drawn. A recipe sees only the visual state and the tokens:

```rust
struct OutlineRecipe;

impl ButtonRecipe for OutlineRecipe {
    fn layout(&self, bounds: Rect, state: ButtonVisualState, tokens: &DesignTokens) -> SlotLayout {
        SlotLayout {
            content: bounds.inset(tokens.spacing),
            alignment: ContentAlignment::Center,
            icon: None,
            content_color: if state.enabled { tokens.accent } else { tokens.content.with_alpha(0.4) },
        }
    }

    fn paint_slot(&self, slot: Slot, list: &mut DisplayList, context: &SlotContext<'_>) {
        let tokens = context.tokens;
        match slot {
            Slot::Background if context.state.pressed => {
                let _ = list.fill_rounded_rect(context.bounds, tokens.radius, tokens.accent.with_alpha(0.15));
            }
            Slot::Indicator => {
                let width = if context.state.focused { tokens.border_width * 2.0 } else { tokens.border_width };
                let _ = list.stroke_rounded_rect(context.bounds, tokens.radius, context.layout.content_color, width);
            }
            _ => {}
        }
    }
}

paint_button(&OutlineRecipe, &mut list, bounds, button.visual_state(&tree, node), &tokens, |list, slots| {
    // draw the label into slots.content using slots.content_color
});
```

Slots paint in a fixed order: `Background`, `Icon`, `Content`, `Indicator` (focus), `Overlay`. You can swap recipes at runtime, even mid-press, without changing activation or focus. The test checks that Enter still activates the button.

Other controls take a style struct with one color per state. Examples are `CheckboxStyle`, `TextFieldStyle`, `SliderStyle`, and `TabStyle`. Build these styles from tokens in the same way.

## Custom list, tree, and table cells

`ItemView` virtualizes rows and calls an `ItemDelegate` to paint each realized cell:

```rust
struct BadgeDelegate;

impl ItemDelegate for BadgeDelegate {
    fn paint_cell(&self, list: &mut DisplayList, rect: Rect, model: &dyn ItemModel,
                  key: ItemKey, column: usize, state: ItemState) {
        let tokens = DesignTokens::light();
        if state.selected {
            list.fill_rect(rect, tokens.accent.with_alpha(0.2));
        }
        if let Value::Text(text) = model.data(key, column, Role::Display) {
            let _ = list.draw_text(Point::new(rect.origin.x + 28.0, rect.origin.y + rect.size.height * 0.7),
                                   &text, "Helvetica", 13.0, tokens.content);
            // …badge drawn at the leading edge
        }
    }
}

view.paint(&mut list, &model, &BadgeDelegate, &[], focused);
```

`ItemState` includes `selected`, `current`, `focused`, `depth`, `has_children`, and `expanded`, so tree disclosure and focus rings belong to the delegate too. Accessibility names come from the model's `Role::Display` and `Role::Accessibility` data, not from what the delegate paints. For editing, use `EditorFactory` (see [CONTROLS.md](CONTROLS.md)).

## Custom validation

Text fields show three validation states: `Valid`, `Intermediate` (incomplete, as while typing), and `Invalid(message)`. Compute the state with your own rule and set it:

```rust
fn validate_amount(field: &mut TextField, tree: &mut Tree, node: NodeId, locale: &Locale) {
    let state = match locale.parse_decimal(field.value()) {
        _ if field.value().is_empty() => ValidationState::Intermediate,
        Ok(value) if value > 0.0 => ValidationState::Valid,
        Ok(_) => ValidationState::Invalid("the amount must be positive".into()),
        Err(_) => ValidationState::Invalid("enter a number such as 1.234,50".into()),
    };
    field.handle(tree, node, TextFieldInput::SetValidation(state)).unwrap();
}
```

- The field paints the matching indicator through its `TextFieldStyle`.
- An `Invalid` message is spoken once by VoiceOver (`semantics().announcement`).
- Parsing with the user's `Locale` accepts their own decimal convention.

`NumberInput`, `InputMask`, and `DateInput` are ready-made rules for common cases.

## Context-specific shortcuts

Bind the same chord at different scopes. The innermost scope wins: focused node, then window, then application.

```rust
commands.bind_shortcut(ShortcutScope::Application, command_f, find)?;
commands.bind_shortcut(ShortcutScope::Focused { window: 1, node: list }, command_f, find_in_list)?;

commands.resolve_shortcut(command_f, 1, None);       // Some(find)
commands.resolve_shortcut(command_f, 1, Some(list)); // Some(find_in_list)
```

A duplicate chord within one scope is rejected with `CommandError`. Disable a command with `set_enabled`, and its bound buttons and menu items follow.

## More customization points

| You want to | Use | Details |
| --- | --- | --- |
| Write a control from scratch (measure, input, paint, semantics) | `CustomControl` + `ControlHost` | [CUSTOM_CONTROLS.md](CUSTOM_CONTROLS.md) |
| Animate a property, respecting Reduce Motion | `AnimationClock` | [THEMING.md](THEMING.md#animation) |
| Draw shapes or a large zoomable scene | `Scene` + `CanvasView` | [CANVAS.md](CANVAS.md) |
| Show styled documents | `RichDocument` + `RichLayout` | [RICH_TEXT.md](RICH_TEXT.md) |
| Remember window geometry and panel layout | `Settings`, `WindowGeometry`, `DockLayout::save`/`restore` | [SETTINGS.md](SETTINGS.md) |
| Choose native or custom dialogs | `FileDialogSpec`, `AlertSpec`, `InputDialog`, `ColorPicker` | [DIALOGS.md](DIALOGS.md) |
| Translate and mirror the UI | `Localizer`, `LayoutDirection` | [LOCALIZATION.md](LOCALIZATION.md) |
