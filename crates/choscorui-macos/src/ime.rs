use super::*;
use crate::accessibility_bridge::accessibility_screen_rect;
use crate::ffi::Obj;
use crate::ffi::appkit::{
    ATTRIBUTED_STRING_STRING, ChoscoruiV2ContentView, NSAttributedString, NSView, NSWindow,
    VIEW_WINDOW,
};
use crate::ffi::class::Sel;
use crate::ffi::foundation::{NSArray, NSObject, NSString, ns_array, string};
use crate::ffi::retained::Retained;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImeEvent {
    SetMarkedText {
        text: String,
        selected_utf16: Range<usize>,
        replacement_utf16: Option<Range<usize>>,
    },
    CommitText {
        text: String,
        replacement_utf16: Option<Range<usize>>,
    },
    Unmark,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextInputState {
    pub selected_utf16: Range<usize>,
    pub marked_utf16: Option<Range<usize>>,
    pub caret: Rect,
    /// The focused control takes every key, before the key equivalents
    /// of the menus, for example while it records a shortcut.
    pub captures_keys: bool,
}

pub(super) type ImeHandle = Arc<Mutex<TextInputState>>;
static IME_STATES: OnceLock<Mutex<HashMap<usize, ImeHandle>>> = OnceLock::new();

pub(super) fn ime_states() -> &'static Mutex<HashMap<usize, ImeHandle>> {
    IME_STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The window, event queue, and text input state of `view`.
fn view_window_state(view: &Obj<NSView>) -> Option<(&Obj<NSWindow>, WindowQueue, ImeHandle)> {
    let window = VIEW_WINDOW.send(view, ())?;
    let queue = window_queues()
        .lock()
        .unwrap()
        .get(&window.addr())
        .cloned()?;
    let ime = ime_states().lock().unwrap().get(&window.addr()).cloned()?;
    Some((window, queue, ime))
}

/// The text of an `NSString` or `NSAttributedString` argument.
fn input_string(value: &Obj<NSObject>) -> Option<String> {
    if let Some(text) = value.downcast::<NSString>() {
        return string(text);
    }
    let text = ATTRIBUTED_STRING_STRING.send(value.downcast::<NSAttributedString>()?, ())?;
    string(&text)
}

fn optional_range(range: NSRange) -> Option<Range<usize>> {
    (range.location != usize::MAX)
        .then_some(range.location..range.location.saturating_add(range.length))
}

/// The range of `range`, or [`NSRange::NONE`] without one.
fn native_range(range: Option<Range<usize>>) -> NSRange {
    range.map_or(NSRange::NONE, |range| NSRange {
        location: range.start,
        length: range.end.saturating_sub(range.start),
    })
}

/// Sends a key equivalent to the focused control as a key event while
/// the control captures keys, so no menu item takes it.
pub(super) fn view_perform_key_equivalent(
    view: &Obj<ChoscoruiV2ContentView>,
    event: Option<&Obj<NSEvent>>,
) -> bool {
    let capturing =
        view_window_state(view).is_some_and(|(_, _, state)| state.lock().unwrap().captures_keys);
    if capturing && event.is_some_and(|event| EVENT_TYPE.send(event, ()) == EVENT_KEY_DOWN) {
        crate::view_key_event(view, event);
    }
    capturing
}

pub(super) fn view_has_marked_text(view: &Obj<ChoscoruiV2ContentView>) -> bool {
    view_window_state(view)
        .is_some_and(|(_, _, state)| state.lock().unwrap().marked_utf16.is_some())
}

pub(super) fn view_selected_range(view: &Obj<ChoscoruiV2ContentView>) -> NSRange {
    native_range(
        view_window_state(view).map(|(_, _, state)| state.lock().unwrap().selected_utf16.clone()),
    )
}

pub(super) fn view_marked_range(view: &Obj<ChoscoruiV2ContentView>) -> NSRange {
    native_range(
        view_window_state(view)
            .and_then(|(_, _, state)| state.lock().unwrap().marked_utf16.clone()),
    )
}

pub(super) fn view_set_marked_text(
    view: &Obj<ChoscoruiV2ContentView>,
    value: Option<&Obj<NSObject>>,
    selected: NSRange,
    replacement: NSRange,
) {
    let Some(text) = value.and_then(input_string) else {
        return;
    };
    let Some((_, queue, state)) = view_window_state(view) else {
        return;
    };
    let selected_utf16 = selected.location..selected.location.saturating_add(selected.length);
    let mut state = state.lock().unwrap();
    let start = optional_range(replacement)
        .map(|range| range.start)
        .unwrap_or(state.selected_utf16.start);
    state.marked_utf16 = Some(start..start.saturating_add(text.encode_utf16().count()));
    drop(state);
    queue
        .lock()
        .unwrap()
        .push_back(WindowEvent::Ime(ImeEvent::SetMarkedText {
            text,
            selected_utf16,
            replacement_utf16: optional_range(replacement),
        }));
}

pub(super) fn view_insert_text(
    view: &Obj<ChoscoruiV2ContentView>,
    value: Option<&Obj<NSObject>>,
    replacement: NSRange,
) {
    let Some(text) = value.and_then(input_string) else {
        return;
    };
    let Some((_, queue, state)) = view_window_state(view) else {
        return;
    };
    state.lock().unwrap().marked_utf16 = None;
    queue
        .lock()
        .unwrap()
        .push_back(WindowEvent::Ime(ImeEvent::CommitText {
            text,
            replacement_utf16: optional_range(replacement),
        }));
}

pub(super) fn view_unmark_text(view: &Obj<ChoscoruiV2ContentView>) {
    let Some((_, queue, state)) = view_window_state(view) else {
        return;
    };
    state.lock().unwrap().marked_utf16 = None;
    queue
        .lock()
        .unwrap()
        .push_back(WindowEvent::Ime(ImeEvent::Unmark));
}

pub(super) fn view_valid_marked_attributes(
    _: &Obj<ChoscoruiV2ContentView>,
) -> Option<Retained<NSArray>> {
    ns_array(&[]).ok()
}

pub(super) fn view_first_rect_for_range(
    view: &Obj<ChoscoruiV2ContentView>,
    range: NSRange,
    actual: Option<&mut NSRange>,
) -> NSRect {
    if let Some(actual) = actual {
        *actual = range;
    }
    let Some((window, _, state)) = view_window_state(view) else {
        return NSRect::default();
    };
    accessibility_screen_rect(view, window, state.lock().unwrap().caret)
}

pub(super) fn view_character_index_for_point(
    view: &Obj<ChoscoruiV2ContentView>,
    _: NSPoint,
) -> usize {
    view_window_state(view).map_or(usize::MAX, |(_, _, state)| {
        state.lock().unwrap().selected_utf16.start
    })
}

pub(super) fn view_attributed_substring(
    _: &Obj<ChoscoruiV2ContentView>,
    _: NSRange,
    _: Option<&mut NSRange>,
) -> Option<Retained<NSAttributedString>> {
    None
}

pub(super) fn view_do_command(_: &Obj<ChoscoruiV2ContentView>, _: Option<Sel>) {}
