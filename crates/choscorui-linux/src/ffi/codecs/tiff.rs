//! libtiff: TIFF decoding from memory through client callbacks. libtiff
//! reports errors by its result.

use super::{CodecError, Input, PixelLayout, Raster, checked_size};
use crate::ffi::ctypes::{CChar, CInt, c_void};
use crate::ffi::macros::{c_consts, callback, libraries, opaque_types};
use std::marker::PhantomData;

opaque_types! {
    /// `TIFF`: an open TIFF file.
    pub(crate) TiffFile = "TIFF";
}

/// `TIFFErrorHandler`, a pointer to a C function that takes a `va_list`.
/// The crate passes only null, which turns the messages off.
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct TiffErrorHandler(*const c_void);

#[cfg(test)]
impl crate::ffi::ctype::CType for TiffErrorHandler {
    fn spelling() -> String {
        "TIFFErrorHandler".to_owned()
    }
}

c_consts! {
    /// The constants of libtiff, for the binding check.
    pub(crate) fn constants;
    const TIFFTAG_IMAGEWIDTH: u32 = 256;
    const TIFFTAG_IMAGELENGTH: u32 = 257;
    const ORIENTATION_TOPLEFT: CInt = 1;
}

/// libtiff's `TIFFReadWriteProc`.
type ReadWriteProc = Option<unsafe extern "C" fn(*mut c_void, *mut c_void, isize) -> isize>;
/// libtiff's `TIFFSeekProc`.
type SeekProc = Option<unsafe extern "C" fn(*mut c_void, u64, CInt) -> u64>;
/// libtiff's `TIFFCloseProc`.
type CloseProc = Option<unsafe extern "C" fn(*mut c_void) -> CInt>;
/// libtiff's `TIFFSizeProc`.
type SizeProc = Option<unsafe extern "C" fn(*mut c_void) -> u64>;
/// libtiff's `TIFFMapFileProc`.
type MapProc = Option<unsafe extern "C" fn(*mut c_void, *mut *mut c_void, *mut u64) -> CInt>;
/// libtiff's `TIFFUnmapFileProc`.
type UnmapProc = Option<unsafe extern "C" fn(*mut c_void, *mut c_void, u64)>;

libraries! {
    /// libtiff: TIFF decoding.
    pub(crate) struct Tiff {
        library: "libtiff",
        sonames: ["libtiff.so.6", "libtiff.so.5"],
        headers: ["tiffio.h"],
        pkg_config: "libtiff-4",
        package: "libtiff-dev",
        functions {
            fn TIFFClientOpen(
                name: *const CChar,
                mode: *const CChar,
                client: *mut c_void,
                read: ReadWriteProc,
                write: ReadWriteProc,
                seek: SeekProc,
                close: CloseProc,
                size: SizeProc,
                map: MapProc,
                unmap: UnmapProc,
            ) -> *mut TiffFile;
            fn TIFFClose(file: *mut TiffFile);
            fn TIFFReadRGBAImageOriented(
                file: *mut TiffFile,
                width: u32,
                height: u32,
                raster: *mut u32,
                orientation: CInt,
                stop_on_error: CInt,
            ) -> CInt;
            fn TIFFSetErrorHandler(handler: TiffErrorHandler) -> TiffErrorHandler;
            fn TIFFSetWarningHandler(handler: TiffErrorHandler) -> TiffErrorHandler;
        }
        variadic {
            fn TIFFGetField(file: *mut TiffFile, tag: u32) -> CInt;
        }
    }
}

/// The `Input` behind a client handle of `decode`.
///
/// # Safety
///
/// `client` is the handle that `decode` passed to `TIFFClientOpen`, and
/// the file is still open.
unsafe fn input<'a>(client: *mut c_void) -> &'a Input<'a> {
    // SAFETY: `decode` passes an `Input` that outlives the file, and
    // libtiff calls back on the thread of `decode`.
    unsafe { &*client.cast::<Input<'a>>() }
}

callback! {
    /// `TIFFReadWriteProc` for reads: copies the next bytes to `buffer`.
    ///
    /// # Safety
    ///
    /// `client` is the handle of an open file of `decode`, and `buffer`
    /// has room for `length` bytes.
    fn read_proc(client: *mut c_void, buffer: *mut c_void, length: isize) -> isize
        [boundary "libtiff read", fallback -1]
    {
        // SAFETY: the caller passes the handle of an open file.
        let input = unsafe { input(client) };
        let chunk = input.read(usize::try_from(length).unwrap_or(0));
        // SAFETY: `buffer` has room for `length` bytes, and the chunk is
        // at most that long. The input does not overlap a libtiff buffer.
        unsafe { std::ptr::copy_nonoverlapping(chunk.as_ptr(), buffer.cast(), chunk.len()) };
        chunk.len() as isize
    }

    /// `TIFFReadWriteProc` for writes: the input is read-only, so it
    /// writes nothing.
    ///
    /// # Safety
    ///
    /// No requirement: the procedure reads none of its arguments.
    fn write_proc(_client: *mut c_void, _buffer: *mut c_void, _length: isize) -> isize
        [boundary "libtiff write", fallback -1]
    {
        -1
    }

    /// `TIFFSeekProc`: moves the read position, as `lseek` does.
    ///
    /// # Safety
    ///
    /// `client` is the handle of an open file of `decode`.
    fn seek_proc(client: *mut c_void, offset: u64, whence: CInt) -> u64
        [boundary "libtiff seek", fallback u64::MAX]
    {
        // SAFETY: the caller passes the handle of an open file.
        let input = unsafe { input(client) };
        // libtiff passes a negative relative offset as a wrapped `toff_t`.
        input.seek(offset as i64, whence).unwrap_or(u64::MAX)
    }

    /// `TIFFCloseProc`: `decode` owns the input, so it closes nothing.
    ///
    /// # Safety
    ///
    /// No requirement: the procedure reads none of its arguments.
    fn close_proc(_client: *mut c_void) -> CInt
        [boundary "libtiff close", fallback 0]
    {
        0
    }

    /// `TIFFSizeProc`: the length of the input.
    ///
    /// # Safety
    ///
    /// `client` is the handle of an open file of `decode`.
    fn size_proc(client: *mut c_void) -> u64
        [boundary "libtiff size", fallback 0]
    {
        // SAFETY: the caller passes the handle of an open file.
        unsafe { input(client) }.len()
    }

    /// `TIFFMapFileProc`: the input is not mapped, so libtiff reads it.
    ///
    /// # Safety
    ///
    /// No requirement: the procedure reads none of its arguments.
    fn map_proc(_client: *mut c_void, _base: *mut *mut c_void, _size: *mut u64) -> CInt
        [boundary "libtiff map", fallback 0]
    {
        0
    }

    /// `TIFFUnmapFileProc`: nothing is mapped.
    ///
    /// # Safety
    ///
    /// No requirement: the procedure reads none of its arguments.
    fn unmap_proc(_client: *mut c_void, _base: *mut c_void, _size: u64)
        [boundary "libtiff unmap", fallback ()]
    {}
}

/// An open TIFF file, closed on drop. It reads from an `Input` that lives
/// for `'a`.
struct File<'a> {
    tiff: &'static Tiff,
    file: *mut TiffFile,
    input: PhantomData<&'a Input<'a>>,
}

impl Drop for File<'_> {
    fn drop(&mut self) {
        // SAFETY: the file came from `TIFFClientOpen`, and nothing uses it
        // after the drop. The close procedure does nothing.
        unsafe { (self.tiff.TIFFClose)(self.file) };
    }
}

impl File<'_> {
    /// The value of a `uint32_t` tag.
    fn tag(&self, tag: u32) -> Option<u32> {
        let mut value = 0_u32;
        // SAFETY: the file is open, and both size tags take one
        // `uint32_t *` argument.
        let found =
            unsafe { (self.tiff.TIFFGetField)(self.file, tag, std::ptr::from_mut(&mut value)) };
        (found == 1).then_some(value)
    }
}

/// Turns the libtiff messages off one time for the process. libtiff
/// writes them to standard error by default; `decode` reports each
/// failure by its result.
fn quiet(tiff: &Tiff) {
    static QUIET: std::sync::Once = std::sync::Once::new();
    QUIET.call_once(|| {
        let none = TiffErrorHandler(std::ptr::null());
        // SAFETY: libtiff accepts a null handler and then skips the
        // message.
        unsafe { (tiff.TIFFSetErrorHandler)(none) };
        // SAFETY: as above.
        unsafe { (tiff.TIFFSetWarningHandler)(none) };
    });
}

/// Decodes the first directory of a TIFF file to premultiplied RGBA.
/// libtiff applies the orientation tag of the file.
pub(crate) fn decode(bytes: &[u8], limit: usize) -> Result<Raster, CodecError> {
    let tiff = Tiff::get()?;
    quiet(tiff);
    let input = Input::new(bytes);
    // `m` turns memory mapping off: the input is already in memory.
    // SAFETY: the name and the mode are NUL-terminated. Each procedure
    // matches its libtiff type, and `input` outlives the file, which
    // `File` closes before `input` drops.
    let raw = unsafe {
        (tiff.TIFFClientOpen)(
            c"memory".as_ptr().cast(),
            c"rm".as_ptr().cast(),
            std::ptr::from_ref(&input).cast_mut().cast(),
            Some(read_proc),
            Some(write_proc),
            Some(seek_proc),
            Some(close_proc),
            Some(size_proc),
            Some(map_proc),
            Some(unmap_proc),
        )
    };
    if raw.is_null() {
        return Err(CodecError::Invalid);
    }
    let file = File {
        tiff,
        file: raw,
        input: PhantomData,
    };
    let width = file.tag(TIFFTAG_IMAGEWIDTH).ok_or(CodecError::Invalid)?;
    let height = file.tag(TIFFTAG_IMAGELENGTH).ok_or(CodecError::Invalid)?;
    let (width, height, count) = checked_size(width, height, limit)?;
    let mut raster = vec![0_u32; count];
    // SAFETY: `raster` holds `width * height` pixels, and the orientation
    // puts row 0 at the top.
    let read = unsafe {
        (tiff.TIFFReadRGBAImageOriented)(
            file.file,
            width,
            height,
            raster.as_mut_ptr(),
            ORIENTATION_TOPLEFT,
            1,
        )
    };
    if read != 1 {
        return Err(CodecError::Invalid);
    }
    // libtiff packs each pixel as `A << 24 | B << 16 | G << 8 | R`.
    let pixels = raster
        .iter()
        .flat_map(|pixel| pixel.to_le_bytes())
        .collect();
    Ok(Raster {
        width,
        height,
        layout: PixelLayout::PremultipliedRgba,
        pixels,
    })
}
