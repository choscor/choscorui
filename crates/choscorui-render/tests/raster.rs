use choscorui_core::{Color, DisplayList, Point, Rect, Size};
use choscorui_render::Raster;

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn ordered_fills_and_clip_match_known_pixels_at_one_and_two_times_scale() {
    let mut list = DisplayList::new();
    list.fill_rect(rect(0.0, 0.0, 4.0, 4.0), Color::rgba8(255, 0, 0, 255));
    list.push_clip(rect(1.0, 1.0, 2.0, 2.0));
    list.fill_rect(rect(0.0, 0.0, 4.0, 4.0), Color::rgba8(0, 0, 255, 255));
    list.pop_clip().unwrap();

    let one = Raster::render(&list, 4, 4, 1.0).unwrap();
    assert_eq!(one.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(one.pixel(1, 1), Some([0, 0, 255, 255]));
    assert_eq!(one.pixel(2, 2), Some([0, 0, 255, 255]));
    assert_eq!(one.pixel(3, 3), Some([255, 0, 0, 255]));

    let two = Raster::render(&list, 8, 8, 2.0).unwrap();
    assert_eq!(two.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(two.pixel(2, 2), Some([0, 0, 255, 255]));
    assert_eq!(two.pixel(5, 5), Some([0, 0, 255, 255]));
    assert_eq!(two.pixel(6, 6), Some([255, 0, 0, 255]));
}

#[test]
fn source_over_alpha_uses_literal_expected_pixel() {
    let mut list = DisplayList::new();
    list.fill_rect(rect(0.0, 0.0, 1.0, 1.0), Color::rgba8(255, 0, 0, 255));
    list.fill_rect(rect(0.0, 0.0, 1.0, 1.0), Color::rgba8(0, 0, 255, 128));
    let image = Raster::render(&list, 1, 1, 1.0).unwrap();
    assert_eq!(image.pixel(0, 0), Some([127, 0, 128, 255]));
}

#[test]
fn invalid_scale_and_unbalanced_clip_are_rejected() {
    let list = DisplayList::new();
    assert!(Raster::render(&list, 1, 1, 0.0).is_err());
    assert!(Raster::render(&list, 1, 1, f64::NAN).is_err());
    let mut list = DisplayList::new();
    assert!(list.pop_clip().is_err());
}

#[test]
fn empty_surface_has_no_pixels_even_with_large_other_dimension() {
    let mut list = DisplayList::new();
    list.fill_rect(rect(0.0, 0.0, 1.0, 1.0), Color::rgba8(255, 0, 0, 255));
    let image = Raster::render(&list, 0, u32::MAX, 1.0).unwrap();
    assert_eq!(image.width(), 0);
    assert!(image.pixels().is_empty());
}

#[test]
fn text_command_reports_unsupported_in_software_reference() {
    let mut list = DisplayList::new();
    list.draw_text(
        Point::new(0.0, 10.0),
        "hello",
        "Helvetica",
        12.0,
        Color::rgba8(0, 0, 0, 255),
    )
    .unwrap();
    assert!(matches!(
        Raster::render(&list, 100, 20, 1.0),
        Err(choscorui_render::RenderError::UnsupportedText)
    ));
}

#[test]
fn strokes_and_rounded_corners_match_known_pixels() {
    let mut list = DisplayList::new();
    list.stroke_rect(rect(1.0, 1.0, 4.0, 4.0), Color::rgba8(255, 0, 0, 255), 2.0)
        .unwrap();
    let image = Raster::render(&list, 6, 6, 1.0).unwrap();
    assert_eq!(image.pixel(0, 2), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(2, 2), Some([0, 0, 0, 0]));

    let mut list = DisplayList::new();
    list.fill_rounded_rect(rect(0.0, 0.0, 5.0, 5.0), 2.0, Color::rgba8(0, 255, 0, 255))
        .unwrap();
    let image = Raster::render(&list, 5, 5, 1.0).unwrap();
    // Antialiased corner: the arc leaves most of the corner pixel empty.
    let corner = image.pixel(0, 0).unwrap();
    assert!(corner[3] > 0 && corner[3] < 128, "{corner:?}");
    assert_eq!(image.pixel(2, 2), Some([0, 255, 0, 255]));

    let mut list = DisplayList::new();
    list.stroke_rounded_rect(
        rect(0.0, 0.0, 5.0, 5.0),
        2.0,
        Color::rgba8(0, 0, 255, 255),
        1.0,
    )
    .unwrap();
    let image = Raster::render(&list, 5, 5, 1.0).unwrap();
    // A one-point stroke centered on the top edge covers half of row 0.
    let edge = image.pixel(2, 0).unwrap();
    assert_eq!(&edge[..3], &[0, 0, 255]);
    assert!((edge[3] as i32 - 128).abs() <= 2, "{edge:?}");
    assert_eq!(image.pixel(2, 2), Some([0, 0, 0, 0]));
}

#[test]
fn nested_opacity_multiplies_source_alpha() {
    let mut list = DisplayList::new();
    list.push_opacity(0.5).unwrap();
    list.push_opacity(0.5).unwrap();
    list.fill_rect(rect(0.0, 0.0, 1.0, 1.0), Color::rgba8(20, 40, 60, 255));
    list.pop_opacity().unwrap();
    list.pop_opacity().unwrap();
    let image = Raster::render(&list, 1, 1, 1.0).unwrap();
    assert_eq!(image.pixel(0, 0), Some([20, 40, 60, 64]));
}

mod vector {
    use choscorui_core::{
        Color, DisplayList, FillRule, GradientStop, Image, ImageSampling, LineCap, Paint, Path,
        Point, Rect, Shadow, StrokeStyle, Transform,
    };
    use choscorui_render::Raster;

    fn alpha(raster: &Raster, x: u32, y: u32) -> u8 {
        raster.pixel(x, y).unwrap()[3]
    }

    #[test]
    fn triangle_fill_and_even_odd_hole() {
        let mut list = DisplayList::new();
        list.fill_path(
            Path::polygon(&[
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(0.0, 10.0),
            ]),
            Paint::Solid(Color::BLACK),
            FillRule::NonZero,
        )
        .unwrap();
        let r = Raster::render(&list, 10, 10, 1.0).unwrap();
        assert_eq!(alpha(&r, 1, 1), 255);
        assert_eq!(alpha(&r, 8, 8), 0);
        let mut rings = Path::rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0));
        rings.extend(&Path::rect(Rect::from_xywh(3.0, 3.0, 4.0, 4.0)));
        let mut list = DisplayList::new();
        list.fill_path(rings, Paint::Solid(Color::BLACK), FillRule::EvenOdd)
            .unwrap();
        let r = Raster::render(&list, 10, 10, 1.0).unwrap();
        assert_eq!(alpha(&r, 5, 5), 0);
        assert_eq!(alpha(&r, 1, 5), 255);
    }

    #[test]
    fn gradients_images_and_transforms() {
        let mut list = DisplayList::new();
        list.fill_path(
            Path::rect(Rect::from_xywh(0.0, 0.0, 10.0, 1.0)),
            Paint::LinearGradient {
                start: Point::new(0.0, 0.0),
                end: Point::new(10.0, 0.0),
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: Color::rgb8(0, 0, 0),
                    },
                    GradientStop {
                        offset: 1.0,
                        color: Color::rgb8(250, 0, 0),
                    },
                ],
            },
            FillRule::NonZero,
        )
        .unwrap();
        let r = Raster::render(&list, 10, 1, 1.0).unwrap();
        assert!(r.pixel(0, 0).unwrap()[0] < 20);
        assert!((i32::from(r.pixel(5, 0).unwrap()[0]) - 137).abs() <= 3);
        let image = Image::from_rgba8(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
        let mut list = DisplayList::new();
        list.push_transform(Transform::translate(2.0, 0.0)).unwrap();
        list.draw_image(
            image,
            Rect::from_xywh(0.0, 0.0, 4.0, 2.0),
            ImageSampling::Nearest,
        )
        .unwrap();
        list.pop_transform().unwrap();
        let r = Raster::render(&list, 8, 2, 1.0).unwrap();
        assert_eq!(r.pixel(1, 0).unwrap()[3], 0);
        assert_eq!(r.pixel(2, 0).unwrap(), [255, 0, 0, 255]);
        assert_eq!(r.pixel(5, 1).unwrap(), [0, 0, 255, 255]);
    }

    #[test]
    fn group_opacity_composites_overlapping_draws_once() {
        let mut list = DisplayList::new();
        list.push_opacity(0.5).unwrap();
        list.fill_rect(Rect::from_xywh(0.0, 0.0, 2.0, 1.0), Color::rgb8(255, 0, 0));
        list.fill_rect(Rect::from_xywh(1.0, 0.0, 2.0, 1.0), Color::rgb8(0, 0, 255));
        list.pop_opacity().unwrap();
        let r = Raster::render(&list, 3, 1, 1.0).unwrap();
        // Overlap shows only the top draw at half alpha, not a red/blue mix.
        assert_eq!(r.pixel(1, 0).unwrap(), [0, 0, 255, 128]);
    }

    #[test]
    fn clip_paths_dashes_caps_and_shadows() {
        let mut list = DisplayList::new();
        list.push_clip_path(Path::ellipse(Rect::from_xywh(0.0, 0.0, 10.0, 10.0)))
            .unwrap();
        list.fill_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), Color::BLACK);
        list.pop_clip().unwrap();
        let r = Raster::render(&list, 10, 10, 1.0).unwrap();
        assert_eq!(alpha(&r, 0, 0), 0);
        assert_eq!(alpha(&r, 5, 5), 255);

        let mut style = StrokeStyle::new(2.0);
        style.dash = vec![2.0, 2.0];
        let mut list = DisplayList::new();
        list.stroke_path(
            Path::line(Point::new(0.0, 1.0), Point::new(8.0, 1.0)),
            Paint::Solid(Color::BLACK),
            style,
        )
        .unwrap();
        let r = Raster::render(&list, 8, 2, 1.0).unwrap();
        assert_eq!(alpha(&r, 0, 0), 255);
        assert_eq!(alpha(&r, 2, 0), 0);
        assert_eq!(alpha(&r, 4, 0), 255);

        let mut round = StrokeStyle::new(4.0);
        round.cap = LineCap::Round;
        let mut list = DisplayList::new();
        list.stroke_path(
            Path::line(Point::new(4.0, 4.0), Point::new(8.0, 4.0)),
            Paint::Solid(Color::BLACK),
            round,
        )
        .unwrap();
        let r = Raster::render(&list, 12, 8, 1.0).unwrap();
        assert!(alpha(&r, 2, 3) > 200, "round cap extends past the start");
        assert_eq!(alpha(&r, 0, 0), 0);

        let mut list = DisplayList::new();
        list.draw_shadow(
            Rect::from_xywh(5.0, 5.0, 10.0, 10.0),
            0.0,
            Shadow::drop(Point::new(0.0, 3.0), 2.0, Color::BLACK),
        )
        .unwrap();
        let r = Raster::render(&list, 20, 24, 1.0).unwrap();
        // The casting shape is cut out; the shadow shows below it.
        assert_eq!(alpha(&r, 10, 10), 0);
        assert!(alpha(&r, 10, 15) > 150, "{}", alpha(&r, 10, 15));
        let edge = alpha(&r, 10, 18);
        assert!(edge > 30 && edge < 200, "soft edge {edge}");
        assert_eq!(alpha(&r, 10, 23), 0);
        assert_eq!(alpha(&r, 0, 0), 0);

        // An inset shadow darkens the inside edge and leaves the outside.
        let mut list = DisplayList::new();
        list.draw_shadow(
            Rect::from_xywh(4.0, 4.0, 16.0, 16.0),
            0.0,
            Shadow {
                offset: Point::new(0.0, 2.0),
                blur: 0.0,
                spread: 0.0,
                color: Color::BLACK,
                inset: true,
            },
        )
        .unwrap();
        let r = Raster::render(&list, 24, 24, 1.0).unwrap();
        assert!(alpha(&r, 10, 4) > 200, "top inside edge");
        assert_eq!(alpha(&r, 10, 12), 0);
        assert_eq!(alpha(&r, 10, 2), 0);

        // A negative spread shrinks the shadow under the shape.
        let mut list = DisplayList::new();
        list.draw_shadow(
            Rect::from_xywh(5.0, 5.0, 10.0, 10.0),
            0.0,
            Shadow {
                spread: -2.0,
                ..Shadow::drop(Point::new(0.0, 3.0), 0.0, Color::BLACK)
            },
        )
        .unwrap();
        let r = Raster::render(&list, 20, 24, 1.0).unwrap();
        assert!(alpha(&r, 10, 15) > 150);
        assert_eq!(alpha(&r, 5, 15), 0, "spread pulls the side in");
    }

    #[test]
    fn two_times_scale_doubles_pixel_extent() {
        let mut list = DisplayList::new();
        list.fill_path(
            Path::ellipse(Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
            Paint::Solid(Color::BLACK),
            FillRule::NonZero,
        )
        .unwrap();
        let one = Raster::render(&list, 10, 10, 1.0).unwrap();
        let two = Raster::render(&list, 20, 20, 2.0).unwrap();
        let count = |r: &Raster| r.pixels().chunks(4).filter(|p| p[3] > 127).count() as f64;
        let ratio = count(&two) / count(&one);
        assert!((ratio - 4.0).abs() < 0.3, "{ratio}");
    }
}
