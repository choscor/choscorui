# Notes sample

An external application built on choscorui exactly as a third party would build one:

- It has its own Cargo workspace and depends only on the `choscorui` facade crate.
- `#![forbid(unsafe_code)]` is set. There are no FFI declarations, and no backend crate (`choscorui-macos`, `choscorui-core`, …) is named directly.

## What it shows

| Feature | Where |
| --- | --- |
| Several native windows, each with its own retained tree | `app::Notes::open_window`, `ui::NoteView` |
| Native menu bar with shortcuts, rebuilt on language change | `app::Notes::install_menu` |
| Command buttons (custom-drawn `PillButtonRecipe`), keyboard focus, text field | `ui::NoteView::{pointer, key, text_input, paint}` |
| Brand theme from design tokens, runtime light/dark switch across all windows, system appearance at launch, Increase Contrast and Reduce Motion | `ui::theme`, `NoteView::set_dark`, `set_accessibility_preferences` |
| Translations loaded as resources, plural forms, locale number formatting, runtime language switch, right-to-left mirroring (Hebrew) | `resources/strings/*.lang`, `strings.rs`, `NoteView::relabel` |
| Translation validation and missing-resource fallback | `strings::load` |
| VoiceOver semantics and assistive actions | `NoteView::{sync_semantics, accessibility_action}` |
| Capability query | `choscorui::capabilities()` in the autopilot |

## Run

Prerequisites are the same as for the toolkit: macOS 15 or later, the Xcode Command Line Tools (`xcode-select --install`), and Rust 1.88 or later.

```sh
cd samples/notes
cargo run                       # interactive; ⌘N new window, ⌘T theme, ⌘L language, ⌘W close
NOTES_LANG=he cargo run         # start in Hebrew (right to left)
cargo test                      # layout, input, theme, and translation tests (no window needed)
NOTES_AUTOPILOT=1 cargo run     # scripted end-to-end check; prints "OK (0 failures)"
```

The autopilot opens three windows (through the menu and a toolbar click), types with keyboard focus traversal, and switches the theme. It then switches the language to Vietnamese and to Hebrew, checking mirrored toolbars, translated titles, and the menu. Finally it closes every window and verifies that AppKit released them.
