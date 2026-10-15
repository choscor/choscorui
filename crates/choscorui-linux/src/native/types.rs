//! The public event, state, and error types of the native backend. They
//! follow the types of `choscorui-macos`, so the facade maps both backends
//! the same way.

use choscorui_core::{GestureEvent, KeyEvent, PointerDetails, PointerEvent, Rect, Size};
use std::fmt;
use std::ops::Range;

/// Why a native operation failed.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// The backend must be used from the thread that created the
    /// application.
    WrongThread,
    /// The size must be positive and finite.
    InvalidSize,
    /// The title contains an interior NUL character.
    InvalidTitle,
    /// The interval must be positive and finite.
    InvalidInterval,
    /// The custom cursor image or its hotspot is invalid.
    InvalidCursor,
    /// No display server is reachable, or the requested one is not.
    NoDisplay(String),
    /// A required system library or symbol did not load.
    Library(crate::LoadError),
    /// The display server lacks a protocol that windows need.
    MissingProtocol(&'static str),
    /// The connection to the display server failed.
    Connection(String),
    /// The window is closed.
    Closed,
    /// A payload used an unsupported or malformed format.
    UnsupportedFormat,
    /// A drag can start only while a pointer event is handled.
    NoDragEvent,
    /// The clipboard text contains a NUL character, or a file path is
    /// not absolute.
    InvalidClipboardText,
    /// The display server has no clipboard, or it refused the write.
    ClipboardWriteFailed,
    /// The Cairo painter could not render a capture.
    Capture(crate::CairoError),
    /// The display server cannot show a popup next to this window.
    PopupUnavailable,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongThread => f.write_str("the backend must be used from its UI thread"),
            Self::InvalidSize => f.write_str("the size must be positive and finite"),
            Self::InvalidTitle => f.write_str("the title contains an interior NUL character"),
            Self::InvalidInterval => f.write_str("the interval must be positive and finite"),
            Self::InvalidCursor => f.write_str("the custom cursor image or hotspot is invalid"),
            Self::NoDisplay(reason) => write!(f, "no display server: {reason}"),
            Self::Library(error) => write!(f, "{error}"),
            Self::MissingProtocol(name) => write!(f, "the display server does not offer {name}"),
            Self::Connection(reason) => write!(f, "the display connection failed: {reason}"),
            Self::Closed => f.write_str("the window is closed"),
            Self::UnsupportedFormat => f.write_str("the drag or clipboard format is not supported"),
            Self::NoDragEvent => {
                f.write_str("a drag can only start while handling a pointer event")
            }
            Self::InvalidClipboardText => {
                f.write_str("the clipboard text or a file path of it is invalid")
            }
            Self::ClipboardWriteFailed => {
                f.write_str("the display server refused the clipboard write")
            }
            Self::Capture(error) => write!(f, "the capture failed: {error}"),
            Self::PopupUnavailable => {
                f.write_str("the display server cannot show a popup next to this window")
            }
        }
    }
}

impl std::error::Error for Error {}

/// The modifier keys of an event. `control` is Ctrl, `option` is Alt, and
/// `command` is the Super (logo) key.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub option: bool,
    pub command: bool,
    pub caps_lock: bool,
}

/// A change of the input method composition. Ranges count UTF-16 code
/// units, as on macOS.
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

/// The text input state of the focused control, which the input method
/// reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextInputState {
    /// A text control has the focus, so the input method is on.
    pub active: bool,
    /// The committed text of the focused control. The ranges count UTF-16
    /// code units of it.
    pub text: String,
    pub selected_utf16: Range<usize>,
    pub marked_utf16: Option<Range<usize>>,
    /// The caret rectangle in logical window coordinates.
    pub caret: Rect,
    /// The focused control takes every key, before the menu shortcuts.
    pub captures_keys: bool,
}

/// Owned window notifications, in the order that the display server sent
/// them.
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
    /// The user asked to close the window.
    Closing,
    Pointer {
        event: PointerEvent,
        modifiers: Modifiers,
        /// The click count of a button press or release. Other pointer
        /// events have the default details.
        details: PointerDetails,
    },
    Key {
        event: KeyEvent,
        modifiers: Modifiers,
    },
    /// A touchpad gesture over the content.
    Gesture {
        event: GestureEvent,
        modifiers: Modifiers,
    },
    /// Committed text from an ordinary key event.
    Text {
        text: String,
    },
    Ime(ImeEvent),
    /// The window became fully hidden (`true`) or visible again.
    Occluded(bool),
    /// A drag over the content, after format and operation filtering.
    Drag(choscorui_core::DragEvent),
    /// A drag that `Window::begin_drag` started finished.
    DragEnded(choscorui_core::DragOperation),
}

/// The painter of a window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Painter {
    /// OpenGL ES 3.0 through EGL.
    Gles,
    /// Cairo into shared-memory images.
    Cairo,
}

/// The timing of one paint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaintTiming {
    /// When the paint finished.
    pub finished: std::time::Instant,
    /// The time from the start of the paint to `finished`.
    pub duration: std::time::Duration,
}

/// An edge or corner for an interactive resize, as `xdg_toplevel`
/// numbers them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResizeEdge {
    Top = 1,
    Bottom = 2,
    Left = 4,
    TopLeft = 5,
    BottomLeft = 6,
    Right = 8,
    TopRight = 9,
    BottomRight = 10,
}
