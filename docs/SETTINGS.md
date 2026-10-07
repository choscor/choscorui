# Settings and window state

`choscorui_core::Settings` stores per-user application settings. It is used to restore window geometry and dock layout across launches.

## Storage

| Platform | Path from `user_settings_path(app_id)` |
| --- | --- |
| macOS | `~/Library/Application Support/<app_id>/settings.txt` |
| Others (not shipped) | `$XDG_CONFIG_HOME/<app_id>/settings.txt`, or `~/.config/<app_id>/settings.txt` if that variable is unset |

`app_id` must be a reverse-DNS identifier, such as `com.example.editor`. Path separators and leading dots are rejected. In a sandboxed app, `HOME` points into the app container, so the file lands in the container automatically.

The file is UTF-8 text. The first line is the header `choscorui-settings <version>`. Each later line is `key = type:value`, where the type is `bool`, `int`, `float`, `text`, `list`, or `rect`. Lines starting with `#` are comments. Values equal to their schema default are not written, so changing a default reaches existing users.

## Lifecycle

| Situation | Behavior |
| --- | --- |
| File missing | `load` returns the defaults and `LoadReport.created`. |
| Older version | Migrations registered with `SettingsSchema::with_migration(from, fn)` run one step at a time. `LoadReport.migrated_from` is set and the settings are marked dirty. A missing step returns `MissingMigration`. |
| Newer version | `FutureVersion`. The file is not modified, so a downgrade cannot destroy the data. |
| Malformed | `Parse { line, message }`. `load_or_recover` renames the file to `settings.txt.corrupt` and starts from the defaults, recording the new path in `LoadReport.recovered_to`. |
| Unreadable | `Io { path, kind, message }`. This is never silently replaced with defaults. |
| Save | Parent directories are created. The file is written to `settings.txt.tmp`, synced with `fsync`, then renamed over the original, so a crash leaves either the old file or the new one. |

Keys use ASCII letters, digits, `.`, `_`, `-`, and `/`.

## Window geometry

1. On close, call `window.geometry().store(&mut settings, "window.main")`.
2. On launch, call `WindowGeometry::restore(&settings, "window.main")`.
3. Pass the result through `fit_to_screens(&screen_visible_frames()?, minimum)` before `window.set_geometry(...)`.

`fit_to_screens` handles screen changes between launches:
- If the saved title strip is no longer reachable (for example, a display was unplugged), the window is centered on the main screen.
- If the frame is larger than the screen it is on, the window shrinks to fit, but never below the minimum size.

Zoom and full-screen state are restored after the frame, so leaving them returns the window to its saved size.
