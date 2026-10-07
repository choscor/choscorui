# API guide

This guide walks through building an application with choscorui. Each section names the types to use and points to a working example. [`samples/notes`](../samples/notes/README.md) is a complete small application. The gallery (`examples/gallery`) and stress gallery (`examples/stress`) cover more controls.

## Requirements

| Requirement | Version | Check or install |
| --- | --- | --- |
| macOS | 15 or later, Apple silicon or Intel | `sw_vers -productVersion` |
| Xcode Command Line Tools (SDK, linker) | any release supporting macOS 15 | `xcode-select --install` |
| Rust | 1.88 or later, edition 2024 | `rustup toolchain install 1.88` |

There are no other dependencies. The toolkit links only system frameworks (AppKit, Foundation, Core Foundation, Core Graphics, Core Text, ImageIO, UserNotifications) and the `unicode-segmentation` crate. Windows and Linux backends are not supported yet: on those systems the facade compiles, but `choscorui::platform` is absent and `capabilities()` reports nothing as supported.

Every example and sample builds with standard Cargo commands:

```sh
cargo run -p choscorui-gallery                       # gallery
cargo run -p choscorui-stress                        # large data, canvas, docking
cd samples/notes && cargo run                        # external sample app
```

## Depend on the facade

```toml
[dependencies]
# Not on crates.io yet: point at a checkout of this repository.
choscorui = { path = "../choscorui-v2/crates/choscorui" }
```

Depend on `choscorui` only. It forbids `unsafe` itself and never exposes native handles. Its modules are:

| Module | Contents |
| --- | --- |
| `choscorui::prelude` | The types most applications start with |
| `choscorui::core` | Geometry, retained tree, layout, text, models, themes, semantics, settings, locale, resources, scene, rich text |
| `choscorui::controls` | Controls, item views, dialogs, popups, toolbar, status bar, docking |
| `choscorui::compose` | Declarative composition, reactive bindings, state machines |
| `choscorui::platform` | The current OS backend: application, windows, menus, dialogs, clipboard, desktop integration, text shaping |
| `choscorui::capabilities()` | What the backend supports, so you can query instead of checking the OS |

The backend crates (`choscorui-macos` and others) are implementation details. See [VERSIONING.md](VERSIONING.md) for what is stable.

## The application loop

choscorui does not own your main loop. You create an `Application` on the main thread, open windows, and pump events:

```rust
use choscorui::prelude::*;
use choscorui::core::DisplayList;
use std::time::Duration;

fn main() {
    let app = Application::new().expect("AppKit must start on the main thread");
    let mut window = app
        .create_window(WindowOptions::new("Hello", 640.0, 400.0))
        .expect("window");
    let paint = |size: Size| {
        let mut list = DisplayList::new();
        let tokens = DesignTokens::light();
        list.fill_rect(Rect::new(Point::new(0.0, 0.0), size), tokens.background);
        let _ = list.draw_text(Point::new(24.0, 48.0), "Xin chào", "Helvetica", 24.0, tokens.content);
        list
    };
    window.set_display_list(paint(window.content_size()));
    window.show();
    app.activate();
    'run: loop {
        app.dispatch_next_event(Duration::from_millis(250));
        for event in window.take_events() {
            match event {
                WindowEvent::Closing => break 'run,
                WindowEvent::Resized { size, .. } => window.set_display_list(paint(size)),
                _ => {}
            }
        }
    }
}
```

- `dispatch_next_event(timeout)` waits for one native event and dispatches it.
- `Window::take_events()` drains the queue of `WindowEvent`s: resize, scale change, focus, pointer, key, text, IME, drag and drop, and close.
- `set_display_list` replaces what the window shows. Painting happens on the next frame, and `last_paint()` reports when it finished.
- `Application::window_count()` reports native windows that are open or not yet released. Leak checks use it.

Multiple windows are independent `Window` values. Each window usually has its own `Tree`, while models, commands, themes, and the localizer are shared. `Notes::deliver` in the sample routes each window's events to its view and then runs the commands they activated.

### Threads and timers

All UI types are main-thread only (`!Send`):
- **Work from other threads.** Call `Application::poster()` to get a `Poster`, which is `Send`. `Poster::post(closure)` runs the closure on the main loop.
- **Timers.** `schedule_once` and `schedule_repeating` return a `Timer`. Dropping the timer cancels it.
- **Panics in callbacks.** A panic inside a posted task, timer, or menu callback is caught and counted. It never unwinds into AppKit.

## Retained tree and identity

`Tree` owns the UI structure. `tree.attach(parent, bounds)` returns a `NodeId`, which keeps its identity until the node is detached. The tree provides:
- **Bounds** in absolute logical points.
- **Hit testing:** `hit_test(point)` checks the deepest node, testing the last child first.
- **Focus:**
  - `set_focusable` marks a node focusable.
  - `set_focus` moves focus to a node.
  - `focus_next(reverse)` moves focus in depth-first order and wraps; map Tab and Shift-Tab to it.
- **Pointer capture** for drags.
- **Dirty flags:** `invalidate(id, Dirty::PAINT | LAYOUT | SEMANTICS)`.
- **Dispatch:** capture, target, and bubble phases for pointer and key events.

Controls do not own nodes. You attach a node, keep the control's state object next to it, and pass both to the control.

## Layout

| Type | Use |
| --- | --- |
| `LinearLayout` | Rows or columns with fixed and weighted flex children, padding, spacing, cross alignment, and RTL order |
| `GridLayout` | Fixed and flexible tracks, spans, and RTL columns |
| `FormLayout` | Label and field pairs, with labels aligned against the field column |
| `ScrollViewport` | Clamped scrolling of a content subtree, and reveal-rect |
| `compose_main_window` | Toolbar areas, status bar, and central content |
| `DockLayout` | Dockable, tabbed, and floating panels with keyboard-resizable splitters |

`apply(&mut tree, bounds)` writes bounds into the tree. A layout that cannot satisfy its minimum sizes returns `LayoutError::InsufficientSpace` without moving any node. Decide what to hide; the sample hides its toolbar in very narrow windows. For text and custom controls, measure with `measure` hooks and `Constraints`, and see [CUSTOM_CONTROLS.md](CUSTOM_CONTROLS.md).

## Controls

Every control follows the same shape:

```rust
let node = tree.attach(root, Rect::default())?;
tree.set_focusable(node, true)?;
let mut button = Button::new("Save", SAVE);

// Input: returns what happened (here, the activated command).
if let Some(command) = button.handle_with_commands(&mut tree, node, ButtonInput::Pointer(event), &commands)? {
    run(command);
}
// Paint: a visual state goes to a swappable style or recipe.
let state = button.visual_state_with_commands(&tree, node, &commands)?;
paint_button(&PillButtonRecipe::default(), &mut list, tree.bounds(node).unwrap(), state, &tokens, |list, slots| {
    /* draw the label into slots.content with slots.content_color */
});
// Accessibility: semantics go into the SemanticTree.
semantics.set(node, button.semantics(&tree, node).shared(), &tree)?;
```

Each control owns its behavior: input handling, focus, validation, and accessibility semantics. Painting is separate and replaceable. The behavior is a contract, documented in [CONTROLS.md](CONTROLS.md), with semantics in [ACCESSIBILITY.md](ACCESSIBILITY.md#control-contracts).

The available controls are:
- **Basic:** buttons, checkboxes, radio groups, tabs, sliders, spin boxes, progress bars, labels, and separators.
- **Text:** text fields with validation, input masks, and IME, plus multi-line editors.
- **Data:** virtualized list, tree, table, and grid views (`ItemView`), headers, combo boxes, date/time editors, and calendars.
- **Popups and dialogs:** popup menus, tooltips, and dialogs.
- **Window chrome:** toolbars, status bars, and docking.
- **Canvas:** an interactive 2D canvas.

Write your own controls with `CustomControl` and `ControlHost`.

## Commands, shortcuts, and menus

- `CommandRegistry` holds each command's enabled and checked state.
  - `activate(id)` returns the id only while the command is enabled.
  - Buttons bound to a command follow its state (`handle_with_commands`, `visual_state_with_commands`).
- `bind_shortcut(scope, shortcut, command)` registers a chord. When several scopes bind the same chord, the focused scope wins, then the window, then the application.
- `resolve_shortcut` finds the command for a chord.
- On macOS, build the menu bar with `MenuBarModel`/`MenuSpec` and `Application::install_main_menu`.
  - Menu choices arrive through `MainMenu::take_activations()`.
  - The first menu is the application menu.
  - Installing a new menu replaces the old one; do this on a language switch.
  - `ContextMenu` and `Window::show_context_menu` show native context menus.

## Painting and theming

Painting goes through a renderer-neutral `DisplayList`, which holds rectangles, rounded rectangles, paths, text, images, clips, transforms, opacity, and shadows. The macOS backend replays it with Core Graphics.

Colors and metrics come from `DesignTokens`, resolved by `ThemeResolver`. Precedence, from lowest to highest:
1. The application theme.
2. Scoped overrides.
3. A node's own instance override.
4. The user's accessibility preferences.

On a change, `set_application` marks only the affected subtrees dirty, for paint or for layout. `system_prefers_dark()` reads the system appearance, and `system_accessibility_preferences()` reads Increase Contrast, Reduce Motion, and text scale. See [THEMING.md](THEMING.md) and the recipes in [COOKBOOK.md](COOKBOOK.md).

## Text

- **Shaping:** `CoreTextShaper` (in `platform`) implements the `TextShaper` trait.
- **Layout:** `TextLayoutCache::layout` returns a `TextLayout` used for measurement, painting, hit testing, carets, and selection. It handles line breaking, font fallback, and bidirectional text, and caches by text, style, width, and scale.
- **Large documents:** `layout_paragraphs` relayouts only the changed paragraph.
- **Single lines:** `ShapedLine` measures single-line labels.
- **Editing:** `TextField` and `TextEditor` implement editing, with IME through `WindowEvent::Ime` and `Window::set_text_input_state`. `TextBuffer` gives delta-based undo, capped at `UNDO_LIMIT` steps.
- **Rich text:** `RichDocument` and `RichLayout`; see [RICH_TEXT.md](RICH_TEXT.md).

## Accessibility

Keep a `SemanticTree` per window:
1. Each frame, call `set(node, semantic_node, &tree)` for the nodes that changed.
2. Call `sync_focus` to update focus.
3. Send `snapshot(&tree)` and the collected changes to `Window::set_accessibility`.

VoiceOver actions arrive through `Window::take_accessibility_actions()`. Route them to the control that owns the node. See [ACCESSIBILITY.md](ACCESSIBILITY.md).

## Localization and resources

Use these types:
- `Locale`, `Catalog`, and `Localizer` for strings, plurals, and runtime language switches.
- `Locale` formatting for numbers and dates.
- `ResourceBundle` for packaged assets.

Lookup failures are values you handle, never panics. See [LOCALIZATION.md](LOCALIZATION.md).

## Errors and capabilities

Fallible calls return `Result` with a domain error enum. All errors implement `std::error::Error`. Optional platform features are reported by `choscorui::capabilities()` as `Supported`, `Unsupported`, or `Conditional(reason)`. For example, notifications need an app bundle. See [VERSIONING.md](VERSIONING.md#error-reporting).

## Testing your UI

Everything except the window itself is plain Rust state, so most UI tests need no window. The tree, layout, controls, semantics, display lists, themes, and locale all run headless. Keep the platform-neutral part of each window in a struct, like `ui::NoteView`, that you can drive with synthetic events and inspect through its display list and semantic snapshot. Run native checks separately with a scripted autopilot, as `NOTES_AUTOPILOT=1` does.
