# Linux backend at full macOS parity

- Status: Ready for implementation
- Date: 2026-10-15
- Source: Maintainer brainstorm session on 2026-10-15. The maintainer asked to port ChoscorUI "100% to Linux first (Windows will be later)" and how to test it from a macOS machine. Evidence comes from `docs/architecture.md` (backend selection, layering rules, dependency rule), `docs/platform-linux.md`, `docs/platform-macos.md`, `docs/ffi.md`, `docs/performance.md`, `CONTRIBUTING.md`, `CLAUDE.md`, `bylaw.toml`, `.github/workflows/ci.yml`, `crates/choscorui/src/backend/{mod,none,macos}.rs`, `crates/choscorui-core/src/capabilities.rs`, `crates/choscorui-controls/src/{lib,editing,navigation}.rs`, `crates/choscorui-controls/src/dock/host.rs`, `crates/choscorui-linux/src/*`, `crates/choscorui/tests/compatibility.rs`, and `crates/choscorui-macos/src/imaging.rs`.

## Outcome and current state

**Beneficiary.** Application authors who ship ChoscorUI apps on Linux desktops, and the users of those apps. Today an app built for Linux compiles, but `App::start` returns `AppError::Unsupported`. After this work, the same app source runs on Linux with every capability that macOS reports as supported. It uses Wayland or X11, GPU painting with a CPU fallback, IME, clipboard, drag and drop, menus, dialogs, desktop services, and accessibility.

**Current state (2026-10-15).**

- `crates/choscorui/src/backend/mod.rs` picks `macos.rs` on macOS and `none.rs` everywhere else. `none.rs` reports `BackendCapabilities::none("headless")`, so `App::start` fails before opening a window. `choscorui::platform` is empty off macOS.
- `choscorui-linux` (about 1.2k lines) has platform-neutral state machines only. They cover `select_backend`, `WaylandConnection`/`WaylandSurface` configure handshake, `FrameScheduler`, `X11Window`, `LinuxInput` seat gating, and `Clipboard` ownership. It has no native calls and no dependencies, and it is `publish = false`. `tests/protocol_contracts.rs` pins the rules on every host.
- `choscorui-macos` is about 19k lines with zero crate dependencies. It uses hand-written FFI in `src/ffi/` (`messages!`, `c_functions!`, `sdk_const!`, `ffi_struct!`, `define_classes!`) and SDK header checks compiled by `xcrun clang++` in its tests. It has a Metal painter with a Core Graphics fallback, Core Text shaping, AppKit menus, dialogs, pasteboard, DnD, `NSTextInputClient` IME, `NSAccessibilityElement` bridge, `UserNotifications`, status items, ImageIO codecs, and `NSBeep`.
- Dependency rule (`docs/architecture.md`): runtime code uses std, hand-written FFI to system SDKs, and `unicode-segmentation` only. It allows no build scripts, binding generators, or proc-macro crates.
- `bylaw.toml` lets platform crates depend only on core. The facade may depend on core, controls, compose, and macos.
- `BackendCapabilities` fields: `multiple_windows`, `native_menus`, `context_menus`, `native_dialogs`, `clipboard`, `drag_and_drop`, `input_methods`, `accessibility`, `status_items`, `notifications`, `open_urls`, `system_appearance`, `text_shaping`, `image_codecs`, and `gpu_painting`. `Condition` has one variant, `AppBundle`.
- Controls hard-code macOS key conventions. `ShortcutModifiers { shift, control, option, command }` is used about 25 times in `choscorui-controls`/`choscorui-compose`, for example `editing.rs:28`, `editing.rs:82`, `navigation.rs:787`, and `dock/host.rs:292`. The gallery declares `command: true` shortcuts.
- Golden and compatibility tests use `EstimatedText`, so they are platform-neutral.
- CI (`.github/workflows/ci.yml`) runs only on `macos-15` and `macos-15-intel`. It excludes `choscorui-linux` and `choscorui-windows` from clippy, MSRV, and tests. It runs `cargo check --target x86_64-unknown-linux-gnu` for the facade and gallery only.
- `choscorui-bench` and `bench/budgets.txt` are macOS-only, measured on the M4 reference machine.

**Desired state.** On Linux, `choscorui::capabilities()` reports `backend: "wayland"` or `backend: "x11"`. Every capability that macOS reports `Supported` is `Supported` or honestly `Conditional`. The gallery, the autopilot, the smoke tests, and the bench run on Linux in CI, and in Docker on the maintainer's Mac. `choscorui-windows` is unchanged.

## Decisions and requirements

### Decision ledger

| ID | Decision | Status | Source |
| --- | --- | --- | --- |
| S1 | The facade gains `backend/linux.rs` beside `macos.rs`/`none.rs`. `bylaw.toml` lets the facade depend on `choscorui-linux`. `choscorui::platform` exposes `choscorui-linux` plus `NativeAccess` on Linux | settled | Architecture backend seam; required for any port |
| S2 | `unsafe` only in `choscorui-linux/src/ffi/`, one raw call per block with `SAFETY`. Callback panics are caught at each boundary and recorded in `docs/ffi.md`. Branch on `capabilities()` | settled | `CLAUDE.md` |
| S3 | Golden and compatibility tests stay platform-neutral (`EstimatedText`) | settled | `compatibility.rs` |
| S4 | `choscorui-windows` is out of scope and unchanged | settled | Request |
| S5 | `choscorui-linux` becomes a published crate (drop `publish = false`, add docs.rs metadata), because the facade depends on it for `target_os = "linux"` and CI runs `cargo publish --workspace --dry-run` | settled | Necessity |
| Q1 | Keep the dependency rule. Hand-write FFI to system C libraries (Wayland, XCB/Xlib, xkbcommon, EGL/GLES, Cairo, FreeType, HarfBuzz, fontconfig, libdbus-1, image libs) | settled | User |
| Q2 | Load every system library with `dlopen`/`dlsym` through `ffi` macros, and link only libc. A missing library lowers the matching capability | settled | User |
| Q3 | Native Wayland (default) and native X11 | settled | User |
| Q4 | Toolkit-drawn client-side decorations on Wayland when `xdg-decoration` does not grant server-side ones. Use server-side decorations when granted. X11 uses the window manager | settled | User |
| Q5 | GPU painter: OpenGL ES 3.0 through EGL on both Wayland and X11. GLSL ES shader source is compiled by the driver at runtime | settled | User |
| Q5b | CPU painter: Cairo with FreeType font faces, drawing into `wl_shm` / MIT-SHM images. It is the fallback, the offscreen export path, and a parity peer. `choscorui-render` stays the reference | settled | User |
| Q6 | Text: FreeType + HarfBuzz + fontconfig | settled | Follows from Q1 and Q5b |
| Q7 | D-Bus through libdbus-1 FFI. Its watch and timeout fds join the backend's main poll loop | settled | User (named in the Q1 answer) |
| Q8a | In-window menu bar drawn by the toolkit. Submenus and context menus are `xdg_popup` / override-redirect popups. `native_menus` and `context_menus` report `Supported` | settled | User |
| Q8b | File dialogs use the xdg-desktop-portal `FileChooser`. Without a portal, use a toolkit-drawn file dialog on core's `FileSystemModel`. `native_dialogs` reports `Supported` | settled | User |
| Q8c | Desktop services mapping (see R8), including the new `Condition::StatusNotifierHost` and a real `text_scale` | settled | User |
| Q9 | Image codecs: dlopen libpng (decode and encode), libturbojpeg, giflib, libtiff, libwebp, and libheif. `image_codecs` is `Supported` when PNG and JPEG load | settled | User |
| Q10 | Accessibility: AT-SPI2 implemented directly over libdbus (no ATK, no GLib) | settled | Follows from Q1 and Q7 |
| Q11 | Floor: Ubuntu 22.04 / Debian 12 / Fedora 39 / RHEL 9 era (glibc 2.35), on x86_64 and aarch64 | settled | User |
| Q12 | CI and local verification: sway headless + Xvfb, Mesa llvmpipe, `dbus-run-session`, `at-spi2-registryd`, and fake D-Bus services in tests. One Dockerfile serves CI and the Mac | settled | User |
| Q13 | Port `choscorui-bench`. Add `bench/budgets-linux.txt`. CI enforces non-timing budgets. Timing budgets come from a named Linux reference machine | settled | User |
| Q14 | Delivery in ordered milestones M1–M6. Each merges with green CI and honest capabilities | settled | User |
| Q15 | X11 IME: XIM through libX11 + libX11-xcb (on-the-spot preedit, `XFilterEvent`). XCB does all other X11 work | settled | User |
| Q16 | `BackendCapabilities` gains `keyboard: KeyConvention { Mac, Pc }`. Controls, editing tables, shortcut matching, and shortcut labels follow it. Linux reports `Pc` | settled | User |
| Q17 | Linux-only extras are non-goals: PRIMARY selection and middle-click paste, Flatpak packaging, a `.desktop` helper, and the global shortcuts portal | settled | User |
| D1 | Identity of the Linux reference machine | deferred | The user chose "a named Linux reference machine that you provide". The maintainer records it in `docs/performance.md` before any timing budget is committed. This does not change scope |
| D2 | Font rendering defaults: `system-ui` resolves with fontconfig. Use the desktop `font-name` (portal `org.gnome.desktop.interface`, or XSETTINGS `Gtk/FontName`) when present, else `sans-serif`. Use grayscale anti-aliasing and the hinting reported by fontconfig | deferred | Minor implementation default; parity tests bound the result |
| D3 | Path rasterization, glyph atlas, and batching inside the GLES painter | deferred | Minor and internal; parity and bench bound the result |

### R1. Crate shape and FFI (Q1, Q2, S2, S5)

- All native code lives in `choscorui-linux`. `src/ffi/` holds every `unsafe` block. The crate root denies `unsafe_code`, and only `mod ffi` allows it. Copy the `[lints]` table of `choscorui-macos`: deny `unsafe_op_in_unsafe_fn`, `undocumented_unsafe_blocks`, `multiple_unsafe_ops_per_block`, and `missing_safety_doc`.
- A `libraries!` macro declares each shared object with its sonames, for example `libwayland-client.so.0`. It also declares the exact C signature of each symbol. Loading returns a typed table or a domain error that names the library and the missing symbol. It never panics. Each table loads at most once per process.
- Also provide `ffi_struct!`, `sdk_const!`-style constant declarations, and a callback macro for C callbacks (Wayland listeners, D-Bus handlers, Cairo, and XIM). The callback macro wraps each body in `catch_unwind`, writes the boundary name to stderr, and returns a declared fallback.
- Libraries: libwayland-client, libwayland-egl, libwayland-cursor, libxkbcommon, libxkbcommon-x11, libxcb plus `xcb-xkb`, `xcb-xinput`, `xcb-shm`, `xcb-randr`, `xcb-render`, `xcb-present` (optional), and `xcb-cursor` (optional). Also libX11 and libX11-xcb (XIM only), libEGL, libGLESv2, libcairo, libfreetype, libharfbuzz, libfontconfig, libdbus-1, libpng16, libturbojpeg, libgif, libtiff, libwebp, libheif, and libcanberra (optional).
- Wayland protocol interfaces beyond the core (`xdg-shell`, `xdg-decoration`, `fractional-scale-v1`, `viewporter`, `cursor-shape-v1`, `text-input-v3`, `xdg-activation-v1`, `xdg-dialog-v1`, `xdg-system-bell-v1`, `xdg-output`, and `pointer-gestures`) are hand-written `wl_interface` tables. No `wayland-scanner` and no build script are used. Each table carries the protocol XML file name and version in a comment.
- **Binding check** (the analogue of `sdk_headers_compile`). A `cargo test -p choscorui-linux` test generates one C file from all `libraries!`, `ffi_struct!`, and constant declarations. It compiles the file with `cc -Werror` against the system headers found by `pkg-config`. The file assigns each symbol to a pointer of the declared Rust signature, checks each struct with `_Static_assert` on size and offsets, and checks each constant value. Another test compares each hand-written `wl_interface` table with the protocol XML under `/usr/share/wayland-protocols` (and `wayland.xml`). A missing `-dev` package fails with the package names that `CONTRIBUTING.md` lists. The test runs only on Linux hosts. Fix the binding; never weaken, skip, or suppress the check.
- A `boundary_is_closed` test reads the sources. It fails for `extern "C"`, `dlsym`, `transmute`, or raw pointer casts of C functions outside `src/ffi/`.
- Add a "Linux" part to `docs/ffi.md`. It covers ownership (who destroys each Wayland proxy, XCB reply, Cairo surface, FreeType face, HarfBuzz buffer, and D-Bus message), threads, and the test for each boundary.
- Add `choscorui-linux` docs.rs metadata with `default-target = "x86_64-unknown-linux-gnu"` and the targets x86_64 and aarch64. Remove `publish = false`.

### R2. Event loop, threads, and backend selection (Q3, Q7)

- One UI thread owns everything. The loop `poll`s the Wayland display fd or the XCB fd, the libdbus watch fds, a timerfd for the next timer or frame, and an eventfd for `Poster`. `Platform::wait(timeout)` returns when any of them fires or the timeout passes.
- `Poster::post` (from worker threads) queues a closure and writes the eventfd. Closures run on the UI thread in FIFO order. After the application drops, `post` returns the stopped error and drops the closure, as on Windows and macOS.
- Use the existing `select_backend` as is. Add `CHOSCORUI_BACKEND=wayland|x11` as the explicit request. `BackendSelectionError` maps to `AppError::Unsupported` with the reason. The existing state machines (`WaylandSurface`, `FrameScheduler`, `X11Window`, `LinuxInput`, and `Clipboard`) drive the native code. Do not duplicate their rules.
- `capabilities()` reports `backend: "wayland"` or `"x11"`, and each field from the probes in R3–R10. With no display server, `capabilities()` equals `BackendCapabilities::none("headless")` with `keyboard: Pc`, and `App::start` returns `AppError::Unsupported` as today.

### R3. Windows, decorations, scale, cursors, and geometry (Q3, Q4, Q11)

- Wayland requires `wl_compositor` (v4 or later), `wl_shm`, `xdg_wm_base`, and `wl_seat`. A missing required global makes `multiple_windows` `Unsupported` with the global's name. Other protocols are probed, and the code falls back when one is absent.
- Decorations follow Q4. Without server-side decorations, the toolkit draws the title bar, the buttons (close, maximize, minimize), the resize edges, and the shadow from theme tokens and recipes. It calls `xdg_toplevel.move`, `.resize`, and `.show_window_menu`, and it double-clicks to maximize. Add theme tokens only when they have a second use (`CLAUDE.md`). The title bar is part of the semantic tree, and its buttons are focusable and keyboard-activatable. Right-to-left layouts mirror the button order. Strings go through the localization catalogs.
- Scale: use `wp_fractional_scale_v1` + `wp_viewporter` when present, else the integer `wl_surface.preferred_buffer_scale` / output scale. On X11, use per-monitor RandR scale, then `Xft.dpi`. Changes produce `ScaleChanged` and then `Resized`, as on macOS.
- Cursors: use `wp_cursor_shape_v1` when present, else libwayland-cursor with `XCURSOR_THEME`/`XCURSOR_SIZE`. On X11, use `xcb-cursor`, else core cursor glyphs. Map every `CursorShape`.
- Geometry: on X11, `WindowGeometry` round-trips frame, maximized, and full screen. Wayland cannot set or read a window origin, so `set_geometry` applies the size, maximized, and full-screen states and ignores the origin, and `geometry().frame.origin` is zero. `screens()` returns output rectangles from `xdg-output` / RandR. Document this platform limit in `docs/platform-linux.md`.
- Activation: request focus for new windows with `xdg-activation-v1` when a token exists (`XDG_ACTIVATION_TOKEN`), and use `_NET_ACTIVE_WINDOW` on X11.
- Closing: `xdg_toplevel.close` and `WM_DELETE_WINDOW` become the same close-request event that macOS produces. Window titles change after creation.

### R4. Input and key conventions (Q16)

- Keyboards use xkbcommon (keymap from `wl_keyboard.keymap`, or `xkbcommon-x11` with XKB events), with compose tables from the locale. Key repeat follows `wl_keyboard.repeat_info` / the XKB auto-repeat rate.
- Pointer events include frames, axis value120 / discrete, and smooth scroll in points. Touchpad gestures use `pointer-gestures` when present. X11 uses XInput 2.2 for smooth scroll and touch. Scroll units match the macOS contract that `docs/platform-macos.md` describes.
- Add `KeyConvention { Mac, Pc }` to `choscorui-core`, exported as `BackendCapabilities::keyboard`. macOS reports `Mac`, Linux reports `Pc`, and `none` reports `Pc`. Keep all policy in core and controls:
  - Shortcut matching. A declared `command` modifier matches Command on `Mac` and Ctrl on `Pc`. On `Pc` the Super key never matches `command`.
  - Text editing on `Pc`:
    - Ctrl+Left/Right moves by word.
    - Home/End go to the line ends, and Ctrl+Home/End go to the document ends.
    - Ctrl+Backspace/Delete deletes a word.
    - Ctrl+Z undoes. Ctrl+Y and Ctrl+Shift+Z redo.
    - Shift extends each of these.
  - `dock/host.rs` Cmd+W and every other hard-coded `command` chord follow the same mapping.
  - Shortcut labels in menus and tooltips render `Ctrl+S` style on `Pc` and `⌘S` on `Mac`.
- `HostEnv` and the view host take the convention from the backend. Tests choose it explicitly. Existing goldens keep `Mac`. Add `Pc` goldens for shortcut labels and editing, and call out the blessed diff in the commit and PR.
- Update `docs/controls.md` (key tables per convention) in the same change.

### R5. Painting and text (Q5, Q5b, Q6)

- **GLES painter.** EGL on `wl_egl_window` (Wayland) or the X11 window (`EGL_PLATFORM_X11_KHR` / `EGL_EXT_platform_xcb` when present). It paints every `DisplayCommand` that the Metal painter paints. It uses `EGL_KHR_swap_buffers_with_damage` / `EGL_EXT_buffer_age` when present, else full redraw. Wayland frames are paced with `FrameScheduler` and `wl_surface.frame`. Compile shaders at window-system init, and report a compile or link failure as a painter failure, not a panic.
- **Cairo painter.** Paint into `wl_shm` buffers (double-buffered, released on `wl_buffer.release`) or MIT-SHM images (falling back to `xcb_put_image`). Text uses `cairo_ft_font_face` from the same FreeType faces and HarfBuzz glyph runs as the GLES painter. Both painters place glyphs at the same positions.
- **Fallback contract** (mirrors `docs/platform-macos.md`). The order is: GLES by default; Cairo when EGL/GLES does not load, no config matches, shader setup fails, or `CHOSCORUI_PAINTER=cairo` is set. A GPU failure mid-life (`EGL_CONTEXT_LOST`, swap failure) switches that window to Cairo for the rest of its life and repaints all of it. `gpu_painting` then reports `Unsupported`. `Window::painter()` returns `Painter::Gles` or `Painter::Cairo`, and `capture` / offscreen export use Cairo.
- **Text.** `TextShaper` and `TextMeasure` use HarfBuzz shaping on FreeType faces. fontconfig provides family matching, fallback per grapheme cluster (`unicode-segmentation` boundaries), and `system-ui` (D2). Behavior follows `docs/text.md`: shaping, layout cache, wrapped text, and rich text. `text_shaping` is `Supported` when FreeType, HarfBuzz, and fontconfig load.

### R6. Clipboard, drag and drop (Q3)

- Wayland uses `wl_data_device_manager` (selection and DnD). X11 uses the CLIPBOARD selection with INCR for large data and XDND v5. The existing `Clipboard` state machine owns offer and ownership state.
- Map `DragItem` to MIME types:
  - text → `text/plain;charset=utf-8` (plus `UTF8_STRING` on X11)
  - files → `text/uri-list`
  - PNG → `image/png`
  - URL → `text/x-moz-url` + `text/uri-list`
  - `DragFormat::Custom` → an app-private MIME type that is never offered to other processes, as on macOS.
- Images from other apps in TIFF/JPEG/etc. convert to PNG through R9, which matches macOS `read`.
- `begin_drag` is valid only inside the pointer-down/move handler, as on macOS. Wayland needs that event's serial, so the contract is natural. Implement `Clipboard::temporary` in-process.

### R7. IME, menus, popups, dialogs (Q8a, Q8b, Q15)

- **IME.** Wayland uses `zwp_text_input_v3` (enable on focus, `set_surrounding_text`, `set_cursor_rectangle`, `done` serials). X11 uses XIM on-the-spot (preedit start/draw/done callbacks, `XFilterEvent` before key handling, spot location from `set_text_input_state`). Both produce the same `ImeEvent` sequence (`SetMarkedText`, `CommitText`, `Unmark`) with UTF-16 ranges. `input_methods` is `Supported` when text-input-v3 is advertised or XIM opens. `captures_keys` keeps the macOS meaning.
- **Menu bar.** The `MenuBarModel` renders as an in-window bar under the title bar. It supports Alt mnemonics, F10 focus, arrow navigation, accelerators drawn with the R4 labels, enabled and checked state, and AT-SPI menu roles. Shortcut activation goes through the same `take_activations` / `perform` path as clicks.
- **Popups.** Submenus and `ContextMenu` use `xdg_popup` + `xdg_positioner` with grabs, or override-redirect windows with pointer and keyboard grabs on X11. Popups flip at screen edges. `show_context_menu` keeps the macOS contract. It runs a nested loop until the popup closes, returns the chosen command, and also queues it.
- **File dialogs.** Use portal `org.freedesktop.portal.FileChooser` (`OpenFile` with `multiple`/`directory`, and `SaveFile`) with the parent window handle (`wayland:<xdg-foreign handle>` / `x11:<xid>`). The call waits on the `Response` signal in a nested loop, which keeps the synchronous `show_file_dialog` contract. Without a portal, open the toolkit-drawn file dialog. It is a new composite control on `FileSystemModel` with open file/files, select directory, save with overwrite confirm, extension filters, hidden files, the title and prompt, the initial directory, and the suggested name. It follows `docs/controls.md` (catalog entry, focus order, keyboard activation, semantics, localization, right-to-left, goldens).
- **Alerts.** `show_alert` opens a toolkit-drawn modal window from the existing `AlertDialog`. It is transient for its parent (`xdg_dialog_v1` modal when present, `WM_TRANSIENT_FOR` + `_NET_WM_STATE_MODAL` on X11). Other windows of the app ignore input until it closes. It returns `AlertResponse::Button(index)`. `AlertSpec` validation is unchanged.

### R8. Desktop services (Q8c)

| Service | Linux mapping | Capability |
| --- | --- | --- |
| Notifications | Portal `Notification` when running in Flatpak (`/.flatpak-info` exists), else `org.freedesktop.Notifications`. Permission is always granted. Replace by id. A click (`ActionInvoked`) sends `AppEvent::NotificationActivated` | `Supported` when a notification server owns the name, else `Unsupported` |
| Status items | `StatusNotifierItem` + `com.canonical.dbusmenu` for `set_menu`. Title, tooltip, and visibility map to SNI properties | `Supported` with an `org.kde.StatusNotifierWatcher`, else `Conditional(Condition::StatusNotifierHost)` |
| `open_url` / `url_handler` | Portal `OpenURI`, else spawn `xdg-open`. `url_handler` returns the default handler's `.desktop` path from the `x-scheme-handler/<scheme>` (or MIME) lookup in `mimeapps.list` and the XDG data dirs. Keep the error contract (`InvalidUrl`, `NoHandler`) | `open_urls`: `Supported` |
| App events | `Activated`/`Deactivated` when any app window gains or loses keyboard focus. `OpenFiles`/`OpenUrls` from argv at launch and from `org.freedesktop.Application.Open`. `Reopen` from `org.freedesktop.Application.Activate` | — |
| Alert sound | `xdg_system_bell_v1` on Wayland, `xcb_xkb_bell` on X11, else libcanberra event `bell`, else silent | — |
| Appearance | Portal `org.freedesktop.portal.Settings`: `org.freedesktop.appearance` `color-scheme` and `contrast`, `org.gnome.desktop.interface` `enable-animations` (reduce motion) and `text-scaling-factor` (`text_scale`). Re-read on `SettingChanged` and on window focus | `system_appearance`: `Supported` when the portal answers, else `Unsupported` with light defaults |
| Settings file | `user_settings_path(app_id)` returns `$XDG_CONFIG_HOME/<app_id>/settings.txt`, else `$HOME/.config/<app_id>/settings.txt`. The `None` rules are unchanged | — |

- Add `Condition::StatusNotifierHost` (with its catalog message `choscorui.condition.status_notifier_host`) to core. `Condition` is `#[non_exhaustive]`.
- Wayland `app_id` and X11 `WM_CLASS` come from `App::id`. `docs/platform-linux.md` explains that D-Bus activation and some notification servers need a `.desktop` file and a D-Bus service file named after the app id. This is the analogue of the macOS `Info.plist` notes. No helper is provided (Q17).

### R9. Image codecs (Q9)

- Decoding tries the formats by magic bytes: PNG (libpng16), JPEG (libturbojpeg), GIF first frame (giflib), TIFF (libtiff), WebP (libwebp), and HEIC/HEIF (libheif). Output is straight-alpha RGBA8 sRGB, which matches the macOS `Image` contract, and applies EXIF orientation for JPEG/HEIC.
- `encode_png` uses libpng.
- A format whose library is missing returns the existing unsupported-format `ImageError`. `image_codecs` is `Supported` when PNG and JPEG load, else `Unsupported`.

### R10. Accessibility (Q10)

- Register with the AT-SPI bus (`org.a11y.Bus.GetAddress` on the session bus, then a private connection). Export `org.a11y.atspi.Accessible`, `Component`, `Action`, `Text`, `EditableText`, `Value`, `Selection`, and `Table` where the role needs them. Map every `SemanticRole` to an AT-SPI role, and keep the mapping table in `docs/platform-linux.md`.
- `NativeWindow::publish(snapshot, changes)` keeps one object path per live node and removes the paths of removed nodes. It emits `ChildrenChanged`, `StateChanged` (focused, checked, expanded, selected, and so on), `PropertyChange`, and `TextChanged`/`TextCaretMoved`.
- `announce` emits an `Announcement` event (AT-SPI 2.46 and later), else an `object:announcement` fallback on the focused object.
- Assistive-technology actions queue only when the node declares them, as on macOS.
- `accessibility` is `Supported` when the a11y bus is reachable, else `Unsupported`, and the app runs without it.

### R11. Facade, bench, gallery, and docs (S1, Q13)

- `backend/linux.rs` implements `Platform`, `NativeWindow`, and `NativeMenu` with the same crate-private signatures as `macos.rs`. `backend/mod.rs` gains a `target_os = "linux"` arm, and `none` stays for other targets. `bylaw.toml` adds `linux` to the facade's allowed layers. Update `docs/architecture.md` (layering list, graph, backend selection) in the same change.
- The facade tests that are macOS-gated today (`app_smoke.rs`, `metal_parity_smoke.rs` → a Linux `gles_parity_smoke.rs`, `render_parity.rs`, and `errors_and_capabilities.rs`) gain Linux arms behind `CHOSCORUI_RUN_LINUX_SMOKE=1`.
- The gallery autopilot (`gallery/src/autopilot.rs`) drives the Linux backend. `CHOSCORUI_GALLERY_AUTOCLOSE_MS` and `CHOSCORUI_GALLERY_AUTOPILOT` work the same.
- `choscorui-bench` runs its scenarios on Linux and reads `bench/budgets-linux.txt`. CI enforces the non-timing keys: `windows.leaked_native_windows`, `*.missed_present`, `idle.paints`, `idle.cpu_percent`, `idle.wakeups_per_sec`, and the `size.*` release binary sizes. Timing keys are committed only after measurement on the reference machine (D1), and `docs/performance.md` names that machine and its baselines.
- Rewrite `docs/platform-linux.md` to describe the backend in the format of `docs/platform-macos.md`: requirements, lifecycle, windows, menus, dialogs, clipboard, IME, desktop integration, appearance, settings, capabilities table, and accessibility. Update `docs/guide.md`, `docs/text.md`, `docs/controls.md`, `docs/theming.md` (decoration tokens), `docs/licenses.md` (system libraries are dynamically loaded, not shipped), `README.md` (supported platforms), and `CONTRIBUTING.md`. Follow the `CONTRIBUTING.md` documentation rules and run `scripts/check-docs.sh`.

### R12. Verification setup (Q12)

- `scripts/linux/Dockerfile` is based on Ubuntu 22.04, multi-arch. It installs the Rust toolchains, the `-dev` packages for the binding check, sway, Xvfb, Mesa (llvmpipe EGL/GLES), dbus, at-spi2-core, xdg-desktop-portal (the test fakes stand in for its backends), and the image libraries.
- `scripts/linux/run-in-container.sh <job>` runs `check` or `native` inside that image. It mounts the repo, uses a target dir in a named volume, and works on the maintainer's arm64 Mac with Docker Desktop.
- `scripts/linux/headless-session.sh` starts `dbus-run-session`, `at-spi2-registryd`, sway with `WLR_BACKENDS=headless WLR_RENDERER=pixman` (or gles2 on llvmpipe) and a virtual output, and `Xvfb :99` with RandR. It exports `WAYLAND_DISPLAY`/`DISPLAY` and runs the given command.
- Test-only D-Bus fakes (in `choscorui-linux/tests/support/`, std only) own `org.freedesktop.portal.Desktop` (FileChooser, OpenURI, Settings, Notification), `org.freedesktop.Notifications`, and `org.kde.StatusNotifierWatcher`. They record calls and send scripted responses. A test input-method client on `zwp_input_method_v2` (sway) drives the Wayland IME smoke tests. X11 IME uses a unit-level test of the XIM callbacks. The X11 smoke tests run with `XMODIFIERS=@im=none` and check that the IM opens and closes cleanly. No test XIM server is needed.
- CI (`.github/workflows/ci.yml`):
  - New `linux-check` job on `ubuntu-22.04` and `ubuntu-24.04-arm`. It runs docs, fmt, clippy `--workspace --exclude choscorui-windows --exclude choscorui-macos -D warnings` (including `choscorui-linux`), MSRV 1.88 check, and `cargo test --workspace --exclude choscorui-windows --exclude choscorui-macos` (including the binding check).
  - New `linux-native` job (needs `linux-check`) with the smoke tests one at a time under `headless-session.sh` for both `CHOSCORUI_BACKEND=wayland` and `x11`, plus `CHOSCORUI_PAINTER=cairo` runs, gallery first frame, gallery autopilot, and the bench with `--check bench/budgets-linux.txt` (non-timing keys).
  - The macOS `check` job adds `cargo check -p choscorui-linux --all-targets --target x86_64-unknown-linux-gnu`. dlopen needs no Linux libraries to type-check.
- `CONTRIBUTING.md` lists the Linux commands for "Before submitting a maintainer change", the `-dev` package list, and the Docker route. Update the `CLAUDE.md` Checks section to name the Linux jobs.

### Milestones (Q14)

Each milestone merges on its own with both macOS and Linux CI green. Before a feature lands, `capabilities()` reports it `Unsupported`, and its service returns the existing unavailable errors. "100%" means M6 is done.

| Milestone | Scope | Capabilities that become supported |
| --- | --- | --- |
| M1 Foundation | R1 FFI and binding check, R2 loop and posting, R3 windows on Wayland (SSD only where granted, plain CSD frame otherwise) and X11, R4 keyboard/pointer and `KeyConvention`, R5 Cairo painter and text, R11 facade wiring and `bylaw`, R12 Docker, scripts, and CI jobs, gallery first frame | `multiple_windows`, `text_shaping` |
| M2 GPU and chrome | R5 GLES painter, fallback, `capture`, GLES parity smoke; R3 full CSD, fractional scale, cursors, geometry, activation | `gpu_painting` |
| M3 Interaction | R6 clipboard and DnD, R7 IME (text-input-v3, XIM), menu bar, popups, context menus | `clipboard`, `drag_and_drop`, `input_methods`, `native_menus`, `context_menus` |
| M4 Desktop | R7 file dialogs (portal and drawn), alerts; R8 services; R9 codecs | `native_dialogs`, `notifications`, `status_items`, `open_urls`, `system_appearance`, `image_codecs` |
| M5 Accessibility | R10 AT-SPI bridge and announcements | `accessibility` |
| M6 Performance and docs | R11 bench port and budgets, final docs pass, gallery autopilot covering every page on both backends | — |

### Non-goals

- `choscorui-windows` and any Windows work (S4).
- PRIMARY selection and middle-click paste, Flatpak or Snap packaging, a `.desktop`/D-Bus service file helper, and the global shortcuts portal (Q17).
- Vulkan, and any GPU API other than GLES 3.0 (Q5). A KDE global menu through dbusmenu (Q8a). libdecor or GTK-themed decorations (Q4). IBus/Fcitx D-Bus IME (Q15).
- Linux distributions older than the Q11 floor, 32-bit targets, and non-glibc (musl) targets.
- Changing `choscorui-render` into a production painter.

### Edge cases and failure behavior

- A library is missing or a symbol is absent. The matching capability is `Unsupported` (or the Cairo fallback takes over for EGL/GLES). The error names the library and symbol. Nothing panics. If `libwayland-client` and `libxcb` are both missing, `App::start` returns `AppError::Unsupported`.
- `WAYLAND_DISPLAY` is set but the connection fails. Without an explicit request, fall back to X11 if `DISPLAY` works. With `CHOSCORUI_BACKEND=wayland`, return `RequestedUnavailable` (the existing rule).
- The compositor or X server goes away. The loop sees `POLLHUP`/error, sends close events to all windows, and stops `App::run` with a domain error. It does not abort.
- The D-Bus session bus is absent (for example in a bare container). The D-Bus-backed capabilities are `Unsupported`, the file dialog uses the drawn fallback, and `open_url` uses `xdg-open`.
- The portal request is cancelled (`Response` code 1). Return `FileDialogResponse::Cancelled`, which is not an error.
- A configure arrives during a frame. The existing `WaylandSurface` rules apply. Never commit a buffer for a superseded serial.
- Scale changes mid-drag or mid-IME. Recompute the caret rectangle and resend it.
- A GL context is lost. The window switches to Cairo for life (R5), and the other windows switch at their next draw.
- XIM server restarts. Reopen the IM on `XRegisterIMInstantiateCallback`, and drop the pending preedit with `Unmark`.
- A panic in any C callback is caught, logged with the boundary name, and the callback returns its fallback (R1).

## Acceptance criteria and public test seams

| Criterion | Observable seam |
| --- | --- |
| Every FFI binding matches the system headers and protocol XML | `cargo test -p choscorui-linux` binding-check tests pass on `ubuntu-22.04` and `ubuntu-24.04-arm`. Changing one declared signature, struct field, or constant makes them fail and name the symbol |
| `unsafe` stays inside `src/ffi/` | `boundary_is_closed` test passes. `cargo clippy -p choscorui-linux -- -D warnings` passes with the crate's unsafe lints |
| The facade opens windows on both display servers | `CHOSCORUI_RUN_LINUX_SMOKE=1 cargo test -p choscorui --test app_smoke` passes under `headless-session.sh` with `CHOSCORUI_BACKEND=wayland` and with `x11` |
| Backend selection follows the documented rules | `tests/protocol_contracts.rs` (unchanged rules) plus a smoke case: `CHOSCORUI_BACKEND=x11` with no `DISPLAY` makes `App::start` return `AppError::Unsupported` |
| Window lifecycle, posting, timers, scale, resize, and close match the macOS contract | `choscorui-linux/tests/window_smoke.rs` and `lifetime_smoke.rs` (window churn, no leaked native windows) pass on both backends |
| GLES paints like the reference | `gles_parity_smoke.rs` renders each `render_parity.rs` fixture at scale 1 and 2, captures it, and checks `compare(.., 24)` against `Raster::render` with under 0.5 % differing pixels. Text fixtures compare GLES against `CHOSCORUI_PAINTER=cairo` captures |
| Fallback works and is reported | With `CHOSCORUI_PAINTER=cairo`, `Window::painter()` is `Painter::Cairo`, `capabilities().gpu_painting` is `Unsupported`, and the window and app smoke tests pass. With GLES, `painter()` is `Painter::Gles` and `gpu_painting` is `Supported` on llvmpipe |
| CSD is usable and accessible | Smoke test on sway with SSD refused (`CHOSCORUI_DECORATIONS=client`). Dragging the title bar issues `xdg_toplevel.move`, a button click closes or maximizes, and the semantic snapshot holds the title bar buttons. Goldens pin the decoration display list in LTR and RTL |
| PC key conventions apply on Linux | `capabilities().keyboard == KeyConvention::Pc` on Linux and `Mac` on macOS. Controls unit tests with `Pc` pass: Ctrl+S triggers a `command` shortcut, Ctrl+Left moves by word, Home/End go to line ends, and the label renders `Ctrl+S`. The existing `Mac` tests and goldens are unchanged |
| Clipboard and DnD interoperate | `clipboard_smoke.rs` round-trips text, files, PNG, and URL with a second test process on both backends. Custom formats are not visible to the second process. A DnD smoke test drags between two windows and reports `DragEnded` |
| IME composition works | Wayland: a test `zwp_input_method_v2` client sends preedit and commit. The window gets `SetMarkedText` then `CommitText` with correct UTF-16 ranges, and the caret rectangle reaches the input method. X11: a unit test feeds the XIM preedit callbacks and checks the same `ImeEvent` sequence |
| Menus and context menus work | `menu_smoke.rs`: `perform` and shortcut activation of an in-window menu command appear in `take_activations`. `show_context_menu` returns the chosen command. Popups open on both backends. AT-SPI shows menu roles |
| File dialogs work with and without a portal | With the fake portal, `show_file_dialog` sends the right `FileChooser` options and returns `Selected(paths)` or `Cancelled`. Without it, the drawn dialog opens, and autopilot selects a file and returns it. The drawn dialog has goldens, a `docs/controls.md` entry, and localized strings |
| Desktop services follow R8 | `services_and_storage.rs` Linux arm with the D-Bus fakes. Notifications post and replace by id, and a fake `ActionInvoked` yields `NotificationActivated`. No watcher gives `Conditional(StatusNotifierHost)`, and with a watcher the item registers. `open_url` calls `OpenURI`. Fake `color-scheme`/`contrast`/`text-scaling-factor` changes change `system_prefers_dark`, `AccessibilityPreferences`, and `text_scale`. `user_settings_path` honors `XDG_CONFIG_HOME` |
| Image codecs decode every macOS format | Codec tests decode PNG, JPEG (with EXIF rotation), GIF, TIFF, WebP, and HEIC fixtures to the expected RGBA within tolerance, and `encode_png` round-trips. With a library hidden (`CHOSCORUI_TEST_HIDE_LIB=libtiff`), TIFF returns the unsupported-format error and other formats still work |
| Accessibility exposes the semantic tree | Under `at-spi2-registryd`, a test AT-SPI client (std + the crate's D-Bus binding) walks the gallery window and finds the expected roles and names for every `SemanticRole`. It invokes a declared action and sees it in the action queue, and it receives an announcement. This does not verify Orca (see risks) |
| Capabilities are honest at every milestone | `errors_and_capabilities.rs` Linux arm asserts the table in `docs/platform-linux.md` for the current milestone. For M6, every field macOS reports `Supported` is `Supported`, or `Conditional` per R8 |
| Gallery runs end to end | `CHOSCORUI_GALLERY_AUTOCLOSE_MS=1000 cargo run -p choscorui-gallery` and `CHOSCORUI_GALLERY_AUTOPILOT=1 cargo run -p choscorui-gallery` pass under `headless-session.sh` on both backends and both painters |
| Performance stays within budgets | CI: `choscorui-bench --check bench/budgets-linux.txt` passes the non-timing keys. Reference machine: the full file passes, and `docs/performance.md` records the machine and baselines |
| Same jobs run on the maintainer's Mac | `scripts/linux/run-in-container.sh check` and `... native` succeed in Docker Desktop on Apple silicon |
| Docs are current | `scripts/check-docs.sh` passes, and `docs/platform-linux.md` has the capabilities table, role mapping, and limits (Wayland origin) |
| macOS is unaffected | The existing macOS `check` and `native` jobs pass unchanged, except the added Linux cross-check, `KeyConvention::Mac`, and the facade `bylaw` change |

## Constraints and risks

- **Checks.** Run the CI `check` and `native` job commands from `CONTRIBUTING.md` on macOS. Run the new `linux-check` and `linux-native` commands in Docker or on Linux. Run `scripts/check-docs.sh` for Markdown. Native smoke tests, the gallery, and autopilot run one at a time per display server. Report exact commands and outcomes, and name the checks that were not run.
- **Human-only verification.** Automation does not verify these items, so do not claim them:
  - real input methods (IBus, Fcitx, and so on) on GNOME, KDE, or X11
  - Orca
  - real GPU drivers (Mesa hardware, NVIDIA proprietary)
  - GNOME (mutter) and KDE (KWin) behavior, including CSD under mutter
  - HiDPI and fractional scaling on real monitors
  - budgets off the Linux reference machine

  Check these by hand in a VM or on hardware, and list them in each PR as unverified.
- **Dependency rule.** Do not add crates. System libraries are loaded at runtime and are not shipped. Record them in `docs/licenses.md` as runtime system dependencies with their licenses (LGPL for some, for example libdbus, which is dynamically loaded only).
- **Layering.** Platform policy stays in core and controls (`KeyConvention`, decoration controls, the drawn file dialog). `choscorui-linux` depends only on `choscorui-core`. CSD and the drawn file dialog are controls that the facade composes, not backend code.
- **Risk: scope.** The work is comparable to the 19k-line macOS backend plus X11. Keep milestones small and merge them in order. Do not start M(n+1) on a red M(n).
- **Risk: dlopen typos.** A wrong symbol name or signature is caught only at runtime without the binding check. The binding check must cover every declared symbol, struct, and constant.
- **Risk: driver variance in GLES.** Shader compile failures and EGL extension gaps differ by driver. Every failure path must fall back to Cairo (R5). The parity test runs on llvmpipe only.
- **Risk: XIM.** XIM is old and its servers vary, so automated coverage is unit-level for X11.
- **Risk: CI headless fidelity.** sway is not GNOME. CSD-without-SSD is forced by env in CI and checked by hand on GNOME.
- **Risk: Docker on Apple silicon.** It runs aarch64 Linux natively. x86_64 coverage comes from the `ubuntu-22.04` CI runner, not from local emulation.
- **Risk: binary size.** dlopen keeps link-time size small, but the backend code adds size. Add `size.*` keys for the Linux gallery release binary to `bench/budgets-linux.txt` from the first M1 measurement.
- **Compatibility.** `BackendCapabilities` and `Condition` are `#[non_exhaustive]`, so `keyboard` and `StatusNotifierHost` are additive. `Painter` gains Linux-only variants inside `choscorui::platform`. Apps that match `command` shortcuts keep working and gain Ctrl on Linux. Mention the `KeyConvention` in `docs/versioning.md` if it affects the public surface policy.
- **Deferred.** D1 (reference machine identity) is supplied by the maintainer before timing budgets are committed. D2 and D3 are implementer choices bounded by parity, goldens, and the bench.
- **Existing artifacts.** `crates/choscorui-linux/src/{backend,wayland,x11,frame,input,clipboard}.rs` and `tests/protocol_contracts.rs` are the state machines to build on. `crates/choscorui/src/backend/macos.rs` is the facade contract to match. `docs/platform-macos.md` is the behavior and docs template. `docs/specs/2026-10-13-metal-gpu-renderer.md` holds the parity and fallback precedent.

Read this whole spec and inspect the current workspace before implementing.
