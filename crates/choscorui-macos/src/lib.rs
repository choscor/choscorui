//! AppKit window bootstrap. All public handles stay on the main thread.

#![cfg(target_os = "macos")]
#![deny(unsafe_code)]

use choscorui_core::{
    Color, DisplayCommand, DisplayList, Key, KeyEvent, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticChange, SemanticEntry, SemanticRelation, SemanticRole, SemanticText,
    Size, TextBuffer, WheelUnit,
};
mod ime;
pub use choscorui_core::{
    MenuBarModel, MenuCommandId, MenuEntry, MenuError, MenuItemState, MenuShortcut,
    MenuShortcutModifiers, MenuSpec, WindowOptions,
};
pub use ime::{ImeEvent, TextInputState};
mod backdrop;
mod dialog;
mod menu_native;
mod paint;
pub use dialog::{
    AlertButton, AlertResponse, AlertSpec, AlertStyle, DialogError, FileDialogMode,
    FileDialogResponse, FileDialogSpec,
};
use ime::{ImeHandle, ime_states};
pub use menu_native::{ContextMenu, MainMenu};
mod desktop;
pub use desktop::{
    AppEvent, DesktopError, NotificationPermission, StatusItem, capabilities,
    has_bundle_identifier, notification_permission, open_url, post_notification,
    request_notification_permission, url_handler,
};
mod window_state;
pub use window_state::screen_visible_frames;
mod services;
pub use services::NativeServices;
mod accessibility;
mod accessibility_bridge;
use accessibility_bridge::{AccessibilityActionQueue, NativeAccessibilityState};
mod clipboard;
pub use clipboard::Clipboard;
mod runloop;
pub use runloop::{PostError, Poster, Timer};
use runloop::{PostInner, PostState, TimerAction, TimerInner};
mod imaging;
pub use imaging::{decode_image, encode_png};
mod announce;
mod feedback;
pub use feedback::SystemFeedback;
// All `unsafe` of the crate is in `ffi`: each block is one raw FFI
// operation with its `SAFETY` comment, and the checks in `ffi::checks`
// compare every binding with the runtime and the SDK headers.
#[allow(unsafe_code)]
mod ffi;
use ffi::Obj;
use ffi::Pool;
use ffi::appkit::{
    ACTIVATE, ACTIVATION_POLICY, ACTIVATION_POLICY_REGULAR, APPLICATION_WINDOWS,
    BACKING_SCALE_FACTOR, BACKING_STORE_BUFFERED, BUTTON_NUMBER, CHARACTERS,
    CHARACTERS_IGNORING_MODIFIERS, CLOSE, CONTENT_VIEW, CONVERT_POINT_FROM_VIEW,
    CURRENT_GRAPHICS_CONTEXT, ChoscoruiV2ContentView, ChoscoruiV2WindowDelegate, EVENT_KEY_DOWN,
    EVENT_KEY_UP, EVENT_LEFT_MOUSE_DOWN, EVENT_LEFT_MOUSE_DRAGGED, EVENT_LEFT_MOUSE_UP,
    EVENT_MODIFIER_FLAGS, EVENT_MOUSE_MOVED, EVENT_OTHER_MOUSE_DOWN, EVENT_OTHER_MOUSE_DRAGGED,
    EVENT_OTHER_MOUSE_UP, EVENT_RIGHT_MOUSE_DOWN, EVENT_RIGHT_MOUSE_DRAGGED, EVENT_RIGHT_MOUSE_UP,
    EVENT_SCROLL_WHEEL, EVENT_TYPE, FINISH_LAUNCHING, GRAPHICS_CG_CONTEXT,
    HAS_PRECISE_SCROLLING_DELTAS, INIT_WITH_CONTENT_RECT, INIT_WITH_FRAME, INTERPRET_KEY_EVENTS,
    IS_A_REPEAT, IS_WINDOW_VISIBLE, KEY_CODE, LOCATION_IN_WINDOW, MAKE_KEY_AND_ORDER_FRONT,
    MODIFIER_CAPS_LOCK, MODIFIER_COMMAND, MODIFIER_CONTROL, MODIFIER_OPTION, MODIFIER_SHIFT,
    NEXT_EVENT, NSApplication, NSEvent, NSWindow, RUN, SCROLLING_DELTA_X, SCROLLING_DELTA_Y,
    SEND_EVENT, SET_ACCEPTS_MOUSE_MOVED_EVENTS, SET_ACTIVATION_POLICY, SET_CONTENT_SIZE,
    SET_CONTENT_VIEW, SET_RELEASED_WHEN_CLOSED, SET_TITLE, SET_WINDOW_DELEGATE, SHARED_APPLICATION,
    VIEW_FRAME, VIEW_WINDOW, WINDOW_STYLE_CLOSABLE, WINDOW_STYLE_MINIATURIZABLE,
    WINDOW_STYLE_RESIZABLE, WINDOW_STYLE_TITLED, WINDOW_TITLE,
};
use ffi::foundation::{
    COUNT, DATE_WITH_TIME_INTERVAL_SINCE_NOW, NOTIFICATION_OBJECT, NSDefaultRunLoopMode,
    NSNotification, ns_array, ns_string, string,
};
use ffi::retained::{Retained, alloc, alloc_init};
use ffi::runloop::{MainRunLoop, RunLoopSource, RunLoopTimer};
mod dnd;
mod pasteboard;
mod preferences;
pub use preferences::{system_accessibility_preferences, system_prefers_dark};
mod fonts;
mod shaping;
mod surface;
pub use dnd::SimulatedDragPhase;
mod text_layout;
pub use accessibility::{
    AccessibilityActionError, AccessibilityActionRequest, AccessibilityElement,
    AccessibilityIdentity, AccessibilityNotification, AccessibilityProjection, AccessibilityUpdate,
};
use shaping::draw_text_run;
pub use shaping::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::marker::PhantomData;
use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex, Once, OnceLock};
use std::time::Duration;
pub use text_layout::{CoreTextShaper, available_font_families};

use ffi::geometry::{CGAffineTransform, NSPoint, NSRange, NSRect, NSSize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    WrongThread,
    InvalidSize,
    InvalidTitle,
    InvalidClipboardText,
    ClipboardWriteFailed,
    AppKitUnavailable,
    InvalidInterval,
    /// A custom cursor image was empty, larger than 256 points, or its
    /// hotspot lay outside the image.
    InvalidCursor,
    /// A payload used an unsupported or malformed format.
    UnsupportedFormat,
    /// A drag can only start while handling a mouse-down or mouse-drag.
    NoDragEvent,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongThread => f.write_str("AppKit must be used from the main thread"),
            Self::InvalidSize => f.write_str("the size must be positive and finite"),
            Self::InvalidTitle => f.write_str("the title contains an interior NUL character"),
            Self::InvalidClipboardText => {
                f.write_str("clipboard text contains an interior NUL character")
            }
            Self::ClipboardWriteFailed => f.write_str("the pasteboard rejected the write"),
            Self::AppKitUnavailable => f.write_str("AppKit is unavailable"),
            Self::InvalidInterval => f.write_str("the interval must be positive and finite"),
            Self::InvalidCursor => f.write_str("the custom cursor image or hotspot is invalid"),
            Self::UnsupportedFormat => f.write_str("the drag or clipboard format is not supported"),
            Self::NoDragEvent => {
                f.write_str("a drag can only start while handling a pointer event")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Owned native window notifications, delivered in AppKit callback order.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    Resized {
        size: Size,
        scale: f64,
    },
    ScaleChanged {
        scale: f64,
    },
    Focused(bool),
    Closing,
    Pointer {
        event: PointerEvent,
        modifiers: Modifiers,
    },
    Key {
        event: KeyEvent,
        modifiers: Modifiers,
    },
    /// Owned committed text from an ordinary key event. IME marked text uses
    /// a separate text-input-client path and is not represented here.
    Text {
        text: String,
    },
    Ime(ImeEvent),
    /// The window became fully hidden (`true`) or partly visible again.
    Occluded(bool),
    /// A drag over the content view, after format and operation filtering
    /// by [`Window::accept_drops`].
    Drag(choscorui_core::DragEvent),
    /// A drag started by [`Window::begin_drag`] finished.
    DragEnded(choscorui_core::DragOperation),
}

/// Timing of one completed `drawRect:` replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaintTiming {
    pub finished: std::time::Instant,
    pub duration: std::time::Duration,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub option: bool,
    pub command: bool,
    pub caps_lock: bool,
}

impl Modifiers {
    fn from_native(flags: usize) -> Self {
        Self {
            caps_lock: flags & MODIFIER_CAPS_LOCK != 0,
            shift: flags & MODIFIER_SHIFT != 0,
            control: flags & MODIFIER_CONTROL != 0,
            option: flags & MODIFIER_OPTION != 0,
            command: flags & MODIFIER_COMMAND != 0,
        }
    }
}

type WindowQueue = Arc<Mutex<VecDeque<WindowEvent>>>;
static WINDOW_QUEUES: OnceLock<Mutex<HashMap<usize, WindowQueue>>> = OnceLock::new();
struct PaintState {
    list: Arc<DisplayList>,
    count: usize,
    last_paint: Option<PaintTiming>,
    last_damage: Option<choscorui_core::Rect>,
    cursor: choscorui_core::CursorShape,
    drop_formats: Vec<choscorui_core::DragFormat>,
    drop_operations: choscorui_core::DragOperations,
    drag_inside: bool,
    source_operations: choscorui_core::DragOperations,
}
type PaintHandle = Arc<Mutex<PaintState>>;
static PAINT_STATES: OnceLock<Mutex<HashMap<usize, PaintHandle>>> = OnceLock::new();
fn window_queues() -> &'static Mutex<HashMap<usize, WindowQueue>> {
    WINDOW_QUEUES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn paint_states() -> &'static Mutex<HashMap<usize, PaintHandle>> {
    PAINT_STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn view_draw_rect(view: &Obj<ChoscoruiV2ContentView>, dirty: NSRect) {
    let Some(window) = VIEW_WINDOW.send(view, ()) else {
        return;
    };
    let paint = paint_states().lock().unwrap().get(&window.addr()).cloned();
    let Some(paint) = paint else {
        return;
    };
    let started = std::time::Instant::now();
    let list = Arc::clone(&paint.lock().unwrap().list);
    // drawRect: runs with a current graphics context. Its CGContext is
    // borrowed only during this callback.
    let Some(graphics) = CURRENT_GRAPHICS_CONTEXT.class_send(()).ok().flatten() else {
        return;
    };
    let Some(context) = GRAPHICS_CG_CONTEXT.send(&graphics, ()) else {
        return;
    };
    if paint::paint_commands(context, dirty, &list) {
        let finished = std::time::Instant::now();
        let mut paint = paint.lock().unwrap();
        paint.count += 1;
        paint.last_paint = Some(PaintTiming {
            finished,
            duration: finished - started,
        });
    }
}

fn native_rect(rect: choscorui_core::Rect) -> NSRect {
    NSRect {
        origin: NSPoint {
            x: rect.origin.x,
            y: rect.origin.y,
        },
        size: NSSize {
            width: rect.size.width,
            height: rect.size.height,
        },
    }
}

/// Queues the event of a window delegate notification.
fn record_window_event(
    notification: Option<&Obj<NSNotification>>,
    make_event: fn(&Obj<NSWindow>) -> WindowEvent,
) {
    let Some(window) = notification
        .and_then(|notification| NOTIFICATION_OBJECT.send(notification, ()))
        .and_then(|object| object.downcast::<NSWindow>())
    else {
        return;
    };
    let queue = window_queues().lock().unwrap().get(&window.addr()).cloned();
    if let Some(queue) = queue {
        let event = make_event(window);
        queue.lock().unwrap().push_back(event);
    }
}

fn window_did_resize(
    _: &Obj<ChoscoruiV2WindowDelegate>,
    notification: Option<&Obj<NSNotification>>,
) {
    record_window_event(notification, |window| WindowEvent::Resized {
        size: read_content_size(window),
        scale: BACKING_SCALE_FACTOR.send(window, ()),
    });
}
fn window_did_change_scale(
    _: &Obj<ChoscoruiV2WindowDelegate>,
    notification: Option<&Obj<NSNotification>>,
) {
    record_window_event(notification, |window| WindowEvent::ScaleChanged {
        scale: BACKING_SCALE_FACTOR.send(window, ()),
    });
}
fn window_did_focus(
    _: &Obj<ChoscoruiV2WindowDelegate>,
    notification: Option<&Obj<NSNotification>>,
) {
    record_window_event(notification, |_| WindowEvent::Focused(true));
}
fn window_did_blur(_: &Obj<ChoscoruiV2WindowDelegate>, notification: Option<&Obj<NSNotification>>) {
    record_window_event(notification, |_| WindowEvent::Focused(false));
}
fn window_will_close(
    _: &Obj<ChoscoruiV2WindowDelegate>,
    notification: Option<&Obj<NSNotification>>,
) {
    record_window_event(notification, |_| WindowEvent::Closing);
}

fn view_is_flipped(_: &Obj<ChoscoruiV2ContentView>) -> bool {
    true
}
fn view_accepts_first_responder(_: &Obj<ChoscoruiV2ContentView>) -> bool {
    true
}
fn view_accepts_first_mouse(_: &Obj<ChoscoruiV2ContentView>, _: Option<&Obj<NSEvent>>) -> bool {
    true
}

/// The event of an event callback and the window of the view.
fn event_arguments<'a>(
    view: &'a Obj<ChoscoruiV2ContentView>,
    event: Option<&'a Obj<NSEvent>>,
) -> Option<(&'a Obj<NSWindow>, &'a Obj<NSEvent>)> {
    Some((VIEW_WINDOW.send(view, ())?, event?))
}

fn view_pointer_event(view: &Obj<ChoscoruiV2ContentView>, native_event: Option<&Obj<NSEvent>>) {
    let Some((window, native_event)) = event_arguments(view, native_event) else {
        return;
    };
    // `convertPoint:fromView:nil` maps the window point into the
    // flipped view's logical coordinates.
    let location = LOCATION_IN_WINDOW.send(native_event, ());
    let point = CONVERT_POINT_FROM_VIEW.send(view, (location, None));
    let position = Point::new(point.x, point.y);
    let native_type = EVENT_TYPE.send(native_event, ());
    let event = match native_type {
        EVENT_LEFT_MOUSE_DOWN => PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        },
        EVENT_LEFT_MOUSE_UP => PointerEvent::Up {
            position,
            button: PointerButton::Primary,
        },
        EVENT_RIGHT_MOUSE_DOWN => PointerEvent::Down {
            position,
            button: PointerButton::Secondary,
        },
        EVENT_RIGHT_MOUSE_UP => PointerEvent::Up {
            position,
            button: PointerButton::Secondary,
        },
        EVENT_MOUSE_MOVED
        | EVENT_LEFT_MOUSE_DRAGGED
        | EVENT_RIGHT_MOUSE_DRAGGED
        | EVENT_OTHER_MOUSE_DRAGGED => PointerEvent::Move { position },
        EVENT_SCROLL_WHEEL => {
            let unit = if HAS_PRECISE_SCROLLING_DELTAS.send(native_event, ()) {
                WheelUnit::Points
            } else {
                WheelUnit::Lines
            };
            PointerEvent::Wheel {
                position,
                delta: Point::new(
                    -SCROLLING_DELTA_X.send(native_event, ()),
                    -SCROLLING_DELTA_Y.send(native_event, ()),
                ),
                unit,
            }
        }
        EVENT_OTHER_MOUSE_DOWN | EVENT_OTHER_MOUSE_UP => {
            let number = BUTTON_NUMBER.send(native_event, ()) as usize;
            let button = if number == 2 {
                PointerButton::Middle
            } else {
                PointerButton::Other(number)
            };
            if native_type == 25 {
                PointerEvent::Down { position, button }
            } else {
                PointerEvent::Up { position, button }
            }
        }
        _ => return,
    };
    let modifiers = Modifiers::from_native(EVENT_MODIFIER_FLAGS.send(native_event, ()));
    let queue = window_queues().lock().unwrap().get(&window.addr()).cloned();
    if let Some(queue) = queue {
        queue
            .lock()
            .unwrap()
            .push_back(WindowEvent::Pointer { event, modifiers });
    }
}

/// Maps a macOS virtual keycode for a named key to its [`Key`]. A keycode
/// that is not here becomes a character or [`Key::Other`] from the event text.
fn key_from_code(code: u16) -> Option<Key> {
    use ffi::keys::*;
    Some(match code {
        KEY_TAB => Key::Tab,
        KEY_RETURN => Key::Enter,
        KEY_SPACE => Key::Space,
        KEY_ESCAPE => Key::Escape,
        KEY_UP_ARROW => Key::ArrowUp,
        KEY_DOWN_ARROW => Key::ArrowDown,
        KEY_LEFT_ARROW => Key::ArrowLeft,
        KEY_RIGHT_ARROW => Key::ArrowRight,
        KEY_HOME => Key::Home,
        KEY_END => Key::End,
        KEY_DELETE => Key::Backspace,
        KEY_FORWARD_DELETE => Key::Delete,
        KEY_PAGE_UP => Key::PageUp,
        KEY_PAGE_DOWN => Key::PageDown,
        KEY_F1 => Key::F(1),
        KEY_F2 => Key::F(2),
        KEY_F3 => Key::F(3),
        KEY_F4 => Key::F(4),
        KEY_F5 => Key::F(5),
        KEY_F6 => Key::F(6),
        KEY_F7 => Key::F(7),
        KEY_F8 => Key::F(8),
        KEY_F9 => Key::F(9),
        KEY_F10 => Key::F(10),
        KEY_F11 => Key::F(11),
        KEY_F12 => Key::F(12),
        KEY_F13 => Key::F(13),
        KEY_F14 => Key::F(14),
        KEY_F15 => Key::F(15),
        KEY_F16 => Key::F(16),
        KEY_F17 => Key::F(17),
        KEY_F18 => Key::F(18),
        KEY_F19 => Key::F(19),
        KEY_F20 => Key::F(20),
        KEY_CONTEXTUAL_MENU => Key::ContextMenu,
        _ => return None,
    })
}

/// True for a character that a key event inserts as text.
fn is_text_character(character: &char) -> bool {
    !character.is_control() && !('\u{f700}'..='\u{f8ff}').contains(character)
}

fn view_key_event(view: &Obj<ChoscoruiV2ContentView>, native_event: Option<&Obj<NSEvent>>) {
    let Some((window, native_event)) = event_arguments(view, native_event) else {
        return;
    };
    // Keycodes are macOS virtual keycodes; text insertion remains a
    // separate path.
    let code = KEY_CODE.send(native_event, ());
    let key = match key_from_code(code) {
        Some(key) => key,
        None => {
            let printable = CHARACTERS_IGNORING_MODIFIERS
                .send(native_event, ())
                .and_then(string)
                .and_then(|text| text.chars().next())
                .filter(is_text_character);
            match printable {
                Some(ch) => Key::Character(ch.to_lowercase().next().unwrap_or(ch)),
                None => Key::Other(u32::from(code)),
            }
        }
    };
    let event = match EVENT_TYPE.send(native_event, ()) {
        EVENT_KEY_DOWN => KeyEvent::Down {
            key,
            repeat: IS_A_REPEAT.send(native_event, ()),
        },
        EVENT_KEY_UP => KeyEvent::Up { key },
        _ => return,
    };
    let modifiers = Modifiers::from_native(EVENT_MODIFIER_FLAGS.send(native_event, ()));
    let queue = window_queues().lock().unwrap().get(&window.addr()).cloned();
    if let Some(queue) = queue {
        let event_count = {
            let mut queue = queue.lock().unwrap();
            queue.push_back(WindowEvent::Key { event, modifiers });
            queue.len()
        };
        if matches!(event, KeyEvent::Down { .. }) && !modifiers.control && !modifiers.command {
            if let Ok(events) = ns_array(&[native_event.upcast()]) {
                INTERPRET_KEY_EVENTS.send(view, (&events,));
            }
            let mut queue = queue.lock().unwrap();
            if queue.len() == event_count {
                let text: String = CHARACTERS
                    .send(native_event, ())
                    .and_then(string)
                    .unwrap_or_default()
                    .chars()
                    .filter(is_text_character)
                    .collect();
                if !text.is_empty() {
                    queue.push_back(WindowEvent::Ime(ImeEvent::CommitText {
                        text,
                        replacement_utf16: None,
                    }));
                }
            }
        }
    }
}

/// Main-thread AppKit process handle. The marker prevents worker transfer.
pub struct Application {
    application: &'static Obj<NSApplication>,
    runloop: MainRunLoop,
    posted: Arc<PostInner>,
    /// The source of posted work, held for its `Drop`, which runs after
    /// `Application::drop` clears the signal.
    _source: RunLoopSource<PostInner>,
    timers: RefCell<Vec<Weak<TimerInner>>>,
    timer_panics: Rc<Cell<usize>>,
    _main_thread: PhantomData<Rc<()>>,
}
impl Application {
    pub fn new() -> Result<Self, Error> {
        if !ffi::is_main_thread() {
            return Err(Error::WrongThread);
        }
        let _pool = Pool::new()?;
        let application = SHARED_APPLICATION
            .class_send(())?
            .ok_or(Error::AppKitUnavailable)?;
        // Launched from an app bundle, AppKit may already be in the Regular
        // policy and refuse the redundant switch; only a policy that stays
        // non-regular is a failure.
        if !SET_ACTIVATION_POLICY.send(application, (ACTIVATION_POLICY_REGULAR,))
            && ACTIVATION_POLICY.send(application, ()) != ACTIVATION_POLICY_REGULAR
        {
            return Err(Error::AppKitUnavailable);
        }
        desktop::install_app_delegate(application)?;
        static FINISH_LAUNCHING_ONCE: Once = Once::new();
        FINISH_LAUNCHING_ONCE.call_once(|| FINISH_LAUNCHING.send(application, ()));
        let runloop = MainRunLoop::get().ok_or(Error::AppKitUnavailable)?;
        let posted = Arc::new(PostInner {
            runloop,
            state: Mutex::new(PostState {
                source: None,
                tasks: VecDeque::new(),
                panics: 0,
            }),
        });
        let source =
            RunLoopSource::new(runloop, Arc::clone(&posted)).ok_or(Error::AppKitUnavailable)?;
        posted.state.lock().unwrap().source = Some(source.signal());
        Ok(Self {
            application,
            runloop,
            posted,
            _source: source,
            timers: RefCell::new(Vec::new()),
            timer_panics: Rc::new(Cell::new(0)),
            _main_thread: PhantomData,
        })
    }

    pub fn install_main_menu(&self, model: MenuBarModel) -> Result<MainMenu, MenuError> {
        MainMenu::install(model)
    }

    pub fn create_window(&self, options: WindowOptions) -> Result<Window, Error> {
        let (width, height) = (options.width(), options.height());
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(Error::InvalidSize);
        }
        let _pool = Pool::new()?;
        let title = ns_string(options.title()).map_err(|_| Error::InvalidTitle)?;
        let frame = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize { width, height },
        };
        // Titled, closable, miniaturizable, and resizable, with a buffered
        // backing store and no deferred creation.
        let style = WINDOW_STYLE_TITLED
            | WINDOW_STYLE_CLOSABLE
            | WINDOW_STYLE_MINIATURIZABLE
            | WINDOW_STYLE_RESIZABLE;
        let window = INIT_WITH_CONTENT_RECT
            .init(
                alloc::<NSWindow>()?,
                (frame, style, BACKING_STORE_BUFFERED, false),
            )
            .ok_or(Error::AppKitUnavailable)?;
        // The handle owns the window, so closing must not release it.
        SET_RELEASED_WHEN_CLOSED.send(&window, (false,));
        SET_TITLE.send(&window, (&title,));
        ChoscoruiV2ContentView::register()?;
        let content = INIT_WITH_FRAME
            .init(
                alloc::<ChoscoruiV2ContentView>()?,
                (NSRect {
                    origin: NSPoint { x: 0.0, y: 0.0 },
                    size: frame.size,
                },),
            )
            .ok_or(Error::AppKitUnavailable)?;
        SET_CONTENT_VIEW.send(&window, (Some(content.upcast()),));
        drop(content);
        SET_ACCEPTS_MOUSE_MOVED_EVENTS.send(&window, (true,));
        ChoscoruiV2WindowDelegate::register()?;
        let delegate = alloc_init::<ChoscoruiV2WindowDelegate>()?;
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let paint = Arc::new(Mutex::new(PaintState {
            list: Arc::new(DisplayList::new()),
            count: 0,
            last_paint: None,
            last_damage: None,
            cursor: Default::default(),
            drop_formats: Vec::new(),
            drop_operations: choscorui_core::DragOperations::NONE,
            drag_inside: false,
            source_operations: choscorui_core::DragOperations::NONE,
        }));
        let ime = Arc::new(Mutex::new(TextInputState {
            selected_utf16: 0..0,
            marked_utf16: None,
            caret: Rect::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0)),
        }));
        window_queues()
            .lock()
            .unwrap()
            .insert(window.addr(), Arc::clone(&events));
        paint_states()
            .lock()
            .unwrap()
            .insert(window.addr(), Arc::clone(&paint));
        ime_states()
            .lock()
            .unwrap()
            .insert(window.addr(), Arc::clone(&ime));
        SET_WINDOW_DELEGATE.send(&window, (Some(delegate.upcast()),));
        Ok(Window {
            _delegate: delegate,
            window,
            events,
            paint,
            ime,
            accessibility: RefCell::new(NativeAccessibilityState::new()),
            accessibility_actions: Arc::new(Mutex::new(VecDeque::new())),
            closed: false,
            drop_pool: None,
            _main_thread: PhantomData,
        })
    }

    /// Enters AppKit's blocking event loop; `stop` exits it on the UI thread.
    pub fn run(&self) {
        RUN.send(self.application, ());
    }

    pub fn stop(&self) {
        runloop::stop_application();
    }

    /// Number of native windows AppKit still tracks (open or not yet
    /// released). Leak checks compare it before and after window churn.
    pub fn window_count(&self) -> usize {
        let _pool = Pool::new().expect("AppKit autorelease pool unavailable");
        APPLICATION_WINDOWS
            .send(self.application, ())
            .map_or(0, |windows| COUNT.send(windows, ()))
    }

    /// Requests foreground activation. AppKit may complete it asynchronously.
    pub fn activate(&self) {
        let _pool = Pool::new();
        ACTIVATE.send(self.application, ());
    }

    pub fn poster(&self) -> Poster {
        Poster {
            inner: Arc::clone(&self.posted),
        }
    }

    /// Processes at most one run-loop source or timer, blocking up to `timeout`.
    /// This supports native integration tests and future embedded loops.
    pub fn pump_once(&self, timeout: Duration) {
        let _pool = Pool::new().expect("AppKit autorelease pool unavailable");
        MainRunLoop::run_once(timeout.as_secs_f64());
    }

    /// Waits for and dispatches one native AppKit event, returning whether one
    /// arrived. The OS run loop also services timers and posted work while
    /// waiting. A caller can drain each window's owned events afterward.
    pub fn dispatch_next_event(&self, timeout: Duration) -> bool {
        let _pool = Pool::new().expect("AppKit autorelease pool unavailable");
        let date = DATE_WITH_TIME_INTERVAL_SINCE_NOW
            .class_send((timeout.as_secs_f64(),))
            .expect("NSDate unavailable");
        let event = NEXT_EVENT.send(
            self.application,
            (usize::MAX, date.as_deref(), NSDefaultRunLoopMode, true),
        );
        match event {
            Some(event) => {
                SEND_EVENT.send(self.application, (event,));
                true
            }
            None => false,
        }
    }
    pub fn posted_panic_count(&self) -> usize {
        self.posted.state.lock().unwrap().panics
    }

    /// Schedules a one-shot callback on the main run loop. Posted work is FIFO;
    /// timers run by deadline, with no ordering promise at equal deadlines or
    /// relative to posted work that becomes ready in the same run-loop turn.
    /// Runs `action` once on the UI thread after `delay`. Timers fire in
    /// all common run-loop modes, including live resize and menu tracking.
    pub fn schedule_once(
        &self,
        delay: Duration,
        action: impl FnOnce() + 'static,
    ) -> Result<Timer, Error> {
        self.schedule_timer(delay, None, TimerAction::Once(Box::new(action)))
    }

    /// Runs `action` every `interval` until the timer is canceled or
    /// dropped. Core Foundation skips missed ticks instead of bursting.
    pub fn schedule_repeating(
        &self,
        interval: Duration,
        action: impl FnMut() + 'static,
    ) -> Result<Timer, Error> {
        if interval.is_zero() {
            return Err(Error::InvalidInterval);
        }
        self.schedule_timer(
            interval,
            Some(interval),
            TimerAction::Repeat(Box::new(action)),
        )
    }

    fn schedule_timer(
        &self,
        delay: Duration,
        interval: Option<Duration>,
        action: TimerAction,
    ) -> Result<Timer, Error> {
        let inner = Rc::new_cyclic(|owner| TimerInner {
            timer: RunLoopTimer::new(
                self.runloop,
                delay.as_secs_f64(),
                interval.map_or(0.0, |interval| interval.as_secs_f64()),
                owner.clone(),
            ),
            action: RefCell::new(Some(action)),
            panics: Rc::clone(&self.timer_panics),
            active: Cell::new(true),
        });
        if inner.timer.is_none() {
            return Err(Error::AppKitUnavailable);
        }
        let mut timers = self.timers.borrow_mut();
        timers.retain(|timer| timer.strong_count() > 0);
        timers.push(Rc::downgrade(&inner));
        Ok(Timer { inner })
    }

    pub fn timer_panic_count(&self) -> usize {
        self.timer_panics.get()
    }
}

impl Drop for Application {
    fn drop(&mut self) {
        for timer in self.timers.get_mut().drain(..) {
            if let Some(timer) = timer.upgrade() {
                timer.cancel();
            }
        }
        // The mutex excludes worker signals; the source field then removes
        // and invalidates the source on this thread.
        let mut state = self.posted.state.lock().unwrap();
        state.source = None;
        state.tasks.clear();
    }
}

/// Retained NSWindow. Closing does not release it; Drop releases it once.
pub struct Window {
    /// The window's delegate, held for its `Drop`. It releases before the
    /// window, as fields drop in order.
    _delegate: Retained<ChoscoruiV2WindowDelegate>,
    window: Retained<NSWindow>,
    events: WindowQueue,
    paint: PaintHandle,
    ime: ImeHandle,
    accessibility: RefCell<NativeAccessibilityState>,
    accessibility_actions: AccessibilityActionQueue,
    closed: bool,
    /// The pool that `Drop` creates. It drops after the other fields, so it
    /// drains what releasing the window and the delegate autoreleases.
    drop_pool: Option<Pool>,
    _main_thread: PhantomData<Rc<()>>,
}
impl Window {
    /// The native window, borrowed for as long as this handle lives.
    fn ns_window(&self) -> &Obj<NSWindow> {
        &self.window
    }

    pub fn set_text_input_state(&self, state: TextInputState) {
        *self.ime.lock().unwrap() = state;
    }

    pub fn paint_count(&self) -> usize {
        self.paint.lock().unwrap().count
    }

    /// When the most recent completed paint finished and how long the
    /// display-list replay took. Benchmarks use this as the present point.
    pub fn last_paint(&self) -> Option<PaintTiming> {
        self.paint.lock().unwrap().last_paint
    }

    pub fn take_events(&mut self) -> Vec<WindowEvent> {
        self.events.lock().unwrap().drain(..).collect()
    }
    pub fn show(&mut self) {
        let _pool = Pool::new().expect("AppKit autorelease pool unavailable");
        MAKE_KEY_AND_ORDER_FRONT.send(&self.window, (None,));
        self.closed = false;
    }

    pub fn set_title(&self, title: &str) -> Result<(), Error> {
        let _pool = Pool::new()?;
        let title = ns_string(title).map_err(|_| Error::InvalidTitle)?;
        SET_TITLE.send(&self.window, (&title,));
        Ok(())
    }

    pub fn title(&self) -> String {
        let _pool = Pool::new().expect("AppKit autorelease pool unavailable");
        WINDOW_TITLE
            .send(&self.window, ())
            .and_then(string)
            .unwrap_or_default()
    }

    pub fn is_visible(&self) -> bool {
        let _pool = Pool::new();
        IS_WINDOW_VISIBLE.send(&self.window, ())
    }

    /// The content-view extent in logical points.
    pub fn content_size(&self) -> Size {
        read_content_size(&self.window)
    }

    pub fn set_content_size(&mut self, size: Size) -> Result<(), Error> {
        let _pool = Pool::new();
        if !size.width.is_finite()
            || !size.height.is_finite()
            || size.width <= 0.0
            || size.height <= 0.0
        {
            return Err(Error::InvalidSize);
        }
        SET_CONTENT_SIZE.send(
            &self.window,
            (NSSize {
                width: size.width,
                height: size.height,
            },),
        );
        Ok(())
    }

    pub fn backing_scale_factor(&self) -> f64 {
        let _pool = Pool::new();
        BACKING_SCALE_FACTOR.send(&self.window, ())
    }

    pub fn close(&mut self) {
        if !self.closed {
            let _pool = Pool::new();
            // setReleasedWhenClosed:NO keeps ownership after close.
            CLOSE.send(&self.window, ());
            self.closed = true;
        }
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        // `close` and `release` autorelease internally; this pool drains
        // them, so a caller without its own pool does not keep the window
        // alive. It drops after the window and delegate fields.
        self.drop_pool = Pool::new().ok();
        self.close();
        for (_, element) in self.accessibility.get_mut().elements.drain() {
            accessibility_bridge::unbind(&element);
        }
        // NSWindow.delegate is weak. Detach it before the delegate field
        // releases it, then unregister the window's state.
        SET_WINDOW_DELEGATE.send(&self.window, (None,));
        let window = self.window.addr();
        window_queues().lock().unwrap().remove(&window);
        paint_states().lock().unwrap().remove(&window);
        ime_states().lock().unwrap().remove(&window);
        if paint_states().lock().unwrap().is_empty() {
            paint::clear_image_cache();
        }
    }
}

/// The content-view extent of `window`, or empty without a content view.
fn read_content_size(window: &Obj<NSWindow>) -> Size {
    CONTENT_VIEW
        .send(window, ())
        .map_or(Size::new(0.0, 0.0), |view| {
            let frame = VIEW_FRAME.send(view, ());
            Size::new(frame.size.width, frame.size.height)
        })
}

#[cfg(test)]
mod key_code_tests {
    use super::*;

    #[test]
    fn function_and_menu_keycodes_map_to_named_keys() {
        assert_eq!(key_from_code(0x7a), Some(Key::F(1)));
        assert_eq!(key_from_code(0x6d), Some(Key::F(10)));
        assert_eq!(key_from_code(0x6f), Some(Key::F(12)));
        assert_eq!(key_from_code(0x5a), Some(Key::F(20)));
        assert_eq!(key_from_code(0x6e), Some(Key::ContextMenu));
        assert_eq!(key_from_code(0x30), Some(Key::Tab));
        // The keycode of the A key is a character, read from the event text.
        assert_eq!(key_from_code(0x00), None);
    }
}
