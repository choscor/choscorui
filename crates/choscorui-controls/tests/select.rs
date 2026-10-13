//! Select through the control host: pointer, keys, type-ahead, scrolling,
//! the open animation, and semantics.

mod hosted;

use choscorui_controls::{ControlMessage, ControlSize, ControlValue, ListEntry, Select};
use choscorui_core::{
    DesignTokens, DisplayCommand, Key, Point, PointerButton, PointerEvent, SemanticAction,
    SemanticRole, ThemeResolver,
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

fn letters() -> Harness {
    let names = [
        "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel", "India",
        "Juliett", "Kilo", "Lima",
    ];
    let entries = names.iter().map(|name| ListEntry::option(*name)).collect();
    let select = Select::new("Letter", entries)
        .with_size(ControlSize::Sm)
        .with_placeholder("Letter");
    let mut h = Harness::new(select);
    h.click(h.bounds().center());
    h.set_time(1.0);
    h.messages();
    h
}

/// The first row the list paints.
fn first_row(h: &Harness) -> String {
    texts(&h.paint())[1].clone()
}

/// A point on the scroll-down button: the bottom edge of the open list.
fn down_button(h: &Harness) -> Point {
    let frame = overlay(h).unwrap();
    Point::new(frame.center().x, frame.max_y() - 4.0 - 2.0)
}

/// A point on the scroll-up button: the top edge of the open list.
fn up_button(h: &Harness) -> Point {
    let frame = overlay(h).unwrap();
    Point::new(frame.center().x, frame.origin.y + 4.0 + 2.0)
}

/// Sets the time and fires the control's due deadline.
fn advance(h: &mut Harness, time: f64) {
    h.set_time(time);
    h.host.advance(&mut h.tree, time);
}

#[test]
fn pressing_a_scroll_button_scrolls_one_row() {
    let mut h = letters();
    assert_eq!(first_row(&h), "Alpha");
    h.click(down_button(&h));
    assert!(
        h.messages().is_empty(),
        "a press on the button does not choose"
    );
    h.click(down_button(&h));
    assert_eq!(first_row(&h), "Bravo", "Alpha scrolled out");
    h.click(up_button(&h));
    h.click(up_button(&h));
    assert_eq!(first_row(&h), "Alpha");
}

#[test]
fn hovering_a_scroll_button_scrolls_over_time_until_the_end() {
    let mut h = letters();
    assert_eq!(h.host.next_deadline(&h.tree), None);
    h.hover(down_button(&h));
    let deadline = h.host.next_deadline(&h.tree).expect("a row step waits");
    assert!(deadline > 1.0 && deadline < 1.2);
    assert_eq!(first_row(&h), "Alpha", "no step before the interval");
    let mut time = 1.0;
    let mut steps = 0;
    while let Some(next) = h.host.next_deadline(&h.tree) {
        time = next;
        advance(&mut h, time);
        steps += 1;
        assert!(steps < 20, "the scroll stops at the end");
    }
    assert_eq!(steps, 4, "four rows bring Lima into view");
    let painted = texts(&h.paint());
    assert_eq!(painted.last().map(String::as_str), Some("Lima"));
    assert!(!painted.contains(&"Alpha".to_string()));

    // Hovering the scroll-up button scrolls back.
    h.hover(up_button(&h));
    advance(&mut h, time + 1.0);
    let next = h.host.next_deadline(&h.tree).expect("the scroll goes on");
    advance(&mut h, next);
    assert!(!texts(&h.paint()).contains(&"Lima".to_string()));
}

#[test]
fn leaving_a_scroll_button_stops_the_scroll() {
    let mut h = letters();
    h.hover(down_button(&h));
    advance(&mut h, 1.1);
    let painted = texts(&h.paint());
    // Moving onto a row stops the scroll.
    let frame = overlay(&h).unwrap();
    h.hover(frame.center());
    assert_eq!(h.host.next_deadline(&h.tree), None);
    advance(&mut h, 5.0);
    assert_eq!(texts(&h.paint()), painted);

    // Leaving the control stops it too.
    h.hover(down_button(&h));
    assert!(h.host.next_deadline(&h.tree).is_some());
    h.hover(Point::new(700.0, 500.0));
    assert_eq!(h.host.next_deadline(&h.tree), None);
}

#[test]
fn scroll_buttons_show_only_when_the_list_scrolls() {
    // A short list has no scroll buttons: its last row takes the press.
    let mut h = Harness::new(fruit());
    h.click(h.bounds().center());
    h.set_time(1.0);
    h.messages();
    let frame = overlay(&h).unwrap();
    let last = Point::new(frame.center().x, frame.max_y() - 4.0 - 2.0);
    h.hover(last);
    assert_eq!(h.host.next_deadline(&h.tree), None);
    h.click(last);
    assert_eq!(
        h.messages()[0],
        ControlMessage::Changed(ControlValue::Index(Some(7)))
    );

    // A long list at its top shows only the scroll-down button.
    let mut h = letters();
    h.hover(up_button(&h));
    assert_eq!(h.host.next_deadline(&h.tree), None, "no scroll-up button");
    h.hover(down_button(&h));
    assert!(h.host.next_deadline(&h.tree).is_some());
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
    assert_eq!(
        node.value.as_deref(),
        Some(""),
        "no choice is an empty value"
    );
    assert_eq!(node.description, "Select a fruit");
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

#[test]
fn near_the_bottom_edge_the_list_opens_above_the_trigger() {
    let mut h = Harness::new(fruit());
    let size = h.bounds().size;
    h.place(hosted::rect(10.0, 540.0, size.width, size.height));
    h.click(h.bounds().center());
    let frame = overlay(&h).expect("open");
    assert_eq!(frame.max_y(), 540.0 - 4.0, "4 pt above the trigger");
    assert!(frame.origin.y >= 8.0);
}

#[test]
fn near_the_right_edge_the_list_stays_inside_the_window() {
    let mut h = Harness::new(fruit());
    let size = h.bounds().size;
    h.place(hosted::rect(700.0, 100.0, 90.0, size.height));
    h.click(h.bounds().center());
    let frame = overlay(&h).expect("open");
    assert!(
        frame.size.width > 92.0,
        "wider than the room after the trigger"
    );
    assert_eq!(frame.max_x(), 792.0, "shifted inside the 8 pt margin");
    assert_eq!(frame.origin.y, h.bounds().max_y() + 4.0);
}

#[test]
fn in_a_short_window_the_list_shrinks_and_scrolls() {
    let names = [
        "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf",
    ];
    let entries = names.iter().map(|name| ListEntry::option(*name)).collect();
    let mut h = Harness::new(Select::new("Letter", entries));
    h.tree
        .set_bounds(h.tree.root(), hosted::rect(0.0, 0.0, 800.0, 160.0))
        .unwrap();
    h.place(h.bounds());
    h.click(h.bounds().center());
    let frame = overlay(&h).expect("open");
    assert_eq!(frame.max_y(), 152.0, "down to the bottom margin");
    h.key(Key::End);
    h.set_time(1.0);
    let painted = texts(&h.paint());
    assert!(
        painted.contains(&"Golf".to_string()) && !painted.contains(&"Alpha".to_string()),
        "the list scrolls to the last option: {painted:?}"
    );
}

#[test]
fn an_outside_press_or_disabling_closes_the_list_with_dismissed() {
    let mut h = Harness::new(fruit());
    h.click(h.bounds().center());
    h.messages();
    h.click(Point::new(700.0, 500.0));
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
    h.click(h.bounds().center());
    h.messages();
    h.set_enabled(false);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
    assert_eq!(overlay(&h), None);
}

#[test]
fn only_a_primary_button_release_chooses_an_option() {
    let mut h = Harness::new(fruit());
    h.click(h.bounds().center());
    h.set_time(1.0);
    h.messages();
    let frame = overlay(&h).unwrap();
    let banana = Point::new(frame.center().x, frame.origin.y + 4.0 + 28.0 + 32.0 + 16.0);
    let release = PointerEvent::Up {
        position: banana,
        button: PointerButton::Secondary,
    };
    h.host.dispatch_pointer(&mut h.tree, release);
    assert!(
        h.messages().is_empty(),
        "a secondary release chooses nothing"
    );
    assert!(overlay(&h).is_some());
}

#[test]
fn type_ahead_on_the_closed_trigger_starts_at_the_choice() {
    let mut h = Harness::new(fruit());
    h.focus();
    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Index(Some(2)))
    );
    h.text("b");
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Index(Some(3)))],
        "the search goes on after Banana to Blueberry"
    );
}

#[test]
fn a_native_select_is_a_pop_up_button_with_an_empty_value() {
    let h = Harness::new(Select::native("Status", vec![ListEntry::option("Open")]));
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::PopUpButton);
    assert_eq!(node.value.as_deref(), Some(""));
}
