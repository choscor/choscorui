# Roadmap status (macOS delivery)

Tracks every [roadmap](ROADMAP.md) item for the macOS-only delivery. Windows (phase 10) and Linux (phase 11) are out of this delivery's scope. `Done` cites the code or test that proves it. `Manual` names an automated proxy plus the hands-on check a person must still perform on hardware.

## Phase 0 — contract, support matrix, benchmarks

| Item | Status | Evidence |
| --- | --- | --- |
| Targets, architectures, MSRV, window types, sandbox scope | Done | [CONTRACT.md](CONTRACT.md) support target |
| API sketch for Application/Window/Node/Event/Theme/Control/Style | Done | [CONTRACT.md](CONTRACT.md) API direction; `crates/choscorui/src/lib.rs` |
| Notifications, commands, timers, posted work, models, resources, localization | Done | `core::notify` (`NotificationQueue`, `WorkQueue`), `core::model`, `core::resource`, `core::locale`; tests `notify.rs`, `model.rs`, `resource.rs`, `locale.rs` |
| Qt-adjacent scope classification | Done | [SUPPORT_MATRIX.md](SUPPORT_MATRIX.md) |
| Every Qt inventory family classified | Done | [SUPPORT_MATRIX.md](SUPPORT_MATRIX.md) |
| Desktop contracts and platform limits | Done | [SUPPORT_MATRIX.md](SUPPORT_MATRIX.md) |
| Reference scenarios | Done | [BENCHMARKS.md](BENCHMARKS.md) |
| Baseline measurements on named machine | Open | |
| Dependency rule | Done | [CONTRACT.md](CONTRACT.md) revision 1, [LICENSES.md](LICENSES.md) |

## Phase 1 — macOS process, window, event loop

| Item | Status | Evidence |
| --- | --- | --- |
| Minimal AppKit/Objective-C surface | Done | `choscorui-macos` hand-written FFI; [FFI.md](FFI.md) |
| Main-thread start/stop; create, show, resize, close, reopen | Done | `window_smoke` (opt-in native test) |
| Owned pointer, key, modifier, focus, size, scale events; worker wake-up | Done | `window_smoke`; `Poster` |
| Timers, cancellation, posted-work ordering | Done | `Application::schedule_once`/`schedule_repeating`; core `WorkQueue` (`notify.rs`) |
| Panic containment at every callback | Done | `catch_unwind` in each `extern "C"` callback; `window_smoke` panic case |
| Teardown releases native resources after final callback | Done | `Drop for Window`/`Application`; [FFI.md](FFI.md) |
| Gate: scale changes across displays | Manual | Proxy: `ScaleChanged` event and `backing_scale_factor` in `window_smoke`. Manual: drag the gallery between a 1x and a 2x display. |

## Phase 2 — geometry, tree, routing

| Item | Status | Evidence |
| --- | --- | --- |
| Logical units, rects, transforms, clip bounds, stable IDs | Done | `geometry.rs`, `input.rs` (`transforms_compose_invert_and_bound_rectangles`), `tree.rs` |
| Ownership, attach/detach, dirty flags | Done | `tree.rs`, `state.rs`, `tree_placement.rs` |
| Hit testing, capture, enter/leave, wheel, focus, cancellation | Done | `events.rs`, `focus.rs`, `input.rs` (`HoverTracker`) |
| Device capabilities, gestures, pen, drag-and-drop | Done (core) | `InputCapabilities`, `negotiate_drag`, `DropRouter` in `input.rs`; native macOS drag-and-drop lands in phase 6 |
| Propagation, re-entrancy, notification ordering | Done | `events.rs` (`nested_event_completes_before_outer_dispatch_resumes`), `notify.rs` |
| Deterministic, platform-independent state | Done | all of the above run without a platform crate |

## Phase 3 — layout engine

| Item | Status | Evidence |
| --- | --- | --- |
| Measure/arrange with constraints, intrinsic, min/max, padding, baseline | Done | `LayoutEngine`; `layout_engine.rs` |
| Stack, flexible sizing, spacing, alignment, scroll viewport | Done | `layout_engine.rs`, `scroll.rs` |
| Grid and form with spans and label alignment; splitter geometry | Done | `grid.rs`, `form.rs`, `layout_engine.rs` (`form_rows_align_baselines…`) |
| RTL inheritance and mirroring; locale invalidation | Done | `rtl_mirrors_rows_and_grids_and_is_inherited`, `Localizer` direction flip |
| Cycles and unsatisfiable constraints fail predictably | Done | strict mode in debug, overflow in `LayoutStats`; re-entrancy prevented by borrows |
| Measurement cache with correct invalidation | Done | `dynamic_type_scaling_remeasures_only_after_invalidation` |

## Phase 4 — display list and macOS painter

| Item | Status | Evidence |
| --- | --- | --- |
| Renderer-neutral commands: rects, rounded paths, borders, opacity, clip, images, text | Done | `DisplayCommand`; `display.rs`, `vector.rs` |
| Paths, gradients, SVG import, image formats, effects, export, cursors | Done | `core::path`, `core::svg` (subset, explicit errors), `Shadow`; macOS `decode_image`/`encode_png`, `MacBitmap::encode_png`, `Window::capture`, `Window::set_cursor` |
| Asset contract for icons, images, fonts, translations | Done | `ResourceBundle` (scale variants, typed missing-asset errors); `resource.rs` |
| macOS painter on Core Graphics | Done | `paint.rs`; `bitmap.rs` |
| Color space, alpha, decoding, resize, occlusion, scale | Done | sRGB-tagged drawing; `WindowEvent::Occluded`, `Window::is_occluded`; device resources are CPU-side `CGImage`s, so there is no device loss to recover from |
| Damage regions and frame scheduling | Done | `damage()`; `set_display_list` uses `setNeedsDisplayInRect:`; `window_smoke` checks `last_damage` |
| Gate: golden fixtures at 1x and 2x | Done | `crates/choscorui/tests/render_parity.rs` (software vs Core Graphics), `choscorui-render` `raster.rs` |
| Gate: on-screen check and idle window stops drawing | Done | `window_smoke` `check_surface` (captured pixels, unchanged paint count while idle) |
