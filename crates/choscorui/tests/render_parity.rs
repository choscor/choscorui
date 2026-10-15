//! The software renderer and the native painter must agree on every
//! vector fixture at 1x and 2x. The native painter is Core Graphics on
//! macOS, and the Cairo painter of the offscreen export on Linux, which
//! needs no display server. Antialiasing differs slightly between the
//! rasterizers, so a small share of edge pixels may differ by more than the
//! per-channel tolerance; interiors must match.
#![cfg(any(target_os = "macos", target_os = "linux"))]

mod parity;

use choscorui_core::{Color, DisplayList, Image, Rect};
use choscorui_render::{Raster, compare};
use parity::{HEIGHT, WIDTH, background, fixtures};

/// Renders `list` with Core Graphics into a `width × height` bitmap.
#[cfg(target_os = "macos")]
fn native(list: &DisplayList, width: u32, height: u32, scale: f64) -> Image {
    choscorui_macos::MacBitmap::render(list, width, height, scale)
        .unwrap()
        .to_image()
        .unwrap()
}

/// Renders `list` with the Cairo painter into a `width × height` image.
#[cfg(target_os = "linux")]
fn native(list: &DisplayList, width: u32, height: u32, scale: f64) -> Image {
    let size = choscorui_core::Size::new(f64::from(width) / scale, f64::from(height) / scale);
    let image = choscorui_linux::render_offscreen(list, size, scale).unwrap();
    assert_eq!((image.width(), image.height()), (width, height));
    image
}

#[test]
fn software_and_native_painters_agree_at_one_and_two_times_scale() {
    for (name, list) in fixtures() {
        for scale in [1.0, 2.0] {
            let width = (WIDTH * scale) as u32;
            let height = (HEIGHT * scale) as u32;
            let software = Raster::render(&list, width, height, scale).unwrap();
            let native = native(&list, width, height, scale);
            let (worst, share) = compare(software.pixels(), native.pixels(), 24).unwrap();
            println!("{name} @{scale}x: worst {worst}, share {share:.4}");
            assert!(
                share < 0.005,
                "{name} at {scale}x: {:.2}% of pixels differ (worst {worst})",
                share * 100.0
            );
        }
    }
}

#[test]
fn backdrop_blur_softens_what_is_beneath() {
    let mut list = background();
    list.fill_rect(Rect::from_xywh(0.0, 0.0, 32.0, HEIGHT), Color::BLACK);
    list.backdrop_blur(Rect::from_xywh(16.0, 8.0, 32.0, 32.0), 0.0, 4.0)
        .unwrap();
    let native = native(&list, WIDTH as u32, HEIGHT as u32, 1.0);
    let software = Raster::render(&list, WIDTH as u32, HEIGHT as u32, 1.0).unwrap();
    for pixels in [native.pixels(), software.pixels()] {
        // At the black and white edge, the blur leaves a middle gray.
        let at = (24 * WIDTH as usize + 32) * 4;
        assert!((64..192).contains(&pixels[at]), "{}", pixels[at]);
        // Outside the blurred rectangle, the edge stays sharp.
        let outside = (4 * WIDTH as usize + 32) * 4;
        assert_eq!(pixels[outside], 255);
    }
}
