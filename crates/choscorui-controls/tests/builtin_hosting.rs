//! The built-in controls through the control host: pointer, keys,
//! assistive actions, values from the application, and semantics.

mod hosted;

use choscorui_controls::{Button, Checkbox, CommandId, ControlMessage, ControlValue};
use choscorui_core::{Key, Point, SemanticAction, SemanticRole};
use hosted::{Harness, texts};

#[test]
fn a_hosted_button_activates_by_pointer_key_and_action() {
    let mut h = Harness::new(Button::new("Save", CommandId(1)));
    assert_eq!(h.semantics().role, SemanticRole::Button);
    assert_eq!(h.semantics().name, "Save");
    let center = h.bounds().center();
    h.click(center);
    h.focus();
    h.key(Key::Enter);
    h.key(Key::Space);
    assert!(h.perform(SemanticAction::Activate, None));
    assert_eq!(h.messages(), vec![ControlMessage::Activated; 4]);
    assert_eq!(texts(&h.paint()), vec!["Save"]);
    h.set_enabled(false);
    h.click(center);
    assert!(!h.perform(SemanticAction::Activate, None));
    assert!(h.messages().is_empty());
    assert!(h.semantics().state.disabled);
}

#[test]
fn a_hosted_checkbox_reports_and_takes_its_value() {
    let mut h = Harness::new(Checkbox::new("Remember me", false));
    let center = h.bounds().center();
    h.click(center);
    h.focus();
    h.key(Key::Space);
    assert!(h.perform(SemanticAction::SetValue, Some("true")));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Changed(ControlValue::Bool(true)),
            ControlMessage::Changed(ControlValue::Bool(false)),
            ControlMessage::Changed(ControlValue::Bool(true)),
        ]
    );
    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Mixed(None))
    );
    assert_eq!(h.semantics().value.as_deref(), Some("mixed"));
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_checkbox_can_start_invalid() {
    let h = Harness::new(Checkbox::new("Accept the terms", false).with_invalid(true));
    assert!(h.semantics().state.invalid);
}

#[test]
fn a_wide_hosted_checkbox_takes_presses_on_its_box_and_label_only() {
    let mut h = Harness::new(Checkbox::new("Remember me", false));
    let natural = h.bounds();
    h.place(hosted::rect(10.0, 10.0, 320.0, natural.size.height));
    let y = natural.center().y;
    h.click(Point::new(320.0, y));
    assert!(h.messages().is_empty());
    assert_ne!(h.tree.focus(), Some(h.node));
    h.click(Point::new(natural.max_x() - 2.0, y));
    assert_eq!(
        h.messages(),
        vec![ControlMessage::Changed(ControlValue::Bool(true))]
    );
}
