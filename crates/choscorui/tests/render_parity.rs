//! The software renderer and the Core Graphics painter must agree on every
//! vector fixture at 1x and 2x. Antialiasing differs slightly between the two
//! rasterizers, so a small share of edge pixels may differ by more than the
//! per-channel tolerance; interiors must match.
#![cfg(target_os = "macos")]

use choscorui_core::{
    Color, DisplayList, FillRule, GradientStop, Image, ImageSampling, LineCap, Paint, Path, Point,
    Rect, Shadow, StrokeStyle, Transform,
};
use choscorui_macos::MacBitmap;
use choscorui_render::{Raster, compare};

const WIDTH: f64 = 64.0;
const HEIGHT: f64 = 48.0;

fn background() -> DisplayList {
    let mut list = DisplayList::new();
    list.fill_rect(Rect::from_xywh(0.0, 0.0, WIDTH, HEIGHT), Color::WHITE);
    list
}

fn stops(from: Color, to: Color) -> Vec<GradientStop> {
    vec![
        GradientStop {
            offset: 0.0,
            color: from,
        },
        GradientStop {
            offset: 1.0,
            color: to,
        },
    ]
}

fn fixtures() -> Vec<(&'static str, DisplayList)> {
    let mut out = Vec::new();

    let mut list = background();
    list.fill_rect(
        Rect::from_xywh(4.0, 4.0, 20.0, 12.0),
        Color::rgb8(220, 60, 80),
    );
    list.fill_rounded_rect(
        Rect::from_xywh(30.0, 6.0, 28.0, 30.0),
        8.0,
        Color::rgb8(30, 90, 200),
    )
    .unwrap();
    list.stroke_rect(Rect::from_xywh(6.0, 24.0, 16.0, 16.0), Color::BLACK, 2.0)
        .unwrap();
    out.push(("rects", list));

    let mut list = background();
    list.fill_path(
        Path::ellipse(Rect::from_xywh(6.0, 6.0, 52.0, 36.0)),
        Paint::LinearGradient {
            start: Point::new(6.0, 0.0),
            end: Point::new(58.0, 0.0),
            stops: stops(Color::rgb8(0, 120, 255), Color::rgb8(255, 200, 0)),
        },
        FillRule::NonZero,
    )
    .unwrap();
    out.push(("linear-gradient-ellipse", list));

    let mut list = background();
    list.fill_path(
        Path::rect(Rect::from_xywh(0.0, 0.0, WIDTH, HEIGHT)),
        Paint::RadialGradient {
            center: Point::new(32.0, 24.0),
            radius: 22.0,
            stops: stops(Color::rgb8(255, 255, 255), Color::rgb8(20, 20, 60)),
        },
        FillRule::NonZero,
    )
    .unwrap();
    out.push(("radial-gradient", list));

    let mut list = background();
    let mut style = StrokeStyle::new(4.0);
    style.cap = LineCap::Round;
    list.stroke_path(
        Path::line(Point::new(8.0, 8.0), Point::new(56.0, 40.0)),
        Paint::Solid(Color::rgb8(10, 120, 60)),
        style,
    )
    .unwrap();
    out.push(("stroke", list));

    let mut list = background();
    list.push_opacity(0.5).unwrap();
    list.fill_rect(
        Rect::from_xywh(8.0, 8.0, 30.0, 24.0),
        Color::rgb8(200, 0, 0),
    );
    list.fill_rect(
        Rect::from_xywh(24.0, 16.0, 30.0, 24.0),
        Color::rgb8(0, 0, 200),
    );
    list.pop_opacity().unwrap();
    out.push(("group-opacity", list));

    let mut list = background();
    list.push_transform(Transform::rotate(0.3).then(Transform::translate(32.0, 8.0)))
        .unwrap();
    list.push_clip_path(Path::ellipse(Rect::from_xywh(-14.0, 0.0, 28.0, 28.0)))
        .unwrap();
    list.fill_rect(
        Rect::from_xywh(-20.0, 0.0, 40.0, 40.0),
        Color::rgb8(120, 40, 160),
    );
    list.pop_clip().unwrap();
    list.pop_transform().unwrap();
    out.push(("transform-clip-path", list));

    let mut list = background();
    let checker: Vec<u8> = (0..8 * 8)
        .flat_map(|i| {
            if (i / 8 + i % 8) % 2 == 0 {
                [0, 0, 0, 255]
            } else {
                [255, 160, 0, 255]
            }
        })
        .collect();
    let image = Image::from_rgba8(8, 8, checker).unwrap();
    list.draw_image(
        image,
        Rect::from_xywh(16.0, 8.0, 32.0, 32.0),
        ImageSampling::Nearest,
    )
    .unwrap();
    out.push(("image-nearest", list));

    let mut list = background();
    let card = Rect::from_xywh(14.0, 10.0, 36.0, 24.0);
    list.draw_shadow(
        card,
        6.0,
        Shadow {
            offset: Point::new(0.0, 3.0),
            blur: 4.0,
            color: Color::BLACK.with_alpha(0.4),
        },
    )
    .unwrap();
    list.fill_rounded_rect(card, 6.0, Color::WHITE).unwrap();
    out.push(("shadow", list));

    out
}

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
