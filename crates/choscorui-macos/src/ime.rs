use super::*;
use crate::accessibility_bridge::accessibility_screen_rect;
use crate::ffi::Obj;
use crate::ffi::appkit::{
    ATTRIBUTED_STRING_STRING, NSAttributedString, NSView, NSWindow, VIEW_WINDOW,
};
use crate::ffi::foundation::{NSObject, NSString, string};

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

/// The view of a text input callback.
///
/// # Safety
///
/// `view` is nil or the live content view.
unsafe fn callback_view<'a>(view: Id) -> Option<&'a Obj<NSView>> {
    // SAFETY: the caller guarantees the class and the lifetime.
    unsafe { ffi::legacy::borrow(view) }
}

/// The text of an `NSString` or `NSAttributedString` argument.
fn input_string(value: &Obj<NSObject>) -> Option<String> {
    if let Some(text) = value.downcast::<NSString>() {
        return string(text);
    }
    let text = ATTRIBUTED_STRING_STRING.send(value.downcast::<NSAttributedString>()?, ())?;
    string(&text)
}

/// The window state of `view` for a callback.
///
/// # Safety
///
/// `view` is nil or the live content view.
unsafe fn callback_state<'a>(view: Id) -> Option<(&'a Obj<NSWindow>, WindowQueue, ImeHandle)> {
    // SAFETY: the caller guarantees the view.
    view_window_state(unsafe { callback_view(view) }?)
}

fn optional_range(range: NSRange) -> Option<Range<usize>> {
    (range.location != usize::MAX)
        .then_some(range.location..range.location.saturating_add(range.length))
}

pub(super) unsafe extern "C" fn view_has_marked_text(view: Id, _: Sel) -> i8 {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: AppKit passes the live content view.
        unsafe { callback_state(view) }
            .is_some_and(|(_, _, state)| state.lock().unwrap().marked_utf16.is_some())
            .into()
    }))
    .unwrap_or(0)
}

pub(super) unsafe extern "C" fn view_selected_range(view: Id, _: Sel) -> NSRange {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: AppKit passes the live content view.
        unsafe { callback_state(view) }.map_or(
            NSRange {
                location: usize::MAX,
                length: 0,
            },
            |(_, _, state)| {
                let state = state.lock().unwrap();
                NSRange {
                    location: state.selected_utf16.start,
                    length: state
                        .selected_utf16
                        .end
                        .saturating_sub(state.selected_utf16.start),
                }
            },
        )
    }))
    .unwrap_or(NSRange {
        location: usize::MAX,
        length: 0,
    })
}

pub(super) unsafe extern "C" fn view_marked_range(view: Id, _: Sel) -> NSRange {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: AppKit passes the live content view.
        unsafe { callback_state(view) }
            .and_then(|(_, _, state)| state.lock().unwrap().marked_utf16.clone())
            .map_or(
                NSRange {
                    location: usize::MAX,
                    length: 0,
                },
                |range| NSRange {
                    location: range.start,
                    length: range.end.saturating_sub(range.start),
                },
            )
    }))
    .unwrap_or(NSRange {
        location: usize::MAX,
        length: 0,
    })
}

pub(super) unsafe extern "C" fn view_set_marked_text(
    view: Id,
    _: Sel,
    value: Id,
    selected: NSRange,
    replacement: NSRange,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: AppKit passes a live string for the call.
        let Some(text) = unsafe { ffi::legacy::borrow::<NSObject>(value) }.and_then(input_string)
        else {
            return;
        };
        // SAFETY: AppKit passes the live content view.
        let Some((_, queue, state)) = (unsafe { callback_state(view) }) else {
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
    }));
}

pub(super) unsafe extern "C" fn view_insert_text(
    view: Id,
    _: Sel,
    value: Id,
    replacement: NSRange,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: AppKit passes a live string for the call.
        let Some(text) = unsafe { ffi::legacy::borrow::<NSObject>(value) }.and_then(input_string)
        else {
            return;
        };
        // SAFETY: AppKit passes the live content view.
        let Some((_, queue, state)) = (unsafe { callback_state(view) }) else {
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
    }));
}

pub(super) unsafe extern "C" fn view_unmark_text(view: Id, _: Sel) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: AppKit passes the live content view.
        let Some((_, queue, state)) = (unsafe { callback_state(view) }) else {
            return;
        };
        state.lock().unwrap().marked_utf16 = None;
        queue
            .lock()
            .unwrap()
            .push_back(WindowEvent::Ime(ImeEvent::Unmark));
    }));
}

pub(super) unsafe extern "C" fn view_valid_marked_attributes(_: Id, _: Sel) -> Id {
    catch_unwind(AssertUnwindSafe(|| {
        ffi::foundation::ns_array(&[]).map_or(std::ptr::null_mut(), ffi::legacy::autoreleased)
    }))
    .unwrap_or(std::ptr::null_mut())
}

pub(super) unsafe extern "C" fn view_first_rect_for_range(
    view: Id,
    _: Sel,
    range: NSRange,
    actual: *mut NSRange,
) -> NSRect {
    catch_unwind(AssertUnwindSafe(|| {
        if !actual.is_null() {
            unsafe { *actual = range };
        }
        // SAFETY: AppKit passes the live content view.
        let Some(view) = (unsafe { callback_view(view) }) else {
            return NSRect::default();
        };
        let Some((window, _, state)) = view_window_state(view) else {
            return NSRect::default();
        };
        accessibility_screen_rect(view, window, state.lock().unwrap().caret)
    }))
    .unwrap_or_default()
}

pub(super) unsafe extern "C" fn view_character_index_for_point(
    view: Id,
    _: Sel,
    _: NSPoint,
) -> usize {
    // SAFETY: AppKit passes the live content view.
    unsafe { callback_state(view) }.map_or(usize::MAX, |(_, _, state)| {
        state.lock().unwrap().selected_utf16.start
    })
}

pub(super) unsafe extern "C" fn view_attributed_substring(
    _: Id,
    _: Sel,
    _: NSRange,
    _: *mut NSRange,
) -> Id {
    std::ptr::null_mut()
}

pub(super) unsafe extern "C" fn view_do_command(_: Id, _: Sel, _: Sel) {}
