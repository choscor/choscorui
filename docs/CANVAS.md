# Canvas

`choscorui_core::Scene` is a retained 2D scene graph. `choscorui_controls::CanvasView` is an interactive view of it. Several views can show the same scene, and each keeps its own zoom, pan, selection, and focus. All drawing goes through the renderer-neutral `DisplayList`, so a scene paints the same way on screen, in offscreen bitmaps, and in exports.

## Supported

| Area | Behavior |
| --- | --- |
| Items | Rectangle, ellipse, line, polygon, arbitrary `Path`, single-line text, image, SVG document (`Arc<SvgDocument>`), and group. Each item can have a fill (solid or gradient `Paint`), a stroke (`StrokeStyle` with dashes, caps, joins), and an opacity that also applies to its children. |
| Hierarchy and transforms | `add_child`; any invertible affine item-to-parent transform; `set_parent` reparents an item without moving it in world space and rejects cycles; `z` sets order among siblings, with insertion order breaking ties. |
| Spatial index | A uniform grid of 256-unit cells holds world bounds that include stroke width. Items covering more than 1024 cells are kept in a separate list. Visible-area queries and hit tests cost time in proportion to the queried area, not the scene size. The automated test uses 100,000 items. |
| Hit testing | Topmost visible, selectable item. Fills use the exact path (nonzero rule). Strokes and lines are hit within half the stroke width plus a 2-unit slop. Text, image, and SVG items use their box. A point outside a `clip_children` ancestor does not hit. |
| Culling and clipping | `Scene::paint(list, visible)` skips whole subtrees outside the visible area. `clip_children` clips descendants to the parent's shape. |
| View | Zoom (0.05×–32×) about any point, keeping that point fixed. Pan, fit, and `ensure_visible`. Scroll pans; ⌘-scroll or pinch zooms; `+`, `-`, `0` zoom from the keyboard. |
| Selection and editing | Click selects; ⌘/⇧-click toggles; a rubber band selects items fully inside it, and ⇧ toggles items in the band. Dragging moves selected movable items with the view delta divided by zoom, so the item tracks the pointer at any zoom. A child whose ancestor is also selected moves only once. Arrow keys nudge selected items by 1 unit, or 10 with ⇧. Delete/Backspace asks the app to delete; Return activates; Escape clears the selection. |
| Accessibility | The canvas is a `List` whose collection count is the number of items with a role. Only items in view are exposed (virtualized like list rows), each with its index among all items, view-space bounds, and selected state. Tab and ⇧-Tab move focus in paint order and scroll the focused item into view. The `Focus` and `Activate` assistive actions map to focus and select. |
| Change tracking | `Scene::take_changes` reports Added, Removed, and Changed items with old and new bounds, for damage-based repaint. `revision()` increases on every edit. |
| Export | `Scene::export(rect, scale)` returns a clipped `DisplayList` for any rasterizer, for example `MacBitmap` followed by `encode_png`. It matches the on-screen output because both use the same commands. |

## Not supported in 1.0

| Operation | Status |
| --- | --- |
| Scene layouts (anchor, grid, and linear layouts inside the scene) | Deferred. Position items with transforms. |
| Embedding interactive controls as scene items (widget proxy) | Deferred. Overlay controls on the view in window coordinates instead. Native controls are out of scope. |
| Per-item blur or drop-shadow effects | Not supported. Opacity, gradients, and clipping are the only item effects. |
| Multi-line or rich text items | Not supported. Lay the text out with `TextLayout`, then use a path or image item. |
| Hit testing that ignores transparent pixels in image or SVG items | Not supported. These items hit anywhere in their box. |
| Vector export formats (PDF, SVG) | Not supported. Export produces display lists and bitmaps. |
