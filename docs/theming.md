# Theming and customization

This document describes design tokens, theme resolution, visual recipes, and
animation. The second part gives customization recipes with tested code.

The rule behind all theming: **controls own behavior, you own appearance**.
Tokens, recipes, and delegates change what the toolkit paints. They do not
change input handling, focus, or accessibility semantics.

## Design tokens

`DesignTokens` in `choscorui-core` holds each themeable value. A theme is a
`BaseColor` in a `ColorScheme`. `DesignTokens::new(base, scheme)` builds any
of the 7 bases (`Neutral`, `Stone`, `Zinc`, `Mauve`, `Olive`, `Mist`,
`Taupe`) in `Light` or `Dark`. `DesignTokens::light()` and
`DesignTokens::dark()` are the `Neutral` base.

| Group | Fields and helpers | Change invalidates |
| --- | --- | --- |
| Color | `base`, `scheme`, `palette`, `color(role)` | paint |
| Shape | `radius`, `radius(step)`, `radius_for(step, bounds)`, `border_width` | layout, paint, semantics |
| Spacing | `spacing`, `space(units)` | layout, paint, semantics |
| Typography | `text_scale`, `font_sans`, `font_mono`, `text(size, weight)`, `preset(preset)` | layout, paint, semantics |
| Focus ring | `ring_width`, `ring_alpha`, `ring_color(color)` | `ring_width`: layout; `ring_alpha`: paint |
| Shadows | `shadow(step)` | paint |
| Motion | `motion.duration_ms`, `motion.easing`, `motion.reduced` | paint |
| Contrast | `contrast` (`Contrast::Normal` or `Contrast::High`) | paint |

### Color roles and palettes

A `Palette` holds one `Color` for each `ColorRole`. Controls ask for a role,
not for a raw color, so one control follows each base and scheme. The roles
are `Background`, `Card`, `Popover`, `Primary`, `Secondary`, `Muted`,
`Accent`, `Destructive`, `Border`, `Input`, `Ring`, and `Chart1` to `Chart5`.
The `Sidebar` roles repeat these for a sidebar. Each surface also has a
`…Foreground` role for the content on it.

The palette tables are OKLCH values. `Color::from_oklch` converts them to
sRGB and clips each channel to the sRGB gamut. Some roles keep an alpha
value. For example, the dark `Border` is white at 10%.

### Scales

All sizes are in logical points. One CSS pixel is one point, and one `rem`
is 16 points.

| Scale | Steps |
| --- | --- |
| `RadiusScale` | `Sm`, `Md`, `Lg`, `Xl`, `Xl2`, `Xl3`, `Xl4` are 0.6, 0.8, 1, 1.4, 1.8, 2.2, and 2.6 × `radius` (10). `None` is 0 and `Full` makes a pill. |
| Spacing | `space(n)` is `n` × `spacing` (4). |
| `TextSize` | `Xs` 12/16, `Sm` 14/20, `Base` 16/24, `Lg` 18/28, `Xl` 20/28, `Xl2` 24/32, `Xl3` 30/36, `Xl4` 36/40 (size/line height). |
| `Tracking` | `Tighter` −0.05 em, `Tight` −0.025 em, `Normal`, `Wide` 0.025 em, `Wider` 0.05 em, `Widest` 0.1 em. |
| `ShadowScale` | `Xs2`, `Xs`, `Sm`, `Md`, `Lg`, `Xl`, `Xl2`, and the inset steps `InsetXs2`, `InsetXs`, `InsetSm`. |

`text` and `preset` return a `TypeSpec`: family, size, line height, weight,
slant, and tracking. `TypeSpec::text_style` makes a `TextStyle` for the
shaper. The `TextPreset` values are the typography rules: `H1` to `H4`,
`Paragraph`, `Lead`, `Large`, `Small`, `Muted`, `Blockquote`, `List`, and
`InlineCode`.

`font_sans` and `font_mono` are `FontFamily` values. The defaults
`FontFamily::SYSTEM_UI` and `FontFamily::SYSTEM_MONO` select the system faces.
The toolkit bundles no fonts.

A `Shadow` has an offset, a blur radius, a spread, a color, and an `inset`
flag, with the meaning of CSS `box-shadow`. `ShadowLayers` holds up to three
shadows. `DisplayList::draw_shadows` draws them in order.

`DisplayList::draw_ring` draws a focus ring. The ring is a stroke of
`ring_width` just outside the shape, and it follows the corner radius.

`ThemeResolver` returns a `ThemeInvalidation` for each change: `None`, `Paint`,
or `Layout`. It marks only the changed subtree dirty. A `Layout` change also
marks the subtree paint-dirty and semantics-dirty.

### Overrides

`ThemeOverride` has an optional value for each token. The resolver applies an
override in this order:

1. A `base` or `scheme` builds the palette of that base and scheme again.
2. A `palette` replaces the full palette.
3. Each entry in `roles` replaces one color role.
4. The other fields replace their tokens.

`ThemeOverride::role(role, color)` and `ThemeOverride::theme(base, scheme)`
make the usual overrides.

## Resolution order

`ThemeResolver::resolve` computes the tokens for one node. It applies four
layers, from the lowest to the highest precedence.

```mermaid
flowchart TD
    A["Application theme: set_application"] --> B["Scoped overrides on ancestors, root first: set_scope"]
    B --> C["Instance override on the node: set_instance"]
    C --> D["Accessibility preferences: set_accessibility_preferences"]
    D --> E["Resolved DesignTokens"]
```

1. The application theme comes from `set_application`. Light and dark are two
   application themes.
2. Scoped overrides on ancestors apply root first. Use `set_scope` to add one
   and `remove_scope` to remove it.
3. The instance override from `set_instance` applies to the node itself.
   `set_instance` and `set_scope` write the same override slot for a node.
4. The accessibility preferences from `set_accessibility_preferences` always
   apply last.

The accessibility preferences change the tokens as follows:

- `text_scale` multiplies the `text_scale` token, and thus each type step.
- `high_contrast` sets `contrast` to `Contrast::High`. With high contrast, the
  resolver makes `MutedForeground` reach 4.5:1 against its surfaces. It makes
  `Border`, `Input`, and `Ring` reach 3:1 against `Background`, and sets
  `ring_alpha` to 1. These colors become opaque.
- `reduced_motion` sets `motion.reduced` and sets `motion.duration_ms` to zero.

A change that gives invalid tokens returns `ThemeError`, and the previous theme
stays in effect. Invalid tokens include a negative or non-finite size, a
`text_scale` of zero, an empty font family, and a `ring_alpha` outside
`0..=1`. A `text_scale` that is not positive and finite returns
`ThemeError::InvalidPreferences`.

## Style recipes

The `choscorui_controls::style` module holds one recipe system for every
control. A recipe maps a `StyleQuery` and the tokens to a `ResolvedStyle`.

A `StyleQuery` has these keys:

- `kind`: the `ControlKind`, for example `Button` or `Select`.
- `slot`: the part of the control, for example `Root`, `Trigger`, `Thumb`, or
  `Item`.
- `variant`: the control's variant enum, for example `ButtonVariant::Outline`.
- `size`: a `ControlSize`. A control uses the subset that its reference
  defines.
- `state`: a `VisualState` bitset of hovered, pressed, focus-visible,
  disabled, invalid, checked, indeterminate, selected, open, highlighted, and
  focused.

A `ResolvedStyle` holds every visual value of one slot. These include the
background, foreground, border, corner radii, shadows, ring, logical padding,
minimum size, gap, text, icon size, opacity, cursor, and duration.

`Styles` is the registry and the cache:

- `Styles::resolve` returns the style of a query. It caches the result by the
  token fingerprint and the query, so paint does not compute a style again
  for each node.
- `Styles::set_recipe` registers a `Rc<dyn Recipe>` for one control kind in a
  `RecipeSet`. A scope selects the set with the `recipes` token in a
  `ThemeOverride`. Use `RecipeSet::DEFAULT` to replace a look globally.
- A recipe returns `None` for a query that it does not style. The default
  recipe then styles that query.

A recipe sees only the query and the tokens. It cannot read input, focus,
commands, or semantics.

The default recipe gives each control the reference values. The focus-visible
state sets the border to `ring` and adds a 3 pt ring at the ring alpha. The
invalid state uses a `destructive` ring at 20% (40% in dark) and a
`destructive` border. The disabled state sets the opacity to 50%. With
`motion.reduced`, every duration is zero.

`paint_slot` is the general slot painter. It paints the drop shadows, the
ring, the background, the inset shadows, and the border, in that order. Then
it calls a closure with the box inside the padding. `draw_text` and
`draw_icon` draw the content of a slot.

## Animation

`AnimationClock` in `choscorui-core` runs on monotonic time that the caller
supplies. Call `animate` to start an animation on a node and an
`AnimationKey` channel. Call `advance` with the current time to get the
sampled values.

- **Easing:** `Easing::Linear`, `Easing::EaseInOut`, `Easing::In`,
  `Easing::Out`, `Easing::Standard`, or `Easing::CubicBezier`. A
  `CubicBezier` has the meaning of CSS `cubic-bezier(x1, y1, x2, y2)`. The
  default curve `CubicBezier::STANDARD` is `(0.4, 0, 0.2, 1)`, and the default
  duration is 150 ms. A non-zero
  `motion.duration_ms` token replaces the duration in the `AnimationSpec`.
- **Interruption:** a new animation on the same node and channel starts from
  the current sampled value.
- **Reduced motion:** when `motion.reduced` is set or the duration is zero,
  the animation finishes immediately at its target value.
- **Node removal:** `advance` drops the animations of a removed node.
- **Frame requests:** request an OS frame only while `needs_frame()` returns
  true. An idle clock requests no frames.
- **Time:** `advance` returns `AnimationError::TimeWentBackwards` for a time
  earlier than the previous time.
- **Invalidation:** animations mark nodes paint-dirty only, never
  layout-dirty.

## Native look and system UI

The toolkit draws its controls. They are not AppKit controls, and a theme that
looks native can differ from a given macOS release. These parts use system UI
directly, and tokens do not change them:

- the menu bar and context menus (`NSMenu`)
- open panels, save panels, and alerts (`NSOpenPanel`, `NSSavePanel`,
  `NSAlert`)
- the IME candidate window
- drag images from other applications
- the window title bar and its buttons

## Customization recipes

Each snippet in this part comes from a test in
[cookbook.rs](../crates/choscorui/tests/cookbook.rs). The snippets compile
against the current API, and each test checks the claim of its recipe. Some
snippets show only the relevant lines of the test.

### A brand theme

Start from a stock theme and replace the tokens of your brand:

```rust
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
```

Change the application theme at runtime with `set_application`:

```rust
let mut themes = ThemeResolver::new(brand(false));
assert_eq!(themes.resolve(&tree, root).unwrap().radius, 8.0);
themes.set_application(&mut tree, brand(true)).unwrap();
```

`set_application` returns the `ThemeInvalidation` that it caused. For more
than one window, keep one `ThemeResolver` for each window tree.

To follow the system settings on macOS, read them at launch:

1. Call `choscorui::platform::system_prefers_dark` and give the result to
   `brand`.
2. Call `choscorui::platform::system_accessibility_preferences`.
3. Give the preferences to `set_accessibility_preferences`.

`system_accessibility_preferences` reads Increase Contrast and Reduce Motion.
AppKit has no global text size, so `text_scale` stays `1.0`. Your application
can offer its own text scale setting.

### Restyle one subtree

Use a scoped override to give part of the UI different tokens. In this
example, the primary color of a "danger zone" subtree becomes the
destructive color:

```rust
let tokens = themes.resolve(&tree, root).unwrap();
themes
    .set_scope(
        &mut tree,
        danger_zone,
        ThemeOverride::role(ColorRole::Primary, tokens.color(ColorRole::Destructive)),
    )
    .unwrap();
```

Descendants of `danger_zone` get the destructive primary color. The other nodes do not
change. `set_instance` gives an override to one node in the same way.

### A custom button look

Implement `Recipe` to change how a control paints. The recipe sees only the
style query and the tokens. Return `None` for a slot that the default recipe
must style:

```rust
struct OutlineRecipe;

impl Recipe for OutlineRecipe {
    fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
        if query.slot != Slot::Root {
            return None;
        }
        let mut style = ResolvedStyle::new(tokens);
        style.foreground = tokens.color(ColorRole::Primary);
        style.border = tokens.color(ColorRole::Primary);
        style.border_width = if query.state.focus_visible() { 2.0 } else { 1.0 };
        style.radius = Corners::all(tokens.radius);
        style.padding = Edges::axes(tokens.space(3.0), 0.0);
        style.min_height = tokens.space(9.0);
        if query.state.pressed() {
            style.background = tokens.color(ColorRole::Primary).with_alpha(0.15);
        }
        if query.state.disabled() {
            style.opacity = 0.5;
        }
        Some(style)
    }
}
```

Register the recipe for the button kind. Then paint the button through a
`PaintContext`:

```rust
styles.set_recipe(RecipeSet::DEFAULT, ControlKind::Button, Rc::new(OutlineRecipe));
let mut cx = PaintContext::new(&mut list, bounds, &tokens, &styles, &EstimatedText);
button.paint(&mut cx, &tree, node);
```

The test paints the same button with the default recipe and gets different
display commands. Then it presses Enter, and the button still returns its
command.

### Custom list, tree, and table cells

`ItemView` virtualizes rows. It calls an `ItemDelegate` to paint each realized
cell:

```rust
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
            list.fill_rect(rect, tokens.accent.with_alpha(0.2));
        }
        if let Value::Text(text) = model.data(key, column, Role::Display) {
            let _ = list.draw_text(
                Point::new(rect.origin.x + 28.0, rect.origin.y + rect.size.height * 0.7),
                &text,
                "Helvetica",
                13.0,
                tokens.content,
            );
            let badge = rect.size.height - 8.0;
            let _ = list.fill_rounded_rect(
                Rect::new(
                    Point::new(rect.origin.x + 4.0, rect.origin.y + 4.0),
                    Size::new(badge, badge),
                ),
                badge / 2.0,
                tokens.accent,
            );
        }
    }
}
```

Give the delegate to `ItemView::paint`:

```rust
view.paint(&mut list, &model, &BadgeDelegate, &[], false);
```

`ItemState` has the fields `selected`, `current`, `focused`, `depth`,
`has_children`, and `expanded`. Thus the delegate also paints tree disclosure
and focus rings. Accessibility names come from the model's
`Role::Accessibility` and `Role::Display` data, not from the painted cell. For
cell editing, use `EditorFactory` (see [controls.md](controls.md)).

### Custom validation

A text field has three validation states: `Valid`, `Intermediate`, and
`Invalid(message)`. `Intermediate` is for incomplete input, for example while
the user types. Calculate the state with your rule and set it:

```rust
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
```

- The field paints the indicator for the state through its `TextFieldStyle`.
- The field semantics give an `Invalid` message as `announcement`.
- `Locale::parse_decimal` accepts the decimal format of the user's locale.

`NumberInput`, `InputMask`, and `DateInput` are ready-made rules for usual
cases.

### Context-specific shortcuts

Bind the same chord at different scopes. The innermost scope wins: the focused
node, then the window, then the application.

```rust
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
```

A second binding of a chord in the same scope returns
`CommandError::DuplicateShortcut`. `CommandRegistry::set_enabled` disables a
command, and `resolve_shortcut` then returns `None` for its chord.

## More customization points

| You want to | Use | Details |
| --- | --- | --- |
| Write a control (measure, input, paint, semantics) | `CustomControl`, `ControlHost` | [guide.md](guide.md) |
| Animate a property and obey Reduce Motion | `AnimationClock` | [Animation](#animation) |
| Draw shapes or a large zoomable scene | `Scene`, `CanvasView` | [canvas.md](canvas.md) |
| Show styled documents | `RichDocument`, `RichLayout` | [text.md](text.md) |
| Keep window geometry and panel layout | `Settings`, `WindowGeometry`, `DockLayout` | [guide.md](guide.md) |
| Use native or custom dialogs | `FileDialogSpec`, `AlertSpec`, `InputDialog`, `ColorPicker` | [platform-macos.md](platform-macos.md) |
| Translate and mirror the UI | `Localizer`, `LayoutDirection` | [localization.md](localization.md) |
