//! Input and InputGroup through the control host.

mod hosted;

use choscorui_controls::{
    AddonAlign, ControlMessage, ControlSize, ControlValue, Input, InputGroup, InputGroupAddon,
    InputOtp, ShortcutModifiers, TextField, Textarea, ValidationState,
};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, Icon, Key, Point, SemanticAction, SemanticRole,
};
use hosted::{Harness, paints, texts};

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

#[test]
fn otp_fills_slots_in_order_and_completes() {
    let mut h = Harness::new(InputOtp::new("Code", 6).with_groups(vec![3, 3]));
    // Six 36 pt slots, and a 16 pt separator with 8 pt gaps.
    assert_eq!(h.measure().width, 6.0 * 36.0 + 16.0 + 16.0);
    h.click(h.bounds().center());
    h.text("12a3");
    assert_eq!(h.semantics().value.as_deref(), Some("123"), "digits only");
    h.key(Key::Backspace);
    h.text("4567890");
    assert_eq!(
        h.messages(),
        [
            typed("123"),
            typed("12"),
            typed("124567"),
            ControlMessage::Activated,
        ]
    );
    assert_eq!(texts(&h.paint()), ["1", "2", "4", "5", "6", "7"]);
    assert!(h.perform(SemanticAction::SetValue, Some("9 8 7")));
    assert_eq!(h.semantics().value.as_deref(), Some("987"));
    assert_eq!(h.semantics().role, SemanticRole::TextField);
}

#[test]
fn otp_marks_the_active_slot_with_the_ring_while_focused() {
    let mut h = Harness::new(InputOtp::new("Code", 4).alphanumeric());
    let ring = DesignTokens::light().color(ColorRole::Ring);
    let rings = |h: &Harness| paints(&h.paint(), ring);
    assert!(!rings(&h));
    h.focus();
    h.text("ab");
    assert!(rings(&h), "the third slot shows the ring border");
}

#[test]
fn textarea_wraps_lines_and_grows() {
    let mut h = Harness::new(Textarea::new("Notes", "").with_placeholder("Type here"));
    assert_eq!(h.measure().height, 64.0, "the 64 pt minimum");
    h.focus();
    let long = "word ".repeat(40);
    h.text(&long);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Text(long.clone()))]
    );
    let grown = h.measure().height;
    assert!(grown > 64.0, "wrapped lines grow it: {grown}");
    h.place(hosted::rect(10.0, 10.0, 240.0, grown));
    let painted = texts(&h.paint());
    assert!(painted.len() > 2, "one text run per line");
    assert!(painted.iter().all(|line| line.starts_with("word")));
    h.text("\nend");
    assert!(texts(&h.paint()).last().is_some_and(|line| line == "end"));
}

#[test]
fn native_select_keeps_the_chevron_in_the_end_padding() {
    use choscorui_controls::{ListEntry, Select};
    let mut h = Harness::new(Select::native(
        "Status",
        vec![ListEntry::option("Todo"), ListEntry::option("Done")],
    ));
    let size = h.measure();
    assert_eq!(size.height, 36.0);
    h.click(h.bounds().center());
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(true)]);
}
