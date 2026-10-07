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

## Phase 5 — text

| Item | Status | Evidence |
| --- | --- | --- |
| One text-layout contract: fallback, shaping, line breaking, bidi, glyph positions, hit testing | Done | `core::text_layout` (`TextStyle`, `TextShaper`, `TextLayout`, `GlyphRun`); `core/tests/text_layout.rs` |
| Core Text backend; measurement and drawing share shaped runs | Done | `CoreTextShaper`; `DisplayCommand::DrawGlyphs` paints the measured runs; `macos/tests/text_layout.rs` |
| Bounded font/layout cache with invalidation on font, size, locale, scale | Done | `TextLayoutCache` (LRU by entries and bytes; style and locale in the key; `invalidate_family`, `set_scale`); 64-entry painter font cache |
| Vietnamese, CJK, emoji, combining marks, RTL, mixed-script fixtures | Done | `every_fixture_has_consistent_caret_geometry`, `painted_glyphs_stay_inside_measured_lines_at_one_and_two_times` |
| Gate: selection, carets, wrapping, and glyphs agree in the gallery | Done (automated) + Manual | The gallery document uses `CoreTextShaper` + `TextLayoutCache` for hit testing, selection, carets, and drawing; fixture tests prove agreement. Manual: select across the Hebrew line and the wrapped paragraph in the gallery and confirm the highlight matches the glyphs. |

## Phase 6 — input, focus, clipboard, basic controls

| Item | Status | Evidence |
| --- | --- | --- |
| Semantic actions independent of pointer events | Done | `SemanticAction` inputs on every control; `controls/tests/*` |
| Commands with enabled/checked state, scoped shortcuts, conflict rules | Done | `CommandRegistry`; `controls/tests/commands.rs`, `shortcuts.rs`; macOS `MainMenu` shares command state |
| Button, label, checkbox, radio, slider, separator, scroll container with all states | Done | `Button`, `Label`, `Checkbox`, `RadioGroup`, `Slider`, `Separator`, `ScrollArea`; `controls/tests/navigation.rs` |
| Keyboard order, tab/shift-tab, mnemonics, default actions, focus indicators | Done | core focus order (`focus.rs`), `MnemonicTable`, `DialogKeys`, focus rings in each `*Style::paint`; layout-aware `Key::Character` |
| Clipboard and drag payloads through the platform interface | Done | `Clipboard::write`/`read`/`formats`; `Window::accept_drops`, `Window::begin_drag`; `clipboard_smoke`, `window_smoke::check_drop_target` |
| Payload formats, copy/move/link negotiation, cancellation, cross-application behavior (text, files, images) | Done | `negotiate_drag`, `DropRouter`; [FFI.md](FFI.md) pasteboard row |
| Per-control semantic role, state changes, keyboard actions | Done | each control's `semantics()` with role, value, state, and actions |
| Gate: keyboard-only operation, deterministic state tests, capture outside bounds | Done (automated) + Manual | `thumb_drag_keeps_capture_outside_bounds_and_releases`, slider and button capture tests. Manual: drag text from TextEdit onto the gallery text field; drag a file from Finder onto a drop target. |

## Phase 7 — text editing and input methods

| Item | Status | Evidence |
| --- | --- | --- |
| UTF-8 text model with byte, grapheme, and UTF-16 mappings | Done | `TextBuffer` (`byte_to_utf16`, `utf16_to_byte`, grapheme boundaries); `core/tests/text.rs` |
| Caret, selection, word motion, undo/redo, copy/paste, multiline, scroll-to-caret, password masking | Done | `TextField` (single line, password masking), `TextEditor` (multiline, visual-line Home/End, Page Up/Down, preferred column, click/double/triple-click); gallery `reveal_document_caret`; `controls/tests/text_field.rs`, `text_editor.rs` |
| Validation states and commit separate from editing; error announcements | Done | `ValidationState`, `TextFieldSemantics::announcement`; Return commits only when `Valid` |
| Input masks and locale-aware numeric/date contracts | Done | `InputMask`, `NumberInput`, `DateInput`; `controls/tests/input_rules.rs` |
| Undo decision: text-only vs. reusable stack | Done | `core::undo` module docs: `TextBuffer` keeps text history (marked text never enters it); `UndoStack` groups, merging, labels, and save point for application actions; `core/tests/undo.rs` |
| AppKit IME composition and candidate placement; marked vs. committed text | Done | `NSTextInputClient` bridge (`setMarkedText`, `insertText`, `firstRectForCharacterRange`); `TextEditor::marked_range_utf16`, `caret_rect` |
| Composition survives redraws; edits rejected when the focused node is removed | Done | `composition_survives_redraws_commits_on_click_and_undo_skips_marked_text`, `removed_node_rejects_edits_and_read_only_blocks_changes` |
| Gate: Vietnamese, Japanese/Chinese, emoji, dead keys, paste, undo, selection, screen reader editing | Done (automated) + Manual | `window_smoke` drives dead-key (´ → é) and Telex (ê → ệ) composition through the real `NSTextInputClient`; editor tests cover kana → kanji commit, undo, selection, and `SetValue`. Manual: type with macOS Vietnamese Telex and Japanese Romaji input sources in the gallery document; edit the document with VoiceOver. |
