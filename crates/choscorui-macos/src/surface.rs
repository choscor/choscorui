//! Cursor shapes, view capture, occlusion, and damage-driven invalidation.

use super::*;
use crate::ffi::Obj;
use crate::ffi::appkit::{
    ADD_CURSOR_RECT, ARROW_CURSOR, BITMAP_CG_IMAGE, BITMAP_IMAGE_REP_FOR_CACHING, BOUNDS,
    CACHE_DISPLAY_IN_RECT, CLOSED_HAND_CURSOR, CONTENT_VIEW, CROSSHAIR_CURSOR,
    ChoscoruiV2ContentView, ChoscoruiV2WindowDelegate, I_BEAM_CURSOR, INIT_WITH_IMAGE_HOT_SPOT,
    INVALIDATE_CURSOR_RECTS_FOR_VIEW, NOT_ALLOWED_CURSOR, NSCursor, OCCLUSION_STATE,
    OCCLUSION_STATE_VISIBLE, OPEN_HAND_CURSOR, POINTING_HAND_CURSOR, RESIZE_LEFT_RIGHT_CURSOR,
    RESIZE_UP_DOWN_CURSOR, SET_NEEDS_DISPLAY_IN_RECT, VIEW_WINDOW,
};
use crate::ffi::retained::{Retained, alloc};
use choscorui_core::{CursorShape, Image, ImageError, Rect, damage};

pub(crate) fn window_did_change_occlusion(
    _: &Obj<ChoscoruiV2WindowDelegate>,
    notification: Option<&Obj<NSNotification>>,
) {
    record_window_event(notification, |window| {
        WindowEvent::Occluded(OCCLUSION_STATE.send(window, ()) & OCCLUSION_STATE_VISIBLE == 0)
    });
}

/// Adds the cursor rect of the window's current cursor shape. The cursor
/// rect retains the cursor.
pub(crate) fn view_reset_cursor_rects(view: &Obj<ChoscoruiV2ContentView>) {
    let Some(window) = VIEW_WINDOW.send(view, ()) else {
        return;
    };
    let shape = paint_states()
        .lock()
        .unwrap()
        .get(&window.addr())
        .map(|paint| paint.lock().unwrap().cursor.clone())
        .unwrap_or_default();
    let Some(cursor) = native_cursor(&shape) else {
        return;
    };
    ADD_CURSOR_RECT.send(view, (BOUNDS.send(view, ()), &cursor));
}

/// NSCursor for `shape`, or `None` when a custom image is unusable.
/// Requires an autorelease pool.
fn native_cursor(shape: &CursorShape) -> Option<Retained<NSCursor>> {
    let cursor = match shape {
        CursorShape::Arrow => &ARROW_CURSOR,
        CursorShape::IBeam => &I_BEAM_CURSOR,
        CursorShape::PointingHand => &POINTING_HAND_CURSOR,
        CursorShape::ClosedHand => &CLOSED_HAND_CURSOR,
        CursorShape::OpenHand => &OPEN_HAND_CURSOR,
        CursorShape::Crosshair => &CROSSHAIR_CURSOR,
        CursorShape::NotAllowed => &NOT_ALLOWED_CURSOR,
        CursorShape::ResizeLeftRight => &RESIZE_LEFT_RIGHT_CURSOR,
        CursorShape::ResizeUpDown => &RESIZE_UP_DOWN_CURSOR,
        CursorShape::Custom { image, hotspot } => return custom_cursor(image, *hotspot),
    };
    cursor.class_send(()).ok().flatten().map(Obj::retain)
}

fn custom_cursor(image: &Image, hotspot: Point) -> Option<Retained<NSCursor>> {
    let width = f64::from(image.width());
    let height = f64::from(image.height());
    let inside = (0.0..width).contains(&hotspot.x) && (0.0..height).contains(&hotspot.y);
    if !inside || width > 256.0 || height > 256.0 {
        return None;
    }
    let ns_image = imaging::ns_image(image)?;
    let cursor = alloc::<NSCursor>().ok()?;
    INIT_WITH_IMAGE_HOT_SPOT.init(
        cursor,
        (
            &ns_image,
            NSPoint {
                x: hotspot.x,
                y: hotspot.y,
            },
        ),
    )
}

impl Window {
    /// Replaces the display list. Only the region whose drawing changed is
    /// invalidated; an identical list schedules no frame at all.
    pub fn set_display_list(&self, list: DisplayList) {
        let _pool = Pool::new();
        let mut paint = self.paint.lock().unwrap();
        let region = damage(&paint.list, &list);
        paint.list = Arc::new(list);
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
        if let Some(view) = CONTENT_VIEW.send(self.ns_window(), ()) {
            SET_NEEDS_DISPLAY_IN_RECT.send(view, (rect,));
        }
    }

    /// Logical region invalidated by the most recent display-list change.
    pub fn last_damage(&self) -> Option<Rect> {
        self.paint.lock().unwrap().last_damage
    }

    /// Sets the pointer shape shown over the window's content.
    pub fn set_cursor(&self, shape: CursorShape) -> Result<(), Error> {
        let _pool = Pool::new()?;
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
        let window = self.ns_window();
        if let Some(view) = CONTENT_VIEW.send(window, ()) {
            INVALIDATE_CURSOR_RECTS_FOR_VIEW.send(window, (view,));
        }
        Ok(())
    }

    pub fn cursor(&self) -> CursorShape {
        self.paint.lock().unwrap().cursor.clone()
    }

    /// True when no part of the window is visible on any display; apps should
    /// pause animations and timers that only drive drawing.
    pub fn is_occluded(&self) -> bool {
        let _pool = Pool::new();
        OCCLUSION_STATE.send(self.ns_window(), ()) & OCCLUSION_STATE_VISIBLE == 0
    }

    /// Renders the content view into straight-alpha sRGB pixels at the
    /// backing scale. Capturing runs `drawRect:` and counts as a paint.
    pub fn capture(&self) -> Result<Image, ImageError> {
        let _pool = Pool::new().map_err(|_| ImageError::DecodeFailed)?;
        let view = CONTENT_VIEW
            .send(self.ns_window(), ())
            .ok_or(ImageError::DecodeFailed)?;
        let bounds = BOUNDS.send(view, ());
        let rep = BITMAP_IMAGE_REP_FOR_CACHING
            .send(view, (bounds,))
            .ok_or(ImageError::InvalidDimensions)?;
        CACHE_DISPLAY_IN_RECT.send(view, (bounds, &rep));
        let cg = BITMAP_CG_IMAGE
            .send(&rep, ())
            .ok_or(ImageError::DecodeFailed)?;
        imaging::image_from_cg(cg)
    }
}
