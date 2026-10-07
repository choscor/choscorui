# Control behavior contract

This document fixes how each control responds to input: pointer, keyboard,
focus, disabled state, value changes, and cancellation. The control owns
this behavior. Visuals are separate: styles and recipes read a visual state
and paint it, so they can be swapped without changing behavior (see
[THEMING.md](THEMING.md)). Roles, names, states, and actions are in the
[control contracts table](ACCESSIBILITY.md#control-contracts).

Behavior here is public API. Changing it is a breaking change. It is pinned
by `crates/choscorui/tests/compatibility.rs` and by the tests named after
each section (`controls/tests/` is `crates/choscorui-controls/tests/`).

## Shared rules

These apply to controls whose `handle` takes the retained `Tree` (Button,
Checkbox, RadioGroup, TabSet, TextField, TextEditor, Slider, SpinBox,
ScrollArea, Splitter), unless a section says otherwise.

- **Missing node.** Button, Checkbox, TextField, TextEditor, Slider,
  SpinBox, and ScrollArea return a missing-node error and change nothing.
- **Primary button only.** Pointer presses act only for
  `PointerButton::Primary`.
- **Keys need focus.** Key events act only when the control's node (for
  Splitter, its handle) has focus. Semantic actions and external setters
  do not need focus.
- **Change reporting.** `handle` reports a change only when the value
  changed. Re-selecting the current option or stepping past a bound
  reports nothing.
- **Invalidation.** Value changes invalidate `PAINT` and `SEMANTICS`, not
  `LAYOUT`. Hover and press changes invalidate `PAINT`. TextEditor edits
  also invalidate `LAYOUT`.
- **Disabled.** A disabled control ignores input. `set_enabled(false)`
  cancels an outstanding press or drag and releases pointer capture
  (Button, Checkbox, Slider, ScrollArea). Text controls drop IME
  composition. RadioGroup and TabSet disable single options instead.
- **Cancel.** `Cancel` clears armed presses and drags and releases capture.
  It never activates and never restores an earlier value.
- **Commands.** A disabled command never activates. If a shortcut resolves
  to a disabled command, it still wins over less-specific bindings.

Other controls (ComboBox, PopupMenu, Toolbar, ToolBox, ToolButton, Dial,
ScrollBar, DecimalSpinBox, DateTimeEdit, Calendar, HeaderView, CanvasView,
KeySequenceEdit, and ItemView keys) take a plain `Key` with no down/up. The
host delivers it (normally on key-down) and checks focus. `Label`,
`Separator`, and `ProgressBar` take no input.

### Button

| Input | Behavior |
| --- | --- |
| Primary press on the node | Takes focus, captures the pointer, arms. |
| Pointer move while armed | `pressed` shows only while inside. |
| Primary release | Activates only with capture and inside. Outside cancels; moving back in before release activates. |
| Enter key-down | Activates at once. Repeats ignored. |
| Space key-down / key-up | Down arms (repeats ignored). Up activates if armed and still focused. |
| `Semantic(Activate)` | Activates. |
| `Cancel` | Disarms pointer and Space, releases capture. |

- Activation returns the button's `CommandId`.
- `handle_with_commands` syncs enabled state first, so a command disabled
  mid-press cancels the press.

Pinned by `controls/tests/button.rs`, `controls/tests/commands.rs`,
`controls/tests/recipe.rs`.

### Checkbox

- Pointer, Space, and `Cancel` work like Button; activation toggles.
- Enter does nothing.
- `Semantic(Activate)` toggles. `Semantic(SetValue(v))` sets `v`.
- Returns `Some(value)` only when the value changed.

Pinned by `controls/tests/checkbox.rs`.

### RadioGroup and TabSet

- **Roving focus.** After `install`, only the selected option is focusable.
- **Selection follows focus.** Arrows select the target and focus it.
- ArrowRight/ArrowDown go to the next enabled option, ArrowLeft/ArrowUp to
  the previous. Both wrap and skip disabled options.
- Home and End select the first and last enabled option.
- Space or Enter on key-down select the focused option. Repeats are
  accepted.
- `Activate` (host pointer click) and `Semantic(Activate)` select without
  needing focus. `Semantic(Focus)` focuses without selecting.
- Input on a disabled option is ignored.
- Selecting the current option reports nothing and does not move focus.
- Arrows are not mirrored in right-to-left layouts.

Pinned by `controls/tests/selection_controls.rs`.

### TextField

- **Focus.** Editing needs focus. `SetValue` and `SetValidation` do not.
- **Disabled.** Everything except `SetValidation` is ignored.
- **Pointer.** None. The host hit-tests layout and sends `SetCaret`.
- **Keys (key-down).** Backspace and Delete remove one grapheme cluster.
  ArrowLeft/ArrowRight move a grapheme, or a word with `word: true`;
  `extend: true` extends the selection. Home and End go to the text's
  start and end. Arrows are logical, not mirrored.
- **Line breaks.** `\n` and `\r` are stripped from all incoming text.
- **Commit.** Validation is set by the app. Enter (not a repeat) returns
  `Committed` only when `Valid`, else `CommitRejected`.
- **IME.** Marked text does not change the value. `CommitMarkedText` is one
  undoable edit. `CancelMarkedText` discards with no edit.
  `UnmarkMarkedText` keeps the marked text.
- **Undo.** `Undo` and `Redo` step through the edit history.
- **Password.** `Cut` does nothing; `selected_text` is empty.
- No `Cancel` input and no Escape behavior.

Pinned by `controls/tests/text_field.rs`.

### TextEditor

- Focus and disabled rules match TextField. `SetValue` works without focus
  and when read-only.
- **Pointer.** The host sends `PointerDown { clicks, extend }`,
  `PointerDrag`, `PointerUp`. One click places the caret, two select a
  word, three or more a paragraph; dragging extends by that unit. The
  editor does not capture the pointer.
- A press while IME text is marked only commits the marked text.
- **Keys (key-down, repeats accepted).** Enter inserts a newline. Tab
  inserts a tab only with `set_tab_inserts(true)`. ArrowUp/ArrowDown move
  by visual line and keep the column; PageUp/PageDown move by
  `page_height`. Home/End go to the visual line's ends, or the text's ends
  with `word: true`.
- **Read-only.** Typing, marked text, Enter, Tab, Backspace, Delete, Cut,
  Undo, Redo, and semantic `SetValue` are ignored. Navigation works.
- Undo skips marked text. Typed `\r\n` becomes `\n`.

Pinned by `controls/tests/text_editor.rs`.

### Slider

| Input | Behavior |
| --- | --- |
| Primary press on the node | Focus, capture, start drag, jump to pointer. |
| Move while dragging | Value follows the pointer, clamped. |
| Primary release | Sets value from the release point. Outside does not cancel. |
| ArrowRight/ArrowUp, ArrowLeft/ArrowDown | Step up / down. Repeats accepted. |
| Home / End | Minimum / maximum. |
| `Cancel` | Ends drag, releases capture. Value stays. |

- PageUp/PageDown do nothing. Arrows are not mirrored for right-to-left.
- Vertical sliders put the minimum at the bottom.

Pinned by `controls/tests/range_controls.rs`.

### SpinBox and DecimalSpinBox

**SpinBox:**

- A press focuses it. A press in the trailing quarter steps up (top half)
  or down (bottom half) once, on press.
- ArrowUp/ArrowDown step. Home/End go to the bounds. PageUp/PageDown do
  nothing. Values clamp; they do not wrap. No `Cancel` input.

**DecimalSpinBox:**

- ArrowUp/ArrowDown step; PageUp/PageDown step ten times; Home/End go to
  the bounds. Results clamp and round to `fraction_digits`.
- A step starts from the committed value and replaces edited text.
- `set_text` changes only the text. Enter (or `commit`) applies it only if
  valid; invalid text stays. Escape restores the committed text.
- Semantic `SetValue` replaces the text, then commits.
- A locale change reformats the text and keeps the value.

Pinned by `controls/tests/range_controls.rs`, `controls/tests/widgets.rs`.

### Splitter

- The handle is focusable after `install`. No pointer handling.
- Arrows along the split axis move by 10 points. Home and End go to the
  smallest and largest position allowed by both panes' minimums.
- Positions clamp to that range. `set_position` outside it fails.
- Semantic `Increment`/`Decrement` use the same step. No disabled state.

Pinned by `controls/tests/containers.rs`.

### ScrollBar and ScrollArea

**ScrollBar:**

- Press on the thumb starts a drag; press on the track pages one viewport
  toward the press. Drag follows the thumb travel; release ends it. No
  capture or cancel.
- Arrows on the bar's axis step `line_step` (default 16). PageUp/PageDown
  page one viewport. Home/End go to the ends.
- Semantic `Increment`/`Decrement` page one viewport.
- The value stays within `0..=content - viewport`.

**ScrollArea:**

- Press on a thumb captures; the drag works outside the area.
- Press on the vertical track pages. Press on the horizontal track scrolls
  one line.
- Wheel scrolls only over the area or while it holds capture.
- Keys (focused): arrows scroll a line, PageUp/PageDown a page, Space pages
  down, Home/End go to top and bottom. A page keeps one line of overlap.
- Semantic `Increment` pages down, `Decrement` pages up.
- `Cancel`, a primary release, and disabling end the drag. No focus on
  press.

Pinned by `controls/tests/widgets.rs`, `controls/tests/navigation.rs`.

### ItemView

- **Press** focuses the view. One click on a tree row's disclosure area
  toggles expansion. Otherwise: plain replaces the selection, Shift
  extends, Command toggles. `clicks >= 2` also activates.
- **Context menu** focuses the view, selects the row alone if it is not
  selected, and reports `context_menu`.
- **Wheel** scrolls, clamped, and requests more rows near the end.
- **Keys.** Arrows move the current item (Up/Down by a row in a grid).
  PageUp/PageDown move a page. Home/End go to the ends. With no current
  item, forward keys go to the first item and backward keys to the last.
  Shift extends; Command moves only the current item (neither in single
  mode). Enter activates. Space selects, or toggles with Command or in
  multi mode. Command+A selects all except in single mode.
- **Tree keys (list layout).** ArrowRight expands, or moves to the first
  child if expanded. ArrowLeft collapses, or moves to the parent. Not
  mirrored for right-to-left.
- **Type-ahead.** Characters build a prefix that resets after 1 s
  (`TYPE_AHEAD_TIMEOUT`). Matching ignores case and diacritics and wraps.
  A new search starts after the current item.
- **Semantic.** `Activate` selects and activates. `Focus` moves only the
  current item. `Expand`, `Collapse`, `ShowMenu` act on the item.
- Selection is kept by key across sort and filter. No disabled state.

Pinned by `controls/tests/item_view.rs`.

### HeaderView

- **Press** within 4 points of a column's right edge starts a resize.
  Otherwise it focuses the column and toggles its sort, on press: a new
  column sorts ascending, then descending.
- **Drag** resizes, never below `min_width`. **Release** ends it.
- **Keys.** ArrowLeft/ArrowRight move the focused column and stop at the
  ends. Space or Enter toggles its sort. `+` or `=` widens it by 10 points,
  `-` narrows it.

Pinned by `controls/tests/item_view.rs`.

### ComboBox

- **Press** toggles the popup.
- **Closed, not editable.** ArrowUp/ArrowDown select and commit the
  previous or next item. Space or Alt+ArrowDown opens.
- **Closed, editable.** ArrowDown opens. Enter commits the typed text,
  selecting the item whose text matches (ignoring case), or none.
- **Open.** ArrowUp/ArrowDown move the highlight without wrapping.
  PageUp/Home and PageDown/End jump to the ends. Enter commits the
  highlight; Space does too when not editable.
- **Typing** (editable) filters by prefix or substring, ignoring case and
  diacritics. It opens the popup when something matches and closes it
  when nothing does. **Tab** accepts the inline completion.
- **Escape** (open) closes and restores the text from when it opened.
- **Dismiss** (outside press, window deactivation) closes without
  restoring or committing.
- Semantic `SetValue` picks the matching item; only editable combos accept
  free text.
- Disabled: input is ignored and the popup closes.

Pinned by `controls/tests/popup.rs`.

### PopupMenu and PopupStack

**PopupMenu:**

- Hover highlights an enabled item; hover on a submenu opens it at once.
  Release over an enabled action activates it.
- ArrowDown/ArrowUp move between enabled items and wrap. Home/End go to
  the first and last enabled item.
- ArrowRight, Enter, or Space on a submenu opens it. Enter or Space on an
  action activates it.
- ArrowLeft or Escape closes one submenu level; at the root they return
  `Closed`.
- A character jumps to the next enabled item starting with it.
- Disabled or unknown commands, separators, and empty submenus are skipped
  and never activate.

**PopupStack:**

- `open` saves focus. `close(id)` closes that popup and all later ones,
  restoring the saved focus if the node still exists.
- `escape` closes only the topmost popup.
- A press inside a popup routes to the topmost one containing it. A press
  outside all popups closes them all and is consumed.
- `window_deactivated` closes all. `prune` closes popups whose owner left.

Pinned by `controls/tests/popup.rs`.

### TooltipController

- Shows 600 ms (`TOOLTIP_DELAY`) after the pointer enters a target. `poll`
  returns the absolute time of the next check.
- Within 400 ms (`TOOLTIP_GRACE`) of hiding, or while one is visible, a new
  target shows at once.
- Re-hovering the same target does not restart the delay. Empty text
  counts as leaving.
- `interrupt` (press, key, scroll) hides it until the pointer reaches
  another target.

Pinned by `controls/tests/popup.rs`.

### DialogStack and dialogs

- **DialogStack.** `open` saves focus and focuses `initial`. While a modal
  is open, `accepts_input` allows only the topmost modal's subtree, and
  `focus_next` cycles within it. Modeless dialogs do not restrict input.
  `close` restores saved focus if the node still exists.
- **Button keys.** Enter triggers the `Accept` button and Escape the
  `Reject` button (`dialog_key_button`, `MessageDialog`). Enter never
  triggers `Destructive`. Order follows macOS and mirrors in right-to-left.
- **DialogKeys::route.** Key-down only, no repeats. Plain Enter runs the
  default command unless the focused control uses Enter. Plain Escape and
  Command+period run cancel. Disabled commands do not run.
- **InputDialog.** Enter returns the value only when valid. Escape returns
  `Err(())`. Empty or partial input is `Intermediate`.
- **ColorPicker.** `set_hex` takes `#RGB`, `#RRGGBB`, `#RRGGBBAA`; shorter
  hex runs are `Intermediate`; anything else is `Invalid` and changes
  nothing. A gray keeps the previous hue.
- **FontDialog.** `set_size` accepts 1 to 4096 points; otherwise `Invalid`.

Pinned by `controls/tests/dialogs.rs`, `controls/tests/navigation.rs`.

### Toolbar and StatusBar

- **Toolbar focus** roves over placed (not overflowed) controls and
  enabled commands. Horizontal: ArrowRight next, ArrowLeft previous,
  swapped in right-to-left. Vertical: ArrowDown/ArrowUp. Wraps. Home/End
  go to the ends; Tab entering the toolbar should send Home.
- Enter or Space on a focused enabled command returns it. On an embedded
  control it returns nothing.
- **StatusBar.** `show_message` queues one announcement. A timed message
  expires on `poll`, which returns the time remaining (not an absolute
  time). An untimed message stays until replaced or cleared.

Pinned by `controls/tests/shell.rs`.

### DockLayout

- `close` hides a closable panel, remembers its place, and returns the
  panel to focus next. Non-closable panels fail.
- `show` restores a hidden panel at its place and activates it. `dock`
  moves a panel and makes it visible and active.
- **Splitter keys.** Arrows move the area's edge 10 points in the arrow's
  direction (ArrowRight grows the left area). Home shrinks it to 80
  points. End does nothing. `arrange` keeps the center at least 80 points.

Pinned by `controls/tests/shell.rs`.

### CanvasView

- **Press on an item.** Plain press selects it alone unless already
  selected; `extend` toggles it. It becomes the focus. If selected, the
  drag moves all selected movable items.
- **Press on empty space.** Plain press clears the selection, then starts a
  rubber band. With `extend`, the band toggles against the old selection.
- **Drag** moves items at once (no threshold) or updates the band.
  **Release** ends it. No cancel; moves are applied live.
- **Wheel** pans; with `zoom` it zooms about the pointer. **Magnify** zooms
  about the pointer. Zoom clamps to `MIN_ZOOM`..`MAX_ZOOM`.
- **Keys.** Arrows nudge selected movable items 1 point (10 with Shift),
  or pan when none. Tab/Shift+Tab cycle focus and selection through
  visible selectable items. Space toggles the focused item. Enter
  activates it. Escape clears the selection. Delete and Backspace request
  deletion. `+`/`=`, `-`, and `0` zoom in, out, and to 100%.

Pinned by `controls/tests/canvas.rs`.

### DateTimeEdit and Calendar

**DateTimeEdit:**

- ArrowLeft/ArrowRight move between sections and stop at the ends. Date
  separators, `/ . - :`, and space advance a section.
- ArrowUp/ArrowDown step by 1, PageUp/PageDown by 10. Fields wrap, except
  the year, which clamps to 1..9999. Home/End set the field's bounds.
- Digits type into the section, which advances when full or when no more
  digits fit. A year applies after four digits.
- The day is clamped to the month, so the value is always a real date.
- A value outside the range is `Intermediate`. Enter clamps it into range.

**Calendar:**

- Left/Right move a day, Up/Down a week, PageUp/PageDown a month.
  Home/End go to the week's start and end (locale's first weekday).
- Moving onto a date outside the range does nothing. Not mirrored for
  right-to-left.

Pinned by `controls/tests/widgets.rs`.

### Dial

- `point_to` sets the value from the angle, clockwise from up. Wrapping
  dials use a full turn; others use 270 degrees and snap the dead zone to
  the nearer end.
- ArrowUp/ArrowRight step up, ArrowDown/ArrowLeft down; wrapping dials
  wrap. PageUp/PageDown step by `page_step`. Home/End go to the bounds.

Pinned by `controls/tests/widgets.rs`.

### ToolButton, ButtonGroup, and ToolBox

- **ToolButton.** `click` runs the command, or opens the menu for an
  `Instant` menu or a click on a `Split` arrow. Space or Enter runs the
  command (opens an `Instant` menu). ArrowDown opens any menu. A checkable
  button flips the command's checked state. Disabled commands do nothing.
- **ButtonGroup.** Exclusive groups keep exactly one member checked;
  clicking the checked one does nothing. Non-exclusive groups toggle.
  `step` moves the check to the next enabled member and wraps.
- **ToolBox.** One page open at a time. ArrowUp/ArrowDown move header
  focus and stop at the ends. Home/End go to the ends. Space or Enter
  opens the focused page.

Pinned by `controls/tests/widgets.rs`.

### KeySequenceEdit

- Records only after `start`. Each press is one chord. Recording finishes
  at `max_chords` (1 to 4) or on `finish` (pause or focus loss).
- Plain Escape cancels and keeps the old sequence. Plain Backspace or
  Delete as the first key clears it. `finish` with nothing recorded
  cancels. `Key::Other` keys are ignored.

Pinned by `controls/tests/widgets.rs`.

### NumberInput, InputMask, and DateInput

These validate text; they hold no focus or value.

- **NumberInput.** Uses locale separators. Empty or partial numeric text is
  `Intermediate`. Too many fraction digits or above the maximum is
  `Invalid`. Below a positive minimum is `Intermediate` while more whole
  digits could reach it.
- **InputMask.** Inserts literals and skips optional slots when the next
  character fits a later one. Rejected characters make it `Invalid`;
  unfilled required slots make it `Intermediate`.
- **DateInput.** Uses the locale's short date order. Empty or partial dates
  are `Intermediate`. Dates outside the range are `Invalid`.

Pinned by `controls/tests/input_rules.rs`, `controls/tests/properties.rs`.

## Known inconsistencies (to resolve before 1.0)

This document describes the current behavior. The differences below were found while writing it. Each one will either be unified or be documented as intended before 1.0. Either change is breaking, and the release notes will list it.

| Area | Current behavior |
| --- | --- |
| Space timing | Button and Checkbox activate on Space key-up and ignore repeats. RadioGroup and TabSet select on Space or Enter key-down and accept repeats. |
| Enter | Button activates on Enter. Checkbox ignores it. |
| Paging | Slider and SpinBox have no Page Up/Down, although `IntegerRange` has a `page_step`. Dial and DecimalSpinBox (×10) do page. |
| Release outside | Button and Checkbox cancel on a release outside the control. Slider commits the value at the release point, and `Cancel` keeps the dragged value. |
| ScrollArea track | A click on the vertical track pages. A click on the horizontal track moves one line. ScrollBar pages on both axes. |
| Right-to-left arrows | Only Toolbar mirrors the left and right arrows. |
| Wrapping | RadioGroup, TabSet, PopupMenu, Toolbar, and ButtonGroup wrap. ToolBox, HeaderView, ComboBox's highlight, and ItemView clamp. DateTimeEdit wraps every field except the year. |
| Out-of-range input | DateTimeEdit reports `Intermediate`, and DateInput reports `Invalid`. NumberInput reports `Intermediate` below a positive minimum and `Invalid` above the maximum. |
| Timer polling | `TooltipController::poll` returns an absolute due time, and `StatusBar::poll` returns the time remaining. |
| ComboBox dismissal | Escape restores the text from when the popup opened. `Dismiss` (an outside press or window deactivation) closes the popup and keeps the edited text. |
| Advertised actions | Dial and DateTimeEdit list Increment and Decrement, Dial also lists SetValue, and ToolBox lists Activate and Expand. None of these has a handler method, so the host maps them. |
| Disabled and cancel | Splitter, ItemView, ScrollBar, and CanvasView have no disabled state. CanvasView drags have no threshold, and Escape clears the selection without undoing a move. |
| Press timing | HeaderView sorts on press, not on release. SpinBox steps once on press, does not repeat while held, and has no `Cancel`. |
| Focus on click | Clicking the radio option or tab that is already selected does not move focus to it. |
