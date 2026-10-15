//! XInput 2.2 touch events: the first touch point that goes down acts as
//! the primary button of the pointer, as `crate::touch` decides.
//!
//! Each window selects the touch events of the master devices. The server
//! then sends no pointer events that it emulates from a touch to this
//! client, so each touch arrives one time. While a popup grabs the
//! pointer, the server sends the touches to the grab as core pointer
//! events instead.

use super::Client;
use crate::InputEvent;
use crate::ffi::x11::{Bytes, XCB_INPUT_TOUCH_BEGIN, XCB_INPUT_TOUCH_UPDATE};
use crate::native::pointer;
use crate::native::types::WindowEvent;
use std::time::Instant;

impl Client {
    /// Handles `XI_TouchBegin`, `XI_TouchUpdate`, or `XI_TouchEnd` in wire
    /// format. The layout is that of the device events: the touch id is
    /// the detail.
    pub(super) fn touch_event(&self, event: &Bytes) {
        let id = event.u32(16);
        let pixels = (event.fp1616(40), event.fp1616(44));
        let Some((window, position)) = self.pointer_target(event.u32(24), pixels) else {
            return;
        };
        let now = Instant::now();
        let (modifiers, events) = {
            let mut seat = self.seat.borrow_mut();
            seat.device = event.u16(10);
            seat.root = (
                event.fp1616(32).round() as i32,
                event.fp1616(36).round() as i32,
            );
            let touch = InputEvent::Touch {
                id: id.cast_signed(),
                x: position.x,
                y: position.y,
            };
            if seat.input.dispatch(touch).is_err() {
                return;
            }
            let mut changes = Vec::with_capacity(2);
            match event.u16(8) {
                XCB_INPUT_TOUCH_BEGIN => changes.extend(seat.touch.down(id, position)),
                XCB_INPUT_TOUCH_UPDATE => changes.extend(seat.touch.motion(id, position)),
                // The end can have a position of its own.
                _ => {
                    changes.extend(seat.touch.motion(id, position));
                    changes.extend(seat.touch.up(id));
                }
            }
            let modifiers = seat.keyboard.modifiers();
            let events: Vec<_> = changes
                .into_iter()
                .map(|event| (event, pointer::touch_details(&mut seat.clicks, now, event)))
                .collect();
            (modifiers, events)
        };
        for (event, details) in events {
            window.shared.push(WindowEvent::Pointer {
                event,
                modifiers,
                details,
            });
        }
    }
}
