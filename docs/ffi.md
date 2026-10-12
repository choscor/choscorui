# FFI safety record

This record describes each hand-written FFI boundary in `choscorui-macos`. For each boundary, it gives the ownership rule, the thread rule, and the tests. Add an entry here in the same change as new `unsafe` FFI.

## General rules

- The crate binds AppKit, Foundation, Core Foundation, Core Graphics, Core Text, ImageIO, UserNotifications, and the Objective-C runtime by hand. The crate uses no build script and no binding generator.
- The ABI shapes and numeric constants come from the macOS SDK headers, for example `NSApplication.h`, `NSWindow.h`, `NSGraphics.h`, and `objc/message.h`.
- Raw Objective-C ids stay private to the crate. The safe types are `!Send` and `!Sync`, except `Poster`.
- Each `objc_msgSend` cast uses the SDK argument and return ABI. The casts pass `NSRect` by value and use `i8` for Objective-C `BOOL`.
- The callbacks for events, accessibility actions, drawing, posted tasks, timers, menus, and the application delegate catch Rust panics before they return.
- Some small callbacks do not catch panics, for example `accessibility_character_count` and `notification_will_present`. If one of them panics, Rust stops the process at the `extern "C"` boundary. Thus, a panic never unwinds into AppKit.
- Objective-C exceptions are not caught. The crate calls selectors only with validated inputs.
- Apple's [NSApplication reference](https://developer.apple.com/documentation/appkit/nsapplication) and [NSWindow reference](https://developer.apple.com/documentation/appkit/nswindow) define the platform behavior.

## Architectures

On arm64, every `NSRect` return uses `objc_msgSend`. On x86-64, a 32-byte `NSRect` return needs `objc_msgSend_stret`. The helpers `send_rect`, `send_rect_rect`, and `send_rect_rect_id` select the correct function for each architecture.

Smaller structs (`NSPoint`, `NSSize`, `NSRange`) return in registers on both architectures. `CGFloat` is `f64` on both architectures. CI runs the build and the native tests on `macos-15` (arm64) and `macos-15-intel` (x86-64).

## Boundaries

### Application

- **Ownership.** `NSApplication sharedApplication` is a process singleton. Rust never releases it. `Application` is a handle that does not own it.
- **Activation policy.** `setActivationPolicy:` can refuse a switch that is not necessary when the application starts from a bundle. A refusal is an error only if `activationPolicy` is still not Regular.
- **Threads.** Construct and use `Application` on the main thread only.

### Windows

- **Ownership.** `initWithContentRect:...` returns a +1 window. `setReleasedWhenClosed:NO` stops `close` from releasing it. `Window::drop` closes the window and then releases it one time.
- **Threads.** All methods and `Drop` run on the main thread.

### Autorelease pools and strings

- **Pools.** A local RAII guard creates and drains each `NSAutoreleasePool` on one thread. Autoreleased objects do not live after the call.
- **Strings.** `stringWithUTF8String:` returns an autoreleased `NSString`. A setter such as `setTitle:` copies or retains it before the pool drains. The crate rejects a Rust string that contains NUL.

### Run loop source and posted work

- **Ownership.** `Application` owns the +1 Core Foundation run-loop source. On `Drop`, it removes, invalidates, and releases the source. An `Arc` keeps the callback context alive through the last callback.
- **Threads.** Workers signal the source through `Poster`. The source callback runs the queued tasks on the main thread. A mutex stops worker signals during teardown.
- **Panics.** The callback catches a panic in each posted task, increments a count, and continues with the next task.
- **Stop.** `Poster::request_stop` posts `NSApplication stop:` to the main thread. AppKit checks the stop flag only after `run` handles an event. Thus, `stop_application` also posts an application-defined `NSEvent` with `postEvent:atStart:`, and `run` returns without user input.

### Timers

- **Ownership.** A `Timer` owns the +1 `CFRunLoopTimer` and its callback context. `Drop` removes, invalidates, and releases the timer, and then frees the context.
- **Callbacks.** The context holds a weak reference to its owner. Thus, a cancel or a drop inside the callback cannot free the context during the callback.
- **Repeating timers.** The `FnMut` stays in the context between ticks. A cancel inside the callback stops the next delivery.
- **Modes.** Timers and the posted-work source use `kCFRunLoopCommonModes`. They run during live resize, menu tracking, and modal panels.
- **Order.** Timers with equal deadlines, and timers that are ready at the same time as posted work, have no defined order.
- **Teardown.** `Application` teardown cancels all timers that are still alive.

### Window delegate

- **Ownership.** One registered Objective-C subclass handles the resize, scale, focus, and close selectors. `NSWindow.delegate` is weak, so each window owns a +1 delegate.
- **Teardown.** `Drop` closes the window, sets the delegate to nil, and removes its queue. Then it releases the delegate and the window.
- **Callbacks.** AppKit calls the delegate on the main thread. Each callback copies data into an owned Rust queue.

### Content view and text input

- **Ownership.** The window retains its custom flipped content view. Rust releases the +1 creation reference after `setContentView:`.
- **Events.** The view maps mouse, wheel, key, and `NSTextInputClient` data into owned `WindowEvent` values. Precise scroll deltas keep point units, and coarse deltas keep line units.
- **Text input.** Marked text, UTF-16 ranges, candidate-window rectangles, and character-index queries cross the boundary as owned values.
- **Locks.** `interpretKeyEvents:` runs without the event-queue lock. A fallback for ordinary characters runs only when no text callback occurred.
- **Tests.** The native smoke tests call the marked-text and committed-text callbacks directly with Japanese text.

### Painting

- **Context.** `drawRect:` borrows the current `CGContext` only while it draws. No `CGContext` goes into a safe API.
- **Display list.** Each window owns a shared `Arc<DisplayList>`. The callback replays it and does not copy it.
- **Color.** Colors, gradients, and images use one sRGB color space, `CGColorSpaceCreateWithName(kCGColorSpaceSRGB)`. The process keeps one +1 reference.
- **Commands.** Each push command maps to exactly one `CGContextSaveGState`. Group opacity uses a transparency layer.
- **Shadows.** A shadow draws the mask from `Shadow::mask` through `CGContextClipToMask`. Thus, it matches the software renderer.
- **Backdrop blur.** `BackdropBlur` and `BackdropBlurCorners` read the pixels of a 32-bit bitmap context through `CGBitmapContextGetData`. They check the row stride before they read rows. They blur a copy with `blur_pixels` and draw the copy back in device space, clipped to the rounded rectangle. A context that is not a bitmap context gets no blur. [`render_parity.rs`](../crates/choscorui/tests/render_parity.rs) compares the result with the software renderer.
- **Damage.** `set_display_list` compares the old and new lists with `choscorui_core::damage`. It calls `setNeedsDisplayInRect:` only for the changed area, so an idle window does not draw.

### Images and ImageIO

- **Cache.** `cg_image` copies RGBA pixels into a premultiplied +1 `CGImage`. A thread-local cache keyed by `Image::id` holds up to 256 images. The cache clears when the last window drops.
- **Decode.** `decode_image` decodes frame 0 with `CGImageSource` and draws it into an sRGB bitmap. Thus, the decoder converts the embedded color profile. `decode_image` rejects images larger than 64 Mpx.
- **Encode.** `encode_png` writes into an owned `CFData` and copies the bytes before the release.
- **Threads.** Decode and encode work on any thread and create their own autorelease pool. Paint with cached images occurs on the main thread only.

### Cursors, capture, and occlusion

- **Cursors.** `resetCursorRects` adds one cursor rectangle for the view bounds. The crate rejects a custom cursor larger than 256 points, or with a hotspot outside the image.
- **Capture.** `Window::capture` uses `cacheDisplayInRect:toBitmapImageRep:` and copies the result into an owned sRGB `Image`.
- **Occlusion.** `windowDidChangeOcclusionState:` queues `WindowEvent::Occluded`.
- **Tests.** The smoke test checks idle paint counts, damage rectangles, captured pixel colors, and cursor validation.

### Core Text

- **Lines.** `ShapedLine` owns the `CTLine` that it uses for metrics, caret offsets, hit tests, and `CTLineDraw`. Rust RAII releases each Core Text object.
- **Wrapping.** `WrappedText` keeps its attributed string and font alive while a +1 typesetter gives break counts. Rust maps each break to a grapheme boundary.
- **Shaping.** `CoreTextShaper` creates the font, paragraph style, attributes, attributed string, and typesetter for each paragraph. It copies glyphs, positions, and font names into owned `GlyphRun` values. No Core Text object lives after `shape`.
- **Font faces.** `fonts::create_font` makes each primary font. The generic families `system-ui` and `ui-monospace` call `NSFont systemFontOfSize:weight:` and `monospacedSystemFontOfSize:weight:`. These return autoreleased fonts, and `CFRetain` makes them +1. `NSFont` is toll-free bridged to `CTFont`. Other families use `CTFontCreateWithName` and the symbolic bold and italic traits.
- **Tracking.** `fonts::kern_attribute` makes a +1 `CFNumber` for `kCTKernAttributeName`. The attribute dictionary retains it, and Rust RAII releases the local reference.
- **Fonts for paint.** Paint creates fonts again by PostScript name, through a thread-local cache of 64 entries. It draws with `CTFontDrawGlyphs`.
- **Text matrix.** Graphics-state save and restore do not include the text matrix, so the painter restores it explicitly.
- **Threads.** `shape` works on any thread. Paint occurs on the main thread or in an offscreen bitmap.
- **Tests.** The `fonts` unit test makes each generic face at three weights. The tests cover Vietnamese, CJK, emoji sequences, combining marks, Hebrew, Arabic, font fallback, wrapping, and painted ink at 1x and 2x.

### Clipboard and drag and drop

- **Clipboard.** `Clipboard` retains the pasteboard. A temporary named pasteboard calls `releaseGlobally` before its release. The general pasteboard never calls `releaseGlobally`.
- **Formats.** `pasteboard.rs` maps the core formats to UTIs: `public.utf8-plain-text`, `public.file-url`, `public.png` (with a `public.tiff` fallback), `public.url`, and `com.choscorui.private.<name>`.
- **Copies.** Each read and write uses a local autorelease pool. It copies every string and `NSData` before the pool drains. File paths return in precomposed (NFC) form.
- **Drop target.** The content view implements `NSDraggingDestination`. It negotiates the operation through `choscorui_core::negotiate_drag`, and it reads the payload only in `performDragOperation:`.
- **Drag source.** As `NSDraggingSource`, the view reports the operations from `Window::begin_drag` and queues `WindowEvent::DragEnded`. `begin_drag` needs a current left mouse-down or drag event in the window.
- **Threads.** All calls occur on the main thread. A callback that fails returns `NSDragOperationNone` or `NO`.
- **Tests.** The smoke tests use a temporary pasteboard, so the general clipboard does not change. `simulate_external_drag` drives the real destination callbacks with a test `NSDraggingInfo` class.

### Native dialogs

- **Ownership.** An `NSAlert` is a +1 object that a Rust RAII guard holds. The local pool of the call releases the panels, strings, arrays, and URLs.
- **Copies.** Selected paths become owned `PathBuf` values before the pool drains.
- **Validation.** The calls reject inputs that contain NUL. File filters also reject separators and leading dots.
- **Threads.** The calls need the `Application` handle and run the synchronous modal loop of AppKit. A cancel returns a typed response, not an error.

### Offscreen bitmaps

- **Ownership.** `MacBitmap` creates a +1 sRGB bitmap context. Rust RAII releases it. `MacBitmap` copies each row before the context frees its memory.
- **Validation.** `MacBitmap` checks the row stride and the total length before raw slice access.
- **Tests.** [`render_parity.rs`](../crates/choscorui/tests/render_parity.rs) renders each vector fixture through `MacBitmap` and the software `Raster` at 1x and 2x and compares the pixels.

### Preferences

- **Calls.** `preferences.rs` reads `accessibilityDisplayShouldReduceMotion` and `accessibilityDisplayShouldIncreaseContrast` from `NSWorkspace`. It reads `NSApp.effectiveAppearance.name` and copies it into a Rust `String` in a local pool.
- **Threads.** The calls occur on the main thread. The calls keep no object after they return.

### Accessibility

- **Announcements.** `Window::announce` builds an autoreleased `NSDictionary` in a local pool. It posts `NSAccessibilityAnnouncementRequestedNotification` through `NSAccessibilityPostNotificationWithUserInfo`.
- **Collections.** Element updates set `accessibilityRowCount`, `accessibilityColumnCount`, and `accessibilityIndex` with `NSInteger` values, saturated from `usize`.
- **Tests.** `window_smoke` reads the row count, the index, and the text area role back from the native elements.

The [`choscorui-macos` README](../crates/choscorui-macos/README.md) describes the full accessibility bridge.

### Window placement and context menus

- **Placement.** `window_state.rs` reads `frame`, `isZoomed`, and the full-screen bit of `styleMask`. Global coordinates flip against the height of the first screen in `+[NSScreen screens]`.
- **Context menus.** `ContextMenu` owns a +1 `NSMenu` and a +1 target object. Only the menu retains its items.
- **Tracking.** `Window::show_context_menu` calls `popUpMenuPositioningItem:atLocation:inView:`. The menu and its target stay alive during this synchronous call.
- **Tests.** `window_smoke` sends geometry through `set_geometry` and `geometry`. It also selects context-menu items through the native target and action.

### Desktop integration

- **Delegate.** `desktop.rs` registers `ChoscoruiV2AppDelegate`. The process creates one instance and never releases it, because `NSApplication` and `UNUserNotificationCenter` hold their delegates weakly.
- **Launch.** The crate installs the delegate before `finishLaunching`, so the application receives open-document events at launch.
- **Callbacks.** Callbacks copy URL paths and strings into Rust and queue `AppEvent` values behind a `Mutex`.
- **Blocks.** The delegate calls each system completion block exactly one time, through the `invoke` pointer of the block ABI.
- **Authorization block.** The authorization callback is a global block with no captures. It writes an `AtomicU8`, so any queue can call it.
- **Status item.** `StatusItem` retains its `NSStatusItem` and owns its `ContextMenu`. `Drop` clears the menu, calls `removeStatusItem:`, and then releases the item.
- **Notifications.** UserNotifications calls need a bundle identifier, because the framework raises an Objective-C exception without one. Without a bundle, the call returns an error.
- **Tests.** `window_smoke` drives the open-URLs and reopen methods, the status item, URL handler lookup, and the notification error for an application without a bundle.

## Smoke tests

The native smoke tests create real windows, so they run only when `CHOSCORUI_RUN_MACOS_SMOKE=1` is set. [CONTRIBUTING.md](../CONTRIBUTING.md) lists the commands.

The tests check window construction, title, visibility, resize, scale, and close events. They also check event isolation between two windows, posted work, timers, input translation, scroll units, `drawRect:` calls, panic containment, and loop stop.
