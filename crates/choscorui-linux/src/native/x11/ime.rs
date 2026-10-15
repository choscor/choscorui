//! Input methods with XIM, on-the-spot.
//!
//! The input method runs on its own Xlib connection (see `ffi::xlib`).
//! A window has an input context while its text input state is active.
//! The context has the focus while the window has it, and its spot is the
//! bottom-left corner of the caret. Each key of such a window goes to the
//! input method first. The keys that it does not use come back on the
//! Xlib connection, with the committed text. The preedit callbacks give
//! the marked text. `Composition` turns these events into `ImeEvent`s
//! with UTF-16 ranges.
//!
//! When the input method stops, the marked text ends with `Unmark` and
//! the contexts go. The connection then waits for an input method again.
//! When one opens, each window with an active text input state gets a
//! new context.

use super::{Client, X11Surface};
use crate::ffi::x11::Bytes;
use crate::ffi::xlib::{
    self, ImEvent, InputContext, InputMethod, KeyInput, Opened, PreeditChange, XimDisplay,
};
use crate::native::types::{ImeEvent, WindowEvent};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::rc::Rc;

/// The input context of a window, and whether it has the focus.
struct Context {
    context: InputContext,
    focused: bool,
}

/// The marked text of each window, in characters. It turns the events of
/// the input method into `ImeEvent`s, without calls into Xlib.
#[derive(Default)]
pub(crate) struct Composition {
    marked: HashMap<u32, Vec<char>>,
}

impl Composition {
    /// The `ImeEvent` of an event of the input method, and its window.
    /// A key gives none, because it goes to the keyboard.
    pub(crate) fn convert(&mut self, event: ImEvent) -> Option<(u32, ImeEvent)> {
        match event {
            ImEvent::Key { .. } => None,
            ImEvent::Commit { window, text } => {
                self.marked.remove(&window);
                let event = ImeEvent::CommitText {
                    text,
                    replacement_utf16: None,
                };
                Some((window, event))
            }
            ImEvent::Preedit(PreeditChange::Start(window)) => {
                self.marked.insert(window, Vec::new());
                None
            }
            ImEvent::Preedit(PreeditChange::Draw {
                window,
                caret,
                first,
                length,
                text,
            }) => {
                // A draw that clears the marked text after a commit or
                // after the end changes nothing.
                if text.is_empty() && !self.marked.contains_key(&window) {
                    return None;
                }
                let marked = self.marked.entry(window).or_default();
                let (text, caret) = draw(marked, (caret, first, length), &text);
                let event = ImeEvent::SetMarkedText {
                    text,
                    selected_utf16: caret..caret,
                    replacement_utf16: None,
                };
                Some((window, event))
            }
            ImEvent::Preedit(PreeditChange::Done(window)) => {
                self.end(window).map(|event| (window, event))
            }
        }
    }

    /// Ends the marked text of `window`: `Unmark` when a composition is in
    /// progress.
    pub(crate) fn end(&mut self, window: u32) -> Option<ImeEvent> {
        self.marked.remove(&window).map(|_| ImeEvent::Unmark)
    }

    /// Ends the marked text of every window, by window id.
    pub(crate) fn end_all(&mut self) -> Vec<(u32, ImeEvent)> {
        let mut windows: Vec<u32> = self.marked.drain().map(|(window, _)| window).collect();
        windows.sort_unstable();
        windows
            .into_iter()
            .map(|window| (window, ImeEvent::Unmark))
            .collect()
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

/// What an instantiate callback opened.
enum Instantiated {
    /// The callback did not run, or no input method opened.
    Nothing,
    /// An input method with preedit callbacks.
    Method,
    /// An input method without preedit callbacks, which closed again.
    Unusable,
}

/// The Xlib connection, its input method, and the input contexts of the
/// windows.
pub(crate) struct Ime {
    display: Rc<XimDisplay>,
    method: Option<Rc<InputMethod>>,
    contexts: HashMap<u32, Context>,
    composition: Composition,
}

impl Ime {
    /// Opens the Xlib connection and the input method of `XMODIFIERS`.
    /// Without an input method, the connection waits for one. Returns None
    /// without libX11 or a UTF-8 locale, and when the input method takes
    /// no preedit callbacks, as the local input method of Xlib.
    pub(crate) fn open() -> Option<Self> {
        let display = XimDisplay::open()?;
        if !display.watch() {
            return None;
        }
        let mut ime = Self {
            display,
            method: None,
            contexts: HashMap::new(),
            composition: Composition::default(),
        };
        match ime.instantiate() {
            Instantiated::Unusable => None,
            Instantiated::Nothing | Instantiated::Method => Some(ime),
        }
    }

    /// Opens the input method after the instantiate callback ran. The
    /// registration ends when an input method opens.
    fn instantiate(&mut self) -> Instantiated {
        if self.method.is_some() || !self.display.take_instantiable() {
            return Instantiated::Nothing;
        }
        match InputMethod::open(&self.display) {
            Opened::Method(method) => {
                self.display.unwatch();
                self.method = Some(method);
                Instantiated::Method
            }
            Opened::Unusable => {
                self.display.unwatch();
                Instantiated::Unusable
            }
            Opened::Missing => Instantiated::Nothing,
        }
    }
}

impl Client {
    pub(crate) fn has_input_method(&self) -> bool {
        self.ime
            .borrow()
            .as_ref()
            .is_some_and(|ime| ime.method.is_some())
    }

    /// The Xlib connection of the input method, for the event loop.
    pub(crate) fn xim_display(&self) -> Option<Rc<XimDisplay>> {
        self.ime
            .borrow()
            .as_ref()
            .map(|ime| Rc::clone(&ime.display))
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
            let unmark = ime.composition.end(window.id);
            drop(slot);
            drop(removed);
            // The destroyed context may still report its last changes.
            xlib::take_preedit_changes();
            if let Some(event) = unmark {
                window.shared.push(WindowEvent::Ime(event));
            }
            return;
        }
        let Some(method) = ime.method.as_ref() else {
            return;
        };
        let context = match ime.contexts.entry(window.id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let Some(context) = method.create_context(window.id) else {
                    return;
                };
                entry.insert(Context {
                    context,
                    focused: false,
                })
            }
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
            ime.composition.end(id);
            ime.contexts.remove(&id)
        });
        drop(removed);
        xlib::take_preedit_changes();
    }

    /// Gives an xcb key event of `window` to the input method. Returns
    /// true when the input method took it.
    pub(crate) fn ime_filter(&self, window: u32, event: &Bytes, pressed: bool) -> bool {
        let Some(display) = self.xim_display() else {
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
        let taken = display.filter_key(KeyInput {
            pressed,
            window,
            root: event.u32(8),
            time: event.u32(4),
            position: (event.i16(24), event.i16(26)),
            root_position: (event.i16(20), event.i16(22)),
            state: event.u16(28),
            keycode: event.u8(1),
        });
        for change in xlib::take_preedit_changes() {
            self.ime_push(ImEvent::Preedit(change));
        }
        self.ime_refresh();
        taken
    }

    /// Handles what arrived on the Xlib connection, in order: the keys
    /// that the input method did not use, the committed text, and the
    /// preedit changes. Then follows a stop or a start of the input
    /// method.
    pub(crate) fn ime_dispatch(&self) {
        let Some(display) = self.xim_display() else {
            return;
        };
        let events = display.events(&|window| {
            self.ime
                .borrow()
                .as_ref()?
                .contexts
                .get(&window)
                .and_then(|context| context.context.raw())
        });
        for event in events {
            match event {
                ImEvent::Key {
                    window,
                    keycode,
                    pressed,
                } => self.key_code(u32::from(keycode), pressed, window),
                event => self.ime_push(event),
            }
        }
        self.ime_refresh();
    }

    /// Gives the `ImeEvent` of an event of the input method to its
    /// window.
    fn ime_push(&self, event: ImEvent) {
        let converted = self
            .ime
            .borrow_mut()
            .as_mut()
            .and_then(|ime| ime.composition.convert(event));
        if let Some((id, event)) = converted
            && let Some(window) = self.window(id)
        {
            window.shared.push(WindowEvent::Ime(event));
        }
    }

    /// Ends the marked text and drops the contexts after the input method
    /// stopped. Opens the input method when the instantiate callback ran,
    /// and gives each window with an active text input a context.
    fn ime_refresh(&self) {
        let (lost, unmarks, opened) = {
            let mut slot = self.ime.borrow_mut();
            let Some(ime) = slot.as_mut() else {
                return;
            };
            let mut lost = HashMap::new();
            let mut unmarks = Vec::new();
            if ime.method.as_ref().is_some_and(|method| !method.is_alive()) {
                // Xlib destroyed the method and its contexts, so the drops
                // call no Xlib function.
                lost = std::mem::take(&mut ime.contexts);
                ime.method = None;
                unmarks = ime.composition.end_all();
                // Without a registration, the client stays without an
                // input method.
                ime.display.watch();
            }
            let opened = matches!(ime.instantiate(), Instantiated::Method);
            (lost, unmarks, opened)
        };
        drop(lost);
        for (id, event) in unmarks {
            if let Some(window) = self.window(id) {
                window.shared.push(WindowEvent::Ime(event));
            }
        }
        if opened {
            let windows: Vec<Rc<X11Surface>> = self.windows.borrow().clone();
            for window in windows {
                self.text_input_changed(&window);
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

    #[test]
    fn a_stop_of_the_input_method_unmarks_each_composition() {
        let mut composition = Composition::default();
        for window in [9, 4] {
            composition.convert(ImEvent::Preedit(PreeditChange::Start(window)));
        }
        composition.convert(ImEvent::Preedit(PreeditChange::Draw {
            window: 4,
            caret: 1,
            first: 0,
            length: 0,
            text: "é".to_owned(),
        }));
        assert_eq!(
            composition.end_all(),
            vec![(4, ImeEvent::Unmark), (9, ImeEvent::Unmark)]
        );
        assert!(composition.end_all().is_empty());
        // The end of a window without a composition gives nothing.
        assert_eq!(composition.end(4), None);
        assert_eq!(
            composition.convert(ImEvent::Preedit(PreeditChange::Done(4))),
            None
        );
    }
}
