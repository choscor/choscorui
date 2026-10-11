# Notes sample

The notes sample is an external application. It uses ChoscorUI in the same way as an application from a different developer:

- It has its own Cargo workspace and depends only on the `choscorui` facade crate.
- It sets `#![forbid(unsafe_code)]`. It has no FFI declarations, and it does not name a backend crate such as `choscorui-macos`.

## Features

| Feature | Location |
| --- | --- |
| Many native windows, each with its own retained tree | `app::Notes::open_window`, `ui::NoteView` |
| Native menu bar with shortcuts, built again when the language changes | `app::Notes::install_menu` |
| Command buttons with `PillButtonRecipe`, keyboard focus, and a text field | `ui::NoteView::{pointer, key, text_input, paint}` |
| Brand theme from design tokens, light and dark switch for all windows, system appearance at launch, Increase Contrast, and Reduce Motion | `ui::theme`, `NoteView::set_dark`, `set_accessibility_preferences` |
| Translations as resources, plural forms, number formats, language switch at run time, and right-to-left mirroring (Hebrew) | `resources/strings/*.lang`, `strings.rs`, `NoteView::relabel` |
| Translation validation and a fallback for missing resources | `strings::load` |
| Accessibility semantics and assistive actions | `NoteView::{sync_semantics, accessibility_action}` |
| Capability query | `choscorui::capabilities()` in the autopilot |

## Run

You need the same tools as for the toolkit: macOS 15 or later, the Xcode Command Line Tools (`xcode-select --install`), and Rust 1.88 or later.

```sh
cd samples/notes
cargo run                       # interactive: ⌘N new window, ⌘T theme, ⌘L language, ⌘W close
NOTES_LANG=he cargo run         # start in Hebrew (right to left)
cargo test                      # layout, input, theme, and translation tests (no window)
NOTES_AUTOPILOT=1 cargo run     # automatic end-to-end check; prints "OK (0 failures)"
```

The autopilot opens three windows, through the menu and a toolbar click. It types text, moves the keyboard focus, and switches the theme. Then it switches the language to Vietnamese and to Hebrew. It checks the mirrored toolbars, the translated titles, and the menu. At the end, it closes all windows and makes sure that AppKit released them.
