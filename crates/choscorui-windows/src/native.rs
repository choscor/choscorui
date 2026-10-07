use crate::{InputTranslator, Modifiers, NativeMessage, NativeRect, WindowEvent};
use choscorui_core::Size;
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::ffi::{c_int, c_void};
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

type Bool = i32;
type Dword = u32;
type Handle = *mut c_void;
type Hinstance = Handle;
type Hwnd = Handle;
type Lparam = isize;
type Lresult = isize;
type Uint = u32;
type Wparam = usize;

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: Uint,
    w_param: Wparam,
    l_param: Lparam,
    time: Dword,
    point: Point,
    private: Dword,
}
#[repr(C)]
struct CreateStructW {
    create_params: *mut c_void,
    instance: Hinstance,
    menu: Handle,
    parent: Hwnd,
    cy: c_int,
    cx: c_int,
    y: c_int,
    x: c_int,
    style: i32,
    name: *const u16,
    class: *const u16,
    extended_style: Dword,
}
#[repr(C)]
struct WndClassExW {
    size: Uint,
    style: Uint,
    procedure: Option<unsafe extern "system" fn(Hwnd, Uint, Wparam, Lparam) -> Lresult>,
    class_extra: c_int,
    window_extra: c_int,
    instance: Hinstance,
    icon: Handle,
    cursor: Handle,
    background: Handle,
    menu_name: *const u16,
    class_name: *const u16,
    small_icon: Handle,
}

const CW_USEDEFAULT: c_int = i32::MIN;
const GWLP_USERDATA: c_int = -21;
const IDC_ARROW: *const u16 = 32512usize as *const u16;
const PM_REMOVE: Uint = 1;
const QS_ALLINPUT: Dword = 0x04ff;
const MWMO_INPUTAVAILABLE: Dword = 0x0004;
const WAIT_TIMEOUT: Dword = 258;
const SW_SHOW: c_int = 5;
const SWP_NOACTIVATE: Uint = 0x0010;
const SWP_NOZORDER: Uint = 0x0004;
const WS_OVERLAPPEDWINDOW: Dword = 0x00cf_0000;
const CW_STYLE: Dword = WS_OVERLAPPEDWINDOW;
const WM_NCCREATE: Uint = 0x0081;
const WM_NCDESTROY: Uint = 0x0082;
const WM_SIZE: Uint = 0x0005;
const WM_DPICHANGED: Uint = 0x02e0;
const WM_SETFOCUS: Uint = 0x0007;
const WM_KILLFOCUS: Uint = 0x0008;
const WM_CLOSE: Uint = 0x0010;
const WM_MOUSEMOVE: Uint = 0x0200;
const WM_LBUTTONDOWN: Uint = 0x0201;
const WM_LBUTTONUP: Uint = 0x0202;
const WM_RBUTTONDOWN: Uint = 0x0204;
const WM_RBUTTONUP: Uint = 0x0205;
const WM_MBUTTONDOWN: Uint = 0x0207;
const WM_MBUTTONUP: Uint = 0x0208;
const WM_MOUSEWHEEL: Uint = 0x020a;
const WM_MOUSEHWHEEL: Uint = 0x020e;
const WM_KEYDOWN: Uint = 0x0100;
const WM_KEYUP: Uint = 0x0101;
const WM_SYSKEYDOWN: Uint = 0x0104;
const WM_SYSKEYUP: Uint = 0x0105;
const WM_CHAR: Uint = 0x0102;
const WM_QUIT: Uint = 0x0012;
const WM_APP_TASK: Uint = 0x8000 + 41;
const VK_SHIFT: c_int = 0x10;
const VK_CONTROL: c_int = 0x11;
const VK_MENU: c_int = 0x12;
const VK_LWIN: c_int = 0x5b;
const VK_RWIN: c_int = 0x5c;
const VK_CAPITAL: c_int = 0x14;

#[link(name = "user32")]
unsafe extern "system" {
    fn AdjustWindowRectExForDpi(
        rect: *mut Rect,
        style: Dword,
        menu: Bool,
        extended: Dword,
        dpi: Uint,
    ) -> Bool;
    fn CreateWindowExW(
        extended: Dword,
        class: *const u16,
        name: *const u16,
        style: Dword,
        x: c_int,
        y: c_int,
        width: c_int,
        height: c_int,
        parent: Hwnd,
        menu: Handle,
        instance: Hinstance,
        param: *mut c_void,
    ) -> Hwnd;
    fn DefWindowProcW(hwnd: Hwnd, message: Uint, w_param: Wparam, l_param: Lparam) -> Lresult;
    fn DestroyWindow(hwnd: Hwnd) -> Bool;
    fn DispatchMessageW(message: *const Msg) -> Lresult;
    fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> Bool;
    fn GetDpiForSystem() -> Uint;
    fn GetDpiForWindow(hwnd: Hwnd) -> Uint;
    fn GetKeyState(key: c_int) -> i16;
    fn GetMessageW(message: *mut Msg, hwnd: Hwnd, min: Uint, max: Uint) -> Bool;
    fn GetWindowLongPtrW(hwnd: Hwnd, index: c_int) -> isize;
    fn LoadCursorW(instance: Hinstance, name: *const u16) -> Handle;
    fn MsgWaitForMultipleObjectsEx(
        count: Dword,
        handles: *const Handle,
        milliseconds: Dword,
        mask: Dword,
        flags: Dword,
    ) -> Dword;
    fn PeekMessageW(message: *mut Msg, hwnd: Hwnd, min: Uint, max: Uint, remove: Uint) -> Bool;
    fn PostMessageW(hwnd: Hwnd, message: Uint, w_param: Wparam, l_param: Lparam) -> Bool;
    fn PostQuitMessage(code: c_int);
    fn RegisterClassExW(class: *const WndClassExW) -> u16;
    fn ScreenToClient(hwnd: Hwnd, point: *mut Point) -> Bool;
    fn SetWindowLongPtrW(hwnd: Hwnd, index: c_int, value: isize) -> isize;
    fn SetWindowPos(
        hwnd: Hwnd,
        after: Hwnd,
        x: c_int,
        y: c_int,
        cx: c_int,
        cy: c_int,
        flags: Uint,
    ) -> Bool;
    fn ShowWindow(hwnd: Hwnd, command: c_int) -> Bool;
    fn TranslateMessage(message: *const Msg) -> Bool;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThreadId() -> Dword;
    fn GetModuleHandleW(name: *const u16) -> Hinstance;
}

type PostedTask = Box<dyn FnOnce() + Send + 'static>;
static NEXT_TASK: AtomicUsize = AtomicUsize::new(1);
static CLASS_RESULT: OnceLock<Result<(), Error>> = OnceLock::new();
const CLASS_NAME: &[u16] = &[
    67, 104, 111, 115, 99, 111, 114, 117, 105, 87, 105, 110, 100, 111, 119, 0,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Win32Unavailable,
    InvalidSize,
    InvalidTitle,
    Stopped,
}

#[derive(Clone)]
pub struct Poster {
    inner: Arc<PostInner>,
}

struct PostInner {
    state: Mutex<PostState>,
}

struct PostState {
    dispatcher: usize,
    tasks: HashMap<usize, PostedTask>,
}

impl Poster {
    pub fn post(&self, task: impl FnOnce() + Send + 'static) -> Result<(), Error> {
        let token = NEXT_TASK.fetch_add(1, Ordering::Relaxed);
        let mut state = self.inner.state.lock().unwrap();
        state.tasks.insert(token, Box::new(task));
        let dispatcher = state.dispatcher;
        if dispatcher == 0
            // SAFETY: a nonzero value is the hidden dispatcher HWND. Drop
            // clears it before destroying that window.
            || unsafe { PostMessageW(dispatcher as Hwnd, WM_APP_TASK, token, 0) } == 0
        {
            state.tasks.remove(&token);
            return Err(Error::Stopped);
        }
        Ok(())
    }
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

struct WindowState {
    events: Arc<Mutex<VecDeque<WindowEvent>>>,
    input: Mutex<InputTranslator>,
    panics: Arc<AtomicUsize>,
    post: Option<Arc<PostInner>>,
}

/// UI-thread Win32 process handle. Its marker prevents transfer to workers.
pub struct Application {
    thread: Dword,
    dispatcher: NonNull<c_void>,
    panics: Arc<AtomicUsize>,
    post: Arc<PostInner>,
    _ui: PhantomData<Rc<()>>,
}

impl Application {
    pub fn new() -> Result<Self, Error> {
        register_class()?;
        let panics = Arc::new(AtomicUsize::new(0));
        let post = Arc::new(PostInner {
            state: Mutex::new(PostState {
                dispatcher: 0,
                tasks: HashMap::new(),
            }),
        });
        let state = Arc::new(WindowState {
            events: Arc::new(Mutex::new(VecDeque::new())),
            input: Mutex::new(InputTranslator::new(96).map_err(|_| Error::Win32Unavailable)?),
            panics: Arc::clone(&panics),
            post: Some(Arc::clone(&post)),
        });
        let raw = Arc::as_ptr(&state) as *mut c_void;
        // SAFETY: registered class and creation payload match window_proc.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                CLASS_NAME.as_ptr(),
                CLASS_NAME.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                module(),
                raw,
            )
        };
        let Some(dispatcher) = NonNull::new(hwnd) else {
            return Err(Error::Win32Unavailable);
        };
        post.state.lock().unwrap().dispatcher = dispatcher.as_ptr() as usize;
        Ok(Self {
            thread: unsafe { GetCurrentThreadId() },
            dispatcher,
            panics,
            post,
            _ui: PhantomData,
        })
    }

    pub fn create_window(&self, options: WindowOptions) -> Result<Window, Error> {
        if !options.width.is_finite()
            || !options.height.is_finite()
            || options.width <= 0.0
            || options.height <= 0.0
        {
            return Err(Error::InvalidSize);
        }
        let title = wide(&options.title)?;
        let dpi = unsafe { GetDpiForSystem() }.max(96);
        let scale = f64::from(dpi) / 96.0;
        let mut frame = Rect {
            left: 0,
            top: 0,
            right: (options.width * scale).round() as i32,
            bottom: (options.height * scale).round() as i32,
        };
        if unsafe { AdjustWindowRectExForDpi(&mut frame, CW_STYLE, 0, 0, dpi) } == 0 {
            return Err(Error::Win32Unavailable);
        }
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let state = Arc::new(WindowState {
            events: Arc::clone(&events),
            input: Mutex::new(InputTranslator::new(dpi).unwrap()),
            panics: Arc::clone(&self.panics),
            post: None,
        });
        let raw = Arc::as_ptr(&state) as *mut c_void;
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                CLASS_NAME.as_ptr(),
                title.as_ptr(),
                CW_STYLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                frame.right - frame.left,
                frame.bottom - frame.top,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                module(),
                raw,
            )
        };
        let Some(raw_window) = NonNull::new(hwnd) else {
            return Err(Error::Win32Unavailable);
        };
        Ok(Window {
            raw: Cell::new(raw_window.as_ptr() as usize),
            events,
            _ui: PhantomData,
        })
    }

    pub fn run(&self) {
        let mut message = empty_message();
        while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
            dispatch(&message);
        }
    }
    pub fn stop(&self) {
        unsafe { PostQuitMessage(0) }
    }
    pub fn poster(&self) -> Poster {
        Poster {
            inner: Arc::clone(&self.post),
        }
    }
    pub fn panic_count(&self) -> usize {
        self.panics.load(Ordering::Relaxed)
    }

    pub fn dispatch_next_event(&self, timeout: Duration) -> bool {
        let millis = timeout.as_millis().min(u128::from(u32::MAX - 1)) as u32;
        let ready = unsafe {
            MsgWaitForMultipleObjectsEx(
                0,
                std::ptr::null(),
                millis,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            )
        };
        if ready == WAIT_TIMEOUT {
            return false;
        }
        let mut message = empty_message();
        if unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } == 0 {
            return false;
        }
        if message.message == WM_QUIT {
            return false;
        }
        dispatch(&message);
        true
    }
    pub fn pump_once(&self, timeout: Duration) {
        let _ = self.dispatch_next_event(timeout);
    }
}

impl Drop for Application {
    fn drop(&mut self) {
        let mut post = self.post.state.lock().unwrap();
        post.dispatcher = 0;
        post.tasks.clear();
        drop(post);
        unsafe { DestroyWindow(self.dispatcher.as_ptr()) };
        debug_assert_eq!(self.thread, unsafe { GetCurrentThreadId() });
    }
}

/// Retained HWND, destroyed exactly once on its creating UI thread.
pub struct Window {
    raw: Cell<usize>,
    events: Arc<Mutex<VecDeque<WindowEvent>>>,
    _ui: PhantomData<Rc<()>>,
}
impl Window {
    pub fn show(&self) {
        let raw = self.raw.get();
        if raw != 0 {
            unsafe { ShowWindow(raw as Hwnd, SW_SHOW) };
        }
    }
    pub fn take_events(&self) -> Vec<WindowEvent> {
        self.events.lock().unwrap().drain(..).collect()
    }
    pub fn content_size(&self) -> Size {
        let raw = self.raw.get();
        if raw == 0 {
            return Size::new(0.0, 0.0);
        }
        let mut rect = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe {
            GetClientRect(raw as Hwnd, &mut rect);
        }
        let dpi = unsafe { GetDpiForWindow(raw as Hwnd) }.max(96);
        let scale = f64::from(dpi) / 96.0;
        Size::new(
            f64::from(rect.right) / scale,
            f64::from(rect.bottom) / scale,
        )
    }
    pub fn backing_scale_factor(&self) -> f64 {
        let raw = self.raw.get();
        if raw == 0 {
            return 1.0;
        }
        f64::from(unsafe { GetDpiForWindow(raw as Hwnd) }.max(96)) / 96.0
    }
    pub fn close(&self) {
        let raw = self.raw.replace(0);
        if raw != 0 {
            unsafe { DestroyWindow(raw as Hwnd) };
        }
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        self.close();
    }
}

fn register_class() -> Result<(), Error> {
    *CLASS_RESULT.get_or_init(|| {
        let class = WndClassExW {
            size: std::mem::size_of::<WndClassExW>() as u32,
            style: 0,
            procedure: Some(window_proc),
            class_extra: 0,
            window_extra: 0,
            instance: module(),
            icon: std::ptr::null_mut(),
            cursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
            background: std::ptr::null_mut(),
            menu_name: std::ptr::null(),
            class_name: CLASS_NAME.as_ptr(),
            small_icon: std::ptr::null_mut(),
        };
        if unsafe { RegisterClassExW(&class) } == 0 {
            Err(Error::Win32Unavailable)
        } else {
            Ok(())
        }
    })
}

unsafe extern "system" fn window_proc(
    hwnd: Hwnd,
    message: Uint,
    w_param: Wparam,
    l_param: Lparam,
) -> Lresult {
    if message == WM_NCCREATE {
        let create = unsafe { &*(l_param as *const CreateStructW) };
        // The caller keeps its Arc alive for the duration of CreateWindowExW.
        // Add the HWND-owned reference before storing the pointer; it is
        // released exactly once by WM_NCDESTROY.
        unsafe {
            Arc::increment_strong_count(create.create_params.cast::<WindowState>());
        }
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.create_params as isize);
        }
        return 1;
    }
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState };
    if state.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, w_param, l_param) };
    }
    if message == WM_NCDESTROY {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Arc::from_raw(state));
        }
        return unsafe { DefWindowProcW(hwnd, message, w_param, l_param) };
    }
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        handle_message(hwnd, &*state, message, w_param, l_param)
    }));
    match result {
        Ok(Some(value)) => value,
        Ok(None) => unsafe { DefWindowProcW(hwnd, message, w_param, l_param) },
        Err(_) => {
            unsafe { &*state }.panics.fetch_add(1, Ordering::Relaxed);
            0
        }
    }
}

unsafe fn handle_message(
    hwnd: Hwnd,
    state: &WindowState,
    message: Uint,
    w_param: Wparam,
    l_param: Lparam,
) -> Option<Lresult> {
    if message == WM_APP_TASK {
        if let Some(post) = &state.post {
            let task = post.state.lock().unwrap().tasks.remove(&w_param);
            if let Some(task) = task {
                task();
            }
        }
        return Some(0);
    }
    let native = match message {
        WM_SIZE => NativeMessage::Resized {
            width: low_word(l_param as usize).into(),
            height: high_word(l_param as usize).into(),
        },
        WM_DPICHANGED => {
            let rect = unsafe { &*(l_param as *const Rect) };
            unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
            };
            let mut client = Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            unsafe { GetClientRect(hwnd, &mut client) };
            NativeMessage::DpiChanged {
                dpi: u32::from(low_word(w_param)),
                suggested_frame: NativeRect {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                },
                client_width: client.right as u32,
                client_height: client.bottom as u32,
            }
        }
        WM_SETFOCUS => NativeMessage::Focused(true),
        WM_KILLFOCUS => NativeMessage::Focused(false),
        WM_CLOSE => NativeMessage::CloseRequested,
        WM_MOUSEMOVE => pointer_move(l_param),
        WM_LBUTTONDOWN => pointer_button(l_param, 1, true),
        WM_LBUTTONUP => pointer_button(l_param, 1, false),
        WM_RBUTTONDOWN => pointer_button(l_param, 2, true),
        WM_RBUTTONUP => pointer_button(l_param, 2, false),
        WM_MBUTTONDOWN => pointer_button(l_param, 3, true),
        WM_MBUTTONUP => pointer_button(l_param, 3, false),
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let mut point = Point {
                x: signed_low(l_param),
                y: signed_high(l_param),
            };
            unsafe { ScreenToClient(hwnd, &mut point) };
            let delta = f64::from(signed_high(w_param as isize));
            NativeMessage::Wheel {
                x: point.x,
                y: point.y,
                delta_x: if message == WM_MOUSEHWHEEL {
                    delta
                } else {
                    0.0
                },
                delta_y: if message == WM_MOUSEWHEEL {
                    -delta
                } else {
                    0.0
                },
                precise: false,
                modifiers: modifiers(),
            }
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => NativeMessage::KeyDown {
            virtual_key: w_param as u32,
            repeat: (l_param & (1 << 30)) != 0,
            modifiers: modifiers(),
        },
        WM_KEYUP | WM_SYSKEYUP => NativeMessage::KeyUp {
            virtual_key: w_param as u32,
            modifiers: modifiers(),
        },
        WM_CHAR => NativeMessage::Character(w_param as u16),
        _ => return None,
    };
    state
        .events
        .lock()
        .unwrap()
        .extend(state.input.lock().unwrap().translate(native));
    Some(0)
}

fn pointer_move(value: Lparam) -> NativeMessage {
    NativeMessage::PointerMoved {
        x: signed_low(value),
        y: signed_high(value),
        modifiers: modifiers(),
    }
}
fn pointer_button(value: Lparam, button: usize, down: bool) -> NativeMessage {
    let (x, y, modifiers) = (signed_low(value), signed_high(value), modifiers());
    if down {
        NativeMessage::PointerDown {
            x,
            y,
            button,
            modifiers,
        }
    } else {
        NativeMessage::PointerUp {
            x,
            y,
            button,
            modifiers,
        }
    }
}
fn modifiers() -> Modifiers {
    unsafe {
        Modifiers {
            shift: GetKeyState(VK_SHIFT) < 0,
            control: GetKeyState(VK_CONTROL) < 0,
            alt: GetKeyState(VK_MENU) < 0,
            system: GetKeyState(VK_LWIN) < 0 || GetKeyState(VK_RWIN) < 0,
            caps_lock: GetKeyState(VK_CAPITAL) & 1 != 0,
        }
    }
}
fn low_word(value: usize) -> u16 {
    value as u16
}
fn high_word(value: usize) -> u16 {
    (value >> 16) as u16
}
fn signed_low(value: isize) -> i32 {
    i32::from(value as i16)
}
fn signed_high(value: isize) -> i32 {
    i32::from((value >> 16) as i16)
}
fn module() -> Hinstance {
    unsafe { GetModuleHandleW(std::ptr::null()) }
}
fn empty_message() -> Msg {
    Msg {
        hwnd: std::ptr::null_mut(),
        message: 0,
        w_param: 0,
        l_param: 0,
        time: 0,
        point: Point { x: 0, y: 0 },
        private: 0,
    }
}
fn dispatch(message: &Msg) {
    unsafe {
        TranslateMessage(message);
        DispatchMessageW(message);
    }
}
fn wide(value: &str) -> Result<Vec<u16>, Error> {
    if value.encode_utf16().any(|unit| unit == 0) {
        return Err(Error::InvalidTitle);
    }
    Ok(value.encode_utf16().chain(Some(0)).collect())
}
