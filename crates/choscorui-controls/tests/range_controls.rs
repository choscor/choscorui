use choscorui_controls::{
    ControlKind, EstimatedText, IntegerRange, PaintContext, Progress, RangeError, Recipe,
    ResolvedStyle, Slider, SliderInput, SliderOrientation, SliderSemanticAction, SpinBox,
    SpinBoxInput, SpinBoxSemanticAction, StyleQuery, Styles,
};
use choscorui_core::{
    Color, ColorRole, DesignTokens, Dirty, DisplayCommand, DisplayList, Key, KeyEvent,
    LayoutDirection, Point, PointerButton, PointerEvent, RecipeSet, Rect, SemanticAction,
    SemanticRole, Size, Tree,
};
use std::rc::Rc;

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
        Some(40),
        "a value snaps to the step"
    );
    let semantics = slider.semantics();
    assert_eq!(semantics.role, SemanticRole::Slider);
    assert_eq!(semantics.value.as_deref(), Some("40"));
    assert_eq!(
        semantics.description, "",
        "the bounds are numbers, not text"
    );
    let range = semantics.state.range.unwrap();
    assert_eq!(
        (range.minimum, range.maximum, range.step),
        (0.0, 100.0, 5.0)
    );
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
    let semantics = spin.semantics();
    assert_eq!(semantics.role, SemanticRole::SpinButton);
    assert_eq!(semantics.value.as_deref(), Some("1"));
}

#[test]
fn progress_is_noninteractive_and_supports_indeterminate_mode() {
    let determinate = Progress::new("Download", 25, 100).unwrap();
    let semantics = determinate.semantics();
    assert_eq!(semantics.role, SemanticRole::ProgressIndicator);
    assert_eq!(semantics.value.as_deref(), Some("25%"));
    assert!(semantics.actions.is_empty());
    let indeterminate = Progress::indeterminate("Loading");
    assert_eq!(indeterminate.semantics().value, None);

    let (mut tree, id) = setup();
    tree.detach(id).unwrap();
    let mut detached = Progress::new("Work", 1, 10).unwrap();
    assert_eq!(
        detached.set_value(&mut tree, id, 2, 10),
        Err(RangeError::MissingNode)
    );
}

#[test]
fn recipes_are_swappable_without_changing_control_values() {
    struct Flat;
    impl Recipe for Flat {
        fn resolve(&self, _: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
            Some(ResolvedStyle {
                background: Color::rgba8(30, 100, 220, 255).into(),
                min_width: 12.0,
                min_height: 4.0,
                ..ResolvedStyle::new(tokens)
            })
        }
    }
    let (tree, id) = setup();
    let slider = Slider::new(
        "Volume",
        IntegerRange::new(0, 100, 40, 5, 20).unwrap(),
        SliderOrientation::Horizontal,
    );
    let spin = SpinBox::new("Guests", IntegerRange::new(1, 10, 4, 1, 3).unwrap());
    let progress = Progress::new("Work", 40, 100).unwrap();
    let tokens = DesignTokens::light();
    let mut custom = Styles::new();
    for kind in [
        ControlKind::Slider,
        ControlKind::Input,
        ControlKind::Progress,
    ] {
        custom.set_recipe(RecipeSet::DEFAULT, kind, Rc::new(Flat));
    }
    let bounds = tree.bounds(id).unwrap();
    for styles in [Styles::new(), custom] {
        let mut list = DisplayList::new();
        let mut cx = PaintContext::new(&mut list, bounds, &tokens, &styles, &EstimatedText);
        slider.paint(&mut cx, &tree, id);
        spin.paint(&mut cx, &tree, id);
        progress.paint(&mut cx);
        assert!(!list.commands().is_empty());
        assert!(list.is_balanced());
    }
    assert_eq!(slider.value(), 40);
    assert_eq!(spin.value(), 4);
    assert_eq!(progress.value(), Some((40, 100)));
}

#[test]
fn slider_paints_range_and_thumb_at_the_value_and_mirrors_in_rtl() {
    let (mut tree, id) = setup();
    tree.set_bounds(id, Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 16.0)))
        .unwrap();
    let mut slider = Slider::new(
        "Volume",
        IntegerRange::new(0, 100, 25, 5, 20).unwrap(),
        SliderOrientation::Horizontal,
    );
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let primary = tokens.color(ColorRole::Primary);
    let range_of = |list: &DisplayList| {
        list.commands()
            .iter()
            .find_map(|command| match command {
                DisplayCommand::FillRoundedRect { rect, color, .. }
                | DisplayCommand::FillRect { rect, color }
                    if *color == primary =>
                {
                    Some(*rect)
                }
                _ => None,
            })
            .unwrap()
    };
    let mut ltr = DisplayList::new();
    slider.paint(
        &mut PaintContext::new(
            &mut ltr,
            tree.bounds(id).unwrap(),
            &tokens,
            &styles,
            &EstimatedText,
        ),
        &tree,
        id,
    );
    // The range ends at the thumb center, which travels half a 16 pt
    // thumb inside each end: 8 + 25% of 184.
    let range = range_of(&ltr);
    assert_eq!((range.origin.x, range.size.width), (0.0, 54.0));
    assert_eq!(range.size.height, 6.0, "the track is 6 pt thick");

    let mut rtl = DisplayList::new();
    slider.paint(
        &mut PaintContext::new(
            &mut rtl,
            tree.bounds(id).unwrap(),
            &tokens,
            &styles,
            &EstimatedText,
        )
        .rtl(true),
        &tree,
        id,
    );
    let range = range_of(&rtl);
    assert_eq!((range.origin.x, range.size.width), (146.0, 54.0));

    // Pointer input follows the same direction.
    slider.set_direction(LayoutDirection::Rtl);
    slider
        .handle(
            &mut tree,
            id,
            SliderInput::Pointer(PointerEvent::Down {
                position: Point::new(50.0, 8.0),
                button: PointerButton::Primary,
            }),
        )
        .unwrap();
    assert_eq!(slider.value(), 75);
    assert!(slider.visual_state(&tree, id).pressed());
    assert!(
        !slider.visual_state(&tree, id).focus_visible(),
        "a pointer press focuses without the ring"
    );
}

#[test]
fn spin_box_steppers_sit_at_the_end_in_reading_order() {
    let (mut tree, id) = setup();
    let bounds = Rect::new(Point::new(0.0, 0.0), Size::new(120.0, 36.0));
    tree.set_bounds(id, bounds).unwrap();
    let mut spin = SpinBox::new("Guests", IntegerRange::new(1, 10, 4, 1, 3).unwrap());
    assert_eq!(spin.stepper_rect(bounds).origin.x, 96.0);
    spin.set_direction(LayoutDirection::Rtl);
    assert_eq!(spin.stepper_rect(bounds).origin.x, 0.0);
    let up = |y| {
        SpinBoxInput::Pointer(PointerEvent::Down {
            position: Point::new(10.0, y),
            button: PointerButton::Primary,
        })
    };
    assert_eq!(spin.handle(&mut tree, id, up(5.0)).unwrap(), Some(5));
    assert_eq!(spin.handle(&mut tree, id, up(30.0)).unwrap(), Some(4));
}
