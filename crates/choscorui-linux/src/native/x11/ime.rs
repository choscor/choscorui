//! Input methods with XIM, on-the-spot.
//!
//! The input method runs on its own Xlib connection (see `ffi::xlib`).
//! A window has an input context while its text input state is active.
//! The context has the focus while the window has it, and its spot is the
//! bottom-left corner of the caret. Each key of such a window goes to the
//! input method first. The keys that it does not use come back on the
//! Xlib connection, with the committed text. The preedit callbacks give
//! the marked text, which becomes `ImeEvent`s with UTF-16 ranges.

use super::{Client, X11Surface};
use crate::ffi::x11::Bytes;
use crate::ffi::xlib::{self, ImEvent, InputContext, InputMethod, KeyInput, PreeditChange};
use crate::native::types::{ImeEvent, WindowEvent};
use std::collections::HashMap;
use std::rc::Rc;

/// The input context of a window, and whether it has the focus.
struct Context {
    context: InputContext,
    focused: bool,
}

/// The input method and the input contexts of the windows.
pub(crate) struct Ime {
    method: Rc<InputMethod>,
    contexts: HashMap<u32, Context>,
    /// The marked text of each window, in characters.
    marked: HashMap<u32, Vec<char>>,
}

impl Ime {
    /// Opens the input method of `XMODIFIERS`, if it takes preedit
    /// callbacks.
    pub(crate) fn open() -> Option<Self> {
        Some(Self {
            method: InputMethod::open()?,
            contexts: HashMap::new(),
            marked: HashMap::new(),
        })
    }
}

/// Applies a preedit draw to `marked`, and returns the marked text and
/// the UTF-16 offset of the caret.
fn draw(
    marked: &mut Vec<char>,
    (caret, first, length): (usize, usize, usize),
    text: &str,
) -> (String, usize) {
    let first = first.min(marked.len());
    let end = first.saturating_add(length).min(marked.len());
    marked.splice(first..end, text.chars());
    let caret = marked[..caret.min(marked.len())]
        .iter()
        .map(|character| character.len_utf16())
        .sum();
    (marked.iter().collect(), caret)
}

impl Client {
    pub(crate) fn has_input_method(&self) -> bool {
        self.ime.borrow().is_some()
    }

    /// The input method, for the event loop.
    pub(crate) fn input_method(&self) -> Option<Rc<InputMethod>> {
        self.ime.borrow().as_ref().map(|ime| Rc::clone(&ime.method))
    }

    /// Creates, focuses, places, or destroys the input context of
    /// `window` after a change of its text input state.
    pub(crate) fn text_input_changed(&self, window: &X11Surface) {
        let state = window.shared.text_input.borrow().clone();
        let focused = window.shared.focused.get();
        let mut slot = self.ime.borrow_mut();
        let Some(ime) = slot.as_mut() else {
            return;
        };
        if !state.active {
            let removed = ime.contexts.remove(&window.id);
            let marked = ime
                .marked
                .remove(&window.id)
                .is_some_and(|marked| !marked.is_empty());
            drop(slot);
            drop(removed);
            // The destroyed context may still report its last changes.
            xlib::take_preedit_changes();
            if marked {
                window.shared.push(WindowEvent::Ime(ImeEvent::Unmark));
            }
            return;
        }
        if !ime.contexts.contains_key(&window.id) {
            let Some(context) = ime.method.create_context(window.id) else {
                return;
            };
            ime.contexts.insert(
                window.id,
                Context {
                    context,
                    focused: false,
                },
            );
        }
        let Some(context) = ime.contexts.get_mut(&window.id) else {
            return;
        };
        if context.focused != focused {
            context.context.set_focus(focused);
            context.focused = focused;
        }
        let scale = window.shared.scale.get();
        let caret = state.caret;
        let spot = |value: f64| (value * scale).round().clamp(0.0, f64::from(i16::MAX)) as i16;
        context.context.set_spot((
            spot(caret.origin.x),
            spot(caret.origin.y + caret.size.height),
        ));
    }

    /// Gives the focus to the input context of `id`, or takes it away.
    pub(crate) fn ime_focus(&self, id: u32, focused: bool) {
        if let Some(context) = self
            .ime
            .borrow_mut()
            .as_mut()
            .and_then(|ime| ime.contexts.get_mut(&id))
            .filter(|context| context.focused != focused)
        {
            context.context.set_focus(focused);
            context.focused = focused;
        }
    }

    /// Drops the input context of a closed window.
    pub(crate) fn ime_forget(&self, id: u32) {
        let removed = self.ime.borrow_mut().as_mut().map(|ime| {
            ime.marked.remove(&id);
            ime.contexts.remove(&id)
        });
        drop(removed);
        xlib::take_preedit_changes();
    }

    /// Gives an xcb key event of `window` to the input method. Returns
    /// true when the input method took it.
    pub(crate) fn ime_filter(&self, window: u32, event: &Bytes, pressed: bool) -> bool {
        let Some(method) = self.input_method() else {
            return false;
        };
        let focused = self
            .ime
            .borrow()
            .as_ref()
            .and_then(|ime| ime.contexts.get(&window))
            .is_some_and(|context| context.focused);
        if !focused {
            return false;
        }
        let taken = method.filter_key(KeyInput {
            pressed,
            window,
            root: event.u32(8),
            time: event.u32(4),
            position: (event.i16(24), event.i16(26)),
            root_position: (event.i16(20), event.i16(22)),
            state: event.u16(28),
            keycode: event.u8(1),
        });
        self.apply_preedit_changes();
        taken
    }

    /// Handles what the input method sent on its connection: the keys that
    /// it did not use, the committed text, and the preedit changes.
    pub(crate) fn ime_dispatch(&self) {
        let Some(method) = self.input_method() else {
            return;
        };
        let events = method.events(&|window| {
            self.ime
                .borrow()
                .as_ref()?
                .contexts
                .get(&window)
                .map(|context| context.context.raw())
        });
        // The changes before a commit come first.
        self.apply_preedit_changes();
        for event in events {
            match event {
                ImEvent::Key {
                    window,
                    keycode,
                    pressed,
                } => self.key_code(u32::from(keycode), pressed, window),
                ImEvent::Commit { window, text } => {
                    if let Some(ime) = self.ime.borrow_mut().as_mut() {
                        ime.marked.remove(&window);
                    }
                    if let Some(window) = self.window(window) {
                        window.shared.push(WindowEvent::Ime(ImeEvent::CommitText {
                            text,
                            replacement_utf16: None,
                        }));
                    }
                }
            }
        }
        self.apply_preedit_changes();
    }

    /// Turns the queued preedit changes into `ImeEvent`s.
    fn apply_preedit_changes(&self) {
        for change in xlib::take_preedit_changes() {
            let (id, event) = {
                let mut slot = self.ime.borrow_mut();
                let Some(ime) = slot.as_mut() else {
                    return;
                };
                match change {
                    PreeditChange::Start(id) => {
                        ime.marked.insert(id, Vec::new());
                        continue;
                    }
                    PreeditChange::Draw {
                        window,
                        caret,
                        first,
                        length,
                        text,
                    } => {
                        let marked = ime.marked.entry(window).or_default();
                        let (text, caret) = draw(marked, (caret, first, length), &text);
                        let event = ImeEvent::SetMarkedText {
                            text,
                            selected_utf16: caret..caret,
                            replacement_utf16: None,
                        };
                        (window, event)
                    }
                    PreeditChange::Done(id) => {
                        if ime.marked.remove(&id).is_none() {
                            continue;
                        }
                        (id, ImeEvent::Unmark)
                    }
                }
            };
            if let Some(window) = self.window(id) {
                window.shared.push(WindowEvent::Ime(event));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_splice_the_marked_text_and_count_the_caret_in_utf16() {
        let mut marked = Vec::new();
        assert_eq!(draw(&mut marked, (2, 0, 0), "xin"), ("xin".to_owned(), 2));
        assert_eq!(draw(&mut marked, (4, 1, 2), "👩a"), ("x👩a".to_owned(), 4));
        // A change past the end clamps to it.
        assert_eq!(draw(&mut marked, (9, 7, 4), "!"), ("x👩a!".to_owned(), 5));
        assert_eq!(draw(&mut marked, (0, 0, 9), ""), (String::new(), 0));
    }
}
