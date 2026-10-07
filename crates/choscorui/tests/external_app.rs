use choscorui::{
    AlertButton, AlertSpec, Button, ButtonInput, CommandId, ControlAction, Event, FileDialogMode,
    FileDialogSpec, Node, Point, Rect, Size, Theme, Tree, WindowOptions,
};

#[test]
fn application_code_can_build_a_control_tree_through_the_facade() {
    let bounds = Rect::new(Point::new(0.0, 0.0), Size::new(320.0, 200.0));
    let mut tree = Tree::new(bounds);
    let button_node: Node = tree.attach(tree.root(), bounds).unwrap();
    tree.set_focusable(button_node, true).unwrap();
    let mut button = Button::new("Save", CommandId(7));

    assert_eq!(
        button
            .handle(
                &mut tree,
                button_node,
                ButtonInput::Semantic(ControlAction::Activate),
            )
            .unwrap(),
        Some(CommandId(7))
    );
    assert_eq!(tree.parent(button_node), Some(tree.root()));

    let event = Event::Pointer(choscorui::PointerEvent::Move {
        position: Point::new(12.0, 8.0),
    });
    assert!(matches!(event, Event::Pointer(_)));

    let _theme = Theme::light();
    let _window = WindowOptions::new("Facade example", 320.0, 200.0);
    let alert = AlertSpec::new("Save changes?", [AlertButton::new("Save")]).unwrap();
    assert_eq!(alert.message(), "Save changes?");
    assert_eq!(FileDialogSpec::save_file().mode(), FileDialogMode::SaveFile);
}
