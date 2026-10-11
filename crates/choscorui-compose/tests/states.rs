use choscorui_compose::{
    DeclarativeState, StateAssignment, StateError, StateMachine, StateName, TransitionDescription,
};
use choscorui_core::{Easing, MotionTokens, Point, Rect, Size, Tree};
use std::time::Duration;

fn tree_with_node() -> (Tree, choscorui_core::NodeId) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(100.0, 100.0)));
    let root = tree.root();
    let node = tree
        .attach(root, Rect::new(Point::new(0.0, 0.0), Size::new(20.0, 20.0)))
        .unwrap();
    (tree, node)
}

#[test]
fn first_matching_named_state_assigns_its_properties() {
    let runtime = choscorui_compose::Runtime::new();
    let enabled = runtime.property(false);
    let opacity = runtime.property(1.0_f64);
    let scale = runtime.property(1.0_f64);
    let enabled_condition = enabled.clone();
    let mut machine = StateMachine::new([
        DeclarativeState::new("enabled", move || enabled_condition.get())
            .assign(StateAssignment::new(&opacity, 1.0))
            .assign(StateAssignment::new(&scale, 1.0)),
        DeclarativeState::new("disabled", || true)
            .assign(StateAssignment::new(&opacity, 0.4))
            .assign(StateAssignment::new(&scale, 0.95)),
    ])
    .unwrap();
    let (mut tree, node) = tree_with_node();
    let motion = MotionTokens {
        duration_ms: 100,
        reduced: false,
        ..MotionTokens::default()
    };

    let selected = machine.select(&mut tree, node, motion).unwrap();
    assert_eq!(selected, Some(StateName::new("disabled")));
    assert_eq!((opacity.get(), scale.get()), (0.4, 0.95));

    enabled.set(true);
    let selected = machine.select(&mut tree, node, motion).unwrap();
    assert_eq!(selected, Some(StateName::new("enabled")));
    assert_eq!((opacity.get(), scale.get()), (1.0, 1.0));
}

#[test]
fn transition_animates_assignments_and_interruption_starts_at_current_value() {
    let runtime = choscorui_compose::Runtime::new();
    let expanded = runtime.property(false);
    let width = runtime.property(20.0_f64);
    let expanded_condition = expanded.clone();
    let transition = TransitionDescription::between("collapsed", "expanded")
        .duration(Duration::from_millis(100))
        .easing(Easing::Linear);
    let reverse = TransitionDescription::between("expanded", "collapsed")
        .duration(Duration::from_millis(100))
        .easing(Easing::Linear);
    let mut machine = StateMachine::new([
        DeclarativeState::new("expanded", move || expanded_condition.get())
            .assign(StateAssignment::new(&width, 100.0)),
        DeclarativeState::new("collapsed", || true).assign(StateAssignment::new(&width, 20.0)),
    ])
    .unwrap()
    .with_transitions([transition, reverse]);
    let (mut tree, node) = tree_with_node();
    let motion = MotionTokens {
        duration_ms: 100,
        reduced: false,
        ..MotionTokens::default()
    };

    machine.select(&mut tree, node, motion).unwrap();
    expanded.set(true);
    machine.select(&mut tree, node, motion).unwrap();
    assert!(machine.needs_frame());
    machine
        .advance(&mut tree, Duration::from_millis(40))
        .unwrap();
    assert!((width.get() - 52.0).abs() < 0.001);

    expanded.set(false);
    machine.select(&mut tree, node, motion).unwrap();
    assert!((width.get() - 52.0).abs() < 0.001);
    machine
        .advance(&mut tree, Duration::from_millis(90))
        .unwrap();
    assert!((width.get() - 36.0).abs() < 0.001);
}

#[test]
fn reduced_motion_applies_transition_target_without_scheduling_a_frame() {
    let runtime = choscorui_compose::Runtime::new();
    let active = runtime.property(false);
    let value = runtime.property(0.0_f64);
    let condition = active.clone();
    let mut machine = StateMachine::new([
        DeclarativeState::new("on", move || condition.get())
            .assign(StateAssignment::new(&value, 1.0)),
        DeclarativeState::new("off", || true).assign(StateAssignment::new(&value, 0.0)),
    ])
    .unwrap()
    .with_transition(TransitionDescription::any().duration(Duration::from_secs(1)));
    let (mut tree, node) = tree_with_node();
    let reduced = MotionTokens {
        duration_ms: 1000,
        reduced: true,
        ..MotionTokens::default()
    };

    machine.select(&mut tree, node, reduced).unwrap();
    active.set(true);
    machine.select(&mut tree, node, reduced).unwrap();

    assert_eq!(value.get(), 1.0);
    assert!(!machine.needs_frame());
}

#[test]
fn authored_duration_overrides_the_theme_default_and_removed_nodes_cancel() {
    let runtime = choscorui_compose::Runtime::new();
    let active = runtime.property(false);
    let value = runtime.property(0.0_f64);
    let condition = active.clone();
    let mut machine = StateMachine::new([
        DeclarativeState::new("on", move || condition.get())
            .assign(StateAssignment::new(&value, 10.0)),
        DeclarativeState::new("off", || true).assign(StateAssignment::new(&value, 0.0)),
    ])
    .unwrap()
    .with_transition(
        TransitionDescription::any()
            .duration(Duration::from_millis(200))
            .easing(Easing::Linear),
    );
    let (mut tree, node) = tree_with_node();
    let motion = MotionTokens {
        duration_ms: 20,
        reduced: false,
        ..MotionTokens::default()
    };

    machine.select(&mut tree, node, motion).unwrap();
    active.set(true);
    machine.select(&mut tree, node, motion).unwrap();
    machine
        .advance(&mut tree, Duration::from_millis(100))
        .unwrap();
    assert!((value.get() - 5.0).abs() < 0.001);

    tree.detach(node).unwrap();
    assert!(
        !machine
            .advance(&mut tree, Duration::from_millis(120))
            .unwrap()
    );
    assert_eq!(value.get(), 5.0);
}

#[test]
fn invalid_state_definitions_are_rejected_before_use() {
    let runtime = choscorui_compose::Runtime::new();
    let value = runtime.property(0.0_f64);
    let duplicate_names = StateMachine::new([
        DeclarativeState::new("same", || true),
        DeclarativeState::new("same", || false),
    ]);
    assert!(matches!(duplicate_names, Err(StateError::DuplicateName)));

    let duplicate_assignment = StateMachine::new([DeclarativeState::new("bad", || true)
        .assign(StateAssignment::new(&value, 1.0))
        .assign(StateAssignment::new(&value, 2.0))]);
    assert!(matches!(
        duplicate_assignment,
        Err(StateError::DuplicateAssignment)
    ));

    let invalid_target = StateMachine::new([
        DeclarativeState::new("bad", || true).assign(StateAssignment::new(&value, f64::NAN))
    ]);
    assert!(matches!(invalid_target, Err(StateError::InvalidTarget)));
}

#[test]
fn discrete_assignments_switch_non_numeric_properties_immediately() {
    let runtime = choscorui_compose::Runtime::new();
    let selected = runtime.property(false);
    let label = runtime.property(String::from("Idle"));
    let condition = selected.clone();
    let mut machine = StateMachine::new([
        DeclarativeState::new("selected", move || condition.get())
            .assign(StateAssignment::discrete(&label, String::from("Selected"))),
        DeclarativeState::new("idle", || true)
            .assign(StateAssignment::discrete(&label, String::from("Idle"))),
    ])
    .unwrap()
    .with_transition(TransitionDescription::any().duration(Duration::from_secs(1)));
    let (mut tree, node) = tree_with_node();
    let motion = MotionTokens {
        duration_ms: 1000,
        reduced: false,
        ..MotionTokens::default()
    };

    machine.select(&mut tree, node, motion).unwrap();
    selected.set(true);
    machine.select(&mut tree, node, motion).unwrap();

    assert_eq!(label.get(), "Selected");
    assert!(!machine.needs_frame());
}

#[test]
fn an_immediate_state_change_cancels_an_in_flight_property_transition() {
    let runtime = choscorui_compose::Runtime::new();
    let expanded = runtime.property(false);
    let width = runtime.property(20.0_f64);
    let condition = expanded.clone();
    let mut machine = StateMachine::new([
        DeclarativeState::new("expanded", move || condition.get())
            .assign(StateAssignment::new(&width, 100.0)),
        DeclarativeState::new("collapsed", || true).assign(StateAssignment::new(&width, 20.0)),
    ])
    .unwrap()
    .with_transition(
        TransitionDescription::between("collapsed", "expanded")
            .duration(Duration::from_millis(100))
            .easing(Easing::Linear),
    );
    let (mut tree, node) = tree_with_node();
    let motion = MotionTokens {
        duration_ms: 100,
        reduced: false,
        ..MotionTokens::default()
    };

    machine.select(&mut tree, node, motion).unwrap();
    expanded.set(true);
    machine.select(&mut tree, node, motion).unwrap();
    machine
        .advance(&mut tree, Duration::from_millis(40))
        .unwrap();
    expanded.set(false);
    machine.select(&mut tree, node, motion).unwrap();

    assert_eq!(width.get(), 20.0);
    assert!(!machine.needs_frame());
    machine
        .advance(&mut tree, Duration::from_millis(80))
        .unwrap();
    assert_eq!(width.get(), 20.0);
}
