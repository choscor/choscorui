# Theming and customization

## Design tokens

`DesignTokens` (in `choscorui-core`) holds every themeable value:

| Group | Tokens | Change triggers |
| --- | --- | --- |
| Color | `background`, `surface`, `content`, `accent`, `danger` | paint |
| Typography | `font_size`, `font_weight`, `line_height` | layout + paint + semantics |
| Spacing and shape | `spacing`, `radius`, `border_width` | layout + paint + semantics |
| Elevation | `elevation` | paint |
| Motion | `motion.duration_ms`, `motion.reduced` | paint |
| Contrast | `contrast` | paint |

`DesignTokens::text_style(family)` turns the typography tokens into a
`TextStyle` for the shaper. `ThemeResolver` reports each change as
`ThemeInvalidation::{None, Paint, Layout}` and marks only the affected
subtree dirty.

## Resolution order

From lowest to highest precedence:

1. Application theme (`set_application`). Light and dark are two
   application themes; `choscorui_macos::system_prefers_dark` reports the
   system appearance.
2. Scoped overrides on ancestors, root first (`set_scope`).
3. The node's own instance override (`set_instance`).
4. User accessibility preferences (`set_accessibility_preferences`): text
   scale multiplies `font_size`, high contrast forces `Contrast::High`, and
   reduced motion sets `motion.reduced` with zero duration. On macOS,
   `system_accessibility_preferences` reads Increase Contrast and Reduce
   Motion from System Settings.

An update that would produce invalid tokens is rejected and the previous
resolved theme stays in effect.

## Visual recipes and slots

A control's appearance is a recipe, separate from its behavior. A
`ButtonRecipe` lays out and paints five slots in a fixed order:
`Background`, `Icon`, `Content`, `Indicator` (focus), `Overlay`
(pressed/disabled). `paint_button` calls each slot and lets the caller draw
the label into the content rect with the recipe's alignment and color.

Recipes receive only the visual state (`enabled`, `hovered`, `pressed`,
`focused`) and the resolved tokens. They cannot reach input, focus,
commands, or semantics, so swapping a recipe at runtime, even while a key
or pointer is held, never changes activation, keyboard handling, focus, or
the accessibility tree (`controls/tests/recipe.rs`).

Built-in recipes:

- `ButtonStyle`: flat, square fill, optional leading rail, leading label,
  top and bottom focus bars.
- `PillButtonRecipe`: rounded and elevated, with a trailing icon badge,
  centered label, outset focus ring, and pressed/disabled overlay.

The gallery switches between these at runtime along with the dark and
light token sets.

## Animation

`AnimationClock` runs deterministically on caller-supplied monotonic time:

- **Easing:** `Linear` or `EaseInOut`. Theme motion tokens override the
  duration.
- **Interruption:** a new animation on the same node and channel starts
  from the current sampled value.
- **Reduced motion:** with `motion.reduced` or a zero duration, an
  animation finishes immediately at its target.
- **Node removal:** removing a node cancels its animations on the next
  `advance`.
- **Frame scheduling:** keep scheduling OS frames only while
  `needs_frame()` is true; an idle clock requests no frames.
- **Invalidation:** animations mark nodes paint-dirty only, never layout.

## Native look and system UI

Custom-drawn controls are not AppKit controls. Native-looking themes are
best-effort emulation and may drift between macOS releases. These parts use
system UI directly and are not themed:

- the menu bar and context menus (`NSMenu`)
- open and save panels and alerts (`NSOpenPanel`, `NSSavePanel`,
  `NSAlert`)
- the IME candidate window
- drag images supplied by other applications
- window chrome (title bar and traffic lights)
