//! The declarations of FreeType, and `FtFace`, an owned reference to one
//! face of a font file.
//!
//! The crate uses one `FT_Library` for the process. FreeType requires a
//! lock around `FT_New_Face` and `FT_Done_Face` when threads share a
//! library, so each call that can create or free a face holds `lock()`.
//! HarfBuzz and Cairo free their face references with `FT_Done_Face`, so
//! the wrappers in `harfbuzz` and `cairo` hold the same lock when they
//! release an object that can own a face.

use super::ctypes::{CChar, CInt, CLong, CUint};
use super::macros::{c_consts, libraries, opaque_types};
use std::ffi::{CStr, CString};
use std::sync::{Mutex, MutexGuard, OnceLock};

opaque_types! {
    /// `struct FT_LibraryRec_`, the target of `FT_Library`.
    pub(crate) FtLibraryRec = "struct FT_LibraryRec_";
    /// `struct FT_FaceRec_`, the target of `FT_Face`.
    pub(crate) FtFaceRec = "struct FT_FaceRec_";
}

libraries! {
    /// libfreetype: font files and glyph outlines.
    pub(crate) struct FreeType {
        library: "libfreetype",
        sonames: ["libfreetype.so.6"],
        headers: ["ft2build.h", "freetype/freetype.h"],
        pkg_config: "freetype2",
        package: "libfreetype-dev",
        functions {
            fn FT_Init_FreeType(library: *mut *mut FtLibraryRec) -> CInt;
            fn FT_New_Face(
                library: *mut FtLibraryRec,
                path: *const CChar,
                index: CLong,
                face: *mut *mut FtFaceRec,
            ) -> CInt;
            fn FT_Done_Face(face: *mut FtFaceRec) -> CInt;
            fn FT_Set_Char_Size(
                face: *mut FtFaceRec,
                width: CLong,
                height: CLong,
                horizontal: CUint,
                vertical: CUint,
            ) -> CInt;
            fn FT_Get_Postscript_Name(face: *mut FtFaceRec) -> *const CChar;
        }
    }
}

c_consts! {
    /// The constants of this module, for the binding check.
    pub(crate) fn constants;
    /// Load glyphs without the hinter, so advances scale linearly.
    pub(crate) const FT_LOAD_NO_HINTING: CInt = 2;
}

/// The library of the process, and the lock of face creation.
struct Shared {
    library: *mut FtLibraryRec,
    lock: Mutex<()>,
}

// SAFETY: the library pointer is only used under `lock` for face creation
// and release, which FreeType permits from any thread.
unsafe impl Send for Shared {}
// SAFETY: as above.
unsafe impl Sync for Shared {}

/// The library of the process, or `None` when FreeType does not load.
fn shared() -> Option<&'static Shared> {
    static SHARED: OnceLock<Option<Shared>> = OnceLock::new();
    SHARED
        .get_or_init(|| {
            let lib = FreeType::get().ok()?;
            let mut library = std::ptr::null_mut();
            // SAFETY: the out-pointer is valid for the call.
            let error = unsafe { (lib.FT_Init_FreeType)(&raw mut library) };
            (error == 0 && !library.is_null()).then(|| Shared {
                library,
                lock: Mutex::new(()),
            })
        })
        .as_ref()
}

/// Holds the lock of face creation and release. Callers that release a
/// HarfBuzz or Cairo object that can own a face hold it too.
pub(crate) fn lock() -> Option<MutexGuard<'static, ()>> {
    let shared = shared()?;
    Some(
        shared
            .lock
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()),
    )
}

/// Releases one reference to `face` under the lock.
///
/// # Safety
///
/// `face` is a live face, and the caller owns the reference it releases.
pub(crate) unsafe fn release_face(face: *mut FtFaceRec) {
    let Ok(lib) = FreeType::get() else {
        return;
    };
    let _guard = lock();
    // SAFETY: the caller owns this reference; the lock serializes the
    // release with face creation on other threads.
    unsafe { (lib.FT_Done_Face)(face) };
}

/// One owned reference to a face of a font file.
pub(crate) struct FtFace {
    raw: *mut FtFaceRec,
}

impl FtFace {
    /// Opens face `index` of the font file at `path`. The index can name
    /// a named instance of a variable font in its upper 16 bits.
    pub(crate) fn open(path: &str, index: i64) -> Option<Self> {
        let lib = FreeType::get().ok()?;
        let shared = shared()?;
        let path = CString::new(path).ok()?;
        let mut raw = std::ptr::null_mut();
        let error = {
            let _guard = lock();
            // SAFETY: the library is initialized, the path is
            // NUL-terminated, and the out-pointer is valid for the call.
            unsafe { (lib.FT_New_Face)(shared.library, path.as_ptr().cast(), index, &raw mut raw) }
        };
        (error == 0 && !raw.is_null()).then_some(Self { raw })
    }

    /// Sets the size of the face in pixels at 72 dots per inch.
    pub(crate) fn set_pixel_size(&self, pixels: f64) -> bool {
        let Ok(lib) = FreeType::get() else {
            return false;
        };
        let size = (pixels * 64.0).round() as CLong;
        // SAFETY: the face is live; the call changes only its size.
        let error = unsafe { (lib.FT_Set_Char_Size)(self.raw, 0, size, 72, 72) };
        error == 0
    }

    /// The PostScript name of the face, when it has one.
    pub(crate) fn postscript_name(&self) -> Option<String> {
        let lib = FreeType::get().ok()?;
        // SAFETY: the face is live.
        let name = unsafe { (lib.FT_Get_Postscript_Name)(self.raw) };
        if name.is_null() {
            return None;
        }
        // SAFETY: FreeType returns a NUL-terminated string that lives as
        // long as the face; it is copied at once.
        let name = unsafe { CStr::from_ptr(name.cast()) };
        Some(name.to_string_lossy().into_owned())
    }

    /// The raw face, for the HarfBuzz and Cairo wrappers. The pointer is
    /// valid while `self` lives.
    pub(crate) fn as_raw(&self) -> *mut FtFaceRec {
        self.raw
    }

    /// Gives up the reference without releasing it. The caller now owns
    /// it.
    pub(crate) fn into_raw(self) -> *mut FtFaceRec {
        let raw = self.raw;
        std::mem::forget(self);
        raw
    }
}

impl Drop for FtFace {
    fn drop(&mut self) {
        // SAFETY: `self` owns one reference to the live face.
        unsafe { release_face(self.raw) }
    }
}
