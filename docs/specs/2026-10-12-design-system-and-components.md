# Default design system: tokens, recipes, and the full component set
- Status: Ready for implementation
- Date: 2026-10-12
- Source: `/brainstorm` request on 2026-10-12. The request was to copy the design tokens, the typography, and all components of the reference design system at https://ui.shadcn.com/docs/components (and https://ui.shadcn.com/docs/theming). "Copy (not clone source)", "go from core first, try to make as general as possible, no brand name". The follow-up asked for "my component set identical … each component: every their variants, hover, effect, color... identical 100%". Decisions were answered one at a time in the session. Evidence comes from the workspace on `main` at `43e256b` plus uncommitted changes, and from the reference pages fetched on 2026-10-12. The companion spec is [2026-10-12-declarative-views.md](2026-10-12-declarative-views.md); implement this spec first.

## Outcome and current state

**Beneficiary.** People who build applications with ChoscorUI. Out of the box they get a complete, modern component set whose tokens, variants, sizes, states, and effects match the reference design system. Light and dark modes come in every base colour.

**Current state (verified in the workspace):**
- `DesignTokens` (`crates/choscorui-core/src/theme.rs:53-69`) is a flat struct with 5 colours (`background`, `surface`, `content`, `accent`, `danger`). It has one `font_size`, `font_weight` and `line_height`, one `spacing`, `radius` and `border_width`, an `elevation` scalar, plus `motion` and `contrast`.
  - `light()` and `dark()` (`theme.rs:79-121`) are unrelated looks: 17pt type, blue and purple accents, and radius 10 vs 4.
  - `ThemeOverride` (`theme.rs:125-140`) mirrors the fields as `Option`s. `ThemeResolver` applies application, scope, instance, then accessibility, and reports `Paint` or `Layout` invalidation.
- Only Button has a recipe. `ButtonRecipe`, `paint_button`, `Slot`, `SlotLayout` and `PillButtonRecipe` live in `crates/choscorui-controls/src/recipe.rs`, and `ButtonStyle` lives in `controls/src/lib.rs`. Every other control takes a hand-filled per-state colour struct (`CheckboxStyle`, `TextFieldStyle`, `SliderStyle`, `TabStyle`, …). No control has a variant or size concept.
- The `DisplayList` (`crates/choscorui-core/src/display.rs`) has:
  - rects, rounded rects, paths with `Paint` (solid, linear and radial gradient)
  - images, text and glyphs
  - clip, opacity and transform stacks
  - `DrawShadow`
  - `Shadow` (`display.rs:88-92`) is a single layer `{offset, blur, color}` with no spread and no inset.
- `Easing` (`crates/choscorui-core/src/animation.rs:12-15`) is only `Linear | EaseInOut`.
- core has an SVG path parser (`crates/choscorui-core/src/svg.rs:409`, `parse_path_data`).
- The gallery hardcodes the font family `"Helvetica"` (`gallery/src/page.rs:22`).
- The gallery covers every public name of `choscorui-controls` (`gallery/src/catalog.rs` `PAGES`, `SHOWN`, `EXEMPT`, plus a coverage test). It ships the `en` and `vi` string catalogs only (`gallery/resources/strings/`).
- Golden files in `crates/choscorui/tests/golden/` pin `theme_resolution.txt`, `painted_output.txt`, focus order, semantics and layout. `crates/choscorui/tests/render_parity.rs` compares the software rasterizer with the macOS painter.
- Dependency rule: std plus `unicode-segmentation` only, with no proc macros and no build scripts (`docs/architecture.md:179-189`, `docs/licenses.md`).

**Desired state.**
- core owns a general, brand-free token model.
- controls owns one general recipe system keyed by control, variant, size and state.
- Every reference component exists as a toolkit control whose resolved visuals match the reference values.
- Existing controls keep their behaviour and are repainted with the same system.
- The old look is deleted.

## Decisions and requirements

### D1. Reference, naming, and copying (S1)
- **Reference snapshot.**
  - The reference is the docs site as of 2026-10-12, with its default style ("Vega", formerly "New York") and the default primitives (Base UI).
  - Where the docs show only a Radix version, use that.
  - Reconcile the component list below with the live sidebar before starting. Include any entry that is missing here; the sidebar listed 65 entries and this spec names 64.
- **Copy values, not code.**
  - Take token values, sizes, paddings, radii, shadows, colours per state, transitions and animations from the reference pages and their published class lists.
  - Write all code in Rust from scratch, and do not vendor TSX or CSS.
  - No reference source is copied, so no MIT notice is needed. If maintainers later want a credit, it goes only in `docs/licenses.md`.
- **No brand name.**
  - The strings "shadcn" and "Vega" must not appear in any identifier, file name, crate, doc under `docs/` (outside `docs/specs/`), user-visible string, catalog key, or commit message.
  - The look is simply the toolkit's default theme.
  - Base palettes take the generic colour names (`Neutral`, `Stone`, `Zinc`, `Mauve`, `Olive`, `Mist`, `Taupe`).
- **Verify the values.** The token table in D3 was gathered by a research agent and summarized. The implementer re-reads each value from the source page, and the page for each component, before encoding it. Where a value here differs from the source, the source wins; record the difference in the PR.

### D2. Replace the default look (Q1)
- The new system replaces the bodies of `DesignTokens::light()` and `DesignTokens::dark()`. They become the `Neutral` base in light and dark.
- Delete the old look and its presets: the old colours, the 17pt type, `PillButtonRecipe`, flat `ButtonStyle`, and the per-state style structs.
- Rejected alternatives: keeping the old look beside the new one, and keeping it as a legacy preset. Both mean two looks to maintain through every control.

### D3. Token model in core (Q2, Q3, Q6, Q7)
All of this lives in `choscorui-core` and has no knowledge of any control.

- **Colour.**
  - Add `Color::from_oklch(l, c, h, alpha)`, which converts OKLCH to sRGB with gamut clipping.
  - Presets may store precomputed sRGB literals. A unit test then re-derives each literal from its OKLCH source within 1/255 per channel, so the source of truth stays the OKLCH table.
- **Colour roles.**
  - A `ColorRole` enum indexes a `Palette` (a fixed array, not a `HashMap`):
    - `Background`, `Foreground`, `Card`, `CardForeground`, `Popover`, `PopoverForeground`
    - `Primary`, `PrimaryForeground`, `Secondary`, `SecondaryForeground`
    - `Muted`, `MutedForeground`, `Accent`, `AccentForeground`
    - `Destructive`, `Border`, `Input`, `Ring`, `Chart1`…`Chart5`
    - `Sidebar`, `SidebarForeground`, `SidebarPrimary`, `SidebarPrimaryForeground`, `SidebarAccent`, `SidebarAccentForeground`, `SidebarBorder`, `SidebarRing`
  - Alpha colours such as the dark `Border` (white at 10%) keep their alpha.
- **Bases and schemes.**
  - `BaseColor { Neutral, Stone, Zinc, Mauve, Olive, Mist, Taupe }` and `ColorScheme { Light, Dark }`.
  - `DesignTokens::new(BaseColor, ColorScheme)` builds any combination.
  - `light()` and `dark()` equal `new(Neutral, Light)` and `new(Neutral, Dark)`.
  - Every base has a full light and dark palette taken from the theming page.
- **Neutral values gathered on 2026-10-12** (light / dark; verify against the source per D1):
  - `background`: 1 0 0 / 0.145 0 0
  - `foreground`: 0.145 / 0.985
  - `card` and `popover`: 1 / 0.205
  - `card-foreground` and `popover-foreground`: 0.145 / 0.985
  - `primary`: 0.205 / 0.922
  - `primary-foreground`: 0.985 / 0.205
  - `secondary`, `muted` and `accent`: 0.97 / 0.269
  - `secondary-foreground` and `accent-foreground`: 0.205 / 0.985
  - `muted-foreground`: 0.556 / 0.708
  - `destructive`: 0.577 0.245 27.325 / 0.704 0.191 22.216
  - `border`: 0.922 / 1 0 0 at 10%
  - `input`: 0.922 / 1 0 0 at 15%
  - `ring`: 0.708 / 0.556
  - `chart-1`…`chart-5` (light): 0.646 0.222 41.116; 0.6 0.118 184.704; 0.398 0.07 227.392; 0.828 0.189 84.429; 0.769 0.188 70.08
  - `chart-1`…`chart-5` (dark): 0.488 0.243 264.376; 0.696 0.17 162.48; 0.769 0.188 70.08; 0.627 0.265 303.9; 0.645 0.246 16.439
  - `sidebar`: 0.985 / 0.205
  - `sidebar-primary`: 0.205 / 0.488 0.243 264.376
  - Other sidebar roles follow the source page.
- **Units (Q3).** 1 CSS px = 1 pt and 1rem = 16pt. Sizes are copied 1:1, so a default button is 36pt tall. There is no desktop density tier.
- **Radius.**
  - `radius` base is 10pt (0.625rem).
  - `RadiusScale { Sm, Md, Lg, Xl, Xl2, Xl3, Xl4 }` resolves to base × 0.6, 0.8, 1, 1.4, 1.8, 2.2 and 2.6, plus `None` and `Full`.
- **Spacing.** The spacing unit is 4pt (0.25rem). `space(n)` = n × unit.
- **Typography (Q6).**
  - Font families are tokens: `font_sans` defaults to the system UI font (SF Pro) and `font_mono` to the system monospace (SF Mono). Nothing is bundled.
  - The hardcoded `"Helvetica"` in the gallery is removed.
  - Type scale, size / line height in pt: `Xs` 12/16, `Sm` 14/20, `Base` 16/24, `Lg` 18/28, `Xl` 20/28, `Xl2` 24/32, `Xl3` 30/36, `Xl4` 36/40.
  - Weights: `FontWeight` gains named constants for normal 400, medium 500, semibold 600 and bold 700 where missing.
  - Tracking scale: `Tighter` −0.05em, `Tight` −0.025em, `Normal`, `Wide` 0.025em, `Wider` 0.05em, `Widest` 0.1em. If `TextStyle` lacks letter spacing, add it to core text and both painters.
  - The reference typography rules (headings, lead, large, small, muted, blockquote, list, inline code, and the "typeset" size, leading and flow variables) become named `TextStyle` presets in core.
- **Shadows.**
  - `Shadow` gains `spread` and `inset`.
  - A `ShadowLayers` value holds up to three layers.
  - `ShadowScale { Xs2, Xs, Sm, Md, Lg, Xl, Xl2, InsetXs2, InsetXs, InsetSm }` carries the Tailwind v4 values listed in the research (for example `Sm` = `0 1px 3px 0 #0000001a, 0 1px 2px -1px #0000001a`).
  - `DrawShadow` paints all layers in both the macOS painter and the software rasterizer. Update render parity.
- **Rings.** A focus ring is an outset stroke: width 3pt, colour `ring` at 50% alpha, following the control's radius. Add a display command, or a helper over `StrokeRoundedRect`, that both painters draw identically.
- **Motion.**
  - The default duration is 150ms.
  - `Easing` gains `CubicBezier(x1, y1, x2, y2)`. The default is (0.4, 0, 0.2, 1), plus `In` and `Out` presets matching the reference.
  - Reduced motion keeps the existing behaviour: the duration becomes zero.
- **Old fields.** Remove `surface`, `content`, `accent`, `danger` and `elevation`. `ThemeOverride` gains `Option` overrides for the palette (one role or the whole palette), base, scheme, scales and fonts.
- **Invalidation.** Keep the classification: metric tokens invalidate layout, and colour, shadow and motion tokens invalidate paint.
- **Accessibility.** Accessibility still wins, and this is the one allowed deviation from identity.
  - `text_scale` multiplies the type scale.
  - `high_contrast` raises `MutedForeground`, `Border`, `Input` and `Ring` until each text role is at least 4.5:1 and each non-text role at least 3:1 against its paired surface. Ring alpha becomes 100%.
  - `reduced_motion` zeroes durations.
- **Rejected:** adding fields to the flat struct. It doesn't scale to about 30 roles × 7 bases × 2 schemes, and it isn't general.

### D4. One general recipe system in controls (Q5)
- **Visual state.** `VisualState` is a small bitset: `hovered`, `pressed`, `focus_visible`, `disabled`, `invalid`, `checked`, `indeterminate`, `selected`, `open`, `highlighted`.
  - core tracks focus-visible: focus caused by keyboard navigation shows the ring, and pointer focus does not. This follows the reference `focus-visible` behaviour. Add it to core focus handling if it is missing.
- **Resolved style.** A recipe maps (`ControlKind`, variant, `ControlSize`, `VisualState`, tokens) to a `ResolvedStyle`:
  - background `Paint`, foreground colour, border colour and width
  - corner radius per corner, shadow layers, ring
  - padding (logical inline and block insets, so RTL mirrors), min height, gap
  - text style, icon size, opacity, cursor
- **Variants and sizes.** Each control has its own variant enum (`ButtonVariant`, `BadgeVariant`, `ToggleVariant`, `AlertVariant`, …) and uses a shared `ControlSize { Xs, Sm, Default, Lg, Icon, IconXs, IconSm, IconLg }`, or its own subset.
- **Slots.** Slots generalize from Button to every control. A control declares its slots (for example a Switch has `Track` and `Thumb`, a Select has `Trigger`, `Value`, `Icon`, `Content`, `Item` and `Indicator`). The recipe styles each slot.
- **No behaviour.** Recipes still see only visual state and tokens, never input, focus, commands or semantics (`docs/theming.md` rule; `crates/choscorui-controls/tests/recipe.rs` extends to every control).
- **Cache.** Resolved styles are cached by (kind, variant, size, state, theme generation). Paint does a cache lookup and doesn't recompute styles per node, so paint cost does not grow with the model.
- **Custom recipes.** An app replaces the recipe for a control kind globally or per scope through `ThemeResolver`. One trait object type serves every kind; there is no per-control recipe trait.
- **Deleted:** `ButtonRecipe`, `PillButtonRecipe`, `ButtonStyle`, and the per-state style structs. `paint_button` becomes the general slot painter.

### D5. States and effects
For every component, copy from its reference page:
- the hover, active, focus-visible, disabled, invalid, checked/on, open and highlighted colours of each variant
- **Transitions:** `transition-[color,box-shadow]` and friends at 150ms with the default easing.
- **Focus:** the border becomes `ring`, plus the 3pt ring at 50%.
- **Invalid:** the ring becomes `destructive` at 20% (40% in dark), and the border becomes `destructive`.
- **Disabled:** opacity 50% and no pointer interaction. Keyboard focus rules stay as `docs/controls.md` defines them.
- **Open and close animations:** fade in or out, zoom from 95%, and slide-in from the side (8pt), at the durations in the source.
- **Overlay scrim:** black at the reference alpha. If the reference uses backdrop blur, add a backdrop-blur display command to both painters, with render parity.
- **Cursor:** the platform default arrow, as the reference uses since Tailwind v4.

### D6. Icons (Q-icons)
- Bundle only the Lucide icons the components use, as SVG path data in a core `icons` module (`Icon` enum), drawn through `parse_path_data` or precomputed `Path`s.
  - The set includes check, chevron-down/up/left/right, chevrons-up-down, x, circle, dot, minus, plus, search, loader / loader-circle, more-horizontal, panel-left, arrow-left/right, calendar, info, circle-check, triangle-alert, octagon-x, grip-vertical, and any others a component page shows.
- Geometry: 24-unit viewBox, stroke width 2, round caps and joins. Size 16pt by default, or whatever each component specifies.
- Add the Lucide ISC notice to `docs/licenses.md`.
- The data is Rust source or strings, with no build script and no new crate.

### D7. Component set (Q4)
Every reference entry becomes a toolkit control, with behaviour in `choscorui-controls`, visuals from D4, and a section in `docs/controls.md`. "Replaces" means the old control is renamed or merged; see D8. Names are generic words with no brand.

| Group | Control | Variants and sizes (verify on the page) | Notes |
| --- | --- | --- | --- |
| Form | Button | default, destructive, outline, secondary, ghost, link × default, xs, sm, lg, icon, icon-xs, icon-sm, icon-lg | Replaces `Button` styles |
| Form | Checkbox | checked, indeterminate, invalid | Restyle the existing control |
| Form | Combobox | — | Replaces `ComboBox`; filterable list in a popover |
| Form | DatePicker | single, range | Composes Calendar + Popover; replaces the date-input look |
| Form | Input | — | Replaces the `TextField` look |
| Form | InputOtp | groups, separator | New |
| Form | Label | — | Restyle |
| Form | RadioGroup | — | Restyle |
| Form | Select | trigger sizes sm, default | New; popover list with scroll buttons |
| Form | NativeSelect | — | New; pop-up button with the reference look |
| Form | Slider | horizontal, vertical, range | Restyle |
| Form | Switch | sizes from the page | New |
| Form | Textarea | — | New; field-sizing content, built on `TextEditor` behaviour |
| Form | Toggle | default, outline × sm, default, lg | New |
| Form | ToggleGroup | single, multiple; inherits Toggle variants and sizes | New |
| Form | Field (FieldSet, FieldLegend, FieldGroup, FieldContent, FieldLabel, FieldTitle, FieldDescription, FieldSeparator, FieldError) | vertical, horizontal, responsive | New |
| Form | InputGroup (addon, button, text) | addon align inline-start, inline-end, block-start, block-end; button sizes xs, icon-xs, sm, icon-sm | New |
| Form | ButtonGroup | horizontal, vertical, separator | Replaces the `ButtonGroup` look |
| Form | Kbd, KbdGroup | — | New; reused by the `KeySequenceEdit` look |
| Overlay | AlertDialog | — | Replaces `MessageDialog` |
| Overlay | Dialog | — | Builds on `DialogStack` |
| Overlay | Sheet | side top, right, bottom, left | New |
| Overlay | Drawer | directions from the page | New; pointer drag to dismiss |
| Overlay | Popover | — | Builds on `PopupStack` |
| Overlay | Tooltip | — | Replaces the `TooltipController` look |
| Overlay | HoverCard | — | New |
| Overlay | DropdownMenu | items, checkbox and radio items, sub-menus, shortcuts, destructive item | Replaces `PopupMenu` |
| Overlay | ContextMenu | as DropdownMenu | Shares the menu behaviour |
| Overlay | Menubar | as DropdownMenu | New, in-window; the app's native menu bar stays native |
| Overlay | Command | input, list, groups, empty, separator, shortcut; dialog form | New |
| Navigation | Tabs | default, line × horizontal, vertical | Replaces `TabSet` |
| Navigation | Breadcrumb | separator, ellipsis | New |
| Navigation | Pagination | — | New |
| Navigation | NavigationMenu | — | New, in-window |
| Navigation | Sidebar | sidebar, floating, inset × collapsible offcanvas, icon, none × left, right | New; width 16rem, icon width from the page, toggle shortcut Cmd+B; below the reference mobile breakpoint (768pt window width) it becomes a Sheet |
| Data | Table | header, body, footer, row hover and selected, caption | Restyle the `ItemView` table mode and `HeaderView` |
| Data | DataTable | sort, filter, column visibility, row selection, pagination | Composes Table with `SortFilterProxy` and `Selection` |
| Data | Card (header, title, description, action, content, footer) | sizes from the page | New |
| Data | Badge | default, secondary, destructive, outline, ghost, link | New |
| Data | Avatar (image, fallback, group, badge) | sizes from the page | New |
| Data | Chart | area, bar, line, pie, radar, radial; tooltip; legend | New native chart control using `Chart1`…`Chart5`; data comes from the app |
| Data | Calendar | single, multiple, range; dropdown caption | Restyle `Calendar` |
| Data | Carousel | horizontal, vertical; previous and next buttons | New; arrow keys and scroll snap |
| Feedback | Alert | default, destructive | New |
| Feedback | Toast (Toaster) | default, success, info, warning, error, loading | New in-window stack; position and stacking from the page |
| Feedback | Progress | — | Replaces `ProgressBar` |
| Feedback | Skeleton | — | New; pulse animation honours reduced motion |
| Feedback | Spinner | — | New; spin honours reduced motion |
| Feedback | Empty (header, media, title, description, content) | media default, icon | New |
| Layout | Separator | horizontal, vertical | Restyle |
| Layout | Resizable (panel group, panel, handle with grip) | horizontal, vertical | Replaces `Splitter` |
| Layout | ScrollArea, ScrollBar | — | Restyle |
| Layout | AspectRatio | — | New |
| Layout | Collapsible | — | New |
| Layout | Accordion | single, multiple | New; the `ToolBox` look reuses it |
| Layout | Item (group, separator, header, media, content, title, description, actions, footer) | default, outline, muted × default, sm, xs | New |
| Chat | Message (group, avatar, content, header, footer) | align start, end | New |
| Chat | Bubble | default, secondary, muted, tinted, outline, ghost, destructive | New; max width 80% except ghost |
| Chat | Attachment | media icon, image × horizontal, vertical × idle, uploading, processing, error, done × default, sm, xs | New |
| Chat | Marker | default, border, separator | New |
| Chat | MessageScroller | — | New; virtualized, sticks to the bottom |
| Chat | Questionnaire | progress, choices, freeform, previous/skip/next/submit | New |

- **Direction** is not a control. Layout direction already lives in core (`LayoutDirection`), and every control mirrors in RTL.
- **Typography** is not a control. It becomes text style presets (D3).
- **Keyboard behaviour** follows the WAI-ARIA pattern each reference component uses: roving focus in menus, toggle groups, tabs and radio groups; type-ahead in menus and selects; Escape closes overlays; focus is trapped in dialogs and returns to the trigger.
- **Semantics.** Each new control has semantics in the semantic tree and keyboard activation, per `docs/controls.md` shared rules.

### D8. Existing controls (Q11)
- Controls with a reference counterpart are replaced or renamed as in D7: `ComboBox`, `TextField` look, `PopupMenu`, `TabSet`, `ProgressBar`, `Splitter`, `MessageDialog`, `TooltipController` look, and `ButtonGroup`.
  - Follow `docs/versioning.md`: a renamed public item keeps a `#[deprecated(since, note)]` alias for one minor release, and the release notes list it.
  - Removed style structs and recipes are breaking changes and are listed in the release notes.
- Controls without a counterpart keep their behaviour and are repainted with the new tokens and D4 recipes so they match the set:
  - `ItemView` (list and tree), `HeaderView`, `ColumnBrowser`, `CellEdit`
  - `SpinBox`, `DecimalSpinBox`, `NumberInput`, `InputMask`, `DateTimeEdit`: Input look with stepper buttons
  - `Dial`, `NumericDisplay`
  - `KeySequenceEdit`: Kbd look
  - `GroupBox`: Card look
  - `StackedContainer`
  - `ToolBox`: Accordion look
  - `DockLayout`, `Toolbar`, `StatusBar`
  - `ColorPicker`, `FontDialog`, `InputDialog`
  - `FocusFrame`: ring
  - `SizeGrip`, `CanvasView`, `CustomControl`, `TextEditor`
- These stay native and unchanged: the app menu bar (`NSMenu`), open and save panels, the IME window, and the title bar (`docs/theming.md`).

### D9. Gallery
- Every new or renamed control gets a gallery page and is added to `PAGES` and `SHOWN` (`CONTRIBUTING.md` "Adding a control").
- Each page shows every variant × size, and lets the user reach every state: hover, focus-visible, disabled, invalid, checked, open.
- The state bar gains a base-colour picker (7 bases) beside light and dark.
- The gallery keeps shipping `en` and `vi` only. Add sample strings to both catalogs. Do not add another language without asking the maintainer.

### D10. Implementation order (S3)
Core first, then outward. Each step leaves the workspace green, and the golden files are re-blessed only where that step changes them.

1. **core**: `Color::from_oklch`, the palette, roles, bases and schemes, the scales, fonts, tracking, multi-layer and inset shadows, the ring, cubic-bezier easing, focus-visible tracking, text presets, icons, and the new `DesignTokens`/`ThemeOverride`/`ThemeResolver`.
2. **render and macos**: paint the new or changed display commands (shadow layers, ring, letter spacing, backdrop blur if needed) and extend render parity.
3. **controls**: the general recipe system and the slot painter, then the existing controls ported onto it (D8).
4. **controls**: the new components, in this order: form, overlay, navigation, data, feedback, layout, chat.
5. **gallery and docs** in the same changes as each step: `docs/theming.md`, `docs/controls.md`, `docs/licenses.md`, `docs/architecture.md` where crate contents change.

Split the work into reviewable PRs along these steps.

### Non-goals
- A stylesheet file or CSS parsing (deferred in the companion spec).
- The declarative view layer (companion spec).
- Bundled fonts.
- Desktop density tiers.
- Any web-only feature with no window meaning, such as URL routing in Breadcrumb or Pagination. They emit actions instead.

## Acceptance criteria and public test seams

| Criterion | Observable seam |
| --- | --- |
| Every palette role matches its OKLCH source for all 7 bases × 2 schemes | core unit test that re-derives each sRGB value from the OKLCH table within 1/255; `theme_resolution.txt` golden lists every role per base and scheme |
| `light()`/`dark()` equal the Neutral base and the old look is gone | `theme_resolution.txt` golden; `git grep` finds no `PillButtonRecipe`, `ButtonStyle` or old style structs outside the deprecation aliases |
| Scales match the reference (radius, spacing, type, tracking, shadows, motion) | core unit tests on each scale's resolved values; golden entries |
| Every control resolves the reference style for every variant × size × state | `painted_output.txt` golden per control, variant, size and state; per-control resolved-style table tests in `choscorui-controls` |
| The recipe cannot change behaviour | the `crates/choscorui-controls/tests/recipe.rs` pattern, extended to every control: swapping recipes leaves activation, focus and semantics unchanged |
| Focus ring only for keyboard focus | core focus test: Tab shows `focus_visible`, a pointer press does not |
| High contrast meets 4.5:1 text and 3:1 non-text for each base and scheme | core contrast test over all palettes with `high_contrast` set |
| Reduced motion finishes every open, close, spinner and skeleton animation at once | `AnimationClock` tests with `motion.reduced` |
| Shadows, ring and blur paint identically in both painters | `crates/choscorui/tests/render_parity.rs` cases for each new command |
| Every reference component exists, with keyboard behaviour and semantics | gallery coverage test (`cargo test -p choscorui-gallery`); semantics and focus-order goldens; per-control keyboard tests |
| Every gallery page builds in LTR and RTL, enabled and disabled, in each base and scheme | headless page test in the gallery |
| No brand name ships | CI or test step: `git grep -il -e shadcn -e vega -- ':!docs/specs'` prints nothing |
| Lucide icons are licensed | `docs/licenses.md` contains the ISC notice; icon path tests parse every `Icon` |
| Performance budgets hold | `choscorui-bench --check bench/budgets.txt` passes; resolved-style cache test shows no per-node recomputation |
| Docs follow the rules | `scripts/check-docs.sh` passes; `docs/controls.md` catalog is complete |

## Constraints and risks
- **Scale.** This is roughly 40 new controls plus a port of about 50. Land it in the D10 order as several PRs. Each PR runs the CI `check` and `native` job commands from `CONTRIBUTING.md`, plus `scripts/check-docs.sh` for Markdown.
- **Golden files** change in nearly every step. Bless them with `CHOSCORUI_BLESS=1 cargo test -p choscorui --test compatibility` only for the intended change, review the diff, and call it out in each commit and PR.
- **Value accuracy.** The token and variant data here came from summarized pages. Re-read each source page (D1), and keep a per-control value table in `docs/controls.md` as the pinned record.
- **"Identical" limits.** Core Text and browser text rasterization differ, so text shapes can't match the browser pixel for pixel. Matching is defined by resolved values: colour, geometry, radius, shadow, timing. A side-by-side visual check against the browser is a human task; automation can't claim it.
- **Accessibility deviation.** High-contrast adjustments (D3) intentionally differ from the reference.
- **Human-only checks.** VoiceOver, real input methods, a notarized clean install and Intel hardware need a person. Don't claim them from automation.
- **Desktop fit.** The 36pt default controls and the hover-driven states are web conventions that the maintainer accepted for identity (Q3).
- **Chart** is a large standalone piece. Treat it as its own PR in step 4.
- **Dependencies.** Stay within the dependency rule: no crates, no proc macros, no build scripts. Icon data and palettes are plain Rust.
- **File size.** `rustquty.toml` limits a file to 3,000 lines and a function to 700. Split controls into modules by responsibility, for example `controls/src/overlay/`, `controls/src/form/`.
- **Layers.** Keep `bylaw.toml` layers: tokens, icons and focus-visible go in core; recipes and controls in controls. Run `cargo bylaw check`.
- **Deferred:** a stylesheet file (owner postponed it in the companion spec), and bundled fonts (owner chose the system font).
- **Paths:** `crates/choscorui-core/src/theme.rs`, `display.rs`, `animation.rs`, `svg.rs`; `crates/choscorui-controls/src/recipe.rs`, `lib.rs` and control modules; `crates/choscorui-render`; `crates/choscorui-macos/src/paint.rs`; `crates/choscorui/tests/golden/`; `gallery/src/catalog.rs`, `gallery/src/pages/`; `docs/theming.md`, `docs/controls.md`, `docs/licenses.md`.

Read this whole spec and inspect the current workspace before implementing.
