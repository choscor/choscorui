//! The state that a window shares between its public handle and the
//! display-server code, and the public `Window`.
//!
//! The display-server code owns the native objects behind the `Surface`
//! trait. It queues events in the shared state, and it reads the display
//! list from there when it paints.

use super::transfer::{self, Payload};
use super::types::{Error, PaintTiming, Painter, ResizeEdge, TextInputState, WindowEvent};
use choscorui_core::{
    CursorShape, DisplayList, DragFormat, DragItem, DragOperations, DrawRegions, Image, Point,
    Rect, Size, WindowGeometry,
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Instant;

/// The native side of a window, which the display-server code
/// implements.
pub(crate) trait Surface {
    /// Maps the window.
    fn show(&self);
    fn set_title(&self, title: &str);
    /// Shows `shape` while the pointer is over the content.
    fn set_cursor(&self, shape: &CursorShape) -> Result<(), Error>;
    /// Asks for a paint at the next frame.
    fn request_paint(&self);
    fn set_content_size(&self, size: Size);
    fn geometry(&self) -> WindowGeometry;
    fn set_geometry(&self, geometry: &WindowGeometry);
    /// Sends the text input state to the input method.
    fn text_input_changed(&self);
    /// Destroys the native window.
    fn close(&self);
    /// Starts an interactive move after a press on a client-side title
    /// bar.
    fn begin_move(&self);
    /// Starts an interactive resize from `edge`.
    fn begin_resize(&self, edge: ResizeEdge);
    /// Shows the window manager's window menu at `position`.
    fn show_window_menu(&self, position: Point);
    fn set_maximized(&self, maximized: bool);
    fn minimize(&self);
    /// True when the display server draws the title bar and the frame.
    fn server_decorations(&self) -> bool;
    /// Makes the window a modal dialog of `parent`: transient for it, and
    /// modal where the display server supports it.
    fn set_modal_parent(&self, parent: &Rc<WindowShared>);
    /// The screen position of the content in logical points, when the
    /// display server tells it. Wayland does not.
    fn content_origin(&self) -> Option<Point> {
        None
    }
    /// Starts a drag of `payload` from the press that is in progress.
    fn begin_drag(
        &self,
        payload: Payload,
        allowed: DragOperations,
        preview: Option<&Image>,
    ) -> Result<(), Error>;
    /// Test support: moves the pointer to `position` and, with `button`,
    /// presses or releases the left button.
    fn simulate_pointer(&self, position: Point, button: Option<bool>) -> Result<(), Error>;
}

/// The formats and operations that a window takes from drags.
#[derive(Default)]
pub(crate) struct DropTarget {
    pub(crate) formats: Vec<DragFormat>,
    pub(crate) operations: DragOperations,
}

/// The paint state of a window.
pub(crate) struct PaintState {
    pub(crate) list: Rc<DisplayList>,
    /// The sorted draw regions of `list`, kept for the next damage.
    regions: Option<DrawRegions>,
    /// The logical region to paint at the next frame, or None when the
    /// next frame has nothing to paint.
    pub(crate) pending: Option<Rect>,
    /// The region of the most recent display-list change.
    pub(crate) last_damage: Option<Rect>,
    pub(crate) painter: Painter,
    pub(crate) paint_count: usize,
    pub(crate) first_paint: Option<PaintTiming>,
    pub(crate) last_paint: Option<PaintTiming>,
}

/// The state of a window that both sides share.
pub(crate) struct WindowShared {
    pub(crate) title: RefCell<String>,
    /// The logical size of the content.
    pub(crate) size: Cell<Size>,
    pub(crate) minimum: Size,
    pub(crate) scale: Cell<f64>,
    pub(crate) visible: Cell<bool>,
    pub(crate) closed: Cell<bool>,
    pub(crate) focused: Cell<bool>,
    pub(crate) cursor: RefCell<CursorShape>,
    pub(crate) text_input: RefCell<TextInputState>,
    pub(crate) events: RefCell<VecDeque<WindowEvent>>,
    pub(crate) paint: RefCell<PaintState>,
    pub(crate) drop_target: RefCell<DropTarget>,
    /// The handle of the window for the parent of portal dialogs, such as
    /// `wayland:HANDLE` or `x11:XID`.
    pub(crate) parent_handle: RefCell<Option<String>>,
    /// True while a modal window of the application is open: the window
    /// then drops its input events.
    pub(crate) blocked: Cell<bool>,
    /// True once the window is the modal dialog of another window.
    pub(crate) has_parent: Cell<bool>,
}

impl WindowShared {
    pub(crate) fn new(title: &str, size: Size, minimum: Size, painter: Painter) -> Rc<Self> {
        Rc::new(Self {
            title: RefCell::new(title.to_owned()),
            size: Cell::new(size),
            minimum,
            scale: Cell::new(1.0),
            visible: Cell::new(false),
            closed: Cell::new(false),
            focused: Cell::new(false),
            cursor: RefCell::new(CursorShape::Arrow),
            text_input: RefCell::new(TextInputState::default()),
            events: RefCell::new(VecDeque::new()),
            paint: RefCell::new(PaintState {
                list: Rc::new(DisplayList::new()),
                regions: None,
                pending: None,
                last_damage: None,
                painter,
                paint_count: 0,
                first_paint: None,
                last_paint: None,
            }),
            drop_target: RefCell::new(DropTarget::default()),
            parent_handle: RefCell::new(None),
            blocked: Cell::new(false),
            has_parent: Cell::new(false),
        })
    }

    /// Queues `event`. A blocked window drops pointer, key, text, and
    /// drag input.
    pub(crate) fn push(&self, event: WindowEvent) {
        let input = matches!(
            event,
            WindowEvent::Pointer { .. }
                | WindowEvent::Key { .. }
                | WindowEvent::Gesture { .. }
                | WindowEvent::Text { .. }
                | WindowEvent::Ime(_)
                | WindowEvent::Drag(_)
        );
        if input && self.blocked.get() {
            return;
        }
        self.events.borrow_mut().push_back(event);
    }

    /// The whole content, in logical points.
    pub(crate) fn bounds(&self) -> Rect {
        Rect::new(Point::new(0.0, 0.0), self.size.get())
    }

    /// Marks the whole content for the next paint, after a resize, a
    /// scale change, or a painter change.
    pub(crate) fn invalidate(&self) {
        let bounds = self.bounds();
        let mut paint = self.paint.borrow_mut();
        paint.pending = Some(bounds);
    }

    /// Takes the region to paint, if any.
    pub(crate) fn take_pending(&self) -> Option<Rect> {
        self.paint.borrow_mut().pending.take()
    }

    /// Records a finished paint that started at `start`.
    pub(crate) fn painted(&self, start: Instant) {
        let finished = Instant::now();
        let timing = PaintTiming {
            finished,
            duration: finished.saturating_duration_since(start),
        };
        let mut paint = self.paint.borrow_mut();
        paint.paint_count += 1;
        paint.first_paint.get_or_insert(timing);
        paint.last_paint = Some(timing);
    }
}

/// Round out a logical region and pad it one point, so antialiased edges
/// repaint too.
pub(crate) fn padded(region: Rect) -> Rect {
    let x = region.origin.x.floor() - 1.0;
    let y = region.origin.y.floor() - 1.0;
    Rect::new(
        Point::new(x, y),
        Size::new(
            (region.origin.x + region.size.width).ceil() + 1.0 - x,
            (region.origin.y + region.size.height).ceil() + 1.0 - y,
        ),
    )
}

/// A native window. Dropping it does not close it; `close` does.
pub struct Window {
    pub(crate) shared: Rc<WindowShared>,
    pub(crate) surface: Rc<dyn Surface>,
}

impl Window {
    /// Sets the state that the input method reads: the selection, the
    /// marked range, and the caret rectangle.
    pub fn set_text_input_state(&self, state: TextInputState) {
        let changed = *self.shared.text_input.borrow() != state;
        if changed {
            *self.shared.text_input.borrow_mut() = state;
            self.surface.text_input_changed();
        }
    }

    /// The painter of the window.
    pub fn painter(&self) -> Painter {
        self.shared.paint.borrow().painter
    }

    /// The number of finished paints.
    pub fn paint_count(&self) -> usize {
        self.shared.paint.borrow().paint_count
    }

    pub fn first_paint(&self) -> Option<PaintTiming> {
        self.shared.paint.borrow().first_paint
    }

    pub fn last_paint(&self) -> Option<PaintTiming> {
        self.shared.paint.borrow().last_paint
    }

    /// The events since the last call, oldest first.
    pub fn take_events(&self) -> Vec<WindowEvent> {
        self.shared.events.borrow_mut().drain(..).collect()
    }

    /// Makes the window a modal dialog of `parent`: transient for it, and
    /// modal where the display server supports it (`xdg_dialog_v1` on
    /// Wayland, `_NET_WM_STATE_MODAL` on X11). Call it before `show`.
    pub fn set_modal_parent(&self, parent: &Window) {
        if !self.shared.closed.get() && !parent.shared.closed.get() {
            self.surface.set_modal_parent(&parent.shared);
            self.shared.has_parent.set(true);
        }
    }

    pub fn show(&self) {
        if self.shared.closed.get() {
            return;
        }
        self.surface.show();
    }

    pub fn set_title(&self, title: &str) -> Result<(), Error> {
        if title.contains('\0') {
            return Err(Error::InvalidTitle);
        }
        if self.shared.closed.get() {
            return Err(Error::Closed);
        }
        *self.shared.title.borrow_mut() = title.to_owned();
        self.surface.set_title(title);
        Ok(())
    }

    pub fn title(&self) -> String {
        self.shared.title.borrow().clone()
    }

    pub fn is_visible(&self) -> bool {
        self.shared.visible.get() && !self.shared.closed.get()
    }

    /// The logical size of the content.
    pub fn content_size(&self) -> Size {
        self.shared.size.get()
    }

    /// Asks for a new content size. The window manager may refuse it; a
    /// `Resized` event reports the size that applies.
    pub fn set_content_size(&self, size: Size) -> Result<(), Error> {
        if !(size.width.is_finite() && size.height.is_finite())
            || size.width <= 0.0
            || size.height <= 0.0
        {
            return Err(Error::InvalidSize);
        }
        self.surface.set_content_size(size);
        Ok(())
    }

    /// The scale of the window's buffers: device pixels per point.
    pub fn backing_scale_factor(&self) -> f64 {
        self.shared.scale.get()
    }

    pub fn close(&self) {
        if self.shared.closed.replace(true) {
            return;
        }
        self.shared.visible.set(false);
        self.surface.close();
    }

    pub fn is_closed(&self) -> bool {
        self.shared.closed.get()
    }

    /// Replaces the display list. The next frame paints only the region
    /// whose drawing changed, and nothing when the content is the same.
    pub fn set_display_list(&self, list: DisplayList) {
        let mut paint = self.shared.paint.borrow_mut();
        if *paint.list == list {
            return;
        }
        let regions = list.sorted_draw_regions();
        let previous = paint
            .regions
            .take()
            .unwrap_or_else(|| paint.list.sorted_draw_regions());
        let damage = previous.damage(&regions);
        paint.list = Rc::new(list);
        paint.regions = Some(regions);
        let Some(region) = damage else {
            return;
        };
        paint.last_damage = Some(region);
        let region = padded(region);
        paint.pending = Some(match paint.pending {
            Some(pending) => pending.union(region),
            None => region,
        });
        drop(paint);
        self.surface.request_paint();
    }

    /// Renders the display list of the window with Cairo, at its content
    /// size and backing scale, as straight-alpha RGBA8. Both painters use
    /// it, so a capture never reads GPU memory.
    pub fn capture(&self) -> Result<Image, Error> {
        if self.shared.closed.get() {
            return Err(Error::Closed);
        }
        let list = Rc::clone(&self.shared.paint.borrow().list);
        crate::render_offscreen(&list, self.shared.size.get(), self.shared.scale.get())
            .map_err(Error::Capture)
    }

    /// The logical region of the most recent display-list change.
    pub fn last_damage(&self) -> Option<Rect> {
        self.shared.paint.borrow().last_damage
    }

    /// Sets the pointer shape over the content.
    pub fn set_cursor(&self, shape: CursorShape) -> Result<(), Error> {
        if let CursorShape::Custom { image, hotspot } = &shape {
            let (width, height) = (f64::from(image.width()), f64::from(image.height()));
            let valid = image.width() > 0
                && image.height() > 0
                && width <= 256.0
                && height <= 256.0
                && (0.0..=width).contains(&hotspot.x)
                && (0.0..=height).contains(&hotspot.y);
            if !valid {
                return Err(Error::InvalidCursor);
            }
        }
        if *self.shared.cursor.borrow() == shape {
            return Ok(());
        }
        self.surface.set_cursor(&shape)?;
        *self.shared.cursor.borrow_mut() = shape;
        Ok(())
    }

    /// The frame, maximized, and full-screen state. On Wayland the origin
    /// is always zero, because clients cannot read their position.
    pub fn geometry(&self) -> WindowGeometry {
        self.surface.geometry()
    }

    /// Applies a geometry. On Wayland the origin is ignored.
    pub fn set_geometry(&self, geometry: &WindowGeometry) -> Result<(), Error> {
        let size = geometry.frame.size;
        if !(size.width.is_finite() && size.height.is_finite())
            || size.width <= 0.0
            || size.height <= 0.0
        {
            return Err(Error::InvalidSize);
        }
        self.surface.set_geometry(geometry);
        Ok(())
    }

    /// True when the window manager or compositor draws the title bar
    /// and the frame. When false, the application draws them with the
    /// title bar control and calls the methods below.
    pub fn has_server_decorations(&self) -> bool {
        self.surface.server_decorations()
    }

    /// Starts an interactive move. Call it while a press on the
    /// client-side title bar is handled.
    pub fn begin_move(&self) {
        self.surface.begin_move();
    }

    /// Starts an interactive resize from `edge`, while a press is
    /// handled.
    pub fn begin_resize(&self, edge: ResizeEdge) {
        self.surface.begin_resize(edge);
    }

    /// Shows the window menu of the window manager at `position`, in
    /// logical window coordinates.
    pub fn show_window_menu(&self, position: Point) {
        self.surface.show_window_menu(position);
    }

    /// Maximizes or restores the window.
    pub fn set_maximized(&self, maximized: bool) {
        self.surface.set_maximized(maximized);
    }

    /// Minimizes the window.
    pub fn minimize(&self) {
        self.surface.minimize();
    }

    /// Makes the content a drop target for `formats`. An empty list or no
    /// operations stops accepting drops. Route the resulting
    /// [`WindowEvent::Drag`] events through [`choscorui_core::DropRouter`].
    pub fn accept_drops(
        &self,
        formats: &[DragFormat],
        operations: DragOperations,
    ) -> Result<(), Error> {
        if !formats.iter().all(transfer::supported) {
            return Err(Error::UnsupportedFormat);
        }
        *self.shared.drop_target.borrow_mut() = super::window::DropTarget {
            formats: formats.to_vec(),
            operations,
        };
        Ok(())
    }

    /// Starts dragging `items` from the content. Call it while handling
    /// the pointer-down or pointer-move of the left button that began the
    /// gesture. The center of `preview` follows the pointer. The window
    /// gets [`WindowEvent::DragEnded`] when the drag ends.
    pub fn begin_drag(
        &self,
        items: &[DragItem],
        allowed: DragOperations,
        preview: Option<&Image>,
    ) -> Result<(), Error> {
        if items.is_empty() || allowed.is_empty() {
            return Err(Error::UnsupportedFormat);
        }
        let payload = transfer::encode(items)?;
        self.surface.begin_drag(payload, allowed, preview)
    }

    /// Test support: moves the pointer to `position` in the content and,
    /// with `button`, presses (`true`) or releases (`false`) the left
    /// button. It uses XTEST on X11; Wayland has no such protocol.
    #[doc(hidden)]
    pub fn simulate_pointer(&self, position: Point, button: Option<bool>) -> Result<(), Error> {
        if self.shared.closed.get() {
            return Err(Error::Closed);
        }
        self.surface.simulate_pointer(position, button)
    }
}
