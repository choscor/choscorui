//! Window placement persistence, screen enumeration, and native context
//! menus. Coordinates use a top-left origin at the main screen's top-left
//! corner, y growing downward, matching `WindowGeometry`.
#![deny(unsafe_code)]

use super::{ContextMenu, Error, MenuCommandId, NSPoint, NSRect, NSSize, Pool, Size, Window};
use crate::ffi::appkit::{
    CONTENT_VIEW, IS_ZOOMED, NSScreen, POP_UP_MENU, SCREEN_FRAME, SCREEN_VISIBLE_FRAME, SCREENS,
    SET_FRAME_DISPLAY, STYLE_MASK, TOGGLE_FULL_SCREEN, WINDOW_FRAME, WINDOW_STYLE_FULL_SCREEN,
    ZOOM,
};
use crate::ffi::foundation::array_items;
use choscorui_core::{Point, Rect, WindowGeometry};

/// The connected screens, main (menu-bar) screen first. Requires a pool.
fn screen_frames(frame: impl Fn(&crate::ffi::Obj<NSScreen>) -> NSRect) -> Vec<NSRect> {
    let Ok(Some(screens)) = SCREENS.send_class(()) else {
        return Vec::new();
    };
    array_items(&screens)
        .filter_map(|screen| screen.downcast::<NSScreen>())
        .map(frame)
        .collect()
}

/// Height of the main (menu-bar) screen, the pivot for flipping AppKit's
/// bottom-left global coordinates.
fn main_screen_height() -> Option<f64> {
    let frames = screen_frames(|screen| SCREEN_FRAME.send(screen, ()));
    frames.first().map(|frame| frame.size.height)
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
    let Some(main_height) = main_screen_height() else {
        return Ok(Vec::new());
    };
    Ok(
        screen_frames(|screen| SCREEN_VISIBLE_FRAME.send(screen, ()))
            .into_iter()
            .map(|frame| flip(frame, main_height))
            .collect(),
    )
}

impl Window {
    /// The current frame (title bar included) and zoom/full-screen state.
    pub fn geometry(&self) -> WindowGeometry {
        let _pool = Pool::new();
        let window = self.ns_window();
        let frame = WINDOW_FRAME.send(window, ());
        let main_height = main_screen_height().unwrap_or(frame.origin.y + frame.size.height);
        WindowGeometry {
            frame: flip(frame, main_height),
            zoomed: IS_ZOOMED.send(window, ()),
            full_screen: STYLE_MASK.send(window, ()) & WINDOW_STYLE_FULL_SCREEN != 0,
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
        let window = self.ns_window();
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
        SET_FRAME_DISPLAY.send(window, (native, true));
        if IS_ZOOMED.send(window, ()) != geometry.zoomed {
            ZOOM.send(window, (None,));
        }
        let full_screen = STYLE_MASK.send(window, ()) & WINDOW_STYLE_FULL_SCREEN != 0;
        if full_screen != geometry.full_screen {
            TOGGLE_FULL_SCREEN.send(window, (None,));
        }
        Ok(())
    }

    /// Shows `menu` at `at` (content-view coordinates, top-left origin) and
    /// runs AppKit's menu tracking loop until it closes. Returns the chosen
    /// command, which is also queued in `menu.take_activations()`.
    pub fn show_context_menu(&self, menu: &ContextMenu, at: Point) -> Option<MenuCommandId> {
        let _pool = Pool::new();
        menu.take_activations();
        // The content view is flipped, so the location is in its top-left
        // coordinates. The menu and target outlive the synchronous tracking
        // loop.
        let view = CONTENT_VIEW.send(self.ns_window(), ())?;
        POP_UP_MENU.send(
            menu.menu(),
            (None, NSPoint { x: at.x, y: at.y }, Some(view)),
        );
        menu.take_activations().into_iter().next()
    }
}
