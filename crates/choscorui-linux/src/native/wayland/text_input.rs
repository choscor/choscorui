//! Input methods on `zwp_text_input_v3`.
//!
//! The text input is on while a focused window has a focused text
//! control. Each state change sends the surrounding text, the cursor
//! rectangle, and a `commit`. The compositor sends the changes of the
//! input method as a group that ends with `done`; the group becomes one
//! commit and one marked text, as `ImeEvent`s with UTF-16 ranges.

use super::super::types::{ImeEvent, TextInputState, WindowEvent};
use super::{Client, Role, WaylandWindow};
use crate::ffi::wayland::{Arg, Event, ObjectId, Proxy};
use std::ops::Range;
use std::rc::{Rc, Weak};

// zwp_text_input_manager_v3 requests.
const MANAGER_GET_TEXT_INPUT: u16 = 1;
// zwp_text_input_v3 requests.
const ENABLE: u16 = 1;
const DISABLE: u16 = 2;
const SET_SURROUNDING_TEXT: u16 = 3;
const SET_CONTENT_TYPE: u16 = 5;
const SET_CURSOR_RECTANGLE: u16 = 6;
const COMMIT: u16 = 7;
// zwp_text_input_v3 events.
const ENTER: u16 = 0;
const LEAVE: u16 = 1;
const PREEDIT_STRING: u16 = 2;
const COMMIT_STRING: u16 = 3;
const DELETE_SURROUNDING_TEXT: u16 = 4;
const DONE: u16 = 5;

/// The longest surrounding text that the protocol allows, in bytes.
const SURROUNDING_LIMIT: usize = 4000;

/// The changes of one group, before `done`.
#[derive(Default)]
struct Pending {
    preedit: Option<(String, i32, i32)>,
    commit: Option<String>,
    delete: Option<(u32, u32)>,
}

/// The text input of the seat.
pub(crate) struct TextInput {
    proxy: Proxy,
    /// The window whose surface has the text input focus.
    focus: Option<Weak<WaylandWindow>>,
    enabled: bool,
    /// The number of `commit` requests, which `done` echoes.
    commits: u32,
    pending: Pending,
    /// True while the window shows marked text.
    marked: bool,
    /// The state that the last commit sent.
    sent: Option<TextInputState>,
}

/// The byte offset of UTF-16 offset `utf16` in `text`, clamped to its end.
fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (index, character) in text.char_indices() {
        if units >= utf16 {
            return index;
        }
        units += character.len_utf16();
    }
    text.len()
}

/// The UTF-16 offset of byte offset `byte` in `text`, clamped to a
/// character boundary and to the end.
fn utf16_offset(text: &str, byte: usize) -> usize {
    text.char_indices()
        .take_while(|(index, _)| *index < byte)
        .map(|(_, character)| character.len_utf16())
        .sum()
}

/// At most `SURROUNDING_LIMIT` bytes of `text` around `selection` (byte
/// offsets), on character boundaries: the window, and the selection in
/// it.
fn surrounding(text: &str, selection: Range<usize>) -> (&str, Range<usize>) {
    if text.len() <= SURROUNDING_LIMIT {
        return (text, selection);
    }
    let middle = selection.start / 2 + selection.end / 2;
    let mut start = middle
        .saturating_sub(SURROUNDING_LIMIT / 2)
        .min(selection.start);
    let mut end = (start + SURROUNDING_LIMIT).min(text.len());
    while !text.is_char_boundary(start) {
        start += 1;
    }
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let clamp = |offset: usize| offset.clamp(start, end) - start;
    (
        &text[start..end],
        clamp(selection.start)..clamp(selection.end),
    )
}

impl Client {
    /// Gets the text input of the seat once both globals are bound.
    pub(super) fn ensure_text_input(&self) {
        let globals = self.globals.borrow();
        let (Some(manager), Some(seat)) = (&globals.text_input_manager, &globals.seat) else {
            return;
        };
        let mut text_input = self.text_input.borrow_mut();
        if text_input.is_some() {
            return;
        }
        let proxy = manager
            .request(
                MANAGER_GET_TEXT_INPUT,
                &[Arg::NewId, Arg::Object(Some(seat))],
            )
            .ok()
            .flatten();
        if let Some(proxy) = proxy {
            self.routes.borrow_mut().insert(proxy.id(), Role::TextInput);
            *text_input = Some(TextInput {
                proxy,
                focus: None,
                enabled: false,
                commits: 0,
                pending: Pending::default(),
                marked: false,
                sent: None,
            });
        }
    }

    /// True when the compositor offers input methods.
    pub(crate) fn has_text_input(&self) -> bool {
        self.text_input.borrow().is_some()
    }

    /// Sends the text input state of the window of `surface`, when it has
    /// the text input focus.
    pub(crate) fn text_input_changed(&self, surface: ObjectId) {
        let mut slot = self.text_input.borrow_mut();
        let Some(input) = slot.as_mut() else {
            return;
        };
        let Some(window) = input.focus.as_ref().and_then(Weak::upgrade) else {
            return;
        };
        if window.surface.id() != surface {
            return;
        }
        let state = window.shared.text_input.borrow().clone();
        if input.sent.as_ref() == Some(&state) && input.enabled == state.active {
            return;
        }
        if state.active {
            if !input.enabled {
                let _ = input.proxy.request(ENABLE, &[]);
                // Neither hint nor purpose: plain text.
                let _ = input
                    .proxy
                    .request(SET_CONTENT_TYPE, &[Arg::Uint(0), Arg::Uint(0)]);
                input.enabled = true;
            }
            // Marked text is not part of the surrounding text, so the
            // text goes only while nothing is marked.
            if state.marked_utf16.is_none() {
                let selection = byte_offset(&state.text, state.selected_utf16.start)
                    ..byte_offset(&state.text, state.selected_utf16.end);
                let (text, selection) = surrounding(&state.text, selection);
                let _ = input.proxy.request(
                    SET_SURROUNDING_TEXT,
                    &[
                        Arg::Str(Some(text)),
                        Arg::Int(i32::try_from(selection.end).unwrap_or(0)),
                        Arg::Int(i32::try_from(selection.start).unwrap_or(0)),
                    ],
                );
            }
            let caret = state.caret;
            let _ = input.proxy.request(
                SET_CURSOR_RECTANGLE,
                &[
                    Arg::Int(caret.origin.x.round() as i32),
                    Arg::Int(caret.origin.y.round() as i32),
                    Arg::Int(caret.size.width.round().max(1.0) as i32),
                    Arg::Int(caret.size.height.round().max(1.0) as i32),
                ],
            );
        } else if input.enabled {
            let _ = input.proxy.request(DISABLE, &[]);
            input.enabled = false;
        } else {
            input.sent = Some(state);
            return;
        }
        let _ = input.proxy.request(COMMIT, &[]);
        input.commits = input.commits.wrapping_add(1);
        input.sent = Some(state);
        drop(slot);
        self.flush();
    }

    /// Handles an event of the text input.
    pub(super) fn text_input_event(&self, event: &Event) {
        match event.opcode {
            ENTER => {
                let window = event
                    .arg(0)
                    .object()
                    .and_then(|surface| self.window_of(surface));
                let surface = window.as_ref().map(|window| window.surface.id());
                if let Some(input) = self.text_input.borrow_mut().as_mut() {
                    input.focus = window.as_ref().map(Rc::downgrade);
                    input.sent = None;
                    input.enabled = false;
                }
                if let Some(surface) = surface {
                    self.text_input_changed(surface);
                }
            }
            LEAVE => {
                let mut slot = self.text_input.borrow_mut();
                if let Some(input) = slot.as_mut() {
                    if input.enabled {
                        let _ = input.proxy.request(DISABLE, &[]);
                        let _ = input.proxy.request(COMMIT, &[]);
                        input.commits = input.commits.wrapping_add(1);
                    }
                    input.focus = None;
                    input.enabled = false;
                    input.sent = None;
                    input.pending = Pending::default();
                }
            }
            PREEDIT_STRING => {
                if let Some(input) = self.text_input.borrow_mut().as_mut() {
                    input.pending.preedit = event
                        .arg(0)
                        .str_opt()
                        .map(|text| (text.to_owned(), event.arg(1).int(), event.arg(2).int()));
                }
            }
            COMMIT_STRING => {
                if let Some(input) = self.text_input.borrow_mut().as_mut() {
                    input.pending.commit = event.arg(0).str_opt().map(str::to_owned);
                }
            }
            DELETE_SURROUNDING_TEXT => {
                if let Some(input) = self.text_input.borrow_mut().as_mut() {
                    input.pending.delete = Some((event.arg(0).uint(), event.arg(1).uint()));
                }
            }
            DONE => self.text_input_done(),
            _ => {}
        }
    }

    /// Applies one group of changes: the deletion and the commit, then the
    /// new marked text.
    fn text_input_done(&self) {
        let (window, pending, was_marked) = {
            let mut slot = self.text_input.borrow_mut();
            let Some(input) = slot.as_mut() else {
                return;
            };
            let pending = std::mem::take(&mut input.pending);
            let marked = pending
                .preedit
                .as_ref()
                .is_some_and(|(text, _, _)| !text.is_empty());
            let was_marked = std::mem::replace(&mut input.marked, marked);
            (
                input.focus.as_ref().and_then(Weak::upgrade),
                pending,
                was_marked,
            )
        };
        let Some(window) = window else {
            return;
        };
        let state = window.shared.text_input.borrow().clone();
        let mut events = Vec::new();
        if pending.commit.is_some() || pending.delete.is_some() {
            let replacement = pending.delete.map(|(before, after)| {
                let start = byte_offset(&state.text, state.selected_utf16.start);
                let end = byte_offset(&state.text, state.selected_utf16.end);
                let from = start.saturating_sub(before as usize);
                let to = (end + after as usize).min(state.text.len());
                utf16_offset(&state.text, from)..utf16_offset(&state.text, to)
            });
            events.push(ImeEvent::CommitText {
                text: pending.commit.unwrap_or_default(),
                replacement_utf16: replacement,
            });
        }
        match pending.preedit {
            Some((text, begin, end)) if !text.is_empty() => {
                // A negative cursor hides it; the caret then goes to the end.
                let to_utf16 = |offset: i32| {
                    usize::try_from(offset).map_or(text.encode_utf16().count(), |byte| {
                        utf16_offset(&text, byte)
                    })
                };
                let (begin, end) = (to_utf16(begin), to_utf16(end));
                events.push(ImeEvent::SetMarkedText {
                    selected_utf16: begin.min(end)..begin.max(end),
                    text,
                    replacement_utf16: None,
                });
            }
            // The input method cleared its marked text without a commit.
            _ if was_marked && events.is_empty() => events.push(ImeEvent::SetMarkedText {
                text: String::new(),
                selected_utf16: 0..0,
                replacement_utf16: None,
            }),
            _ => {}
        }
        for event in events {
            window.shared.push(WindowEvent::Ime(event));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_convert_between_bytes_and_utf16() {
        let text = "aé👩b";
        assert_eq!(byte_offset(text, 0), 0);
        assert_eq!(byte_offset(text, 2), 3);
        assert_eq!(byte_offset(text, 4), 7);
        assert_eq!(byte_offset(text, 99), text.len());
        assert_eq!(utf16_offset(text, 3), 2);
        assert_eq!(utf16_offset(text, 7), 4);
        // A byte inside a character rounds up to its end.
        assert_eq!(utf16_offset(text, 4), 4);
    }

    #[test]
    fn long_text_sends_a_window_around_the_selection() {
        let text = "é".repeat(5000);
        let (window, selection) = surrounding(&text, 6000..6002);
        assert!(window.len() <= SURROUNDING_LIMIT);
        assert_eq!(&window[selection], "é");
        let short = "hello";
        assert_eq!(surrounding(short, 1..3), (short, 1..3));
    }
}
