//! Window placement persistence, screen enumeration, and native context
//! menus. Coordinates use a top-left origin at the main screen's top-left
//! corner, y growing downward, matching `WindowGeometry`.

use super::*;
use choscorui_core::{Point, Rect, WindowGeometry};

/// NSWindowStyleMaskFullScreen.
const FULL_SCREEN_MASK: usize = 1 << 14;

pub(super) unsafe fn send_id_usize(receiver: Id, name: &CStr, value: usize) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, usize) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value) }
}

unsafe fn send_void_rect_bool(receiver: Id, name: &CStr, rect: NSRect, flag: i8) {
    let f: unsafe extern "C" fn(Id, Sel, NSRect, i8) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), rect, flag) }
}

unsafe fn send_bool_id_point_id(receiver: Id, name: &CStr, first: Id, at: NSPoint, view: Id) -> i8 {
    let f: unsafe extern "C" fn(Id, Sel, Id, NSPoint, Id) -> i8 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), first, at, view) }
}

/// Height of the main (menu-bar) screen, the pivot for flipping AppKit's
/// bottom-left global coordinates.
unsafe fn main_screen_height() -> Option<f64> {
    unsafe {
        let screens = send_id(class(c"NSScreen").ok()?, c"screens");
        if screens.is_null() || send_usize(screens, c"count") == 0 {
            return None;
        }
        Some(
            send_rect(send_id_usize(screens, c"objectAtIndex:", 0), c"frame")
                .size
                .height,
        )
    }
}

fn flip(rect: NSRect, main_height: f64) -> Rect {
    Rect::new(
        Point::new(
            rect.origin.x,
            main_height - rect.origin.y - rect.size.height,
        ),
        Size::new(rect.size.width, rect.size.height),
    )
}

/// Visible frames (excluding menu bar and Dock) of the connected screens,
/// main screen first.
pub fn screen_visible_frames() -> Result<Vec<Rect>, Error> {
    let _pool = Pool::new()?;
    // SAFETY: +[NSScreen screens] returns an autoreleased NSArray of live
    // NSScreen objects; visibleFrame returns NSRect.
    unsafe {
        let Some(main_height) = main_screen_height() else {
            return Ok(Vec::new());
        };
        let screens = send_id(class(c"NSScreen")?, c"screens");
        Ok((0..send_usize(screens, c"count"))
            .map(|index| {
                let screen = send_id_usize(screens, c"objectAtIndex:", index);
                flip(send_rect(screen, c"visibleFrame"), main_height)
            })
            .collect())
    }
}

impl Window {
    /// The current frame (title bar included) and zoom/full-screen state.
    pub fn geometry(&self) -> WindowGeometry {
        // SAFETY: frame returns NSRect; isZoomed BOOL; styleMask NSUInteger.
        unsafe {
            let window = self.raw.as_ptr();
            let frame = send_rect(window, c"frame");
            let main_height = main_screen_height().unwrap_or(frame.origin.y + frame.size.height);
            WindowGeometry {
                frame: flip(frame, main_height),
                zoomed: send_bool(window, c"isZoomed") != 0,
                full_screen: send_usize(window, c"styleMask") & FULL_SCREEN_MASK != 0,
            }
        }
    }

    /// Applies saved placement. Fit the frame first with
    /// `WindowGeometry::fit_to_screens(&screen_visible_frames()?, ...)`.
    /// Zoom and full screen are applied after the frame so un-zooming
    /// returns to the saved size.
    pub fn set_geometry(&mut self, geometry: &WindowGeometry) -> Result<(), Error> {
        let frame = geometry.frame;
        if !(frame.origin.x.is_finite()
            && frame.origin.y.is_finite()
            && frame.size.width.is_finite()
            && frame.size.height.is_finite())
            || frame.size.width <= 0.0
            || frame.size.height <= 0.0
        {
            return Err(Error::InvalidSize);
        }
        let _pool = Pool::new()?;
        // SAFETY: setFrame:display: takes NSRect and BOOL; zoom: and
        // toggleFullScreen: accept a nil sender.
        unsafe {
            let window = self.raw.as_ptr();
            let main_height = main_screen_height().unwrap_or(frame.max_y());
            let native = NSRect {
                origin: NSPoint {
                    x: frame.origin.x,
                    y: main_height - frame.origin.y - frame.size.height,
                },
                size: NSSize {
                    width: frame.size.width,
                    height: frame.size.height,
                },
            };
            send_void_rect_bool(window, c"setFrame:display:", native, 1);
            if (send_bool(window, c"isZoomed") != 0) != geometry.zoomed {
                send_void_id(window, c"zoom:", std::ptr::null_mut());
            }
            let full_screen = send_usize(window, c"styleMask") & FULL_SCREEN_MASK != 0;
            if full_screen != geometry.full_screen {
                send_void_id(window, c"toggleFullScreen:", std::ptr::null_mut());
            }
        }
        Ok(())
    }

    /// Shows `menu` at `at` (content-view coordinates, top-left origin) and
    /// runs AppKit's menu tracking loop until it closes. Returns the chosen
    /// command, which is also queued in `menu.take_activations()`.
    pub fn show_context_menu(&self, menu: &ContextMenu, at: Point) -> Option<MenuCommandId> {
        menu.take_activations();
        // SAFETY: the content view is owned by the window and flipped, so
        // the location is in its top-left coordinates. The menu and target
        // outlive the synchronous tracking loop.
        unsafe {
            let view = send_id(self.raw.as_ptr(), c"contentView");
            if view.is_null() {
                return None;
            }
            send_bool_id_point_id(
                menu.raw(),
                c"popUpMenuPositioningItem:atLocation:inView:",
                std::ptr::null_mut(),
                NSPoint { x: at.x, y: at.y },
                view,
            );
        }
        menu.take_activations().into_iter().next()
    }
}
