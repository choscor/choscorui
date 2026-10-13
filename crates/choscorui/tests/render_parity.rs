//! The software renderer and the Core Graphics painter must agree on every
//! vector fixture at 1x and 2x. Antialiasing differs slightly between the two
//! rasterizers, so a small share of edge pixels may differ by more than the
//! per-channel tolerance; interiors must match.
#![cfg(target_os = "macos")]

mod parity;

use choscorui_core::{Color, Rect};
use choscorui_macos::MacBitmap;
use choscorui_render::{Raster, compare};
use parity::{HEIGHT, WIDTH, background, fixtures};

#[test]
fn software_and_core_graphics_agree_at_one_and_two_times_scale() {
    for (name, list) in fixtures() {
        for scale in [1.0, 2.0] {
            let width = (WIDTH * scale) as u32;
            let height = (HEIGHT * scale) as u32;
            let software = Raster::render(&list, width, height, scale).unwrap();
            let native = MacBitmap::render(&list, width, height, scale)
                .unwrap()
                .to_image()
                .unwrap();
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
    let native = MacBitmap::render(&list, WIDTH as u32, HEIGHT as u32, 1.0)
        .unwrap()
        .to_image()
        .unwrap();
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
