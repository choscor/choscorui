//! `wl_touch`: the first touch point that goes down acts as the primary
//! button of the pointer, as `crate::touch` decides.

use super::{Client, Role, Seat, WaylandWindow};
use crate::InputEvent;
use crate::ffi::wayland::{Arg, Event, Proxy};
use crate::native::pointer;
use crate::native::types::WindowEvent;
use choscorui_core::{Point, PointerEvent};
use std::rc::{Rc, Weak};
use std::time::Instant;

const SEAT_GET_TOUCH: u16 = 2;
const TOUCH_DOWN: u16 = 0;
const TOUCH_UP: u16 = 1;
const TOUCH_MOTION: u16 = 2;
const TOUCH_CANCEL: u16 = 4;

impl Client {
    /// Gets the `wl_touch` of `seat_proxy` when the seat has touch, and
    /// drops it when the seat loses touch.
    pub(super) fn update_touch(&self, seat_proxy: &Proxy, touch: bool) {
        let mut seat = self.seat.borrow_mut();
        if touch && seat.touch_proxy.is_none() {
            seat.touch_proxy = seat_proxy
                .request(SEAT_GET_TOUCH, &[Arg::NewId])
                .ok()
                .flatten();
            if let Some(proxy) = &seat.touch_proxy {
                self.routes.borrow_mut().insert(proxy.id(), Role::Touch);
            }
        } else if !touch && seat.touch_proxy.take().is_some() {
            drop(seat);
            // A removed device cannot leave the button down.
            self.touch_pointer(|seat| seat.touch.cancel(), None);
        }
    }

    /// Handles an event of `wl_touch`. Each event applies at once, so
    /// `frame` needs no action. The shape and the orientation of a touch
    /// point have no use.
    pub(super) fn touch_event(&self, event: &Event) {
        match event.opcode {
            TOUCH_DOWN => {
                let Some(window) = event.arg(2).object().and_then(|id| self.window_of(id)) else {
                    return;
                };
                let id = event.arg(3).int();
                let surface = Point::new(event.arg(4).fixed(), event.arg(5).fixed());
                let position = window.content_point(surface);
                let serial = event.arg(0).uint();
                let focus = Rc::downgrade(&window);
                self.touch_pointer(
                    |seat| {
                        let touch = InputEvent::Touch {
                            id,
                            x: position.x,
                            y: position.y,
                        };
                        seat.input.dispatch(touch).ok()?;
                        let down = seat.touch.down(id.cast_unsigned(), position)?;
                        // A touch-down serial moves, resizes, and
                        // activates windows, and grabs popups, as a
                        // button press does.
                        seat.input_serial = serial;
                        seat.press_serial = Some(serial);
                        seat.touch_focus = Some(focus);
                        Some(down)
                    },
                    Some(window),
                );
            }
            TOUCH_MOTION => {
                let id = event.arg(1).int();
                let surface = Point::new(event.arg(2).fixed(), event.arg(3).fixed());
                let Some(window) = self.touch_window() else {
                    return;
                };
                let position = window.content_point(surface);
                self.touch_pointer(
                    |seat| seat.touch.motion(id.cast_unsigned(), position),
                    Some(window),
                );
            }
            TOUCH_UP => {
                let id = event.arg(2).int().cast_unsigned();
                self.touch_pointer(|seat| seat.touch.up(id), None);
            }
            TOUCH_CANCEL => self.touch_pointer(|seat| seat.touch.cancel(), None),
            _ => {}
        }
    }

    /// The window of the primary touch point.
    fn touch_window(&self) -> Option<Rc<WaylandWindow>> {
        self.seat
            .borrow()
            .touch_focus
            .as_ref()
            .and_then(Weak::upgrade)
    }

    /// Applies `change` to the touch state of the seat, and sends the pointer
    /// event that it gives to `window`, or to the window of the touch
    /// point. A release also ends the focus of the touch point.
    fn touch_pointer(
        &self,
        change: impl FnOnce(&mut Seat) -> Option<PointerEvent>,
        window: Option<Rc<WaylandWindow>>,
    ) {
        let (event, details, modifiers, focus) = {
            let mut seat = self.seat.borrow_mut();
            let Some(event) = change(&mut seat) else {
                return;
            };
            let details = pointer::touch_details(&mut seat.clicks, Instant::now(), event);
            let focus = if seat.touch.is_down() {
                seat.touch_focus.clone()
            } else {
                seat.touch_focus.take()
            };
            (event, details, seat.keyboard.modifiers(), focus)
        };
        let Some(window) = window.or_else(|| focus.as_ref().and_then(Weak::upgrade)) else {
            return;
        };
        window.shared.push(WindowEvent::Pointer {
            event,
            modifiers,
            details,
        });
    }
}
