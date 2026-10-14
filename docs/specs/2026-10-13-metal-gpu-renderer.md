# Metal GPU renderer for 120 Hz frames

- Status: Ready for implementation
- Date: 2026-10-13
- Source: Maintainer brainstorm session on 2026-10-13. It asked how wgpu, egui, and similar toolkits use the GPU, and to apply GPU acceleration to ChoscorUI for performance. Evidence comes from `docs/architecture.md`, `docs/performance.md`, `docs/platform-macos.md`, `crates/choscorui-core/src/display.rs`, `crates/choscorui-core/src/capabilities.rs`, `crates/choscorui-macos/src/{surface,paint,backdrop,lib}.rs`, `crates/choscorui/tests/render_parity.rs`, `CONTRIBUTING.md`, and the external sources listed under "Research summary". The gallery size cap is shared with [2026-10-13-portable-declarative-app-and-api-docs.md](2026-10-13-portable-declarative-app-and-api-docs.md); the owner settled this after a cross-spec review on 2026-10-13.

## Outcome and current state

**Beneficiary.** Application users on ProMotion and other 120 Hz displays, and application authors who need smooth scrolling, resizing, view updates, and theme changes. Today ChoscorUI meets 60 Hz budgets. After this change, the main frame scenarios present within one 120 Hz frame (8.3 ms p95) on the reference machine.

**Current state (measured 2026-10-13).**

- `Window::set_display_list` computes `damage` between the old and the new `DisplayList` and calls `setNeedsDisplayInRect:` on the content view (`crates/choscorui-macos/src/surface.rs`). AppKit then calls `drawRect:`, and `paint_commands` replays the list through Core Graphics and Core Text on the CPU (`crates/choscorui-macos/src/paint.rs`).
- `BackdropBlur` reads back bitmap rows from the `CGContext`, blurs them with `blur_pixels` on the CPU, and draws the result again (`crates/choscorui-macos/src/backdrop.rs`).
- `DisplayCommand` has these variants: `FillRect`, `StrokeRect`, `FillRoundedRect`, `StrokeRoundedRect`, `FillPath`, `StrokePath` (with `Paint` = solid, linear, or radial gradient, `FillRule`, and `StrokeStyle` with caps, joins, and dashes), `DrawImage` (with `ImageSampling`), `DrawShadow`, `DrawShadowCorners`, `DrawText`, `DrawGlyphs`, `BackdropBlur`, `BackdropBlurCorners`, `PushClip`, `PushClipPath`, `PopClip`, `PushOpacity`, `PopOpacity`, `PushTransform`, `PopTransform`.
- `BackendCapabilities::gpu_painting` exists and is always `Unsupported` (`crates/choscorui-macos/src/desktop.rs:542`). `docs/performance.md` states: "Core Graphics paints on the CPU, so there is no GPU device to lose and no software fallback."
- Bench timing: each `present_ms` sample ends when `drawRect:` completes (`Window::last_paint`, `PaintTiming` in `crates/choscorui-macos/src/lib.rs`).
- Baselines on the reference M4 (`docs/performance.md`): `scroll.present_ms.p95` 14.8 ms, `scroll.frame_interval_ms.p95` 15.5 ms, `resize.present_ms.p95` 19 ms, `views.present_ms.p95` 6.3–8.0 ms, `theme.present_ms.p95` 2.7–2.9 ms, `document.present_ms.p95` 20.7 ms (`document.shape_ms.p95` 6.8 ms), gallery first frame 134 ms, idle footprint 122 MiB.
- The gallery release binary is about 3,141,900 bytes against a 3 MiB budget. Mach-O growth fails in 16 KiB page steps.
- Minimum OS is macOS 15, on Apple silicon and Intel (`docs/platform-macos.md`). The backend links AppKit, Foundation, Core Foundation, Core Graphics, Core Text, ImageIO, and UserNotifications only.
- Dependency rule: runtime code uses std, hand-written FFI to system SDKs, and `unicode-segmentation` only. No build scripts, no binding generators, no proc-macro crates (`docs/architecture.md`, "Dependency rule").
- `render_parity.rs` compares `choscorui-render`'s `Raster` with `MacBitmap` at 1x and 2x with `compare(.., 24)` and requires under 0.5 % differing pixels.

**Desired state.** On macOS, each window paints its `DisplayList` with a Metal renderer on a `CAMetalLayer`, through hand-written FFI. Core Graphics remains as the fallback painter and as the offscreen export path. Scroll, resize, views, and theme frames present in ≤ 8.3 ms p95 on the reference machine.

### Research summary

None of these sources measures GPU against CPU rasterization directly. The design follows the toolkit that is closest to ChoscorUI's constraints.

- **wgpu**: WebGPU-style abstraction over Metal, Vulkan, DX12, and GL, with runtime Naga shader translation. No damage or partial-present API. Rejected: it breaks the hand-written-FFI rule and the size budget. Bevy discussion of linked shader-compiler size: <https://github.com/bevyengine/bevy/discussions/2476>.
- **egui / epaint**: CPU tessellation of every shape into triangle meshes, feathered edge anti-aliasing, one glyph atlas with a white texel, full-frame repaint. <https://github.com/emilk/egui/blob/main/crates/epaint/src/tessellator.rs>.
- **Zed GPUI** (the model for this spec): direct Metal on macOS. One instanced pipeline per primitive kind (quads, shadows, underlines, paths, sprites). SDF rounded rects, closed-form erf shadows, Core Text alpha glyphs in an atlas with subpixel-offset variants, precompiled `.metallib`, `CAMetalLayer` with three drawables, full-frame redraw. Frames under 4 ms against an 8.33 ms budget. Lessons: do not block on `presentsWithTransaction` in steady state; use it only for startup and live resize; ProMotion down-clocking needs explicit pacing. <https://zed.dev/blog/videogame> (2023-03-07), <https://zed.dev/blog/120fps> (2024-02-07).
- **Vello**: compute-shader path renderer, now with a "sparse strips" CPU/GPU line as the main one. <https://github.com/linebender/vello>.
- **Skia Graphite / Flutter Impeller**: precompile shaders and build pipelines ahead of time, because runtime compilation caused first-use jank (60 to 6 fps observed with Skia). Flutter's Metal surface tracks damage per drawable. <https://blog.google/chromium/introducing-skia-graphite-chromes/>, <https://docs.flutter.dev/perf/impeller>.
- **Slint / Iced**: GPU renderers redraw full frames. Only their CPU renderers take damage regions. Iced notes glyph differences between linear and sRGB blending. <https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backends_and_renderers/>, <https://docs.iced.rs/iced_tiny_skia/struct.Renderer.html>.
- **Apple**: `CAMetalLayer.presentsWithTransaction` for resize synchronization: <https://developer.apple.com/documentation/quartzcore/cametallayer/presentswithtransaction>. Since Xcode 26 the Metal toolchain (`xcrun metal`, `metallib`) is a separate download (`xcodebuild -downloadComponent MetalToolchain`).

## Decisions and requirements

### Decision ledger

| ID | Decision | Status | Source |
| --- | --- | --- | --- |
| Q1 | Goal: 120 Hz frames | settled | User answer |
| Q2 | Approach: direct Metal renderer, GPUI-style | settled | User answer |
| Q3 | Keep the dependency rule; no wgpu, objc2, or metal crates | settled | `docs/architecture.md` and research; user chose Q2 within it |
| Q4 | Shaders ship as a checked-in precompiled `.metallib` | settled | User answer |
| Q5 | Text: grayscale Core Text glyph atlas with subpixel positioning | settled | Follows from Q6 and Q3; macOS has had LCD anti-aliasing off by default since 10.14 |
| Q6 | Full command coverage, Metal on by default, Core Graphics fallback | settled | User answer |
| Q7 | Full-frame redraw when damage exists | settled | User answer |
| Q8 | Gallery size budget: raise to measured plus one 16 KiB page, capped at 3.25 MiB. The cap is shared with the portable declarative app spec | settled | User answer; shared cap from the cross-spec review |
| Q9 | Rollout: replace Core Graphics as the default window painter in one change | settled | Follows from Q6 |
| Q10 | 8.3 ms p95 for scroll, resize, views, theme; document stays ≤ 33 ms | settled | User answer |
| Q11 | 2026-10-14: the 8.3 ms budgets measure frame cost to the GPU end time; the bench reports the on-display latency (`presentedTime`) without a budget, and display sync stays on | settled | Maintainer decision |
| Q12 | 2026-10-14: capture re-renders the current display list offscreen on Metal, because `framebufferOnly` is YES | settled | Maintainer decision |
| D1 | Path rasterization technique (CPU flattening plus stencil-then-cover, or tessellation) | deferred | Minor: the implementer chooses; parity tests bound the result |
| D2 | Atlas packing, instance-buffer layout, number of pipelines | deferred | Minor: internal; bench budgets and parity bound the result |

### R1. Metal renderer in `choscorui-macos`

- The content view becomes layer-hosting with a `CAMetalLayer`: `device` from `MTLCreateSystemDefaultDevice`, `pixelFormat` BGRA8Unorm with the window's sRGB color space, `contentsScale` equal to the backing scale, `drawableSize` from the view's backing bounds, `framebufferOnly` YES, and `maximumDrawableCount` 3.
- One `MTLDevice` and one `MTLCommandQueue` serve all windows of the `Application`. Pipelines are created once, from the embedded library, before the first frame of the first window.
- Each frame takes `nextDrawable`, encodes the whole display list into one render pass (clear load action), then `presentDrawable:` and `commit`.
- Every `DisplayCommand` variant paints on Metal:
  - Rects, rounded rects, and their strokes: instanced SDF quads with per-corner radii and analytic anti-aliasing.
  - `DrawShadow` and `DrawShadowCorners`: analytic Gaussian (erf) shadows in a shader, matching the current Core Graphics look within the parity tolerance.
  - `FillPath` and `StrokePath`: both fill rules, all `Paint` kinds (solid, linear, radial), caps, joins, and dashes. Path flattening that does not need an OS is platform-independent. Put it in `choscorui-core` if the implementer flattens on the CPU, per the architecture rule.
  - `DrawImage`: textures cached by image identity, with `ImageSampling` mapped to the sampler filter. Evict them with the same rule as today's image cache (`clear_image_cache`).
  - `DrawText` and `DrawGlyphs`: Core Text shapes and rasterizes alpha-only glyphs into an atlas texture. Glyphs are keyed by font, glyph, size, and subpixel x offset. Text blends in the same color space as Core Graphics so parity holds.
  - `BackdropBlur` and `BackdropBlurCorners`: copy the region under the rect to an offscreen texture, blur it in a shader with the same sigma rule as `blur_pixels`, and draw it back clipped to the rounded rect. Inside an opacity group the command reads the surface beneath the group, as it does today.
  - `PushClip`: scissor rect when axis-aligned in device space, otherwise a mask. `PushClipPath`: stencil or mask. `PushOpacity`: offscreen group texture composited with the opacity. `PushTransform`: per-instance transform.
- Paint, layout, and input work must not scale with the whole model. Encoding cost follows the display list, which is already culled to the visible area.

### R2. Frame pacing, present, and resize

- A frame is drawn only when `set_display_list` reports damage, so `idle.paints` stays 0. The damage region decides whether to draw, not what to draw (Q7).
- Pace frames with `NSView.displayLink` (macOS 14+, available on the macOS 15 minimum), so ProMotion runs at 120 Hz while frames are pending. A display-link callback that has no pending frame does nothing, and the link pauses when the window has no pending frame, so idle wakeups stay within budget.
- During startup and live resize, set `presentsWithTransaction` YES. Commit, wait until scheduled, then present inside the Core Animation transaction, so content and window frame stay in sync. In steady state it is NO, and the main thread never waits on GPU completion.
- `resize.missed_configure` stays 0.

### R3. Shaders as a checked-in `.metallib` (Q4)

- The Metal Shading Language source lives in `crates/choscorui-macos/src/shaders/` (for example `ui.metal`), with the compiled `ui.metallib` next to it. The crate embeds the library with `include_bytes!` and loads it with `newLibraryWithData:error:` through a `dispatch_data` value.
- A script, for example `scripts/macos/build-shaders.sh`, regenerates the `.metallib` with `xcrun metal` and `xcrun metallib`. Target macOS 15 and the Metal language version that it supports, so one library serves Apple silicon and Intel.
- A `cargo test -p choscorui-macos` test fails when the `.metal` source changed without a regenerated library. For example, the script writes a stamp file with a stable hash of the source, and the test recomputes the hash with std only. The test must not need the Metal toolchain.
- No build script is added. `CONTRIBUTING.md` documents the Metal toolchain download as needed only to change shaders.

### R4. FFI and safety

- Metal.framework and QuartzCore.framework join the linked system frameworks. Every new class, selector, C function (`MTLCreateSystemDefaultDevice`, `dispatch_data_create`, and so on), struct (`MTLViewport`, `MTLScissorRect`, `MTLClearColor`, `MTLRegion`, and so on), constant, and callback is declared through the `ffi` macros in `crates/choscorui-macos/src/ffi/`. Each `unsafe` block is one raw call with a `SAFETY` comment.
- Metal objects are held in `Retained`. The `cargo test -p choscorui-macos` SDK header and runtime checks cover every new binding, including Metal protocols, deprecations included. Fix a binding when it fails; never weaken the check.
- The display-link callback and any `addCompletedHandler:` or `addPresentedHandler:` block catch panics at the boundary. Record each boundary in `docs/ffi.md`.

### R5. Fallback, capability, and public surface (Q6)

- Metal is the default window painter. The window uses the existing Core Graphics `drawRect:` path when any of these is true:
  1. `MTLCreateSystemDefaultDevice` returns nil.
  2. Library or pipeline creation fails.
  3. The environment variable `CHOSCORUI_PAINTER=core-graphics` is set. This is the diagnostic and test switch.
- When a command buffer fails or the device is removed (an external GPU on Intel), the window switches to Core Graphics for the rest of its life and schedules a full repaint. No panic, and no lost frame beyond the failed one.
- Add `Window::painter() -> Painter`, where `Painter` is a `#[non_exhaustive]` enum with `Metal` and `CoreGraphics`. It reports the active painter of that window. Expose it through `choscorui::platform` as for the other window types.
- `capabilities().gpu_painting` is `Supported` when a Metal device and pipelines exist and the override is absent. Otherwise it is `Unsupported`. Branch on it, not on the OS.
- `Window::capture` re-renders the current display list offscreen when the painter is Metal, because `framebufferOnly` is YES and the drawable cannot be read (Q12). It returns straight-alpha sRGB pixels as today. When the painter is Core Graphics, it keeps today's behavior.
- `MacBitmap` and `Scene::export` keep Core Graphics. Offscreen export is out of scope.
- `Window::last_paint` and `PaintTiming` end each sample at the GPU end time of the frame (`addCompletedHandler:` / `GPUEndTime`) for Metal, and at `drawRect:` completion for Core Graphics (Q11). The presented time (`addPresentedHandler:` / `presentedTime`) goes to `Window::last_display`. The bench keeps using `Window::last_paint`.

### R6. Budgets and docs (Q8, Q10)

- `bench/budgets.txt`: `scroll.present_ms.p95`, `scroll.frame_interval_ms.p95`, `resize.present_ms.p95`, `views.present_ms.p95`, and `theme.present_ms.p95` become `<= 8.3`. Other budgets keep their limits: `document.present_ms.p95` ≤ 33, startup gallery ≤ 250 ms, startup stress ≤ 400 ms, `idle.paints` ≤ 0, `idle.cpu_percent` ≤ 1, `idle.wakeups_per_sec` ≤ 20, `idle.footprint_mib.after` ≤ 200, and the correctness counters at 0.
- `size.choscorui-gallery_mib`: measure the release gallery after the change, on top of the portable declarative app change if that landed first. Set the budget to the measured size plus one 16 KiB page, with a reason users can see ("GPU rendering for 120 Hz frames"). It must not exceed 3.25 MiB, which is the cap for both specs together. When it would exceed it, shrink code instead (shared generic instances, function pointers instead of closures; see `docs/performance.md`).
- Run the bench on the reference machine and record new baselines in `docs/performance.md`.
- Update in the same change:
  - `docs/architecture.md`: the frame pipeline (Metal present, Core Graphics fallback) and the macOS framework list.
  - `docs/performance.md`: budgets, baselines, the present-time definition, and the profiling-findings row for presentation. Remove the "no GPU device to lose" sentence and describe the fallback.
  - `docs/platform-macos.md`: frameworks, the `gpu_painting` value, `CHOSCORUI_PAINTER`, and `Window::painter`.
  - `docs/ffi.md`: new boundaries and callbacks.
  - `CONTRIBUTING.md`: the shader rebuild procedure and the Metal toolchain note.
  - `docs/licenses.md` only if a license entry changes. No new crates are added, so none is expected.

### Non-goals

- wgpu, Vulkan, DX12, or any non-macOS GPU backend. Windows and Linux stay unsupported.
- Partial redraw on Metal (per-drawable damage or a retained texture). Q7 chose full-frame redraw.
- Faster text shaping, and an 8.3 ms `document` budget.
- GPU offscreen export (`MacBitmap`, `Scene::export`).
- LCD subpixel text anti-aliasing.
- New `DisplayCommand` variants, or changes to the display-list semantics, controls, compose, or golden files.

### Edge cases and failure behavior

- Window moves between displays with a different scale or refresh rate: update `contentsScale` and `drawableSize`, and let the view's display link follow the new display.
- Occluded or minimized window: draw no frames. `Window::is_occluded` behavior is unchanged.
- `nextDrawable` returns nil or times out: skip the frame, keep the damage pending, and try on the next display-link tick. Do not block the main thread past one frame.
- Display lists that fail validation behave as today; this change adds no new error paths to `DisplayList`.
- Glyph or image atlas full: evict least-recently-used entries or grow up to a fixed limit. Never fail the frame.

## Acceptance criteria and public test seams

| Criterion | Observable seam |
| --- | --- |
| Metal paints every `DisplayCommand` with the same look as the reference renderer | New smoke test (with `CHOSCORUI_RUN_MACOS_SMOKE=1`) renders each `render_parity.rs` fixture in a window at 1x and 2x, reads it with `Window::capture`, and checks `compare(.., 24)` against `Raster::render` with under 0.5 % differing pixels. Text fixtures compare against the same list captured with `CHOSCORUI_PAINTER=core-graphics`, because `Raster` does not draw text. |
| Backdrop blur on Metal softens what is beneath it | The `backdrop_blur_softens_what_is_beneath` fixture passes through the same capture seam |
| Metal is the default and is reported | `Window::painter()` returns `Painter::Metal`, and `choscorui::capabilities().gpu_painting.is_supported()` is true, on a Mac with a Metal device |
| Fallback works | With `CHOSCORUI_PAINTER=core-graphics`, `Window::painter()` returns `Painter::CoreGraphics`, `gpu_painting` is unsupported, and the existing window and app smoke tests pass |
| 120 Hz frames | `target/release/choscorui-bench --check bench/budgets.txt` passes on the reference machine, with the five budgets at `<= 8.3` |
| No idle cost | The same bench run passes `idle.paints <= 0`, `idle.cpu_percent <= 1`, `idle.wakeups_per_sec <= 20`, and `idle.footprint_mib.after <= 200` |
| Startup and size stay in bounds | The bench passes the startup budgets and the new `size.choscorui-gallery_mib` budget, which is ≤ 3.25 |
| Resize stays in sync | `resize.missed_configure` and `resize.missed_present` are 0, and the lifetime smoke test passes (window churn, resize and theme storm, footprint floor) |
| Shader source and library cannot drift | `cargo test -p choscorui-macos` fails after a `.metal` edit without a rebuild, and passes after the script runs |
| FFI rules hold | `cargo test -p choscorui-macos` SDK and runtime checks pass for all Metal and QuartzCore bindings. `cargo clippy` with `-D warnings` passes with the crate's unsafe lints |
| Docs are current | `scripts/check-docs.sh` passes |

## Constraints and risks

- **Checks.** Run the CI `check` job and `native` job commands from `CONTRIBUTING.md` ("Before submitting a maintainer change"), the parity smoke test, and the bench with `--check`. Run AppKit window tests one at a time.
- **Human-only verification.** Automation does not verify Intel hardware, external GPUs and device removal, external-display tearing, VoiceOver, or budgets off the reference machine. Do not claim them. Name them in the PR as unverified. CI runs on `macos-15-intel` but cannot judge visual quality there.
- **Accessibility.** The `NSAccessibilityElement` tree is separate from painting. Keep `Window::set_accessibility` and its behavior unchanged.
- **Risk: ProMotion pacing.** Zed saw dropped frames from display down-clocking while frames took under 4 ms. Measure `scroll.frame_interval_ms` and adjust pacing, not the budget.
- **Risk: `presentsWithTransaction`.** Blocking on it in steady state stalls the main thread (Zed). Use it only during startup and live resize.
- **Risk: memory.** Three full-window drawables at 2x scale and 1600×1000 points are about 77 MB, plus atlases and group textures. `idle.footprint_mib.after` must stay ≤ 200 MiB (122 MiB baseline). If needed, use `maximumDrawableCount` 2 at rest or release group textures when idle.
- **Risk: startup.** Device, library, and pipeline creation add launch time. The precompiled library avoids compile time. The gallery has about 116 ms of headroom.
- **Risk: size.** The renderer and `.metallib` are an estimate of order 100 KB, which is not measured. The 3.25 MiB cap is firm and is shared with the portable declarative app spec, so the two changes together must fit under it.
- **Risk: text parity.** Glyph blending in linear rather than sRGB space changes the look (Iced). Match the Core Graphics blending.
- **Risk: Metal toolchain.** Since Xcode 26 it is a separate download. Only shader changes need it. CI never compiles shaders.
- **Deferred minor details (D1, D2).** The path technique, atlas packing, instance layout, and pipeline count are the implementer's choice, bounded by the parity and bench criteria.
- **Compatibility.** `Painter` and `Window::painter` are additive. `BackendCapabilities` is `#[non_exhaustive]`, and `gpu_painting` already exists. Applications see no API break. Golden files in `crates/choscorui/tests/golden/` pin display lists, not pixels, so they do not change.

Read this whole spec and inspect the current workspace before implementing.
