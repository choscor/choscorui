use choscorui_controls::{
    IntegerRange, ProgressBar, ProgressStyle, RangeError, Slider, SliderInput, SliderOrientation,
    SliderSemanticAction, SliderStyle, SpinBox, SpinBoxInput, SpinBoxSemanticAction, SpinBoxStyle,
};
use choscorui_core::{
    Color, Dirty, DisplayList, Key, KeyEvent, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticRole, Size, Tree,
};

fn setup() -> (Tree, choscorui_core::NodeId) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(240.0, 120.0)));
    let id = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(20.0, 20.0), Size::new(200.0, 40.0)),
        )
        .unwrap();
    (tree, id)
}

#[test]
fn integer_range_validates_and_clamps_all_mutations() {
    assert_eq!(IntegerRange::new(5, 4, 5, 1, 2), Err(RangeError::Reversed));
    assert_eq!(IntegerRange::new(0, 10, 5, 0, 2), Err(RangeError::ZeroStep));
    let mut range = IntegerRange::new(-10, 10, 40, 3, 7).unwrap();
    assert_eq!(range.value(), 10);
    assert_eq!(range.increment(), 10);
    assert_eq!(range.decrement(), 7);
    assert_eq!(range.page_decrement(), 0);
    assert_eq!(range.set_value(-99), -10);
}

#[test]
fn slider_supports_drag_keyboard_and_accessibility_actions() {
    let (mut tree, id) = setup();
    let range = IntegerRange::new(0, 100, 0, 5, 20).unwrap();
    let mut slider = Slider::new("Volume", range, SliderOrientation::Horizontal);
    slider
        .handle(
            &mut tree,
            id,
            SliderInput::Pointer(PointerEvent::Down {
                position: Point::new(120.0, 40.0),
                button: PointerButton::Primary,
            }),
        )
        .unwrap();
    assert_eq!(tree.pointer_capture(), Some(id));
    assert_eq!(slider.value(), 50);
    assert_eq!(
        slider
            .handle(
                &mut tree,
                id,
                SliderInput::Pointer(PointerEvent::Move {
                    position: Point::new(220.0, 40.0)
                })
            )
            .unwrap(),
        Some(100)
    );
    slider
        .handle(
            &mut tree,
            id,
            SliderInput::Pointer(PointerEvent::Up {
                position: Point::new(220.0, 40.0),
                button: PointerButton::Primary,
            }),
        )
        .unwrap();
    assert_eq!(tree.pointer_capture(), None);
    tree.set_focus(Some(id)).unwrap();
    assert_eq!(
        slider
            .handle(
                &mut tree,
                id,
                SliderInput::Key(KeyEvent::Down {
                    key: Key::ArrowLeft,
                    repeat: false
                })
            )
            .unwrap(),
        Some(95)
    );
    assert_eq!(
        slider
            .handle(
                &mut tree,
                id,
                SliderInput::Semantic(SliderSemanticAction::SetValue(42))
            )
            .unwrap(),
        Some(42)
    );
    let semantics = slider.semantics(&tree, id);
    assert_eq!(semantics.role, SemanticRole::Slider);
    assert_eq!(semantics.value.as_deref(), Some("42"));
    assert_eq!(
        semantics.actions,
        vec![
            SemanticAction::SetValue,
            SemanticAction::Increment,
            SemanticAction::Decrement,
            SemanticAction::Focus
        ]
    );
}

#[test]
fn slider_disable_cancels_drag_and_value_changes_are_paint_semantic_only() {
    let (mut tree, id) = setup();
    let mut slider = Slider::new(
        "Level",
        IntegerRange::new(0, 10, 5, 1, 2).unwrap(),
        SliderOrientation::Horizontal,
    );
    tree.take_dirty(id).unwrap();
    tree.take_dirty(tree.root()).unwrap();
    slider
        .handle(
            &mut tree,
            id,
            SliderInput::Semantic(SliderSemanticAction::Increment),
        )
        .unwrap();
    let dirty = tree.take_dirty(tree.root()).unwrap();
    assert!(dirty.contains(Dirty::PAINT));
    assert!(dirty.contains(Dirty::SEMANTICS));
    assert!(!dirty.contains(Dirty::LAYOUT));
    slider
        .handle(
            &mut tree,
            id,
            SliderInput::Pointer(PointerEvent::Down {
                position: Point::new(80.0, 40.0),
                button: PointerButton::Primary,
            }),
        )
        .unwrap();
    slider.set_enabled(&mut tree, id, false).unwrap();
    assert_eq!(tree.pointer_capture(), None);
    assert_eq!(
        slider
            .handle(
                &mut tree,
                id,
                SliderInput::Semantic(SliderSemanticAction::Increment)
            )
            .unwrap(),
        None
    );
}

#[test]
fn spin_box_uses_steps_home_end_and_semantic_actions() {
    let (mut tree, id) = setup();
    let mut spin = SpinBox::new("Guests", IntegerRange::new(1, 12, 3, 1, 5).unwrap());
    tree.set_focus(Some(id)).unwrap();
    assert_eq!(
        spin.handle(
            &mut tree,
            id,
            SpinBoxInput::Key(KeyEvent::Down {
                key: Key::ArrowUp,
                repeat: false
            })
        )
        .unwrap(),
        Some(4)
    );
    assert_eq!(
        spin.handle(
            &mut tree,
            id,
            SpinBoxInput::Key(KeyEvent::Down {
                key: Key::End,
                repeat: false
            })
        )
        .unwrap(),
        Some(12)
    );
    assert_eq!(
        spin.handle(
            &mut tree,
            id,
            SpinBoxInput::Semantic(SpinBoxSemanticAction::SetValue(-4))
        )
        .unwrap(),
        Some(1)
    );
    let semantics = spin.semantics(&tree, id);
    assert_eq!(semantics.role, SemanticRole::SpinButton);
    assert_eq!(semantics.value.as_deref(), Some("1"));
}

#[test]
fn progress_is_noninteractive_and_supports_indeterminate_mode() {
    let determinate = ProgressBar::new("Download", 25, 100).unwrap();
    let semantics = determinate.semantics();
    assert_eq!(semantics.role, SemanticRole::ProgressIndicator);
    assert_eq!(semantics.value.as_deref(), Some("25%"));
    assert!(semantics.actions.is_empty());
    let indeterminate = ProgressBar::indeterminate("Loading");
    assert_eq!(indeterminate.semantics().value, None);

    let (mut tree, id) = setup();
    tree.detach(id).unwrap();
    let mut detached = ProgressBar::new("Work", 1, 10).unwrap();
    assert_eq!(
        detached.set_value(&mut tree, id, 2, 10),
        Err(RangeError::MissingNode)
    );
}

#[test]
fn styles_are_swappable_without_changing_control_values() {
    let (tree, id) = setup();
    let slider = Slider::new(
        "Volume",
        IntegerRange::new(0, 100, 40, 5, 20).unwrap(),
        SliderOrientation::Horizontal,
    );
    let spin = SpinBox::new("Guests", IntegerRange::new(1, 10, 4, 1, 3).unwrap());
    let progress = ProgressBar::new("Work", 40, 100).unwrap();
    let slider_style = SliderStyle {
        track: Color::rgba8(10, 10, 10, 255),
        fill: Color::rgba8(30, 100, 220, 255),
        thumb: Color::rgba8(255, 255, 255, 255),
        disabled: Color::rgba8(90, 90, 90, 255),
        focus: Color::rgba8(255, 200, 0, 255),
        track_thickness: 4.0,
        thumb_size: 12.0,
    };
    let spin_style = SpinBoxStyle {
        background: Color::rgba8(20, 20, 20, 255),
        buttons: Color::rgba8(60, 60, 60, 255),
        disabled: Color::rgba8(90, 90, 90, 255),
        focus: Color::rgba8(255, 200, 0, 255),
        button_width: 24.0,
    };
    let progress_style = ProgressStyle {
        track: Color::rgba8(10, 10, 10, 255),
        fill: Color::rgba8(30, 100, 220, 255),
        indeterminate: Color::rgba8(100, 160, 255, 255),
    };
    let mut list = DisplayList::new();
    let bounds = tree.bounds(id).unwrap();
    slider_style.paint(&mut list, bounds, slider.visual_state(&tree, id));
    spin_style.paint(&mut list, bounds, spin.visual_state(&tree, id));
    progress_style.paint(&mut list, bounds, progress.visual_state());
    assert!(!list.commands().is_empty());
    assert_eq!(slider.value(), 40);
    assert_eq!(spin.value(), 4);
    assert_eq!(progress.value(), Some((40, 100)));
}
