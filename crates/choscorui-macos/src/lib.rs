//! AppKit window bootstrap. All public handles stay on the main thread.

#![cfg(target_os = "macos")]

use choscorui_core::{
    Color, DisplayCommand, DisplayList, Key, KeyEvent, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticChange, SemanticEntry, SemanticRelation, SemanticRole, SemanticText,
    Size, TextBuffer, WheelUnit,
};
mod ime;
pub use ime::{ImeEvent, TextInputState};
mod menu;
pub use menu::{
    MenuBarModel, MenuCommandId, MenuEntry, MenuError, MenuItemState, MenuShortcut,
    MenuShortcutModifiers, MenuSpec,
};
mod dialog;
mod menu_native;
mod paint;
pub use dialog::{
    AlertButton, AlertResponse, AlertSpec, AlertStyle, DialogError, FileDialogMode,
    FileDialogResponse, FileDialogSpec,
};
use ime::{
    ImeHandle, ime_states, view_attributed_substring, view_character_index_for_point,
    view_do_command, view_first_rect_for_range, view_has_marked_text, view_insert_text,
    view_marked_range, view_selected_range, view_set_marked_text, view_unmark_text,
    view_valid_marked_attributes,
};
pub use menu_native::MainMenu;
mod accessibility;
pub use accessibility::{
    AccessibilityActionError, AccessibilityActionRequest, AccessibilityElement,
    AccessibilityIdentity, AccessibilityNotification, AccessibilityProjection, AccessibilityUpdate,
};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::ffi::{CStr, CString, c_int, c_void};
use std::marker::PhantomData;
use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex, Once, OnceLock};
use std::time::Duration;

type Id = *mut c_void;
type Sel = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct NSPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct NSSize {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct NSRange {
    location: usize,
    length: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGAffineTransform {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSPasteboardTypeString: Id;
    static NSAccessibilityButtonRole: Id;
    static NSAccessibilityCheckBoxRole: Id;
    static NSAccessibilityGroupRole: Id;
    static NSAccessibilityListRole: Id;
    static NSAccessibilityIncrementorRole: Id;
    static NSAccessibilityProgressIndicatorRole: Id;
    static NSAccessibilityRadioButtonRole: Id;
    static NSAccessibilityScrollAreaRole: Id;
    static NSAccessibilitySliderRole: Id;
    static NSAccessibilityStaticTextRole: Id;
    static NSAccessibilityTextFieldRole: Id;
    static NSAccessibilitySecureTextFieldSubrole: Id;
    static NSAccessibilityWindowRole: Id;
    static NSAccessibilityLayoutChangedNotification: Id;
    static NSAccessibilityTitleChangedNotification: Id;
    static NSAccessibilityFocusedUIElementChangedNotification: Id;
    static NSAccessibilityValueChangedNotification: Id;
    fn NSAccessibilityPostNotification(element: Id, notification: Id);
}
#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {
    static NSDefaultRunLoopMode: Id;
}
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGContextSaveGState(context: Id);
    fn CGContextRestoreGState(context: Id);
    fn CGContextClipToRect(context: Id, rect: NSRect);
    fn CGContextSetRGBFillColor(context: Id, red: f64, green: f64, blue: f64, alpha: f64);
    fn CGContextSetRGBStrokeColor(context: Id, red: f64, green: f64, blue: f64, alpha: f64);
    fn CGContextSetAlpha(context: Id, alpha: f64);
    fn CGContextFillRect(context: Id, rect: NSRect);
    fn CGContextStrokeRectWithWidth(context: Id, rect: NSRect, width: f64);
    fn CGContextBeginPath(context: Id);
    fn CGContextMoveToPoint(context: Id, x: f64, y: f64);
    fn CGContextAddArcToPoint(context: Id, x1: f64, y1: f64, x2: f64, y2: f64, radius: f64);
    fn CGContextClosePath(context: Id);
    fn CGContextFillPath(context: Id);
    fn CGContextStrokePath(context: Id);
    fn CGContextSetLineWidth(context: Id, width: f64);
    fn CGContextTranslateCTM(context: Id, x: f64, y: f64);
    fn CGContextScaleCTM(context: Id, x: f64, y: f64);
    fn CGContextSetTextPosition(context: Id, x: f64, y: f64);
    fn CGContextSetTextMatrix(context: Id, matrix: CGAffineTransform);
    fn CGContextGetTextMatrix(context: Id) -> CGAffineTransform;
    fn CGColorSpaceCreateDeviceRGB() -> Id;
    fn CGBitmapContextCreate(
        data: Id,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        color_space: Id,
        bitmap_info: u32,
    ) -> Id;
    fn CGBitmapContextGetData(context: Id) -> Id;
    fn CGBitmapContextGetBytesPerRow(context: Id) -> usize;
}
#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const i8) -> Id;
    fn sel_registerName(name: *const i8) -> Sel;
    fn objc_msgSend();
    fn objc_allocateClassPair(superclass: Id, name: *const i8, extra_bytes: usize) -> Id;
    fn objc_disposeClassPair(class: Id);
    fn objc_registerClassPair(class: Id);
    fn class_addMethod(class: Id, name: Sel, imp: *const c_void, types: *const i8) -> i8;
    fn object_getClass(object: Id) -> Id;
    fn class_getSuperclass(class: Id) -> Id;
    fn objc_msgSendSuper();
}

#[repr(C)]
struct ObjcSuper {
    receiver: Id,
    superclass: Id,
}
unsafe extern "C" {
    fn pthread_main_np() -> c_int;
}

#[repr(C)]
struct CFRunLoopSourceContext {
    version: isize,
    info: *mut c_void,
    retain: Option<unsafe extern "C" fn(*const c_void) -> *const c_void>,
    release: Option<unsafe extern "C" fn(*const c_void)>,
    copy_description: Option<unsafe extern "C" fn(*const c_void) -> *const c_void>,
    equal: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> u8>,
    hash: Option<unsafe extern "C" fn(*const c_void) -> usize>,
    schedule: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_void)>,
    cancel: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_void)>,
    perform: Option<unsafe extern "C" fn(*mut c_void)>,
}

#[repr(C)]
struct CFRunLoopTimerContext {
    version: isize,
    info: *mut c_void,
    retain: Option<unsafe extern "C" fn(*const c_void) -> *const c_void>,
    release: Option<unsafe extern "C" fn(*const c_void)>,
    copy_description: Option<unsafe extern "C" fn(*const c_void) -> *const c_void>,
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopDefaultMode: *const c_void;
    static kCFBooleanTrue: *const c_void;
    fn CFStringCreateWithBytes(
        allocator: Id,
        bytes: *const u8,
        length: isize,
        encoding: u32,
        external: u8,
    ) -> Id;
    fn CFDictionaryCreate(
        allocator: Id,
        keys: *const Id,
        values: *const Id,
        count: isize,
        key_callbacks: Id,
        value_callbacks: Id,
    ) -> Id;
    fn CFAttributedStringCreate(allocator: Id, string: Id, attributes: Id) -> Id;
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFRunLoopSourceCreate(
        allocator: *const c_void,
        order: isize,
        context: *mut CFRunLoopSourceContext,
    ) -> *mut c_void;
    fn CFRunLoopAddSource(runloop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopRemoveSource(runloop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopSourceSignal(source: *mut c_void);
    fn CFRunLoopSourceInvalidate(source: *mut c_void);
    fn CFRunLoopAddTimer(runloop: *mut c_void, timer: *mut c_void, mode: *const c_void);
    fn CFRunLoopRemoveTimer(runloop: *mut c_void, timer: *mut c_void, mode: *const c_void);
    fn CFRunLoopTimerCreate(
        allocator: *const c_void,
        fire_date: f64,
        interval: f64,
        flags: usize,
        order: isize,
        callback: unsafe extern "C" fn(*mut c_void, *mut c_void),
        context: *mut CFRunLoopTimerContext,
    ) -> *mut c_void;
    fn CFRunLoopTimerInvalidate(timer: *mut c_void);
    fn CFRunLoopWakeUp(runloop: *mut c_void);
    fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source: u8) -> i32;
    fn CFRelease(object: *const c_void);
    fn CFAbsoluteTimeGetCurrent() -> f64;
}

#[link(name = "CoreText", kind = "framework")]
unsafe extern "C" {
    static kCTFontAttributeName: Id;
    static kCTForegroundColorFromContextAttributeName: Id;
    fn CTFontCreateWithName(name: Id, size: f64, matrix: Id) -> Id;
    fn CTLineCreateWithAttributedString(string: Id) -> Id;
    fn CTLineDraw(line: Id, context: Id);
    fn CTLineGetTypographicBounds(
        line: Id,
        ascent: *mut f64,
        descent: *mut f64,
        leading: *mut f64,
    ) -> f64;
    fn CTLineGetOffsetForStringIndex(line: Id, index: isize, secondary: *mut f64) -> f64;
    fn CTTypesetterCreateWithAttributedString(string: Id) -> Id;
    fn CTTypesetterSuggestLineBreak(typesetter: Id, start: isize, width: f64) -> isize;
}

struct CfOwned(NonNull<c_void>);
impl CfOwned {
    fn new(raw: Id) -> Option<Self> {
        NonNull::new(raw).map(Self)
    }
    fn as_ptr(&self) -> Id {
        self.0.as_ptr()
    }
}
impl Drop for CfOwned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.as_ptr()) }
    }
}

type PostedTask = Box<dyn FnOnce() + Send + 'static>;

struct PostState {
    source: usize,
    tasks: VecDeque<PostedTask>,
    panics: usize,
}

struct PostInner {
    application: usize,
    runloop: usize,
    state: Mutex<PostState>,
}

/// Worker-safe path to queue work on the AppKit main run loop.
#[derive(Clone)]
pub struct Poster {
    inner: Arc<PostInner>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostError {
    Stopped,
}

impl Poster {
    pub fn post(&self, task: impl FnOnce() + Send + 'static) -> Result<(), PostError> {
        let mut state = self.inner.state.lock().map_err(|_| PostError::Stopped)?;
        if state.source == 0 {
            return Err(PostError::Stopped);
        }
        state.tasks.push_back(Box::new(task));
        // SAFETY: The mutex excludes Application::drop, which removes and
        // releases the source. The run loop is process-owned and still live.
        unsafe {
            CFRunLoopSourceSignal(state.source as *mut c_void);
            CFRunLoopWakeUp(self.inner.runloop as *mut c_void);
        }
        Ok(())
    }

    /// Requests exit from `Application::run` through the main run loop.
    pub fn request_stop(&self) -> Result<(), PostError> {
        let application = self.inner.application;
        self.post(move || {
            // SAFETY: post executes only while Application is alive, on its
            // main run loop; `stop:` accepts a nullable sender.
            unsafe { send_void_id(application as Id, c"stop:", std::ptr::null_mut()) }
        })
    }
}

unsafe extern "C" fn perform_posted(info: *mut c_void) {
    // SAFETY: info owns an Arc strong count until the source is removed and
    // released on the main thread. This callback runs only on that thread.
    let inner = unsafe { &*(info as *const PostInner) };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        loop {
            let task = match inner.state.lock() {
                Ok(mut state) => state.tasks.pop_front(),
                Err(_) => return,
            };
            let Some(task) = task else {
                return;
            };
            if catch_unwind(AssertUnwindSafe(task)).is_err() {
                if let Ok(mut state) = inner.state.lock() {
                    state.panics += 1;
                }
            }
        }
    }));
}

struct TimerContext {
    owner: Weak<TimerInner>,
    action: RefCell<Option<Box<dyn FnOnce()>>>,
    panics: Rc<Cell<usize>>,
}

struct TimerInner {
    raw: Cell<usize>,
    info: Cell<usize>,
    runloop: usize,
    active: Cell<bool>,
}

impl TimerInner {
    fn cancel(&self) {
        if self.active.replace(false) {
            // SAFETY: TimerInner owns a live +1 CF timer on the main thread.
            unsafe { CFRunLoopTimerInvalidate(self.raw.get() as *mut c_void) }
        }
        if self.info.get() != 0 {
            // SAFETY: TimerInner owns this context until Drop; callback cannot
            // overlap cancellation because both run on the main thread.
            unsafe {
                (&*(self.info.get() as *const TimerContext))
                    .action
                    .borrow_mut()
                    .take();
            }
        }
    }
}

impl Drop for TimerInner {
    fn drop(&mut self) {
        if self.raw.get() == 0 {
            return;
        }
        self.cancel();
        // SAFETY: CF timer was created +1 and added to this run loop. Removal
        // and release happen after the last Rust handle and callback finish.
        unsafe {
            let raw = self.raw.get() as *mut c_void;
            CFRunLoopRemoveTimer(self.runloop as *mut c_void, raw, kCFRunLoopDefaultMode);
            CFRelease(raw);
            drop(Box::from_raw(self.info.get() as *mut TimerContext));
        }
    }
}

/// Main-thread one-shot timer. Dropping or canceling it prevents delivery.
pub struct Timer {
    inner: Rc<TimerInner>,
}

impl Timer {
    pub fn cancel(&self) {
        self.inner.cancel();
    }
    pub fn is_active(&self) -> bool {
        self.inner.active.get()
    }
}

unsafe extern "C" fn fire_timer(_: *mut c_void, info: *mut c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: TimerInner keeps this Box alive through the callback. The
        // upgraded Rc prevents a user action from freeing it during delivery.
        let context = unsafe { &*(info as *const TimerContext) };
        let Some(owner) = context.owner.upgrade() else {
            return;
        };
        owner.active.set(false);
        // SAFETY: One-shot timer is no longer needed after firing.
        unsafe { CFRunLoopTimerInvalidate(owner.raw.get() as *mut c_void) }
        let action = context.action.borrow_mut().take();
        if let Some(action) = action {
            if catch_unwind(AssertUnwindSafe(action)).is_err() {
                context.panics.set(context.panics.get() + 1);
            }
        }
    }));
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    WrongThread,
    InvalidSize,
    InvalidTitle,
    InvalidClipboardText,
    ClipboardWriteFailed,
    AppKitUnavailable,
}

/// Main-thread access to a native pasteboard. Temporary boards are released
/// globally on Drop; the general board is never released globally.
pub struct Clipboard {
    raw: NonNull<c_void>,
    temporary: bool,
    _main_thread: PhantomData<Rc<()>>,
}

impl Clipboard {
    pub fn general() -> Result<Self, Error> {
        Self::open(c"generalPasteboard", false)
    }

    /// Private named board for drag data or isolated tests.
    pub fn temporary() -> Result<Self, Error> {
        Self::open(c"pasteboardWithUniqueName", true)
    }

    fn open(selector_name: &CStr, temporary: bool) -> Result<Self, Error> {
        if unsafe { pthread_main_np() } == 0 {
            return Err(Error::WrongThread);
        }
        let _pool = Pool::new()?;
        // SAFETY: NSPasteboard class methods return an autoreleased object;
        // retain grants this Rust handle a +1 reference until Drop.
        unsafe {
            let cls = class(c"NSPasteboard")?;
            let raw = NonNull::new(send_id(cls, selector_name)).ok_or(Error::AppKitUnavailable)?;
            send_id(raw.as_ptr(), c"retain");
            Ok(Self {
                raw,
                temporary,
                _main_thread: PhantomData,
            })
        }
    }

    pub fn write_text(&self, text: &str) -> Result<(), Error> {
        let ctext = CString::new(text).map_err(|_| Error::InvalidClipboardText)?;
        let _pool = Pool::new()?;
        // SAFETY: NSString copies the validated UTF-8 bytes; AppKit owns the
        // pasteboard and type constant, and the local pool outlives the send.
        unsafe {
            let ns_text = send_id_cstr(
                class(c"NSString")?,
                c"stringWithUTF8String:",
                ctext.as_ptr(),
            );
            if ns_text.is_null() {
                return Err(Error::InvalidClipboardText);
            }
            send_usize(self.raw.as_ptr(), c"clearContents");
            if send_bool_id_id(
                self.raw.as_ptr(),
                c"setString:forType:",
                ns_text,
                NSPasteboardTypeString,
            ) == 0
            {
                return Err(Error::ClipboardWriteFailed);
            }
        }
        Ok(())
    }

    pub fn read_text(&self) -> Result<Option<String>, Error> {
        let _pool = Pool::new()?;
        // SAFETY: stringForType: returns an autoreleased NSString or nil. The
        // UTF8String bytes remain live until copied before the pool drains.
        unsafe {
            let ns_text = send_id_id(self.raw.as_ptr(), c"stringForType:", NSPasteboardTypeString);
            if ns_text.is_null() {
                return Ok(None);
            }
            let ptr = send_cstr(ns_text, c"UTF8String");
            if ptr.is_null() {
                return Err(Error::AppKitUnavailable);
            }
            let text = CStr::from_ptr(ptr)
                .to_str()
                .map_err(|_| Error::InvalidClipboardText)?;
            Ok(Some(text.to_owned()))
        }
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // SAFETY: Clipboard owns a retained AppKit object on the main thread.
        unsafe {
            if self.temporary {
                send_void(self.raw.as_ptr(), c"releaseGlobally");
            }
            send_void(self.raw.as_ptr(), c"release");
        }
    }
}

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
            caps_lock: flags & (1 << 16) != 0,
            shift: flags & (1 << 17) != 0,
            control: flags & (1 << 18) != 0,
            option: flags & (1 << 19) != 0,
            command: flags & (1 << 20) != 0,
        }
    }
}

type WindowQueue = Arc<Mutex<VecDeque<WindowEvent>>>;
static WINDOW_QUEUES: OnceLock<Mutex<HashMap<usize, WindowQueue>>> = OnceLock::new();
struct PaintState {
    list: DisplayList,
    count: usize,
}
struct NativeAccessibilityState {
    projection: AccessibilityProjection,
    elements: HashMap<AccessibilityIdentity, NonNull<c_void>>,
}

impl NativeAccessibilityState {
    fn new() -> Self {
        Self {
            projection: AccessibilityProjection::new(),
            elements: HashMap::new(),
        }
    }
}
type PaintHandle = Arc<Mutex<PaintState>>;
static PAINT_STATES: OnceLock<Mutex<HashMap<usize, PaintHandle>>> = OnceLock::new();
type AccessibilityActionQueue = Arc<Mutex<VecDeque<AccessibilityActionRequest>>>;
#[derive(Clone)]
struct NativeAccessibilityBinding {
    node: choscorui_core::NodeId,
    actions: Vec<SemanticAction>,
    queue: AccessibilityActionQueue,
    updating: bool,
    text: Option<SemanticText>,
}
static ACCESSIBILITY_BINDINGS: OnceLock<Mutex<HashMap<usize, NativeAccessibilityBinding>>> =
    OnceLock::new();

fn accessibility_bindings() -> &'static Mutex<HashMap<usize, NativeAccessibilityBinding>> {
    ACCESSIBILITY_BINDINGS.get_or_init(|| Mutex::new(HashMap::new()))
}

unsafe extern "C" fn accessibility_perform_press(element: Id, _: Sel) -> i8 {
    catch_unwind(AssertUnwindSafe(|| {
        queue_accessibility_action(element, SemanticAction::Activate)
    }))
    .unwrap_or(0)
}

fn queue_accessibility_action(element: Id, action: SemanticAction) -> i8 {
    let binding = accessibility_bindings()
        .lock()
        .unwrap()
        .get(&(element as usize))
        .cloned();
    let Some(binding) = binding else { return 0 };
    if !binding.actions.contains(&action) {
        return 0;
    }
    binding
        .queue
        .lock()
        .unwrap()
        .push_back(AccessibilityActionRequest {
            node: binding.node,
            action,
            value: None,
        });
    1
}

unsafe extern "C" fn accessibility_perform_increment(element: Id, _: Sel) -> i8 {
    catch_unwind(AssertUnwindSafe(|| {
        queue_accessibility_action(element, SemanticAction::Increment)
    }))
    .unwrap_or(0)
}

unsafe extern "C" fn accessibility_perform_decrement(element: Id, _: Sel) -> i8 {
    catch_unwind(AssertUnwindSafe(|| {
        queue_accessibility_action(element, SemanticAction::Decrement)
    }))
    .unwrap_or(0)
}

unsafe extern "C" fn accessibility_set_value(element: Id, _: Sel, value: Id) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let binding = accessibility_bindings()
            .lock()
            .unwrap()
            .get(&(element as usize))
            .cloned();
        let Some(binding) = binding else { return };
        if binding.updating {
            unsafe { send_super_void_id(element, c"setAccessibilityValue:", value) };
            return;
        }
        if !binding.actions.contains(&SemanticAction::SetValue) || value.is_null() {
            return;
        }
        let printable = unsafe { send_id(value, c"description") };
        let bytes = unsafe { send_cstr(printable, c"UTF8String") };
        if bytes.is_null() {
            return;
        }
        let Ok(value) = (unsafe { CStr::from_ptr(bytes) }).to_str() else {
            return;
        };
        binding
            .queue
            .lock()
            .unwrap()
            .push_back(AccessibilityActionRequest {
                node: binding.node,
                action: SemanticAction::SetValue,
                value: Some(value.to_owned()),
            });
    }));
}

unsafe extern "C" fn accessibility_set_focused(element: Id, _: Sel, focused: i8) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let binding = accessibility_bindings()
            .lock()
            .unwrap()
            .get(&(element as usize))
            .cloned();
        let Some(binding) = binding else { return };
        if binding.updating {
            unsafe { send_super_void_bool(element, c"setAccessibilityFocused:", focused) };
            return;
        }
        if focused == 0 || !binding.actions.contains(&SemanticAction::Focus) {
            return;
        }
        binding
            .queue
            .lock()
            .unwrap()
            .push_back(AccessibilityActionRequest {
                node: binding.node,
                action: SemanticAction::Focus,
                value: None,
            });
    }));
}

unsafe extern "C" fn accessibility_is_editable(element: Id, _: Sel) -> i8 {
    accessibility_bindings()
        .lock()
        .unwrap()
        .get(&(element as usize))
        .is_some_and(|binding| {
            binding.text.is_some() && binding.actions.contains(&SemanticAction::SetValue)
        })
        .into()
}

unsafe extern "C" fn accessibility_character_count(element: Id, _: Sel) -> usize {
    accessibility_bindings()
        .lock()
        .unwrap()
        .get(&(element as usize))
        .and_then(|binding| binding.text.as_ref())
        .map_or(0, |text| text.character_count_utf16)
}

unsafe fn accessibility_element_class() -> Result<Id, Error> {
    static CLASS: OnceLock<Result<usize, Error>> = OnceLock::new();
    let result = CLASS.get_or_init(|| unsafe {
        let superclass = class(c"NSAccessibilityElement")?;
        let cls =
            objc_allocateClassPair(superclass, c"ChoscoruiV2AccessibilityElement".as_ptr(), 0);
        if cls.is_null() {
            return Err(Error::AppKitUnavailable);
        }
        if class_addMethod(
            cls,
            selector(c"accessibilityPerformPress"),
            accessibility_perform_press as *const c_void,
            c"c@:".as_ptr(),
        ) == 0
        {
            objc_disposeClassPair(cls);
            return Err(Error::AppKitUnavailable);
        }
        for (name, imp) in [
            (
                c"accessibilityPerformIncrement",
                accessibility_perform_increment as *const c_void,
            ),
            (
                c"accessibilityPerformDecrement",
                accessibility_perform_decrement as *const c_void,
            ),
        ] {
            if class_addMethod(cls, selector(name), imp, c"c@:".as_ptr()) == 0 {
                objc_disposeClassPair(cls);
                return Err(Error::AppKitUnavailable);
            }
        }
        if class_addMethod(
            cls,
            selector(c"setAccessibilityFocused:"),
            accessibility_set_focused as *const c_void,
            c"v@:c".as_ptr(),
        ) == 0
        {
            objc_disposeClassPair(cls);
            return Err(Error::AppKitUnavailable);
        }
        for (name, imp, types) in [
            (
                c"isAccessibilityEditable",
                accessibility_is_editable as *const c_void,
                c"c@:",
            ),
            (
                c"accessibilityNumberOfCharacters",
                accessibility_character_count as *const c_void,
                c"Q@:",
            ),
        ] {
            if class_addMethod(cls, selector(name), imp, types.as_ptr()) == 0 {
                objc_disposeClassPair(cls);
                return Err(Error::AppKitUnavailable);
            }
        }
        if class_addMethod(
            cls,
            selector(c"setAccessibilityValue:"),
            accessibility_set_value as *const c_void,
            c"v@:@".as_ptr(),
        ) == 0
        {
            objc_disposeClassPair(cls);
            return Err(Error::AppKitUnavailable);
        }
        objc_registerClassPair(cls);
        Ok(cls as usize)
    });
    result.map(|value| value as Id)
}

fn window_queues() -> &'static Mutex<HashMap<usize, WindowQueue>> {
    WINDOW_QUEUES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn paint_states() -> &'static Mutex<HashMap<usize, PaintHandle>> {
    PAINT_STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

unsafe extern "C" fn view_draw_rect(view: Id, _: Sel, dirty: NSRect) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: drawRect: runs with a current NSGraphicsContext. The CGContext
        // is borrowed only during this callback and is never exposed publicly.
        unsafe {
            let window = send_id(view, c"window");
            if window.is_null() {
                return;
            }
            let paint = paint_states()
                .lock()
                .unwrap()
                .get(&(window as usize))
                .cloned();
            let Some(paint) = paint else {
                return;
            };
            let list = paint.lock().unwrap().list.clone();
            let graphics = send_id(class(c"NSGraphicsContext").unwrap(), c"currentContext");
            if graphics.is_null() {
                return;
            }
            let context = send_id(graphics, c"CGContext");
            if context.is_null() {
                return;
            }
            if paint::paint_commands(context, dirty, &list) {
                paint.lock().unwrap().count += 1;
            }
        }
    }));
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitmapError {
    InvalidScale,
    TooLarge,
    ContextUnavailable,
    PaintFailed,
}

/// Premultiplied RGBA8 pixels drawn with the same Core Graphics/Core Text
/// command path as a window. Rows are contiguous in bitmap storage order.
pub struct MacBitmap {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl MacBitmap {
    pub fn render(
        list: &DisplayList,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<Self, BitmapError> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(BitmapError::InvalidScale);
        }
        let row_len = (width as usize)
            .checked_mul(4)
            .ok_or(BitmapError::TooLarge)?;
        let len = row_len
            .checked_mul(height as usize)
            .ok_or(BitmapError::TooLarge)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(len)
            .map_err(|_| BitmapError::TooLarge)?;
        if width == 0 || height == 0 {
            return Ok(Self {
                width,
                height,
                pixels,
            });
        }
        // SAFETY: Core Graphics owns the automatically allocated context data.
        // It stays alive until every row has been copied into `pixels`.
        unsafe {
            let color_space = CfOwned::new(CGColorSpaceCreateDeviceRGB())
                .ok_or(BitmapError::ContextUnavailable)?;
            let context = CfOwned::new(CGBitmapContextCreate(
                std::ptr::null_mut(),
                width as usize,
                height as usize,
                8,
                0,
                color_space.as_ptr(),
                1 | (4 << 12),
            ))
            .ok_or(BitmapError::ContextUnavailable)?;
            CGContextTranslateCTM(context.as_ptr(), 0.0, f64::from(height));
            CGContextScaleCTM(context.as_ptr(), scale, -scale);
            let dirty = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: f64::from(width) / scale,
                    height: f64::from(height) / scale,
                },
            };
            if !paint::paint_commands(context.as_ptr(), dirty, list) {
                return Err(BitmapError::PaintFailed);
            }
            let data = NonNull::new(CGBitmapContextGetData(context.as_ptr()))
                .ok_or(BitmapError::ContextUnavailable)?;
            let stride = CGBitmapContextGetBytesPerRow(context.as_ptr());
            if stride < row_len || stride.checked_mul(height as usize).is_none() {
                return Err(BitmapError::TooLarge);
            }
            pixels.resize(len, 0);
            for y in 0..height as usize {
                let row = std::slice::from_raw_parts(
                    (data.as_ptr() as *const u8).add(y * stride),
                    row_len,
                );
                pixels[y * row_len..(y + 1) * row_len].copy_from_slice(row);
            }
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let offset = (y as usize * self.width as usize + x as usize) * 4;
        Some(
            self.pixels[offset..offset + 4]
                .try_into()
                .expect("pixel has four channels"),
        )
    }
}

unsafe fn cf_utf8(text: &str) -> Option<CfOwned> {
    let length = isize::try_from(text.len()).ok()?;
    CfOwned::new(unsafe {
        CFStringCreateWithBytes(std::ptr::null_mut(), text.as_ptr(), length, 0x0800_0100, 0)
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShapeError {
    InvalidFont,
    InvalidWidth,
    InvalidSelection,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLineMetrics {
    pub advance: f64,
    pub ascent: f64,
    pub descent: f64,
    pub leading: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretOffsets {
    pub primary: f64,
    pub secondary: f64,
}

/// An immutable Core Text line. Measurement, caret offsets, and drawing use
/// this same shaped object, with grapheme indices at the safe API boundary.
pub struct ShapedLine {
    text: TextBuffer,
    line: CfOwned,
}

impl ShapedLine {
    pub fn new(text: &str, family: &str, size: f64) -> Result<Self, ShapeError> {
        if family.is_empty() || !size.is_finite() || size <= 0.0 {
            return Err(ShapeError::InvalidFont);
        }
        // SAFETY: The attributed string and font stay alive through line
        // creation. Core Text returns a separately owned immutable CTLine.
        unsafe {
            let string = cf_utf8(text).ok_or(ShapeError::Unavailable)?;
            let font_name = cf_utf8(family).ok_or(ShapeError::Unavailable)?;
            let font = CfOwned::new(CTFontCreateWithName(
                font_name.as_ptr(),
                size,
                std::ptr::null_mut(),
            ))
            .ok_or(ShapeError::Unavailable)?;
            let keys = [
                kCTFontAttributeName,
                kCTForegroundColorFromContextAttributeName,
            ];
            let values = [font.as_ptr(), kCFBooleanTrue as Id];
            let attributes = CfOwned::new(CFDictionaryCreate(
                std::ptr::null_mut(),
                keys.as_ptr(),
                values.as_ptr(),
                2,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ))
            .ok_or(ShapeError::Unavailable)?;
            let attributed = CfOwned::new(CFAttributedStringCreate(
                std::ptr::null_mut(),
                string.as_ptr(),
                attributes.as_ptr(),
            ))
            .ok_or(ShapeError::Unavailable)?;
            let line = CfOwned::new(CTLineCreateWithAttributedString(attributed.as_ptr()))
                .ok_or(ShapeError::Unavailable)?;
            Ok(Self {
                text: TextBuffer::new(text),
                line,
            })
        }
    }

    pub fn metrics(&self) -> TextLineMetrics {
        let mut ascent = 0.0;
        let mut descent = 0.0;
        let mut leading = 0.0;
        // SAFETY: `line` is an owned live CTLine; output pointers are valid.
        let advance = unsafe {
            CTLineGetTypographicBounds(self.line.as_ptr(), &mut ascent, &mut descent, &mut leading)
        };
        TextLineMetrics {
            advance,
            ascent,
            descent,
            leading,
        }
    }

    pub fn caret_offsets(&self, grapheme_index: usize) -> Option<CaretOffsets> {
        let byte = self.text.grapheme_to_byte(grapheme_index)?;
        let utf16 = self.text.byte_to_utf16(byte)?;
        let index = isize::try_from(utf16).ok()?;
        let mut secondary = 0.0;
        // SAFETY: UTF-16 offset is a valid grapheme boundary in this CTLine.
        let primary =
            unsafe { CTLineGetOffsetForStringIndex(self.line.as_ptr(), index, &mut secondary) };
        Some(CaretOffsets { primary, secondary })
    }

    /// Returns the nearest grapheme caret for a horizontal point on this line.
    /// Uses Core Text's primary caret positions; mixed-direction affinity is
    /// exposed separately through `caret_offsets`.
    pub fn hit_test_x(&self, x: f64) -> usize {
        if !x.is_finite() {
            return 0;
        }
        let mut nearest = 0;
        let mut distance = f64::INFINITY;
        for index in 0..=self.text.grapheme_count() {
            let Some(offset) = self.caret_offsets(index) else {
                continue;
            };
            let current = (offset.primary - x).abs();
            if current < distance {
                nearest = index;
                distance = current;
            }
        }
        nearest
    }

    unsafe fn draw(&self, context: Id, baseline: Point, color: choscorui_core::Color) {
        unsafe {
            let previous_text_matrix = CGContextGetTextMatrix(context);
            CGContextSaveGState(context);
            CGContextSetRGBFillColor(
                context,
                f64::from(color.red) / 255.0,
                f64::from(color.green) / 255.0,
                f64::from(color.blue) / 255.0,
                f64::from(color.alpha) / 255.0,
            );
            CGContextTranslateCTM(context, baseline.x, baseline.y);
            CGContextScaleCTM(context, 1.0, -1.0);
            CGContextSetTextMatrix(
                context,
                CGAffineTransform {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    tx: 0.0,
                    ty: 0.0,
                },
            );
            CGContextSetTextPosition(context, 0.0, 0.0);
            CTLineDraw(self.line.as_ptr(), context);
            CGContextRestoreGState(context);
            CGContextSetTextMatrix(context, previous_text_matrix);
        }
    }
}

pub struct WrappedLine {
    text: String,
    shaped: ShapedLine,
    baseline_y: f64,
    grapheme_range: Range<usize>,
}

impl WrappedLine {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn metrics(&self) -> TextLineMetrics {
        self.shaped.metrics()
    }
    pub fn baseline_y(&self) -> f64 {
        self.baseline_y
    }
    pub fn shaped(&self) -> &ShapedLine {
        &self.shaped
    }
    pub fn grapheme_range(&self) -> Range<usize> {
        self.grapheme_range.clone()
    }
}

/// Width-limited lines from Core Text's contextual break suggestions.
/// Each line owns the same ShapedLine type used for metrics and painting.
pub struct WrappedText {
    lines: Vec<WrappedLine>,
    family: String,
    size: f64,
    source_graphemes: usize,
}

impl WrappedText {
    pub fn new(text: &str, family: &str, size: f64, width: f64) -> Result<Self, ShapeError> {
        if !width.is_finite() || width <= 0.0 {
            return Err(ShapeError::InvalidWidth);
        }
        if family.is_empty() || !size.is_finite() || size <= 0.0 {
            return Err(ShapeError::InvalidFont);
        }
        let mut lines = Vec::new();
        let mut top = 0.0;
        let source = TextBuffer::new(text);
        let mut paragraph_byte = 0usize;
        for raw_paragraph in text.split('\n') {
            let paragraph_start = paragraph_byte;
            let paragraph_end = paragraph_start + raw_paragraph.len();
            paragraph_byte = paragraph_end + usize::from(paragraph_end < text.len());
            let paragraph = raw_paragraph.strip_suffix('\r').unwrap_or(raw_paragraph);
            if paragraph.is_empty() {
                let grapheme = source
                    .byte_to_grapheme(paragraph_start)
                    .ok_or(ShapeError::Unavailable)?;
                append_wrapped_line(&mut lines, "", family, size, &mut top, grapheme..grapheme)?;
                continue;
            }
            // SAFETY: The temporary attributed text, font and dictionary stay
            // alive for every typesetter query in this paragraph.
            unsafe {
                let string = cf_utf8(paragraph).ok_or(ShapeError::Unavailable)?;
                let font_name = cf_utf8(family).ok_or(ShapeError::Unavailable)?;
                let font = CfOwned::new(CTFontCreateWithName(
                    font_name.as_ptr(),
                    size,
                    std::ptr::null_mut(),
                ))
                .ok_or(ShapeError::Unavailable)?;
                let keys = [
                    kCTFontAttributeName,
                    kCTForegroundColorFromContextAttributeName,
                ];
                let values = [font.as_ptr(), kCFBooleanTrue as Id];
                let attributes = CfOwned::new(CFDictionaryCreate(
                    std::ptr::null_mut(),
                    keys.as_ptr(),
                    values.as_ptr(),
                    2,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                ))
                .ok_or(ShapeError::Unavailable)?;
                let attributed = CfOwned::new(CFAttributedStringCreate(
                    std::ptr::null_mut(),
                    string.as_ptr(),
                    attributes.as_ptr(),
                ))
                .ok_or(ShapeError::Unavailable)?;
                let typesetter =
                    CfOwned::new(CTTypesetterCreateWithAttributedString(attributed.as_ptr()))
                        .ok_or(ShapeError::Unavailable)?;
                let buffer = TextBuffer::new(paragraph);
                let total = buffer.grapheme_count();
                let mut start = 0;
                while start < total {
                    let start_byte = buffer
                        .grapheme_to_byte(start)
                        .ok_or(ShapeError::Unavailable)?;
                    let start_utf16 = buffer
                        .byte_to_utf16(start_byte)
                        .ok_or(ShapeError::Unavailable)?;
                    let start_index =
                        isize::try_from(start_utf16).map_err(|_| ShapeError::Unavailable)?;
                    let count =
                        CTTypesetterSuggestLineBreak(typesetter.as_ptr(), start_index, width);
                    let suggested_end =
                        start_utf16.saturating_add(usize::try_from(count).unwrap_or(0));
                    let mut end = start;
                    for candidate in start + 1..=total {
                        let byte = buffer
                            .grapheme_to_byte(candidate)
                            .ok_or(ShapeError::Unavailable)?;
                        let offset = buffer.byte_to_utf16(byte).ok_or(ShapeError::Unavailable)?;
                        if offset > suggested_end {
                            break;
                        }
                        end = candidate;
                    }
                    if end == start {
                        end += 1;
                    }
                    let end_byte = buffer
                        .grapheme_to_byte(end)
                        .ok_or(ShapeError::Unavailable)?;
                    let source_start = source
                        .byte_to_grapheme(paragraph_start + start_byte)
                        .ok_or(ShapeError::Unavailable)?;
                    let source_end = source
                        .byte_to_grapheme(paragraph_start + end_byte)
                        .ok_or(ShapeError::Unavailable)?;
                    append_wrapped_line(
                        &mut lines,
                        &paragraph[start_byte..end_byte],
                        family,
                        size,
                        &mut top,
                        source_start..source_end,
                    )?;
                    start = end;
                }
            }
        }
        Ok(Self {
            lines,
            family: family.to_owned(),
            size,
            source_graphemes: source.grapheme_count(),
        })
    }

    pub fn lines(&self) -> &[WrappedLine] {
        &self.lines
    }

    /// Maps a point relative to the paragraph origin to the nearest source
    /// grapheme caret, including across explicit newline gaps.
    pub fn hit_test(&self, point: Point) -> usize {
        if !point.x.is_finite() || !point.y.is_finite() {
            return 0;
        }
        let Some(mut nearest) = self.lines.first() else {
            return 0;
        };
        let mut distance = (point.y - nearest.baseline_y).abs();
        for line in self.lines.iter().skip(1) {
            let current = (point.y - line.baseline_y).abs();
            if current < distance {
                nearest = line;
                distance = current;
            }
        }
        (nearest.grapheme_range.start + nearest.shaped.hit_test_x(point.x))
            .min(nearest.grapheme_range.end)
    }

    pub fn selection_rects(
        &self,
        selection: Range<usize>,
    ) -> Result<Vec<choscorui_core::Rect>, ShapeError> {
        if selection.start > selection.end || selection.end > self.source_graphemes {
            return Err(ShapeError::InvalidSelection);
        }
        let mut rects = Vec::new();
        for line in &self.lines {
            let start = selection.start.max(line.grapheme_range.start);
            let end = selection.end.min(line.grapheme_range.end);
            if start >= end {
                continue;
            }
            let first = line
                .shaped
                .caret_offsets(start - line.grapheme_range.start)
                .ok_or(ShapeError::Unavailable)?
                .primary;
            let last = line
                .shaped
                .caret_offsets(end - line.grapheme_range.start)
                .ok_or(ShapeError::Unavailable)?
                .primary;
            let metrics = line.shaped.metrics();
            rects.push(choscorui_core::Rect::new(
                Point::new(first.min(last), line.baseline_y - metrics.ascent),
                Size::new(
                    (last - first).abs(),
                    (metrics.ascent + metrics.descent + metrics.leading).max(0.0),
                ),
            ));
        }
        Ok(rects)
    }

    pub fn caret_rect(&self, caret: usize) -> Result<choscorui_core::Rect, ShapeError> {
        if caret > self.source_graphemes {
            return Err(ShapeError::InvalidSelection);
        }
        let line = self
            .lines
            .iter()
            .find(|line| line.grapheme_range.start <= caret && caret <= line.grapheme_range.end)
            .or_else(|| {
                self.lines
                    .iter()
                    .find(|line| line.grapheme_range.start > caret)
            })
            .or_else(|| self.lines.last())
            .ok_or(ShapeError::Unavailable)?;
        let local = caret
            .saturating_sub(line.grapheme_range.start)
            .min(line.grapheme_range.end - line.grapheme_range.start);
        let x = line
            .shaped
            .caret_offsets(local)
            .ok_or(ShapeError::Unavailable)?
            .primary;
        let metrics = line.shaped.metrics();
        Ok(choscorui_core::Rect::new(
            Point::new(x, line.baseline_y - metrics.ascent),
            Size::new(
                1.0,
                (metrics.ascent + metrics.descent + metrics.leading).max(0.0),
            ),
        ))
    }

    /// Moves one visual line up or down while retaining a preferred x value.
    /// The returned caret is a source grapheme index.
    pub fn move_caret_vertical(
        &self,
        caret: usize,
        direction: i32,
        preferred_x: Option<f64>,
    ) -> Result<(usize, f64), ShapeError> {
        if caret > self.source_graphemes {
            return Err(ShapeError::InvalidSelection);
        }
        let current = self
            .lines
            .iter()
            .position(|line| line.grapheme_range.start <= caret && caret <= line.grapheme_range.end)
            .or_else(|| {
                self.lines
                    .iter()
                    .position(|line| line.grapheme_range.start > caret)
            })
            .ok_or(ShapeError::Unavailable)?;
        let x = match preferred_x {
            Some(x) if x.is_finite() => x,
            _ => self.caret_rect(caret)?.origin.x,
        };
        let target = if direction < 0 {
            current.saturating_sub(1)
        } else if direction > 0 {
            (current + 1).min(self.lines.len() - 1)
        } else {
            current
        };
        let line = &self.lines[target];
        let local = line.shaped.hit_test_x(x);
        Ok((
            (line.grapheme_range.start + local).min(line.grapheme_range.end),
            x,
        ))
    }

    pub fn append_to_display_list(
        &self,
        list: &mut DisplayList,
        origin: Point,
        color: choscorui_core::Color,
    ) -> Result<(), choscorui_core::DisplayListError> {
        for line in &self.lines {
            list.draw_text(
                Point::new(origin.x, origin.y + line.baseline_y),
                &line.text,
                &self.family,
                self.size,
                color,
            )?;
        }
        Ok(())
    }
}

fn append_wrapped_line(
    lines: &mut Vec<WrappedLine>,
    text: &str,
    family: &str,
    size: f64,
    top: &mut f64,
    grapheme_range: Range<usize>,
) -> Result<(), ShapeError> {
    let shaped = ShapedLine::new(text, family, size)?;
    let metrics = shaped.metrics();
    let baseline_y = *top + metrics.ascent.max(size * 0.8);
    *top += (metrics.ascent + metrics.descent + metrics.leading).max(size * 1.2);
    lines.push(WrappedLine {
        text: text.to_owned(),
        shaped,
        baseline_y,
        grapheme_range,
    });
    Ok(())
}

unsafe fn draw_text_run(
    context: Id,
    baseline: Point,
    text: &str,
    family: &str,
    size: f64,
    color: choscorui_core::Color,
) -> bool {
    let Ok(line) = ShapedLine::new(text, family, size) else {
        return false;
    };
    unsafe { line.draw(context, baseline, color) };
    true
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

fn record_window_event(notification: Id, make_event: impl FnOnce(Id) -> WindowEvent) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Each delegate selector passes a live NSNotification.
        let window = unsafe { send_id(notification, c"object") };
        let queue = window_queues()
            .lock()
            .unwrap()
            .get(&(window as usize))
            .cloned();
        if let Some(queue) = queue {
            let event = make_event(window);
            queue.lock().unwrap().push_back(event);
        }
    }));
}

unsafe extern "C" fn window_did_resize(_: Id, _: Sel, notification: Id) {
    record_window_event(notification, |window| WindowEvent::Resized {
        // SAFETY: notification.object is the live NSWindow currently resizing.
        size: unsafe { read_content_size(window) },
        scale: unsafe { send_f64(window, c"backingScaleFactor") },
    });
}
unsafe extern "C" fn window_did_change_scale(_: Id, _: Sel, notification: Id) {
    record_window_event(notification, |window| WindowEvent::ScaleChanged {
        // SAFETY: notification.object is the live NSWindow.
        scale: unsafe { send_f64(window, c"backingScaleFactor") },
    });
}
unsafe extern "C" fn window_did_focus(_: Id, _: Sel, notification: Id) {
    record_window_event(notification, |_| WindowEvent::Focused(true));
}
unsafe extern "C" fn window_did_blur(_: Id, _: Sel, notification: Id) {
    record_window_event(notification, |_| WindowEvent::Focused(false));
}
unsafe extern "C" fn window_will_close(_: Id, _: Sel, notification: Id) {
    record_window_event(notification, |_| WindowEvent::Closing);
}

unsafe extern "C" fn view_is_flipped(_: Id, _: Sel) -> i8 {
    1
}
unsafe extern "C" fn view_accepts_first_responder(_: Id, _: Sel) -> i8 {
    1
}
unsafe extern "C" fn view_accepts_first_mouse(_: Id, _: Sel, _: Id) -> i8 {
    1
}

unsafe extern "C" fn view_pointer_event(view: Id, _: Sel, native_event: Id) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: AppKit supplies a live NSEvent; the custom NSView is attached
        // to a live NSWindow. `convertPoint:fromView:nil` maps its window point
        // into the flipped view's logical coordinates.
        unsafe {
            let window = send_id(view, c"window");
            if window.is_null() {
                return;
            }
            let location = send_point(native_event, c"locationInWindow");
            let point = send_point_point_id(
                view,
                c"convertPoint:fromView:",
                location,
                std::ptr::null_mut(),
            );
            let position = Point::new(point.x, point.y);
            let native_type = send_usize(native_event, c"type");
            let event = match native_type {
                1 => PointerEvent::Down {
                    position,
                    button: PointerButton::Primary,
                },
                2 => PointerEvent::Up {
                    position,
                    button: PointerButton::Primary,
                },
                3 => PointerEvent::Down {
                    position,
                    button: PointerButton::Secondary,
                },
                4 => PointerEvent::Up {
                    position,
                    button: PointerButton::Secondary,
                },
                5 | 6 | 7 | 27 => PointerEvent::Move { position },
                22 => {
                    let unit = if send_bool(native_event, c"hasPreciseScrollingDeltas") != 0 {
                        WheelUnit::Points
                    } else {
                        WheelUnit::Lines
                    };
                    PointerEvent::Wheel {
                        position,
                        delta: Point::new(
                            -send_f64(native_event, c"scrollingDeltaX"),
                            -send_f64(native_event, c"scrollingDeltaY"),
                        ),
                        unit,
                    }
                }
                25 | 26 => {
                    let number = send_usize(native_event, c"buttonNumber");
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
            let modifiers = Modifiers::from_native(send_usize(native_event, c"modifierFlags"));
            let queue = window_queues()
                .lock()
                .unwrap()
                .get(&(window as usize))
                .cloned();
            if let Some(queue) = queue {
                queue
                    .lock()
                    .unwrap()
                    .push_back(WindowEvent::Pointer { event, modifiers });
            }
        }
    }));
}

unsafe extern "C" fn view_key_event(view: Id, _: Sel, native_event: Id) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: AppKit supplies a live NSEvent and attached NSView. Keycodes
        // are macOS virtual keycodes; text insertion remains a separate path.
        unsafe {
            let window = send_id(view, c"window");
            if window.is_null() {
                return;
            }
            let key = match send_u16(native_event, c"keyCode") {
                0x30 => Key::Tab,
                0x24 => Key::Enter,
                0x31 => Key::Space,
                0x35 => Key::Escape,
                0x7e => Key::ArrowUp,
                0x7d => Key::ArrowDown,
                0x7b => Key::ArrowLeft,
                0x7c => Key::ArrowRight,
                0x73 => Key::Home,
                0x77 => Key::End,
                0x33 => Key::Backspace,
                0x75 => Key::Delete,
                other => Key::Other(u32::from(other)),
            };
            let event = match send_usize(native_event, c"type") {
                10 => KeyEvent::Down {
                    key,
                    repeat: send_bool(native_event, c"isARepeat") != 0,
                },
                11 => KeyEvent::Up { key },
                _ => return,
            };
            let modifiers = Modifiers::from_native(send_usize(native_event, c"modifierFlags"));
            let queue = window_queues()
                .lock()
                .unwrap()
                .get(&(window as usize))
                .cloned();
            if let Some(queue) = queue {
                let event_count = {
                    let mut queue = queue.lock().unwrap();
                    queue.push_back(WindowEvent::Key { event, modifiers });
                    queue.len()
                };
                if matches!(event, KeyEvent::Down { .. })
                    && !modifiers.control
                    && !modifiers.command
                {
                    let events = ns_array(&[native_event]).unwrap_or(std::ptr::null_mut());
                    if !events.is_null() {
                        send_void_id(view, c"interpretKeyEvents:", events);
                    }
                    let mut queue = queue.lock().unwrap();
                    if queue.len() == event_count {
                        let characters = send_id(native_event, c"characters");
                        if !characters.is_null() {
                            let bytes = send_cstr(characters, c"UTF8String");
                            if !bytes.is_null() {
                                let text: String = CStr::from_ptr(bytes)
                                    .to_string_lossy()
                                    .chars()
                                    .filter(|character| {
                                        !character.is_control()
                                            && !(('\u{f700}'..='\u{f8ff}').contains(character))
                                    })
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
            }
        }
    }));
}

unsafe fn view_class() -> Result<Id, Error> {
    static CLASS: OnceLock<Result<usize, Error>> = OnceLock::new();
    let result = CLASS.get_or_init(|| {
        // SAFETY: Registration happens once. BOOL methods use c@: and
        // mouseDown: uses v@:@ as required by Objective-C runtime ABI.
        unsafe {
            let superclass = class(c"NSView")?;
            let cls = objc_allocateClassPair(superclass, c"ChoscoruiV2ContentView".as_ptr(), 0);
            if cls.is_null() {
                return Err(Error::AppKitUnavailable);
            }
            for (name, imp, types) in [
                (c"isFlipped", view_is_flipped as *const c_void, c"c@:"),
                (
                    c"acceptsFirstResponder",
                    view_accepts_first_responder as *const c_void,
                    c"c@:",
                ),
                (
                    c"acceptsFirstMouse:",
                    view_accepts_first_mouse as *const c_void,
                    c"c@:@",
                ),
                (c"mouseDown:", view_pointer_event as *const c_void, c"v@:@"),
                (c"mouseUp:", view_pointer_event as *const c_void, c"v@:@"),
                (
                    c"rightMouseDown:",
                    view_pointer_event as *const c_void,
                    c"v@:@",
                ),
                (
                    c"rightMouseUp:",
                    view_pointer_event as *const c_void,
                    c"v@:@",
                ),
                (
                    c"otherMouseDown:",
                    view_pointer_event as *const c_void,
                    c"v@:@",
                ),
                (
                    c"otherMouseUp:",
                    view_pointer_event as *const c_void,
                    c"v@:@",
                ),
                (c"mouseMoved:", view_pointer_event as *const c_void, c"v@:@"),
                (
                    c"scrollWheel:",
                    view_pointer_event as *const c_void,
                    c"v@:@",
                ),
                (
                    c"mouseDragged:",
                    view_pointer_event as *const c_void,
                    c"v@:@",
                ),
                (
                    c"rightMouseDragged:",
                    view_pointer_event as *const c_void,
                    c"v@:@",
                ),
                (
                    c"otherMouseDragged:",
                    view_pointer_event as *const c_void,
                    c"v@:@",
                ),
                (c"keyDown:", view_key_event as *const c_void, c"v@:@"),
                (c"keyUp:", view_key_event as *const c_void, c"v@:@"),
                (
                    c"hasMarkedText",
                    view_has_marked_text as *const c_void,
                    c"c@:",
                ),
                (
                    c"markedRange",
                    view_marked_range as *const c_void,
                    c"{_NSRange=QQ}@:",
                ),
                (
                    c"selectedRange",
                    view_selected_range as *const c_void,
                    c"{_NSRange=QQ}@:",
                ),
                (
                    c"setMarkedText:selectedRange:replacementRange:",
                    view_set_marked_text as *const c_void,
                    c"v@:@{_NSRange=QQ}{_NSRange=QQ}",
                ),
                (
                    c"insertText:replacementRange:",
                    view_insert_text as *const c_void,
                    c"v@:@{_NSRange=QQ}",
                ),
                (c"unmarkText", view_unmark_text as *const c_void, c"v@:"),
                (
                    c"validAttributesForMarkedText",
                    view_valid_marked_attributes as *const c_void,
                    c"@@:",
                ),
                (
                    c"attributedSubstringForProposedRange:actualRange:",
                    view_attributed_substring as *const c_void,
                    c"@@:{_NSRange=QQ}^{_NSRange=QQ}",
                ),
                (
                    c"firstRectForCharacterRange:actualRange:",
                    view_first_rect_for_range as *const c_void,
                    c"{CGRect={CGPoint=dd}{CGSize=dd}}@:{_NSRange=QQ}^{_NSRange=QQ}",
                ),
                (
                    c"characterIndexForPoint:",
                    view_character_index_for_point as *const c_void,
                    c"Q@:{CGPoint=dd}",
                ),
                (
                    c"doCommandBySelector:",
                    view_do_command as *const c_void,
                    c"v@::",
                ),
                (
                    c"drawRect:",
                    view_draw_rect as *const c_void,
                    c"v@:{CGRect={CGPoint=dd}{CGSize=dd}}",
                ),
            ] {
                if class_addMethod(cls, selector(name), imp, types.as_ptr()) == 0 {
                    objc_disposeClassPair(cls);
                    return Err(Error::AppKitUnavailable);
                }
            }
            objc_registerClassPair(cls);
            Ok(cls as usize)
        }
    });
    result.map(|value| value as Id)
}

unsafe fn delegate_class() -> Result<Id, Error> {
    static CLASS: OnceLock<Result<usize, Error>> = OnceLock::new();
    let result = CLASS.get_or_init(|| {
        // SAFETY: Runtime registration happens once; all method signatures
        // are void(id, SEL, NSNotification*) with Objective-C type v@:@.
        unsafe {
            let superclass = class(c"NSObject")?;
            let cls = objc_allocateClassPair(superclass, c"ChoscoruiV2WindowDelegate".as_ptr(), 0);
            if cls.is_null() {
                return Err(Error::AppKitUnavailable);
            }
            for (name, imp) in [
                (c"windowDidResize:", window_did_resize as *const c_void),
                (
                    c"windowDidChangeBackingProperties:",
                    window_did_change_scale as *const c_void,
                ),
                (c"windowDidBecomeKey:", window_did_focus as *const c_void),
                (c"windowDidResignKey:", window_did_blur as *const c_void),
                (c"windowWillClose:", window_will_close as *const c_void),
            ] {
                if class_addMethod(cls, selector(name), imp, c"v@:@".as_ptr()) == 0 {
                    objc_disposeClassPair(cls);
                    return Err(Error::AppKitUnavailable);
                }
            }
            objc_registerClassPair(cls);
            Ok(cls as usize)
        }
    });
    result.map(|value| value as Id)
}

pub struct WindowOptions {
    title: String,
    width: f64,
    height: f64,
}
impl WindowOptions {
    pub fn new(title: impl Into<String>, width: f64, height: f64) -> Self {
        Self {
            title: title.into(),
            width,
            height,
        }
    }
}

struct Pool(NonNull<c_void>);
impl Pool {
    fn new() -> Result<Self, Error> {
        // SAFETY: NSAutoreleasePool alloc/init are SDK selectors and the pool
        // is created and drained on the same thread.
        unsafe {
            let cls = class(c"NSAutoreleasePool")?;
            NonNull::new(send_id(send_id(cls, c"alloc"), c"init"))
                .map(Self)
                .ok_or(Error::AppKitUnavailable)
        }
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        // SAFETY: This pool is still owned by the creating thread.
        unsafe { send_void(self.0.as_ptr(), c"drain") }
    }
}

/// Main-thread AppKit process handle. The marker prevents worker transfer.
pub struct Application {
    raw: NonNull<c_void>,
    posted: Arc<PostInner>,
    source_info: *const PostInner,
    timers: RefCell<Vec<Weak<TimerInner>>>,
    timer_panics: Rc<Cell<usize>>,
    _main_thread: PhantomData<Rc<()>>,
}
impl Application {
    pub fn new() -> Result<Self, Error> {
        // SAFETY: pthread_main_np takes no arguments and returns a C int.
        if unsafe { pthread_main_np() } == 0 {
            return Err(Error::WrongThread);
        }
        let _pool = Pool::new()?;
        // SAFETY: The selectors and Regular activation policy (0) match SDK headers.
        unsafe {
            let cls = class(c"NSApplication")?;
            let raw =
                NonNull::new(send_id(cls, c"sharedApplication")).ok_or(Error::AppKitUnavailable)?;
            if send_bool_usize(raw.as_ptr(), c"setActivationPolicy:", 0) == 0 {
                return Err(Error::AppKitUnavailable);
            }
            static FINISH_LAUNCHING: Once = Once::new();
            FINISH_LAUNCHING.call_once(|| send_void(raw.as_ptr(), c"finishLaunching"));
            let runloop = CFRunLoopGetMain();
            if runloop.is_null() {
                return Err(Error::AppKitUnavailable);
            }
            let posted = Arc::new(PostInner {
                application: raw.as_ptr() as usize,
                runloop: runloop as usize,
                state: Mutex::new(PostState {
                    source: 0,
                    tasks: VecDeque::new(),
                    panics: 0,
                }),
            });
            let source_info = Arc::into_raw(Arc::clone(&posted));
            let mut context = CFRunLoopSourceContext {
                version: 0,
                info: source_info as *mut c_void,
                retain: None,
                release: None,
                copy_description: None,
                equal: None,
                hash: None,
                schedule: None,
                cancel: None,
                perform: Some(perform_posted),
            };
            let source = CFRunLoopSourceCreate(std::ptr::null(), 0, &mut context);
            if source.is_null() {
                drop(Arc::from_raw(source_info));
                return Err(Error::AppKitUnavailable);
            }
            posted.state.lock().unwrap().source = source as usize;
            CFRunLoopAddSource(runloop, source, kCFRunLoopDefaultMode);
            Ok(Self {
                raw,
                posted,
                source_info,
                timers: RefCell::new(Vec::new()),
                timer_panics: Rc::new(Cell::new(0)),
                _main_thread: PhantomData,
            })
        }
    }

    pub fn install_main_menu(&self, model: MenuBarModel) -> Result<MainMenu, MenuError> {
        MainMenu::install(self, model)
    }

    pub fn create_window(&self, options: WindowOptions) -> Result<Window, Error> {
        if !options.width.is_finite()
            || !options.height.is_finite()
            || options.width <= 0.0
            || options.height <= 0.0
        {
            return Err(Error::InvalidSize);
        }
        let title = CString::new(options.title).map_err(|_| Error::InvalidTitle)?;
        let _pool = Pool::new()?;
        let frame = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: options.width,
                height: options.height,
            },
        };
        // SAFETY: NSWindow's designated initializer takes NSRect, NSUInteger,
        // NSBackingStoreType, BOOL. The returned +1 object is released in Drop.
        unsafe {
            let cls = class(c"NSWindow")?;
            let raw = NonNull::new(send_window_init(
                send_id(cls, c"alloc"),
                c"initWithContentRect:styleMask:backing:defer:",
                frame,
                1 | 2 | 4 | 8,
                2,
                0,
            ))
            .ok_or(Error::AppKitUnavailable)?;
            send_void_bool(raw.as_ptr(), c"setReleasedWhenClosed:", 0);
            if let Err(error) = set_title(raw.as_ptr(), &title) {
                send_void(raw.as_ptr(), c"release");
                return Err(error);
            }
            let content_class = match view_class() {
                Ok(class) => class,
                Err(error) => {
                    send_void(raw.as_ptr(), c"release");
                    return Err(error);
                }
            };
            let content = send_view_init(
                send_id(content_class, c"alloc"),
                c"initWithFrame:",
                NSRect {
                    origin: NSPoint { x: 0.0, y: 0.0 },
                    size: frame.size,
                },
            );
            if content.is_null() {
                send_void(raw.as_ptr(), c"release");
                return Err(Error::AppKitUnavailable);
            }
            send_void_id(raw.as_ptr(), c"setContentView:", content);
            send_void(content, c"release");
            send_void_bool(raw.as_ptr(), c"setAcceptsMouseMovedEvents:", 1);
            let delegate_class = match delegate_class() {
                Ok(class) => class,
                Err(error) => {
                    send_void(raw.as_ptr(), c"release");
                    return Err(error);
                }
            };
            let delegate = send_id(send_id(delegate_class, c"alloc"), c"init");
            if delegate.is_null() {
                send_void(raw.as_ptr(), c"release");
                return Err(Error::AppKitUnavailable);
            }
            let events = Arc::new(Mutex::new(VecDeque::new()));
            let paint = Arc::new(Mutex::new(PaintState {
                list: DisplayList::new(),
                count: 0,
            }));
            let ime = Arc::new(Mutex::new(TextInputState {
                selected_utf16: 0..0,
                marked_utf16: None,
                caret: Rect::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0)),
            }));
            window_queues()
                .lock()
                .unwrap()
                .insert(raw.as_ptr() as usize, Arc::clone(&events));
            paint_states()
                .lock()
                .unwrap()
                .insert(raw.as_ptr() as usize, Arc::clone(&paint));
            ime_states()
                .lock()
                .unwrap()
                .insert(raw.as_ptr() as usize, Arc::clone(&ime));
            send_void_id(raw.as_ptr(), c"setDelegate:", delegate);
            Ok(Window {
                raw,
                delegate: NonNull::new_unchecked(delegate),
                events,
                paint,
                ime,
                accessibility: RefCell::new(NativeAccessibilityState::new()),
                accessibility_actions: Arc::new(Mutex::new(VecDeque::new())),
                closed: false,
                _main_thread: PhantomData,
            })
        }
    }

    /// Enters AppKit's blocking event loop; `stop` exits it on the UI thread.
    pub fn run(&self) {
        // SAFETY: NSApplication is initialized and used on the main thread.
        unsafe { send_void(self.raw.as_ptr(), c"run") }
    }

    pub fn stop(&self) {
        // SAFETY: stop: accepts a nullable sender.
        unsafe { send_void_id(self.raw.as_ptr(), c"stop:", std::ptr::null_mut()) }
    }

    /// Requests foreground activation. AppKit may complete it asynchronously.
    pub fn activate(&self) {
        // SAFETY: NSApplication activate is available on the macOS 15+ target.
        unsafe { send_void(self.raw.as_ptr(), c"activate") }
    }

    pub fn poster(&self) -> Poster {
        Poster {
            inner: Arc::clone(&self.posted),
        }
    }

    /// Processes at most one run-loop source or timer, blocking up to `timeout`.
    /// This supports native integration tests and future embedded loops.
    pub fn pump_once(&self, timeout: Duration) {
        // SAFETY: Called on the main thread, using the registered default mode.
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, timeout.as_secs_f64(), 1);
        }
    }

    /// Waits for and dispatches one native AppKit event, returning whether one
    /// arrived. The OS run loop also services timers and posted work while
    /// waiting. A caller can drain each window's owned events afterward.
    pub fn dispatch_next_event(&self, timeout: Duration) -> bool {
        let _pool = Pool::new().expect("AppKit autorelease pool unavailable");
        // SAFETY: NSDate and NSApplication selectors use SDK signatures.
        // NSDefaultRunLoopMode is a live Foundation NSString constant.
        unsafe {
            let date = send_id_f64(
                class(c"NSDate").expect("NSDate unavailable"),
                c"dateWithTimeIntervalSinceNow:",
                timeout.as_secs_f64(),
            );
            let event = send_next_event(
                self.raw.as_ptr(),
                c"nextEventMatchingMask:untilDate:inMode:dequeue:",
                usize::MAX,
                date,
                NSDefaultRunLoopMode,
                1,
            );
            if event.is_null() {
                false
            } else {
                send_void_id(self.raw.as_ptr(), c"sendEvent:", event);
                true
            }
        }
    }

    pub fn posted_panic_count(&self) -> usize {
        self.posted.state.lock().unwrap().panics
    }

    /// Schedules a one-shot callback on the main run loop. Posted work is FIFO;
    /// timers run by deadline, with no ordering promise at equal deadlines or
    /// relative to posted work that becomes ready in the same run-loop turn.
    pub fn schedule_once(
        &self,
        delay: Duration,
        action: impl FnOnce() + 'static,
    ) -> Result<Timer, Error> {
        let inner = Rc::new(TimerInner {
            raw: Cell::new(0),
            info: Cell::new(0),
            runloop: self.posted.runloop,
            active: Cell::new(false),
        });
        let info = Box::into_raw(Box::new(TimerContext {
            owner: Rc::downgrade(&inner),
            action: RefCell::new(Some(Box::new(action))),
            panics: Rc::clone(&self.timer_panics),
        }));
        inner.info.set(info as usize);
        let mut context = CFRunLoopTimerContext {
            version: 0,
            info: info as *mut c_void,
            retain: None,
            release: None,
            copy_description: None,
        };
        // SAFETY: Context Box remains owned by TimerInner until invalidation
        // and CFRelease. CF timer callback runs only on the main run loop.
        unsafe {
            let raw = CFRunLoopTimerCreate(
                std::ptr::null(),
                CFAbsoluteTimeGetCurrent() + delay.as_secs_f64(),
                0.0,
                0,
                0,
                fire_timer,
                &mut context,
            );
            if raw.is_null() {
                inner.info.set(0);
                drop(Box::from_raw(info));
                return Err(Error::AppKitUnavailable);
            }
            inner.raw.set(raw as usize);
            inner.active.set(true);
            CFRunLoopAddTimer(
                self.posted.runloop as *mut c_void,
                raw,
                kCFRunLoopDefaultMode,
            );
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
        // SAFETY: This runs on the main thread. The mutex excludes worker
        // signals before invalidation; no callback can overlap main-thread Drop.
        unsafe {
            let mut state = self.posted.state.lock().unwrap();
            let source = std::mem::replace(&mut state.source, 0) as *mut c_void;
            state.tasks.clear();
            CFRunLoopRemoveSource(
                self.posted.runloop as *mut c_void,
                source,
                kCFRunLoopDefaultMode,
            );
            CFRunLoopSourceInvalidate(source);
            CFRelease(source);
            drop(state);
            drop(Arc::from_raw(self.source_info));
        }
    }
}

/// Retained NSWindow. Closing does not release it; Drop releases it once.
pub struct Window {
    raw: NonNull<c_void>,
    delegate: NonNull<c_void>,
    events: WindowQueue,
    paint: PaintHandle,
    ime: ImeHandle,
    accessibility: RefCell<NativeAccessibilityState>,
    accessibility_actions: AccessibilityActionQueue,
    closed: bool,
    _main_thread: PhantomData<Rc<()>>,
}
impl Window {
    pub fn set_text_input_state(&self, state: TextInputState) {
        *self.ime.lock().unwrap() = state;
    }

    /// Reconciles the retained semantic snapshot into stable AppKit
    /// accessibility elements and posts the corresponding OS notifications.
    pub fn set_accessibility(
        &self,
        snapshot: &[SemanticEntry],
        changes: &[SemanticChange],
    ) -> Result<(), Error> {
        let _pool = Pool::new()?;
        let view = unsafe { send_id(self.raw.as_ptr(), c"contentView") };
        let mut state = self.accessibility.borrow_mut();
        let update = state.projection.reconcile(snapshot, changes);
        for identity in &update.retired {
            if let Some(element) = state.elements.remove(identity) {
                accessibility_bindings()
                    .lock()
                    .unwrap()
                    .remove(&(element.as_ptr() as usize));
                unsafe { send_void(element.as_ptr(), c"release") };
            }
        }
        for element in &update.elements {
            if state.elements.contains_key(&element.identity) {
                continue;
            }
            let role = unsafe { native_accessibility_role(element.semantic.role) };
            let label = ns_string(&element.semantic.name)?;
            let parent = element
                .parent
                .and_then(|identity| state.elements.get(&identity).copied())
                .map_or(view, NonNull::as_ptr);
            let frame =
                unsafe { accessibility_screen_rect(view, self.raw.as_ptr(), element.bounds) };
            let raw = unsafe {
                send_accessibility_element(
                    accessibility_element_class()?,
                    c"accessibilityElementWithRole:frame:label:parent:",
                    role,
                    frame,
                    label,
                    parent,
                )
            };
            let raw = NonNull::new(raw).ok_or(Error::AppKitUnavailable)?;
            unsafe { send_id(raw.as_ptr(), c"retain") };
            accessibility_bindings().lock().unwrap().insert(
                raw.as_ptr() as usize,
                NativeAccessibilityBinding {
                    node: element.node,
                    actions: element.semantic.actions.clone(),
                    queue: Arc::clone(&self.accessibility_actions),
                    updating: true,
                    text: element.semantic.text.clone(),
                },
            );
            state.elements.insert(element.identity, raw);
        }
        for element in &update.elements {
            let raw = state.elements[&element.identity].as_ptr();
            if let Some(binding) = accessibility_bindings()
                .lock()
                .unwrap()
                .get_mut(&(raw as usize))
            {
                binding.actions.clone_from(&element.semantic.actions);
                binding.updating = true;
                binding.text.clone_from(&element.semantic.text);
            }
            let frame =
                unsafe { accessibility_screen_rect(view, self.raw.as_ptr(), element.bounds) };
            unsafe {
                send_void_id(
                    raw,
                    c"setAccessibilityRole:",
                    native_accessibility_role(element.semantic.role),
                );
                send_void_id(
                    raw,
                    c"setAccessibilitySubrole:",
                    if element.semantic.role == SemanticRole::PasswordField {
                        NSAccessibilitySecureTextFieldSubrole
                    } else {
                        std::ptr::null_mut()
                    },
                );
                send_void_rect(raw, c"setAccessibilityFrame:", frame);
                send_void_bool(
                    raw,
                    c"setAccessibilityEnabled:",
                    i8::from(!element.semantic.state.disabled),
                );
                send_void_bool(raw, c"setAccessibilityFocused:", i8::from(element.focused));
                send_void_id(
                    raw,
                    c"setAccessibilityLabel:",
                    ns_string(&element.semantic.name)?,
                );
                send_void_id(
                    raw,
                    c"setAccessibilityHelp:",
                    ns_string(&element.semantic.description)?,
                );
                let value = if let Some(checked) = element.semantic.state.checked {
                    send_id_bool(class(c"NSNumber")?, c"numberWithBool:", i8::from(checked))
                } else if let Some(value) = &element.semantic.value {
                    ns_string(value)?
                } else {
                    std::ptr::null_mut()
                };
                send_void_id(raw, c"setAccessibilityValue:", value);
                if let Some(text) = &element.semantic.text {
                    send_void_range(
                        raw,
                        c"setAccessibilitySelectedTextRange:",
                        NSRange {
                            location: text.selection_utf16.start,
                            length: text.selection_utf16.end - text.selection_utf16.start,
                        },
                    );
                }
            }
            if let Some(binding) = accessibility_bindings()
                .lock()
                .unwrap()
                .get_mut(&(raw as usize))
            {
                binding.updating = false;
            }
        }
        let mut children: HashMap<Option<AccessibilityIdentity>, Vec<Id>> = HashMap::new();
        let by_node: HashMap<_, _> = update
            .elements
            .iter()
            .map(|element| (element.node, state.elements[&element.identity].as_ptr()))
            .collect();
        for element in &update.elements {
            let raw = state.elements[&element.identity].as_ptr();
            let mut linked = Vec::new();
            for relation in &element.semantic.relations {
                match *relation {
                    SemanticRelation::LabelledBy(target) => {
                        if let Some(target) = by_node.get(&target) {
                            unsafe {
                                send_void_id(raw, c"setAccessibilityTitleUIElement:", *target)
                            };
                        }
                    }
                    SemanticRelation::DescribedBy(target) | SemanticRelation::Controls(target) => {
                        if let Some(target) = by_node.get(&target) {
                            linked.push(*target);
                        }
                    }
                }
            }
            if !linked.is_empty() {
                unsafe {
                    send_void_id(
                        raw,
                        c"setAccessibilityLinkedUIElements:",
                        ns_array(&linked)?,
                    )
                };
            }
        }
        for element in &update.elements {
            children
                .entry(element.parent)
                .or_default()
                .push(state.elements[&element.identity].as_ptr());
        }
        for element in &update.elements {
            let raw = state.elements[&element.identity].as_ptr();
            let values = children
                .get(&Some(element.identity))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            unsafe { send_void_id(raw, c"setAccessibilityChildren:", ns_array(values)?) };
        }
        let roots = children.get(&None).map(Vec::as_slice).unwrap_or(&[]);
        unsafe { send_void_id(view, c"setAccessibilityChildren:", ns_array(roots)?) };
        for notification in update.notifications {
            unsafe {
                match notification {
                    AccessibilityNotification::LayoutChanged => NSAccessibilityPostNotification(
                        view,
                        NSAccessibilityLayoutChangedNotification,
                    ),
                    AccessibilityNotification::ValueChanged(identity)
                    | AccessibilityNotification::StateChanged(identity) => {
                        if let Some(element) = state.elements.get(&identity) {
                            NSAccessibilityPostNotification(
                                element.as_ptr(),
                                NSAccessibilityValueChangedNotification,
                            );
                        }
                    }
                    AccessibilityNotification::LabelChanged(identity) => {
                        if let Some(element) = state.elements.get(&identity) {
                            NSAccessibilityPostNotification(
                                element.as_ptr(),
                                NSAccessibilityTitleChangedNotification,
                            );
                        }
                    }
                    AccessibilityNotification::FocusChanged(identity) => {
                        if let Some(element) = state.elements.get(&identity) {
                            NSAccessibilityPostNotification(
                                element.as_ptr(),
                                NSAccessibilityFocusedUIElementChangedNotification,
                            );
                        }
                    }
                    AccessibilityNotification::FocusCleared => NSAccessibilityPostNotification(
                        view,
                        NSAccessibilityFocusedUIElementChangedNotification,
                    ),
                }
            }
        }
        Ok(())
    }

    pub fn take_accessibility_actions(&self) -> Vec<AccessibilityActionRequest> {
        let mut actions: Vec<_> = self
            .accessibility_actions
            .lock()
            .unwrap()
            .drain(..)
            .collect();
        actions.extend(self.accessibility.borrow_mut().projection.take_actions());
        actions
    }

    /// Replaces the display list and asks AppKit for a frame only if it changed.
    pub fn set_display_list(&self, list: DisplayList) {
        let mut paint = self.paint.lock().unwrap();
        if paint.list == list {
            return;
        }
        paint.list = list;
        drop(paint);
        // SAFETY: The window owns this NSView until Window::drop.
        unsafe {
            let view = send_id(self.raw.as_ptr(), c"contentView");
            send_void_bool(view, c"setNeedsDisplay:", 1);
        }
    }

    pub fn paint_count(&self) -> usize {
        self.paint.lock().unwrap().count
    }

    pub fn take_events(&mut self) -> Vec<WindowEvent> {
        self.events.lock().unwrap().drain(..).collect()
    }
    pub fn show(&mut self) {
        let _pool = Pool::new().expect("AppKit autorelease pool unavailable");
        // SAFETY: makeKeyAndOrderFront: accepts a nullable sender.
        unsafe {
            send_void_id(
                self.raw.as_ptr(),
                c"makeKeyAndOrderFront:",
                std::ptr::null_mut(),
            )
        }
        self.closed = false;
    }

    pub fn set_title(&self, title: &str) -> Result<(), Error> {
        let title = CString::new(title).map_err(|_| Error::InvalidTitle)?;
        let _pool = Pool::new()?;
        // SAFETY: An autoreleased NSString is created within the pool.
        unsafe { set_title(self.raw.as_ptr(), &title) }
    }

    pub fn title(&self) -> String {
        let _pool = Pool::new().expect("AppKit autorelease pool unavailable");
        // SAFETY: title is a live NSString while this retained window exists;
        // UTF8String remains valid until copied below.
        unsafe {
            let title = send_id(self.raw.as_ptr(), c"title");
            let bytes = send_cstr(title, c"UTF8String");
            if bytes.is_null() {
                String::new()
            } else {
                CStr::from_ptr(bytes).to_string_lossy().into_owned()
            }
        }
    }

    pub fn is_visible(&self) -> bool {
        // SAFETY: isVisible returns an Objective-C BOOL.
        unsafe { send_bool(self.raw.as_ptr(), c"isVisible") != 0 }
    }

    /// The content-view extent in logical points.
    pub fn content_size(&self) -> Size {
        // SAFETY: NSWindow contentView returns a live NSView owned by the window;
        // NSView frame returns NSRect with architecture-specific struct ABI.
        unsafe { read_content_size(self.raw.as_ptr()) }
    }

    pub fn set_content_size(&mut self, size: Size) -> Result<(), Error> {
        if !size.width.is_finite()
            || !size.height.is_finite()
            || size.width <= 0.0
            || size.height <= 0.0
        {
            return Err(Error::InvalidSize);
        }
        // SAFETY: setContentSize: takes an NSSize by value in logical points.
        unsafe {
            send_void_size(
                self.raw.as_ptr(),
                c"setContentSize:",
                NSSize {
                    width: size.width,
                    height: size.height,
                },
            )
        }
        Ok(())
    }

    pub fn backing_scale_factor(&self) -> f64 {
        // SAFETY: backingScaleFactor returns CGFloat (double on supported Macs).
        unsafe { send_f64(self.raw.as_ptr(), c"backingScaleFactor") }
    }

    pub fn close(&mut self) {
        if !self.closed {
            // SAFETY: setReleasedWhenClosed:NO retains ownership after close.
            unsafe { send_void(self.raw.as_ptr(), c"close") }
            self.closed = true;
        }
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        self.close();
        // SAFETY: NSWindow.delegate is weak. Detach it before releasing the
        // +1 delegate, then unregister its event queue and release the window.
        unsafe {
            for (_, element) in self.accessibility.get_mut().elements.drain() {
                accessibility_bindings()
                    .lock()
                    .unwrap()
                    .remove(&(element.as_ptr() as usize));
                send_void(element.as_ptr(), c"release");
            }
            send_void_id(self.raw.as_ptr(), c"setDelegate:", std::ptr::null_mut());
            window_queues()
                .lock()
                .unwrap()
                .remove(&(self.raw.as_ptr() as usize));
            paint_states()
                .lock()
                .unwrap()
                .remove(&(self.raw.as_ptr() as usize));
            ime_states()
                .lock()
                .unwrap()
                .remove(&(self.raw.as_ptr() as usize));
            send_void(self.delegate.as_ptr(), c"release");
            send_void(self.raw.as_ptr(), c"release");
        }
    }
}

unsafe fn read_content_size(window: Id) -> Size {
    // SAFETY: NSWindow owns its contentView. On a missing view, return empty
    // geometry rather than sending frame to nil.
    unsafe {
        let view = send_id(window, c"contentView");
        if view.is_null() {
            return Size::new(0.0, 0.0);
        }
        let frame = send_rect(view, c"frame");
        Size::new(frame.size.width, frame.size.height)
    }
}

unsafe fn set_title(window: Id, title: &CString) -> Result<(), Error> {
    // SAFETY: NSString copies valid UTF-8; setTitle: retains/copies the value.
    unsafe {
        let cls = class(c"NSString")?;
        let ns_title = send_id_cstr(cls, c"stringWithUTF8String:", title.as_ptr());
        if ns_title.is_null() {
            return Err(Error::InvalidTitle);
        }
        send_void_id(window, c"setTitle:", ns_title);
    }
    Ok(())
}

fn ns_string(value: &str) -> Result<Id, Error> {
    let value = CString::new(value).map_err(|_| Error::InvalidTitle)?;
    unsafe {
        let result = send_id_cstr(
            class(c"NSString")?,
            c"stringWithUTF8String:",
            value.as_ptr(),
        );
        NonNull::new(result)
            .map(NonNull::as_ptr)
            .ok_or(Error::AppKitUnavailable)
    }
}

fn ns_array(values: &[Id]) -> Result<Id, Error> {
    unsafe {
        let result = send_id_ptr_usize(
            class(c"NSArray")?,
            c"arrayWithObjects:count:",
            values.as_ptr(),
            values.len(),
        );
        NonNull::new(result)
            .map(NonNull::as_ptr)
            .ok_or(Error::AppKitUnavailable)
    }
}

unsafe fn native_accessibility_role(role: SemanticRole) -> Id {
    unsafe {
        match role {
            SemanticRole::Window => NSAccessibilityWindowRole,
            SemanticRole::Button => NSAccessibilityButtonRole,
            SemanticRole::Checkbox => NSAccessibilityCheckBoxRole,
            SemanticRole::RadioButton | SemanticRole::Tab => NSAccessibilityRadioButtonRole,
            SemanticRole::RadioGroup | SemanticRole::TabList | SemanticRole::TabPanel => {
                NSAccessibilityGroupRole
            }
            SemanticRole::TextField | SemanticRole::PasswordField => NSAccessibilityTextFieldRole,
            SemanticRole::StaticText => NSAccessibilityStaticTextRole,
            SemanticRole::List => NSAccessibilityListRole,
            SemanticRole::ScrollArea => NSAccessibilityScrollAreaRole,
            SemanticRole::Slider => NSAccessibilitySliderRole,
            SemanticRole::SpinButton => NSAccessibilityIncrementorRole,
            SemanticRole::ProgressIndicator => NSAccessibilityProgressIndicatorRole,
            SemanticRole::Application | SemanticRole::Group | SemanticRole::ListItem => {
                NSAccessibilityGroupRole
            }
        }
    }
}

unsafe fn accessibility_screen_rect(view: Id, window: Id, bounds: choscorui_core::Rect) -> NSRect {
    let local = native_rect(bounds);
    unsafe {
        let window_rect =
            send_rect_rect_id(view, c"convertRect:toView:", local, std::ptr::null_mut());
        send_rect_rect(window, c"convertRectToScreen:", window_rect)
    }
}
unsafe fn class(name: &CStr) -> Result<Id, Error> {
    // SAFETY: objc_getClass reads a static NUL-terminated class name.
    NonNull::new(unsafe { objc_getClass(name.as_ptr()) })
        .map(NonNull::as_ptr)
        .ok_or(Error::AppKitUnavailable)
}
unsafe fn selector(name: &CStr) -> Sel {
    // SAFETY: sel_registerName reads a static NUL-terminated selector.
    unsafe { sel_registerName(name.as_ptr()) }
}

// Each typed cast matches the SDK method signature used at its call site.
unsafe fn send_id(receiver: Id, name: &CStr) -> Id {
    let f: unsafe extern "C" fn(Id, Sel) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name)) }
}
unsafe fn send_id_id(receiver: Id, name: &CStr, value: Id) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, Id) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value) }
}
unsafe fn send_id_cstr(receiver: Id, name: &CStr, value: *const i8) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, *const i8) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value) }
}
unsafe fn send_id_f64(receiver: Id, name: &CStr, value: f64) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, f64) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value) }
}
unsafe fn send_id_bool(receiver: Id, name: &CStr, value: i8) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, i8) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value) }
}
unsafe fn send_id_ptr_usize(receiver: Id, name: &CStr, values: *const Id, count: usize) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, *const Id, usize) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), values, count) }
}
unsafe fn send_accessibility_element(
    receiver: Id,
    name: &CStr,
    role: Id,
    frame: NSRect,
    label: Id,
    parent: Id,
) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, Id, NSRect, Id, Id) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), role, frame, label, parent) }
}
unsafe fn send_next_event(
    receiver: Id,
    name: &CStr,
    mask: usize,
    date: Id,
    mode: Id,
    dequeue: i8,
) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, usize, Id, Id, i8) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), mask, date, mode, dequeue) }
}
unsafe fn send_void(receiver: Id, name: &CStr) {
    let f: unsafe extern "C" fn(Id, Sel) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name)) }
}
unsafe fn send_bool(receiver: Id, name: &CStr) -> i8 {
    let f: unsafe extern "C" fn(Id, Sel) -> i8 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name)) }
}
unsafe fn send_bool_id_id(receiver: Id, name: &CStr, first: Id, second: Id) -> i8 {
    let f: unsafe extern "C" fn(Id, Sel, Id, Id) -> i8 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), first, second) }
}
unsafe fn send_bool_id(receiver: Id, name: &CStr, value: Id) -> i8 {
    let f: unsafe extern "C" fn(Id, Sel, Id) -> i8 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value) }
}
unsafe fn send_cstr(receiver: Id, name: &CStr) -> *const i8 {
    let f: unsafe extern "C" fn(Id, Sel) -> *const i8 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name)) }
}
unsafe fn send_f64(receiver: Id, name: &CStr) -> f64 {
    let f: unsafe extern "C" fn(Id, Sel) -> f64 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name)) }
}
unsafe fn send_usize(receiver: Id, name: &CStr) -> usize {
    let f: unsafe extern "C" fn(Id, Sel) -> usize =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name)) }
}
unsafe fn send_u16(receiver: Id, name: &CStr) -> u16 {
    let f: unsafe extern "C" fn(Id, Sel) -> u16 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name)) }
}
unsafe fn send_point(receiver: Id, name: &CStr) -> NSPoint {
    let f: unsafe extern "C" fn(Id, Sel) -> NSPoint =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name)) }
}
unsafe fn send_point_point_id(receiver: Id, name: &CStr, point: NSPoint, view: Id) -> NSPoint {
    let f: unsafe extern "C" fn(Id, Sel, NSPoint, Id) -> NSPoint =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), point, view) }
}
unsafe fn send_view_init(receiver: Id, name: &CStr, frame: NSRect) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, NSRect) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), frame) }
}
unsafe fn send_void_size(receiver: Id, name: &CStr, size: NSSize) {
    let f: unsafe extern "C" fn(Id, Sel, NSSize) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), size) }
}
unsafe fn send_void_range(receiver: Id, name: &CStr, value: NSRange) {
    let f: unsafe extern "C" fn(Id, Sel, NSRange) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value) }
}
unsafe fn send_void_rect(receiver: Id, name: &CStr, rect: NSRect) {
    let f: unsafe extern "C" fn(Id, Sel, NSRect) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), rect) }
}

unsafe fn send_rect_rect_id(receiver: Id, name: &CStr, rect: NSRect, value: Id) -> NSRect {
    let f: unsafe extern "C" fn(Id, Sel, NSRect, Id) -> NSRect =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), rect, value) }
}

unsafe fn send_rect_rect(receiver: Id, name: &CStr, rect: NSRect) -> NSRect {
    let f: unsafe extern "C" fn(Id, Sel, NSRect) -> NSRect =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), rect) }
}

#[cfg(target_arch = "aarch64")]
unsafe fn send_rect(receiver: Id, name: &CStr) -> NSRect {
    let f: unsafe extern "C" fn(Id, Sel) -> NSRect =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name)) }
}
#[cfg(target_arch = "x86_64")]
unsafe fn send_rect(receiver: Id, name: &CStr) -> NSRect {
    unsafe extern "C" {
        fn objc_msgSend_stret();
    }
    let f: unsafe extern "C" fn(*mut NSRect, Id, Sel) =
        unsafe { std::mem::transmute(objc_msgSend_stret as *const ()) };
    let mut result = std::mem::MaybeUninit::<NSRect>::uninit();
    unsafe {
        f(result.as_mut_ptr(), receiver, selector(name));
        result.assume_init()
    }
}
unsafe fn send_void_id(receiver: Id, name: &CStr, value: Id) {
    let f: unsafe extern "C" fn(Id, Sel, Id) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value) }
}
unsafe fn send_super_void_id(receiver: Id, name: &CStr, value: Id) {
    let mut target = ObjcSuper {
        receiver,
        superclass: unsafe { class_getSuperclass(object_getClass(receiver)) },
    };
    let f: unsafe extern "C" fn(*mut ObjcSuper, Sel, Id) =
        unsafe { std::mem::transmute(objc_msgSendSuper as *const ()) };
    unsafe { f(&mut target, selector(name), value) }
}
unsafe fn send_super_void_bool(receiver: Id, name: &CStr, value: i8) {
    let mut target = ObjcSuper {
        receiver,
        superclass: unsafe { class_getSuperclass(object_getClass(receiver)) },
    };
    let f: unsafe extern "C" fn(*mut ObjcSuper, Sel, i8) =
        unsafe { std::mem::transmute(objc_msgSendSuper as *const ()) };
    unsafe { f(&mut target, selector(name), value) }
}
unsafe fn send_void_bool(receiver: Id, name: &CStr, value: i8) {
    let f: unsafe extern "C" fn(Id, Sel, i8) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value) }
}
unsafe fn send_bool_usize(receiver: Id, name: &CStr, value: usize) -> i8 {
    let f: unsafe extern "C" fn(Id, Sel, usize) -> i8 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value) }
}
unsafe fn send_window_init(
    receiver: Id,
    name: &CStr,
    frame: NSRect,
    style: usize,
    backing: usize,
    defer: i8,
) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, NSRect, usize, usize, i8) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), frame, style, backing, defer) }
}
