//! The vector fixtures of `crates/choscorui/tests/parity`, checked
//! against expected pixels computed from the core geometry: interiors
//! must match, and edges one device pixel wide may blend.

use super::*;
use choscorui_core::{GradientStop, ImageSampling, Point, Shadow};

const WIDTH: f64 = 64.0;
const HEIGHT: f64 = 48.0;

fn background() -> DisplayList {
    let mut list = DisplayList::new();
    list.fill_rect(Rect::from_xywh(0.0, 0.0, WIDTH, HEIGHT), Color::WHITE);
    list
}

fn render(list: &DisplayList, scale: f64) -> Image {
    render_offscreen(list, Size::new(WIDTH, HEIGHT), scale).unwrap()
}

fn pixel(image: &Image, x: f64, y: f64, scale: f64) -> [u8; 4] {
    image.pixel((x * scale) as u32, (y * scale) as u32).unwrap()
}

fn near(actual: [u8; 4], expected: [u8; 4], tolerance: u8, what: &str) {
    let worst = (0..4)
        .map(|i| actual[i].abs_diff(expected[i]))
        .max()
        .unwrap();
    assert!(
        worst <= tolerance,
        "{what}: {actual:?} is not {expected:?} within {tolerance}"
    );
}

fn rgba(color: Color) -> [u8; 4] {
    [color.red, color.green, color.blue, color.alpha]
}

/// `color` over opaque white.
fn over_white(color: [u8; 4]) -> [u8; 4] {
    let a = f64::from(color[3]) / 255.0;
    let mix = |c: u8| (f64::from(c) * a + 255.0 * (1.0 - a)).round() as u8;
    [mix(color[0]), mix(color[1]), mix(color[2]), 255]
}

/// Checks every device pixel whose center is clearly inside or outside
/// a shape: `expect` returns the color, or `None` near an edge.
fn check_all(image: &Image, scale: f64, tolerance: u8, expect: impl Fn(Point) -> Option<[u8; 4]>) {
    for y in 0..image.height() {
        for x in 0..image.width() {
            let point = Point::new((f64::from(x) + 0.5) / scale, (f64::from(y) + 0.5) / scale);
            if let Some(expected) = expect(point) {
                let actual = image.pixel(x, y).unwrap();
                near(actual, expected, tolerance, &format!("pixel {x},{y}"));
            }
        }
    }
}

const WHITE: [u8; 4] = [255, 255, 255, 255];

#[test]
fn rects_fill_and_stroke_on_pixel_edges() {
    let mut list = background();
    let red = Color::rgb8(220, 60, 80);
    let blue = Color::rgb8(30, 90, 200);
    list.fill_rect(Rect::from_xywh(4.0, 4.0, 20.0, 12.0), red);
    list.fill_rounded_rect(Rect::from_xywh(30.0, 6.0, 28.0, 30.0), 8.0, blue)
        .unwrap();
    list.stroke_rect(Rect::from_xywh(6.0, 24.0, 16.0, 16.0), Color::BLACK, 2.0)
        .unwrap();
    for scale in [1.0, 2.0] {
        let image = render(&list, scale);
        near(pixel(&image, 4.0, 4.0, scale), rgba(red), 0, "red corner");
        near(
            pixel(&image, 23.5, 15.5, scale),
            rgba(red),
            0,
            "red far corner",
        );
        near(pixel(&image, 3.5, 8.0, scale), WHITE, 0, "left of red");
        near(
            pixel(&image, 44.0, 21.0, scale),
            rgba(blue),
            0,
            "blue center",
        );
        near(pixel(&image, 30.2, 6.2, scale), WHITE, 0, "rounded corner");
        near(
            pixel(&image, 5.0, 30.0, scale),
            [0, 0, 0, 255],
            0,
            "stroke outer half",
        );
        near(
            pixel(&image, 6.5, 30.0, scale),
            [0, 0, 0, 255],
            0,
            "stroke inner half",
        );
        near(
            pixel(&image, 14.0, 32.0, scale),
            WHITE,
            0,
            "stroke interior",
        );
        near(
            pixel(&image, 4.5, 30.0, scale),
            WHITE,
            0,
            "outside the stroke",
        );
    }
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

#[test]
fn gradients_follow_the_core_colors() {
    let linear = Paint::LinearGradient {
        start: Point::new(6.0, 0.0),
        end: Point::new(58.0, 0.0),
        stops: stops(Color::rgb8(0, 120, 255), Color::rgb8(255, 200, 0)),
    };
    let radial = Paint::RadialGradient {
        center: Point::new(32.0, 24.0),
        radius: 22.0,
        stops: stops(Color::rgb8(255, 255, 255), Color::rgb8(20, 20, 60)),
    };
    for paint in [linear, radial] {
        let mut list = background();
        list.fill_path(
            Path::rect(Rect::from_xywh(0.0, 0.0, WIDTH, HEIGHT)),
            paint.clone(),
            FillRule::NonZero,
        )
        .unwrap();
        for scale in [1.0, 2.0] {
            let image = render(&list, scale);
            check_all(&image, scale, 4, |point| Some(rgba(paint.color_at(point))));
        }
    }
}

#[test]
fn a_round_capped_stroke_covers_its_width() {
    let mut list = background();
    let mut style = StrokeStyle::new(4.0);
    style.cap = LineCap::Round;
    let green = Color::rgb8(10, 120, 60);
    let (a, b) = (Point::new(8.0, 8.0), Point::new(56.0, 40.0));
    list.stroke_path(Path::line(a, b), Paint::Solid(green), style)
        .unwrap();
    let distance = |p: Point| {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        p.distance(Point::new(a.x + dx * t, a.y + dy * t))
    };
    for scale in [1.0, 2.0] {
        let image = render(&list, scale);
        check_all(&image, scale, 2, |point| {
            let d = distance(point);
            if d < 2.0 - 0.75 / scale {
                Some(rgba(green))
            } else if d > 2.0 + 0.75 / scale {
                Some(WHITE)
            } else {
                None
            }
        });
    }
}

#[test]
fn an_opacity_group_composites_as_one_layer() {
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
    for scale in [1.0, 2.0] {
        let image = render(&list, scale);
        near(
            pixel(&image, 12.0, 12.0, scale),
            [228, 128, 128, 255],
            2,
            "red alone",
        );
        near(
            pixel(&image, 30.0, 20.0, scale),
            [128, 128, 228, 255],
            2,
            "overlap",
        );
        near(pixel(&image, 2.0, 2.0, scale), WHITE, 0, "outside");
    }
}

#[test]
fn a_transformed_clip_path_bounds_the_fill() {
    let transform = Transform::rotate(0.3).then(Transform::translate(32.0, 8.0));
    let ellipse = Rect::from_xywh(-14.0, 0.0, 28.0, 28.0);
    let purple = Color::rgb8(120, 40, 160);
    let mut list = background();
    list.push_transform(transform).unwrap();
    list.push_clip_path(Path::ellipse(ellipse)).unwrap();
    list.fill_rect(Rect::from_xywh(-20.0, 0.0, 40.0, 40.0), purple);
    list.pop_clip().unwrap();
    list.pop_transform().unwrap();
    let inverse = transform.invert().unwrap();
    for scale in [1.0, 2.0] {
        let image = render(&list, scale);
        check_all(&image, scale, 2, |point| {
            let local = inverse.apply(point);
            let d = local.distance(ellipse.center()) - 14.0;
            if d < -1.0 / scale {
                Some(rgba(purple))
            } else if d > 1.0 / scale {
                Some(WHITE)
            } else {
                None
            }
        });
    }
}

#[test]
fn nearest_images_keep_their_texels_and_linear_images_pad_their_edges() {
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
    let rect = Rect::from_xywh(16.0, 8.0, 32.0, 32.0);
    let mut list = background();
    list.draw_image(image.clone(), rect, ImageSampling::Nearest)
        .unwrap();
    let flat = Image::from_rgba8(1, 1, vec![40, 80, 120, 255]).unwrap();
    list.draw_image(
        flat,
        Rect::from_xywh(0.0, 0.0, 8.0, 8.0),
        ImageSampling::Linear,
    )
    .unwrap();
    for scale in [1.0, 2.0] {
        let rendered = render(&list, scale);
        for row in 0..8u32 {
            for column in 0..8u32 {
                let x = rect.min_x() + f64::from(column) * 4.0 + 2.0;
                let y = rect.min_y() + f64::from(row) * 4.0 + 2.0;
                let expected = image.pixel(column, row).unwrap();
                near(pixel(&rendered, x, y, scale), expected, 0, "texel");
            }
        }
        near(
            pixel(&rendered, 0.0, 0.0, scale),
            [40, 80, 120, 255],
            1,
            "padded edge",
        );
        near(
            pixel(&rendered, 7.6, 7.6, scale),
            [40, 80, 120, 255],
            1,
            "padded edge",
        );
    }
}

#[test]
fn shadows_draw_the_core_mask() {
    let card = Rect::from_xywh(14.0, 10.0, 36.0, 24.0);
    let shadow = Shadow::drop(Point::new(0.0, 3.0), 4.0, Color::BLACK.with_alpha(0.4));
    let mut list = background();
    list.draw_shadow(card, 6.0, shadow).unwrap();
    for scale in [1.0, 2.0] {
        let image = render(&list, scale);
        let mask = shadow.mask(card, 6.0, scale).unwrap();
        let bounds = shadow.bounds(card);
        check_all(&image, scale, 2, |point| {
            if !bounds.contains(point) {
                return Some(WHITE);
            }
            let x = ((point.x - bounds.min_x()) * scale).floor() as u32;
            let y = ((point.y - bounds.min_y()) * scale).floor() as u32;
            Some(over_white(mask.pixel(x, y)?))
        });
    }
}

#[test]
fn backdrop_blur_reads_beneath_an_opacity_group() {
    for opacity in [None, Some(0.5)] {
        let mut list = background();
        list.fill_rect(Rect::from_xywh(0.0, 0.0, 32.0, HEIGHT), Color::BLACK);
        if let Some(opacity) = opacity {
            list.push_opacity(opacity).unwrap();
        }
        list.backdrop_blur(Rect::from_xywh(16.0, 8.0, 32.0, 32.0), 0.0, 4.0)
            .unwrap();
        if opacity.is_some() {
            list.pop_opacity().unwrap();
        }
        for scale in [1.0, 2.0] {
            let image = render(&list, scale);
            // At the black and white edge, the blur leaves a middle gray.
            // A group at half opacity mixes it with the sharp white.
            let [gray, ..] = pixel(&image, 32.0, 24.0, scale);
            let range = if opacity.is_some() { 160..224 } else { 64..192 };
            assert!(range.contains(&gray), "{gray}");
            // Deep inside each side, the blur keeps the color.
            near(
                pixel(&image, 18.0, 24.0, scale),
                [0, 0, 0, 255],
                2,
                "black side",
            );
            near(pixel(&image, 46.0, 24.0, scale), WHITE, 2, "white side");
            // Outside the blurred rectangle, the edge stays sharp.
            near(pixel(&image, 32.0, 4.0, scale), WHITE, 0, "sharp edge");
        }
    }
}

#[test]
fn transforms_compose_with_the_scale() {
    let mut list = background();
    list.push_transform(Transform::translate(4.0, 4.0)).unwrap();
    list.push_transform(Transform::scale(2.0, 2.0)).unwrap();
    list.fill_rect(Rect::from_xywh(1.0, 1.0, 2.0, 2.0), Color::BLACK);
    list.pop_transform().unwrap();
    list.pop_transform().unwrap();
    for scale in [1.0, 2.0] {
        let image = render(&list, scale);
        check_all(&image, scale, 0, |point| {
            let inside = (6.0..10.0).contains(&point.x) && (6.0..10.0).contains(&point.y);
            Some(if inside { [0, 0, 0, 255] } else { WHITE })
        });
    }
}

#[test]
fn a_clip_limits_painting_and_clears_first() {
    let (width, height) = (16u32, 8u32);
    let stride = Format::Argb32.stride_for_width(width).unwrap();
    let mut pixels = vec![0x55u8; stride * height as usize];
    let mut list = DisplayList::new();
    list.fill_rect(
        Rect::from_xywh(0.0, 0.0, 16.0, 8.0),
        Color::rgba8(255, 0, 0, 128),
    );
    let clip = Rect::from_xywh(2.2, 1.0, 4.0, 2.0);
    paint_into(&list, &mut pixels, width, height, stride, 1.0, Some(clip)).unwrap();
    let word = |pixels: &[u8], x: usize, y: usize| {
        let at = y * stride + x * 4;
        u32::from_ne_bytes(pixels[at..at + 4].try_into().unwrap())
    };
    // The clip covers whole device pixels 2..7 and 1..3.
    assert_eq!(word(&pixels, 2, 1), 0x8080_0000);
    assert_eq!(word(&pixels, 6, 2), 0x8080_0000);
    assert_eq!(word(&pixels, 1, 1), 0x5555_5555);
    assert_eq!(word(&pixels, 7, 1), 0x5555_5555);
    assert_eq!(word(&pixels, 3, 3), 0x5555_5555);

    let empty = DisplayList::new();
    paint_into(&empty, &mut pixels, width, height, stride, 2.0, Some(clip)).unwrap();
    assert_eq!(word(&pixels, 5, 2), 0);
    assert_eq!(word(&pixels, 0, 0), 0x5555_5555);
}

#[test]
fn an_origin_moves_the_list_and_its_clip_into_the_margin() {
    // A surface with a margin of 3 points at the left and 2 at the top,
    // at scale 2: the content starts at device pixel (6, 4).
    let (width, height) = (24u32, 16u32);
    let stride = Format::Argb32.stride_for_width(width).unwrap();
    let mut pixels = vec![0x55u8; stride * height as usize];
    let mut list = DisplayList::new();
    // The shadow of a frame paints at negative content coordinates.
    list.fill_rect(Rect::from_xywh(-3.0, -2.0, 12.0, 8.0), Color::BLACK);
    list.fill_rect(Rect::from_xywh(0.0, 0.0, 2.0, 2.0), Color::WHITE);
    let clip = Rect::from_xywh(-3.0, -2.0, 6.0, 4.0);
    let origin = Point::new(3.0, 2.0);
    let size = (width, height, stride);
    paint_into_at(&list, &mut pixels, size, 2.0, Some(clip), origin).unwrap();
    let word = |pixels: &[u8], x: usize, y: usize| {
        let at = y * stride + x * 4;
        u32::from_ne_bytes(pixels[at..at + 4].try_into().unwrap())
    };
    assert_eq!(word(&pixels, 0, 0), 0xFF00_0000, "the margin");
    assert_eq!(word(&pixels, 6, 4), 0xFFFF_FFFF, "the content origin");
    assert_eq!(word(&pixels, 9, 7), 0xFFFF_FFFF);
    assert_eq!(word(&pixels, 10, 7), 0xFF00_0000);
    // The clip moves with the list: outside it, pixels keep their value.
    assert_eq!(word(&pixels, 12, 4), 0x5555_5555);
    assert_eq!(word(&pixels, 6, 8), 0x5555_5555);
}

#[test]
fn a_bad_buffer_or_scale_is_an_error() {
    let list = background();
    let mut small = vec![0u8; 16];
    assert_eq!(
        paint_into(&list, &mut small, 4, 4, 16, 1.0, None),
        Err(CairoError::InvalidBuffer)
    );
    assert_eq!(
        paint_into(&list, &mut small, 4, 1, 15, 1.0, None),
        Err(CairoError::InvalidBuffer)
    );
    assert_eq!(
        render_offscreen(&list, Size::new(4.0, 4.0), 0.0).err(),
        Some(CairoError::InvalidScale)
    );
    assert_eq!(
        render_offscreen(&list, Size::new(0.0, 4.0), 1.0).err(),
        Some(CairoError::InvalidBuffer)
    );
}

#[test]
fn premultiplied_words_become_straight_rgba() {
    assert_eq!(straight_rgba(0x8040_2000), [128, 64, 0, 128]);
    assert_eq!(straight_rgba(0), [0, 0, 0, 0]);
    assert_eq!(straight_rgba(0xFF10_2030), [0x10, 0x20, 0x30, 255]);
}
