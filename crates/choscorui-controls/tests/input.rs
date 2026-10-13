//! Input and InputGroup through the control host.

mod hosted;

use choscorui_controls::{
    AddonAlign, Checkbox, ControlMessage, ControlSize, ControlValue, EstimatedText, FocusFrame,
    Input, InputGroup, InputGroupAddon, InputOtp, PaintContext, ShortcutModifiers, Styles,
    TextField, Textarea, ValidationState,
};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, DisplayList, Icon, Key, Point, RadiusScale,
    SemanticAction, SemanticRole,
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
fn input_group_button_addons_take_focus_and_keep_the_validation_message() {
    let mut group = search();
    group.set_validation(ValidationState::Invalid("Too long".into()));
    let mut h = Harness::new(group);
    // The addon text follows the validation message.
    assert_eq!(h.semantics().description, "Too long 12 results");
    // The slot around the button is not a hit target: only the button is.
    let bounds = h.bounds();
    h.click(Point::new(bounds.max_x() - 12.0, bounds.origin.y + 2.0));
    assert!(
        !h.messages().contains(&ControlMessage::ItemActivated(2)),
        "a press above the button does not activate it"
    );
    h.focus();
    h.key(Key::Tab);
    let node = h.semantics();
    assert_eq!(
        (node.role, node.name.as_str()),
        (SemanticRole::Button, "Clear")
    );
    h.text("x");
    h.key(Key::Space);
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(2)]);
    h.modifiers(ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    });
    h.key(Key::Tab);
    h.modifiers(ShortcutModifiers::default());
    assert_eq!(h.semantics().name, "Search");
    h.text("x");
    assert_eq!(h.messages(), [typed("x")]);
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
    assert_eq!(
        h.semantics().value.as_deref(),
        Some(""),
        "a paste with a letter is rejected as a whole"
    );
    h.text("1");
    h.text("2");
    h.text("a");
    h.text("3");
    assert_eq!(h.semantics().value.as_deref(), Some("123"), "digits only");
    h.key(Key::Backspace);
    h.text("4");
    // A paste replaces the slots from the first slot, and full-width
    // digits become ASCII digits.
    h.text("９87654321");
    assert_eq!(
        h.messages(),
        [
            typed("1"),
            typed("12"),
            typed("123"),
            typed("12"),
            typed("124"),
            typed("987654"),
            ControlMessage::Activated,
        ]
    );
    assert_eq!(texts(&h.paint()), ["9", "8", "7", "6", "5", "4"]);
    let mut letters = Harness::new(InputOtp::new("Code", 4).alphanumeric());
    letters.focus();
    letters.text("aé");
    letters.text("b1");
    assert_eq!(
        letters.semantics().value.as_deref(),
        Some("b1"),
        "the alphanumeric kind takes ASCII only"
    );
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
fn invalid_otp_rings_its_group_until_it_has_focus() {
    let tokens = DesignTokens::light();
    let soft = tokens.color(ColorRole::Destructive).with_alpha(0.2);
    let edge = tokens.color(ColorRole::Destructive);
    let mut otp = InputOtp::new("Code", 4);
    otp.set_invalid(true);
    let mut h = Harness::new(otp);
    let ring = |h: &Harness| {
        h.paint()
            .commands()
            .iter()
            .filter(|command| {
                matches!(command, DisplayCommand::StrokeRoundedRect { color, .. } if *color == soft)
            })
            .count()
    };
    assert_eq!(ring(&h), 1, "one ring around the group");
    assert!(paints(&h.paint(), edge), "the slot edges");
    h.focus();
    // Past the ring transition.
    h.paint();
    h.set_time(10.0);
    assert_eq!(ring(&h), 0, "the active slot shows the focus ring");
    assert!(paints(
        &h.paint(),
        tokens.ring_color(tokens.color(ColorRole::Ring))
    ));
}

#[test]
fn focus_frame_draws_the_ring_of_the_focused_control() {
    let mut h = Harness::new(Checkbox::new("Terms", false).with_invalid(true));
    h.focus();
    h.paint();
    let mut list = DisplayList::new();
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let mut cx = PaintContext::new(&mut list, h.bounds(), &tokens, &styles, &EstimatedText);
    FocusFrame.paint(&mut cx, &h.tree, &h.host);
    let ring = tokens.ring_color(tokens.color(ColorRole::Ring));
    // The 4 pt box radius, not the md radius of the theme ring.
    assert!(
        list.commands().iter().any(|command| matches!(
            command,
            DisplayCommand::StrokeRoundedRect { color, radius, .. }
                if *color == ring && *radius > 4.0 && *radius < tokens.radius(RadiusScale::Md)
        )),
        "{:?}",
        list.commands()
    );
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

/// The baseline height of the first text run `text` in `list`.
fn baseline_of(list: &DisplayList, text: &str) -> f64 {
    list.commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawText {
                baseline,
                text: run,
                ..
            } if run == text => Some(baseline.y),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{text} is not painted"))
}

fn wheel(h: &mut Harness, dy: f64) {
    h.host.dispatch_pointer(
        &mut h.tree,
        choscorui_core::PointerEvent::Wheel {
            position: Point::new(50.0, 40.0),
            delta: Point::new(0.0, dy),
            unit: choscorui_core::WheelUnit::Points,
        },
    );
}

fn numbered_lines(count: usize) -> String {
    (0..count)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_textarea_stops_at_its_maximum_height_and_scrolls_to_the_caret() {
    let mut h = Harness::new(Textarea::new("Notes", "").with_max_height(100.0));
    h.focus();
    h.text(&numbered_lines(20));
    let height = h.measure().height;
    assert_eq!(height, 100.0);
    h.place(hosted::rect(10.0, 10.0, 240.0, height));
    // The caret is on the last line, so the text scrolls to show it.
    let last = baseline_of(&h.paint(), "line 19");
    assert!((10.0..=110.0).contains(&last), "{last}");
    assert!(baseline_of(&h.paint(), "line 0") < 10.0);
    // The wheel scrolls back to the top.
    wheel(&mut h, -1.0e4);
    let first = baseline_of(&h.paint(), "line 0");
    assert!((10.0..=40.0).contains(&first), "{first}");
}

#[test]
fn a_hosted_text_editor_scrolls_with_the_wheel_and_follows_the_caret() {
    use choscorui_controls::TextEditor;
    let mut h = Harness::new(TextEditor::new("Doc", numbered_lines(30)));
    h.place(hosted::rect(10.0, 10.0, 240.0, 120.0));
    h.focus();
    let top = baseline_of(&h.paint(), "line 0");
    // The overlay scroll bar paints its thumb at the trailing edge.
    let thumb = DesignTokens::light().color(ColorRole::Border);
    let thumbs = hosted::rounded_fills(&h.paint(), thumb);
    assert!(thumbs.iter().any(|rect| rect.min_x() > 200.0), "{thumbs:?}");
    wheel(&mut h, 60.0);
    assert_eq!(baseline_of(&h.paint(), "line 0"), top - 60.0);
    wheel(&mut h, -1.0e4);
    assert_eq!(baseline_of(&h.paint(), "line 0"), top);
    // Moving the caret down past the bottom scrolls it into view.
    for _ in 0..29 {
        h.key(Key::ArrowDown);
    }
    let last = baseline_of(&h.paint(), "line 29");
    assert!((10.0..=130.0).contains(&last), "{last}");
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
