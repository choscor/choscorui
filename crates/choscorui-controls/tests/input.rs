//! Input and InputGroup through the control host.

mod hosted;

use choscorui_controls::{
    AddonAlign, ControlMessage, ControlSize, ControlValue, Input, InputGroup, InputGroupAddon,
    ShortcutModifiers, TextField, ValidationState,
};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, Icon, Key, Point, SemanticAction, SemanticRole,
};
use hosted::{Harness, texts};

fn typed(text: &str) -> ControlMessage {
    ControlMessage::Changed(ControlValue::Text(text.into()))
}

#[test]
fn input_takes_focus_on_click_and_edits_with_keys_and_text() {
    let mut h = Harness::new(Input::new("Email", "").with_placeholder("you@example.com"));
    let size = h.measure();
    assert_eq!(
        (size.width, size.height),
        (240.0, 36.0),
        "60 spacing units wide"
    );
    assert_eq!(texts(&h.paint()), ["you@example.com"]);
    h.click(h.bounds().center());
    assert!(h.text("Hi"));
    h.key(Key::Enter);
    assert_eq!(h.messages(), [typed("Hi"), ControlMessage::Activated]);
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::TextField);
    assert_eq!(node.value.as_deref(), Some("Hi"));

    h.modifiers(ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    });
    h.key(Key::ArrowLeft);
    h.modifiers(ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    });
    h.key(Key::Character('a'));
    h.modifiers(ShortcutModifiers::default());
    h.key(Key::Backspace);
    assert_eq!(
        h.semantics().value.as_deref(),
        Some(""),
        "select all, then delete"
    );
}

#[test]
fn a_click_places_the_caret_nearest_the_press() {
    let mut h = Harness::new(Input::new("Name", "abcd"));
    h.focus();
    let list = h.paint();
    let start = list
        .commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawText { baseline, .. } => Some(baseline.x),
            _ => None,
        })
        .unwrap();
    h.click(Point::new(start + 0.5, h.bounds().center().y));
    h.text("X");
    assert_eq!(h.semantics().value.as_deref(), Some("Xabcd"));
}

#[test]
fn invalid_and_disabled_inputs() {
    let mut input = Input::password("Password", "secret");
    input.set_validation(ValidationState::Invalid("Too short".into()));
    let mut h = Harness::new(input);
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::PasswordField);
    assert!(node.state.invalid);
    assert_eq!(node.description, "Too short");
    assert_ne!(node.value.as_deref(), Some("secret"));
    let destructive = DesignTokens::light().color(ColorRole::Destructive);
    assert!(h.paint().commands().iter().any(|command| matches!(
        command,
        DisplayCommand::StrokeRoundedRect { color, .. } if *color == destructive
    )));

    h.set_enabled(false);
    h.click(h.bounds().center());
    h.text("x");
    assert!(h.messages().is_empty());
    assert!(!h.perform(SemanticAction::SetValue, Some("new")));
}

fn search() -> InputGroup {
    InputGroup::new(
        TextField::new("Search", "").with_placeholder("Search…"),
        vec![
            InputGroupAddon::icon(AddonAlign::InlineStart, Icon::Search),
            InputGroupAddon::text(AddonAlign::InlineEnd, "12 results"),
            InputGroupAddon::button(
                AddonAlign::InlineEnd,
                "Clear",
                Some(Icon::X),
                ControlSize::IconXs,
            ),
        ],
    )
}

#[test]
fn input_group_lays_out_addons_and_activates_buttons() {
    let mut h = Harness::new(search());
    assert_eq!(h.measure().height, 36.0);
    let bounds = h.bounds();
    let list = h.paint();
    assert_eq!(texts(&list), ["12 results", "Search…"]);
    // The placeholder starts after the 16 pt icon and its padding.
    let placeholder = list
        .commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText { baseline, text, .. } if text == "Search…" => {
                Some(baseline.x)
            }
            _ => None,
        })
        .next()
        .unwrap();
    assert!(placeholder > bounds.origin.x + 16.0 + 8.0);

    // The clear button is the last addon at the trailing edge.
    h.click(Point::new(bounds.max_x() - 12.0, bounds.center().y));
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(2)]);
    h.click(bounds.center());
    h.text("cat");
    assert_eq!(h.messages(), [typed("cat")]);
    let node = h.semantics();
    assert_eq!(node.value.as_deref(), Some("cat"));
    assert_eq!(node.description, "12 results");
}

#[test]
fn block_addons_stack_above_and_below_the_text() {
    let group = InputGroup::new(
        TextField::new("Message", ""),
        vec![
            InputGroupAddon::text(AddonAlign::BlockStart, "To: team"),
            InputGroupAddon::button(
                AddonAlign::BlockEnd,
                "Send",
                Some(Icon::Send),
                ControlSize::Sm,
            ),
        ],
    );
    let mut h = Harness::new(group);
    let height = h.measure().height;
    assert!(
        height > 36.0 + 28.0,
        "both block addons add height: {height}"
    );
}
