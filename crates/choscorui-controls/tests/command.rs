//! The Command palette through the control host.

mod hosted;

use choscorui_controls::{Command, CommandItem, ControlMessage, ControlValue};
use choscorui_core::{Icon, Key, Point, SemanticAction, SemanticRole};
use hosted::{Harness, texts};

fn groups() -> Vec<(String, Vec<CommandItem>)> {
    vec![
        (
            "Suggestions".to_string(),
            vec![
                CommandItem::new("Calendar").with_icon(Icon::Calendar),
                CommandItem::new("Search Emoji").with_icon(Icon::Search),
                CommandItem::new("Calculator").disabled(),
            ],
        ),
        (
            "Settings".to_string(),
            vec![
                CommandItem::new("Profile").with_shortcut("⌘P"),
                CommandItem::new("Billing").with_shortcut("⌘B"),
                CommandItem::new("Settings").with_shortcut("⌘S"),
            ],
        ),
    ]
}

fn palette() -> Command {
    Command::new("Commands", "Type a command or search…", groups()).with_empty("No results found.")
}

#[test]
fn typing_filters_and_enter_runs_the_highlighted_command() {
    let mut h = Harness::new(palette());
    let full = h.measure().height;
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::ComboBox);
    assert_eq!(h.focused_part(), "Calendar", "the first enabled command");
    h.focus();
    assert!(h.text("s"));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Text("s".into()))]
    );
    assert_eq!(h.focused_part(), "Search Emoji");
    assert!(h.measure().height < full, "fewer rows are shorter");
    h.key(Key::ArrowDown);
    assert_eq!(h.focused_part(), "Settings");
    h.key(Key::ArrowDown);
    assert_eq!(h.focused_part(), "Settings", "it stops at the end");
    h.key(Key::Enter);
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(5)]);
    assert!(h.perform(SemanticAction::SetValue, Some("zzz")));
    assert_eq!(h.semantics().description, "No results found.");
    let size = h.measure();
    h.place(choscorui_core::Rect::from_xywh(
        10.0,
        10.0,
        size.width,
        size.height,
    ));
    assert!(texts(&h.paint()).contains(&"No results found.".to_string()));
}

#[test]
fn arrows_skip_disabled_commands_and_the_pointer_runs_commands() {
    let mut h = Harness::new(palette());
    h.focus();
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    assert_eq!(h.focused_part(), "Profile", "Calculator is skipped");
    let painted = texts(&h.paint());
    for text in ["Suggestions", "Calendar", "Settings", "⌘P"] {
        assert!(painted.contains(&text.to_string()), "{text}");
    }
    // Rows start below the 32 pt input and the 4 pt padding of the
    // palette and the group. The group label comes first.
    let bounds = h.bounds();
    let label = 28.0;
    let calendar = Point::new(
        bounds.center().x,
        bounds.origin.y + 4.0 + 32.0 + 4.0 + label + 16.0,
    );
    h.hover(calendar);
    assert_eq!(h.focused_part(), "Calendar");
    h.messages();
    h.click(calendar);
    assert_eq!(h.messages(), [ControlMessage::ItemActivated(0)]);
    let calculator = Point::new(calendar.x, calendar.y + 64.0);
    h.click(calculator);
    assert!(h.messages().is_empty(), "a disabled command does not run");
}

#[test]
fn the_dialog_form_opens_filters_and_closes() {
    let mut h = Harness::new(
        Command::dialog("Open palette", "Commands", "Type a command…", groups())
            .with_empty("No results found."),
    );
    assert_eq!(h.semantics().role, SemanticRole::Button);
    h.focus();
    h.key(Key::Enter);
    assert_eq!(h.host.modal(&h.tree), Some(h.node));
    assert_eq!(h.semantics().role, SemanticRole::ComboBox);
    assert!(h.text("bil"));
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(true),
            ControlMessage::Changed(ControlValue::Text("bil".into())),
            ControlMessage::ItemActivated(4),
            ControlMessage::OpenChanged(false),
        ]
    );
    assert!(h.perform(SemanticAction::Activate, None));
    assert_eq!(
        h.semantics().value.as_deref(),
        Some(""),
        "the search starts empty"
    );
    h.key(Key::Escape);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(true),
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed,
        ]
    );
    h.key(Key::Space);
    h.click(Point::new(5.0, 590.0));
    assert_eq!(h.host.modal(&h.tree), None, "a press outside closes it");
}

#[test]
fn a_large_palette_publishes_only_the_rows_in_view() {
    let items = (0..10_000)
        .map(|index| CommandItem::new(format!("Command {index}")))
        .collect();
    let mut h = Harness::new(Command::new(
        "Commands",
        "Search",
        vec![(String::new(), items)],
    ));
    h.measure();
    let parts = h.parts();
    let list = &parts.parts()[0];
    assert_eq!(list.node.role, SemanticRole::List);
    assert_eq!(list.collection.unwrap().item_count, 10_000);
    assert!(parts.len() < 20, "{} parts", parts.len());
    assert_eq!(h.focused_part(), "Command 0");
    h.focus();
    for _ in 0..3 {
        h.key(Key::ArrowDown);
    }
    let parts = h.parts();
    let last = &parts.parts()[parts.focused().unwrap()];
    assert_eq!(last.node.name, "Command 3");
    assert_eq!(last.item_index, Some(3));
    assert!(parts.len() < 20, "{} parts", parts.len());
}
