# Dialogs

ChoscorUI uses two kinds of dialog. Both follow the same keyboard and button-order rules.

## Native dialogs (macOS)

These are system UI. Their appearance, accessibility, and sandbox behavior come from AppKit.

| Dialog | API | AppKit class |
| --- | --- | --- |
| Open file(s), select directory | `FileDialogSpec` with `FileDialogMode::{OpenFile, OpenFiles, SelectDirectory}` | `NSOpenPanel` |
| Save file | `FileDialogSpec` with `FileDialogMode::SaveFile` | `NSSavePanel` |
| Alert / confirmation | `AlertSpec` (`AlertStyle`, `AlertButton`) | `NSAlert` |

File dialogs must stay native so that sandboxed apps get security-scoped access through the system's Powerbox.

## Custom-drawn dialogs (`choscorui-controls`)

| Dialog | Model | Notes |
| --- | --- | --- |
| Color | `ColorPicker` | HSV editing with RGB and hex text, alpha, recent colors. Grays keep the last hue. |
| Font | `FontDialog` | Family through `FontChooser` (a missing family falls back), size 1–4096 pt, weight, slant. |
| Single value | `InputDialog` | `InputKind::{Text, Integer, Decimal, Choice}` with locale-aware validation. Accept is enabled only when the value is valid. |
| Message | `MessageDialog` | Use it where an in-window, themed message is wanted instead of `NSAlert`. |

## Shared contract

- **Button order** (`order_dialog_buttons`), left to right: Help, Discard ("Don't Save"), a flexible gap, Other actions, Cancel, then the default button. Right-to-left layouts mirror the row.
- **Keys** (`dialog_key_button`): Return triggers the `Accept` button. A `Destructive` button is never triggered by Return. Escape triggers `Reject`.
- **Stacking** (`DialogStack`):
  - A modal dialog confines input (`accepts_input`) and Tab cycling (`focus_next`) to its subtree.
  - A modeless dialog does not.
  - Opening a dialog saves the current focus; closing it restores that focus if the node still exists.
- **Accessibility**: dialogs expose the `Dialog` role, with the title as the name and the message as the description. They support the `Dismiss` action (VoiceOver Escape gesture).
