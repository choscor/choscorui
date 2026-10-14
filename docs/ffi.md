# FFI safety record

This record describes each hand-written FFI boundary in `choscorui-macos`. For each boundary, it gives the ownership rule, the thread rule, and the tests. Add an entry here in the same change as new `unsafe` FFI.

## General rules

The crate binds AppKit, Foundation, Core Foundation, Core Graphics, Core Text, ImageIO, UserNotifications, the process usage calls of libSystem, and the Objective-C runtime by hand. It uses no build script and no binding generator. The ABI shapes and numeric constants come from the macOS SDK headers, for example `NSWindow.h` and `objc/message.h`. Apple's [NSApplication reference](https://developer.apple.com/documentation/appkit/nsapplication) and [NSWindow reference](https://developer.apple.com/documentation/appkit/nswindow) define the platform behavior.

### The `ffi` module

- All `unsafe` code of the crate is in `src/ffi/`. The crate root denies `unsafe_code`, and only `mod ffi` allows it.
- Each `unsafe` block holds one raw operation and a `SAFETY` comment. The `[lints]` table of the crate enforces both rules.
- The code outside `ffi` is safe. It sends messages through a registered `Msg` and holds objects in `Retained` or `CfRetained`.
- Raw Objective-C ids stay private to the crate. The safe types are `!Send` and `!Sync`, except `Poster`.

### The registry

- `messages!` declares each message as a `Msg`: the receiver class or protocol, the selector, the argument types, and the result type.
- `Msg::send` casts `objc_msgSend` to exactly these types. It passes `NSRect` by value and reads Objective-C `BOOL` as one byte.
- `c_functions!` declares the C functions and statics of each framework in one `extern "C-unwind"` block. Each item states `safe` or `unsafe`.
- `sdk_const!` declares each SDK constant with the SDK symbol that it equals. `ffi_struct!` declares each `repr(C)` struct with its C name and C field names.
- `define_classes!` in `ffi/classes.rs` declares each Objective-C class of the crate. Each method states its origin: a superclass method, a protocol method, or a selector that the crate invents.
- The macro makes an `extern "C"` trampoline for each method. It derives the type encoding from the Rust types. `BOOL` encodes as `B` on arm64 and as `c` on x86-64.
- No SDK header declares the Block ABI layouts in `ffi/block.rs`. They follow the clang Block ABI specification.

### Ownership

- `Retained<T>` holds one +1 reference to an Objective-C object and releases it in `Drop`. `CfRetained<T>` does the same for a Core Foundation object.
- A message in the `alloc`, `new`, `copy`, `mutableCopy`, or `init` family returns a +1 object. Any other object result is autoreleased or borrowed from the receiver.
- A C function whose name contains `Create` or `Copy` returns a `CfRetained`. Any other object result is borrowed from its argument.
- A nil result or a missing class returns `None` or an `Error`, not a panic.

### Objective-C exceptions

- `objc_msgSend` and every C function are `extern "C-unwind"`. Thus, an Objective-C exception that leaves a call unwinds through Rust as a defined foreign unwind.
- The callback trampolines stay `extern "C"`. An exception that reaches one of them aborts the process, unless AppKit handles it first.
- The crate calls selectors only with validated inputs, so an exception is a defect. The abort makes it loud.

### Panics in callbacks

- Each trampoline runs its handler inside `catch_unwind`. After a panic, it writes the class and the selector to standard error. Then it returns the fallback of the method, for example `NO`, 0, or a zero rectangle.
- The run loop callbacks and the authorization block also catch panics and report them. Thus, a panic never unwinds into AppKit or Core Foundation.

## Checks

`cargo test -p choscorui-macos` runs these checks. Each check collects all mismatches and then fails one time, so one run shows the whole effect of an SDK change.

- **Runtime.** `messages_match_runtime` compares the type encoding of each `Msg` with the encoding of the method in the Objective-C runtime. `callbacks_match_sdk` compares each crate method with its superclass method or protocol method. A selector that the crate invents must not occur in the superclass or in an adopted protocol. The runtime has no record of a protocol that no loaded image uses, for example `NSApplicationDelegate` on macOS 15. The runtime checks skip such a protocol, and the header check covers it.
- **Ownership.** `ownership_follows_naming` checks the Cocoa naming rule for each `Msg` and the `Create` or `Copy` rule for each C function.
- **SDK headers.** `sdk_headers_compile` makes one Objective-C++ file from all declarations and compiles it against the SDK headers with `xcrun clang++`. The minimum system is macOS 15, and all warnings are errors. The file sends each message, and each protocol method of a crate class, with the header types of its Rust signature.
- **Boundary.** `boundary_is_closed` reads the sources. Outside `src/ffi/`, it fails for a selector literal, `objc_msgSend`, `transmute`, `extern "C"`, or `class_addMethod`. Inside `ffi`, only `define_classes!` adds methods to a class.

How to read a failure:

- A runtime failure names the class, the selector, the expected encoding, and the actual encoding. Change the Rust types of the descriptor to the actual types.
- A header failure is a compiler error. Its message names the symbol, and the note "expression evaluates to" shows both values.
- A deprecation or availability error names the API. Change the code to the API that the message names. Do not suppress the warning.
- If `xcrun` or the SDK is missing, the header check fails and names the prerequisite in [CONTRIBUTING.md](../CONTRIBUTING.md).

## Architectures

On arm64, every `NSRect` return uses `objc_msgSend`. On x86-64, a 32-byte `NSRect` return needs `objc_msgSend_stret`. The return type of each message descriptor selects the correct function for each architecture.

Smaller structs (`NSPoint`, `NSSize`, `NSRange`) return in registers on both architectures. `CGFloat` is `f64` on both architectures. CI runs the build and the native tests on `macos-15` (arm64) and `macos-15-intel` (x86-64).

## Boundaries

### Application

- **Ownership.** `NSApplication sharedApplication` is a process singleton. Rust never releases it. `Application` is a handle that does not own it.
- **Quality of service.** `Application::new` calls `pthread_set_qos_class_self_np` with `QOS_CLASS_USER_INTERACTIVE`. The call changes only the main thread. A failure leaves the default class, and the application stays usable.
- **Activation policy.** `setActivationPolicy:` can refuse a switch that is not necessary when the application starts from a bundle. A refusal is an error only if `activationPolicy` is still not Regular.
- **Threads.** Construct and use `Application` on the main thread only.

### Windows

- **Ownership.** `initWithContentRect:...` returns a +1 window. `setReleasedWhenClosed:NO` stops `close` from releasing it. `Window::drop` closes the window and then releases it one time.
- **Threads.** All methods and `Drop` run on the main thread.

### Autorelease pools and strings

- **Pools.** A local RAII guard creates and drains each `NSAutoreleasePool` on one thread. Autoreleased objects do not live after the call.
- **Strings.** `stringWithUTF8String:` returns an autoreleased `NSString`. A setter such as `setTitle:` copies or retains it before the pool drains. The crate rejects a Rust string that contains NUL.

### Run loop source and posted work

- **Ownership.** `Application` owns a `RunLoopSource`. It holds the +1 run-loop source and one `Arc` count of the posted state. On `Drop`, it removes and invalidates the source, and then releases both.
- **Callbacks.** The source callback takes its own `Arc` count for each call. Thus, the posted state stays alive until the callback returns.
- **Threads.** Workers signal the source through `Poster`. The source callback runs the queued tasks on the main thread. A mutex stops worker signals during teardown.
- **Panics.** The callback catches a panic in each posted task, increments a count, and continues with the next task.
- **Stop.** `Poster::request_stop` posts `NSApplication stop:` to the main thread. AppKit checks the stop flag only after `run` handles an event. Thus, `stop_application` also posts an application-defined `NSEvent` with `postEvent:atStart:`, and `run` returns without user input.

### Timers

- **Ownership.** A `Timer` owns a `RunLoopTimer`. It holds the +1 `CFRunLoopTimer` and a weak reference to the timer state. `Drop` invalidates, removes, and releases the timer, and then frees the weak reference.
- **Callbacks.** The callback upgrades the weak reference for the call. Thus, a cancel or a drop inside the callback cannot free the timer state during the callback.
- **Repeating timers.** The `FnMut` stays in the timer state between ticks. A cancel inside the callback stops the next delivery.
- **Modes.** Timers and the posted-work source use `kCFRunLoopCommonModes`. They run during live resize, menu tracking, and modal panels.
- **Order.** Timers with equal deadlines, and timers that are ready at the same time as posted work, have no defined order.
- **Teardown.** `Application` teardown cancels all timers that are still alive.

### Window delegate

- **Ownership.** One registered Objective-C subclass handles the resize, scale, focus, and close selectors. `NSWindow.delegate` is weak, so each window owns a +1 delegate.
- **Teardown.** `Drop` closes the window, sets the delegate to nil, and removes its queue. Then it releases the delegate and the window.
- **Callbacks.** AppKit calls the delegate on the main thread. Each callback copies data into an owned Rust queue.

### Content view and text input

- **Ownership.** The window retains its custom flipped content view. Rust releases the +1 creation reference after `setContentView:`.
- **Events.** The view maps mouse, wheel, pinch, key, and `NSTextInputClient` data into owned `WindowEvent` values. Precise scroll deltas keep point units, and coarse deltas keep line units. `magnifyWithEvent:` reads `magnification` and drops a scale that is not finite or not positive.
- **Text input.** Marked text, UTF-16 ranges, candidate-window rectangles, and character-index queries cross the boundary as owned values.
- **Key equivalents.** `performKeyEquivalent:` returns `NO` unless the text input state has `captures_keys`. Then it queues the key-down event as `keyDown:` does and returns `YES`.
- **Locks.** `interpretKeyEvents:` runs without the event-queue lock. A fallback for ordinary characters runs only when no text callback occurred.
- **Tests.** The native smoke tests call the marked-text and committed-text callbacks directly with Japanese text. They also offer a ⌘S key equivalent to the view with and without `captures_keys`.

### Painting

- **Context.** `drawRect:` borrows the current `CGContext` only while it draws. No `CGContext` goes into a safe API.
- **Display list.** Each window owns a shared `Arc<DisplayList>`. The callback replays it and does not copy it.
- **Color.** Colors, gradients, and images use one sRGB color space, `CGColorSpaceCreateWithName(kCGColorSpaceSRGB)`. The process keeps one +1 reference.
- **Commands.** Each push command maps to exactly one `CGContextSaveGState`. Group opacity uses a transparency layer.
- **Shadows.** A shadow draws the mask from `Shadow::mask` through `CGContextClipToMask`. Thus, it matches the software renderer.
- **Backdrop blur.** `BackdropBlur` and `BackdropBlurCorners` read the pixels of a 32-bit bitmap context through `CGBitmapContextGetData`. They check the row stride before they read rows. They blur a copy with `blur_pixels` and draw the copy back in device space, clipped to the rounded rectangle. A context that is not a bitmap context gets no blur. [`render_parity.rs`](../crates/choscorui/tests/render_parity.rs) compares the result with the software renderer.
- **Damage.** `set_display_list` compares the old and new lists. An equal list schedules no frame, so an idle window does not draw. The window keeps the sorted draw regions of its list, so the damage sorts only the regions of the new list. Both painters draw a frame only when there is damage. With Metal, the frame draws the full list. With Core Graphics, `set_display_list` calls `setNeedsDisplayInRect:` only for the damaged area.

### Metal painter

- **Objects.** `MTLCreateSystemDefaultDevice` and each `new` message return +1 objects that `Retained` releases. The process keeps one device, one command queue, the pipelines, and the samplers in a `OnceLock`.
- **Setup.** `Application::new` calls `dispatch_async_f` with a function pointer as the context, which is valid for the whole process. A libdispatch global queue then fills that `OnceLock`, without a thread of its own. The first window waits for the `OnceLock`, so it does not create the objects again. The work holds no Rust reference to AppKit objects.
- **Library.** `new_library` copies the precompiled `.metallib` bytes into a `dispatch_data_t` with a null destructor. Thus, Metal does not borrow the Rust slice.
- **Buffers and textures.** `new_buffer` and the `set*Bytes:` messages copy their Rust slices. `replace_region` checks that the region fits in the texture and that the slice holds all its rows. `read_region` checks the region, and `getBytes:` writes into a vector of exactly the rows of the region. It reads only glyph atlases, which no GPU work writes. `read_buffer` checks the buffer length, and it reads only after the GPU work completes. `write_buffer` checks that the buffer is longer than the slices. A window writes its instance buffers in turn. It has one more of them than the frames that the GPU can work on, so no GPU work reads the buffer that it writes.
- **Layer.** The content view hosts a `CAMetalLayer` that the surface owns, with 3 drawables. `nextDrawable` blocks when no drawable is free. Each frame that waits for its presented handler can hold a drawable. Thus, the surface does not call `nextDrawable` while three frames wait. Then, and when `nextDrawable` returns nil, the frame stays pending for the next display refresh.
- **Close.** The window server can keep the last frame of a closed window. On the virtual GPU of a CI runner, it keeps that `IOSurface` after the release of the window. Thus, after the close, `Window::close` presents a transparent frame of one pixel, which does not show. The window also releases its textures. Its display link pauses when it fires. When the window shows again, it draws its first frame as at startup: when the window is visible, inside the Core Animation transaction. The window server can report a window with only a blank frame as occluded, and then the display link does not fire. Thus, `show` draws that frame at once.
- **Drop.** The drop of a `Window` removes its surface: the layer, the textures, and the display link. It also removes the frames that wait for a presented handler, because that handler can fail to run. The completed handler always runs, and it removes its frame. `Application::metal_usage` and `Window::metal_usage` count the surfaces, the textures, the presented pixels, and the frames without a window.
- **Drawable size.** While the window size changes from frame to frame, `drawableSize` keeps the largest size. The frame and group textures follow the same rule. Each render pass sets its viewport to the frame size. Thus, the frame fills the top-left part of each texture. `contentsGravity` puts that part at the top-left corner of the layer, and `masksToBounds` hides the rest. AppKit flips the layer of the flipped content view, so the gravity is `kCAGravityBottomLeft` when `contentsAreFlipped` is YES.
- **Scissor.** Each draw sets the scissor rectangle of its axis-aligned clip in whole pixels. The plan keeps the rectangle inside the target, as `setScissorRect:` requires.
- **Display link.** `displayLinkWithTarget:selector:` makes the link of the content view. The link sends `choscoruiDisplayLinkFired:` on the main thread. The link retains the view, so the drop of a `Window` invalidates its link.
- **Handler blocks.** The presented handler and the completed handler are global blocks with no captures. Metal calls them on its own threads. They find the frame by the object address in a map behind a `Mutex`, and they change only atomics and that frame's paint state. The completed handler records the paint at the `GPUEndTime` of the command buffer. When the command buffer failed, it sets a process-wide flag, so `gpu_painting` reports unsupported. The presented handler records the `presentedTime` of the drawable for `Window::last_display`. Sometimes the presented handler of a finished frame does not run. When a visible window waits 200 ms for it, the window removes the frame from the map and draws it again. A handler that runs later finds no frame and records no paint.
- **Panics.** The display link callback is a method of the content view class, so its trampoline catches a panic. The blocks catch a panic and report it as "the Metal presented handler" or "the Metal completed handler". The work function of `dispatch_async_f` catches a panic, and then the first window creates the objects.
- **Tests.** The Metal parity smoke test compares each frame with the software renderer, and text with Core Graphics captures. The window, app, and lifetime smoke tests also paint with Metal. The lifetime smoke test checks the Metal usage of closed and dropped windows.

### Images and ImageIO

- **Cache.** `cg_image` copies RGBA pixels into a premultiplied +1 `CGImage`. A thread-local cache keyed by `Image::id` holds up to 256 images. The cache clears when the last window drops.
- **Decode.** `decode_image` decodes frame 0 with `CGImageSource` and draws it into an sRGB bitmap. Thus, the decoder converts the embedded color profile. `decode_image` rejects images larger than 64 Mpx.
- **Encode.** `encode_png` writes into an owned `CFData` and copies the bytes before the release.
- **Threads.** Decode and encode work on any thread and create their own autorelease pool. Paint with cached images occurs on the main thread only.

### Cursors, capture, and occlusion

- **Cursors.** `resetCursorRects` adds one cursor rectangle for the view bounds. The diagonal resize shapes come from `frameResizeCursorFromPosition:inDirections:` with the top left or top right corner. The crate rejects a custom cursor larger than 256 points, or with a hotspot outside the image.
- **Capture.** With Metal, `Window::capture` draws the display list into a texture and copies it through a shared buffer into an owned sRGB `Image`. With Core Graphics, it uses `cacheDisplayInRect:toBitmapImageRep:` and copies the result. `Window::capture_at_scale` draws through Metal in the same way, or through `MacBitmap` with Core Graphics.
- **Occlusion.** `windowDidChangeOcclusionState:` queues `WindowEvent::Occluded`.
- **Tests.** The smoke test checks idle paint counts, damage rectangles, captured pixel colors, and cursor validation.

### Core Text

- **Lines.** `ShapedLine` owns the `CTLine` that it uses for metrics, caret offsets, hit tests, and glyph runs. Rust RAII releases each Core Text object.
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

### Process usage and the counting allocator

- **Calls.** `process_usage` calls `getrusage` and `proc_pid_rusage` with the `RUSAGE_INFO_V2` flavor. It passes `rusage` and `rusage_info_v2` structs that `ffi_struct!` declares, so the SDK header check checks their layouts.
- **Allocator.** `CountingAllocator` in `ffi/alloc.rs` implements `GlobalAlloc`. Each method adds to an atomic count and forwards its arguments to `System` without change.
- **Threads.** Any thread can call these functions. The count is an atomic value, and the calls keep no state.
- **Tests.** The doc tests read the usage and the count. `lifetime_smoke` reads the footprint, and the bench reads the count in each scroll frame.

### Accessibility

- **Announcements.** `Window::announce` builds an autoreleased `NSDictionary` in a local pool. It posts `NSAccessibilityAnnouncementRequestedNotification` through `NSAccessibilityPostNotificationWithUserInfo`.
- **Collections.** Element updates set `accessibilityRowCount`, `accessibilityColumnCount`, and `accessibilityIndex` with `NSInteger` values, saturated from `usize`.
- **Ranges.** For a node with a `SemanticRange`, element updates set `accessibilityValue`, `accessibilityMinValue`, and `accessibilityMaxValue` to autoreleased `NSNumber` values from `numberWithDouble:`.
- **Tests.** `window_smoke` reads the row count, the index, and the text area role back from the native elements.

The [`choscorui-macos` README](../crates/choscorui-macos/README.md) describes the full accessibility bridge.

### Window placement and context menus

- **Placement.** `window_state.rs` reads `frame`, `isZoomed`, and the full-screen bit of `styleMask`. Global coordinates flip against the height of the first screen in `+[NSScreen screens]`.
- **Context menus.** `ContextMenu` owns a +1 `NSMenu` and a +1 target object. Only the menu retains its items.
- **Tracking.** `Window::show_context_menu` calls `popUpMenuPositioningItem:atLocation:inView:`. The menu and its target stay alive during this synchronous call.
- **Tests.** `window_smoke` sends geometry through `set_geometry` and `geometry`. It also selects context-menu items through the native target and action.

### Main menu

- **Submenus.** `MainMenu` builds a nested `NSMenu` for each `MenuEntry::Submenu`. Each menu turns off `autoenablesItems`, so the enabled state comes only from the model.
- **Key equivalents.** `MainMenu::perform_key_equivalent` makes an autoreleased key-down `NSEvent` with `keyEventWithType:location:modifierFlags:timestamp:windowNumber:context:characters:charactersIgnoringModifiers:isARepeat:keyCode:` in a local pool. It sends `performKeyEquivalent:` to the root menu while the event is alive.
- **Actions.** The facade runs each chosen item's Rust action after the AppKit callback returns. It catches a panic of the action with `catch_unwind` and counts it in `AppContext::menu_panic_count`.
- **Tests.** `app_smoke` chooses items through the native target and action, presses a key equivalent, and checks that a panicking action stays usable.

### Desktop integration

- **Delegate.** `desktop.rs` registers `ChoscoruiV2AppDelegate`. The process creates one instance and never releases it, because `NSApplication` and `UNUserNotificationCenter` hold their delegates weakly.
- **Launch.** The crate installs the delegate before `finishLaunching`, so the application receives open-document events at launch.
- **Callbacks.** Callbacks copy URL paths and strings into Rust and queue `AppEvent` values behind a `Mutex`. `NativeServices::take_events` empties the queue on each turn of the `choscorui` loop.
- **Blocks.** The delegate calls each system completion block exactly one time, through the `invoke` pointer of the block ABI.
- **Authorization block.** The authorization callback is a global block with no captures. It writes an `AtomicU8`, so any queue can call it.
- **Delegate object.** A `ProcessObject` keeps the delegate. It lends the delegate only on the main thread.
- **Status item.** `StatusItem` retains its `NSStatusItem` and owns its `ContextMenu`. `Drop` clears the menu, calls `removeStatusItem:`, and then releases the item.
- **Notifications.** UserNotifications calls need a bundle identifier, because the framework raises an Objective-C exception without one. Without a bundle, the call returns an error.
- **Tests.** `window_smoke` drives the open-URLs and reopen methods, the status item, URL handler lookup, and the notification error for an application without a bundle.

## Smoke tests

The native smoke tests create real windows, so they run only when `CHOSCORUI_RUN_MACOS_SMOKE=1` is set. [CONTRIBUTING.md](../CONTRIBUTING.md) lists the commands.

The tests check window construction, title, visibility, resize, scale, and close events. They also check event isolation between two windows, posted work, timers, input translation, scroll units, `drawRect:` calls, panic containment, and loop stop.
