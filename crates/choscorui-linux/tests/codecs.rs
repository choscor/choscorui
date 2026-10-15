//! The image codecs decode every format of the macOS backend.
//!
//! Each fixture in `tests/fixtures` is 32 x 16 pixels with four 16 x 8
//! quadrants: red at the top left, green at the top right, blue at the
//! bottom left, and a fourth color at the bottom right. In the PNG, TIFF,
//! and WebP files it is (200, 100, 50) with alpha 128. In the GIF file it
//! is transparent, and in the JPEG and HEIC files it is opaque white.
//!
//! The fixtures came from an Ubuntu 22.04 container with `imagemagick`,
//! `webp`, `libheif-examples`, and `libimage-exiftool-perl`:
//!
//! ```text
//! convert alpha.pam -strip quadrants.png
//! convert alpha.pam -strip -compress lzw -define tiff:alpha=unassociated quadrants.tiff
//! cwebp -lossless -exact -metadata none alpha.png -o quadrants.webp
//! convert clear.pam -strip quadrants.gif
//! convert opaque.png -strip -quality 95 -sampling-factor 1x1 quadrants.jpg
//! exiftool -n -Orientation=6 quadrants-orientation-6.jpg   # a copy of quadrants.jpg
//! convert opaque.ppm -strip PNG24:opaque24.png
//! heif-enc -q 90 -o quadrants.heic opaque24.png
//! ```
//!
//! `heif-enc` 1.12 swaps the colors of a palette PNG, so the HEIC file
//! comes from a truecolor PNG.

#![cfg(target_os = "linux")]

use choscorui_core::{Image, ImageError, Support};
use choscorui_linux::{decode_image, encode_png, image_codecs_support};

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const HALF_ORANGE: [u8; 4] = [200, 100, 50, 128];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Asserts that each channel of the pixel at (x, y) is within `tolerance`
/// of `expected`. A transparent pixel matches any color.
fn assert_pixel(image: &Image, name: &str, (x, y): (u32, u32), expected: [u8; 4], tolerance: u8) {
    let actual = image.pixel(x, y).unwrap();
    let close = if expected[3] == 0 {
        actual[3] <= tolerance
    } else {
        actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= tolerance)
    };
    assert!(
        close,
        "{name}: pixel ({x}, {y}) is {actual:?}, expected {expected:?} within {tolerance}"
    );
}

/// Asserts the size and the color at the center of each quadrant.
fn assert_quadrants(name: &str, bottom_right: [u8; 4], tolerance: u8) {
    let image = decode_image(&fixture(name)).unwrap_or_else(|error| panic!("{name}: {error}"));
    assert_eq!((image.width(), image.height()), (32, 16), "{name}");
    assert_pixel(&image, name, (8, 4), RED, tolerance);
    assert_pixel(&image, name, (24, 4), GREEN, tolerance);
    assert_pixel(&image, name, (8, 12), BLUE, tolerance);
    assert_pixel(&image, name, (24, 12), bottom_right, tolerance);
}

#[test]
fn png_decodes_with_straight_alpha() {
    assert_quadrants("quadrants.png", HALF_ORANGE, 0);
}

#[test]
fn jpeg_decodes() {
    assert_quadrants("quadrants.jpg", WHITE, 8);
}

#[test]
fn gif_decodes_its_first_frame_with_transparency() {
    assert_quadrants("quadrants.gif", CLEAR, 0);
}

#[test]
fn tiff_decodes_with_straight_alpha() {
    // libtiff premultiplies the alpha, so the round trip can be off by one.
    assert_quadrants("quadrants.tiff", HALF_ORANGE, 1);
}

#[test]
fn webp_decodes_with_straight_alpha() {
    assert_quadrants("quadrants.webp", HALF_ORANGE, 0);
}

#[test]
fn heic_decodes() {
    assert_quadrants("quadrants.heic", WHITE, 8);
}

#[test]
fn jpeg_exif_orientation_rotates_the_image() {
    // Orientation 6: the stored image turns 90 degrees clockwise.
    let name = "quadrants-orientation-6.jpg";
    let image = decode_image(&fixture(name)).unwrap();
    assert_eq!((image.width(), image.height()), (16, 32));
    assert_pixel(&image, name, (4, 8), BLUE, 8);
    assert_pixel(&image, name, (12, 8), RED, 8);
    assert_pixel(&image, name, (4, 24), WHITE, 8);
    assert_pixel(&image, name, (12, 24), GREEN, 8);
}

#[test]
fn encode_png_round_trips() {
    let source = Image::from_fn(7, 5, |x, y| {
        [x as u8 * 30, y as u8 * 50, 200, (x * y) as u8 * 9]
    });
    let encoded = encode_png(&source).unwrap();
    assert!(encoded.starts_with(b"\x89PNG\r\n\x1a\n"));
    let decoded = decode_image(&encoded).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (7, 5));
    assert_eq!(decoded.pixels(), source.pixels());
}

#[test]
fn unknown_bytes_are_unsupported() {
    assert_eq!(decode_image(b""), Err(ImageError::UnsupportedFormat));
    assert_eq!(
        decode_image(b"BM\0\0\0\0 not an image"),
        Err(ImageError::UnsupportedFormat)
    );
}

#[test]
fn damaged_data_fails_without_a_panic() {
    let damaged: [&[u8]; 6] = [
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR garbage",
        &[0xFF, 0xD8, 0xFF, 0xE0, 0, 16, 1, 2, 3],
        b"GIF89a\x20\0\x10\0garbage",
        b"II*\0\x08\0\0\0\xff\xff",
        b"RIFF\x20\0\0\0WEBPVP8 garbage",
        b"\0\0\0\x18ftypheic\0\0\0\0mif1heic garbage",
    ];
    for bytes in damaged {
        assert_eq!(
            decode_image(bytes),
            Err(ImageError::DecodeFailed),
            "{bytes:?}"
        );
    }
    // A file cut in half.
    let png = fixture("quadrants.png");
    assert_eq!(
        decode_image(&png[..png.len() / 2]),
        Err(ImageError::DecodeFailed)
    );
}

#[test]
fn every_truncation_of_every_fixture_returns() {
    // A decoder can accept a short file with a warning, such as a JPEG
    // file without its end, so only the absence of a panic or a crash
    // counts here.
    for name in [
        "quadrants.png",
        "quadrants.jpg",
        "quadrants.gif",
        "quadrants.tiff",
        "quadrants.webp",
        "quadrants.heic",
    ] {
        let bytes = fixture(name);
        for length in 0..bytes.len() {
            let _ = decode_image(&bytes[..length]);
        }
    }
}

#[test]
fn png_and_jpeg_make_the_codecs_supported() {
    assert_eq!(image_codecs_support(), Support::Supported);
}

/// The name of the test that runs in a child process without libtiff.
const WITHOUT_LIBTIFF: &str = "a_missing_library_makes_only_its_format_unsupported";

#[test]
fn a_missing_library_makes_only_its_format_unsupported() {
    // A library loads one time for each process, so the check runs in a
    // child process that hides libtiff from its first load.
    if std::env::var_os("CHOSCORUI_TEST_HIDE_LIB").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                WITHOUT_LIBTIFF,
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("CHOSCORUI_TEST_HIDE_LIB", "libtiff")
            .status()
            .unwrap();
        assert!(status.success(), "the child process failed: {status}");
        return;
    }
    assert_eq!(
        decode_image(&fixture("quadrants.tiff")),
        Err(ImageError::UnsupportedFormat)
    );
    for name in [
        "quadrants.png",
        "quadrants.jpg",
        "quadrants.gif",
        "quadrants.webp",
    ] {
        assert!(decode_image(&fixture(name)).is_ok(), "{name}");
    }
    assert_eq!(image_codecs_support(), Support::Supported);
}
