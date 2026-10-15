//! The declarations of libwayland-cursor, and a safe cursor theme: the
//! cursor images when the compositor has no `wp_cursor_shape_v1`.

use super::sys::WlProxy;
use super::{Display, ObjectId, Proxy, WL_BUFFER};
use crate::ffi::ctypes::{CChar, CInt, CUint};
use crate::ffi::macros::{ffi_struct, libraries, opaque_types};
use std::ffi::CString;
use std::rc::Rc;

opaque_types! {
    /// `struct wl_cursor_theme`.
    pub(crate) WlCursorTheme = "struct wl_cursor_theme";
    /// `struct wl_shm`.
    pub(crate) WlShm = "struct wl_shm";
    /// `struct wl_buffer`.
    pub(crate) WlBuffer = "struct wl_buffer";
}

ffi_struct! {
    /// `struct wl_cursor_image`: one frame of a cursor.
    pub(crate) struct WlCursorImage = "struct wl_cursor_image" {
        pub(crate) width: u32,
        pub(crate) height: u32,
        pub(crate) hotspot_x: u32,
        pub(crate) hotspot_y: u32,
        pub(crate) delay: u32,
    }

    /// `struct wl_cursor`: the frames of one cursor name.
    pub(crate) struct WlCursor = "struct wl_cursor" {
        pub(crate) image_count: CUint,
        pub(crate) images: *mut *mut WlCursorImage,
        pub(crate) name: *mut CChar,
    }
}

libraries! {
    /// libwayland-cursor: the cursor images of an Xcursor theme.
    pub(crate) struct WaylandCursor {
        library: "libwayland-cursor",
        sonames: ["libwayland-cursor.so.0"],
        headers: ["wayland-cursor.h"],
        pkg_config: "wayland-cursor",
        package: "libwayland-dev",
        functions {
            fn wl_cursor_theme_load(name: *const CChar, size: CInt, shm: *mut WlShm) -> *mut WlCursorTheme;
            fn wl_cursor_theme_destroy(theme: *mut WlCursorTheme);
            fn wl_cursor_theme_get_cursor(theme: *mut WlCursorTheme, name: *const CChar) -> *mut WlCursor;
            fn wl_cursor_image_get_buffer(image: *mut WlCursorImage) -> *mut WlBuffer;
        }
    }
}

/// The first frame of a theme cursor.
pub(crate) struct CursorImage {
    /// The buffer of the frame. The theme owns it, so it is not
    /// destroyed when this value drops.
    pub(crate) buffer: Proxy,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) hotspot: (u32, u32),
}

/// A loaded cursor theme. It holds the connection open until it is
/// destroyed.
pub(crate) struct CursorTheme {
    lib: &'static WaylandCursor,
    raw: *mut WlCursorTheme,
    display: Rc<Display>,
}

impl CursorTheme {
    /// Loads theme `name`, or the default theme, at `size` pixels, with
    /// buffers from `shm`. Returns `None` when the library does not load
    /// or no theme loads.
    pub(crate) fn load(shm: &Proxy, name: Option<&str>, size: u32) -> Option<Self> {
        if !std::ptr::eq(shm.interface(), &super::WL_SHM) {
            return None;
        }
        let lib = WaylandCursor::get().ok()?;
        let name = name.map(CString::new).transpose().ok()?;
        let pointer = name
            .as_deref()
            .map_or(std::ptr::null(), |name| name.as_ptr());
        let size = CInt::try_from(size).ok()?;
        // SAFETY: `pointer` is null or a NUL-terminated name, and `shm` is
        // a live `wl_shm` proxy of this connection.
        let raw = unsafe {
            (lib.wl_cursor_theme_load)(pointer.cast::<CChar>(), size, shm.raw().cast::<WlShm>())
        };
        (!raw.is_null()).then(|| Self {
            lib,
            raw,
            display: Rc::clone(&shm.display),
        })
    }

    /// The first frame of the first of `names` that the theme has.
    pub(crate) fn image(&self, names: &[&str]) -> Option<CursorImage> {
        names.iter().find_map(|name| self.first_frame(name))
    }

    fn first_frame(&self, name: &str) -> Option<CursorImage> {
        let name = CString::new(name).ok()?;
        // SAFETY: the theme is live and `name` is NUL-terminated.
        let cursor =
            unsafe { (self.lib.wl_cursor_theme_get_cursor)(self.raw, name.as_ptr().cast()) };
        if cursor.is_null() {
            return None;
        }
        // SAFETY: a non-null cursor of a live theme stays valid while the
        // theme lives.
        let cursor = unsafe { &*cursor };
        if cursor.image_count == 0 || cursor.images.is_null() {
            return None;
        }
        // SAFETY: `images` holds `image_count` entries, and there is one.
        let image = unsafe { *cursor.images };
        if image.is_null() {
            return None;
        }
        // SAFETY: the image belongs to the live theme.
        let frame = unsafe { *image };
        // SAFETY: the image belongs to the live theme; the theme owns the
        // buffer that the call returns.
        let buffer = unsafe { (self.lib.wl_cursor_image_get_buffer)(image) };
        if buffer.is_null() {
            return None;
        }
        Some(CursorImage {
            buffer: Proxy {
                display: Rc::clone(&self.display),
                raw: buffer.cast::<WlProxy>(),
                id: ObjectId(0),
                interface: &WL_BUFFER,
                version: 1,
                owned: false,
            },
            width: frame.width,
            height: frame.height,
            hotspot: (frame.hotspot_x, frame.hotspot_y),
        })
    }
}

impl Drop for CursorTheme {
    fn drop(&mut self) {
        // SAFETY: the theme came from `wl_cursor_theme_load` and this is
        // its only release; the connection is still open.
        unsafe { (self.lib.wl_cursor_theme_destroy)(self.raw) };
    }
}
