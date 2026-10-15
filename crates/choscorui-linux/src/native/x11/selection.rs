//! The X11 clipboard: the `CLIPBOARD` selection, with INCR for large data.
//!
//! A hidden window owns the selections of the application and receives
//! the data of other owners. To read, the client asks the owner to
//! convert the selection into a property of the hidden window and waits
//! up to `READ_TIMEOUT` for `SelectionNotify`. During that wait it still
//! answers requests for its own selections and keeps the other events for
//! the event loop. While the application owns the clipboard, it reads its
//! own payload directly. Data larger than one request goes in chunks
//! through the INCR protocol of the ICCCM, in both directions.

use super::Client;
use crate::ffi::libc::{self, Watch};
use crate::ffi::x11::{
    Bytes, Connection, XCB_ATOM_ATOM, XCB_ATOM_INTEGER, XCB_ATOM_STRING, XCB_CW_EVENT_MASK,
    XCB_EVENT_MASK_PROPERTY_CHANGE, XCB_PROPERTY_DELETE, XCB_PROPERTY_NEW_VALUE,
    XCB_PROPERTY_NOTIFY, XCB_SELECTION_CLEAR, XCB_SELECTION_NOTIFY, XCB_SELECTION_REQUEST,
};
use crate::ffi::xlib_xcb::Event;
use crate::native::transfer::{self, Payload};
use crate::native::types::Error;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How long a read waits for the owner to answer each step.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// How long an outgoing INCR transfer waits for the requestor.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// The largest chunk of an outgoing transfer.
const CHUNK_BYTES: usize = 256 * 1024;

/// The atoms of the selections and of XDND.
pub(crate) struct TransferAtoms {
    pub(crate) clipboard: u32,
    targets: u32,
    timestamp: u32,
    multiple: u32,
    incr: u32,
    atom_pair: u32,
    text: u32,
    /// The property of the hidden window that receives the data.
    property: u32,
    pub(crate) xdnd_aware: u32,
    pub(crate) xdnd_enter: u32,
    pub(crate) xdnd_position: u32,
    pub(crate) xdnd_status: u32,
    pub(crate) xdnd_leave: u32,
    pub(crate) xdnd_drop: u32,
    pub(crate) xdnd_finished: u32,
    pub(crate) xdnd_selection: u32,
    pub(crate) xdnd_type_list: u32,
    pub(crate) xdnd_action_list: u32,
    pub(crate) xdnd_action_copy: u32,
    pub(crate) xdnd_action_move: u32,
    pub(crate) xdnd_action_link: u32,
}

impl TransferAtoms {
    pub(crate) fn intern(connection: &Connection) -> Self {
        let names = [
            "CLIPBOARD",
            "TARGETS",
            "TIMESTAMP",
            "MULTIPLE",
            "INCR",
            "ATOM_PAIR",
            "TEXT",
            "_CHOSCORUI_SELECTION",
            "XdndAware",
            "XdndEnter",
            "XdndPosition",
            "XdndStatus",
            "XdndLeave",
            "XdndDrop",
            "XdndFinished",
            "XdndSelection",
            "XdndTypeList",
            "XdndActionList",
            "XdndActionCopy",
            "XdndActionMove",
            "XdndActionLink",
        ];
        let atoms = connection.intern_atoms(&names);
        let atom = |index: usize| atoms.get(index).copied().unwrap_or(0);
        Self {
            clipboard: atom(0),
            targets: atom(1),
            timestamp: atom(2),
            multiple: atom(3),
            incr: atom(4),
            atom_pair: atom(5),
            text: atom(6),
            property: atom(7),
            xdnd_aware: atom(8),
            xdnd_enter: atom(9),
            xdnd_position: atom(10),
            xdnd_status: atom(11),
            xdnd_leave: atom(12),
            xdnd_drop: atom(13),
            xdnd_finished: atom(14),
            xdnd_selection: atom(15),
            xdnd_type_list: atom(16),
            xdnd_action_list: atom(17),
            xdnd_action_copy: atom(18),
            xdnd_action_move: atom(19),
            xdnd_action_link: atom(20),
        }
    }
}

/// An outgoing INCR transfer: the requestor deletes the property to ask
/// for the next chunk, and a chunk of zero length ends the transfer.
struct Send {
    requestor: u32,
    property: u32,
    kind: u32,
    data: Vec<u8>,
    offset: usize,
    touched: Instant,
}

/// The selections that the application owns or serves.
#[derive(Default)]
pub(crate) struct Selections {
    machine: crate::Clipboard,
    /// The clipboard payload of the application and the time of its
    /// ownership.
    source: Option<(Rc<Payload>, u32)>,
    /// The payload of the running drag, which `XdndSelection` serves.
    pub(crate) drag: Option<Rc<Payload>>,
    sends: Vec<Send>,
}

/// What the clipboard holds, as the application sees it.
pub(crate) enum Contents {
    Empty,
    Local(Rc<Payload>),
    /// The MIME types that the other owner offers.
    Remote(Vec<String>),
}

/// The Latin-1 bytes of `text`, for the `STRING` target. Characters
/// outside Latin-1 become question marks.
fn latin1(text: &[u8]) -> Vec<u8> {
    String::from_utf8_lossy(text)
        .chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect()
}

/// The 32-bit values in the bytes of a property of format 32.
pub(crate) fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| u32::from_ne_bytes(*chunk))
        .collect()
}

/// A `SelectionNotify` event for a request, which tells the requestor
/// where the data is, or that the owner refused (`property` 0).
fn selection_notify(
    time: u32,
    requestor: u32,
    selection: u32,
    target: u32,
    property: u32,
) -> [u8; 32] {
    let mut event = [0; 32];
    event[0] = XCB_SELECTION_NOTIFY;
    for (offset, value) in [
        (4, time),
        (8, requestor),
        (12, selection),
        (16, target),
        (20, property),
    ] {
        event[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
    }
    event
}

impl Client {
    /// The atom of `name`, interned once.
    pub(crate) fn atom(&self, name: &str) -> u32 {
        if let Some(atom) = self.atom_ids.borrow().get(name) {
            return *atom;
        }
        let atom = self
            .connection
            .intern_atoms(&[name])
            .first()
            .copied()
            .unwrap_or(0);
        if atom != 0 {
            self.atom_ids.borrow_mut().insert(name.to_owned(), atom);
            self.atom_names.borrow_mut().insert(atom, name.to_owned());
        }
        atom
    }

    /// The name of `atom`, read once.
    pub(crate) fn atom_name(&self, atom: u32) -> Option<String> {
        if let Some(name) = self.atom_names.borrow().get(&atom) {
            return Some(name.clone());
        }
        let name = self.connection.atom_name(atom)?;
        self.atom_names.borrow_mut().insert(atom, name.clone());
        self.atom_ids.borrow_mut().insert(name.clone(), atom);
        Some(name)
    }

    /// The current server time: the time of a property change on the
    /// hidden window. It falls back to the time of the latest event.
    fn server_time(&self) -> u32 {
        let (window, property) = (self.selection_window, self.transfer_atoms.timestamp);
        self.connection
            .set_property8(window, property, XCB_ATOM_INTEGER, &[]);
        let wanted = |event: &Bytes| {
            event.u8(0) & 0x7f == XCB_PROPERTY_NOTIFY
                && event.u32(4) == window
                && event.u32(8) == property
        };
        self.wait_for(&wanted)
            .map_or(self.time.get(), |event| event.u32(12))
    }

    /// Waits up to `READ_TIMEOUT` for an event that `wanted` matches.
    /// Meanwhile it answers the requests for the application's selections
    /// and keeps the other events for the event loop.
    pub(crate) fn wait_for(&self, wanted: &dyn Fn(&Bytes) -> bool) -> Option<Bytes> {
        let deadline = Instant::now() + READ_TIMEOUT;
        loop {
            self.flush();
            while let Some(event) = self.display.next_event() {
                // The event loop gives the kept events to the filters of
                // Xlib. The filters of XIM watch other windows than the
                // selection events, so these events skip them.
                if let Event::Server(bytes, _) = &event {
                    if wanted(bytes) {
                        return Some(bytes.clone());
                    }
                    if self.selection_event(bytes) {
                        continue;
                    }
                }
                self.deferred.borrow_mut().push_back(event);
            }
            self.flush();
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() || self.display.lost() {
                return None;
            }
            let watch = Watch {
                fd: self.connection.fd(),
                read: true,
                write: false,
            };
            libc::wait(&[watch], Some(left)).ok()?;
        }
    }

    /// Handles the selection events: requests for the selections of the
    /// application, a lost ownership, and the next chunk of an INCR
    /// transfer. Returns false for other events.
    pub(crate) fn selection_event(&self, event: &Bytes) -> bool {
        match event.u8(0) & 0x7f {
            XCB_SELECTION_REQUEST => {
                self.selection_request(event);
                true
            }
            XCB_SELECTION_CLEAR => {
                if event.u32(12) == self.transfer_atoms.clipboard {
                    let mut selections = self.selections.borrow_mut();
                    selections.source = None;
                    selections.machine.ownership_lost();
                }
                true
            }
            XCB_PROPERTY_NOTIFY if event.u8(16) == XCB_PROPERTY_DELETE => {
                self.continue_send(event.u32(4), event.u32(8))
            }
            _ => false,
        }
    }

    /// Makes `payload` the clipboard contents.
    pub(crate) fn set_selection(&self, payload: Payload) -> Result<(), Error> {
        let clipboard = self.transfer_atoms.clipboard;
        let time = self.server_time();
        self.connection
            .set_selection_owner(self.selection_window, clipboard, time);
        if self.connection.selection_owner(clipboard) != self.selection_window {
            return Err(Error::ClipboardWriteFailed);
        }
        let mimes = payload.mime_types();
        let mut selections = self.selections.borrow_mut();
        // A private payload has no MIME types, but the application still
        // owns the selection.
        let _ = selections
            .machine
            .claim(mimes.iter().map(String::as_str).chain(["private"]));
        selections.source = Some((Rc::new(payload), time));
        Ok(())
    }

    /// The clipboard contents as the application sees it.
    pub(crate) fn selection(&self) -> Contents {
        {
            let selections = self.selections.borrow();
            if selections.machine.owns_selection()
                && let Some((payload, _)) = &selections.source
            {
                return Contents::Local(Rc::clone(payload));
            }
        }
        let clipboard = self.transfer_atoms.clipboard;
        if self.connection.selection_owner(clipboard) == 0 {
            return Contents::Empty;
        }
        let mimes = self.remote_targets(clipboard);
        self.selections
            .borrow_mut()
            .machine
            .receive_offer(mimes.iter().cloned());
        if mimes.is_empty() {
            Contents::Empty
        } else {
            Contents::Remote(mimes)
        }
    }

    /// Reads `mime` of the clipboard from the other owner.
    pub(crate) fn receive(&self, mime: &str) -> Option<Vec<u8>> {
        if self.selections.borrow().machine.read(mime).is_err() {
            return None;
        }
        self.receive_from(self.transfer_atoms.clipboard, mime, 0)
    }

    /// The MIME types of the targets of `selection`. An owner that offers
    /// only Latin-1 `STRING` text offers `text/plain`.
    fn remote_targets(&self, selection: u32) -> Vec<String> {
        let Some((_, bytes)) = self.convert(selection, self.transfer_atoms.targets, 0) else {
            return Vec::new();
        };
        let targets = words(&bytes);
        let mut mimes: Vec<String> = targets
            .iter()
            .filter_map(|atom| self.atom_name(*atom))
            .filter(|name| name.contains('/') || name == "UTF8_STRING")
            .collect();
        if targets.contains(&XCB_ATOM_STRING)
            && !mimes
                .iter()
                .any(|mime| mime.starts_with("text/plain") || mime == "UTF8_STRING")
        {
            mimes.push("text/plain".to_owned());
        }
        mimes
    }

    /// Reads `mime` of `selection` at `time`. `text/plain` falls back to
    /// Latin-1 `STRING` text.
    pub(crate) fn receive_from(&self, selection: u32, mime: &str, time: u32) -> Option<Vec<u8>> {
        let target = self.atom(mime);
        if let Some((_, bytes)) = self.convert(selection, target, time) {
            return Some(bytes);
        }
        if mime != "text/plain" {
            return None;
        }
        let (_, bytes) = self.convert(selection, XCB_ATOM_STRING, time)?;
        Some(
            bytes
                .iter()
                .map(|byte| char::from(*byte))
                .collect::<String>()
                .into_bytes(),
        )
    }

    /// Asks the owner of `selection` for `target` and reads the answer:
    /// its type and its bytes. Follows INCR to its end.
    fn convert(&self, selection: u32, target: u32, time: u32) -> Option<(u32, Vec<u8>)> {
        let (window, property) = (self.selection_window, self.transfer_atoms.property);
        let connection = &self.connection;
        connection.delete_property(window, property);
        connection.convert_selection(window, (selection, target, property), time);
        let notify = self.wait_for(&|event: &Bytes| {
            event.u8(0) & 0x7f == XCB_SELECTION_NOTIFY
                && event.u32(8) == window
                && event.u32(12) == selection
                && event.u32(16) == target
        })?;
        if notify.u32(20) == 0 {
            return None;
        }
        // Taking the property deletes it, which starts an INCR transfer.
        let (kind, _, bytes) = connection.take_property(window, property)?;
        if kind != self.transfer_atoms.incr {
            return Some((kind, bytes));
        }
        let mut data = Vec::new();
        loop {
            self.wait_for(&|event: &Bytes| {
                event.u8(0) & 0x7f == XCB_PROPERTY_NOTIFY
                    && event.u32(4) == window
                    && event.u32(8) == property
                    && event.u8(16) == XCB_PROPERTY_NEW_VALUE
            })?;
            let (kind, _, chunk) = connection.take_property(window, property)?;
            if chunk.is_empty() {
                return Some((kind, data));
            }
            data.extend_from_slice(&chunk);
        }
    }

    /// The payload that the application serves as `selection` for a
    /// request at `time`.
    fn served_payload(&self, selection: u32, time: u32) -> Option<(Rc<Payload>, u32)> {
        let atoms = &self.transfer_atoms;
        let selections = self.selections.borrow();
        if selection == atoms.clipboard {
            let (payload, owned) = selections.source.as_ref()?;
            // A request older than the ownership is for the previous owner.
            (time == 0 || time >= *owned).then(|| (Rc::clone(payload), *owned))
        } else if selection == atoms.xdnd_selection {
            selections
                .drag
                .as_ref()
                .map(|payload| (Rc::clone(payload), time))
        } else {
            None
        }
    }

    /// Answers a `SelectionRequest`. A property of 0 is an obsolete
    /// requestor, which takes the target as the property.
    fn selection_request(&self, event: &Bytes) {
        let (time, requestor, selection, target) =
            (event.u32(4), event.u32(12), event.u32(16), event.u32(20));
        let property = match event.u32(24) {
            0 => target,
            property => property,
        };
        let answered = self
            .served_payload(selection, time)
            .is_some_and(|(payload, owned)| {
                if target == self.transfer_atoms.multiple {
                    self.answer_multiple(&payload, owned, requestor, property)
                } else {
                    self.answer(&payload, owned, (requestor, target, property))
                        .is_some()
                }
            });
        let property = if answered { property } else { 0 };
        let notify = selection_notify(time, requestor, selection, target, property);
        self.connection.send_event(requestor, 0, &notify);
        self.connection.flush();
    }

    /// Answers each pair of target and property in the `ATOM_PAIR`
    /// property of a `MULTIPLE` request. A refused pair gets property 0.
    fn answer_multiple(
        &self,
        payload: &Payload,
        owned: u32,
        requestor: u32,
        property: u32,
    ) -> bool {
        let atom_pair = self.transfer_atoms.atom_pair;
        let mut pairs = self
            .connection
            .get_property32(requestor, property, atom_pair);
        if pairs.len() < 2 {
            return false;
        }
        for pair in pairs.as_chunks_mut::<2>().0 {
            if self
                .answer(payload, owned, (requestor, pair[0], pair[1]))
                .is_none()
            {
                pair[1] = 0;
            }
        }
        self.connection
            .set_property32(requestor, property, atom_pair, &pairs);
        true
    }

    /// Writes `target` of `payload` to `property` of `requestor`.
    fn answer(
        &self,
        payload: &Payload,
        owned: u32,
        (requestor, target, property): (u32, u32, u32),
    ) -> Option<()> {
        let atoms = &self.transfer_atoms;
        let connection = &self.connection;
        if target == atoms.targets {
            let mut list = vec![atoms.targets, atoms.timestamp, atoms.multiple];
            if payload.data(transfer::TEXT).is_some() {
                list.extend([self.atoms.utf8_string, XCB_ATOM_STRING, atoms.text]);
            }
            for mime in payload.mime_types() {
                let atom = self.atom(&mime);
                if atom != 0 && !list.contains(&atom) {
                    list.push(atom);
                }
            }
            connection.set_property32(requestor, property, XCB_ATOM_ATOM, &list);
            return Some(());
        }
        if target == atoms.timestamp {
            connection.set_property32(requestor, property, XCB_ATOM_INTEGER, &[owned]);
            return Some(());
        }
        let (kind, data) = if target == XCB_ATOM_STRING {
            (target, latin1(payload.data(transfer::TEXT)?))
        } else if target == atoms.text || target == self.atoms.utf8_string {
            (
                self.atoms.utf8_string,
                payload.data(transfer::TEXT)?.to_vec(),
            )
        } else {
            let name = self.atom_name(target)?;
            (target, payload.data(&name)?.to_vec())
        };
        self.send_data(requestor, property, kind, data);
        Some(())
    }

    /// Writes `data` in one property, or starts an INCR transfer when it
    /// is larger than a chunk.
    fn send_data(&self, requestor: u32, property: u32, kind: u32, data: Vec<u8>) {
        let connection = &self.connection;
        if data.len() <= self.chunk_bytes() {
            connection.set_property8(requestor, property, kind, &data);
            return;
        }
        // The requestor deletes the property to ask for each chunk.
        connection.change_window_attributes(
            requestor,
            XCB_CW_EVENT_MASK,
            &[XCB_EVENT_MASK_PROPERTY_CHANGE],
        );
        let length = u32::try_from(data.len()).unwrap_or(u32::MAX);
        connection.set_property32(requestor, property, self.transfer_atoms.incr, &[length]);
        let mut selections = self.selections.borrow_mut();
        let now = Instant::now();
        selections
            .sends
            .retain(|send| now.duration_since(send.touched) < SEND_TIMEOUT);
        selections.sends.push(Send {
            requestor,
            property,
            kind,
            data,
            offset: 0,
            touched: now,
        });
    }

    /// The largest chunk that one request carries.
    fn chunk_bytes(&self) -> usize {
        CHUNK_BYTES.min(self.connection.maximum_request_bytes() / 4)
    }

    /// Sends the next chunk of the INCR transfer to `property` of
    /// `requestor`, after the requestor deleted it. Returns false when no
    /// transfer goes there.
    fn continue_send(&self, requestor: u32, property: u32) -> bool {
        let chunk = self.chunk_bytes();
        let mut selections = self.selections.borrow_mut();
        let Some(index) = selections
            .sends
            .iter()
            .position(|send| send.requestor == requestor && send.property == property)
        else {
            return false;
        };
        let send = &mut selections.sends[index];
        let end = (send.offset + chunk).min(send.data.len());
        self.connection
            .set_property8(requestor, property, send.kind, &send.data[send.offset..end]);
        if send.offset == send.data.len() {
            // The chunk of zero length ends the transfer.
            selections.sends.remove(index);
        } else {
            send.offset = end;
            send.touched = Instant::now();
        }
        self.connection.flush();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin1_text_replaces_wider_characters() {
        assert_eq!(latin1("café 👩".as_bytes()), b"caf\xe9 ?".to_vec());
    }

    #[test]
    fn selection_notify_events_carry_the_request() {
        let event = selection_notify(1, 2, 3, 4, 5);
        assert_eq!(event[0], XCB_SELECTION_NOTIFY);
        assert_eq!(words(&event[4..24]), vec![1, 2, 3, 4, 5]);
    }
}
