# Custom controls

You can write your own controls in safe Rust by implementing `CustomControl` (from `choscorui_controls`, also re-exported by `choscorui`). No platform or FFI code is needed.

| Hook | Called when | Contract |
| --- | --- | --- |
| `measure(constraints, direction)` | During layout. Use it from a `LeafMeasure` closure: `\|node, c, d\| host.measure(node, c, d).unwrap_or(default)`. | Returns a preferred size and optional baseline. The host clamps the size to the constraints. |
| `event(context, event)` | When the host routes `ControlEvent::{Pointer, Key, Text, FocusChanged}`. Pointer and key events go through capture, target, and bubble phases (`context.phase()`). | Return `EventReply::Stop` to consume the event. Use `context` to request paint, layout, or semantics updates, take focus, or capture the pointer for drags. |
| `paint(list, bounds, tokens, focused)` | When `ControlHost::paint` walks visible mounted nodes in tree order. Each control is clipped to its bounds. | Draws only through the renderer-neutral `DisplayList`, using the resolved `DesignTokens`. |
| `semantics()` | When `ControlHost::sync_semantics` runs for a node marked dirty for semantics (or for every node when `force` is set). | Returns role, name, value, state, and supported actions. The returned `SemanticChange`s drive platform notifications. |
| `perform(context, action, value)` | When an assistive technology sends an action through `ControlHost::perform`. | Called only for actions listed in `semantics().actions`. Returns whether the action was applied. |
| `focusable()` | At mount time. | Focusable controls take focus when pressed, and receive `FocusChanged` when focus arrives or leaves. |

The host owns controls by `NodeId`:
- `mount` and `unmount` attach and detach the node.
- `prune` drops controls whose nodes were detached elsewhere.
- `sync_semantics` keeps paint and layout requests on the node for the frame loop.

`controls/tests/custom.rs` is a complete example: a star-rating control that handles pointer, key, and text input, focus, painting, semantics, and assistive Increment/Decrement.
