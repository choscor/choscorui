use super::*;

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

fn view_window_state(view: Id) -> Option<(usize, WindowQueue, ImeHandle)> {
    let window = unsafe { send_id(view, c"window") } as usize;
    if window == 0 {
        return None;
    }
    let queue = window_queues().lock().unwrap().get(&window).cloned()?;
    let ime = ime_states().lock().unwrap().get(&window).cloned()?;
    Some((window, queue, ime))
}

unsafe fn input_string(value: Id) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let string = if unsafe { send_bool_id(value, c"isKindOfClass:", class(c"NSString").ok()?) } != 0
    {
        value
    } else {
        unsafe { send_id(value, c"string") }
    };
    let bytes = unsafe { send_cstr(string, c"UTF8String") };
    (!bytes.is_null()).then(|| {
        unsafe { CStr::from_ptr(bytes) }
            .to_string_lossy()
            .into_owned()
    })
}

fn optional_range(range: NSRange) -> Option<Range<usize>> {
    (range.location != usize::MAX)
        .then_some(range.location..range.location.saturating_add(range.length))
}

pub(super) unsafe extern "C" fn view_has_marked_text(view: Id, _: Sel) -> i8 {
    catch_unwind(AssertUnwindSafe(|| {
        view_window_state(view)
            .is_some_and(|(_, _, state)| state.lock().unwrap().marked_utf16.is_some())
            .into()
    }))
    .unwrap_or(0)
}

pub(super) unsafe extern "C" fn view_selected_range(view: Id, _: Sel) -> NSRange {
    catch_unwind(AssertUnwindSafe(|| {
        view_window_state(view).map_or(
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
        view_window_state(view)
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
        let Some(text) = (unsafe { input_string(value) }) else {
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
    }));
}

pub(super) unsafe extern "C" fn view_insert_text(
    view: Id,
    _: Sel,
    value: Id,
    replacement: NSRange,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let Some(text) = (unsafe { input_string(value) }) else {
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
    }));
}

pub(super) unsafe extern "C" fn view_unmark_text(view: Id, _: Sel) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let Some((_, queue, state)) = view_window_state(view) else {
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
        ns_array(&[]).unwrap_or(std::ptr::null_mut())
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
        let Some((window, _, state)) = view_window_state(view) else {
            return NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: 0.0,
                    height: 0.0,
                },
            };
        };
        unsafe { accessibility_screen_rect(view, window as Id, state.lock().unwrap().caret) }
    }))
    .unwrap_or(NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size: NSSize {
            width: 0.0,
            height: 0.0,
        },
    })
}

pub(super) unsafe extern "C" fn view_character_index_for_point(
    view: Id,
    _: Sel,
    _: NSPoint,
) -> usize {
    view_window_state(view).map_or(usize::MAX, |(_, _, state)| {
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
