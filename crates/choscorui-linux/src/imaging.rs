//! Image decoding and encoding with the system codec libraries.
//!
//! `decode_image` finds the format from the first bytes, decodes it with
//! its library, and converts the pixels to straight-alpha RGBA8. It also
//! applies the EXIF orientation of a JPEG file. libheif applies the
//! rotation and mirror properties of a HEIF file itself.

use crate::ffi::codecs::{self, CodecError, PixelLayout, Raster, gif::GifFrame};
use choscorui_core::{Image, ImageError, Support};

/// The largest image that `decode_image` accepts, in pixels. It matches
/// the macOS backend.
const MAX_PIXELS: usize = 64 * 1024 * 1024;

/// An encoded image format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Format {
    Png,
    Jpeg,
    Gif,
    Tiff,
    WebP,
    Heif,
}

/// The ISO BMFF brands of the HEIF images that libheif decodes.
const HEIF_BRANDS: &[&[u8; 4]] = &[
    b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"hevm", b"hevs", b"mif1", b"msf1",
    b"avif", b"avis",
];

/// True when the `ftyp` box at the start of `bytes` names a HEIF brand,
/// as the major brand or a compatible brand.
fn is_heif(bytes: &[u8]) -> bool {
    if bytes.get(4..8) != Some(b"ftyp") {
        return false;
    }
    let Some(size) = bytes.get(0..4) else {
        return false;
    };
    let size = u32::from_be_bytes([size[0], size[1], size[2], size[3]]) as usize;
    let end = size.min(bytes.len());
    // The major brand is at 8..12, and the minor version at 12..16. The
    // compatible brands follow it.
    bytes
        .get(8..12)
        .into_iter()
        .chain(bytes.get(16..end))
        .flat_map(|brands| brands.as_chunks::<4>().0)
        .any(|brand| HEIF_BRANDS.contains(&brand))
}

/// The format of `bytes`, from their signature.
fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Format::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Format::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(Format::Gif)
    } else if [b"II*\0", b"MM\0*", b"II+\0", b"MM\0+"]
        .iter()
        .any(|magic| bytes.starts_with(*magic))
    {
        Some(Format::Tiff)
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some(Format::WebP)
    } else if is_heif(bytes) {
        Some(Format::Heif)
    } else {
        None
    }
}

/// The error of a failed decode: a missing library makes its format
/// unsupported.
fn decode_error(error: CodecError) -> ImageError {
    match error {
        CodecError::Unavailable => ImageError::UnsupportedFormat,
        CodecError::Invalid => ImageError::DecodeFailed,
        CodecError::TooLarge => ImageError::InvalidDimensions,
    }
}

/// Decodes PNG, JPEG, GIF (first frame), TIFF, WebP, and HEIC/HEIF into
/// straight-alpha sRGB RGBA8 pixels, with the EXIF orientation of a JPEG
/// file applied.
///
/// Unknown bytes, and a format whose library is not installed, return
/// [`ImageError::UnsupportedFormat`]. Damaged data returns
/// [`ImageError::DecodeFailed`], and an image of more than 64 Mpx returns
/// [`ImageError::InvalidDimensions`].
pub fn decode_image(bytes: &[u8]) -> Result<Image, ImageError> {
    let format = sniff(bytes).ok_or(ImageError::UnsupportedFormat)?;
    let raster = match format {
        Format::Png => codecs::png::decode(bytes, MAX_PIXELS),
        Format::Jpeg => codecs::jpeg::decode(bytes, MAX_PIXELS)
            .map(|raster| orient(raster, exif_orientation(bytes))),
        Format::Gif => codecs::gif::decode(bytes, MAX_PIXELS).map(compose),
        Format::Tiff => codecs::tiff::decode(bytes, MAX_PIXELS),
        Format::WebP => codecs::webp::decode(bytes, MAX_PIXELS),
        Format::Heif => codecs::heif::decode(bytes, MAX_PIXELS),
    }
    .map_err(decode_error)?;
    let pixels = straight_rgba(raster.layout, raster.pixels);
    Image::from_rgba8(raster.width, raster.height, pixels)
}

/// Encodes straight-alpha RGBA8 pixels as an sRGB PNG file.
pub fn encode_png(image: &Image) -> Result<Vec<u8>, ImageError> {
    codecs::png::encode(image.width(), image.height(), image.pixels())
        .map_err(|_| ImageError::EncodeFailed)
}

/// `Supported` when libpng and libturbojpeg load, else `Unsupported`. The
/// other formats depend on their own libraries.
pub fn image_codecs_support() -> Support {
    if codecs::png::Png::available() && codecs::jpeg::TurboJpeg::available() {
        Support::Supported
    } else {
        Support::Unsupported
    }
}

/// Converts pixels in `layout` to straight-alpha RGBA.
fn straight_rgba(layout: PixelLayout, mut pixels: Vec<u8>) -> Vec<u8> {
    match layout {
        PixelLayout::Rgba => {}
        PixelLayout::PremultipliedRgba => {
            for pixel in pixels.as_chunks_mut::<4>().0 {
                *pixel = unpremultiply(*pixel);
            }
        }
        PixelLayout::Cmyk => {
            for pixel in pixels.as_chunks_mut::<4>().0 {
                *pixel = inverted_cmyk_to_rgba(*pixel);
            }
        }
    }
    pixels
}

/// The straight-alpha value of a premultiplied pixel.
fn unpremultiply(pixel: [u8; 4]) -> [u8; 4] {
    let alpha = pixel[3];
    if alpha == 0 {
        return [0, 0, 0, 0];
    }
    let channel = |value: u8| {
        ((u16::from(value) * 255 + u16::from(alpha) / 2) / u16::from(alpha)).min(255) as u8
    };
    [
        channel(pixel[0]),
        channel(pixel[1]),
        channel(pixel[2]),
        alpha,
    ]
}

/// The opaque RGBA value of an inverted CMYK pixel, where 255 is no ink.
fn inverted_cmyk_to_rgba([cyan, magenta, yellow, black]: [u8; 4]) -> [u8; 4] {
    let channel = |value: u8| ((u16::from(value) * u16::from(black) + 127) / 255) as u8;
    [channel(cyan), channel(magenta), channel(yellow), 255]
}

/// The first frame of a GIF file on its logical screen. The pixels
/// outside the frame, and the transparent index, are transparent.
fn compose(frame: GifFrame) -> Raster {
    let (width, height) = frame.canvas;
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    let (left, top) = (frame.origin.0 as usize, frame.origin.1 as usize);
    let (frame_width, frame_height) = (frame.size.0 as usize, frame.size.1 as usize);
    for y in 0..frame_height.min((height as usize).saturating_sub(top)) {
        for x in 0..frame_width.min((width as usize).saturating_sub(left)) {
            let index = frame.indices[y * frame_width + x];
            if Some(index) == frame.transparent {
                continue;
            }
            let Some(&[red, green, blue]) = frame.palette.get(usize::from(index)) else {
                continue;
            };
            let at = ((top + y) * width as usize + left + x) * 4;
            pixels[at..at + 4].copy_from_slice(&[red, green, blue, 255]);
        }
    }
    Raster {
        width,
        height,
        layout: PixelLayout::Rgba,
        pixels,
    }
}

/// The EXIF orientation (1 to 8) of a JPEG file, from the `Orientation`
/// tag of IFD0 in its APP1 `Exif` segment. A file without a valid tag has
/// orientation 1.
fn exif_orientation(jpeg: &[u8]) -> u16 {
    let mut at = 2;
    while at + 4 <= jpeg.len() {
        if jpeg[at] != 0xFF {
            return 1;
        }
        let marker = jpeg[at + 1];
        if marker == 0xFF {
            at += 1;
            continue;
        }
        // Start of scan and end of image: the metadata segments are
        // before them.
        if marker == 0xDA || marker == 0xD9 {
            return 1;
        }
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            at += 2;
            continue;
        }
        let length = usize::from(u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]));
        let Some(segment) = jpeg.get(at + 4..at + 2 + length) else {
            return 1;
        };
        if marker == 0xE1
            && let Some(tiff) = segment.strip_prefix(b"Exif\0\0")
            && let Some(orientation) = tiff_orientation(tiff)
        {
            return orientation;
        }
        at += 2 + length;
    }
    1
}

/// The `Orientation` tag (0x0112) of IFD0 of a TIFF structure.
fn tiff_orientation(tiff: &[u8]) -> Option<u16> {
    let little = match tiff.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |at: usize| {
        let bytes: [u8; 2] = tiff.get(at..at + 2)?.try_into().ok()?;
        Some(if little {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        })
    };
    let u32_at = |at: usize| {
        let bytes: [u8; 4] = tiff.get(at..at + 4)?.try_into().ok()?;
        Some(if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    };
    if u16_at(2)? != 42 {
        return None;
    }
    let ifd = usize::try_from(u32_at(4)?).ok()?;
    let count = usize::from(u16_at(ifd)?);
    (0..count)
        .map(|entry| ifd + 2 + entry * 12)
        .find(|&entry| u16_at(entry) == Some(0x0112))
        // The value of a SHORT tag with one value is in the entry.
        .filter(|&entry| u16_at(entry + 2) == Some(3))
        .and_then(|entry| u16_at(entry + 8))
        .filter(|orientation| (1..=8).contains(orientation))
}

/// Applies an EXIF orientation to straight or premultiplied RGBA or CMYK
/// pixels: the result is the image as it shows on the screen.
fn orient(raster: Raster, orientation: u16) -> Raster {
    if !(2..=8).contains(&orientation) {
        return raster;
    }
    let (width, height) = (raster.width as usize, raster.height as usize);
    let transposed = orientation >= 5;
    let (out_width, out_height) = if transposed {
        (height, width)
    } else {
        (width, height)
    };
    // The stored pixel that shows at (x, y) of the result.
    let source = |x: usize, y: usize| match orientation {
        2 => (width - 1 - x, y),
        3 => (width - 1 - x, height - 1 - y),
        4 => (x, height - 1 - y),
        5 => (y, x),
        6 => (y, height - 1 - x),
        7 => (width - 1 - y, height - 1 - x),
        _ => (width - 1 - y, x),
    };
    let mut pixels = Vec::with_capacity(raster.pixels.len());
    for y in 0..out_height {
        for x in 0..out_width {
            let (sx, sy) = source(x, y);
            let at = (sy * width + sx) * 4;
            pixels.extend_from_slice(&raster.pixels[at..at + 4]);
        }
    }
    Raster {
        width: out_width as u32,
        height: out_height as u32,
        layout: raster.layout,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_name_each_format() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), Some(Format::Png));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(Format::Jpeg));
        assert_eq!(sniff(b"GIF89a"), Some(Format::Gif));
        assert_eq!(sniff(b"MM\0*\0\0\0\x08"), Some(Format::Tiff));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(Format::WebP));
        assert_eq!(
            sniff(b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"),
            Some(Format::Heif)
        );
        assert_eq!(
            sniff(b"\0\0\0\x18ftypisom\0\0\0\0mif1heic"),
            Some(Format::Heif)
        );
        assert_eq!(sniff(b"\0\0\0\x14ftypisom\0\0\0\0mp41"), None);
        assert_eq!(sniff(b"BM"), None);
        assert_eq!(sniff(b""), None);
    }

    /// A 3 x 2 image whose pixel at (x, y) is `[x, y, 0, 255]`.
    fn grid() -> Raster {
        let pixels = (0..2_u8)
            .flat_map(|y| (0..3_u8).flat_map(move |x| [x, y, 0, 255]))
            .collect();
        Raster {
            width: 3,
            height: 2,
            layout: PixelLayout::Rgba,
            pixels,
        }
    }

    /// The stored (x, y) of the pixel at (x, y) of `raster`.
    fn at(raster: &Raster, x: usize, y: usize) -> (u8, u8) {
        let i = (y * raster.width as usize + x) * 4;
        (raster.pixels[i], raster.pixels[i + 1])
    }

    #[test]
    fn orientations_move_the_stored_corners() {
        // (orientation, result size, stored pixel at the top-left and
        // top-right corners of the result)
        let cases = [
            (1, (3, 2), (0, 0), (2, 0)),
            (2, (3, 2), (2, 0), (0, 0)),
            (3, (3, 2), (2, 1), (0, 1)),
            (4, (3, 2), (0, 1), (2, 1)),
            (5, (2, 3), (0, 0), (0, 1)),
            (6, (2, 3), (0, 1), (0, 0)),
            (7, (2, 3), (2, 1), (2, 0)),
            (8, (2, 3), (2, 0), (2, 1)),
        ];
        for (orientation, size, top_left, top_right) in cases {
            let result = orient(grid(), orientation);
            assert_eq!((result.width, result.height), size, "{orientation}");
            assert_eq!(at(&result, 0, 0), top_left, "{orientation}");
            assert_eq!(
                at(&result, size.0 as usize - 1, 0),
                top_right,
                "{orientation}"
            );
        }
    }

    /// A JPEG prefix with an APP0 segment and an APP1 Exif segment.
    fn jpeg_with_exif(tiff: &[u8]) -> Vec<u8> {
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0];
        let length = u16::try_from(2 + 6 + tiff.len()).unwrap();
        jpeg.extend_from_slice(&[0xFF, 0xE1]);
        jpeg.extend_from_slice(&length.to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(tiff);
        jpeg.extend_from_slice(&[0xFF, 0xDA, 0, 2]);
        jpeg
    }

    #[test]
    fn exif_orientation_reads_both_byte_orders() {
        let big = [
            b'M', b'M', 0, 42, 0, 0, 0, 8, 0, 1, 0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0,
        ];
        assert_eq!(exif_orientation(&jpeg_with_exif(&big)), 6);
        let little = [
            b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 8, 0, 0, 0,
        ];
        assert_eq!(exif_orientation(&jpeg_with_exif(&little)), 8);
    }

    #[test]
    fn damaged_exif_gives_orientation_one() {
        let out_of_range = [
            b'M', b'M', 0, 42, 0, 0, 0, 8, 0, 1, 0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 9, 0, 0,
        ];
        assert_eq!(exif_orientation(&jpeg_with_exif(&out_of_range)), 1);
        let bad_offset = [b'M', b'M', 0, 42, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(exif_orientation(&jpeg_with_exif(&bad_offset)), 1);
        assert_eq!(exif_orientation(&[0xFF, 0xD8, 0xFF, 0xE1, 0xFF, 0xFF]), 1);
        assert_eq!(exif_orientation(&[0xFF, 0xD8]), 1);
    }

    #[test]
    fn pixel_layouts_become_straight_rgba() {
        assert_eq!(unpremultiply([64, 32, 0, 128]), [128, 64, 0, 128]);
        assert_eq!(unpremultiply([10, 10, 10, 0]), [0, 0, 0, 0]);
        assert_eq!(
            inverted_cmyk_to_rgba([255, 0, 255, 255]),
            [255, 0, 255, 255]
        );
        assert_eq!(inverted_cmyk_to_rgba([255, 255, 255, 0]), [0, 0, 0, 255]);
    }

    #[test]
    fn a_gif_frame_lands_on_its_screen() {
        let frame = GifFrame {
            canvas: (3, 2),
            origin: (1, 1),
            size: (3, 1),
            indices: vec![0, 1, 2],
            palette: vec![[10, 20, 30], [40, 50, 60]],
            transparent: Some(1),
        };
        let raster = compose(frame);
        assert_eq!((raster.width, raster.height), (3, 2));
        let pixel = |x: usize, y: usize| raster.pixels[(y * 3 + x) * 4..][..4].to_vec();
        assert_eq!(pixel(0, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(1, 1), [10, 20, 30, 255]);
        // Transparent index 1, and index 2 outside the palette.
        assert_eq!(pixel(2, 1), [0, 0, 0, 0]);
    }
}
