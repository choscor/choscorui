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
    assert_eq!(image.pixel(0, 0), Some([0, 0, 0, 0]));
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
    assert_eq!(image.pixel(1, 0), Some([0, 0, 255, 255]));
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
