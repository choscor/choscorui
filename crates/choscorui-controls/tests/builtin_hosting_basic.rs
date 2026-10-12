//! The static, range, selection, and text built-in controls through the
//! control host: pointer, keys, assistive actions, values from the
//! application, disabled state, semantics, and right-to-left paint.

mod hosted;

use choscorui_controls::{
    ControlMessage, ControlValue, CustomControl, IntegerRange, Label, Progress, RadioGroup,
    SelectionError, Separator, SeparatorOrientation, Slider, SliderOrientation, SpinBox, Tabs,
    TextField,
};
use choscorui_core::{
    Color, DisplayCommand, DisplayList, Key, LayoutDirection, Point, SemanticAction, SemanticRole,
    Tree,
};
use hosted::{Harness, rect, rounded_fills, texts};

fn number(value: f64) -> ControlMessage {
    ControlMessage::Changed(ControlValue::Number(value))
}

fn index(value: usize) -> ControlMessage {
    ControlMessage::Changed(ControlValue::Index(Some(value)))
}

fn text(value: &str) -> ControlMessage {
    ControlMessage::Changed(ControlValue::Text(value.to_owned()))
}

/// The baseline x of the text run `label` in `list`.
fn text_x(list: &DisplayList, label: &str) -> f64 {
    list.commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawText { baseline, text, .. } if text == label => Some(baseline.x),
            _ => None,
        })
        .unwrap()
}

fn volume() -> Slider {
    Slider::new(
        "Volume",
        IntegerRange::new(0, 100, 25, 1, 10).unwrap(),
        SliderOrientation::Horizontal,
    )
}

/// The point at `fraction` along the bounds of `h`, at its middle.
fn along(h: &Harness, fraction: f64) -> Point {
    let bounds = h.bounds();
    Point::new(
        bounds.origin.x + bounds.size.width * fraction,
        bounds.center().y,
    )
}

#[test]
fn a_hosted_label_and_separator_take_no_input() {
    // A label names its node only for `set_text`; the host ignores it.
    let scratch = Tree::new(rect(0.0, 0.0, 10.0, 10.0));
    let mut h = Harness::new(Label::new(scratch.root(), "Email"));
    assert_eq!(h.semantics().role, SemanticRole::StaticText);
    assert_eq!(h.semantics().name, "Email");
    assert!(!h.host.get(h.node).unwrap().focusable());
    let center = h.bounds().center();
    h.click(center);
    assert!(h.messages().is_empty());
    assert_eq!(texts(&h.paint()), vec!["Email"]);

    let mut h = Harness::new(Separator::new(SeparatorOrientation::Horizontal));
    assert_eq!(h.measure().height, 1.0);
    assert!(h.semantics().state.hidden);
    assert!(!h.host.get(h.node).unwrap().focusable());
    assert!(!h.perform(SemanticAction::Activate, None));
}

#[test]
fn a_hosted_progress_reports_its_value_and_sweeps_while_indeterminate() {
    let h = Harness::new(Progress::new("Upload", 3, 10).unwrap());
    assert_eq!(h.semantics().role, SemanticRole::ProgressIndicator);
    assert_eq!(h.semantics().value.as_deref(), Some("30%"));
    assert!(!h.host.get(h.node).unwrap().focusable());
    assert!(!h.host.animating(&h.tree, 0.0));

    let h = Harness::new(Progress::indeterminate("Upload"));
    assert_eq!(h.semantics().value, None);
    assert!(h.host.animating(&h.tree, 0.0));
}

#[test]
fn a_hosted_slider_reports_pointer_key_and_assistive_changes() {
    let mut h = Harness::new(volume());
    assert_eq!(h.semantics().role, SemanticRole::Slider);
    assert_eq!(h.semantics().name, "Volume");
    assert_eq!(h.semantics().value.as_deref(), Some("25"));

    h.click(along(&h, 0.5));
    h.focus();
    h.key(Key::ArrowRight);
    h.key(Key::Home);
    // A step past the minimum reports nothing.
    h.key(Key::ArrowLeft);
    assert!(h.perform(SemanticAction::Increment, None));
    assert!(h.perform(SemanticAction::SetValue, Some("40")));
    assert!(!h.perform(SemanticAction::SetValue, Some("loud")));
    assert_eq!(
        h.messages(),
        vec![
            number(50.0),
            number(51.0),
            number(0.0),
            number(1.0),
            number(40.0),
        ]
    );

    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Number(70.0))
    );
    assert!(
        !h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Bool(true))
    );
    assert_eq!(h.semantics().value.as_deref(), Some("70"));
    assert!(h.messages().is_empty());

    h.set_enabled(false);
    h.click(along(&h, 0.1));
    h.key(Key::ArrowRight);
    assert!(!h.perform(SemanticAction::Increment, None));
    assert!(h.messages().is_empty());
    assert_eq!(h.semantics().value.as_deref(), Some("70"));
}

#[test]
fn a_hosted_slider_mirrors_in_right_to_left_layouts() {
    let ltr = Harness::new(volume());
    let mut rtl = Harness::with_direction(volume(), LayoutDirection::Rtl);
    let thumb = |h: &Harness| rounded_fills(&h.paint(), Color::WHITE)[0];
    let middle = ltr.bounds().center().x;
    assert!(thumb(&ltr).center().x < middle);
    assert!(thumb(&rtl).center().x > middle);

    // A press near the left end of a right-to-left slider is near the
    // maximum.
    rtl.click(along(&rtl, 0.1));
    assert_eq!(rtl.messages(), vec![number(90.0)]);
}

#[test]
fn a_hosted_spin_box_steps_by_pointer_key_and_action() {
    let range = IntegerRange::new(0, 10, 5, 1, 1).unwrap();
    let mut h = Harness::new(SpinBox::new("Copies", range));
    assert_eq!(h.semantics().role, SemanticRole::SpinButton);
    assert_eq!(h.semantics().name, "Copies");
    let bounds = h.bounds();
    // The top half of the stepper column at the end steps up.
    h.click(Point::new(bounds.max_x() - 4.0, bounds.origin.y + 4.0));
    h.focus();
    h.key(Key::ArrowUp);
    h.key(Key::End);
    assert!(h.perform(SemanticAction::Decrement, None));
    assert!(h.perform(SemanticAction::SetValue, Some("3")));
    assert_eq!(
        h.messages(),
        vec![
            number(6.0),
            number(7.0),
            number(10.0),
            number(9.0),
            number(3.0)
        ]
    );

    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Number(8.0))
    );
    assert_eq!(h.semantics().value.as_deref(), Some("8"));
    assert!(h.messages().is_empty());

    h.set_enabled(false);
    h.key(Key::ArrowUp);
    assert!(!h.perform(SemanticAction::Increment, None));
    assert!(h.messages().is_empty());

    // The steppers sit at the left in a right-to-left layout.
    let mut rtl = Harness::with_direction(SpinBox::new("Copies", range), LayoutDirection::Rtl);
    let bounds = rtl.bounds();
    rtl.click(Point::new(bounds.origin.x + 4.0, bounds.max_y() - 4.0));
    assert_eq!(rtl.messages(), vec![number(4.0)]);
}

#[test]
fn a_hosted_text_field_reports_edits_and_commits() {
    let mut h = Harness::new(TextField::new("Name", "Ada"));
    assert_eq!(h.semantics().role, SemanticRole::TextField);
    assert_eq!(h.semantics().name, "Name");
    assert_eq!(h.semantics().value.as_deref(), Some("Ada"));
    // Text without focus does nothing.
    assert!(!h.text("x"));

    h.focus();
    assert!(h.text("m"));
    h.key(Key::Backspace);
    h.key(Key::Enter);
    assert!(h.perform(SemanticAction::SetValue, Some("Grace")));
    assert_eq!(
        h.messages(),
        vec![
            text("Adam"),
            text("Ada"),
            ControlMessage::Activated,
            text("Grace"),
        ]
    );

    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Text("Linus".into()))
    );
    assert_eq!(h.semantics().value.as_deref(), Some("Linus"));
    assert!(h.messages().is_empty());

    // A press at the start of the text puts the caret there.
    let _ = h.paint();
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 1.0, bounds.center().y));
    assert!(h.text("X"));
    assert_eq!(h.messages(), vec![text("XLinus")]);

    h.set_enabled(false);
    assert!(!h.text("y"));
    assert!(!h.perform(SemanticAction::SetValue, Some("Z")));
    assert!(h.messages().is_empty());
    assert!(h.semantics().state.disabled);
}

fn delivery() -> impl CustomControl {
    RadioGroup::hosted("Delivery", ["Standard", "Express", "Pickup"], 0)
        .unwrap()
        .with_disabled(1)
        .unwrap()
}

#[test]
fn a_hosted_radio_group_selects_by_pointer_key_and_action() {
    let mut h = Harness::new(delivery());
    assert_eq!(h.semantics().role, SemanticRole::RadioGroup);
    assert_eq!(h.semantics().name, "Delivery");
    assert_eq!(h.semantics().value.as_deref(), Some("Standard"));
    assert_eq!(texts(&h.paint()), vec!["Standard", "Express", "Pickup"]);

    let bounds = h.bounds();
    let start = bounds.origin.x + 4.0;
    // The disabled middle option ignores a press.
    h.click(Point::new(start, bounds.center().y));
    h.click(Point::new(start, bounds.max_y() - 2.0));
    h.focus();
    // Arrow keys wrap and skip the disabled option.
    h.key(Key::ArrowDown);
    h.key(Key::ArrowUp);
    // Selection follows focus, so Space selects nothing new.
    h.key(Key::Space);
    assert!(h.perform(SemanticAction::SetValue, Some("Standard")));
    assert!(h.perform(SemanticAction::Increment, None));
    assert!(!h.perform(SemanticAction::SetValue, Some("Express")));
    assert_eq!(
        h.messages(),
        vec![index(2), index(0), index(2), index(0), index(2)]
    );

    let take = |h: &mut Harness, value| h.host.set_value(&mut h.tree, h.node, &value);
    assert!(take(&mut h, ControlValue::Index(Some(0))));
    assert!(!take(&mut h, ControlValue::Index(Some(1))));
    assert!(!take(&mut h, ControlValue::Index(None)));
    assert_eq!(h.semantics().value.as_deref(), Some("Standard"));
    assert!(h.messages().is_empty());

    h.set_enabled(false);
    h.click(Point::new(start, bounds.max_y() - 2.0));
    h.key(Key::ArrowDown);
    assert!(!h.perform(SemanticAction::Increment, None));
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_radio_group_or_tab_list_rejects_a_bad_selection() {
    let empty: [&str; 0] = [];
    assert_eq!(
        RadioGroup::hosted("Delivery", empty, 0).err(),
        Some(SelectionError::Empty)
    );
    assert_eq!(
        Tabs::hosted("Sections", ["Account"], 1).err(),
        Some(SelectionError::UnknownSelection)
    );
    assert_eq!(
        RadioGroup::hosted("Delivery", ["Standard", "Pickup"], 0)
            .unwrap()
            .with_disabled(0)
            .err(),
        Some(SelectionError::DisabledSelection)
    );
}

fn sections() -> impl CustomControl {
    Tabs::hosted("Sections", ["Account", "Password", "Billing"], 0).unwrap()
}

#[test]
fn a_hosted_tab_list_selects_by_pointer_key_and_action() {
    let mut h = Harness::new(sections());
    assert_eq!(h.semantics().role, SemanticRole::TabList);
    assert_eq!(h.semantics().name, "Sections");
    assert_eq!(h.semantics().value.as_deref(), Some("Account"));

    h.click(along(&h, 0.97));
    h.focus();
    h.key(Key::ArrowRight);
    h.key(Key::End);
    h.key(Key::Home);
    assert!(h.perform(SemanticAction::Decrement, None));
    assert!(h.perform(SemanticAction::SetValue, Some("1")));
    assert_eq!(
        h.messages(),
        vec![index(2), index(0), index(2), index(0), index(2), index(1)]
    );

    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Index(Some(0)))
    );
    assert_eq!(h.semantics().value.as_deref(), Some("Account"));
    assert!(h.messages().is_empty());

    h.set_enabled(false);
    h.click(along(&h, 0.97));
    h.key(Key::ArrowRight);
    assert!(h.messages().is_empty());
    assert!(h.semantics().state.disabled);
}

#[test]
fn a_hosted_tab_list_mirrors_in_right_to_left_layouts() {
    let ltr = Harness::new(sections());
    let mut rtl = Harness::with_direction(sections(), LayoutDirection::Rtl);
    let (list, mirrored) = (ltr.paint(), rtl.paint());
    assert!(text_x(&list, "Account") < text_x(&list, "Billing"));
    assert!(text_x(&mirrored, "Account") > text_x(&mirrored, "Billing"));

    // The last tab is at the left in a right-to-left layout.
    rtl.click(along(&rtl, 0.03));
    assert_eq!(rtl.messages(), vec![index(2)]);
}
