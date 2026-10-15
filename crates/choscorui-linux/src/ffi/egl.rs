//! A safe interface to libEGL and libwayland-egl, for the GLES painter.
//!
//! - `EglDisplay` opens an EGL display on a Wayland connection, or on no
//!   window system with `EGL_MESA_platform_surfaceless`. It terminates
//!   the display when the last context and surface are gone.
//! - `EglContext` is an OpenGL ES 3.0 context and its config.
//! - `EglSurface` is a window surface on a `wl_egl_window` of a
//!   `wl_surface`. It holds the surface proxy, so the proxy outlives it.
//!
//! Each object holds what it uses, so the drop order is always the order
//! that EGL needs: surfaces and contexts before the display, and the
//! display before the Wayland connection.

use super::ctypes::{CChar, CInt, c_void};
use super::dl::LoadError;
use super::macros::{c_consts, libraries, lookup_functions, opaque_types};
use super::wayland::{Connection, DisplayRef, Proxy};
use std::ffi::CStr;
use std::rc::Rc;

opaque_types! {
    /// `struct wl_surface`.
    pub(crate) WlSurfaceRaw = "struct wl_surface";
    /// `struct wl_egl_window`.
    pub(crate) WlEglWindowRaw = "struct wl_egl_window";
}

/// `EGLBoolean`.
type EglBoolean = u32;
/// `EGLint`.
type EglInt = i32;
/// `EGLenum`.
type EglEnum = u32;
/// `EGLAttrib`.
type EglAttrib = isize;
/// `EGLDisplay`, `EGLConfig`, `EGLContext`, and `EGLSurface`.
type Handle = *mut c_void;

/// `__eglMustCastToProperFunctionPointerType`: what `eglGetProcAddress`
/// returns.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub(crate) struct EglProc(Option<unsafe extern "C" fn()>);

#[cfg(test)]
impl super::ctype::CType for EglProc {
    fn spelling() -> String {
        "__eglMustCastToProperFunctionPointerType".to_owned()
    }
}

libraries! {
    /// libEGL: the window-system binding of OpenGL ES.
    pub(crate) struct Egl {
        library: "libEGL",
        sonames: ["libEGL.so.1"],
        headers: ["EGL/egl.h", "EGL/eglext.h"],
        pkg_config: "egl",
        package: "libegl-dev",
        functions {
            fn eglGetError() -> EglInt;
            fn eglGetProcAddress(procname: *const CChar) -> EglProc;
            fn eglQueryString(dpy: Handle, name: EglInt) -> *const CChar;
            fn eglGetPlatformDisplay(
                platform: EglEnum,
                native_display: *mut c_void,
                attrib_list: *const EglAttrib,
            ) -> Handle;
            fn eglInitialize(dpy: Handle, major: *mut EglInt, minor: *mut EglInt) -> EglBoolean;
            fn eglTerminate(dpy: Handle) -> EglBoolean;
            fn eglBindAPI(api: EglEnum) -> EglBoolean;
            fn eglChooseConfig(
                dpy: Handle,
                attrib_list: *const EglInt,
                configs: *mut Handle,
                config_size: EglInt,
                num_config: *mut EglInt,
            ) -> EglBoolean;
            fn eglCreateContext(
                dpy: Handle,
                config: Handle,
                share_context: Handle,
                attrib_list: *const EglInt,
            ) -> Handle;
            fn eglDestroyContext(dpy: Handle, ctx: Handle) -> EglBoolean;
            fn eglCreatePlatformWindowSurface(
                dpy: Handle,
                config: Handle,
                native_window: *mut c_void,
                attrib_list: *const EglAttrib,
            ) -> Handle;
            fn eglDestroySurface(dpy: Handle, surface: Handle) -> EglBoolean;
            fn eglMakeCurrent(dpy: Handle, draw: Handle, read: Handle, ctx: Handle) -> EglBoolean;
            fn eglSwapBuffers(dpy: Handle, surface: Handle) -> EglBoolean;
            fn eglSwapInterval(dpy: Handle, interval: EglInt) -> EglBoolean;
            fn eglQuerySurface(
                dpy: Handle,
                surface: Handle,
                attribute: EglInt,
                value: *mut EglInt,
            ) -> EglBoolean;
        }
    }

    /// libwayland-egl: the native window of EGL on a `wl_surface`.
    pub(crate) struct WaylandEgl {
        library: "libwayland-egl",
        sonames: ["libwayland-egl.so.1"],
        headers: ["wayland-egl.h"],
        pkg_config: "wayland-egl",
        package: "libwayland-dev",
        functions {
            fn wl_egl_window_create(
                surface: *mut WlSurfaceRaw,
                width: CInt,
                height: CInt,
            ) -> *mut WlEglWindowRaw;
            fn wl_egl_window_destroy(egl_window: *mut WlEglWindowRaw);
            fn wl_egl_window_resize(
                egl_window: *mut WlEglWindowRaw,
                width: CInt,
                height: CInt,
                dx: CInt,
                dy: CInt,
            );
        }
    }
}

lookup_functions! {
    /// The swap extensions, from `eglGetProcAddress`.
    pub(crate) struct EglExtensions in Egl {
        fn eglSwapBuffersWithDamageKHR(
            dpy: Handle,
            surface: Handle,
            rects: *const EglInt,
            n_rects: EglInt,
        ) -> EglBoolean = "PFNEGLSWAPBUFFERSWITHDAMAGEKHRPROC";
        fn eglSwapBuffersWithDamageEXT(
            dpy: Handle,
            surface: Handle,
            rects: *const EglInt,
            n_rects: EglInt,
        ) -> EglBoolean = "PFNEGLSWAPBUFFERSWITHDAMAGEEXTPROC";
    }
}

c_consts! {
    /// The EGL constants, for the binding check.
    pub(crate) fn constants;
    const EGL_FALSE: EglBoolean = 0;
    const EGL_SUCCESS: EglInt = 0x3000;
    const EGL_CONTEXT_LOST: EglInt = 0x300E;
    const EGL_ALPHA_SIZE: EglInt = 0x3021;
    const EGL_BLUE_SIZE: EglInt = 0x3022;
    const EGL_GREEN_SIZE: EglInt = 0x3023;
    const EGL_RED_SIZE: EglInt = 0x3024;
    const EGL_SURFACE_TYPE: EglInt = 0x3033;
    const EGL_NONE: EglInt = 0x3038;
    const EGL_RENDERABLE_TYPE: EglInt = 0x3040;
    const EGL_EXTENSIONS: EglInt = 0x3055;
    const EGL_CONTEXT_MAJOR_VERSION: EglInt = 0x3098;
    const EGL_CONTEXT_MINOR_VERSION: EglInt = 0x30FB;
    const EGL_OPENGL_ES_API: EglEnum = 0x30A0;
    const EGL_PBUFFER_BIT: EglInt = 0x0001;
    const EGL_WINDOW_BIT: EglInt = 0x0004;
    const EGL_OPENGL_ES3_BIT: EglInt = 0x0040;
    const EGL_BUFFER_AGE_EXT: EglInt = 0x313D;
    const EGL_PLATFORM_WAYLAND_KHR: EglEnum = 0x31D8;
    const EGL_PLATFORM_SURFACELESS_MESA: EglEnum = 0x31DD;
}

/// `EGL_NO_DISPLAY`, `EGL_NO_CONTEXT`, and `EGL_NO_SURFACE`.
const NONE: Handle = std::ptr::null_mut();

/// Why an EGL call failed. The codes are `eglGetError` values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EglFailure {
    /// libEGL or libwayland-egl did not load.
    Library(LoadError),
    /// The platform is not available, or `eglInitialize` failed.
    Display(i32),
    /// No config has 8-bit RGBA and OpenGL ES 3.0.
    NoConfig,
    /// The context did not open.
    Context(i32),
    /// The window surface did not open.
    Surface(i32),
    /// `eglMakeCurrent` failed.
    MakeCurrent(i32),
    /// The swap failed.
    Swap(i32),
}

impl EglFailure {
    /// True when the GPU context is gone, so painting cannot continue.
    pub(crate) fn context_lost(&self) -> bool {
        matches!(self, Self::MakeCurrent(code) | Self::Swap(code) if *code == EGL_CONTEXT_LOST)
    }
}

impl std::fmt::Display for EglFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Library(error) => write!(f, "{error}"),
            Self::Display(code) => write!(f, "the EGL display did not open (0x{code:04X})"),
            Self::NoConfig => f.write_str("no EGL config has RGBA8 and OpenGL ES 3.0"),
            Self::Context(code) => write!(f, "the OpenGL ES context did not open (0x{code:04X})"),
            Self::Surface(code) => write!(f, "the EGL window surface did not open (0x{code:04X})"),
            Self::MakeCurrent(code) => write!(f, "eglMakeCurrent failed (0x{code:04X})"),
            Self::Swap(code) => write!(f, "eglSwapBuffers failed (0x{code:04X})"),
        }
    }
}

/// An initialized display. Dropping it releases the current context and
/// terminates the display.
struct DisplayInner {
    lib: &'static Egl,
    raw: Handle,
    extensions: String,
    swap_with_damage:
        Option<unsafe extern "C" fn(Handle, Handle, *const EglInt, EglInt) -> EglBoolean>,
    /// The Wayland connection, which must outlive the display.
    _native: Option<DisplayRef>,
}

impl DisplayInner {
    fn error(&self) -> i32 {
        // SAFETY: `eglGetError` reads the thread's last error.
        unsafe { (self.lib.eglGetError)() }
    }
}

impl Drop for DisplayInner {
    fn drop(&mut self) {
        // SAFETY: the display is initialized; releasing a context that is
        // not current is allowed.
        unsafe { (self.lib.eglMakeCurrent)(self.raw, NONE, NONE, NONE) };
        // SAFETY: every context and surface of the display is gone, since
        // each holds an `Rc` of it.
        unsafe { (self.lib.eglTerminate)(self.raw) };
    }
}

/// The extension names in a space-separated list.
fn has_name(list: &str, name: &str) -> bool {
    list.split_ascii_whitespace().any(|item| item == name)
}

/// The string of `name` for `display`, or empty.
fn query_string(lib: &Egl, display: Handle, name: EglInt) -> String {
    // SAFETY: `display` is null (for client extensions) or initialized.
    let pointer = unsafe { (lib.eglQueryString)(display, name) };
    if pointer.is_null() {
        return String::new();
    }
    // SAFETY: EGL returns a NUL-terminated string that lives as long as
    // the display.
    unsafe { CStr::from_ptr(pointer.cast()) }
        .to_string_lossy()
        .into_owned()
}

/// An initialized EGL display.
pub(crate) struct EglDisplay(Rc<DisplayInner>);

impl EglDisplay {
    /// Opens the EGL display of a Wayland connection.
    pub(crate) fn wayland(connection: &Connection) -> Result<Self, EglFailure> {
        let native = connection.display_ref();
        let pointer = native.raw().cast::<c_void>();
        Self::open(EGL_PLATFORM_WAYLAND_KHR, pointer, Some(native))
    }

    /// Opens an EGL display without a window system, for offscreen
    /// painting. It needs `EGL_MESA_platform_surfaceless`.
    pub(crate) fn surfaceless() -> Result<Self, EglFailure> {
        let lib = Egl::get().map_err(EglFailure::Library)?;
        if !has_name(
            &query_string(lib, NONE, EGL_EXTENSIONS),
            "EGL_MESA_platform_surfaceless",
        ) {
            return Err(EglFailure::Display(0));
        }
        Self::open(EGL_PLATFORM_SURFACELESS_MESA, NONE, None)
    }

    fn open(
        platform: EglEnum,
        native: *mut c_void,
        keep: Option<DisplayRef>,
    ) -> Result<Self, EglFailure> {
        let lib = Egl::get().map_err(EglFailure::Library)?;
        // SAFETY: `native` is null or the live `wl_display` that `keep`
        // holds open; the attribute list is null.
        let raw = unsafe { (lib.eglGetPlatformDisplay)(platform, native, std::ptr::null()) };
        if raw.is_null() {
            // SAFETY: as in `DisplayInner::error`.
            return Err(EglFailure::Display(unsafe { (lib.eglGetError)() }));
        }
        let (mut major, mut minor) = (0, 0);
        // SAFETY: `raw` is a display of EGL; the out pointers are locals.
        if unsafe { (lib.eglInitialize)(raw, &raw mut major, &raw mut minor) } == EGL_FALSE {
            // SAFETY: as in `DisplayInner::error`.
            return Err(EglFailure::Display(unsafe { (lib.eglGetError)() }));
        }
        let extensions = query_string(lib, raw, EGL_EXTENSIONS);
        let lookup = |name: &'static str| {
            let Ok(name) = CStr::from_bytes_with_nul(name.as_bytes()) else {
                return NONE;
            };
            // SAFETY: `name` is NUL-terminated.
            let address = unsafe { (lib.eglGetProcAddress)(name.as_ptr().cast()) };
            address.0.map_or(NONE, |function| function as *mut c_void)
        };
        // SAFETY: `eglGetProcAddress` returns the function of each name.
        let table = unsafe { EglExtensions::load(lookup) };
        let swap_with_damage = if has_name(&extensions, "EGL_KHR_swap_buffers_with_damage") {
            table.eglSwapBuffersWithDamageKHR
        } else if has_name(&extensions, "EGL_EXT_swap_buffers_with_damage") {
            table.eglSwapBuffersWithDamageEXT
        } else {
            None
        };
        Ok(Self(Rc::new(DisplayInner {
            lib,
            raw,
            extensions,
            swap_with_damage,
            _native: keep,
        })))
    }

    /// True when the display has the extension `name`.
    pub(crate) fn has_extension(&self, name: &str) -> bool {
        has_name(&self.0.extensions, name)
    }

    /// Opens an OpenGL ES 3.0 context with an RGBA8 config. `window`
    /// asks for a config that window surfaces can use.
    pub(crate) fn context(&self, window: bool) -> Result<EglContext, EglFailure> {
        let display = &self.0;
        let lib = display.lib;
        // SAFETY: binding the API sets thread state only.
        if unsafe { (lib.eglBindAPI)(EGL_OPENGL_ES_API) } == EGL_FALSE {
            return Err(EglFailure::Context(display.error()));
        }
        let surface_type = if window {
            EGL_WINDOW_BIT
        } else {
            EGL_PBUFFER_BIT
        };
        let attributes = [
            EGL_SURFACE_TYPE,
            surface_type,
            EGL_RENDERABLE_TYPE,
            EGL_OPENGL_ES3_BIT,
            EGL_RED_SIZE,
            8,
            EGL_GREEN_SIZE,
            8,
            EGL_BLUE_SIZE,
            8,
            EGL_ALPHA_SIZE,
            8,
            EGL_NONE,
        ];
        let mut config = NONE;
        let mut count = 0;
        // SAFETY: the attribute list ends with EGL_NONE; room for one
        // config and the count are locals.
        let chosen = unsafe {
            (lib.eglChooseConfig)(
                display.raw,
                attributes.as_ptr(),
                &raw mut config,
                1,
                &raw mut count,
            )
        };
        if chosen == EGL_FALSE || count < 1 || config.is_null() {
            return Err(EglFailure::NoConfig);
        }
        let attributes = [
            EGL_CONTEXT_MAJOR_VERSION,
            3,
            EGL_CONTEXT_MINOR_VERSION,
            0,
            EGL_NONE,
        ];
        // SAFETY: `config` came from this display; the list ends with
        // EGL_NONE.
        let raw = unsafe { (lib.eglCreateContext)(display.raw, config, NONE, attributes.as_ptr()) };
        if raw.is_null() {
            return Err(EglFailure::Context(display.error()));
        }
        Ok(EglContext {
            display: Rc::clone(display),
            config,
            raw,
        })
    }
}

/// An OpenGL ES 3.0 context.
pub(crate) struct EglContext {
    display: Rc<DisplayInner>,
    config: Handle,
    raw: Handle,
}

impl Drop for EglContext {
    fn drop(&mut self) {
        let display = &self.display;
        // SAFETY: the context belongs to the display; a current context is
        // released first, so the call never leaves one current.
        unsafe { (display.lib.eglMakeCurrent)(display.raw, NONE, NONE, NONE) };
        // SAFETY: as above.
        unsafe { (display.lib.eglDestroyContext)(display.raw, self.raw) };
    }
}

impl EglContext {
    /// Makes the context current on this thread with `surface`, or with
    /// no surface (`EGL_KHR_surfaceless_context`).
    pub(crate) fn make_current(&self, surface: Option<&EglSurface>) -> Result<(), EglFailure> {
        let display = &self.display;
        let draw = surface.map_or(NONE, |surface| surface.raw);
        // SAFETY: the context and the surface belong to the display and
        // live during the call.
        let made = unsafe { (display.lib.eglMakeCurrent)(display.raw, draw, draw, self.raw) };
        if made == EGL_FALSE {
            Err(EglFailure::MakeCurrent(display.error()))
        } else {
            Ok(())
        }
    }

    /// Sets the swap interval of the current surface. 0 makes a swap
    /// return without waiting for the compositor.
    pub(crate) fn set_swap_interval(&self, interval: i32) {
        // SAFETY: the call applies to the current surface of the thread.
        unsafe { (self.display.lib.eglSwapInterval)(self.display.raw, interval) };
    }
}

/// A `wl_egl_window`. It holds its `wl_surface`.
struct WlEglWindow {
    lib: &'static WaylandEgl,
    raw: *mut WlEglWindowRaw,
    _surface: Rc<Proxy>,
}

impl Drop for WlEglWindow {
    fn drop(&mut self) {
        // SAFETY: the window came from `wl_egl_window_create`, and its EGL
        // surface is already destroyed.
        unsafe { (self.lib.wl_egl_window_destroy)(self.raw) }
    }
}

/// An EGL window surface on a `wl_surface`.
pub(crate) struct EglSurface {
    display: Rc<DisplayInner>,
    raw: Handle,
    window: WlEglWindow,
}

impl Drop for EglSurface {
    fn drop(&mut self) {
        // SAFETY: the surface belongs to the display. EGL defers the
        // destruction of a current surface until it is released.
        unsafe { (self.display.lib.eglDestroySurface)(self.display.raw, self.raw) };
    }
}

impl EglSurface {
    /// Opens a window surface of `width` × `height` pixels on `surface`.
    pub(crate) fn wayland(
        context: &EglContext,
        surface: &Rc<Proxy>,
        width: u32,
        height: u32,
    ) -> Result<Self, EglFailure> {
        let lib = WaylandEgl::get().map_err(EglFailure::Library)?;
        let (width, height) = (size(width), size(height));
        // SAFETY: the proxy is a live `wl_surface`; `window` holds it.
        let raw = unsafe {
            (lib.wl_egl_window_create)(surface.raw().cast::<WlSurfaceRaw>(), width, height)
        };
        if raw.is_null() {
            return Err(EglFailure::Surface(0));
        }
        let window = WlEglWindow {
            lib,
            raw,
            _surface: Rc::clone(surface),
        };
        let display = &context.display;
        // SAFETY: the config came from this display, and `window` is a live
        // `wl_egl_window`; the attribute list is null.
        let raw = unsafe {
            (display.lib.eglCreatePlatformWindowSurface)(
                display.raw,
                context.config,
                window.raw.cast::<c_void>(),
                std::ptr::null(),
            )
        };
        if raw.is_null() {
            return Err(EglFailure::Surface(display.error()));
        }
        Ok(Self {
            display: Rc::clone(display),
            raw,
            window,
        })
    }

    /// Resizes the window. The next frame has the new size.
    pub(crate) fn resize(&self, width: u32, height: u32) {
        // SAFETY: the window is live.
        unsafe {
            (self.window.lib.wl_egl_window_resize)(
                self.window.raw,
                size(width),
                size(height),
                0,
                0,
            );
        }
    }

    /// The age of the back buffer with `EGL_EXT_buffer_age`: 0 for
    /// unknown content, or how many frames ago it was the front buffer.
    /// `None` without the extension.
    pub(crate) fn buffer_age(&self) -> Option<u32> {
        let display = &self.display;
        if !has_name(&display.extensions, "EGL_EXT_buffer_age")
            && !has_name(&display.extensions, "EGL_KHR_partial_update")
        {
            return None;
        }
        let mut age = 0;
        // SAFETY: the surface belongs to the display; the out pointer is a
        // local.
        let queried = unsafe {
            (display.lib.eglQuerySurface)(display.raw, self.raw, EGL_BUFFER_AGE_EXT, &raw mut age)
        };
        (queried != EGL_FALSE).then(|| u32::try_from(age).unwrap_or(0))
    }

    /// Shows the back buffer. `damage` lists the changed rectangles as
    /// `[x, y, width, height]` with y up from the bottom edge; it is a
    /// hint for the compositor when the swap extension is present.
    pub(crate) fn swap(&self, damage: &[[i32; 4]]) -> Result<(), EglFailure> {
        let display = &self.display;
        let flat: Vec<EglInt> = damage.iter().flatten().copied().collect();
        let count = EglInt::try_from(damage.len()).unwrap_or(0);
        let swapped = match display.swap_with_damage {
            Some(swap) if count > 0 => {
                // SAFETY: the surface belongs to the display, and `flat`
                // holds four values for each of `count` rectangles.
                unsafe { swap(display.raw, self.raw, flat.as_ptr(), count) }
            }
            // SAFETY: the surface belongs to the display.
            _ => unsafe { (display.lib.eglSwapBuffers)(display.raw, self.raw) },
        };
        if swapped == EGL_FALSE {
            let code = display.error();
            if code != EGL_SUCCESS {
                return Err(EglFailure::Swap(code));
            }
        }
        Ok(())
    }
}

/// A pixel size as a C int, at least 1.
fn size(pixels: u32) -> CInt {
    CInt::try_from(pixels).unwrap_or(CInt::MAX).max(1)
}
