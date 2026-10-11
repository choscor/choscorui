use choscorui_controls::{
    EstimatedText, Label, PaintContext, Separator, SeparatorOrientation, Styles, VisualState,
};
use choscorui_core::{
    ColorRole, DesignTokens, Dirty, DisplayCommand, DisplayList, Point, Rect, SemanticRole, Size,
    Tree,
};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn label_updates_layout_paint_and_accessibility_and_uses_its_recipe() {
    let mut tree = Tree::new(rect(0.0, 0.0, 200.0, 50.0));
    let node = tree
        .attach(tree.root(), rect(0.0, 0.0, 200.0, 50.0))
        .unwrap();
    tree.take_dirty(node).unwrap();
    let mut label = Label::new(node, "Name");
    assert!(label.set_text(&mut tree, "Display name").unwrap());
    let dirty = tree.take_dirty(node).unwrap();
    assert!(dirty.contains(Dirty::LAYOUT));
    assert!(dirty.contains(Dirty::PAINT));
    assert!(dirty.contains(Dirty::SEMANTICS));
    assert!(!label.set_text(&mut tree, "Display name").unwrap());
    assert_eq!(label.semantics().role, SemanticRole::StaticText);
    assert_eq!(label.semantics().name, "Display name");

    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let mut list = DisplayList::new();
    label.paint(&mut PaintContext::new(
        &mut list,
        rect(4.0, 0.0, 200.0, 36.0),
        &tokens,
        &styles,
        &EstimatedText,
    ));
    let foreground = tokens.color(ColorRole::Foreground);
    assert!(matches!(
        list.commands(),
        [DisplayCommand::DrawText { text, font_size, color, .. }]
            if text == "Display name" && *font_size == 14.0 && *color == foreground
    ));

    let mut dimmed = DisplayList::new();
    label.paint(
        &mut PaintContext::new(
            &mut dimmed,
            rect(4.0, 0.0, 200.0, 36.0),
            &tokens,
            &styles,
            &EstimatedText,
        )
        .state(VisualState::DISABLED),
    );
    assert!(
        matches!(dimmed.commands().first(), Some(DisplayCommand::PushOpacity(alpha)) if *alpha == 0.5),
        "a disabled label paints at half opacity"
    );
}

#[test]
fn separator_centers_a_border_rule_in_either_orientation() {
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let border = tokens.color(ColorRole::Border);
    let paint = |bounds: Rect, orientation| {
        let mut list = DisplayList::new();
        Separator::new(orientation).paint(&mut PaintContext::new(
            &mut list,
            bounds,
            &tokens,
            &styles,
            &EstimatedText,
        ));
        list
    };
    let horizontal = paint(
        rect(10.0, 20.0, 100.0, 10.0),
        SeparatorOrientation::Horizontal,
    );
    assert!(
        matches!(horizontal.commands(), [DisplayCommand::FillRect { rect: line, color }]
            if *line == rect(10.0, 24.5, 100.0, 1.0) && *color == border)
    );
    let vertical = paint(
        rect(10.0, 20.0, 10.0, 100.0),
        SeparatorOrientation::Vertical,
    );
    assert!(
        matches!(vertical.commands(), [DisplayCommand::FillRect { rect: line, .. }]
            if *line == rect(14.5, 20.0, 1.0, 100.0))
    );
}
