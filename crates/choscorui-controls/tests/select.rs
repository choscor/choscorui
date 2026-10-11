//! Select through the control host: pointer, keys, type-ahead, scrolling,
//! the open animation, and semantics.

mod hosted;

use choscorui_controls::{ControlMessage, ControlSize, ControlValue, ListEntry, Select};
use choscorui_core::{
    DesignTokens, DisplayCommand, Key, Point, SemanticAction, SemanticRole, ThemeResolver,
};
use hosted::{Harness, texts};

fn fruit() -> Select {
    Select::new(
        "Fruit",
        vec![
            ListEntry::group("Fruits"),
            ListEntry::option("Apple"),
            ListEntry::option("Banana"),
            ListEntry::option("Blueberry"),
            ListEntry::disabled("Grapes"),
            ListEntry::Separator,
            ListEntry::group("Vegetables"),
            ListEntry::option("Carrot"),
        ],
    )
    .with_placeholder("Select a fruit")
}

fn overlay(h: &Harness) -> Option<choscorui_core::Rect> {
    h.host.get(h.node).unwrap().overlay()
}

#[test]
fn click_opens_the_list_below_and_a_click_on_an_option_chooses_it() {
    let mut h = Harness::new(fruit());
    assert_eq!(h.measure().height, 36.0, "the default trigger height");
    assert_eq!(texts(&h.paint()), ["Select a fruit"]);
    let bounds = h.bounds();
    h.click(bounds.center());
    assert_eq!(h.messages(), [ControlMessage::OpenChanged(true)]);
    let frame = overlay(&h).expect("the list is open");
    assert_eq!(
        frame.origin.y,
        bounds.max_y() + 4.0,
        "4 pt below the trigger"
    );
    assert_eq!(frame.origin.x, bounds.origin.x);
    assert!(frame.size.width >= bounds.size.width);

    // The open animation has finished one second later.
    h.set_time(1.0);
    let painted = texts(&h.paint());
    assert_eq!(
        painted,
        [
            "Select a fruit",
            "Fruits",
            "Apple",
            "Banana",
            "Blueberry",
            "Grapes",
            "Vegetables",
            "Carrot",
        ]
    );
    // Rows: content padding 4, group label 28, then 32 pt options.
    let banana = Point::new(frame.center().x, frame.origin.y + 4.0 + 28.0 + 32.0 + 16.0);
    h.click(banana);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Changed(ControlValue::Index(Some(2))),
            ControlMessage::OpenChanged(false),
        ]
    );
    assert_eq!(overlay(&h), None);
    assert_eq!(texts(&h.paint())[0], "Banana");
}

#[test]
fn keys_open_move_over_enabled_options_choose_and_dismiss() {
    let mut h = Harness::new(fruit());
    h.focus();
    h.key(Key::ArrowDown);
    assert!(h.semantics().state.expanded == Some(true));
    assert_eq!(h.semantics().description, "Apple", "the first option");
    h.key(Key::ArrowUp);
    assert_eq!(
        h.semantics().description,
        "Apple",
        "arrows stop at the ends"
    );
    h.key(Key::End);
    assert_eq!(h.semantics().description, "Carrot");
    h.key(Key::ArrowUp);
    assert_eq!(h.semantics().description, "Blueberry", "Grapes is disabled");
    h.key(Key::Enter);
    assert_eq!(h.semantics().value.as_deref(), Some("Blueberry"));
    assert_eq!(h.semantics().state.expanded, Some(false));

    h.messages();
    h.key(Key::Space);
    assert_eq!(
        h.semantics().description,
        "Blueberry",
        "opens at the choice"
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
}

#[test]
fn type_ahead_chooses_while_closed_and_highlights_while_open() {
    let mut h = Harness::new(fruit());
    h.focus();
    assert!(h.text("c"));
    assert_eq!(h.semantics().value.as_deref(), Some("Carrot"));
    h.set_time(5.0);
    h.key(Key::Enter);
    // A repeated letter cycles through the options that start with it.
    h.text("b");
    assert_eq!(h.semantics().description, "Banana");
    h.text("b");
    assert_eq!(h.semantics().description, "Blueberry");
    h.set_time(7.0);
    h.text("a");
    assert_eq!(h.semantics().description, "Apple", "the search restarts");
    assert_eq!(h.semantics().value.as_deref(), Some("Carrot"));
}

#[test]
fn long_lists_scroll_to_keep_the_highlight_in_view() {
    let names = [
        "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel", "India",
        "Juliett", "Kilo", "Lima",
    ];
    let entries = names.iter().map(|name| ListEntry::option(*name)).collect();
    let mut h = Harness::new(Select::new("Letter", entries).with_size(ControlSize::Sm));
    assert_eq!(h.measure().height, 32.0, "the small trigger height");
    h.focus();
    h.key(Key::Enter);
    let frame = overlay(&h).unwrap();
    assert_eq!(frame.size.height, 8.0 * 32.0 + 8.0, "eight rows show");
    h.set_time(1.0);
    assert!(!texts(&h.paint()).contains(&"Lima".to_string()));
    h.key(Key::End);
    let painted = texts(&h.paint());
    assert!(painted.contains(&"Lima".to_string()));
    assert!(!painted.contains(&"Alpha".to_string()), "scrolled out");
}

#[test]
fn the_list_fades_and_zooms_in_and_reduced_motion_skips_it() {
    let mut h = Harness::new(fruit());
    h.click(h.bounds().center());
    let control = h.host.get(h.node).unwrap();
    assert!(control.animating(0.05));
    assert!(!control.animating(0.2), "the 100 ms content motion");
    h.set_time(0.05);
    let list = h.paint();
    let faded = list.commands().iter().find_map(|command| match command {
        DisplayCommand::PushOpacity(opacity) => Some(*opacity),
        _ => None,
    });
    assert!(faded.is_some_and(|opacity| opacity > 0.0 && opacity < 1.0));

    let mut tokens = DesignTokens::light();
    tokens.motion.reduced = true;
    h.themes = ThemeResolver::new(tokens);
    let list = h.paint();
    assert!(
        !list
            .commands()
            .iter()
            .any(|command| matches!(command, DisplayCommand::PushTransform(_))),
        "reduced motion shows the list at once"
    );
    assert!(texts(&list).contains(&"Carrot".to_string()));
}

#[test]
fn semantics_name_the_choice_and_assistive_actions_choose() {
    let mut h = Harness::new(fruit());
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::ComboBox);
    assert_eq!(node.value.as_deref(), Some("Select a fruit"));
    assert!(node.actions.contains(&SemanticAction::Expand));
    assert!(h.perform(SemanticAction::SetValue, Some("Carrot")));
    assert!(
        !h.perform(SemanticAction::SetValue, Some("Grapes")),
        "disabled"
    );
    assert_eq!(h.semantics().value.as_deref(), Some("Carrot"));
    assert!(h.perform(SemanticAction::Expand, None));
    assert!(h.semantics().actions.contains(&SemanticAction::Collapse));

    h.set_enabled(false);
    assert!(h.semantics().actions.is_empty());
    assert_eq!(overlay(&h), None, "disabling closes the list");
}
