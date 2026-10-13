#[cfg(target_os = "macos")]
use choscorui_core::{Color, DisplayList, Point, Rect, Size};
#[cfg(target_os = "macos")]
use choscorui_macos::MacBitmap;

#[cfg(target_os = "macos")]
fn text_fixture() -> DisplayList {
    let mut list = DisplayList::new();
    list.fill_rect(
        Rect::new(Point::new(0.0, 0.0), Size::new(80.0, 40.0)),
        Color::rgba8(255, 255, 255, 255),
    );
    list.draw_text(
        Point::new(10.0, 28.0),
        "A",
        "Helvetica",
        24.0,
        Color::rgba8(0, 0, 0, 255),
    )
    .unwrap();
    list
}

#[cfg(target_os = "macos")]
#[test]
fn core_text_produces_visible_ink_at_one_and_two_times_scale() {
    let list = text_fixture();
    for scale in [1.0, 2.0] {
        let bitmap =
            MacBitmap::render(&list, (80.0 * scale) as u32, (40.0 * scale) as u32, scale).unwrap();
        assert_eq!(bitmap.pixel(0, 0), Some([255, 255, 255, 255]));
        let ink = (5..35).any(|y| {
            (10..40).any(|x| {
                bitmap
                    .pixel((f64::from(x) * scale) as u32, (f64::from(y) * scale) as u32)
                    .is_some_and(|pixel| pixel[0] < 200)
            })
        });
        assert!(ink, "glyph had no dark pixels at scale {scale}");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn text_respects_display_list_clip() {
    let mut list = DisplayList::new();
    list.fill_rect(
        Rect::new(Point::new(0.0, 0.0), Size::new(80.0, 40.0)),
        Color::rgba8(255, 255, 255, 255),
    );
    list.push_clip(Rect::new(Point::new(0.0, 0.0), Size::new(5.0, 40.0)));
    list.draw_text(
        Point::new(10.0, 28.0),
        "A",
        "Helvetica",
        24.0,
        Color::rgba8(0, 0, 0, 255),
    )
    .unwrap();
    list.pop_clip().unwrap();
    let bitmap = MacBitmap::render(&list, 80, 40, 1.0).unwrap();
    assert_eq!(bitmap.pixel(18, 20), Some([255, 255, 255, 255]));
}

#[cfg(target_os = "macos")]
#[test]
fn bitmap_pixels_use_logical_top_down_rows() {
    let mut list = DisplayList::new();
    list.fill_rect(
        Rect::new(Point::new(0.0, 0.0), Size::new(20.0, 20.0)),
        Color::rgba8(255, 255, 255, 255),
    );
    list.fill_rect(
        Rect::new(Point::new(0.0, 0.0), Size::new(20.0, 5.0)),
        Color::rgba8(255, 0, 0, 255),
    );
    let bitmap = MacBitmap::render(&list, 20, 20, 1.0).unwrap();
    assert_eq!(bitmap.pixel(3, 2), Some([255, 0, 0, 255]));
    assert_eq!(bitmap.pixel(3, 17), Some([255, 255, 255, 255]));
}

#[cfg(target_os = "macos")]
#[test]
fn bitmap_paints_new_primitives_and_nested_opacity() {
    let mut list = DisplayList::new();
    list.fill_rect(
        Rect::new(Point::new(0.0, 0.0), Size::new(12.0, 6.0)),
        Color::rgba8(255, 255, 255, 255),
    );
    list.fill_rounded_rect(
        Rect::new(Point::new(1.0, 1.0), Size::new(4.0, 4.0)),
        2.0,
        Color::rgba8(0, 255, 0, 255),
    )
    .unwrap();
    list.stroke_rect(
        Rect::new(Point::new(7.0, 1.0), Size::new(4.0, 4.0)),
        Color::rgba8(0, 0, 255, 255),
        2.0,
    )
    .unwrap();
    list.push_opacity(0.5).unwrap();
    list.push_opacity(0.5).unwrap();
    list.fill_rect(
        Rect::new(Point::new(5.0, 2.0), Size::new(1.0, 1.0)),
        Color::rgba8(255, 0, 0, 255),
    );
    list.pop_opacity().unwrap();
    list.pop_opacity().unwrap();

    let bitmap = MacBitmap::render(&list, 12, 6, 1.0).unwrap();
    assert_eq!(bitmap.pixel(3, 3), Some([0, 255, 0, 255]));
    assert_eq!(bitmap.pixel(1, 1), Some([182, 255, 182, 255]));
    assert_eq!(bitmap.pixel(7, 2), Some([0, 0, 255, 255]));
    assert_eq!(bitmap.pixel(5, 2), Some([255, 191, 191, 255]));
}

#[cfg(target_os = "macos")]
#[test]
fn png_encode_decode_round_trips_straight_alpha_pixels() {
    use choscorui_core::{Image, ImageError};
    use choscorui_macos::{decode_image, encode_png};

    let pixels = vec![
        255, 0, 0, 255, 0, 255, 0, 128, //
        0, 0, 255, 0, 10, 20, 30, 255,
    ];
    let image = Image::from_rgba8(2, 2, pixels).unwrap();
    let png = encode_png(&image).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    let decoded = decode_image(&png).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (2, 2));
    for (a, b) in image.pixels().chunks(4).zip(decoded.pixels().chunks(4)) {
        if a[3] == 0 {
            assert_eq!(b[3], 0);
        } else {
            assert!(
                a.iter().zip(b).all(|(x, y)| x.abs_diff(*y) <= 2),
                "{a:?} vs {b:?}"
            );
        }
    }
    assert_eq!(decode_image(&[]), Err(ImageError::UnsupportedFormat));
    assert!(decode_image(b"not an image").is_err());

    let bitmap = MacBitmap::render(&text_fixture(), 80, 40, 1.0).unwrap();
    let exported = decode_image(&bitmap.encode_png().unwrap()).unwrap();
    assert_eq!(exported.pixels()[..4], [255, 255, 255, 255]);
}

/// A line of "IIII" in Helvetica with `slant`, black on white.
#[cfg(target_os = "macos")]
fn slanted(slant: choscorui_core::FontSlant) -> Vec<u8> {
    use choscorui_core::{FontWeight, TextRun};
    let mut list = DisplayList::new();
    let rect = Rect::new(Point::new(0.0, 0.0), Size::new(80.0, 40.0));
    list.fill_rect(rect, Color::rgba8(255, 255, 255, 255));
    let run = TextRun {
        family: "Helvetica",
        size: 24.0,
        weight: FontWeight::REGULAR,
        slant,
        tracking: 0.0,
    };
    list.draw_text_run(Point::new(10.0, 28.0), "IIII", &run, Color::BLACK)
        .unwrap();
    MacBitmap::render(&list, 80, 40, 1.0)
        .unwrap()
        .pixels()
        .to_vec()
}

#[cfg(target_os = "macos")]
#[test]
fn a_text_run_draws_its_slant() {
    use choscorui_core::FontSlant;
    assert_ne!(slanted(FontSlant::Upright), slanted(FontSlant::Italic));
}
