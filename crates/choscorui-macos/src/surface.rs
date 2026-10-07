//! Cursor shapes, view capture, occlusion, and damage-driven invalidation.

use super::*;
use choscorui_core::{CursorShape, Image, ImageError, Rect, damage};

/// NSWindowOcclusionStateVisible.
const OCCLUSION_VISIBLE: usize = 1 << 1;

pub(crate) unsafe extern "C" fn window_did_change_occlusion(_: Id, _: Sel, notification: Id) {
    record_window_event(notification, |window| {
        // SAFETY: notification.object is the live NSWindow.
        let state = unsafe { send_usize(window, c"occlusionState") };
        WindowEvent::Occluded(state & OCCLUSION_VISIBLE == 0)
    });
}

pub(crate) unsafe extern "C" fn view_reset_cursor_rects(view: Id, _: Sel) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: AppKit calls resetCursorRects on the main thread for an
        // attached view; the cursor is retained by the cursor rect.
        unsafe {
            let window = send_id(view, c"window");
            if window.is_null() {
                return;
            }
            let shape = paint_states()
                .lock()
                .unwrap()
                .get(&(window as usize))
                .map(|paint| paint.lock().unwrap().cursor.clone())
                .unwrap_or_default();
            let Some(cursor) = native_cursor(&shape) else {
                return;
            };
            let bounds = send_rect(view, c"bounds");
            send_void_rect_id(view, c"addCursorRect:cursor:", bounds, cursor);
        }
    }));
}

/// Autoreleased NSCursor for `shape`, or `None` when a custom image is
/// unusable. Requires an autorelease pool.
unsafe fn native_cursor(shape: &CursorShape) -> Option<Id> {
    unsafe {
        let cursor_class = class(c"NSCursor").ok()?;
        let name = match shape {
            CursorShape::Arrow => c"arrowCursor",
            CursorShape::IBeam => c"IBeamCursor",
            CursorShape::PointingHand => c"pointingHandCursor",
            CursorShape::ClosedHand => c"closedHandCursor",
            CursorShape::OpenHand => c"openHandCursor",
            CursorShape::Crosshair => c"crosshairCursor",
            CursorShape::NotAllowed => c"operationNotAllowedCursor",
            CursorShape::ResizeLeftRight => c"resizeLeftRightCursor",
            CursorShape::ResizeUpDown => c"resizeUpDownCursor",
            CursorShape::Custom { image, hotspot } => {
                return custom_cursor(cursor_class, image, *hotspot);
            }
        };
        let cursor = send_id(cursor_class, name);
        (!cursor.is_null()).then_some(cursor)
    }
}

unsafe fn custom_cursor(cursor_class: Id, image: &Image, hotspot: Point) -> Option<Id> {
    let width = f64::from(image.width());
    let height = f64::from(image.height());
    let inside = (0.0..width).contains(&hotspot.x) && (0.0..height).contains(&hotspot.y);
    if !inside || width > 256.0 || height > 256.0 {
        return None;
    }
    unsafe {
        let ns_image = imaging::ns_image(image)?;
        let cursor = send_id_id_point(
            send_id(cursor_class, c"alloc"),
            c"initWithImage:hotSpot:",
            ns_image,
            NSPoint {
                x: hotspot.x,
                y: hotspot.y,
            },
        );
        if cursor.is_null() {
            return None;
        }
        send_id(cursor, c"autorelease");
        Some(cursor)
    }
}

impl Window {
    /// Replaces the display list. Only the region whose drawing changed is
    /// invalidated; an identical list schedules no frame at all.
    pub fn set_display_list(&self, list: DisplayList) {
        let mut paint = self.paint.lock().unwrap();
        let region = damage(&paint.list, &list);
        paint.list = list;
        let Some(region) = region else {
            return;
        };
        paint.last_damage = Some(region);
        drop(paint);
        // Round out and pad one point so antialiased edges repaint too.
        let x = region.origin.x.floor() - 1.0;
        let y = region.origin.y.floor() - 1.0;
        let rect = NSRect {
            origin: NSPoint { x, y },
            size: NSSize {
                width: (region.origin.x + region.size.width).ceil() + 1.0 - x,
                height: (region.origin.y + region.size.height).ceil() + 1.0 - y,
            },
        };
        // SAFETY: The window owns this NSView until Window::drop.
        unsafe {
            let view = send_id(self.raw.as_ptr(), c"contentView");
            send_void_rect(view, c"setNeedsDisplayInRect:", rect);
        }
    }

    /// Logical region invalidated by the most recent display-list change.
    pub fn last_damage(&self) -> Option<Rect> {
        self.paint.lock().unwrap().last_damage
    }

    /// Sets the pointer shape shown over the window's content.
    pub fn set_cursor(&self, shape: CursorShape) -> Result<(), Error> {
        let _pool = Pool::new()?;
        // SAFETY: Cursor objects are autoreleased into `_pool`; the view is
        // owned by the window until Window::drop.
        unsafe {
            if native_cursor(&shape).is_none() {
                return Err(Error::InvalidCursor);
            }
            {
                let mut paint = self.paint.lock().unwrap();
                if paint.cursor == shape {
                    return Ok(());
                }
                paint.cursor = shape;
            }
            let view = send_id(self.raw.as_ptr(), c"contentView");
            send_void_id(self.raw.as_ptr(), c"invalidateCursorRectsForView:", view);
        }
        Ok(())
    }

    pub fn cursor(&self) -> CursorShape {
        self.paint.lock().unwrap().cursor.clone()
    }

    /// True when no part of the window is visible on any display; apps should
    /// pause animations and timers that only drive drawing.
    pub fn is_occluded(&self) -> bool {
        // SAFETY: The window is live until Window::drop.
        unsafe { send_usize(self.raw.as_ptr(), c"occlusionState") & OCCLUSION_VISIBLE == 0 }
    }

    /// Renders the content view into straight-alpha sRGB pixels at the
    /// backing scale. Capturing runs `drawRect:` and counts as a paint.
    pub fn capture(&self) -> Result<Image, ImageError> {
        let _pool = Pool::new().map_err(|_| ImageError::DecodeFailed)?;
        // SAFETY: The bitmap rep is autoreleased into `_pool`; its CGImage
        // is borrowed while copied into an owned Image.
        unsafe {
            let view = send_id(self.raw.as_ptr(), c"contentView");
            let bounds = send_rect(view, c"bounds");
            let rep = send_id_rect(view, c"bitmapImageRepForCachingDisplayInRect:", bounds);
            if rep.is_null() {
                return Err(ImageError::InvalidDimensions);
            }
            send_void_rect_id(view, c"cacheDisplayInRect:toBitmapImageRep:", bounds, rep);
            let cg = send_id(rep, c"CGImage");
            if cg.is_null() {
                return Err(ImageError::DecodeFailed);
            }
            imaging::image_from_cg(cg)
        }
    }
}

unsafe fn send_void_rect_id(receiver: Id, name: &CStr, rect: NSRect, value: Id) {
    let f: unsafe extern "C" fn(Id, Sel, NSRect, Id) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), rect, value) }
}
unsafe fn send_id_rect(receiver: Id, name: &CStr, rect: NSRect) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, NSRect) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), rect) }
}
unsafe fn send_id_id_point(receiver: Id, name: &CStr, value: Id, point: NSPoint) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, Id, NSPoint) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value, point) }
}
