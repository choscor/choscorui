# Linux backend

In development. The backend opens and paints windows on Wayland. The full rewrite of this document comes with the last milestone of the [Linux backend spec](specs/2026-10-15-linux-backend.md).

The `choscorui-linux` crate has platform-neutral state machines for the Wayland and X11 protocols. On Linux, it also has the native backend, and the facade uses it through `backend/linux.rs`:

- `Application`, `Window`, and `Poster` on Wayland: windows, keyboard and pointer input, scale, and timers.
- Image decoding and encoding with the system codec libraries.

`choscorui::capabilities()` reports `backend: "wayland"` and `multiple_windows: Supported` when a compositor accepts the connection. Without a display server, it returns `BackendCapabilities::none("headless")`. The `keyboard` field is always `KeyConvention::Pc`. [Painters](#painters) gives the `gpu_painting` rule. The other capabilities are `Unsupported` until their milestone lands.

The state machines compile and run on every host. [`tests/protocol_contracts.rs`](../crates/choscorui-linux/tests/protocol_contracts.rs) pins each rule in this document.

## Backend selection

Wayland and X11 are separate backends. `select_backend(requested, wayland_display, x11_display)` chooses one `BackendKind`:

- With no request, it chooses Wayland when `WAYLAND_DISPLAY` is set and not empty. Otherwise it chooses X11 when `DISPLAY` is set and not empty.
- With an explicit request, it returns that backend only when its display is available. It never uses the other display server. The error is `BackendSelectionError::RequestedUnavailable`.
- With no display, it returns `BackendSelectionError::NoDisplayServer`.

`LinuxCapabilities::for_backend` gives the optional features of each backend:

| Feature | Wayland | X11 |
| --- | --- | --- |
| Frame callbacks | Available | Unavailable: X11 uses expose and presentation events |
| Clipboard | Depends on `wl_data_device` | Available |
| Input method | Depends on `text-input-v3` | Depends on XIM |
| Touch | Depends on the `wl_touch` seat capability | Depends on XInput2 |
| Tablet | Depends on `tablet-v2` | Depends on XInput2 |
| Gestures | Depends on `pointer-gestures-v1` | Unavailable: core X11 has no portable gesture protocol |
| Drag and drop | Depends on `wl_data_device` | Depends on XDND |

## Wayland lifecycle

`WaylandConnection` models the connection and the globals:

1. Call `connect`.
2. Call `advertise(global, version)` for each global that the compositor announces. A version of 0 is an error.
3. Call `bind_required_globals`. It returns `WaylandError::MissingGlobal` when the compositor or `xdg_wm_base` is missing.
4. Call `create_toplevel` to get a `WaylandSurface`.

`WaylandSurface` models the `wl_surface` and `xdg_surface` configure handshake:

```mermaid
stateDiagram-v2
    [*] --> Created: create_toplevel
    Created --> InitialCommit: commit_initial
    InitialCommit --> Configured: receive_configure
    Configured --> Configured: newer receive_configure
    Configured --> Acknowledged: ack_configure with the latest serial
    Acknowledged --> Configured: newer receive_configure
    Acknowledged --> Applied: commit_buffer
    Applied --> Configured: newer receive_configure
    Applied --> Applied: commit_buffer
```

- The first commit has no buffer. `receive_configure` before `commit_initial` returns `WaylandError::InitialCommitRequired`.
- A configure gives a serial, a logical size, and a buffer scale. A size or a scale of 0 is an error.
- A newer configure replaces the latest one and clears the acknowledgement. The older serial becomes superseded.
- `ack_configure` accepts only the latest serial. It returns `SupersededConfigure` for an older serial and `UnknownConfigure` for a serial that the surface never received.
- `commit_buffer` returns `ConfigureNotAcknowledged` until `ack_configure` accepts the latest serial. After a commit, `logical_size` and `buffer_scale` give the applied configure.

`FrameScheduler` keeps at most one frame callback outstanding. `request` returns `FrameError::AlreadyPending` while a callback is outstanding. `presented` accepts only the token of the outstanding callback and returns `FrameError::UnknownCallback` for a stale token.

## X11 lifecycle

`X11Window` models the X11 window:

1. Call `connect`.
2. Call `create(width, height)`. A size of 0 is an error.
3. Call `map`.

`configure_notify(width, height, scale)` applies the new size and scale at once. X11 has no configure acknowledgement and no frame callback.

## X11 display server

On Linux, `Application::new` connects to the X server of `DISPLAY` when the selection gives X11. It loads libxcb and its extensions with `dlopen`. Without `DISPLAY`, or when the server refuses the connection, it returns `Error::NoDisplay`.

- **Windows.** Each window has `WM_DELETE_WINDOW`, `_NET_WM_NAME`, `WM_NAME`, `WM_CLASS` from the application id, and `_NET_WM_PID`. `WM_NORMAL_HINTS` holds the minimum size and north-west gravity. The window manager draws the frame, so `has_server_decorations` is true.
- **Events.** `WM_DELETE_WINDOW` gives `Closing`. `ConfigureNotify` gives `Resized`, and the first `MapNotify` gives `Resized` and activates the window. Focus events give `Focused`, and `VisibilityNotify` gives `Occluded`.
- **Scale.** The scale is `Xft.dpi` over 96, from `RESOURCE_MANAGER` on the root window. A change of the property gives `ScaleChanged` and `Resized`, and the window keeps its logical size.
- **Painting.** The window paints into a client-side image and sends only the pending region. It uses MIT-SHM when the server shares memory with the client. A shared image stays busy until its completion event. Otherwise `PutImage` sends the pixels in parts that fit the maximum request length.
- **Keyboard.** xkbcommon-x11 reads the keymap and the state of the core keyboard. XKB state events update the modifiers. With detectable auto-repeat, a held key arrives as repeated presses, and the keyboard does not repeat keys itself.
- **Pointer.** XInput 2.2 gives the pointer events of the master devices. A scroll valuator gives `WheelUnit::Points`: one increment is 15 points. The wheel buttons 4 to 7 give `WheelUnit::Lines` when the server does not emulate them from a valuator. Without XInput 2.2, core events give the same events.
- **Geometry.** `geometry` reads the position with `TranslateCoordinates`, removes `_NET_FRAME_EXTENTS`, and reads `_NET_WM_STATE`. `set_geometry` configures the window. Before the window shows, it writes `_NET_WM_STATE`. After that, it sends client messages to the window manager.
- **Window manager requests.** `begin_move` and `begin_resize` send `_NET_WM_MOVERESIZE`. `show_window_menu` sends `_GTK_SHOW_WINDOW_MENU`, which some window managers ignore. `minimize` sends `WM_CHANGE_STATE` with `IconicState`.
- **Cursors.** A cursor comes from the cursor theme through xcb-cursor, or else from the core cursor font. A custom image becomes a RENDER cursor. Without RENDER, the arrow shows.
- **Screens.** `screens` gives the RandR monitors in logical units. Without RandR, it gives the root window.

When no window manager runs, as on Xvfb, a mapped window takes the input focus itself. The `_NET_WM_STATE` client messages then have no effect, so the geometry smoke test uses a window that does not show.

## Seat input

`LinuxInput::update_seat(keyboard, pointer, touch)` records the capabilities of the seat. `dispatch` accepts an `InputEvent` only from a device that the seat has:

- A key event without a keyboard returns `InputState::KeyboardUnavailable`.
- A pointer event without a pointer returns `InputState::PointerUnavailable`.
- A touch event without touch returns `InputState::TouchUnavailable`.

When the seat loses the keyboard, the input clears the pressed keys. A removed device then cannot leave a modifier pressed.

## Clipboard states

`Clipboard` keeps two states: the remote offer and the local ownership.

- `receive_offer(mime_types)` records the MIME types that another client offers. `read(mime_type)` returns `ClipboardError::NoOffer` without an offer, and `ClipboardError::MimeUnavailable` for a type that the offer does not have.
- `claim(mime_types)` takes ownership of the selection with the given types. An empty list returns `ClipboardError::EmptySource`.
- `owns_selection` tells you if the application owns the selection. `ownership_lost` clears the ownership when another client claims the selection.

## Clipboard and drag and drop

`Application::clipboard()` gives a `SystemClipboard` for the selection of the display server. `SystemClipboard::temporary()` gives a private board that no other process sees. The API is the same as the macOS `Clipboard`: `write_text`, `read_text`, `write`, `formats`, and `read`. The name is different because `Clipboard` is the state machine above.

Each `DragItem` uses these MIME types:

| Format | Offered types | Also read |
| --- | --- | --- |
| `Text` | `text/plain;charset=utf-8`, `text/plain`, and `UTF8_STRING` | |
| `Files` | `text/uri-list` with `file://` URIs | |
| `Image` | `image/png` | JPEG, TIFF, GIF, WebP, and HEIF, converted to PNG |
| `Url` | `text/x-moz-url`, and `text/uri-list` when there are no files | |
| `Custom(name)` | none | |

- Custom data stays in the process. Other processes never see it.
- Text with a NUL character, and a relative file path, return `Error::InvalidClipboardText`. An image that is not PNG, and a custom name that is not valid, return `Error::UnsupportedFormat`.
- While the application owns the selection, it reads its own data. It does not ask the display server.
- A read from another client stops after 2 seconds without data.

On Wayland, `wl_data_device` carries the selection and the drags. The compositor accepts a selection only from a client with recent input. Thus the backend sends the serial of the last key, button, or focus event. `clipboard` and `drag_and_drop` are `Supported` when the compositor has `wl_data_device_manager`.

`Window::accept_drops(formats, operations)` makes a window a drop target. `Window::begin_drag(items, allowed, preview)` starts a drag:

- Call `begin_drag` while the left button is down on the window. Otherwise it returns `Error::NoDragEvent`, because the drag needs the serial of that press.
- The center of `preview` follows the pointer.
- The target window gets `WindowEvent::Drag` events. The source window gets `WindowEvent::DragEnded` with the operation.
- Wayland has copy and move actions, but no link action.
- X11 has copy, move, and link actions. The pointer shows no preview.
- The modifiers follow the PC convention: Ctrl copies, Shift moves, and Ctrl+Shift links. Without a modifier, a drag in the same application moves, and a drag from another application copies.

On X11, the selections of the ICCCM carry the clipboard, and XDND version 5 carries the drags. `clipboard` and `drag_and_drop` are always `Supported` on X11.

- **Clipboard.** A hidden window owns `CLIPBOARD`. It answers `TARGETS`, `TIMESTAMP`, `MULTIPLE`, each MIME type, `UTF8_STRING`, and `TEXT`. It also answers `STRING` with Latin-1 text. `SelectionClear` ends the ownership.
- **Reads.** A read asks the owner to convert the selection into a property of the hidden window. While it waits, the client answers requests for its own selections and keeps the other events for the event loop. When the other owner offers only `STRING`, the text reads as `text/plain`.
- **Large data.** Data larger than 256 KiB, or than a quarter of the maximum request length, goes with INCR in chunks. Reads accept INCR too. Each step of a read stops after 2 seconds without data.
- **Drop targets.** Each window has `XdndAware`. At the first `XdndPosition`, the window decides if it takes the drag, and sends `Enter`. Each `XdndStatus` asks for all positions. On `XdndDrop`, the window reads `XdndSelection` at the time of the drop, and sends `XdndFinished`.
- **Drag sources.** `begin_drag` needs the left button down on the window. The hidden window owns `XdndSelection` and holds `XdndTypeList` and `XdndActionList`. The client grabs the pointer and sends the messages to the aware window under it. It sends one position at a time, and waits for the status. The source gets `DragEnded` on `XdndFinished`, or `DragOperation::None` after 5 seconds without it.
- **Tests.** `Window::simulate_pointer` is hidden test support. On X11 it moves the pointer and presses or releases the left button through XTEST. On Wayland it returns `Error::MissingProtocol`.

## Input methods

`TextInputState` tells the input method about the focused text control. `active` is true while a text control has the focus. `text` is the committed text of the control, and the UTF-16 ranges count in it.

On Wayland, `zwp_text_input_v3` connects the window to the input method:

1. When `active` becomes true, the backend enables the text input.
2. Each state change sends the surrounding text, the cursor rectangle, and a commit. The surrounding text goes only while nothing is marked, because it does not contain the marked text.
3. The protocol uses byte offsets. The backend converts them to and from UTF-16 offsets.
4. A text longer than 4000 bytes sends a part of 4000 bytes or less around the selection.

The changes of the input method come in a group that ends with `done`. Each group gives these `ImeEvent`s, in this order:

- A commit or a deletion gives `CommitText`. A deletion around the cursor gives a `replacement_utf16` range.
- New marked text gives `SetMarkedText`. When the input method clears its marked text without a commit, the event has empty text.

`input_methods` is `Supported` when the compositor has `zwp_text_input_manager_v3`.

On X11, XIM connects the window to the input method, on-the-spot:

1. The client opens the input method of `XMODIFIERS` on a second Xlib connection. It uses the input method only when it takes preedit callbacks. The local input method of Xlib does not, so without `XMODIFIERS` there is no input method. Keys then type their text through xkbcommon and compose.
2. When `active` becomes true, the window gets an input context. The context has the focus while the window has it. Its spot is the bottom-left corner of the caret. When `active` becomes false, the context closes, and marked text gives `Unmark`.
3. Each key of a window with a focused context goes to the input method first. The keys that the input method does not use come back, and the window gets them as usual.
4. The preedit draw callback gives `SetMarkedText`, with the caret as an empty selection in UTF-16 units. The preedit done callback gives `Unmark`. Committed text gives `CommitText` without a replacement range.

XIM has no surrounding text, so the X11 backend does not send `text`. `input_methods` is `Supported` on X11 when such an input method opens.

## Wayland cursors

The cursor of a window comes from the first source that the compositor allows:

1. A custom image goes in a `wl_shm` buffer on a cursor surface.
2. With `wp_cursor_shape_v1`, the compositor draws the cursor of the shape.
3. Else, libwayland-cursor loads the Xcursor theme of `XCURSOR_THEME` at the size of `XCURSOR_SIZE`. The default size is 24 pixels.

The theme loads at the integer scale of the window under the pointer, and the cursor surface has that buffer scale. When the theme has no cursor of the shape, the cursors that libwayland-cursor has built in show.

## Application events

The facade queues these `DesktopEvent`s on the Linux services:

- `OpenFiles` and `OpenUrls` for the files and URLs in the command-line arguments at launch. An argument that starts with `-` is an option, until the argument `--`.
- `Activated` when a window of the application gets the keyboard focus, and `Deactivated` when no window has it. An activation also reads the appearance settings again.
- `Reopen` when the desktop calls `org.freedesktop.Application.Activate`. Its `has_visible_windows` tells if a window of the application shows.

## Alert sound

`Application::beep()` plays the alert sound of the controls. It uses the first source that works:

1. On Wayland, `xdg_system_bell_v1` rings the bell for the focused window. On X11, the XKB bell of the core keyboard rings for the focused window.
2. The libcanberra event sound `bell` plays from the sound theme.
3. Else, the application makes no sound.

The facade `Feedback` of the application calls `beep()`.

## Dialogs

Linux has no alert service, and a file chooser needs a portal. The toolkit draws these dialogs in modal windows of their own.

`Application::begin_modal(window)` makes `window` the modal window of the application until the `ModalSession` drops:

- The window becomes a dialog of its parent. Give the parent with `Window::set_modal_parent`. Without one, the parent is the focused window, or else the newest visible window.
- On Wayland, the window calls `xdg_toplevel.set_parent`. With `xdg_wm_dialog_v1`, it also gets an `xdg_dialog_v1` and calls `set_modal`.
- On X11, the window gets `WM_TRANSIENT_FOR` and the `_NET_WM_STATE_MODAL` state.
- The other windows drop their pointer, key, text, input method, and drag events. They keep their last frame. A close request waits in their queue.

**Alerts.** `choscorui::platform::DrawnDialogs::show_alert(spec)` shows an `AlertSpec` over the active window of an `AppContext`. The contract and the validation of `AlertSpec` are the same as on macOS.

- The window shows `AlertDialog::window`, at the size that the alert measures. The message is the title, and the detail is the text under it.
- The first button is the default button: Enter runs it. Escape runs a button titled Cancel, in English or in the locale of the application.
- A button gives `AlertResponse::Button(index)`. Escape without a Cancel button, or a close of the window, gives `AlertResponse::Cancelled`.
- `AlertStyle` does not change the drawn alert.
- `DialogError::Unavailable` tells that the display server could not open the window.

**File dialogs.** `Services::choose_files` uses the `org.freedesktop.portal.FileChooser` portal. Without a portal, or without a backend for its file chooser, the services open the drawn `FileDialog` control.

- The drawn dialog starts in the folder of the request, or else in `HOME`. It shows the title, the accept label, the extensions, and the suggested name of the request.
- Accept gives the chosen paths. Cancel, Escape, or a close of the window gives `FileDialogResponse::Cancelled`.
- `native_dialogs` is always `Supported`.

Each modal window runs a nested event loop until it has a result. `DrawnDialogs::set_modal_driver` gives a test harness each turn of that loop: the view host, the native window, and its focus.

[`modal_smoke.rs`](../crates/choscorui-linux/tests/modal_smoke.rs) and [`dialog_smoke.rs`](../crates/choscorui/tests/dialog_smoke.rs) check this behavior on headless sway.

## Image codecs

On Linux, `decode_image(bytes)` gives straight-alpha sRGB RGBA8 pixels, as the macOS backend does. The first bytes select the format and its library:

| Format | Signature | Library |
| --- | --- | --- |
| PNG | `\x89PNG\r\n\x1a\n` | libpng16 (`libpng16.so.16`) |
| JPEG | `FF D8 FF` | libturbojpeg (`libturbojpeg.so.0`) |
| GIF, first frame | `GIF87a` or `GIF89a` | giflib (`libgif.so.7`) |
| TIFF | `II*\0`, `MM\0*`, or BigTIFF | libtiff (`libtiff.so.6` or `libtiff.so.5`) |
| WebP, still images | `RIFF` and `WEBP` | libwebp (`libwebp.so.7`) |
| HEIC and HEIF | an `ftyp` box with a HEIF brand | libheif (`libheif.so.1`) |

- A JPEG file turns as its EXIF `Orientation` tag tells. libheif applies the rotation and mirror properties of a HEIF file. libtiff applies the orientation tag of a TIFF file.
- A GIF frame shows on its logical screen. The pixels outside the frame, and the transparent index, are transparent.
- Unknown bytes, and a format whose library does not load, return `ImageError::UnsupportedFormat`. Damaged data returns `ImageError::DecodeFailed`.
- An image of more than 64 Mpx returns `ImageError::InvalidDimensions`.
- An animated WebP file returns `ImageError::DecodeFailed`, because the simple libwebp API decodes only still images.
- The decoders do not convert embedded ICC profiles. libpng converts the PNG gamma to sRGB.
- `encode_png(image)` writes an sRGB PNG file with libpng.
- `image_codecs_support()` is `Supported` when libpng and libturbojpeg load.

## Painters

A window paints with one of two painters, and `Window::painter()` tells which:

- `Painter::Gles` paints with OpenGL ES 3.0 through EGL on a `wl_egl_window`.
- `Painter::Cairo` paints with Cairo into double-buffered `wl_shm` buffers.

### Fallback

- New windows use GLES. They use Cairo when `CHOSCORUI_PAINTER=cairo` is set, or when libEGL, libGLESv2, or libwayland-egl does not load.
- New windows also use Cairo when no EGL config has RGBA8 and OpenGL ES 3.0, or when the shaders do not compile.
- The shaders compile when the application connects, before a window opens. A compile or link failure is a `GlesError`, not a panic.
- A GPU failure during a paint switches that window to Cairo for the rest of its life. The window then repaints all of its content.
- A lost context, a failed swap, and an OpenGL ES error during a paint are GPU failures. The other GLES windows switch to Cairo at their next paint.
- `capabilities().gpu_painting` is `Supported` when the GLES painter started and no GPU failure occurred. Else it is `Unsupported`.
- `Window::capture()` renders the display list with Cairo for both painters. `render_offscreen` is the Cairo export.
- `render_offscreen_gles` renders with GLES on the Mesa surfaceless EGL platform. The parity test uses it.

### GLES frames

- The swap interval is 0. The `wl_surface.frame` callback and `FrameScheduler` pace the paints, as they do for Cairo.
- The window sends the viewport, the window geometry, and the frame request before the swap. The swap attaches the buffer and commits the surface.
- With `EGL_EXT_buffer_age`, a paint repaints the damage of each frame since the back buffer was on the screen. Without it, a paint repaints the whole window.
- With `EGL_KHR_swap_buffers_with_damage` or `EGL_EXT_swap_buffers_with_damage`, the swap gives the damage to the compositor.

### GLES drawing

The painter draws each command in the order of the display list, as the Cairo painter does. One shader program multiplies a paint, a coverage, and a clip.

- A rectangle under an axis-aligned transform gets exact area coverage in the shader.
- Cairo rasterizes the coverage of other fills and strokes into a mask in device pixels. The painter keeps the masks that repeat from frame to frame.
- Glyphs come from one atlas texture. Cairo rasterizes each glyph with the faces of the Cairo painter, at the quarter-pixel phase of its position.
- Thus, both painters cover the same pixels, and they put glyphs at the same positions.
- A clip on whole pixels is a scissor rectangle. Another clip renders its coverage, times the outer clip, into a coverage texture.
- An opacity group draws into a layer texture. The pop of the group composites the layer with the opacity.
- Backdrop blur reads the base target back, blurs it with `blur_pixels`, and draws it in the rounded rectangle.

## Not implemented

- A connection to a Wayland compositor
- Native event loops
- Windows on the screen and painting
- Text shaping and fonts
- Accessibility
- Desktop portals
- The `choscorui::platform` module and capability values for Linux
