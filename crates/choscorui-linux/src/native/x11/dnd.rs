//! Drag and drop with XDND version 5.
//!
//! Each window is `XdndAware`. As a target, a window answers each
//! `XdndPosition` with `XdndStatus`, reads the data of `XdndSelection` on
//! `XdndDrop`, and sends `XdndFinished`. As a source, the client owns
//! `XdndSelection` with the hidden window, grabs the pointer, finds the
//! aware window under the pointer, and sends it the messages. It sends at
//! most one position until the status arrives. A drag between windows of
//! the application reads the payload directly.

use super::{Client, X11Surface, protocol};
use crate::ffi::x11::{
    Bytes, XCB_ATOM_ATOM, XCB_EVENT_MASK_BUTTON_RELEASE, XCB_EVENT_MASK_POINTER_MOTION,
};
use crate::native::transfer::{self, Payload};
use crate::native::types::{Error, WindowEvent};
use crate::native::window::WindowShared;
use choscorui_core::{
    DragEvent, DragFormat, DragItem, DragOperation, DragOperations, Point, negotiate_drag,
};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

/// The XDND version of the application.
const VERSION: u32 = 5;

/// How long a source waits for `XdndFinished` after the drop.
pub(crate) const FINISH_TIMEOUT: Duration = Duration::from_secs(5);

/// What a window takes from a drag: the formats, the allowed operations,
/// and the preferred operation.
type Accepted = (Vec<DragFormat>, DragOperations, DragOperation);

/// A drag over one of the windows.
struct Incoming {
    source: u32,
    window: Weak<X11Surface>,
    mimes: Vec<String>,
    /// The decision of the window, made at the first position: None
    /// before it, and Some(None) when the window refuses the drag.
    accepted: Option<Option<Accepted>>,
    position: Point,
}

/// The aware window under the pointer during a drag of the application.
struct Target {
    window: u32,
    version: u32,
    /// True between a position and its status.
    waiting: bool,
    /// The root position and time of a motion during the wait.
    pending: Option<((i32, i32), u32)>,
    accepted: bool,
    action: u32,
}

/// A drag that a window of the application started.
struct Outgoing {
    window: Weak<WindowShared>,
    allowed: DragOperations,
    types: Vec<u32>,
    target: Option<Target>,
    /// The time of the drop, while the source waits for `XdndFinished`.
    dropped: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct DndState {
    incoming: Option<Incoming>,
    outgoing: Option<Outgoing>,
}

/// The data of a format-32 client message.
fn data(event: &Bytes, index: usize) -> u32 {
    event.u32(12 + 4 * index)
}

impl Client {
    fn action_atom(&self, operation: DragOperation) -> u32 {
        let atoms = &self.transfer_atoms;
        match operation {
            DragOperation::Copy => atoms.xdnd_action_copy,
            DragOperation::Move => atoms.xdnd_action_move,
            DragOperation::Link => atoms.xdnd_action_link,
            DragOperation::None => 0,
        }
    }

    fn operation_of(&self, action: u32) -> DragOperation {
        let atoms = &self.transfer_atoms;
        if action == atoms.xdnd_action_copy {
            DragOperation::Copy
        } else if action == atoms.xdnd_action_move {
            DragOperation::Move
        } else if action == atoms.xdnd_action_link {
            DragOperation::Link
        } else {
            DragOperation::None
        }
    }

    fn operations_of(&self, actions: &[u32]) -> DragOperations {
        let mut operations = DragOperations::NONE;
        for action in actions {
            match self.operation_of(*action) {
                DragOperation::Copy => operations.copy = true,
                DragOperation::Move => operations.move_ = true,
                DragOperation::Link => operations.link = true,
                DragOperation::None => {}
            }
        }
        operations
    }

    fn action_atoms(&self, operations: DragOperations) -> Vec<u32> {
        [
            (operations.copy, DragOperation::Copy),
            (operations.move_, DragOperation::Move),
            (operations.link, DragOperation::Link),
        ]
        .into_iter()
        .filter(|(allowed, _)| *allowed)
        .map(|(_, operation)| self.action_atom(operation))
        .collect()
    }

    /// Makes `window` a drop target for XDND.
    pub(crate) fn make_aware(&self, window: u32) {
        self.connection.set_property32(
            window,
            self.transfer_atoms.xdnd_aware,
            XCB_ATOM_ATOM,
            &[VERSION],
        );
    }

    fn send_message(&self, window: u32, kind: u32, data: [u32; 5]) {
        let message = protocol::client_message(window, kind, data);
        self.connection.send_event(window, 0, &message);
    }

    /// Handles an XDND client message. Returns false for other messages.
    pub(crate) fn dnd_message(&self, event: &Bytes) -> bool {
        let atoms = &self.transfer_atoms;
        let kind = event.u32(8);
        if kind == atoms.xdnd_enter {
            self.xdnd_enter(event);
        } else if kind == atoms.xdnd_position {
            self.xdnd_position(event);
        } else if kind == atoms.xdnd_leave {
            self.xdnd_leave(data(event, 0));
        } else if kind == atoms.xdnd_drop {
            self.xdnd_drop(event);
        } else if kind == atoms.xdnd_status {
            self.xdnd_status(event);
        } else if kind == atoms.xdnd_finished {
            self.xdnd_finished(event);
        } else {
            return false;
        }
        true
    }

    // The target side.

    fn xdnd_enter(&self, event: &Bytes) {
        let Some(window) = self.window(event.u32(4)) else {
            return;
        };
        let source = data(event, 0);
        let types = if data(event, 1) & 1 != 0 {
            self.connection.get_property32(
                source,
                self.transfer_atoms.xdnd_type_list,
                XCB_ATOM_ATOM,
            )
        } else {
            (2..5)
                .map(|index| data(event, index))
                .filter(|atom| *atom != 0)
                .collect()
        };
        let mimes = types
            .into_iter()
            .filter_map(|atom| self.atom_name(atom))
            .collect();
        self.dnd.borrow_mut().incoming = Some(Incoming {
            source,
            window: Rc::downgrade(&window),
            mimes,
            accepted: None,
            position: Point::new(0.0, 0.0),
        });
    }

    /// The payload of the application's own drag.
    fn own_drag_payload(&self) -> Option<Rc<Payload>> {
        self.selections.borrow().drag.clone()
    }

    /// Decides whether `window` takes the drag, at the first position.
    fn decide(
        &self,
        window: &X11Surface,
        (source, mimes): (u32, &[String]),
        action: u32,
        time: u32,
    ) -> Option<Accepted> {
        let (wanted, window_operations) = {
            let target = window.shared.drop_target.borrow();
            if target.formats.is_empty() || target.operations.is_empty() {
                return None;
            }
            (target.formats.clone(), target.operations)
        };
        let own = self.own_drag_payload();
        let selection = self.transfer_atoms.xdnd_selection;
        let offered = match &own {
            Some(payload) => payload.formats(),
            None => transfer::formats(mimes, &mut |mime| self.receive_from(selection, mime, time)),
        };
        let formats: Vec<DragFormat> = offered
            .into_iter()
            .filter(|format| wanted.contains(format))
            .collect();
        if formats.is_empty() {
            return None;
        }
        let source_operations = match &own {
            Some(_) => self
                .dnd
                .borrow()
                .outgoing
                .as_ref()
                .map_or(DragOperations::NONE, |drag| drag.allowed),
            None => {
                let mut actions = self.connection.get_property32(
                    source,
                    self.transfer_atoms.xdnd_action_list,
                    XCB_ATOM_ATOM,
                );
                actions.push(action);
                self.operations_of(&actions)
            }
        };
        let allowed = DragOperations {
            copy: source_operations.copy && window_operations.copy,
            move_: source_operations.move_ && window_operations.move_,
            link: source_operations.link && window_operations.link,
        };
        // PC convention: Ctrl copies, Shift moves, Ctrl+Shift links.
        let modifiers = self.seat.borrow().keyboard.modifiers();
        let preferred = negotiate_drag(allowed, modifiers.control, modifiers.shift, own.is_some());
        (preferred != DragOperation::None).then_some((formats, allowed, preferred))
    }

    fn xdnd_position(&self, event: &Bytes) {
        let source = data(event, 0);
        let (root_x, root_y) = ((data(event, 2) >> 16) as i16, data(event, 2) as i16);
        let (time, action) = (data(event, 3), data(event, 4));
        let (window, mimes, first) = {
            let dnd = self.dnd.borrow();
            let Some(incoming) = dnd.incoming.as_ref().filter(|drag| drag.source == source) else {
                return;
            };
            let Some(window) = incoming.window.upgrade() else {
                return;
            };
            (window, incoming.mimes.clone(), incoming.accepted.is_none())
        };
        let scale = window.shared.scale.get();
        let (x, y, _) = self
            .connection
            .translate_point(self.connection.root(), window.id, (root_x, root_y))
            .unwrap_or((0, 0, 0));
        let position = Point::new(f64::from(x) / scale, f64::from(y) / scale);
        let decision = if first {
            Some(self.decide(&window, (source, &mimes), action, time))
        } else {
            None
        };
        let accepted = {
            let mut dnd = self.dnd.borrow_mut();
            let Some(incoming) = dnd.incoming.as_mut() else {
                return;
            };
            incoming.position = position;
            if let Some(decision) = decision {
                incoming.accepted = Some(decision);
            }
            incoming.accepted.clone().flatten()
        };
        let (accept, action) = match &accepted {
            Some((formats, allowed, preferred)) => {
                let event = if first {
                    DragEvent::Enter {
                        position,
                        formats: formats.clone(),
                        allowed: *allowed,
                    }
                } else {
                    DragEvent::Over {
                        position,
                        allowed: *allowed,
                    }
                };
                window.shared.push(WindowEvent::Drag(event));
                (1, self.action_atom(*preferred))
            }
            None => (0, 0),
        };
        // Bit 1 asks for a position at each motion.
        self.send_message(
            source,
            self.transfer_atoms.xdnd_status,
            [window.id, accept | 2, 0, 0, action],
        );
    }

    fn xdnd_leave(&self, source: u32) {
        let incoming = {
            let mut dnd = self.dnd.borrow_mut();
            if !dnd
                .incoming
                .as_ref()
                .is_some_and(|drag| drag.source == source)
            {
                return;
            }
            dnd.incoming.take()
        };
        if let Some(incoming) = incoming
            && let Some(Some(_)) = incoming.accepted
            && let Some(window) = incoming.window.upgrade()
        {
            window.shared.push(WindowEvent::Drag(DragEvent::Leave));
        }
    }

    fn xdnd_drop(&self, event: &Bytes) {
        let (source, time) = (data(event, 0), data(event, 2));
        let incoming = {
            let mut dnd = self.dnd.borrow_mut();
            if !dnd
                .incoming
                .as_ref()
                .is_some_and(|drag| drag.source == source)
            {
                return;
            }
            dnd.incoming.take()
        };
        let Some(incoming) = incoming else {
            return;
        };
        let window = incoming.window.upgrade();
        let accepted = incoming.accepted.flatten();
        let items: Vec<DragItem> = match &accepted {
            Some((formats, _, _)) => match self.own_drag_payload() {
                Some(payload) => payload.read(formats),
                None => {
                    let selection = self.transfer_atoms.xdnd_selection;
                    transfer::read(formats, &incoming.mimes, &mut |mime| {
                        self.receive_from(selection, mime, time)
                    })
                }
            },
            None => Vec::new(),
        };
        let operation = accepted
            .as_ref()
            .map_or(DragOperation::None, |(_, _, preferred)| *preferred);
        let success = !items.is_empty();
        let action = if success {
            self.action_atom(operation)
        } else {
            0
        };
        self.send_message(
            source,
            self.transfer_atoms.xdnd_finished,
            [
                window.as_ref().map_or(0, |window| window.id),
                u32::from(success),
                action,
                0,
                0,
            ],
        );
        self.connection.flush();
        if let Some(window) = window
            && accepted.is_some()
        {
            let event = if success {
                DragEvent::Drop {
                    position: incoming.position,
                    items,
                    operation,
                }
            } else {
                DragEvent::Leave
            };
            window.shared.push(WindowEvent::Drag(event));
        }
    }

    // The source side.

    /// Starts dragging `payload` from `window`. The left button must be
    /// down on the window.
    pub(crate) fn begin_drag(
        &self,
        window: &X11Surface,
        payload: Payload,
        allowed: DragOperations,
    ) -> Result<(), Error> {
        {
            let seat = self.seat.borrow();
            let over = seat
                .pointer_focus
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some_and(|focus| focus.id == window.id);
            if !(over && seat.left_down) || self.dnd.borrow().outgoing.is_some() {
                return Err(Error::NoDragEvent);
            }
        }
        let atoms = &self.transfer_atoms;
        let source = self.selection_window;
        let time = self.time.get();
        let connection = &self.connection;
        connection.set_selection_owner(source, atoms.xdnd_selection, time);
        let types: Vec<u32> = payload
            .mime_types()
            .iter()
            .map(|mime| self.atom(mime))
            .filter(|atom| *atom != 0)
            .collect();
        connection.set_property32(source, atoms.xdnd_type_list, XCB_ATOM_ATOM, &types);
        connection.set_property32(
            source,
            atoms.xdnd_action_list,
            XCB_ATOM_ATOM,
            &self.action_atoms(allowed),
        );
        let mask = u16::try_from(XCB_EVENT_MASK_BUTTON_RELEASE | XCB_EVENT_MASK_POINTER_MOTION)
            .unwrap_or_default();
        // Without the grab, the implicit grab of the press still sends the
        // motion to the window.
        connection.grab_pointer(window.id, mask, time);
        self.selections.borrow_mut().drag = Some(Rc::new(payload));
        self.dnd.borrow_mut().outgoing = Some(Outgoing {
            window: Rc::downgrade(&window.shared),
            allowed,
            types,
            target: None,
            dropped: None,
        });
        connection.flush();
        Ok(())
    }

    /// The aware window under the root position (`x`, `y`) and its XDND
    /// version, from the top-level windows down.
    fn aware_window(&self, (x, y): (i32, i32)) -> Option<(u32, u32)> {
        let connection = &self.connection;
        let root = connection.root();
        let point = (
            i16::try_from(x).unwrap_or_default(),
            i16::try_from(y).unwrap_or_default(),
        );
        let mut parent = root;
        // Window managers nest a client window in a few frames.
        for _ in 0..8 {
            let (_, _, child) = connection.translate_point(root, parent, point)?;
            if child == 0 {
                return None;
            }
            let version =
                connection.get_property32(child, self.transfer_atoms.xdnd_aware, XCB_ATOM_ATOM);
            if let Some(version) = version.first() {
                return Some((child, (*version).min(VERSION)));
            }
            parent = child;
        }
        None
    }

    /// The operation that the source asks for: the PC convention with the
    /// held modifiers.
    fn requested_action(&self, allowed: DragOperations) -> u32 {
        let modifiers = self.seat.borrow().keyboard.modifiers();
        self.action_atom(negotiate_drag(
            allowed,
            modifiers.control,
            modifiers.shift,
            false,
        ))
    }

    fn send_position(&self, target: u32, (x, y): (i32, i32), time: u32, action: u32) {
        let packed = ((x.clamp(0, 0xffff) as u32) << 16) | y.clamp(0, 0xffff) as u32;
        self.send_message(
            target,
            self.transfer_atoms.xdnd_position,
            [self.selection_window, 0, packed, time, action],
        );
    }

    /// Follows the pointer during the application's drag. Returns false
    /// when no drag runs.
    pub(crate) fn drag_motion(&self) -> bool {
        let root = self.seat.borrow().root;
        let time = self.time.get();
        let (types, allowed, current) = {
            let dnd = self.dnd.borrow();
            let Some(drag) = dnd.outgoing.as_ref() else {
                return false;
            };
            if drag.dropped.is_some() {
                return true;
            }
            (
                drag.types.clone(),
                drag.allowed,
                drag.target.as_ref().map(|target| target.window),
            )
        };
        let found = self.aware_window(root);
        let atoms = &self.transfer_atoms;
        let source = self.selection_window;
        if current != found.map(|(window, _)| window) {
            if let Some(old) = current {
                self.send_message(old, atoms.xdnd_leave, [source, 0, 0, 0, 0]);
            }
            if let Some((window, version)) = found {
                let mut data = [
                    source,
                    (version << 24) | u32::from(types.len() > 3),
                    0,
                    0,
                    0,
                ];
                for (slot, atom) in data[2..].iter_mut().zip(&types) {
                    *slot = *atom;
                }
                self.send_message(window, atoms.xdnd_enter, data);
            }
            if let Some(drag) = self.dnd.borrow_mut().outgoing.as_mut() {
                drag.target = found.map(|(window, version)| Target {
                    window,
                    version,
                    waiting: false,
                    pending: None,
                    accepted: false,
                    action: 0,
                });
            }
        }
        let action = self.requested_action(allowed);
        let send = {
            let mut dnd = self.dnd.borrow_mut();
            let Some(target) = dnd.outgoing.as_mut().and_then(|drag| drag.target.as_mut()) else {
                return true;
            };
            if target.waiting {
                target.pending = Some((root, time));
                None
            } else {
                target.waiting = true;
                Some(target.window)
            }
        };
        if let Some(window) = send {
            self.send_position(window, root, time, action);
        }
        self.connection.flush();
        true
    }

    fn xdnd_status(&self, event: &Bytes) {
        let (window, flags, action) = (data(event, 0), data(event, 1), data(event, 4));
        let (allowed, pending) = {
            let mut dnd = self.dnd.borrow_mut();
            let Some(drag) = dnd.outgoing.as_mut() else {
                return;
            };
            let allowed = drag.allowed;
            let Some(target) = drag
                .target
                .as_mut()
                .filter(|target| target.window == window)
            else {
                return;
            };
            target.accepted = flags & 1 != 0;
            target.action = action;
            target.waiting = false;
            let pending = target.pending.take();
            if pending.is_some() {
                target.waiting = true;
            }
            (allowed, pending)
        };
        if let Some((root, time)) = pending {
            let action = self.requested_action(allowed);
            self.send_position(window, root, time, action);
            self.connection.flush();
        }
    }

    /// Drops on the target that accepted the drag, or cancels it. Returns
    /// false when no drag runs.
    pub(crate) fn drag_release(&self) -> bool {
        let time = self.time.get();
        let source = self.selection_window;
        let atoms = &self.transfer_atoms;
        let target = {
            let mut dnd = self.dnd.borrow_mut();
            let Some(drag) = dnd.outgoing.as_mut() else {
                return false;
            };
            if drag.dropped.is_some() {
                return true;
            }
            match drag.target.as_ref() {
                Some(target) if target.accepted => {
                    drag.dropped = Some(Instant::now());
                    Some((target.window, true))
                }
                Some(target) => Some((target.window, false)),
                None => None,
            }
        };
        self.connection.ungrab_pointer();
        match target {
            Some((window, true)) => {
                self.send_message(window, atoms.xdnd_drop, [source, 0, time, 0, 0]);
            }
            Some((window, false)) => {
                self.send_message(window, atoms.xdnd_leave, [source, 0, 0, 0, 0]);
                self.end_drag(DragOperation::None);
            }
            None => self.end_drag(DragOperation::None),
        }
        self.connection.flush();
        true
    }

    fn xdnd_finished(&self, event: &Bytes) {
        let (window, flags, action) = (data(event, 0), data(event, 1), data(event, 2));
        let operation = {
            let dnd = self.dnd.borrow();
            let Some(target) = dnd
                .outgoing
                .as_ref()
                .and_then(|drag| drag.target.as_ref())
                .filter(|target| target.window == window)
            else {
                return;
            };
            // Before version 5, the message has no result.
            let success = target.version < 5 || flags & 1 != 0;
            let action = if action == 0 { target.action } else { action };
            if success {
                self.operation_of(action)
            } else {
                DragOperation::None
            }
        };
        self.end_drag(operation);
    }

    /// Ends a drop that got no `XdndFinished` within the timeout, and
    /// returns how long the next wait may last.
    pub(crate) fn expire_drag(&self) -> Option<Duration> {
        let dropped = self.dnd.borrow().outgoing.as_ref()?.dropped?;
        let left = FINISH_TIMEOUT.saturating_sub(dropped.elapsed());
        if left.is_zero() {
            self.end_drag(DragOperation::None);
            return None;
        }
        Some(left)
    }

    fn end_drag(&self, operation: DragOperation) {
        let drag = self.dnd.borrow_mut().outgoing.take();
        self.selections.borrow_mut().drag = None;
        if let Some(window) = drag.and_then(|drag| drag.window.upgrade()) {
            window.push(WindowEvent::DragEnded(operation));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::selection::words;
    use super::*;

    #[test]
    fn client_message_data_reads_the_five_words() {
        let message = protocol::client_message(7, 9, [1, 2, 3, 4, 5]);
        let event = Bytes::from(message.to_vec());
        assert_eq!(event.u32(4), 7);
        assert_eq!(
            (0..5).map(|index| data(&event, index)).collect::<Vec<_>>(),
            words(&message[12..32])
        );
        assert_eq!(data(&event, 4), 5);
    }
}
