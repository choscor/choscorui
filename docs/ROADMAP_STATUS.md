# Roadmap status (macOS delivery)

Tracks every [roadmap](ROADMAP.md) item for the macOS-only delivery. Windows (phase 10) and Linux (phase 11) are out of this delivery's scope. `Done` cites the code or test that proves it. `Manual` names an automated proxy plus the hands-on check a person must still perform on hardware.

**macOS development is complete (2026-10-12).** Every item is implemented and passes its automated checks. What remains is human verification only. These checks are listed under `Manual` below and in [RELEASING.md](RELEASING.md#5-manual-gates):

- text selection highlight over the Hebrew line and the wrapped paragraph
- dragging from TextEdit and Finder
- Vietnamese Telex and Japanese Romaji input sources
- VoiceOver walks: gallery editing, a button after a theme switch, the accessibility checklist, the stress gallery, and two notes windows
- notification delivery and click, and Finder document open, in a signed bundle
- a notarized archive with real credentials, then a fresh install on a clean account and on Intel hardware
- the budget run on each additional Mac model

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
| Baseline measurements on named machine | Done | [BUDGETS.md](BUDGETS.md) baselines on `Mac16,1` (M4, macOS 26.5) from `examples/bench` |
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
| Gate: scale changes across displays | Done | `display_scale_smoke` (opt-in, `CHOSCORUI_RUN_MACOS_DISPLAYS=1`, needs a 1x and a 2x display) moves a real window across connected screens. Run on 2026-10-12 with the reference M4 built-in Retina panel (2x) and a ViewSonic VX2780 QHD (1x): 1→2→1 each delivered `ScaleChanged` with the new scale, kept the 320×240 logical content size, and repainted. Rendering at both scales is pinned by the phase 4 1x/2x golden fixtures. |

## Phase 2 — geometry, tree, routing

| Item | Status | Evidence |
| --- | --- | --- |
| Logical units, rects, transforms, clip bounds, stable IDs | Done | `geometry.rs`, `input.rs` (`transforms_compose_invert_and_bound_rectangles`), `tree.rs` |
| Ownership, attach/detach, dirty flags | Done | `tree.rs`, `state.rs`, `tree_placement.rs` |
| Hit testing, capture, enter/leave, wheel, focus, cancellation | Done | `events.rs`, `focus.rs`, `input.rs` (`HoverTracker`) |
| Device capabilities, gestures, pen, drag-and-drop | Done | Core: `InputCapabilities`, `negotiate_drag`, and `DropRouter` in `input.rs`. Native macOS drag and drop arrived in phase 6: `Window::accept_drops`, `begin_drag`, and `WindowEvent::Drag`/`DragEnded`. `window_smoke` drives external drags through `simulate_external_drag` (refused, then accepted and dropped), and the gallery accepts dropped text. |
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

## Phase 8 — theme and customization API

| Item | Status | Evidence |
| --- | --- | --- |
| Tokens for color, typography, spacing, radii, borders, elevation, motion; documented resolution of light/dark, contrast, overrides | Done | `DesignTokens` (`font_weight`, `line_height`, `text_style`); [THEMING.md](THEMING.md); `core/tests/theme.rs` |
| Visual recipe/slot API for background, content, icon, indicator, overlay | Done | `ButtonRecipe`, `Slot`, `paint_button`, `PillButtonRecipe`, `ButtonStyle` recipe; `controls/tests/recipe.rs` |
| Instance, scoped, and application themes; hot changes; metric vs. paint invalidation | Done | `ThemeResolver::set_instance`/`set_scope`/`set_application`; `paint_only_and_metric_changes_request_different_work`, `typography_tokens_drive_layout_and_text_style` |
| Animation clock, easing, interruption, reduced motion, node removal, frame scheduling, paint-only invalidation | Done | `AnimationClock`; `core/tests/animation.rs`; system Reduce Motion via `system_accessibility_preferences` (`macos/tests/preferences.rs`) |
| Two structurally distinct gallery themes | Done | Dark is a flat square button with leading rail and label; light is an elevated pill with badge, centered label, and focus ring (`examples/gallery` `display_list`) |
| Native-looking themes documented as emulation; system UI listed | Done | [THEMING.md](THEMING.md) "Native look and system UI" |
| Gate: runtime theme switch replaces button visuals without replacing behavior | Done (automated) + Manual | `swapping_recipes_mid_interaction_keeps_activation_focus_keyboard_and_semantics` swaps recipe and tokens while Space is held. Manual: in the gallery, press Control-Return or click the button, then confirm Tab focus, Space activation, and VoiceOver name are unchanged. |

## Phase 9 — accessibility foundation

| Item | Status | Evidence |
| --- | --- | --- |
| Role, name, value, description, state, bounds, relations, actions per node | Done | `SemanticNode`, `SemanticEntry`; per-control contracts in [ACCESSIBILITY.md](ACCESSIBILITY.md) |
| Stable macOS accessibility tree with focus/value/tree notifications | Done | `AccessibilityProjection` (stable identities, retirement), `Window::set_accessibility`; `macos/tests/accessibility.rs`; selected-text notification for editable text |
| Semantics linked to layout coordinates and action dispatch; virtualized representation | Done | Screen frames from retained bounds; `take_accessibility_actions`; `SemanticCollection` + `set_item_index` → `accessibilityRowCount`/`accessibilityIndex`; `virtualized_collections_report_total_size_and_absolute_item_index`, `window_smoke` |
| High contrast, reduced motion, larger text | Done | `system_accessibility_preferences` → `ThemeResolver::set_accessibility_preferences`; `accessibility_preferences_link_contrast_motion_and_text_scale` |
| Localized names, RTL reading order, live updates, accessible actions | Done | Vietnamese/Arabic labels in tests; `rtl_snapshot_keeps_logical_reading_order_after_mirrored_layout`; `Window::announce` (gallery announces validation errors); press/set value/focus through native elements in `window_smoke`. Menus, dialogs, and virtualized data controls are checked again as phase 12 adds them. |
| Gate: gallery core controls operable with VoiceOver with correct roles, labels, states, focus, values | Done (automated) + Manual | `window_smoke` reads AX role/label/value/selection/row count/index from the real `NSAccessibilityElement`s and performs press, set value, and focus; the gallery exposes button, checkbox, text field, multiline document (AXTextArea), and rows. Manual: the VoiceOver checklist in [ACCESSIBILITY.md](ACCESSIBILITY.md). |

## Phase 12 — advanced controls and large-data behavior

| Item | Status | Evidence |
| --- | --- | --- |
| Model interfaces: stable identity, change notifications, lazy fetch, sort/filter proxies, shared selection | Done | `ItemModel`/`EditableModel`, `SortFilterProxy` (locale collation, diacritic-insensitive filter, recursive tree filter, keyed diffs), `MappedModel`, `ConcatModel`, `SliceModel`, `FileSystemModel` (background listing, keyed refresh); `core/tests/proxy.rs` |
| Virtualized list/tree/table/icon views, stable selection, incremental updates, keyboard navigation, type-ahead | Done | `ItemView` (recycled realized nodes, `ViewLayout::Grid`, disclosure, context menu, `sync_semantics` with collection size and item index); `hundred_thousand_rows_keep_realized_nodes_bounded_while_scrolling` and the other `controls/tests/item_view.rs` tests |
| Header behavior, editor delegates/factories, data-widget mapping, column view, undo-history presentation | Done | `HeaderView` (click/keyboard sort, drag/keyboard resize, `ColumnHeader` semantics), `EditorFactory`/`CellEdit` (locale numbers, dates, bools), `DataMapper` (pending text per item), `ColumnBrowser`, `undo_history`/`undo_steps_to` |
| Combo box, completion, popup/tooltip positioning, popup menus, font chooser | Done | `ComboBox` (editable/popup, prefix/contains completion, inline suffix, keyed highlight across async updates), `place_popup` (flip, clamp, RTL), `PopupStack` (Escape, outside press consumed, focus restoration, owner pruning), `TooltipController` (delay, warm-up grace, interrupt), `PopupMenu` (command-backed, disabled/checked, submenus, type-select), `FontChooser` + `available_font_families`; `controls/tests/popup.rs` |
| Basic-widget inventory: progress (indeterminate), scroll bars, integer/decimal spin, date/time/date-time editors, calendar, dial, command-link and tool buttons, button groups, toolbox, numeric display, focus frame, size grip, stacked/titled groups, key-sequence capture | Done | `ProgressBar::needs_frame` + `paint_indeterminate`, `ScrollBar`, `SpinBox`/`DecimalSpinBox`, `DateTimeEdit`, `Calendar`, `Dial`, `CommandLinkRecipe`, `ToolButton`, `ButtonGroup`, `ToolBox`, `NumericDisplay`, `FocusFrame`, `SizeGrip`, `GroupBox`/`StackedContainer`, `KeySequenceEdit`; `controls/tests/widgets.rs`; contracts in [ACCESSIBILITY.md](ACCESSIBILITY.md) |
| Dialog and utility inventory: open/save/directory, color, font, message, single-value dialogs; platform button ordering; native vs custom-drawn | Done | Native: `FileDialogSpec` (NSOpenPanel/NSSavePanel) and `AlertSpec` (NSAlert) in `choscorui-macos`. Custom-drawn: `ColorPicker` (HSV/RGB/hex, alpha, recents), `FontDialog`, `InputDialog` (text/integer/decimal/choice with locale validation), `MessageDialog`; `order_dialog_buttons` (macOS order, RTL mirror), `dialog_key_button` (Return = default, never destructive; Escape = Cancel), `DialogStack` (modal input confinement, Tab wrap, focus restoration); `controls/tests/dialogs.rs`; split documented in [DIALOGS.md](DIALOGS.md) |
| Persistent settings: per-user storage, read/write errors, defaults, migration/versioning; persistent window geometry; native context menus | Done | `Settings`/`SettingsSchema` (typed values, atomic save, `load_or_recover`, `FutureVersion` refusal, stepwise migrations), `user_settings_path`, `WindowGeometry` (`fit_to_screens` handles unplugged displays and oversized frames), `Window::geometry`/`set_geometry`, `screen_visible_frames`, `ContextMenu` + `Window::show_context_menu`; `core/tests/settings.rs`, `window_smoke`; [SETTINGS.md](SETTINGS.md) |
| Main-window composition: movable toolbars with embedded controls, status bar, dockable panels, keyboard resizing, focus restoration, dock persistence; MDI decision | Done | `compose_main_window` (toolbars stack inward, status bar along the bottom; macOS menu bar is native), `Toolbar` (overflow chevron menu, roving arrow focus that skips disabled commands, RTL mirroring, `save`/`restore` for area, order, and visibility), `StatusBar` (expiring messages with announcements, trailing sections), `DockLayout` (four edge areas of tab groups, floating, close/show at remembered location, focus successor on close, `handle_splitter_key`, `save`/`restore` tolerant of added or removed panels and atomic on error). MDI deferred (multiple native windows instead; see SUPPORT_MATRIX). `controls/tests/shell.rs` |
| Desktop integration: status items, notifications, opening URLs, activation/reopen and file-open events; permission requirements | Done (automated) + Manual | `Application::take_app_events` (`AppEvent::{Activated, Deactivated, Reopen, OpenFiles, OpenUrls, NotificationActivated}`), `StatusItem`, `open_url`/`url_handler`, `request_notification_permission`/`post_notification` (`RequiresAppBundle`, `PermissionDenied`); `window_smoke`; [DESKTOP.md](DESKTOP.md). Manual: real notification delivery and click in a signed bundle, and Finder double-click opening a document. |
| Interactive 2D canvas: zoom/pan, transformed hit testing and selection, clipping, large-scene culling, accessible items; primitive/SVG items, grouping, transforms, multiple views, scene events, spatial index, effects, export; unsupported operations recorded | Done | `Scene` (grid spatial index, item hierarchy, `clip_children`, opacity, change events, `export`), `CanvasView` (zoom about anchor, rubber band, drag/nudge, Tab focus, virtualized item semantics); `core/tests/scene.rs` (100,000 items, rotated hit test, clipping, reparenting), `controls/tests/canvas.rs` (two views on one scene); [CANVAS.md](CANVAS.md) |
| Rich and plain text documents | Done | `RichDocument` (block kinds, span formats, insert/delete/format/toggle, links, plain/Markdown/HTML conversion), `RichLayout` (styled word pieces, wrapping, alignment, list markers, quote and code decorations, hit testing, carets, link hit testing); `core/tests/rich_text.rs`; [RICH_TEXT.md](RICH_TEXT.md) |
| Custom-control extension points for measure, event, paint, and semantics without FFI | Done | `CustomControl` + `ControlHost` (mount/unmount/prune, constrained measure, capture/target/bubble routing, focus notifications, text input, clipped painting, dirty-driven semantics sync, advertised-action dispatch), re-exported by the `choscorui` facade; `controls/tests/custom.rs`; [CUSTOM_CONTROLS.md](CUSTOM_CONTROLS.md) |
| Popup windows and native menus obey platform focus, modality, and dismissal rules | Done | `PopupStack` (Escape, consumed outside press, window deactivation, focus restoration), `DialogStack` (modal input confinement), native `ContextMenu`/`MainMenu` use AppKit tracking; `controls/tests/popup.rs`, `controls/tests/dialogs.rs`, `window_smoke` |
| Validated text, numeric, and date inputs; resizable splitters and dockable panels; keyboard resizing, focus restoration, validation feedback, accessible values/actions | Done | Phase 7 inputs (`NumberInput`, `InputMask`, `DateInput`, `DecimalSpinBox`, `DateTimeEdit`), `Splitter` (keyboard, Slider semantics), `DockLayout` (`handle_splitter_key`, focus successor on close), `InputDialog` validation; `controls/tests/input_rules.rs`, `controls/tests/shell.rs` |
| Locale-aware data presentation and sorting; view state and selection stable across updates and filtering | Done | `SortFilterProxy` with a `Collator` and diacritic-insensitive `TextFilter`, keyed `Selection`, `ItemView::restore_expanded`; `selection_survives_filter_and_sort_and_type_ahead_folds_diacritics` |
| Gate: stress gallery scrolls/searches large data with bounded nodes; restores settings and dock state after restart; validates structured input; zoom/selection on a large canvas; desktop integration | Done (automated) + Manual | `examples/stress` autopilot (`CHOSCORUI_STRESS_AUTOPILOT=1`): 100,000 records searched by folded text, 2,000 PageDowns with realized rows at most the viewport plus overscan, bounded paint and semantics, a 20,000-item canvas with zoomed hit test, rubber band, culling, and virtualized item semantics, structured-input validation, a v1→v2 settings migration, then a full rebuild from the settings file that verifies window geometry, dock, toolbar, search, and zoom, plus status item, URL handler, and notification capability. Manual: VoiceOver walk through the stress gallery's list and canvas. |

## Phase 13 — performance and reliability engineering

| Item | Status | Evidence |
| --- | --- | --- |
| Profile startup, frame scheduling, text shaping, layout invalidation, allocations, CPU paths | Done | `examples/bench` (`choscorui-bench`): presented-frame timing via `Window::last_paint`, counting allocator, CPU and footprint from `getrusage`/`proc_pid_rusage`. Findings and fixes in [BENCHMARKS.md](BENCHMARKS.md#profiling-findings-phase-13): scene paint 16→3 ms, allocation-free `damage`, `Arc` display lists in `drawRect:`, `TextLayoutCache::layout_paragraphs` (keystroke 106→21 ms on 100,000 characters), cached shadow masks (light-theme replay 47→2.4 ms), delta-based `TextBuffer` undo |
| Measured, enforced release budgets with hardware, OS, workload, percentile | Done | [BUDGETS.md](BUDGETS.md), [`bench/budgets.txt`](../bench/budgets.txt); `choscorui-bench --check` exits non-zero on a failed or missing budget |
| Automated leak, resource-lifetime, fuzz/property, and stress checks; resize storms, rapid theme switches, window churn, device loss, worker-thread wakeups | Done | [RELIABILITY.md](RELIABILITY.md): seeded properties in `core/tests/properties.rs` and `controls/tests/properties.rs`, and the native `lifetime_smoke` (window churn, resize/theme storm, 4,000 worker posts, timer churn, accessibility churn, window-count and footprint-floor leak checks). Found and fixed: ambiguous Markdown run serialization, code-span brackets ending links, `NumberInput` precision with `.` in `,` locales, and windows never released after close. Device loss does not apply (CPU Core Graphics). |
| Software fallback only if needed | Done | Not needed: the macOS painter is CPU Core Graphics, so there is no GPU device to lose or fall back from ([BENCHMARKS.md](BENCHMARKS.md#profiling-findings-phase-13)) |
| Gate: measured budgets pass on representative machines; no known crash, leak, or input-loss issue in core scenarios | Done (automated) + Manual | `choscorui-bench --check bench/budgets.txt` passes every budget on the reference M4. Zero missed frames, configure events, or caret mismatches. Zero leaked windows. Footprint floor is stable in `lifetime_smoke`. Manual: repeat the budget run on each additional Mac model before claiming it ([BUDGETS.md](BUDGETS.md#process)). |

## Phase 14 — developer experience and stable API

| Item | Status | Evidence |
| --- | --- | --- |
| Versioning, deprecation policy, feature flags, backend capability queries, error reporting | Done | [VERSIONING.md](VERSIONING.md); `choscorui::capabilities()` returns a `#[non_exhaustive]` `BackendCapabilities` with `Support::{Supported, Unsupported, Conditional}`. Every public error implements `Display` and `std::error::Error`, and wrapping errors return their cause from `source()`. Tests: `crates/choscorui/tests/errors_and_capabilities.rs` |
| Compatibility suite pinning focus, layout, semantics, and theme behavior | Done | `crates/choscorui/tests/compatibility.rs` compares focus order, layout geometry, control semantics, theme token resolution, and display-list output against `crates/choscorui/tests/golden/*.txt`. Re-record intended changes with `CHOSCORUI_BLESS=1`. |
| API guide, customization cookbook, control behavior contract, FFI safety notes, platform-support table | Done | [GUIDE.md](GUIDE.md) (its quick-start snippet compiles), [COOKBOOK.md](COOKBOOK.md) (every recipe is a test in `crates/choscorui/tests/cookbook.rs`), [CONTROLS.md](CONTROLS.md) (pointer, keyboard, focus, disabled, and cancel behavior per control, with the tests that pin it and the inconsistencies to resolve before 1.0), [FFI.md](FFI.md), [SUPPORT_MATRIX.md](SUPPORT_MATRIX.md), and the capability table in [VERSIONING.md](VERSIONING.md) |
| Examples build and run with standard Cargo commands; documented OS SDK prerequisites | Done | README "Requirements" and [GUIDE.md](GUIDE.md#requirements): macOS 15+, Xcode Command Line Tools, Rust 1.88. `cargo run -p choscorui-gallery`, `cargo run -p choscorui-stress`, and `cd samples/notes && cargo run` |
| Resource packaging and localization guide: translatable strings, plural forms, locale formatting, runtime language switch, RTL layout, translation validation, asset lookup failures | Done | [LOCALIZATION.md](LOCALIZATION.md), with every formatting example verified against `Locale`/`CivilDate` output; `samples/notes` applies it (resource-loaded catalogs with a compiled-in English fallback, `Catalog::validate` in a unit test, bundle-aware `resources_dir`, Hebrew mirroring) |
| Gate: an external sample app builds a themed multi-window desktop UI with safe Rust APIs alone, with no direct native API calls | Done (automated) + Manual | [`samples/notes`](../samples/notes/README.md) is its own Cargo workspace, depends only on `choscorui`, and sets `#![forbid(unsafe_code)]`. It shows several windows, a native menu bar rebuilt per language, a brand theme switched at runtime, Vietnamese and Hebrew (RTL) translations from resource files, plurals, and VoiceOver semantics. Checks: `cargo test` in the sample (10 tests) and `NOTES_AUTOPILOT=1 cargo run`, which prints "OK (0 failures)" after 13 checks, including the release of every native window. Writing the sample exposed a window leak when the main menu is replaced, now fixed and covered by `lifetime_smoke`. Manual: a VoiceOver walk through two windows. |

## Phase 15 — packaging, release, and maintenance

Scope is macOS only. Windows and Linux packaging are skipped with phases 10 and 11; their crates still compile.

| Item | Status | Evidence |
| --- | --- | --- |
| Signed and notarized macOS builds | Done (automated) + Manual | `scripts/macos/package.sh` builds `.app` bundles for the gallery, stress gallery, and notes sample (Info.plist, bundle ids, bundled resources), arm64 or universal. It signs them ad hoc, or with Developer ID and the hardened runtime and a timestamp. It notarizes and staples through `notarytool`, then zips the bundles with a SHA-256 checksum. Manual: a notarized run with real credentials ([RELEASING.md](RELEASING.md#4-signed-notarized-archive)). |
| License audit of linked libraries, bindings, fonts, and assets | Done | [LICENSES.md](LICENSES.md) was re-audited against `Cargo.lock`, the `otool -L` link line, and the archive contents, and the framework list was corrected. The project is `MIT OR Apache-2.0` ([`LICENSE-MIT`](../LICENSE-MIT) and [`LICENSE-APACHE`](../LICENSE-APACHE), `license` set in every crate). The static `std` and `unicode-segmentation` notices are in [`NOTICE`](../NOTICE). All three files ship in every archive. |
| CI on supported OS/architecture combinations; separate manual gates | Done (automated) + Manual | `.github/workflows/ci.yml` runs on `macos-15` (arm64) and `macos-15-intel`: fmt, clippy, the MSRV check, workspace and sample tests, native smoke tests and autopilots, then a universal package and install check. Manual gates (IME, VoiceOver, notifications, notarized fresh install, Intel hardware, budgets) are listed in [RELEASING.md](RELEASING.md#5-manual-gates). |
| Reproducible release notes, known limitations, vulnerability reporting, upgrade path | Done | `scripts/release-notes.sh <from> <to>` (deterministic grouping of conventional commits), per-release GitHub Release notes (highlights, known limitations, upgrading; see [RELEASING.md](RELEASING.md#2-version-and-notes)), [SECURITY.md](../SECURITY.md), [RELEASING.md](RELEASING.md), and the [deprecation policy](VERSIONING.md#deprecation) |
| Gate: a fresh machine installs the gallery, runs every core scenario, and uninstalls cleanly | Done (automated, arm64 and x86-64) + Manual | `scripts/macos/install-check.sh <archive>` unpacks the archive and runs `install.sh` into an empty directory. It verifies signatures and launches the gallery through LaunchServices (`open`). It then runs the gallery first frame, the stress autopilot, a stress launch that saves real per-user settings, and the notes autopilot with bundled translations, all from the installed bundles. Finally it runs `uninstall.sh` and checks that no app and no `com.choscor.*` data remain. This passed 15/15 for the arm64 and universal archives on the reference M4. Running the x86-64 slice under Rosetta found an Intel crash (an `NSRect` stret return). After the fix, the x86-64 smoke tests, unit tests, and all three app autopilots pass under Rosetta 2, including the notes app from the installed universal bundle. The `macos-15-intel` CI runner repeats this on native Intel. Manual: a clean user account or VM with a notarized archive, and Intel hardware. |
