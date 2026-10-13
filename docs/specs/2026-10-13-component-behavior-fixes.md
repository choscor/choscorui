# Component behavior fixes

- Status: Ready for implementation
- Date: 2026-10-13
- Source: Maintainer brainstorm session on 2026-10-13. The maintainer asked to check every gallery component with one read-only reviewer per component. A component "works well" when it behaves as common sense expects. The maintainer then asked for a spec that fixes everything found. The maintainer named these examples: Input accepts any text; the gallery has an "Invalid" Button, but a button is not a field; the invalid Checkbox/Radio style is wrong (it should have a red label and box, a red border only, and no shadow); Switch, Toggle, and ToggleGroup have invalid states, but only fields should. 99 static code reviews (one per page id in `gallery/src/catalog.rs`) produced the evidence. Nothing was built or run during the reviews. The decisions below come from the maintainer's answers in the session.

## Outcome and current state

**Beneficiary.** People who use choscorui applications, and the developers who build those applications with the toolkit and its gallery. Today many controls accept input they should reject, show states that make no sense for them, give no hover or pressed feedback on their inner buttons, overflow their bounds with long text, and hide their parts from assistive technology. After this work, each control behaves the way a polished macOS control with the reference design-system look would behave.

**Current state (static reading, 2026-10-13).** The reviews found about 400 issues across the 99 gallery pages. They fall into these groups:

- **Invalid state on non-fields.** `Button`, `Switch`, `Toggle`, and `ToggleGroup` have public `set_invalid` APIs, invalid recipe branches, gallery "Invalid" sections, and `gallery/src/ui/pages/variants.rs` expectations that force INVALID (`variants.rs:86-92`, `116-117`, `123`, `130`). `Badge` has a dead invalid branch (`style/default/data.rs:217`). The NumericDisplay recipe applies `states()`, which includes invalid.
- **Invalid look.** The shared `invalid()` in `crates/choscorui-controls/src/style/default/mod.rs:126-134` sets a 1 pt destructive border (50% alpha in dark) and a 3 pt destructive ring at 20%/40% alpha. Every control gets this look. The ring replaces the focus ring, so an invalid control that has focus shows no focus. Checkbox and Radio also keep the xs shadow (`form.rs:182-183`). The label is never destructive (`form.rs:785-791`). A checked invalid box resets its border to primary (`form.rs:191-199`). A `Field` error marks the control invalid only for accessibility (`crates/choscorui-compose/src/view/field.rs:174-188`).
- **Input filtering.** `Input` has no kinds and no maximum length (`compose/src/view/widgets.rs:357,366`; `controls/src/text_field.rs:347-351`). `NumberInput`, `DateInput`, `DecimalSpinBox`, and the InputDialog number kinds accept letters and turn invalid only afterwards (`controls/src/input_rules.rs:229-246,328-338`; `widgets.rs:647-654`). The gallery InputMask stores rejected characters (`gallery/src/ui/pages/text.rs:252`). `>` is read as a literal (`input_rules.rs:84-86`). `SpinBox` cannot be typed into (`range_controls.rs:481-497`).
- **Shared infrastructure gaps.**
  - No hosted text control uses the clipboard: Cmd+C and Cmd+V do nothing, and Cmd+X deletes the text (`text_field.rs:406-410`, `form/input.rs:124-131`, `form/textarea.rs:196-197`, `text_editor.rs:429-431,667-671`). The platform `Clipboard` exists (`crates/choscorui/src/lib.rs:151`, `crates/choscorui-macos/tests/clipboard_smoke.rs`).
  - Hosted controls get no click count (`textarea.rs:173`, `text_editor.rs:720` hard-code 1; `core/src/input.rs:91` has `click_count`).
  - Popups never flip or clamp (`listbox.rs:158-206`; `popup.rs:35-46` `place_popup` has no caller).
  - `draw_text` neither truncates nor clips (`style/paint.rs:359-385`), and it draws one line only (`paint.rs:517-527`).
  - The shared panel `Buttons` (`overlay/panel.rs:179-198`) never set HOVERED or DISABLED, and the pressed look stays while the pointer drags off.
  - Many composite controls build part states from `VisualState::NONE` and drop the host state.
  - `Cursor::shape` has no caller (`style/resolved.rs:201`), so style cursors never reach the window.
  - Wheel events return Stop at the scroll limits, so nested scrolling does not chain.
- **Accessibility.** Most composite controls publish one node with no children. The focused part is put in `description` or `value`. VoiceOver cannot list menu items, dialog buttons, toolbar items, tabs, cells, and similar parts.
- **Per-control bugs.** Examples: the Drawer jumps back open when a drag dismisses it; AlertDialog does not move or return focus; Toolbar overflow is unreachable by keyboard; Dock floating panels are never painted; the FontDialog Italic toggle has no effect; Canvas traps Tab; DataTable counts hidden selected rows; ToolButton menus never open.
- **Gallery demos.** Several pages use hand-made stand-ins and not the control they document. Several pages miss disabled demos, reuse string keys from other pages, or show demos that do not work.

**Desired state.** Phases A to E below are complete. Each phase is a separate series of commits that can ship alone, in this order: A, then B, C, D, E.

## Decisions and requirements

### Decision ledger (all settled by the maintainer in this session)

| ID | Decision | Answer |
| --- | --- | --- |
| Q1 | Which controls keep an invalid state | Fields only. Keep invalid on Input, Textarea, InputOtp, InputMask, NumberInput, DateInput, DecimalSpinBox, SpinBox, Select, NativeSelect, Combobox, DatePicker, DateTimeEdit, FontChooser, Checkbox, RadioGroup, and the InputGroup root. Remove it from Button, Switch, Toggle, ToggleGroup, Badge, and NumericDisplay: the public API, the recipe branch, the gallery section, the variants expectation, the strings, the tests, the goldens, and the docs. A required Switch shows its error through the `Field` message only. |
| Q2 | Invalid look | Checkbox and Radio: destructive box border (checked boxes too), destructive label, no ring, no shadow. Text and select fields: keep a destructive border and a soft destructive ring while unfocused. While the field shows focus, show the normal focus ring, so focus stays visible. |
| Q3 | Input restriction | Reject at typing time. A rejected key or paste does not change the text and plays the system beep. `Input` gets `max_length` and an optional character filter. Invalid is only for incomplete or out-of-range values. No inline "numbers only" hint. |
| Q4 | Scope and phasing | One spec with every finding. It has five ordered phases (A: the maintainer's examples, B: shared infrastructure, C: per-control behavior bugs, D: accessibility child nodes, E: polish and gallery). Each phase ships alone. |
| Q5 | Features a control models but does not deliver | Implement what the control name promises. InputDialog becomes modal with Cancel and Escape, and its Choice kind uses a Select. Dock gets tab close, tab drag, and painted floating panels. ToolButton opens and closes its menu. Gallery demos that need content in a dialog or popover use the compose `dialog`/`sheet`/`drawer`/`popover`/`hover_card` modifiers (`compose/src/view/modifier.rs:424-480`). The controls-level `Dialog` and `Popover` panels stay text-only, and the docs say so. |
| Q6 | Strings inside controls | A built-in catalog in core with `choscorui.*` message ids in English and Vietnamese. An application can override and extend it with its own `Catalog`. Controls look strings up through the locale in their context. Validation results become typed error enums that the view layer formats through the catalog. |
| Q7 | Platform conventions when the reference design system and macOS disagree | macOS behavior and the reference design-system look. Keyboard and interaction follow macOS, for example: Return fires the default button, Cmd+. cancels a dialog, arrow keys follow the visual direction in right-to-left layouts, ArrowLeft at a menu root does nothing, and type-ahead uses a prefix. Visual values follow the theme recipes. |

Derived rule (from Q1, Q2, and the reference look): a `Field` with an error makes its control invalid visually as well as semantically, and its label becomes destructive.

### Global rules for every phase

- Put platform-independent behavior in core, `controls`, or `compose`. Branch on `capabilities()`.
- Take every visual value from theme tokens and recipes. Add a token only when it has a second use.
- When a control fixes the same thing in the same way as another control, use one shared helper. Do not copy code. Examples: sub-part hover and pressed tracking, text truncation, type-ahead, and stepper repeat.
- Fallible public APIs return a domain error enum and never panic. This includes `DecimalSpinBox` bounds (`widgets.rs:422,455`) and `HostedStack::semantics` (`containers.rs:1390`).
- Do not name the reference design system in any tracked file outside `docs/specs/`. `scripts/check-docs.sh` fails on it. Code comments, docs, and test names describe the behavior.
- Update `docs/controls.md` and other affected `docs/` files in the same commit as the code. Remove doc rows that describe behavior the code does not have, or implement the behavior.
- Each user-visible string goes through a catalog: the gallery catalogs (`gallery/resources/strings/en.lang`, `vi.lang`) or the new built-in catalog (A5). The gallery ships English and Vietnamese only. Do not add languages.

### Phase A: the maintainer's examples

#### A1. Remove invalid from non-fields

Remove all of these:

- **Button.** `set_invalid` and its state: `controls/src/button.rs:71,89,140-143,321,410`. Also remove `invalid()` from the `button()` recipe (`style/default/form.rs:135`), the gallery Invalid section (`gallery/src/ui/pages/inputs.rs:23-24,50,92-99`), the strings `button.invalid` and `button.required` (`en.lang:287-288`, `vi.lang:284-285`), and `docs/controls.md:276-277,308`. `DatePicker` reuses the `button()` recipe and needs invalid (`form.rs:396`). Move the `invalid()` call into the field-trigger callers (DatePicker, and any other field trigger that reuses `button()`), so the DatePicker invalid look stays.
- **Switch.** `switch.rs:54-63,83,188`. The track recipe calls `states()` (`form.rs:476`). Replace that call with `focus()` and `disabled()` only. Also remove the gallery section (`gallery/src/ui/pages/form.rs:45-49`), the tests (`controls/tests/form.rs:193-199`, `compose/tests/control_views.rs:353-380`), and `docs/controls.md:2222-2223,2232`.
- **Toggle.** `toggle.rs:24,37,63-72,87,148`.
- **ToggleGroup.** `toggle.rs:235,256,286-296,511,557`. Also remove the inline invalid ring in the recipe (`form.rs:520-527`), the gallery sections (`form.rs:82-93,170-181`), the string `toggle-group.required` (`en.lang:168`, `vi.lang:166`), and the tests (`controls/tests/form.rs:193-222`, `styles_form.rs:591-596`). Also remove `docs/controls.md:2263-2265,2283-2284,2293` and `docs/guide.md:101`.
- **Badge.** Remove the dead `invalid()` call (`data.rs:217`).
- **NumericDisplay.** Replace `states()` in the recipe with `focus()` and `disabled()` only (`data.rs:516`).
- **variants.rs.** Remove INVALID from the expectations for button (`86-92`), switch (`116-117`), toggle (`123`), and toggle-group (`130`).
- **Goldens.** Remove the `# invalid` blocks under `## toggle` and `## toggle-group` in `painted_form.txt`, which come from `crates/choscorui/tests/compatibility/painted_slots.rs:146`. Remove the Button invalid rows in `painted_output.txt` and the `invalid` row in `compatibility.rs:358`. Bless them as an intended change and name the bless in the commit and the PR.

Any remaining public `set_invalid` API is on a field-like control only. The compiler then rejects invalid on a non-field.

#### A2. Invalid look

Split the shared helper in `style/default/mod.rs`:

- `field_invalid()` is for text and select fields: Input, Textarea, InputOtp group and slots, InputGroup root, Select and NativeSelect triggers, Combobox, DatePicker trigger, FontChooser, NumberInput, DateInput, SpinBox, and DecimalSpinBox field.
  - The border is destructive at full strength in both themes. Today dark uses 50% (`mod.rs:132`), which is too weak. Record the values in `docs/theming.md`.
  - The ring is the soft destructive ring (20% light, 40% dark) only while the control does not show focus.
  - While focus is visible, the ring is the normal focus ring (`focus_ring()`) and the border stays destructive. Call order: `invalid` first, then `focus`. Or give the helper one rule that reads both states.
- `choice_invalid()` is for Checkbox and Radio boxes:
  - The border is destructive at full strength, in both themes, checked or not.
  - No ring and no shadow while invalid. The normal focus ring still shows when focus is visible.
  - A checked invalid box keeps the primary fill and has a destructive border. Remove the reset to the primary border (`form.rs:191-199`).
  - The Label slot of Checkbox and Radio (and RadioGroup options) uses the destructive foreground while invalid. The label recipe (`form.rs:785-791`) gets an invalid branch.
- The InputOtp group invalid ring is painted (today the recipe sets it at `form.rs:731-741`, but `otp.rs:197-198` reads only the radius).
- The Questionnaire choice indicator follows `choice_invalid()` (`style/default/chat.rs:347-372`). Its freeform Input follows `field_invalid()`.
- FocusFrame paints the ring from the resolved style of the focused node, so it no longer covers a field's own ring with a neutral ring (`widgets.rs:1606-1609`).
- Update `docs/controls.md` (Input, Checkbox, Radio, and Field sections, including `343`, `345`, `388-389`, `402`) and `docs/theming.md`.

#### A3. Field propagates invalid to the visual state

- When a compose `Field` has an error, set the INVALID visual state on its control, not only the semantic flag (`compose/src/view/field.rs:174-188`, `host.rs:1581`, `text_field.rs:246-249`).
- The Field label uses the destructive foreground while the field has an error (`controls/src/form/field.rs:121`).
- A Field around a control that has no invalid state (Switch, Toggle, and similar) shows the error message and a destructive label. The control itself does not change.
- The error message is the accessible description and is related through `DescribedBy`. It does not replace the help description. Both are exposed (`compose field.rs:110-114,177-180`). Compute the relation again when the error appears or goes away (`field.rs:101-103`).
- A click on a Field label focuses the control. For Checkbox and Switch it toggles the control (`field.rs:116-118`, compose `field.rs:108`).
- A disabled Field disables its control and dims its label (compose `field.rs:32-78`).

#### A4. Reject input at typing time

Add one shared filter model in `controls` (next to `input_rules.rs`). Each rule gets two functions:

- `accept(current, edit) -> Option<new_text>`: `None` rejects the edit. An edit is a typed character, an IME commit, a paste, a drop, or a `SetValue`.
- `validate(text) -> Valid | Intermediate | Invalid(ErrorKind)`: this one stays.

Behavior:

- A rejected edit leaves the text, caret, and selection as they were. It adds no undo step. It plays the system beep through a new backend call (B12).
- A paste is filtered as a whole. If any character is not allowed, the full paste is rejected. Exception: InputMask applies the mask to the pasted text and keeps the characters that fit (A4 InputMask below).
- Apply `max_length` in grapheme clusters. A paste that would exceed it is cut to the remaining room. An edit with no room is rejected.
- Rules:
  - **NumberInput, DecimalSpinBox, InputDialog Integer and Decimal.** Allow digits, the locale decimal separator (only when fractions are allowed, and only once), the locale group separator, and one leading minus (only when the minimum is negative). Reject impossible text at once (`1-2`, `1.2.3`, `--`; `input_rules.rs:232-245`). A value below the minimum while typing is Intermediate, not Invalid, for both Integer and Decimal (`dialogs.rs:868-886`). Out of range at commit is Invalid. Format the text with the locale when the value commits (Return or blur).
  - **DateInput.** Allow digits and the locale date separator. Allow at most the field lengths of the locale order. Reject impossible partials at once (`13/`, `1234/`; `input_rules.rs:329-334`). Make `2026-13-45` Invalid in YMD locales, including the root locale (`input_rules.rs:334`, `locale.rs:70`). Normalize on commit.
  - **InputMask.** Store the value after the mask is applied, not the raw text (`gallery/src/ui/pages/text.rs:252`). Support the `>` and `<` case modifiers (`input_rules.rs:84-86`), or remove `>` from the demo and the docs. Make the gallery phone mask required (`(000) 000-0000`, not `(999) 999-9999`). Keep the caret at its logical position after a reformat (`text_field.rs:540-549`). Use `Input::masked` in the gallery and delete the hand-written copy in `text.rs:321-334`. A rejected key must leave the shown text unchanged, even when the bound value is equal (`widgets_form.rs:89-91`, `reactive.rs:629-641`). Add a test for that.
  - **SpinBox.** Make the field editable with the integer rule (`range_controls.rs:481-497`). Commit on Return and on blur, with clamping. Show a caret and selection like Input.
  - **DecimalSpinBox.** Show a caret and selection (`widgets.rs:405-408,656-660`). Commit valid pending text before an arrow, page, or stepper step (`widgets.rs:461-465`). On blur with invalid text, restore the last committed value. Round, then clamp (`widgets.rs:454-455`). Return a domain error for min greater than max, a NaN bound, or a step of 0 or less. Do not panic.
  - **Input.** Add `max_length(n)` and `filter(CharClass)` builders. CharClass includes digits, ASCII alphanumeric, and a custom predicate. Add kinds `email`, `url`, `tel`, `search`, and `number`. A kind sets the filter where it has one (`tel`, `number`), the validation, and the accessibility subrole where one exists. Password blocks copy and cut.
  - **InputOtp.** A paste replaces the slots from the first slot (`otp.rs:101-106`). It is not appended. The alphanumeric kind accepts ASCII only. Full-width digits normalize to ASCII digits (`otp.rs:73-79`).
  - **CellEdit editors.** Number columns use the number rule (`gallery products.rs:394-397`). Bool columns use a checkbox editor with Space to toggle, not free text (`model_widgets.rs:300,331,367-371`).
- Rewrite the tests that pin the old behavior, for example `gallery/src/ui/pages/text.rs:448-450` (`"12abc"` becomes invalid) and `467-470`. Do not weaken other tests.

#### A5. Built-in string catalog

- Add a built-in `Catalog` in `choscorui-core` with `choscorui.*` message ids, in `en` and `vi`. The application `Catalog` takes priority. A message id the application does not define falls back to the built-in one. A message that is missing everywhere returns `LocaleError::MissingMessage`, as today (`docs/localization.md`).
- Give controls access to a `Localizer` through their context (paint, semantics, and event contexts), from the window locale.
- Move every hard-coded user-visible English string in `controls` and `compose` into the catalog. Known sites:
  - `input_rules.rs:182,244,258,270,278,322,338`, `model_widgets.rs:353-396,471-474` (DataMapper shows `{error:?}` today), `dialogs.rs:456,479,873,884,903`
  - `combo.rs:633,675-676`, `dialogs.rs:221` ("Modal"), `shell.rs:696` ("Status"), `dock.rs:122-126,626-634,656`
  - `range_controls.rs:729-730`, `widgets.rs:815`, `key_sequence.rs:125`, `datetime_controls.rs:366`, `model_widgets.rs:189-196`, `canvas.rs` zoom description, `desktop.rs:530`
  - Search the crates for other string literals in `semantics()` and paint code.
- Validation functions return typed error enums, such as `NumberError::{NotANumber, BelowMinimum(..), AboveMaximum(..)}`, and not strings. The view layer formats them through the catalog. InputDialog Integer and Decimal use the same wording.
- Add the new keys to `docs/localization.md`.

### Phase B: shared infrastructure

#### B1. Clipboard in hosted text controls

- Route Cmd+C, Cmd+X, and Cmd+V (and the Edit menu Copy, Cut, and Paste commands) from Input, Textarea, TextEditor, TextField, Combobox (editable), InputOtp, InputMask, DateTimeEdit, SpinBox, and DecimalSpinBox to the platform `Clipboard`.
- Branch on `capabilities().clipboard`. When the clipboard is not supported, Cut does not delete the text.
- Paste goes through the A4 filter. Password fields block Copy and Cut. Cut is one undo step.
- Fix every site that drops the cut text: `text_field.rs:406-410,624-636`, `form/input.rs:124-131`, `form/textarea.rs:196-197`, `text_editor.rs:429-431,667-671`.
- Keep the clipboard read and write behind a core trait. Tests use a fake clipboard. The real clipboard is covered by the existing native smoke test.

#### B2. Pointer click count and macOS editing keys

- Add the click count to the hosted pointer event (`ControlEvent`, `custom.rs:23-41`). Fill it from `PointerDetails::click_count`.
- Remove the hard-coded `clicks: 1` (`textarea.rs:173`, `text_editor.rs:720`). Remove ItemView's private 0.5 s timer (`item_view.rs:1118`).
- Text controls:
  - A double click selects a word, and a triple click selects a line or paragraph.
  - Cmd+Left and Cmd+Right go to the line start or end. Cmd+Up and Cmd+Down go to the document start or end.
  - Cmd+Backspace deletes to the line start. Option+Backspace and Option+Delete delete a word. Option+Left and Option+Right move by a word. With Shift, each of these extends the selection.
  - Keep one shared editing key map. The pointer and key code duplicated between `text_field.rs:640-712` and `input.rs:83-154` becomes one implementation.
- Other double-click uses: double-click on a Resizable or Dock splitter collapses or restores the pane. Double-click on a CellEdit cell starts editing. Double-click on an ItemView row activates it.

#### B3. Popup placement

- Use one placement function for every popup: `place_popup` in `popup.rs:35-46`, or its replacement.
  - It flips to the other side when the preferred side does not fit, and clamps to the window with a margin token.
  - It limits the popup size to the window, and the list scrolls when it is limited.
  - It knows the toolbar area (Bottom opens up; Left and Right open to the side).
- Callers: Select and NativeSelect list (`listbox.rs:158-206`), Combobox (`combo.rs:501-509,700-702,782-788`), DatePicker popover (`date_picker.rs:354-359`), DropdownMenu, ContextMenu, Menubar and their submenus (`menu_popup.rs:77-103`, `menus.rs:306-314`), NavigationMenu panel (`navigation_menu.rs:149-155`), Breadcrumb ellipsis menu, DataTable columns menu, Toolbar overflow (`shell.rs:874-881`), Chart tooltip x (`plot.rs:367-372`), and Popover width (`popover.rs:143-154`).
- Combobox and FontChooser lists scroll past 8 rows and keep the highlight visible (`combo.rs:21,536-538,421`).
- A press outside an open popup closes the popup and is swallowed. The control under the pointer does not get it (`custom.rs:692-706`). This applies to every popup and menu, including Breadcrumb and DataTable presses inside the control but outside the panel.
- Every close without a choice emits `Dismissed`: an outside press, Escape, focus loss, and disabling. Affected sites: Popover (`popover.rs:173-181`), ContextMenu (`menus.rs:111-112`), Select, and others. When `set_enabled(false)` closes an open control, it emits `OpenChanged(false)`: AlertDialog `dialogs.rs:1385-1390`, NavigationMenu `navigation_menu.rs:699-706`, DatePicker `date_picker.rs:538-543`.

#### B4. Text truncation, clipping, and wrapping

- Add an end-truncation option (an ellipsis, mirrored in RTL) and a clip to the label paint helper (`style/paint.rs:359-385,517-527`). Add a wrapped-paragraph paint and measure for multi-line text.
- Use truncation where one line is expected:
  - Label, Select trigger (keep room for the chevron), DatePicker trigger, Marker, Attachment name and detail, DataTable cells, headers, and status
  - ToolBox titles, Breadcrumb labels, Sidebar labels (keep room for badge and chevron), StatusBar message and sections
  - Command item label (keep room for the shortcut, as `menu_popup.rs:271` does), CommandLink, Popover trigger, Tabs labels, and ItemView rows (add an ellipsis to the clip)
- Use a clip and a fit for Avatar fallback text.
- Use wrapping where the reference look wraps:
  - Tooltip text, wrapped at its 320 pt maximum width with height from the line count (`popup.rs:265-270`)
  - AlertDialog message and detail (`dialogs.rs:1007-1019,1087-1092`)
  - Dialog and Sheet titles, which also keep room for the close button (`dialog.rs:317-326,379-384,455-467`)
  - ToolBox page text, Accordion titles, Questionnaire choice rows, and the Chart legend (in rows)
- Each `measure` that ignores constraints honors them: CommandLink `tool_buttons.rs:455-466`, Breadcrumb `breadcrumb.rs:197-236`, and Card `card.rs:136-142` (also add a maximum width token if a second use exists).

#### B5. Sub-part interaction states

Add one shared helper for composite controls with inner buttons or rows. It tracks the hovered part from Move and Leave, and the pressed part while the pointer is captured. The pressed look shows only while the pointer is over the pressed part. The helper also merges the host DISABLED state into each part query.

Apply it to:

- `overlay/panel.rs` Buttons, which serves Alert, Card, Empty, Item, Questionnaire, Dialog, Sheet, Drawer, and Toast. The Dialog close button gets it too.
- AlertDialog buttons, Carousel previous and next, Calendar days and navigation, DatePicker days, Attachment remove, InputGroup addons, ButtonGroup members, Toolbar items (also the checked state), Menubar titles, Dock tabs and splitters, and ToolBox headers.
- HeaderView cells, ColumnBrowser rows, ItemView rows, Collapsible toggle (hover and press only on the toggle, `hosted.rs:39-56`, `custom.rs:893`), NavigationMenu triggers and links (pressed and focused fill), Pagination, Breadcrumb ellipsis, SpinBox and DecimalSpinBox steppers, ToolButton segments, and Sidebar rows.

Also fix each "disabled looks enabled" case. Alert, Card, Empty, Questionnaire, ToolBox, HeaderView, ColumnBrowser, Badge link, Breadcrumb, Collapsible title and items, Accordion chevron, Resizable, Dial track and thumb, ScrollBar, ColorPicker, ContextMenu area, MessageScroller, and the CustomControl gallery StarRating must dim. Prefer one group opacity over the whole control to opacity per slot (the Slider issue at `form.rs:411,419,433`).

#### B6. Focus rings

Every focusable part shows the theme focus ring on keyboard focus, and only on keyboard focus.

- Missing today: Toast close, Breadcrumb ellipsis, Chart, ScrollView (Root recipe never queried), ColumnBrowser, Canvas (no ring when no item has focus; the item outline shows only with focus), Toolbar embedded controls, ColorPicker (whole control), and CustomControl StarRating (use `focus_ring()` and not a primary stroke).
- Wrong today: Attachment uses `focused()` and not `focus_visible()`, so it shows two rings. Dial draws two rings. ButtonGroup and InputOtp neighbours paint over the focused member's ring, so paint the focused member last. HeaderView cells are clipped, so inset the ring.
- DatePicker shows the day ring after a pointer open. Show it only for a keyboard open.

#### B7. Direction-aware keys (macOS convention, Q7)

In right-to-left layouts, horizontal arrow keys follow the visual direction:

- Slider and RangeSlider, Tabs, Calendar days, DropdownMenu, ContextMenu, and Menubar submenus (the arrow toward the submenu opens it)
- HeaderView, ItemView grid, NavigationMenu links, CustomControl StarRating, and Carousel wheel

Remove the "do not mirror" statements in `docs/controls.md`, for example `950-951`, `1370-1371`, `1663`, and the Tabs line near `376`. Mirror the layout and hit-testing that do not mirror today: HeaderView columns (`model_widgets.rs:95-115,216,257-264`), ItemView grid (`item_view.rs:379-384,395-396,558-563`), Textarea hit-testing (`textarea.rs:131-133,167`; use `layout_point` as `text_editor.rs:635-643` does), and the horizontal ScrollBar thumb (`widgets.rs:248-270`). Sheet, Drawer, and Sidebar resolve side and border on the same model, so the border sits on the inner edge in both directions (`style/default/overlay.rs:144-145`, `dialog.rs:363-368`, `sidebar.rs:203-209,772-780,817-823,885-889`).

#### B8. Cursors

- Apply `ResolvedStyle.cursor` to the window at runtime: the hovered part's cursor (`Cursor::shape`, `style/resolved.rs:201`, which has no caller today).
- Add `Cursor::Pointer` (pointing hand) for link roles: Badge link, Item link rows, Breadcrumb links, and CommandLink.
- Add diagonal resize cursors for SizeGrip.
- Set resize cursors on the Resizable handle, the Dock splitter (use one handle recipe and delete the dead Dock `Handle` and `Header` recipes, `navigation.rs:429-447`), and HeaderView edges.
- Show the arrow cursor over SpinBox steppers and InputGroup buttons, not the I-beam (`form.rs:244-246`).

#### B9. Scroll chaining and scroll indicators

- A wheel event that does not change the offset returns Continue, so the outer view scrolls. This applies to ScrollView (`compose/view/layout.rs:567-568`), Carousel (`carousel.rs:304-309`), MessageScroller (`scroller.rs:281-282`), and Canvas at its pan limits.
- Show an overlay scroll bar on ItemView (`item_view.rs:1297-1316`), MessageScroller, TextEditor, and Textarea (with a maximum height). Reuse the ScrollArea bar.
- TextEditor scrolls with the wheel and keeps the caret visible (`text_editor.rs:707-755,781`). Textarea gets an optional maximum height and then scrolls (`textarea.rs:141-145,230`).
- Shift+Space pages up in ScrollView (`layout.rs:542-557`) and MessageScroller.

#### B10. Modal focus and default button (macOS convention, Q7)

- When a modal opens (Dialog, Sheet, Drawer, AlertDialog, the InputDialog modal, and the Command dialog), focus moves into it. Focus goes to the default button, or to the first content control when the content has one. When the modal closes, focus returns to the control that had focus before it opened. The `.alert_dialog()` path already does this (`present.rs:612-614,622-625`). Fix `dialogs.rs:1171-1177,1392-1395` and `dialog.rs:195`.
- Return fires the default (primary) button. Escape and Cmd+. fire the cancel button, or dismiss. This applies to Dialog, Sheet, Drawer, and AlertDialog (Enter must never fire a destructive button, as today).
- In RTL, the Tab order follows the logical order (`dialogs.rs:73-76,1047-1058,1209-1222`).
- `DialogStack`:
  - Restore focus only for the top dialog (`dialogs.rs:149-157`).
  - Move focus to the first focusable node when `initial` is `None`, and check that `initial` is inside `root` (`136-139`).
  - Cycle focus with `focus_next_within` (`192-207`).
  - Demo it in the gallery, as `catalog.rs:309` claims.
- Window and page shortcuts run after the focused control has refused the key, not before it (`compose/src/view/host.rs:1355-1363`). Return in a Textarea then inserts a line at the caret, and Escape closes an open popup before it cancels a dialog. `DialogKeys::route` reports "consumed by a disabled command" apart from "not ours" (`navigation.rs:732`). Pass the real key repeat flag, and do not fire shortcuts on repeats (`host.rs:1358`, `navigation.rs:175`).
- Escape dismisses a visible tooltip or hover card, wherever focus is (`custom.rs:740-756`).

#### B11. Key hygiene

- Keys with a Command or Control modifier fall through to shortcuts unless the control uses that chord. This applies to menus' type-ahead (`menu.rs:265,716-717`), Tabs (`hosted.rs:155-162`), RangeSlider (`range_slider.rs:206-238`), SizeGrip, CustomControl StarRating (`recipes.rs:182-191`), and Canvas.
- Shared `Press` activates Space on key up (`hosted.rs:57-60`). Repeats never toggle: ToggleGroup `hosted.rs:81`, ToolButton `tool_buttons.rs:300-307`, Questionnaire Enter `questionnaire.rs:537-540`, and KeySequenceEdit swallows every key while recording, repeats included (`key_sequence.rs:224`).
- Role Link activates on Enter only. Space is for buttons (Badge link).

#### B12. System beep

- Add `beep()` to the platform interface. On macOS it is `NSBeep` through a registered `ffi` binding in `crates/choscorui-macos/src/ffi/`, with its `SAFETY` comment and the binding checks. Other backends do nothing when it is not supported.
- A4 rejections and refused keys at bounds call it, where macOS beeps.

#### B13. IME marked text in hosted controls

- Deliver marked-text (composition) events to hosted text controls (`custom.rs:23-41`, `text_editor.rs:745-748`) and show the underlined composition inline.
- A person must check this with Vietnamese Telex and Japanese input. Automation cannot verify it.

#### B14. Live announcements

- Add an announcement path from a control to `Window::announce` (`crates/choscorui-macos/src/announce.rs:25`).
- Use it for StatusBar temporary messages (`shell.rs:584,614,688-704`), new toasts (Error at a higher priority, `toaster.rs:766-791`), the Desktop page action results, and the CustomControl and Button recipe page status lines.

### Phase C: per-control behavior bugs

Fix each item. "Gallery" items that block a working demo are in this phase. Cosmetic gallery items are in phase E.

**Text and form fields**

- **Combobox.**
  - Editable text uses the Input editing model: caret, arrows, Delete, selection, Cmd+A, and paste (`combo.rs:359-366,692-697,798-803,451-498`).
  - Typing clears the selection until a match is chosen. Blur commits a match or reverts (`396-404,351,814-816`). Escape restores the text from before the first typed character (`399-401,431-434`).
  - Show a "No results" row (`262-264`). Tab accepts the highlighted row (`186-195,427-430,833-837`).
  - A click in editable text does not toggle the popup (`740-742`).
  - Add a placeholder API.
- **FontChooser.**
  - Typed text that is not committed never drifts from the resolved family (same fix as Combobox).
  - Empty text plus Enter means "use the default" and is not "not installed" (`combo.rs:376-383,671-679`). `set_value(Index(None))` clears the text (`217-225,893-902`).
  - Merge the host invalid state with `is_missing()`, so it does not overwrite it (`951-954`).
  - `measure` does not measure every family on each layout (`769-773`).
- **DateTimeEdit.**
  - Highlight the focused section (`datetime_controls.rs:350-359`).
  - Add an AM/PM section in 12-hour locales (`99,215-227`).
  - Blur commits and clamps (`990-993`). Stepping stops at min and max (`231-243`). Update the gallery test that pins the old behavior.
  - A click selects a section (`953-967`). Show pending digits, and let Backspace clear a section (`255-317`). Clear `typed` on Home, End, and blur.
  - Enter returns Continue when nothing changes. Escape reverts.
- **KeySequenceEdit.**
  - Commit a sequence shorter than `max_chords` after a pause timer and on Return when at least one chord is recorded. Escape cancels. Return and Tab are not recordable as the first key of a chord (`key_sequence.rs:87-94,224`).
  - Show a recording prompt (`148-153`).
  - The first chord needs a modifier, except F-keys (`75-88`).
  - Add a placeholder to the compose view (`widgets_form.rs:218-228`) and an optional clear button.
  - Suspected: menu key equivalents run while recording. Confirm it, then make recording take keys first.
- **InputDialog (Q5).**
  - Make it a modal panel in the dialog stack, with OK and Cancel. Escape cancels and focus returns (`compose/view/dialogs.rs:99-136`).
  - The Choice kind uses a Select (`controls/dialogs.rs:898`).
  - Text returns the trimmed text it checked (`858-865`).
  - The gallery readout formats the value for the user, not with `Debug` (`gallery dialogs.rs:182,450`).
  - Hide the duplicate visible error from accessibility, so it is not read twice.
- **FontDialog.**
  - Carry the slant through `TextRun` and `DrawText` to the backend (`core/src/display.rs:353-359,596-604`, `crates/choscorui-macos/src/shaping.rs:180`), so Italic shows.
  - Measure the preview with the chosen family (`compose dialogs.rs:342-361`). Give the preview a maximum height and wrap it.
  - Add a localized family label parameter (`154-161`).
  - Bold off restores the previous weight (`288-300`). The size field keeps fractional sizes, or uses a DecimalSpinBox.
  - Let the control row wrap.
- **ColorPicker.**
  - Paint the documented `ColorPicker` Root, Track, and Label slots with the focus ring, or remove those doc rows (`dialogs.rs:658-670`, `data.rs:467-510`).
  - Paint the H, S, and V strips opaque, and put a checkerboard under alpha and the swatch.
  - `set_hex` accepts only hex digits and at most one `#` (`424-426`). Accept `#RGBA` (`450-452`).
  - Gallery readouts come from the channel values (`gallery dialogs.rs:252-257`). Recent swatches are named buttons that apply their color. Delete the unused hosted `recent` list (`dialogs.rs:567-571`), together with its last caller.
- **CellEdit (gallery products page).**
  - Show the current cell (`products.rs:224-266`). A click picks the column.
  - The editor is placed over the cell.
  - A row change or blur commits valid text or keeps the pending text (`285-288`).
  - Show specific inline rejection messages (`model_widgets.rs:378-396`).
  - The table and the editor use one locale number format (`products.rs:43-45`). Count fraction digits after either separator (`model_widgets.rs:357-360`).
  - Change the hint text "Editing column" to a selected-column hint while not editing.
- **DataMapper.**
  - Readable localized errors (`model_widgets.rs:471-474`). Locale number formatting when the form fills (`438`).
  - Mark only the edited field as unsaved (`products.rs:495-498,539-545`).
  - The list starts with the bound row selected. Use single selection. Keep errors with their pending text, and check them again on edit.
  - Drop a pending value that equals the model value. Trim names.
- **InputGroup.**
  - `semantics()` appends the addon text, so the validation message stays (`input_group.rs:369-379`).
  - Button addons are focusable and are their own accessibility nodes (`368-381`).
  - The hit target is the addon button, not the slot.
  - The gallery Copy, Clear, and Send buttons work (Copy uses B1). The invalid demo shows an error through Field.
- **NativeSelect.** Add a pop-up button role to core and use it (`select.rs:395`). The docs say that it is the toolkit list with a native-style trigger. The semantic value is empty when nothing is selected (`select.rs:396`).
- **Select.** The semantic value is empty when nothing is selected (`select.rs:396`). Only a primary-button release chooses (`listbox.rs:272`). Type-ahead starts from the current selection (`select.rs:278-293`).
- **Checkbox.** The hit area is the box plus the label, not the whole 320 pt frame (`checkbox.rs:189,197`; gallery `inputs.rs:126`).
- **Switch.** Honor `with_size(Lg)`, or restrict the size type (`switch.rs:49`). `perform(Focus)` returns false when disabled (`204-207`). The same applies to Toggle (`toggle.rs:140-144`).
- **Toggle.** Allow an icon-only toggle with a separate accessible label (`toggle.rs:97-103,145`). Use one role model for toggles: pressed state on a button role, also inside ButtonGroup (`controls.md:485`).
- **ToggleGroup.** Roving focus starts at the selected item, or else at the first enabled item (`toggle.rs:251,305-314,364-367`).
- **ButtonGroup.**
  - In an exclusive group, focus starts on the checked member, and arrows step from the focused member (`button_group.rs:222,257-261,572`). Exclusive members have the RadioButton role (`643-648`).
  - Disabled members are skipped by keys and pointer (`311-315,394-403,526-533`). Arrows reach Button members, and Home and End work in exclusive mode.
  - Add a size API. Use one border token for joined parts (`form.rs:146,510`).
- **Slider.**
  - Pointer and SetValue snap to `step` (`range_controls.rs:113-118,261-299,882-889`).
  - PageUp and PageDown use `page_step` (`278-293`). Clamp the thumb and the pointer mapping to the track (`262-271,409-414`).
- **RangeSlider.**
  - When the thumbs meet, pick the thumb from the drag direction (`range_slider.rs`). Clamp the thumbs inside the bounds (`299-304,180`).
  - `set_enabled(false)` clears the drag (`344-346`). Hover goes on the thumb under the pointer (`296`).
  - Round to the step decimals (`81-82,320-321`).
- **SpinBox.** PageUp and PageDown (`range_controls.rs:481-497`). The steppers auto-repeat, and they dim at the bounds. Arrow Left and Right are not swallowed (`774-778`). The value is formatted with the locale (`550,951`).
- **Dial.** Draw a wrapping dial on a span+1 scale (`widgets.rs:738-740,799-806`). Keep a drag at the end it left (`741-753`). Page steps wrap on a wrapping dial (`781-786`). Hit-test the circle only (`972`). Snap to the step (`755-756`). Show the pressed state during a drag (`1045-1048`).
- **ScrollBar.** Make the bar focusable so its keys work, or remove the key contract from the docs (`widgets.rs:163-176,328-336,378-380`). `perform` returns false at the limit (`359-376`). Holding on the track repeats paging (`140-150`). Add a disabled look and semantics (`382-386,192-205`).
- **NumericDisplay.** The gallery uses the public read-only `NumericDisplay` view. Put the interactive counter on a separate demo that has correct adjustable semantics (`gallery range.rs:329-380`). Start at a value that fits every base. Keep i64 values exact (`widgets_form.rs:287`).
- **Progress.** Animate the determinate fill over the recipe `motion_ms` (`range_controls.rs:660-698`). Expose a numeric value with min and max (`707-709`, `accessibility_bridge.rs:431-432`). Show a non-value pattern for indeterminate with reduced motion (`676-679`).

**Buttons and commands**

- **ToolButton (Q5).** The menu opens as a real menu (`tool_buttons.rs:255-258,314-320`) and closes on Escape, an outside press, and focus loss (`295-312`). Paint the pressed and open states (`318-319`; recipe `form.rs:513-528`). Publish `expanded` (`117-121`). A disabled command never opens its menu (`77-83,90-94`; change `docs/controls.md:436`). The pressed segment is tracked, so a press on the arrow that ends on the label does nothing. The gallery reports open and close.
- **CommandLink.** Show pressed feedback from the inner Button state, and delete the duplicate `pressed` flag (`tool_buttons.rs:446-490`, `layout.rs:238-240`). Place the description with its own line height (`416-423`). The docs say that it resolves through `ControlKind::Item`.
- **Spinner.** Add a size API and a `currentColor`-style foreground option (`feedback.rs:151-157`, `loading.rs:35-48`). Fix the recipe comment about reduced motion (`feedback.rs:154-155`). Read the motion at paint time (`loading.rs`).
- **Button.** Optional loading state with a spinner. This is phase E.

**Overlays and menus**

- **Drawer.** A drag that dismisses continues the close animation from the dragged offset (`dialog.rs:189,294-296,528-529`; `hosted.rs:167-172`). A short drag springs back with animation. Release pointer capture on Escape and Dismiss during a drag (`189-190`). Add a velocity flick and scale the scrim with the drag. Clip or scroll content over the 80% cap (`359-361,385`).
- **Dialog and Sheet.** Clamp the panel height to the viewport and scroll the body (`dialog.rs:321-324`). Drop `expanded` from the open dialog node, and mark it modal (`557-560`). The docs say that top and bottom sheets are sized to their content up to 80%, and they give the scrim timing (`controls.md:2707-2710`).
- **AlertDialog.** The Help button does not close the alert (`dialogs.rs:1189-1194`). The gallery uses a localized help label (`gallery dialogs.rs:41`). The button row stacks when it is too narrow (`1050-1063`). The doc `Media` slot and `size sm` are implemented or removed (`controls.md:1830`).
- **Popover and hover card.**
  - The hover card opens on focus-visible after the delay and closes on blur, as the compose modifier does (`popover.rs:162-176`, `hover_card.rs:217-228`). Both versions have one behavior.
  - A disabled control ignores Hover, so no stale deadline stays (`popover.rs:118-125`, `custom.rs:557-563`).
  - Limit the panel width to the window (`style/default/overlay.rs:186-187`). Add trigger sizes.
- **Tooltip.** The gallery uses `.tooltip()` and deletes `TipButton` (`gallery popups.rs:150-284`). Focus opens without the 600 ms delay. Focus and hover do not hide each other's tooltip (`tooltip.rs:101-105,164-166,338-349`). The bubble is not a hit-test target (`custom.rs:590-607`). Add a side option. Paint the documented arrow radius, or remove it from the docs.
- **DropdownMenu, ContextMenu, and Menubar.**
  - A keyboard open highlights the first row (`compose/view/menus.rs:252`). Pass the real activation source, and also from Breadcrumb (`breadcrumb.rs:135`).
  - ArrowLeft at the root does nothing (`menu.rs:257-262`).
  - Hovering a separator, a disabled row, or outside clears the highlight (`376-383`). Add a submenu grace delay or pointer cone, and do not reset an open submenu (`383-388`).
  - Use a prefix type-ahead from the listbox buffer (`265-281`).
  - Add a `MenuItem::Label` group heading and optional leading icons (`menu.rs:29-44`).
  - The compose ContextMenu takes initial check and disabled states and bindings (`compose widgets.rs:473-484`).
  - Menubar titles show hover (`menus.rs:548-560`). The ContextMenu area dims when disabled and uses a dashed border (`overlay.rs:285-294`).
- **NavigationMenu.**
  - A panel switch by hover resets link focus to the trigger (`navigation_menu.rs:183-190,379-384`).
  - After a click or Escape close, hover does not reopen the panel until the pointer leaves the trigger (`244-245,379-387`).
  - Grid arrows in two-column panels, plus Home and End (`134-171,293-302`).
  - Top-level links paint with the Trigger style (`409-413,444-455`). Add an optional current item.
  - Take the panel offset and column width from tokens (`21,154`).
- **Command.**
  - A command runs only when press and release land on the same item (`command.rs:426-434`).
  - Paint, hit-testing, and lookup are bounded by the visible rows (`208-212,240,313-323,540-575`).
  - `Dismiss` on the inline form returns false and emits nothing (`814-818`).
  - Home, End, PageUp, and PageDown move the highlight (`397-410`). The dialog top edge does not move while typing (`643-650`). Hover refreshes semantics (`436-443`).
  - The icon and inline widths come from tokens.
- **Toast.**
  - Escape closes the focused toast (`toaster.rs:332-351`).
  - `Dismissed` and auto-close carry the `ToastId` (`252-253,228-233`).
  - A variant change after Loading restores the duration (`mod.rs:53-59`).
  - The close hit target is at least 20 pt.
  - Timers pause while the window is inactive or hidden, if the window activity input reaches the control. Confirm this first.

**Navigation and layout**

- **Tabs.** Arrows follow the orientation (`selection_controls.rs:665-670`). The semantic value is not "true"/"false" (`611`). The panel has the TabPanel role.
- **Breadcrumb.** A click on the trail closes an open ellipsis menu (B3). Pressing the ellipsis again closes it. ArrowDown opens the menu. With one item, do not advertise Activate or Focus (`breadcrumb.rs:408-449`). Collapse or truncate on overflow.
- **Pagination.** Increment and Decrement fail in the same way at the bounds (`pagination.rs:418-425,189`). Zero pages is an empty, unfocusable state (`67,396`). Slots grow to fit the number (`477,226-228`). Clear the hovered index when the slots change.
- **Sidebar.**
  - Center the icon rail for Floating and Inset (`navigation.rs:378-390`). Show labels as tooltips on the icon rail (`sidebar.rs:734-736`).
  - Activating a parent keeps focus on it (`483,510-515`). Cmd+B returns Continue when the sidebar cannot collapse (`575-578,612`).
  - Mark the parent of a hidden selected sub-item. `with_selected` validates its input.
- **Accordion.** Animate the height on open and close (`accordion.rs:99-106,293-307`). Add a gap between title and chevron. The last item has no bottom border. Per-item disabled. Only a primary-button release toggles (`174-190`).
- **Collapsible.** Close fades as open does, or the docs say that only open fades (`collapsible.rs:75-81,185-193`). The gallery uses the plural form for one item.
- **Carousel.** A wheel during the animation starts from the shown position (`carousel.rs:180-182`). A flick moves to the next slide (`203-207`). Add Home and End.
- **Resizable.** One accessible name per handle (`containers.rs:952-960`). Collapse with Enter and double-click (`888-912`). Escape cancels a drag. Distribute size changes in proportion (`745-763`). Fix the gallery "Console height" value (`gallery containers.rs:116-121`).
- **Dock (Q5).**
  - Add tab drag to another area or out to float, with drop targets. Add a tab close button and Cmd+W for closable panels (`dock.rs:1034-1061,1173`).
  - Paint, hit-test, focus, and expose floating panels (`543,551-620,916-928`).
  - Clamp extents to what arrange can show, and report that value (`437-441,459,497,636,1168`).
  - Save the area and group of hidden panels (`698-700,789`).
  - Tab stops follow reading order (`481-486,916`). Escape cancels a splitter drag. End goes to the maximum.
  - The compose `Dock` places panel content in slots, as the docs say, or the docs change (`controls.md:2045-2048`).
- **Toolbar.**
  - The overflow chevron is the last roving stop. Enter, Space, and ArrowDown open it (`shell.rs:350-355,1001-1021`).
  - Embedded controls get real focus (`115,449,381-387`), and they appear in the overflow menu (`1004-1015`).
  - Take the item geometry from the recipe. The model default thickness is 40 pt (`150-152,265-273`). Add icon support to `ToolbarItem` (`69-79`).
  - An empty toolbar, or one with no enabled item, is not a Tab stop.
  - The docs say that focus returns to the last focused item (`controls.md:1955`).
- **StatusBar.** Take the height (24 pt) and gap from the recipe (`shell.rs:548-552,618-650,1123-1125`). Remove the Focus action (`702,1150-1164`).
- **Main window.** The gallery page uses `compose_main_window` frames (`gallery shell.rs:374-438`). Add direction-aware Start and End toolbar areas, or a direction parameter (`shell.rs:721-725`).
- **AspectRatio.** Fit inside both width and height bounds (`aspect_ratio.rs:60-72`). Wrap any child view, not only an image (`13-40`). An empty placeholder is not announced as an image. Clip with the full corner radii (`83`).
- **Separator.** A vertical `Divider` does not stretch its HStack to the offered height (`compose layout.rs:384`). Add an explicit orientation. Snap to pixels.
- **Skeleton.** A zero width in an unbounded context fills or uses a minimum, as the docs say (`loading.rs:127-132`). Skeletons are hidden from accessibility, and the container is busy.
- **GroupBox.** Add a compose view. `measure` uses the title width and the member sizes (`containers.rs:486-489`). Members are children, not `Controls` relations (`360-366`). Add an optional disabled state that reaches the members.
- **Stacked.** The gallery selector is a Tabs control (`gallery containers.rs`). The hosted node holds the page or relates to it (`containers.rs:1288,1384-1393`). The value is the page title or "n of N".
- **SizeGrip.** The gallery uses the real control, and its stand-in `MockWindow` is deleted (`grip.rs:24-198`). Escape cancels a drag (`widgets.rs:1739-1742`).
- **Label.** The gallery uses the real `Label` control, not styled `Text`. Add a target ("for") API, so a click focuses or toggles the target.
- **FocusFrame.** The frame and the control do not both draw rings. The frame follows the control shape and state (`widgets.rs:1588-1609`). Use the focused node's tokens and transform.
- **Kbd.** Add a compose group API. Name keys for accessibility (Command, not "⌘"). Report the baseline. The gallery shows Inverted on a dark surface or in a tooltip, where it is visible.
- **Mnemonics.** The gallery uses `MnemonicTable`. Expose the mnemonic position, so labels can underline it (`navigation.rs:602-622`). Keep a single `&` before a character that is not alphanumeric. Fix the "F" prompt (`en.lang:569,574`).

**Data and collections**

- **DataTable.**
  - The selected count counts only rows inside the filter (`mod.rs:795`, `layout.rs:434`).
  - A sort change returns to page 1 (`mod.rs:324-336`).
  - Show a neutral sort icon on sortable headers. Clear the sort when its column is hidden.
  - Keep one data column visible (`419-427`).
  - Numeric columns align to the end and use locale formatting. The status line uses plural forms.
  - A click on an empty area does not move internal focus (`740-748`).
- **HeaderView.** Sort on release inside the same column (`model_widgets.rs:139-145`). Add a third "none" sort step, as DataTable has. Home and End. Escape restores the width during a resize. Pushing widths does not rebuild the ItemView.
- **ItemView.** Flat models reserve no disclosure gap (`item_view.rs:1023-1024`). A search or layout change keeps the scroll position and scrolls the selection into view (`data.rs:60-64,164-177`). Moving focus from the search field to the list shows a current row. Show an empty "No results" state. A click on empty space clears the selection. Space does not reach type-ahead.
- **ColumnBrowser.** Vertical scrolling with the wheel and a scroll bar. Horizontal scrolling that keeps the focused column visible (`model_widgets.rs:845-858,885-894,916`). Lazy columns call `fetch_more` (`519,984`). Home, End, PageUp, PageDown, type-select, and activation (`586-636,931-935`). `Changed` only when the path changes (`875-877`). Column dividers.
- **Calendar.** The previous and next buttons respect the range, disable at the limits, and are Tab stops (`datetime_controls.rs:445-456,701-704,794-797,1074-1085,1228-1233`). Paging moves the cursor into the shown month (`517-520,1300-1302`). In Single mode, arrows move focus, and Return or a click chooses (`1309-1320`). The gallery dropdown-caption demo is range-bounded. Range mode extends a range from an earlier day.
- **DatePicker.** The trigger honors its width and truncates. Multiple mode shows a count summary (`date_picker.rs:338`). Clearing a date is possible (`173-180,241-296`). Add min, max, and disabled-day APIs (`dtc:615`, `widgets.rs:427-455`). ArrowUp and ArrowDown open the picker.
- **Canvas.**
  - Tab and Shift+Tab leave the canvas at the ends. Item cycling uses arrow keys or a modifier (`canvas.rs:366-395,764`).
  - Wire pinch to zoom through a gesture `ControlEvent` (`custom.rs:23-41`, `core/input.rs:110`).
  - A click on one item of a multi-selection without a drag selects only that item (`canvas.rs:244-253,310-313`).
  - Tab, semantics, and outline work are bounded by the view (`368-383,486-506`).
  - Escape cancels a drag. Add a drag threshold, Cmd+A, and `ensure_visible` after arrow nudges.
- **Chart.**
  - The tooltip hides when the pointer leaves, even with focus (`mod.rs:262-272`). Blur and Escape clear the highlight (`209-211,274-276`).
  - A radar chart with fewer than 3 categories shows a fallback (`plot.rs:551-553`). Negative values get a baseline (`206-222,579,623`).
  - Use the locale default number format (`mod.rs:95,134-142`).
- **Attachment.** Cover-fit images (`attachment.rs:258-263`). The progress bar has a muted track, and the space is reserved only while uploading. Put the vertical remove button on a visible surface.
- **Avatar.** Cover-fit images (`avatar.rs:74-77`). `AvatarGroup::new` sets each avatar size. Restrict the sizes. Center the avatar in larger bounds.
- **Badge.** The Ghost and Link variants have a demo as real links (gallery `display.rs:49-64`). Use a non-loader icon for "Syncing".
- **Card.** Footer buttons stack or wrap when they are wider than the card (`panel.rs:139-145`). A Sm card uses Sm buttons.
- **Empty.** The button row wraps inside the 384 pt content width (`empty.rs:181-192`).
- **Item.** `ItemActivated` carries a stable (row, action) identity (`item.rs:344-357,402-406`). A link row ignores `with_action`.
- **Message.** The height includes the avatar (`chat/message.rs`). The reactions pill stays inside the content area (`235-247`).
- **MessageScroller.** Anchor to the first visible message while heights are measured again (`scroller.rs:15,185-193`). Adding a message does not rebuild the control and keeps the offset and `stuck` (compose view). Bubble hover. Cmd+Up and Cmd+Down. Even padding.
- **Questionnaire.** A single-choice question uses a radio indicator, and its arrows select (`chat.rs:355-372`, `questionnaire.rs:555-563`). A rebuild keeps the answers (`Current::restore`). The error text is in the description.
- **UndoHistory (gallery).** After trimming, row 0 is not called "Start (0)" (`undo.rs:131-133`, `model_widgets.rs:655`). Undo and Redo disable at the ends. Actions that change nothing do not push a row. Use checked arithmetic. Show the rows in an ItemView, as the docs say.
- **CustomControl (StarRating gallery).** A click or key shows its result at once (`recipes.rs:167-210`). It dims when disabled. It mirrors in RTL. Digits other than 0-5 are ignored. Bind the value and show it.
- **Desktop (gallery).** Post the notification after permission is granted, and report errors (`gallery app.rs:78-83`, `desktop.rs:479-516`). The status item has a menu and a tooltip. The toggle shows the real state. Drain the `AppEvent` queue (`desktop.rs:93-101`). Fix the "needs {condition}" grammar. Show the reason of a disabled button as a tooltip.

### Phase D: accessibility child nodes

- Add child semantic nodes (core `semantics.rs` already supports children). The focused part is the focused child, not a `description` or `value` on the parent. Add the roles that are missing to core and to the macOS bridge: MenuBar, MenuBarItem, PopUpButton, and a switch subrole if AppKit has one.
- Controls and their children:
  - Menus: DropdownMenu, ContextMenu, and Menubar items, with checked, disabled, and expanded states. `DropdownMenu::semantics(level)` exists and has no caller (`menu.rs:404`).
  - Dialog, Sheet, Drawer, AlertDialog, Card, Alert, Empty, Item, and Toast buttons. Alert also conveys the destructive variant.
  - Toolbar items, Tabs (hosted) tabs, Accordion headers, Collapsible content, and Carousel slide position ("n of N") plus buttons.
  - Pagination pages, with the current page, and Breadcrumb links, with the current page.
  - Select, Combobox, and Command options, as an active descendant. Calendar and DatePicker day cells, with today.
  - DataTable rows and cells, with selected and sort states. ItemView rows. ColumnBrowser columns and items.
  - NavigationMenu triggers and links, Sidebar items, Dock tabs and splitters, ColorPicker channels as sliders, RangeSlider thumbs (low and high names), and InputGroup button addons.
  - MessageScroller messages, Message reactions, Questionnaire options (radio or checkbox), Canvas visible items, Chart data points, AvatarGroup members, HeaderView columns (no `selected` for focus), GroupBox members, and ButtonGroup with a group name (compose `ButtonGroup::new` takes a name).
- Publish min, max, and step for Slider, RangeSlider, Dial, SpinBox, Dock splitters, Progress, and the CustomControl demo. Remove the "Range {min} to {max}" description strings.
- Bound child publishing by the visible range for large models (ItemView, DataTable, Canvas, Command), so semantics do not scale with the whole model.
- A person must verify with VoiceOver. Automation checks the semantic tree only.

### Phase E: polish and gallery

Low-severity items from the reviews that phases A to D do not cover:

- **Every page with a missing disabled demo.** Add a disabled example: Switch, Toggle, Accordion, Alert, Attachment, Breadcrumb, Calendar, Carousel, ColorPicker, Command, ContextMenu, CustomControl, DatePicker, DecimalSpinBox, Dock, Empty, FontChooser, FontDialog, InputGroup, InputOtp, Item, KeySequenceEdit, Kbd, Menubar, NavigationMenu, Pagination, Questionnaire, RangeSlider, Sidebar, Spinner, TextEditor, Textarea, ToolBox, ToolButton, ButtonRecipes, and Tooltip trigger.
- **String keys reused across pages.** Each page gets its own keys in en and vi: `select.invalid` (Switch, Textarea, InputOtp), `slider.disabled` (SpinBox), `toggle-group.vertical` (RangeSlider), `breadcrumb.components` (NavigationMenu), and `number-input.valid` (DateInput). Move hard-coded gallery literals into the catalogs: the "Pedro Duarte" and "Colm Tuite" radios, "Ana Lee"/"AL", the message header time, and the alert "?". Use plural forms where a count appears: `row(s)`, "repositories".
- **Demos that do nothing or mislead.**
  - Command readout, Alert "Restart", Empty actions, Card actions, Toast Undo, and Tooltip Bold/Italic/Underline: show a visible effect.
  - Stacked "Reset" and the hover-card link: wire them, or remove them.
  - Group-box options become RadioGroup and Checkbox. The Menubar "View" menu gets its own items. The Dialog, Sheet, Drawer, and Popover demos use the compose modifiers with real fields (Q5).
  - AspectRatio uses a real landscape image. Skeleton stacks in a profile and card layout, with the second card line.
  - Spinner demos show visible labels. DateTimeEdit labels are correct. The Desktop capability statuses use Badge variants.
  - The main-window content-size label is correct, and the page shows the bottom, right, and stacked toolbars.
- **Pages that use stand-ins.** Label, GroupBox, Stacked, SizeGrip, Mnemonics, NumericDisplay, InputMask, HeaderView (`compose::view::HeaderView`), ColumnBrowser (`ColumnBrowser::new`), Questionnaire (public view with `current`), DropdownMenu (`.dropdown_menu`), and KeySequenceEdit (typed view): switch to the public API. Delete the stand-ins together with their last caller.
- **Doc and code mismatches.** Fix each mismatch the reviews list:
  - Empty Root gap, Item dead gaps, Marker line gap and Border slot, Message Footer and Avatar slots and pill side
  - Progress Label, Value, and Root slots; Dial Value slot and `point_to`; Resizable grip slot; Separator node wording; ScrollBar capture wording
  - TextEditor pointer capture; DateTimeEdit actions; Badge hover values; AlertDialog fallback
  - HoverCard Escape and focus behavior; DecimalSpinBox, Dial, and ScrollBar "no disabled state"; NumericDisplay focus ring; Label vertical placement
  - The stale `PillButtonRecipe` and `CommandLinkRecipe` names in `docs/specs/2026-10-12-component-gallery.md:50`. Edit a dated spec only to add a one-line note that points to this spec.
- **Remaining low items.** Implement each remaining `[low]` finding from the reviews when it is a behavior bug. Examples:
  - Avatar out-of-bounds rings, CellEdit Date editor, Chart value axis option
  - Kbd icon content, Marker empty-label separator, Message hover on static bubbles, Separator pixel snapping
  - TextEditor inactive selection color, Textarea inactive selection, Toast hidden-window timers
  - DataTable PageUp/PageDown/Cmd+A/Shift-click, Canvas rubber band intersect mode
  - ScrollView auto-hide bar, overlay scroll bar corner, Progress compose live `Option<u32>`
  - Toast placement option, Badge cursor, Button loading state, and a Spinner in Button
  - When an item is a design choice and not a bug (for example the Progress track height), record the choice in the docs and do not change the code.

## Acceptance criteria and public test seams

| Criterion | Observable seam |
| --- | --- |
| Button, Switch, Toggle, ToggleGroup, Badge, and NumericDisplay have no invalid state | `cargo test -p choscorui-controls` compiles with no `set_invalid` on these types. `gallery/src/ui/pages/variants.rs` expects no INVALID on their pages. The goldens `painted_form.txt` and `painted_output.txt` have no invalid blocks for them after the bless. |
| DatePicker keeps its invalid look after A1 | Style test that resolves the DatePicker trigger with INVALID and gets a destructive border. Golden unchanged for DatePicker. |
| Checkbox and Radio invalid: destructive border and label, no ring, no shadow, checked too | Style tests in `crates/choscorui-controls/tests/styles_form.rs` for unchecked, checked, and focused-invalid states, in both themes. `painted_form.txt` blessed. |
| Invalid fields keep a visible focus | Style test: Input with INVALID and FOCUS_VISIBLE resolves to `focus_ring()` and a destructive border. |
| A Field error shows visually | `compose/tests` test: a Field with an error gives its Input the INVALID visual state and a destructive label, and exposes the help and the error as descriptions. |
| Numeric, date, mask, and OTP fields reject bad input at typing time | Unit tests of the filter rules in `controls` for keys, paste, IME commit, and `SetValue`. Gallery page tests: typing `12abc` into NumberInput leaves `12`; the InputMask stores the masked value; `>` uppercases or is gone from the demo. |
| A rejected edit beeps | Fake backend records `beep()` calls in a hosted-control test. |
| Input `max_length` and filters | `compose/tests/control_views.rs` test: a paste over the limit is cut, and a filtered character is refused. |
| SpinBox can be typed into | Gallery `range.rs` test types `87` and Return and gets 87. |
| Validation messages are localized | Test with locale `vi` gets the Vietnamese message from the built-in catalog. An application override replaces one message id. |
| Clipboard works in text controls | Hosted-control tests with a fake clipboard for Cmd+C, Cmd+X, and Cmd+V in Input, Textarea, TextEditor, and Combobox. Password blocks copy. `clipboard_smoke` native test still passes. |
| Double and triple click select a word and a line | Hosted Textarea and TextEditor tests with a click count of 2 and 3. |
| macOS editing keys | TextField key tests for Cmd+Left/Right, Cmd+Backspace, Option+Backspace, and Shift variants. |
| Popups flip and clamp | Unit tests of the placement function at each window edge. Listbox and menu tests near the bottom and right edges. |
| An outside press that closes a popup does not reach the control below | `custom.rs` host test: a press outside an open Select dismisses it, and the Button below gets no activation. |
| Closing without a choice emits `Dismissed` | Popover, ContextMenu, and Select tests for an outside press. |
| Text truncates with an ellipsis | Paint-output tests (`painted_output.txt` style) for a long Select value, Sidebar label, and DataTable cell. Tooltip and AlertDialog tests: long text wraps and the measured height grows. |
| Inner buttons show hover, pressed only while inside, and disabled | Paint-state tests for `overlay/panel.rs` Buttons with Move, drag-off, and disabled host state. |
| Focus rings appear on every focusable part | Paint tests for Toast close, Breadcrumb ellipsis, Chart, ScrollView, and ColumnBrowser with FOCUS_VISIBLE. |
| RTL arrows follow the visual direction | Key tests in RTL for Slider, Tabs, Calendar, and menu submenus. |
| Style cursors reach the window | Host test with a fake backend that records the cursor shape over a Resizable handle and a link Badge. |
| Wheel at a limit chains to the parent | ScrollView test: a wheel at the end returns Continue. |
| Modals move and return focus, Return fires the default | AlertDialog and Dialog tests: focus moves to the default button on open and returns to the trigger on close. Return activates the primary button. Cmd+. cancels. |
| Shortcuts run after the focused control | Gallery dialog-keys test: Return in the Textarea inserts a line at the caret, and the dialog stays open. |
| Per-control fixes in phase C | One regression test for each item, at the control's public API or its gallery page test. Examples: Drawer close starts at the dragged offset; DataTable count with a filter; Toolbar chevron is a roving stop; Dock floating panel is hit-testable; FontDialog italic reaches `DrawText`; Canvas Tab leaves; ToolButton menu opens and closes. |
| Accessibility children | `control_semantics.txt` golden (blessed) shows child nodes with roles and states for each control in phase D. VoiceOver checked by a person. |
| Large-model work stays bounded | `choscorui-bench --check bench/budgets.txt` passes. Add bench cases for Command with 10,000 items, Canvas semantics, and ItemView semantics with children. |
| Gallery pages use the real controls and have disabled demos | Gallery page tests that find the public control types and a disabled instance on each listed page. |
| Docs match | `scripts/check-docs.sh` passes. `docs/controls.md` has no row that describes behavior the code lacks. |

## Constraints and risks

- **Checks.** Each Rust commit runs the CI `check` job commands from `CONTRIBUTING.md` (section "Before submitting a maintainer change"). Each backend or UI commit also runs the CI `native` job commands. Markdown changes run `scripts/check-docs.sh`. Fix the findings, and do not add suppressions. Run native smoke tests, the gallery, and autopilot one at a time.
- **Goldens.** Phases A, B4, B5, B6, and D change goldens in `crates/choscorui/tests/golden/`. Bless with `CHOSCORUI_BLESS=1 cargo test -p choscorui --test compatibility` only for these intended changes. Review the diff, and name each bless in the commit and the PR.
- **Human-only checks.** Automation cannot verify VoiceOver output (phase D, B14), real IME composition (B13, Vietnamese Telex and Japanese), notarized clean installs, Intel hardware, or budgets off the reference machine. Do not claim them. List them in the PR as checks a person must do.
- **FFI.** `beep()` (B12) and any new accessibility roles (D) need macOS bindings. Put them only in `crates/choscorui-macos/src/ffi/`, as registered descriptors with `SAFETY` comments. `cargo test -p choscorui-macos` checks them against the runtime and the SDK headers. Do not weaken the check. Record any new callback boundary in `docs/ffi.md`.
- **Public API changes.** Removing `set_invalid` from Button, Switch, Toggle, and ToggleGroup, the typed validation errors, `Cursor::Pointer`, new control events (click count, gesture, marked text), and the new roles are breaking changes. Note them in the changelog if the repository keeps one. Delete removed code together with its last caller.
- **Performance.** B4 (truncation measures), B5 (hover tracking), and D (child nodes) add work to paint and semantics. Keep each bounded by the visible parts. Run the bench check.
- **Gallery size.** The gallery binary is about 3 KB under 3 MiB, and growth fails in 16 KiB page steps. New demos and the built-in catalog may cross the limit. Measure after each phase. If the gallery crosses the limit, ask the maintainer before you change the budget.
- **Languages.** The gallery ships English and Vietnamese only. The built-in catalog ships the same two. Ask the maintainer before you add a language.
- **Dock drag and floating (Q5)** is the largest single item. It may need its own design note under `docs/specs/`. Keep the dock model API stable where it can be.
- **Open evidence labelled "suspected" in the reviews** must be confirmed before a fix. Examples: the hover-card stale deadline busy-wake, the menu key equivalents during KeySequenceEdit recording, Toast timers while the window is hidden, ItemView scroll reset on rebuild, and the Space type-ahead conflict. When a suspicion is false, record that in the PR and skip the fix.
- **Review evidence** came from static reading at commit `4d75722`. Line numbers can move. Search for the named function or string when a line number no longer matches.
- **Reference design-system name.** It appears in this spec only. Do not write it in code, tests, docs, or commit messages outside `docs/specs/`.

Read this whole spec and inspect the current workspace before implementing.
