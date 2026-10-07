use choscorui_core::{Color, DisplayCommand, DisplayList, DisplayListError, Point};

#[test]
fn display_list_owns_text_and_rejects_invalid_font_geometry() {
    let mut list = DisplayList::new();
    let label = String::from("Xin chào 👩‍💻");
    list.draw_text(
        Point::new(12.0, 30.0),
        &label,
        "Helvetica",
        18.0,
        Color::rgba8(255, 255, 255, 255),
    )
    .unwrap();
    drop(label);
    assert!(
        matches!(&list.commands()[0], DisplayCommand::DrawText { text, baseline, font_family, font_size, .. }
        if text == "Xin chào 👩‍💻" && *baseline == Point::new(12.0, 30.0) && font_family == "Helvetica" && *font_size == 18.0)
    );
    assert_eq!(
        list.draw_text(
            Point::new(0.0, 0.0),
            "bad",
            "Helvetica",
            f64::NAN,
            Color::rgba8(0, 0, 0, 255)
        ),
        Err(DisplayListError::InvalidText)
    );
    assert_eq!(list.commands().len(), 1);
}

#[test]
fn primitive_parameters_and_opacity_stack_are_validated() {
    let rect = choscorui_core::Rect::new(Point::new(0.0, 0.0), choscorui_core::Size::new(4.0, 4.0));
    let color = Color::rgba8(0, 0, 0, 255);
    let mut list = DisplayList::new();
    assert_eq!(
        list.stroke_rect(rect, color, 0.0),
        Err(DisplayListError::InvalidGeometry)
    );
    assert_eq!(
        list.fill_rounded_rect(rect, -1.0, color),
        Err(DisplayListError::InvalidGeometry)
    );
    assert_eq!(
        list.push_opacity(1.1),
        Err(DisplayListError::InvalidOpacity)
    );
    assert_eq!(list.pop_opacity(), Err(DisplayListError::UnbalancedOpacity));
    list.push_opacity(0.5).unwrap();
    list.pop_opacity().unwrap();
}
