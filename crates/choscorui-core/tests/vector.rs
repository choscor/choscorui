use choscorui_core::{
    Color, DisplayList, DisplayListError, FillRule, GradientStop, Image, ImageError, ImageSampling,
    Paint, Path, Point, Rect, Shadow, StrokeStyle, SvgDocument, SvgError, Transform, damage,
    parse_path_data,
};

fn red() -> Paint {
    Paint::Solid(Color::rgb8(255, 0, 0))
}

#[test]
fn paths_flatten_bound_and_hit_test_with_fill_rules() {
    let circle = Path::ellipse(Rect::from_xywh(0.0, 0.0, 20.0, 20.0));
    assert!(circle.contains(Point::new(10.0, 10.0), FillRule::NonZero));
    assert!(!circle.contains(Point::new(1.0, 1.0), FillRule::NonZero));
    let bounds = circle.bounds();
    assert!((bounds.size.width - 20.0).abs() < 1e-9);
    // Two nested squares in the same direction: even-odd punches a hole.
    let mut rings = Path::rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0));
    rings.extend(&Path::rect(Rect::from_xywh(3.0, 3.0, 4.0, 4.0)));
    assert!(rings.contains(Point::new(5.0, 5.0), FillRule::NonZero));
    assert!(!rings.contains(Point::new(5.0, 5.0), FillRule::EvenOdd));
    let moved =
        Path::rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0)).transformed(Transform::translate(5.0, 5.0));
    assert_eq!(moved.bounds(), Rect::from_xywh(5.0, 5.0, 1.0, 1.0));
    let flat = circle.flatten(0.1);
    assert_eq!(flat.len(), 1);
    assert!(flat[0].1, "ellipse is closed");
}

#[test]
fn paints_interpolate_and_validate() {
    let gradient = Paint::LinearGradient {
        start: Point::new(0.0, 0.0),
        end: Point::new(10.0, 0.0),
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgb8(0, 0, 0),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgb8(200, 100, 0),
            },
        ],
    };
    assert_eq!(
        gradient.color_at(Point::new(5.0, 3.0)),
        Color::rgb8(100, 50, 0)
    );
    assert_eq!(
        gradient.color_at(Point::new(-5.0, 0.0)),
        Color::rgb8(0, 0, 0)
    );
    let bad = Paint::RadialGradient {
        center: Point::new(0.0, 0.0),
        radius: 0.0,
        stops: vec![],
    };
    let mut list = DisplayList::new();
    assert_eq!(
        list.fill_path(
            Path::rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0)),
            bad,
            FillRule::NonZero
        ),
        Err(DisplayListError::InvalidPaint)
    );
    assert_eq!(
        list.stroke_path(
            Path::line(Point::new(0.0, 0.0), Point::new(1.0, 1.0)),
            red(),
            StrokeStyle::new(0.0)
        ),
        Err(DisplayListError::InvalidGeometry)
    );
    assert_eq!(
        list.pop_transform(),
        Err(DisplayListError::UnbalancedTransform)
    );
    assert!(Color::BLACK.contrast_ratio(Color::WHITE) > 20.9);
}

#[test]
fn images_validate_dimensions_and_compare_by_identity() {
    assert_eq!(
        Image::from_rgba8(0, 1, vec![]),
        Err(ImageError::InvalidDimensions)
    );
    assert_eq!(
        Image::from_rgba8(1, 1, vec![0; 3]),
        Err(ImageError::LengthMismatch)
    );
    let a = Image::from_rgba8(1, 1, vec![1, 2, 3, 4]).unwrap();
    let b = Image::from_rgba8(1, 1, vec![1, 2, 3, 4]).unwrap();
    assert_eq!(a, a.clone());
    assert_ne!(a, b);
    assert_eq!(a.pixel(0, 0), Some([1, 2, 3, 4]));
}

#[test]
fn damage_covers_only_changed_draws_including_state() {
    let mut base = DisplayList::new();
    base.fill_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), Color::BLACK);
    base.fill_rect(Rect::from_xywh(100.0, 0.0, 10.0, 10.0), Color::BLACK);
    assert_eq!(damage(&base, &base.clone()), None);
    let mut changed = DisplayList::new();
    changed.fill_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), Color::BLACK);
    changed.fill_rect(Rect::from_xywh(100.0, 0.0, 10.0, 10.0), Color::WHITE);
    assert_eq!(
        damage(&base, &changed),
        Some(Rect::from_xywh(100.0, 0.0, 10.0, 10.0))
    );
    // Moving a draw damages both the old and new location.
    let mut moved = DisplayList::new();
    moved.fill_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), Color::BLACK);
    moved.fill_rect(Rect::from_xywh(100.0, 50.0, 10.0, 10.0), Color::BLACK);
    assert_eq!(
        damage(&base, &moved),
        Some(Rect::from_xywh(100.0, 0.0, 10.0, 60.0))
    );
    // A transform change damages its content at old and new positions.
    let wrap = |dx: f64| {
        let mut list = DisplayList::new();
        list.push_transform(Transform::translate(dx, 0.0)).unwrap();
        list.fill_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), Color::BLACK);
        list.pop_transform().unwrap();
        list
    };
    assert_eq!(
        damage(&wrap(0.0), &wrap(20.0)),
        Some(Rect::from_xywh(0.0, 0.0, 30.0, 10.0))
    );
    // Clipped-out content never damages.
    let clipped = |color| {
        let mut list = DisplayList::new();
        list.push_clip(Rect::from_xywh(0.0, 0.0, 5.0, 5.0));
        list.fill_rect(Rect::from_xywh(50.0, 50.0, 5.0, 5.0), color);
        list.pop_clip().unwrap();
        list
    };
    assert_eq!(damage(&clipped(Color::BLACK), &clipped(Color::WHITE)), None);
    // Shadows inflate damage by their blur.
    let mut shadow = DisplayList::new();
    shadow
        .draw_shadow(
            Rect::from_xywh(10.0, 10.0, 10.0, 10.0),
            2.0,
            Shadow::drop(Point::new(0.0, 2.0), 4.0, Color::BLACK),
        )
        .unwrap();
    assert_eq!(
        damage(&DisplayList::new(), &shadow),
        Some(Rect::from_xywh(3.0, 5.0, 24.0, 24.0))
    );
}

#[test]
fn path_data_supports_all_commands() {
    let path = parse_path_data(
        "M10 10 h10 v10 H10 z m5 5 l1,1 c1 0 2 1 2 2 s1 2 2 2 q1 1 2 2 t2 2 a5 5 0 0 1 10 0 Z",
    )
    .unwrap();
    assert!(path.elements().len() > 10);
    assert!(path.contains(Point::new(15.0, 12.0), FillRule::NonZero));
    let arc = parse_path_data("M0 10 A10 10 0 0 1 20 10").unwrap();
    let top = arc.flatten(0.01)[0]
        .0
        .iter()
        .map(|p| p.y)
        .fold(f64::MAX, f64::min);
    assert!(
        (top - 0.0).abs() < 0.05,
        "semicircle reaches y=0, got {top}"
    );
    assert!(matches!(
        parse_path_data("M10"),
        Err(SvgError::InvalidPathData(_))
    ));
    assert!(matches!(
        parse_path_data("10 10"),
        Err(SvgError::InvalidPathData(_))
    ));
    assert!(matches!(
        parse_path_data("M0 0 X1"),
        Err(SvgError::Unsupported(_))
    ));
    let exp = parse_path_data("M1e1-5L.5.5").unwrap();
    assert_eq!(exp.bounds(), Rect::from_xywh(0.5, -5.0, 9.5, 5.5));
}

#[test]
fn svg_documents_import_shapes_styles_and_transforms() {
    let source = r##"<?xml version="1.0"?>
<!-- icon -->
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
  <title>Save</title>
  <g fill="currentColor" transform="translate(2 2)">
    <rect x="0" y="0" width="10" height="10" rx="2"/>
    <circle cx="15" cy="15" r="3" style="fill:#f00; fill-opacity:0.5"/>
  </g>
  <polyline points="0,0 4,4 8,0" fill="none" stroke="rgb(0, 0, 255)" stroke-width="2" stroke-linecap="round"/>
  <path d="M0 20 L4 20" stroke="#00f" fill="none"/>
</svg>"##;
    let doc = SvgDocument::parse(source).unwrap();
    assert_eq!(doc.view_box, Rect::from_xywh(0.0, 0.0, 24.0, 24.0));
    assert_eq!(doc.shapes.len(), 4);
    assert_eq!(
        doc.shapes[0].path.bounds(),
        Rect::from_xywh(2.0, 2.0, 10.0, 10.0)
    );
    let mut list = DisplayList::new();
    doc.draw(
        &mut list,
        Rect::from_xywh(0.0, 0.0, 48.0, 48.0),
        Color::rgb8(1, 2, 3),
    )
    .unwrap();
    // rect fill (current color), circle fill, polyline stroke, path stroke.
    assert_eq!(list.len(), 4);
    match &list.commands()[0] {
        choscorui_core::DisplayCommand::FillPath { path, paint, .. } => {
            assert_eq!(path.bounds(), Rect::from_xywh(4.0, 4.0, 20.0, 20.0));
            assert_eq!(*paint, Paint::Solid(Color::rgb8(1, 2, 3)));
        }
        other => panic!("{other:?}"),
    }
    match &list.commands()[1] {
        choscorui_core::DisplayCommand::FillPath { paint, .. } => {
            assert_eq!(*paint, Paint::Solid(Color::rgba8(255, 0, 0, 128)));
        }
        other => panic!("{other:?}"),
    }
    match &list.commands()[2] {
        choscorui_core::DisplayCommand::StrokePath { style, .. } => assert_eq!(style.width, 4.0),
        other => panic!("{other:?}"),
    }
}

#[test]
fn svg_rejects_unsupported_and_malformed_input_explicitly() {
    let unsupported = SvgDocument::parse(r#"<svg viewBox="0 0 1 1"><text>hi</text></svg>"#);
    assert_eq!(unsupported, Err(SvgError::Unsupported("text".into())));
    let gradient = SvgDocument::parse(r#"<svg viewBox="0 0 1 1"><linearGradient id="g"/></svg>"#);
    assert_eq!(
        gradient,
        Err(SvgError::Unsupported("linearGradient".into()))
    );
    assert!(matches!(
        SvgDocument::parse(r#"<svg viewBox="0 0 1 1"><g></svg>"#),
        Err(SvgError::Malformed(_))
    ));
    assert!(matches!(
        SvgDocument::parse("<svg>"),
        Err(SvgError::MissingSize)
    ));
    assert!(matches!(
        SvgDocument::parse(r##"<svg width="1" height="1"><rect fill="#zz"/></svg>"##),
        Err(SvgError::InvalidColor(_))
    ));
    assert!(matches!(
        SvgDocument::parse(r#"<div/>"#),
        Err(SvgError::Malformed(_))
    ));
    let sized = SvgDocument::parse(r#"<svg width="16px" height="8"/>"#).unwrap();
    assert_eq!(sized.view_box, Rect::from_xywh(0.0, 0.0, 16.0, 8.0));
    assert_eq!(
        SvgError::Unsupported("text".into()).to_string(),
        "unsupported SVG feature `text`"
    );
    let _ = ImageSampling::Nearest;
}
