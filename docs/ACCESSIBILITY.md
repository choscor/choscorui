# Accessibility

Every control produces a `SemanticNode`: role, name, description, value,
state, relations, supported actions, and text metrics for editable text.
`SemanticTree` keeps these nodes keyed by retained-tree `NodeId`.
`snapshot` turns them into entries with layout bounds, focus, collection
size, and item index. On macOS, `Window::set_accessibility` reconciles the
snapshot into stable `NSAccessibilityElement`s: a node keeps its native
identity across updates, and removed nodes are retired. The method then
posts the matching notifications.

## Control contracts

No control ships without a row here and a test of its `semantics()`.

| Control | Role (AX role) | Name / value | State | Actions |
| --- | --- | --- | --- | --- |
| `Button` | Button (AXButton) | label | disabled | Activate |
| `Checkbox` | Checkbox (AXCheckBox) | label / checked as number | disabled, checked | Activate |
| `RadioGroup` | RadioGroup (AXGroup) with RadioButton (AXRadioButton) children | group label, option labels | checked per option | Activate per option |
| `TabSet` | TabList (AXGroup) with Tab (AXRadioButton) and TabPanel (AXGroup, labelled by its tab) | tab titles | selected; hidden panels | Activate per tab |
| `TextField` | TextField or PasswordField (AXTextField / AXSecureTextField) | label / text (masked when secure) | disabled, invalid; error text as description | SetValue, Focus |
| `TextEditor` | TextField, multiline (AXTextArea) | label / text, character count, selected range | disabled, read-only | SetValue, Focus |
| `Slider` | Slider (AXSlider) | label / value, range as description | disabled | SetValue, Increment, Decrement, Focus |
| `Splitter` | Slider (AXSlider), controls both panes | label / position | — | Increment, Decrement, Focus |
| `SpinBox` | SpinButton (AXIncrementor) | label / value | disabled | SetValue, Increment, Decrement, Focus |
| `ProgressBar` | ProgressIndicator (AXProgressIndicator) | label / value | — | — |
| `ScrollArea` | ScrollArea (AXScrollArea) | label | — | — |
| `GroupBox`, `StackedContainer` | Group (AXGroup) | title | — | — |
| `Label` | StaticText (AXStaticText) | text | — | — |
| `Separator` | none (decorative) | — | — | — |
| `ItemView` (list/tree/table/grid) | List, Tree, or Table (AXList/AXOutline/AXTable) with ListItem/TreeItem/Row (AXRow) for realized rows | first-column text / other columns joined | selected, expanded; full count and absolute index | Activate, Focus, Expand/Collapse, ShowMenu |
| `HeaderView` | ColumnHeader (AXButton) | title / "ascending" or "descending" | focused column selected | Activate, Focus |
| `ComboBox`, `FontChooser` | ComboBox (AXComboBox) with ListItem suggestions | label / text | expanded; invalid when the font is missing | ShowMenu, SetValue, Focus |
| `PopupMenu` | MenuItem (AXMenuItem) | label, shortcut as description | disabled, checked, highlighted, submenu expanded | Activate |
| Tooltip | Tooltip (AXHelpTag), described-by its target | text | — | — |
| `DialogStack` dialogs, `MessageDialog` | Dialog (AXSheet-like group); description "Modal" for modal dialogs | title / message and detail | — | Dismiss (Escape) |
| `ColorPicker` | Group with hex value | name / "Red r, Green g, Blue b, Opacity n%" | — | SetValue (hex), Focus |
| `Toolbar` | Toolbar; one Tab stop with roving arrow focus | toolbar name | — | items' own actions |
| `StatusBar` | StatusBar; temporary messages are announced at low priority (`take_announcement`) | "Status" / message and sections | — | Focus |
| `DockLayout` panels and splitters | TabPanel per panel (selected = active tab, "Floating" description); Slider per area splitter (value in points) | panel title | selected | Focus, Dismiss (closable); Increment/Decrement on splitters |
| `ScrollBar` | ScrollBar (AXScrollBar) | label / percent | hidden when not needed | Increment, Decrement |
| `DecimalSpinBox`, `DateTimeEdit` | SpinButton (AXIncrementor) | label / formatted value (long date) | invalid with message | Increment, Decrement, SetValue, Focus |
| `Calendar` | Table (AXTable) with Cell (AXCell) days | month title / long date | selected, disabled outside range | Activate per enabled day |
| `Dial` | Slider (AXSlider), "Dial" description | label / value | — | Increment, Decrement, SetValue, Focus |
| `ToolButton` | Button, or Checkbox when checkable | label | disabled, checked | Activate, ShowMenu with a menu |
| `ToolBox` | Button per header | title | expanded | Activate, Expand |
| `NumericDisplay` | StaticText | label / plain number | overflow described | — |
| `KeySequenceEdit` | TextField, read-only | label / "⌘K, ⇧⌘S" | recording described | Activate, Focus |

## Notifications

| Semantic change | AppKit notification |
| --- | --- |
| Node added, removed, or restructured | `NSAccessibilityLayoutChangedNotification` on the view |
| Name changed | `NSAccessibilityTitleChangedNotification` |
| Value or state changed | `NSAccessibilityValueChangedNotification`, plus `NSAccessibilitySelectedTextChangedNotification` for text |
| Focus moved or cleared | `NSAccessibilityFocusedUIElementChangedNotification` |
| Live update with no focused element | `Window::announce(message, AnnouncementPriority)` posts `NSAccessibilityAnnouncementRequestedNotification` (High interrupts) |

Assistive-technology actions (press, increment, decrement, set value,
focus) are queued only when the node declares that action, then read back
with `Window::take_accessibility_actions`. The application routes each
action into the same control input as the keyboard or pointer would.

## Virtualized content

Only realized items exist as semantic nodes. The container declares
`SemanticTree::set_collection(id, SemanticCollection { item_count,
column_count })`, which maps to `accessibilityRowCount` and
`accessibilityColumnCount`. Each realized item declares its absolute
position with `set_item_index`, which maps to `accessibilityIndex`. When
scrolling recycles a row, its index is updated, and pruning a node drops
its collection data. This lets VoiceOver say "row 4,001 of 10,000" without
materializing every row.

## Reading order and localization

Reading order is the logical retained-tree order, never the painted
geometry, so RTL-mirrored rows still read first-to-last
(`rtl_snapshot_keeps_logical_reading_order_after_mirrored_layout`). Names,
descriptions, and values are application strings and pass through
unchanged as UTF-8. The smoke tests use Vietnamese and Arabic labels.

## Preferences

`system_accessibility_preferences` reads Reduce Motion and Increase
Contrast. Feeding them to `ThemeResolver::set_accessibility_preferences`
forces `Contrast::High`, zero-duration motion, and the chosen text scale
([THEMING.md](THEMING.md)).

## Manual VoiceOver checklist (gallery)

1. Turn on VoiceOver (⌘F5) and open the gallery.
2. Use VO-Right to step through: theme button, checkbox, text field,
   document, and rows. Each one announces its role and name.
3. VO-Space on the button switches the theme. On the checkbox it toggles
   the box, and the "checked" state is announced.
4. In the text field, type an invalid value and press Return. The error is
   announced.
5. Edit the document with VoiceOver. Moving the caret reads characters and
   lines.
6. Turn on Reduce Motion and Increase Contrast in System Settings, relaunch,
   and confirm that the theme follows.
