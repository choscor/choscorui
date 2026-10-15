//! giflib: GIF decoding. giflib reads through a callback and reports
//! errors by its result.

use super::{CodecError, Input, checked_size};
use crate::ffi::ctypes::{CInt, c_void};
use crate::ffi::macros::{c_consts, callback, ffi_struct, libraries, opaque_types};
use std::marker::PhantomData;

opaque_types! {
    /// `ExtensionBlock`, which the crate never reads.
    pub(crate) ExtensionBlock = "ExtensionBlock";
}

ffi_struct! {
    /// `GifColorType`: one palette entry.
    #[allow(non_snake_case, reason = "the fields keep the C names")]
    pub(crate) struct GifColorType = "GifColorType" {
        pub(crate) Red: u8,
        pub(crate) Green: u8,
        pub(crate) Blue: u8,
    }

    /// `ColorMapObject`: a palette.
    #[allow(non_snake_case, reason = "the fields keep the C names")]
    #[allow(dead_code, reason = "the binding check needs each C field; Rust reads only some")]
    pub(crate) struct ColorMapObject = "ColorMapObject" {
        pub(crate) ColorCount: CInt,
        pub(crate) BitsPerPixel: CInt,
        pub(crate) SortFlag: bool,
        pub(crate) Colors: *mut GifColorType,
    }

    /// `GifImageDesc`: the place, size, and palette of a frame.
    #[allow(non_snake_case, reason = "the fields keep the C names")]
    #[allow(dead_code, reason = "the binding check needs each C field; Rust reads only some")]
    pub(crate) struct GifImageDesc = "GifImageDesc" {
        pub(crate) Left: CInt,
        pub(crate) Top: CInt,
        pub(crate) Width: CInt,
        pub(crate) Height: CInt,
        pub(crate) Interlace: bool,
        pub(crate) ColorMap: *mut ColorMapObject,
    }

    /// `SavedImage`: one frame that `DGifSlurp` read.
    #[allow(non_snake_case, reason = "the fields keep the C names")]
    #[allow(dead_code, reason = "the binding check needs each C field; Rust reads only some")]
    pub(crate) struct SavedImage = "SavedImage" {
        pub(crate) ImageDesc: GifImageDesc,
        pub(crate) RasterBits: *mut u8,
        pub(crate) ExtensionBlockCount: CInt,
        pub(crate) ExtensionBlocks: *mut ExtensionBlock,
    }

    /// `GifFileType`: an open GIF file.
    #[allow(non_snake_case, reason = "the fields keep the C names")]
    #[allow(dead_code, reason = "the binding check needs each C field; Rust reads only some")]
    pub(crate) struct GifFileType = "GifFileType" {
        pub(crate) SWidth: CInt,
        pub(crate) SHeight: CInt,
        pub(crate) SColorResolution: CInt,
        pub(crate) SBackGroundColor: CInt,
        pub(crate) AspectByte: u8,
        pub(crate) SColorMap: *mut ColorMapObject,
        pub(crate) ImageCount: CInt,
        pub(crate) Image: GifImageDesc,
        pub(crate) SavedImages: *mut SavedImage,
        pub(crate) ExtensionBlockCount: CInt,
        pub(crate) ExtensionBlocks: *mut ExtensionBlock,
        pub(crate) Error: CInt,
        pub(crate) UserData: *mut c_void,
        pub(crate) Private: *mut c_void,
    }

    /// `GraphicsControlBlock`: the transparency and timing of a frame.
    #[allow(non_snake_case, reason = "the fields keep the C names")]
    #[allow(dead_code, reason = "the binding check needs each C field; Rust reads only some")]
    pub(crate) struct GraphicsControlBlock = "GraphicsControlBlock" {
        pub(crate) DisposalMode: CInt,
        pub(crate) UserInputFlag: bool,
        pub(crate) DelayTime: CInt,
        pub(crate) TransparentColor: CInt,
    }
}

c_consts! {
    /// The constants of giflib, for the binding check.
    pub(crate) fn constants;
    const GIF_OK: CInt = 1;
}

/// giflib's `InputFunc`.
type InputFunc = Option<unsafe extern "C" fn(*mut GifFileType, *mut u8, CInt) -> CInt>;

libraries! {
    /// giflib: GIF decoding. Debian has no pkg-config file for giflib, so
    /// the binding check finds `gif_lib.h` in the default include path.
    pub(crate) struct Gif {
        library: "libgif",
        sonames: ["libgif.so.7"],
        headers: ["gif_lib.h"],
        pkg_config: "",
        package: "libgif-dev",
        functions {
            fn DGifOpen(user: *mut c_void, read: InputFunc, error: *mut CInt) -> *mut GifFileType;
            fn DGifSlurp(file: *mut GifFileType) -> CInt;
            fn DGifSavedExtensionToGCB(
                file: *mut GifFileType,
                index: CInt,
                control: *mut GraphicsControlBlock,
            ) -> CInt;
            fn DGifCloseFile(file: *mut GifFileType, error: *mut CInt) -> CInt;
        }
    }
}

callback! {
    /// giflib's `InputFunc`: copies the next `length` bytes of the input
    /// to `buffer`, and returns the count that it copied.
    ///
    /// # Safety
    ///
    /// `file` is a file that `decode` opened, so its `UserData` points at
    /// a live `Input`. `buffer` has room for `length` bytes.
    fn read_input(file: *mut GifFileType, buffer: *mut u8, length: CInt) -> CInt
        [boundary "giflib input", fallback 0]
    {
        // SAFETY: giflib passes the live file that `DGifOpen` made.
        let user = unsafe { (*file).UserData };
        // SAFETY: `decode` set `UserData` to an `Input` that outlives the
        // file, and giflib reads on the thread of `decode`.
        let input = unsafe { &*user.cast::<Input<'_>>() };
        let chunk = input.read(usize::try_from(length).unwrap_or(0));
        // SAFETY: `buffer` has room for `length` bytes, and the chunk is
        // at most that long. The input does not overlap a giflib buffer.
        unsafe { std::ptr::copy_nonoverlapping(chunk.as_ptr(), buffer, chunk.len()) };
        // The chunk is at most `length` bytes, so it fits a C int.
        chunk.len() as CInt
    }
}

/// An open GIF file, closed on drop. It reads from an `Input` that lives
/// for `'a`.
struct File<'a> {
    gif: &'static Gif,
    file: *mut GifFileType,
    input: PhantomData<&'a Input<'a>>,
}

impl Drop for File<'_> {
    fn drop(&mut self) {
        let mut error = 0;
        // SAFETY: the file came from `DGifOpen`, and nothing uses it after
        // the drop. `DGifCloseFile` frees it even when it reports an
        // error.
        unsafe { (self.gif.DGifCloseFile)(self.file, &mut error) };
    }
}

/// The first frame of a GIF file, as palette indices.
#[derive(Clone, Debug)]
pub(crate) struct GifFrame {
    /// The size of the logical screen.
    pub(crate) canvas: (u32, u32),
    /// The top-left corner of the frame on the screen.
    pub(crate) origin: (u32, u32),
    /// The size of the frame.
    pub(crate) size: (u32, u32),
    /// One palette index for each pixel, row by row.
    pub(crate) indices: Vec<u8>,
    /// The red, green, and blue values of each palette entry.
    pub(crate) palette: Vec<[u8; 3]>,
    /// The palette index that is transparent.
    pub(crate) transparent: Option<u8>,
}

/// The entries of the palette at `map`.
fn palette(map: *mut ColorMapObject) -> Result<Vec<[u8; 3]>, CodecError> {
    if map.is_null() {
        return Err(CodecError::Invalid);
    }
    // SAFETY: a non-null palette of a slurped file is a live
    // `ColorMapObject`.
    let map = unsafe { *map };
    let count = usize::try_from(map.ColorCount).map_err(|_| CodecError::Invalid)?;
    if map.Colors.is_null() || count > 256 {
        return Err(CodecError::Invalid);
    }
    // SAFETY: giflib allocates `ColorCount` entries at `Colors`.
    let colors = unsafe { std::slice::from_raw_parts(map.Colors, count) };
    Ok(colors
        .iter()
        .map(|color| [color.Red, color.Green, color.Blue])
        .collect())
}

/// Decodes the first frame of a GIF file. The canvas and the frame are
/// each at most `limit` pixels.
pub(crate) fn decode(bytes: &[u8], limit: usize) -> Result<GifFrame, CodecError> {
    let gif = Gif::get()?;
    let input = Input::new(bytes);
    let mut error = 0;
    // SAFETY: `read_input` matches `InputFunc`, and `input` outlives the
    // file, which `File` closes before `input` drops.
    let raw = unsafe {
        (gif.DGifOpen)(
            std::ptr::from_ref(&input).cast_mut().cast(),
            Some(read_input),
            &mut error,
        )
    };
    if raw.is_null() {
        return Err(CodecError::Invalid);
    }
    let file = File {
        gif,
        file: raw,
        input: PhantomData,
    };
    // SAFETY: the file is open; `DGifOpen` read the screen descriptor.
    let screen = unsafe { *file.file };
    let (canvas_width, canvas_height, _) = checked_size(screen.SWidth, screen.SHeight, limit)?;
    // SAFETY: the file is open.
    if unsafe { (gif.DGifSlurp)(file.file) } != GIF_OK {
        return Err(CodecError::Invalid);
    }
    // SAFETY: the slurp filled the file.
    let header = unsafe { *file.file };
    if header.ImageCount < 1 || header.SavedImages.is_null() {
        return Err(CodecError::Invalid);
    }
    // SAFETY: `SavedImages` holds `ImageCount` frames, at least one.
    let frame = unsafe { *header.SavedImages };
    let desc = frame.ImageDesc;
    let (width, height, count) = checked_size(desc.Width, desc.Height, limit)?;
    let origin = (
        u32::try_from(desc.Left).map_err(|_| CodecError::Invalid)?,
        u32::try_from(desc.Top).map_err(|_| CodecError::Invalid)?,
    );
    if frame.RasterBits.is_null() {
        return Err(CodecError::Invalid);
    }
    // SAFETY: the slurp allocates `Width * Height` indices for the frame
    // and puts interlaced rows in order.
    let indices = unsafe { std::slice::from_raw_parts(frame.RasterBits, count) }.to_vec();
    let palette = palette(if desc.ColorMap.is_null() {
        header.SColorMap
    } else {
        desc.ColorMap
    })?;
    let mut control = GraphicsControlBlock {
        DisposalMode: 0,
        UserInputFlag: false,
        DelayTime: 0,
        TransparentColor: -1,
    };
    // SAFETY: the file is slurped, frame 0 exists, and `control` is a
    // local struct.
    let found = unsafe { (gif.DGifSavedExtensionToGCB)(file.file, 0, &mut control) };
    let transparent = if found == GIF_OK {
        u8::try_from(control.TransparentColor).ok()
    } else {
        None
    };
    Ok(GifFrame {
        canvas: (canvas_width, canvas_height),
        origin,
        size: (width, height),
        indices,
        palette,
        transparent,
    })
}
