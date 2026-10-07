use choscorui_controls::{Label, LabelStyle, Separator, SeparatorOrientation, SeparatorStyle};
use choscorui_core::{
    Color, Dirty, DisplayCommand, DisplayList, Point, Rect, SemanticRole, Size, Tree,
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

    let mut list = DisplayList::new();
    LabelStyle {
        font_family: "sans".into(),
        font_size: 13.0,
        color: Color::rgba8(1, 2, 3, 255),
    }
    .paint(&mut list, Point::new(4.0, 18.0), &label)
    .unwrap();
    assert!(
        matches!(list.commands(), [DisplayCommand::DrawText { text, .. }] if text == "Display name")
    );
}

#[test]
fn separator_centers_the_requested_thickness_in_either_orientation() {
    let style = SeparatorStyle {
        color: Color::rgba8(20, 20, 20, 255),
        thickness: 2,
    };
    let mut horizontal = DisplayList::new();
    style.paint(
        &mut horizontal,
        rect(10.0, 20.0, 100.0, 10.0),
        Separator::new(SeparatorOrientation::Horizontal),
    );
    assert!(
        matches!(horizontal.commands(), [DisplayCommand::FillRect { rect: line, .. }] if *line == rect(10.0, 24.0, 100.0, 2.0))
    );

    let mut vertical = DisplayList::new();
    style.paint(
        &mut vertical,
        rect(10.0, 20.0, 10.0, 100.0),
        Separator::new(SeparatorOrientation::Vertical),
    );
    assert!(
        matches!(vertical.commands(), [DisplayCommand::FillRect { rect: line, .. }] if *line == rect(14.0, 20.0, 2.0, 100.0))
    );
}
