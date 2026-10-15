//! The image codec libraries, with a safe decoder for each format and a
//! PNG encoder.
//!
//! Each decoder gives the pixels in the layout of its library. The
//! conversion to straight-alpha RGBA, EXIF orientation, and GIF frame
//! composition are in `crate::imaging`. Every library reports an error by
//! a result value: no call long-jumps across a Rust frame or exits.

pub(crate) mod gif;
pub(crate) mod heif;
pub(crate) mod jpeg;
pub(crate) mod png;
pub(crate) mod tiff;
pub(crate) mod webp;

use super::dl::LoadError;
use std::cell::Cell;

/// Why a codec call failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CodecError {
    /// The library or one of its required symbols did not load.
    Unavailable,
    /// The library rejected the data.
    Invalid,
    /// The image has more pixels than the limit of the caller.
    TooLarge,
}

impl From<LoadError> for CodecError {
    fn from(_: LoadError) -> Self {
        Self::Unavailable
    }
}

/// How the four bytes of each pixel of a `Raster` are stored.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PixelLayout {
    /// Red, green, blue, and straight alpha.
    Rgba,
    /// Red, green, blue, and alpha, with the color multiplied by alpha.
    PremultipliedRgba,
    /// Cyan, magenta, yellow, and black, inverted as Adobe applications
    /// write them in JPEG files.
    Cmyk,
}

/// Decoded pixels: `width * height` pixels of four bytes, row by row from
/// the top.
#[derive(Clone, Debug)]
pub(crate) struct Raster {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) layout: PixelLayout,
    pub(crate) pixels: Vec<u8>,
}

/// The size of an image as `(width, height, pixel count)` when both sides
/// are positive and the count is at most `limit`.
fn checked_size(
    width: impl TryInto<u32>,
    height: impl TryInto<u32>,
    limit: usize,
) -> Result<(u32, u32, usize), CodecError> {
    let width: u32 = width.try_into().map_err(|_| CodecError::Invalid)?;
    let height: u32 = height.try_into().map_err(|_| CodecError::Invalid)?;
    if width == 0 || height == 0 {
        return Err(CodecError::Invalid);
    }
    let count = (width as usize)
        .checked_mul(height as usize)
        .filter(|count| *count <= limit)
        .ok_or(CodecError::TooLarge)?;
    Ok((width, height, count))
}

/// Encoded bytes that a library reads through callbacks, with the read
/// position.
struct Input<'a> {
    bytes: &'a [u8],
    position: Cell<usize>,
}

impl<'a> Input<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: Cell::new(0),
        }
    }

    /// The next bytes, at most `length` of them, and moves past them.
    fn read(&self, length: usize) -> &'a [u8] {
        let start = self.position.get().min(self.bytes.len());
        let end = start.saturating_add(length).min(self.bytes.len());
        self.position.set(end);
        &self.bytes[start..end]
    }

    /// Moves to `offset` from the start (`whence` 0), from the position
    /// (1), or from the end (2), as `lseek` does. A position before the
    /// start is `None` and does not move.
    fn seek(&self, offset: i64, whence: i32) -> Option<u64> {
        let base = match whence {
            0 => 0,
            1 => self.position.get(),
            2 => self.bytes.len(),
            _ => return None,
        };
        let target = i64::try_from(base).ok()?.checked_add(offset)?;
        let target = usize::try_from(target).ok()?;
        self.position.set(target);
        u64::try_from(target).ok()
    }

    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }
}

/// The declarations of the codec libraries, for the binding check.
#[cfg(test)]
pub(crate) fn check_items() -> Vec<super::checks::LibraryItems> {
    vec![
        png::Png::check_items(),
        jpeg::TurboJpeg::check_items(),
        gif::Gif::check_items(),
        tiff::Tiff::check_items(),
        webp::WebP::check_items(),
        heif::Heif::check_items(),
    ]
}

/// The struct layouts of the codec libraries, for the binding check.
#[cfg(test)]
pub(crate) fn layouts() -> Vec<super::checks::StructItem> {
    vec![
        png::PngImage::layout(),
        gif::GifColorType::layout(),
        gif::ColorMapObject::layout(),
        gif::GifImageDesc::layout(),
        gif::SavedImage::layout(),
        gif::GifFileType::layout(),
        gif::GraphicsControlBlock::layout(),
        heif::HeifError::layout(),
    ]
}

/// The constants of the codec libraries, for the binding check.
#[cfg(test)]
pub(crate) fn constants() -> Vec<super::checks::ConstItem> {
    [
        png::constants(),
        jpeg::constants(),
        gif::constants(),
        tiff::constants(),
        heif::HeifErrorCode::constants(),
        heif::HeifColorspace::constants(),
        heif::HeifChroma::constants(),
        heif::HeifChannel::constants(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_reject_zero_negative_and_too_many_pixels() {
        assert_eq!(checked_size(4, 3, 12), Ok((4, 3, 12)));
        assert_eq!(checked_size(0, 3, 12), Err(CodecError::Invalid));
        assert_eq!(checked_size(-1, 3, 12), Err(CodecError::Invalid));
        assert_eq!(checked_size(4, 4, 12), Err(CodecError::TooLarge));
    }

    #[test]
    fn input_reads_and_seeks_inside_the_bytes() {
        let input = Input::new(b"abcdef");
        assert_eq!(input.read(2), b"ab");
        assert_eq!(input.seek(1, 1), Some(3));
        assert_eq!(input.read(10), b"def");
        assert_eq!(input.read(1), b"");
        assert_eq!(input.seek(-2, 2), Some(4));
        assert_eq!(input.read(1), b"e");
        assert_eq!(input.seek(-1, 0), None);
        assert_eq!(input.len(), 6);
    }
}
