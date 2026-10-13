//! The vector fixtures that every painter must draw like the software
//! renderer, at `WIDTH × HEIGHT` points.

use choscorui::controls::{Corners, ResolvedStyle, Ring, paint_box};
use choscorui_core::{
    Color, DesignTokens, DisplayList, FillRule, GradientStop, Image, ImageSampling, LineCap, Paint,
    Path, Point, Rect, Shadow, ShadowScale, StrokeStyle, Transform,
};

pub const WIDTH: f64 = 64.0;
pub const HEIGHT: f64 = 48.0;

pub fn background() -> DisplayList {
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

pub fn fixtures() -> Vec<(&'static str, DisplayList)> {
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
        Shadow::drop(Point::new(0.0, 3.0), 4.0, Color::BLACK.with_alpha(0.4)),
    )
    .unwrap();
    list.fill_rounded_rect(card, 6.0, Color::WHITE).unwrap();
    out.push(("shadow", list));

    let mut list = background();
    list.draw_shadows(card, 8.0, &ShadowScale::Lg.layers())
        .unwrap();
    list.fill_rounded_rect(card, 8.0, Color::WHITE).unwrap();
    out.push(("shadow-layers-spread", list));

    let mut list = background();
    list.fill_rounded_rect(card, 8.0, Color::WHITE).unwrap();
    list.draw_shadow(
        card,
        8.0,
        Shadow {
            offset: Point::new(0.0, 2.0),
            blur: 4.0,
            spread: 1.0,
            color: Color::BLACK.with_alpha(0.6),
            inset: true,
        },
    )
    .unwrap();
    out.push(("shadow-inset", list));

    let mut list = background();
    let button = Rect::from_xywh(16.0, 12.0, 32.0, 20.0);
    list.fill_rounded_rect(button, 6.0, Color::WHITE).unwrap();
    list.draw_ring(button, 6.0, 3.0, Color::rgba8(30, 90, 200, 128))
        .unwrap();
    out.push(("focus-ring", list));

    let stripes = || {
        let mut list = background();
        for index in 0..8 {
            let x = f64::from(index) * 8.0;
            list.fill_rect(
                Rect::from_xywh(x, 0.0, 4.0, HEIGHT),
                Color::rgba8(30, 90, 200, 255),
            );
        }
        list
    };
    let panel = Rect::from_xywh(12.0, 8.0, 40.0, 32.0);
    let mut list = stripes();
    list.backdrop_blur(panel, 6.0, 4.0).unwrap();
    list.fill_rounded_rect(panel, 6.0, Color::BLACK.with_alpha(0.1))
        .unwrap();
    out.push(("backdrop-blur", list));

    let mut list = stripes();
    list.push_opacity(0.5).unwrap();
    list.backdrop_blur(panel, 0.0, 2.0).unwrap();
    list.pop_opacity().unwrap();
    out.push(("backdrop-blur-opacity", list));

    out.extend(corner_fixtures(stripes()));
    out
}

/// Shadows, blur, a ring, and a gradient on shapes with a radius per
/// corner.
fn corner_fixtures(stripes: DisplayList) -> Vec<(&'static str, DisplayList)> {
    let mut out = Vec::new();
    let card = Rect::from_xywh(14.0, 10.0, 36.0, 24.0);
    let radii = [10.0, 0.0, 6.0, 2.0];
    let shape = Path::rounded_rect_corners(card, radii);
    let white = Paint::Solid(Color::WHITE);

    let mut list = background();
    let drop = Shadow {
        spread: 1.0,
        ..Shadow::drop(Point::new(0.0, 3.0), 4.0, Color::BLACK.with_alpha(0.4))
    };
    list.draw_shadow_corners(card, radii, drop).unwrap();
    list.fill_path(shape.clone(), white.clone(), FillRule::NonZero)
        .unwrap();
    out.push(("shadow-corners", list));

    let mut list = background();
    list.fill_path(shape, white, FillRule::NonZero).unwrap();
    let inset = Shadow {
        offset: Point::new(0.0, 2.0),
        blur: 4.0,
        spread: 1.0,
        color: Color::BLACK.with_alpha(0.6),
        inset: true,
    };
    list.draw_shadow_corners(card, radii, inset).unwrap();
    out.push(("shadow-corners-inset", list));

    let mut list = stripes;
    let panel = Rect::from_xywh(12.0, 8.0, 40.0, 32.0);
    list.backdrop_blur_corners(panel, [12.0, 4.0, 0.0, 8.0], 4.0)
        .unwrap();
    out.push(("backdrop-blur-corners", list));

    let mut list = background();
    let mut style = ResolvedStyle::new(&DesignTokens::light());
    style.radius = Corners {
        top_start: 8.0,
        top_end: 2.0,
        bottom_end: 8.0,
        bottom_start: 0.0,
    };
    style.background = Paint::LinearGradient {
        start: Point::new(0.0, 0.0),
        end: Point::new(1.0, 1.0),
        stops: stops(
            Color::rgba8(30, 90, 200, 255),
            Color::rgba8(200, 60, 90, 255),
        ),
    };
    style.shadow = ShadowScale::Md.layers();
    style.ring = Some(Ring {
        width: 3.0,
        color: Color::rgba8(30, 90, 200, 128),
    });
    paint_box(
        &mut list,
        Rect::from_xywh(16.0, 12.0, 32.0, 20.0),
        &style,
        true,
    );
    out.push(("slot-corners-gradient", list));
    out
}
