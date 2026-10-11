# choscorui-macos

`choscorui-macos` is the macOS backend of choscorui. The `choscorui` facade re-exports it as `choscorui::platform`. The crate is an implementation detail, so applications depend on the facade and not on this crate.

The crate calls AppKit, Foundation, Core Foundation, Core Graphics, Core Text, ImageIO, and UserNotifications through hand-written FFI. Its public handles stay on the main thread.

- [platform-macos.md](../../docs/platform-macos.md) describes the backend for application developers.
- [ffi.md](../../docs/ffi.md) records each `unsafe` FFI use and its safety invariant.

## Accessibility bridge

The bridge gives each semantic node a native `NSAccessibilityElement`. VoiceOver and other assistive technologies read these elements. The bridge has two parts:

- [`src/accessibility.rs`](src/accessibility.rs) has the platform-neutral projection. It keeps the identity of each node and maps semantic changes to notifications.
- [`src/lib.rs`](src/lib.rs) has the native part. `Window::set_accessibility` creates, updates, and releases the AppKit objects, and the action callbacks queue requests.

### Reconciliation

The application builds a `SemanticTree`, takes a `snapshot`, and passes it with the `SemanticChange` list to `Window::set_accessibility`. `AccessibilityProjection::reconcile` then compares the snapshot with the previous one:

- A node that is new gets a new `AccessibilityIdentity` and a new native element.
- A node that stays in the snapshot keeps its identity and its native element. The window updates the attributes of that element in place.
- The projection retires a node that is not in the snapshot. The window releases its native element, and the projection never uses the identity again.

Each update sets the role, subrole, frame in screen coordinates, enabled state, focus, expanded state, selected state, label, help text, and value. A checked state becomes an `NSNumber` value. Editable text also sets the selected text range and the character count in UTF-16 units. `LabelledBy` relations set `accessibilityTitleUIElement`, and `DescribedBy` and `Controls` relations set `accessibilityLinkedUIElements`.

[`tests/accessibility.rs`](tests/accessibility.rs) pins the identity and retirement rules.

### Role mapping

`native_accessibility_role` in `src/lib.rs` maps each `SemanticRole` to an AppKit role:

| `SemanticRole` | AppKit role |
| --- | --- |
| `Window` | `AXWindow` |
| `Button`, `ColumnHeader` | `AXButton` |
| `Checkbox` | `AXCheckBox` |
| `RadioButton`, `Tab` | `AXRadioButton` |
| `TextField` | `AXTextField`, or `AXTextArea` for multiline text |
| `PasswordField` | `AXTextField` with the `AXSecureTextField` subrole |
| `StaticText` | `AXStaticText` |
| `List` | `AXList` |
| `Table` | `AXTable` |
| `Row`, `TreeItem` | `AXRow` |
| `Cell` | `AXCell` |
| `Tree` | `AXOutline` |
| `ScrollArea` | `AXScrollArea` |
| `ScrollBar` | `AXScrollBar` |
| `Slider` | `AXSlider` |
| `SpinButton` | `AXIncrementor` |
| `ProgressIndicator` | `AXProgressIndicator` |
| `Menu` | `AXMenu` |
| `MenuItem` | `AXMenuItem` |
| `ComboBox` | `AXComboBox` |
| `Tooltip` | `AXHelpTag` |
| `Toolbar` | `AXToolbar` |
| `Image` | `AXImage` |
| `Link` | `AXLink` |
| `Application`, `Group`, `RadioGroup`, `TabList`, `TabPanel`, `ListItem`, `Dialog`, `StatusBar` | `AXGroup` |

### Notifications

The projection turns each `SemanticChange` into an `AccessibilityNotification`. The window then posts the AppKit notification:

| Semantic change | AppKit notification | Posted on |
| --- | --- | --- |
| `Tree` (nodes added, removed, or moved) | `NSAccessibilityLayoutChangedNotification` | Content view |
| `Label` | `NSAccessibilityTitleChangedNotification` | Element |
| `Value`, `State`, `ValueAndState` | `NSAccessibilityValueChangedNotification`, plus `NSAccessibilitySelectedTextChangedNotification` for text | Element |
| `Focus` to a node | `NSAccessibilityFocusedUIElementChangedNotification` | Element |
| `Focus` cleared | `NSAccessibilityFocusedUIElementChangedNotification` | Content view |

A live update with no focused element uses `Window::announce(message, priority)`. It posts `NSAccessibilityAnnouncementRequestedNotification` on the application. `AnnouncementPriority::High` interrupts the current speech. The window ignores an empty message.

### Action callbacks

The native element class adds these AppKit methods. Each one queues an `AccessibilityActionRequest` for the node:

| AppKit method | `SemanticAction` |
| --- | --- |
| `accessibilityPerformPress` (`AXPress`) | `Activate` |
| `accessibilityPerformIncrement` (`AXIncrement`) | `Increment` |
| `accessibilityPerformDecrement` (`AXDecrement`) | `Decrement` |
| `accessibilityPerformShowMenu` (`AXShowMenu`) | `ShowMenu` |
| `accessibilityPerformCancel` (`AXCancel`) | `Dismiss` |
| `setAccessibilityFocused:` with `YES` | `Focus` |
| `setAccessibilityExpanded:` | `Expand` or `Collapse` |
| `setAccessibilityValue:` | `SetValue`, with the new value as text |

The bridge queues a request only when the node declares that action in its `SemanticNode`. Otherwise the callback does nothing. The window also calls these setters during an update. In that case, the callback passes the call to AppKit and queues nothing.

Call `Window::take_accessibility_actions` after each event pump. Route each request into the same control input as the keyboard or the pointer. Each action callback catches a panic, so a panic never unwinds into AppKit.

### Collections

Virtualized views have semantic nodes only for realized items. The container declares the full size with `SemanticTree::set_collection`, and each realized item declares its absolute position with `SemanticTree::set_item_index`:

| Semantic data | AppKit attribute |
| --- | --- |
| `SemanticCollection::item_count` | `accessibilityRowCount` |
| `SemanticCollection::column_count` | `accessibilityColumnCount` |
| Item index | `accessibilityIndex` |

The bridge saturates each value to `NSInteger`. VoiceOver can then report the position of a row in the full collection, although only the visible rows exist.

### Diagnostic

[`scripts/macos/ax-check.sh`](../../scripts/macos/ax-check.sh) reads the accessibility tree of the gallery and the notes sample from outside the process. It compares the roles and names with the mapping above. No live session has confirmed the expected roles and names in the script, so a mismatch can be an error in the script. The script needs the Accessibility permission for the terminal.
