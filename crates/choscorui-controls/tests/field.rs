//! Field text parts and the field layout.

mod hosted;

use choscorui_controls::{
    ControlSize, EstimatedText, Field, FieldGap, FieldOrientation, FieldPart, FieldText, HostEnv,
    Styles,
};
use choscorui_core::{ColorRole, Constraints, DesignTokens, Point, Rect, SemanticRole, Size};
use hosted::{Harness, paints, rect, texts};

#[test]
fn descriptions_wrap_to_the_width_they_are_given() {
    let text = "Choose a unique name that your team will recognize";
    let mut h = Harness::new(FieldText::new(FieldPart::Description, text));
    assert_eq!(h.measure().height, 20.0, "one line without a width limit");
    let env = HostEnv::new(&h.themes, &h.styles, &EstimatedText);
    let size = h
        .host
        .measure(
            &h.tree,
            h.node,
            Constraints::loose(Size::new(160.0, 500.0)),
            &env,
        )
        .unwrap()
        .size;
    assert_eq!(size.width, 160.0);
    assert!(size.height >= 60.0, "three or more lines: {}", size.height);
    h.place(Rect::new(Point::new(0.0, 0.0), size));
    let lines = texts(&h.paint());
    assert!(lines.len() >= 3);
    assert_eq!(lines.concat().replace(' ', ""), text.replace(' ', ""));
    let muted = DesignTokens::light().color(ColorRole::MutedForeground);
    assert!(paints(&h.paint(), muted));
}

#[test]
fn errors_are_destructive_and_invalid_and_take_no_focus() {
    let mut h = Harness::new(FieldText::new(FieldPart::Error, "Enter a valid email"));
    let destructive = DesignTokens::light().color(ColorRole::Destructive);
    assert!(paints(&h.paint(), destructive));
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::StaticText);
    assert!(node.state.invalid);
    assert_eq!(h.host.focus_next(&mut h.tree, false), None);
}

#[test]
fn legends_are_larger_than_small_legends() {
    let mut large = Harness::new(FieldText::new(FieldPart::Legend, "Payment"));
    let mut small =
        Harness::new(FieldText::new(FieldPart::Legend, "Payment").with_size(ControlSize::Sm));
    assert!(large.measure().width > small.measure().width);
}

#[test]
fn a_separator_with_text_paints_a_rule_and_a_chip() {
    let mut h = Harness::new(FieldText::new(FieldPart::Separator, "Or continue with"));
    h.place(rect(0.0, 0.0, 300.0, 20.0));
    assert_eq!(texts(&h.paint()), ["Or continue with"]);
    let border = DesignTokens::light().color(ColorRole::Border);
    assert!(paints(&h.paint(), border));
}

#[test]
fn field_layout_stacks_or_places_side_by_side() {
    let styles = Styles::new();
    let tokens = DesignTokens::light();
    let label = Size::new(80.0, 14.0);
    let input = Size::new(240.0, 36.0);
    let help = Size::new(200.0, 20.0);

    let vertical = Field::new(FieldOrientation::Vertical).layout(&styles, &tokens);
    let rects = vertical.place(400.0, false, &[label, input, help]);
    assert_eq!(rects[1].origin.y, 14.0 + 12.0, "the 12 pt field gap");
    assert_eq!(rects[2].origin.y, 14.0 + 12.0 + 36.0 + 12.0);
    assert!(rects.iter().all(|rect| rect.size.width == 400.0));

    let switch = Size::new(32.0, 18.4);
    let horizontal = Field::new(FieldOrientation::Horizontal).layout(&styles, &tokens);
    let rects = horizontal.place(400.0, false, &[switch, label, help]);
    assert_eq!(rects[0].origin, Point::new(0.0, 0.0));
    assert_eq!(rects[1].origin.x, 32.0 + 12.0);
    assert_eq!(rects[2].origin.y, 14.0 + 4.0, "the content gap");
    let mirrored = horizontal.place(400.0, true, &[switch, label, help]);
    assert_eq!(
        mirrored[0].max_x(),
        400.0,
        "the lead goes to the right in RTL"
    );
    let natural = horizontal.place(f64::INFINITY, false, &[label, help]);
    assert_eq!(natural[1], rect(80.0 + 12.0, 0.0, 200.0, 20.0));

    let responsive = Field::new(FieldOrientation::Responsive);
    assert!(!responsive.is_horizontal(300.0));
    assert!(responsive.is_horizontal(500.0));
    let rects = responsive
        .layout(&styles, &tokens)
        .place(300.0, false, &[label, input]);
    assert_eq!(rects[1].max_y(), 14.0 + 12.0 + 36.0);

    assert_eq!(FieldGap::Group.resolve(&styles, &tokens), 28.0);
    assert_eq!(FieldGap::Set.resolve(&styles, &tokens), 24.0);
}
