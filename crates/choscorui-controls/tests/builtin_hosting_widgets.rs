//! The combobox, font chooser, text editor, and basic widgets through the
//! control host: input, assistive actions, values from the application,
//! the disabled state, semantics, and the combobox popup.

mod hosted;

use choscorui_controls::{
    Button, ButtonGroup, ButtonGroupMember, ButtonVariant, ComboItem, Combobox, CommandId,
    CommandLink, ControlMessage, ControlSize, ControlValue, DecimalSpinBox, Dial, FontChooser,
    IntegerRange, KeySequenceEdit, MenuItem, NumberBase, NumericDisplay, ScrollBar,
    ScrollOrientation, ShortcutModifiers, SizeGrip, TextEditor, ToolBox, ToolButton,
    ToolButtonMenu,
};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, Feedback, ItemKey, Key, KeyEvent, LayoutDirection,
    Locale, Paint, Point, PointerButton, PointerEvent, Rect, SemanticAction, SemanticRole, Size,
};
use hosted::{Harness, fills, texts};
use std::cell::Cell;
use std::rc::Rc;

fn press(h: &mut Harness, at: Point) {
    let event = PointerEvent::Down {
        position: at,
        button: PointerButton::Primary,
    };
    h.host.dispatch_pointer(&mut h.tree, event);
}

fn release(h: &mut Harness, at: Point) {
    let event = PointerEvent::Up {
        position: at,
        button: PointerButton::Primary,
    };
    h.host.dispatch_pointer(&mut h.tree, event);
}

fn set_value(h: &mut Harness, value: ControlValue) -> bool {
    h.host.set_value(&mut h.tree, h.node, &value)
}

fn changed(value: ControlValue) -> ControlMessage {
    ControlMessage::Changed(value)
}

fn fruits(editable: bool) -> Combobox {
    let mut combo = Combobox::new("Fruit", editable);
    let items = ["Apple", "Banana", "Cherry"]
        .iter()
        .enumerate()
        .map(|(index, text)| ComboItem {
            key: ItemKey(index as u64 + 1),
            text: (*text).to_string(),
        })
        .collect();
    combo.set_items(items);
    combo
}

/// The center of suggestion `row` in the open popup.
fn suggestion(h: &Harness, row: f64) -> Point {
    let popup = h.host.get(h.node).unwrap().overlay().unwrap();
    let inset = popup.origin.y + 4.0;
    Point::new(popup.center().x, inset + 32.0 * (row + 0.5))
}

#[test]
fn a_hosted_combobox_opens_chooses_and_closes_its_popup() {
    let mut h = Harness::new(fruits(false));
    assert_eq!(h.semantics().role, SemanticRole::ComboBox);
    assert_eq!(h.semantics().state.expanded, Some(false));
    let center = h.bounds().center();
    h.click(center);
    assert_eq!(h.messages(), vec![ControlMessage::OpenChanged(true)]);
    assert_eq!(h.semantics().state.expanded, Some(true));
    // The popup fades in, then paints above the trigger.
    assert!(texts(&h.paint()).is_empty());
    h.set_time(1.0);
    assert_eq!(texts(&h.paint()), vec!["Apple", "Banana", "Cherry"]);
    let banana = suggestion(&h, 1.0);
    h.click(banana);
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Index(Some(1))),
            ControlMessage::OpenChanged(false),
        ]
    );
    assert!(h.host.get(h.node).unwrap().overlay().is_none());
    h.focus();
    h.key(Key::ArrowDown);
    h.key(Key::Space);
    h.key(Key::Escape);
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Index(Some(2))),
            ControlMessage::OpenChanged(true),
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed,
        ]
    );
    assert_eq!(h.semantics().value.as_deref(), Some("Cherry"));
}

#[test]
fn a_hosted_combobox_closes_on_an_outside_press_and_takes_its_value() {
    let mut h = Harness::new(fruits(false));
    assert!(h.host.set_open(&mut h.tree, h.node, true));
    assert!(h.messages().is_empty());
    assert!(h.host.get(h.node).unwrap().overlay().is_some());
    h.click(Point::new(700.0, 500.0));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
    assert!(set_value(&mut h, ControlValue::Index(Some(2))));
    assert!(set_value(&mut h, ControlValue::Text("Apple".into())));
    assert!(!set_value(&mut h, ControlValue::Text("Durian".into())));
    assert!(!set_value(&mut h, ControlValue::Index(Some(9))));
    assert_eq!(h.semantics().value.as_deref(), Some("Apple"));
    assert!(h.messages().is_empty());
    assert!(h.perform(SemanticAction::SetValue, Some("banana")));
    assert!(h.perform(SemanticAction::ShowMenu, None));
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Index(Some(1))),
            ControlMessage::OpenChanged(true),
        ]
    );
    h.set_enabled(false);
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ],
        "disabling closes the open popup and reports it"
    );
    assert!(h.host.get(h.node).unwrap().overlay().is_none());
    h.click(h.bounds().center());
    assert!(!h.perform(SemanticAction::ShowMenu, None));
    assert!(h.messages().is_empty());
    assert!(h.semantics().state.disabled);
}

#[test]
fn a_hosted_editable_combobox_reports_typed_text_and_the_choice() {
    let mut h = Harness::new(fruits(true));
    h.focus();
    assert!(h.text("b"));
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Text("b".into())),
            ControlMessage::OpenChanged(true),
        ]
    );
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Text("Banana".into())),
            changed(ControlValue::Index(Some(1))),
            ControlMessage::OpenChanged(false),
        ]
    );
    h.key(Key::Backspace);
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Text("Banan".into())),
            changed(ControlValue::Index(None)),
            ControlMessage::OpenChanged(true),
        ],
        "typing clears the choice until a match is chosen"
    );
    assert!(set_value(&mut h, ControlValue::Text("Kiwi".into())));
    assert_eq!(h.semantics().value.as_deref(), Some("Kiwi"));
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_font_chooser_marks_a_missing_family() {
    let families = vec!["Helvetica".to_string(), "Menlo".to_string()];
    let mut h = Harness::new(FontChooser::new(families, "Menlo", "Helvetica"));
    assert_eq!(h.semantics().name, "Font");
    assert!(!h.semantics().state.invalid);
    assert!(set_value(&mut h, ControlValue::Text("Comic".into())));
    assert!(h.semantics().state.invalid);
    assert!(h.messages().is_empty());
    h.focus();
    h.text("o");
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Text("Comico".into())),
            ControlMessage::OpenChanged(true),
        ],
        "the popup opens on its no-results row"
    );
}

#[test]
fn a_hosted_text_editor_edits_and_takes_its_value() {
    let mut h = Harness::new(TextEditor::new("Notes", "Hi"));
    let semantics = h.semantics();
    assert_eq!(semantics.role, SemanticRole::TextField);
    assert!(semantics.text.is_some_and(|text| text.multiline));
    h.focus();
    h.key(Key::End);
    assert!(h.text("!"));
    h.key(Key::Enter);
    h.key(Key::Tab);
    assert!(h.perform(SemanticAction::SetValue, Some("Reset")));
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Text("Hi!".into())),
            changed(ControlValue::Text("Hi!\n".into())),
            changed(ControlValue::Text("Reset".into())),
        ]
    );
    assert!(set_value(&mut h, ControlValue::Text("New".into())));
    assert!(!set_value(&mut h, ControlValue::Number(1.0)));
    assert_eq!(h.semantics().value.as_deref(), Some("New"));
    assert!(texts(&h.paint()).contains(&"New".to_string()));
    let start = h.bounds().origin;
    h.click(Point::new(start.x + 12.0, start.y + 12.0));
    assert!(h.messages().is_empty());
    h.set_enabled(false);
    assert!(!h.text("x"));
    assert!(!h.perform(SemanticAction::SetValue, Some("x")));
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_scroll_bar_pages_drags_and_steps() {
    let mut bar = ScrollBar::new("Scroll", ScrollOrientation::Vertical);
    bar.set_range(1000.0, 100.0, 200.0);
    let mut h = Harness::new(bar);
    assert!(
        h.host.get(h.node).unwrap().focusable(),
        "a tab stop for its keys"
    );
    assert_eq!(h.semantics().role, SemanticRole::ScrollBar);
    let bounds = h.bounds();
    h.click(Point::new(bounds.center().x, bounds.origin.y + 150.0));
    // The thumb is now at 20..40 along the 200 pt track.
    let x = bounds.center().x;
    press(&mut h, Point::new(x, bounds.origin.y + 30.0));
    h.hover(Point::new(x + 300.0, bounds.origin.y + 48.0));
    release(&mut h, Point::new(x + 300.0, bounds.origin.y + 48.0));
    assert!(h.perform(SemanticAction::Increment, None));
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Number(100.0)),
            changed(ControlValue::Number(190.0)),
            changed(ControlValue::Number(290.0)),
        ]
    );
    assert!(set_value(&mut h, ControlValue::Number(500.0)));
    assert_eq!(h.semantics().value.as_deref(), Some("56%"));
    assert!(h.messages().is_empty());
    h.set_enabled(false);
    h.click(Point::new(x, bounds.origin.y + 150.0));
    assert!(!h.perform(SemanticAction::Increment, None));
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_decimal_spin_box_steps_types_and_commits() {
    let locale = Locale::parse("en-US").unwrap();
    let spin =
        DecimalSpinBox::new("Amount", locale, (0.0, 10.0), 0.5, 1, 2.0).expect("a valid range");
    let mut h = Harness::new(spin);
    assert_eq!(h.semantics().role, SemanticRole::SpinButton);
    h.focus();
    h.key(Key::ArrowUp);
    assert!(h.text("7"));
    assert_eq!(h.semantics().value.as_deref(), Some("7"));
    h.key(Key::Enter);
    let bounds = h.bounds();
    h.click(Point::new(bounds.max_x() - 4.0, bounds.max_y() - 4.0));
    assert!(h.perform(SemanticAction::Increment, None));
    assert!(h.perform(SemanticAction::SetValue, Some("3")));
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Number(2.5)),
            changed(ControlValue::Number(7.0)),
            changed(ControlValue::Number(6.5)),
            changed(ControlValue::Number(7.0)),
            changed(ControlValue::Number(3.0)),
        ]
    );
    assert!(set_value(&mut h, ControlValue::Number(4.0)));
    assert_eq!(h.semantics().value.as_deref(), Some("4.0"));
    assert!(h.messages().is_empty());
    h.set_enabled(false);
    h.key(Key::ArrowUp);
    assert!(!h.text("1"));
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_dial_points_steps_and_takes_its_value() {
    let range = IntegerRange::new(0, 100, 10, 1, 10).unwrap();
    let mut h = Harness::new(Dial::new(range, false).hosted("Volume"));
    let semantics = h.semantics();
    assert_eq!(semantics.role, SemanticRole::Slider);
    assert_eq!(semantics.name, "Volume");
    let bounds = h.bounds();
    // Straight up is the middle of a dial that does not wrap.
    h.click(Point::new(bounds.center().x, bounds.origin.y + 2.0));
    h.focus();
    h.key(Key::ArrowUp);
    assert!(h.perform(SemanticAction::SetValue, Some("70")));
    assert!(h.perform(SemanticAction::Decrement, None));
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Number(50.0)),
            changed(ControlValue::Number(51.0)),
            changed(ControlValue::Number(70.0)),
            changed(ControlValue::Number(69.0)),
        ]
    );
    assert!(set_value(&mut h, ControlValue::Number(20.0)));
    assert_eq!(h.semantics().value.as_deref(), Some("20"));
    assert!(h.messages().is_empty());
    h.set_enabled(false);
    h.key(Key::ArrowUp);
    h.click(Point::new(bounds.center().x, bounds.origin.y + 2.0));
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_tool_button_runs_toggles_and_opens_its_menu() {
    let mut bold = ToolButton::new("Bold", CommandId(7));
    bold.checkable = true;
    let mut h = Harness::new(bold.hosted());
    assert_eq!(h.semantics().role, SemanticRole::Checkbox);
    h.click(h.bounds().center());
    h.focus();
    h.key(Key::Space);
    assert!(h.perform(SemanticAction::Activate, None));
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Bool(true)),
            changed(ControlValue::Bool(false)),
            changed(ControlValue::Bool(true)),
        ]
    );
    assert!(set_value(&mut h, ControlValue::Bool(false)));
    assert_eq!(h.semantics().state.checked, Some(false));
    assert!(h.messages().is_empty());
    h.set_enabled(false);
    h.click(h.bounds().center());
    assert!(h.messages().is_empty());
    assert!(h.semantics().state.disabled);

    let mut paste = ToolButton::new("Paste", CommandId(8));
    paste.menu = ToolButtonMenu::Split;
    let items = vec![MenuItem::action("Paste as text", CommandId(9))];
    let mut h = Harness::new(paste.hosted().with_items(items));
    let bounds = h.bounds();
    let arrow = Point::new(bounds.max_x() - 4.0, bounds.center().y);
    h.click(Point::new(bounds.origin.x + 4.0, bounds.center().y));
    h.click(arrow);
    assert_eq!(h.semantics().state.expanded, Some(true));
    h.focus();
    // The open menu takes the keys: ArrowDown highlights its first row.
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(h.semantics().state.expanded, Some(false));
    h.focus();
    h.key(Key::ArrowDown);
    h.key(Key::Escape);
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Command(CommandId(8)),
            ControlMessage::OpenChanged(true),
            ControlMessage::Command(CommandId(9)),
            ControlMessage::OpenChanged(false),
            ControlMessage::OpenChanged(true),
            ControlMessage::Dismissed,
            ControlMessage::OpenChanged(false),
        ]
    );
    assert!(h.host.set_open(&mut h.tree, h.node, true));
    assert!(h.messages().is_empty());
    assert!(!set_value(&mut h, ControlValue::Bool(true)));
    assert!(h.host.set_open(&mut h.tree, h.node, false));
    // A disabled command never opens its menu.
    h.set_enabled(false);
    h.click(arrow);
    assert!(!h.host.set_open(&mut h.tree, h.node, true));
    assert!(h.messages().is_empty());
    assert_eq!(h.semantics().state.expanded, Some(false));
}

fn split_paste() -> Harness {
    let mut paste = ToolButton::new("Paste", CommandId(8));
    paste.menu = ToolButtonMenu::Split;
    let items = vec![MenuItem::action("Paste as text", CommandId(9))];
    Harness::new(paste.hosted().with_items(items))
}

#[test]
fn a_press_on_the_split_arrow_that_ends_on_the_label_does_nothing() {
    let mut h = split_paste();
    let bounds = h.bounds();
    let arrow = Point::new(bounds.max_x() - 4.0, bounds.center().y);
    let label = Point::new(bounds.origin.x + 4.0, bounds.center().y);
    let button = PointerButton::Primary;
    for (down, up) in [(arrow, label), (label, arrow)] {
        h.host.dispatch_pointer(
            &mut h.tree,
            PointerEvent::Down {
                position: down,
                button,
            },
        );
        h.host
            .dispatch_pointer(&mut h.tree, PointerEvent::Move { position: up });
        h.host.dispatch_pointer(
            &mut h.tree,
            PointerEvent::Up {
                position: up,
                button,
            },
        );
    }
    assert!(h.messages().is_empty());
    assert_eq!(h.semantics().state.expanded, Some(false));
}

#[test]
fn a_split_tool_button_menu_closes_on_an_outside_press_and_on_focus_loss() {
    let mut h = split_paste();
    let bounds = h.bounds();
    let arrow = Point::new(bounds.max_x() - 4.0, bounds.center().y);
    h.focus();
    h.click(arrow);
    assert_eq!(h.semantics().state.expanded, Some(true));
    h.click(Point::new(bounds.max_x() + 300.0, bounds.max_y() + 300.0));
    assert_eq!(h.semantics().state.expanded, Some(false), "outside press");
    h.focus();
    h.click(arrow);
    assert_eq!(h.semantics().state.expanded, Some(true));
    h.host.move_focus(&mut h.tree, None);
    assert_eq!(h.semantics().state.expanded, Some(false), "focus loss");
    let messages = h.messages();
    assert_eq!(
        messages
            .iter()
            .filter(|message| **message == ControlMessage::OpenChanged(false))
            .count(),
        2,
        "{messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, ControlMessage::Command(_))),
        "{messages:?}"
    );
}

#[test]
fn a_split_tool_button_paints_the_pressed_and_the_open_segment() {
    let mut h = split_paste();
    let bounds = h.bounds();
    let muted = DesignTokens::light().color(ColorRole::Muted);
    // Each segment fill has square corners on its inner side, so it is a
    // path.
    let segments = |h: &Harness| -> Vec<Rect> {
        h.paint()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DisplayCommand::FillPath {
                    path,
                    paint: Paint::Solid(color),
                    ..
                } if *color == muted => Some(path.bounds()),
                _ => None,
            })
            .collect()
    };
    assert!(segments(&h).is_empty(), "no fill at rest");
    let label = Point::new(bounds.origin.x + 4.0, bounds.center().y);
    h.hover(label);
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Down {
            position: label,
            button: PointerButton::Primary,
        },
    );
    let pressed = segments(&h);
    assert_eq!(pressed.len(), 1, "the pressed label");
    assert!(pressed[0].contains(label));
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Up {
            position: label,
            button: PointerButton::Primary,
        },
    );
    // Move away so that no hover fill shows.
    h.hover(Point::new(bounds.max_x() + 300.0, bounds.max_y() + 300.0));
    h.messages();
    let arrow = Point::new(bounds.max_x() - 4.0, bounds.center().y);
    h.click(arrow);
    h.hover(Point::new(bounds.max_x() + 300.0, bounds.max_y() + 300.0));
    let open = segments(&h);
    assert!(
        open.iter()
            .any(|segment| segment.contains(arrow) && !segment.contains(label)),
        "the open arrow segment: {open:?}"
    );
}

#[test]
fn a_hosted_split_tool_button_mirrors_its_arrow() {
    let mut paste = ToolButton::new("Paste", CommandId(8));
    paste.menu = ToolButtonMenu::Split;
    let mut h = Harness::with_direction(paste.hosted(), LayoutDirection::Rtl);
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 4.0, bounds.center().y));
    h.click(Point::new(bounds.max_x() - 4.0, bounds.center().y));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::OpenChanged(true),
            ControlMessage::Command(CommandId(8)),
        ]
    );
}

fn alignment(exclusive: bool) -> ButtonGroup {
    ButtonGroup::new(vec![CommandId(1), CommandId(2), CommandId(3)], exclusive)
}

/// The node of the focused member.
fn member_node(h: &Harness) -> choscorui_core::SemanticNode {
    h.focused_node().expect("a focused member")
}

fn labels() -> Vec<String> {
    ["Left", "Center", "Right"].map(String::from).to_vec()
}

/// The center of member `index` of a row of three.
fn member(h: &Harness, index: usize) -> Point {
    let bounds = h.bounds();
    let width = bounds.size.width / 3.0;
    Point::new(
        bounds.origin.x + width * (index as f64 + 0.5),
        bounds.center().y,
    )
}

#[test]
fn a_hosted_exclusive_button_group_moves_its_check() {
    let mut h = Harness::new(alignment(true).hosted("Alignment", labels()));
    let center = member(&h, 1);
    h.click(center);
    assert_eq!(member_node(&h).name, "Center");
    assert_eq!(member_node(&h).state.checked, Some(true));
    h.focus();
    h.key(Key::ArrowRight);
    assert!(h.perform(SemanticAction::Activate, None));
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Indices(vec![1])),
            changed(ControlValue::Indices(vec![2])),
        ]
    );
    assert!(set_value(&mut h, ControlValue::Indices(vec![0])));
    assert!(!set_value(&mut h, ControlValue::Indices(vec![0, 1])));
    assert_eq!(member_node(&h).state.checked, Some(false));
    assert!(h.messages().is_empty());
    h.set_enabled(false);
    h.click(center);
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_button_group_mirrors_and_toggles_members() {
    let mut h = Harness::with_direction(
        alignment(false).hosted("Alignment", labels()),
        LayoutDirection::Rtl,
    );
    // The first member sits at the right of a right-to-left row.
    let right = member(&h, 2);
    h.click(right);
    assert_eq!(member_node(&h).name, "Left");
    h.focus();
    h.key(Key::ArrowLeft);
    h.key(Key::Space);
    assert_eq!(member_node(&h).name, "Center");
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Indices(vec![0])),
            changed(ControlValue::Indices(vec![0, 1])),
        ]
    );
}

#[test]
fn a_hosted_button_group_skips_a_disabled_member() {
    let mut h = Harness::new(
        alignment(true)
            .hosted("Alignment", labels())
            .with_enabled(1, false),
    );
    assert!(set_value(&mut h, ControlValue::Indices(vec![0])));
    // A press on the disabled member focuses the group at its checked
    // member and does not reach the disabled one.
    h.click(member(&h, 1));
    assert_eq!(member_node(&h).name, "Left");
    assert!(!member_node(&h).state.disabled);
    assert!(h.messages().is_empty());
    h.focus();
    h.key(Key::ArrowLeft);
    h.key(Key::ArrowRight);
    assert_eq!(member_node(&h).name, "Left");
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Indices(vec![2])),
            changed(ControlValue::Indices(vec![0])),
        ]
    );
}

#[test]
fn a_hosted_exclusive_button_group_starts_at_the_check_and_reaches_buttons() {
    use ButtonGroupMember::{Button, Toggle};
    let group = ButtonGroup::from_members(
        vec![
            Toggle(CommandId(1)),
            Toggle(CommandId(2)),
            Button(CommandId(3), ButtonVariant::Outline),
        ],
        true,
    );
    let mut h = Harness::new(group.hosted("Alignment", labels()));
    assert!(set_value(&mut h, ControlValue::Indices(vec![1])));
    h.focus();
    let node = member_node(&h);
    assert_eq!(
        (node.name.as_str(), node.role),
        ("Center", SemanticRole::RadioButton)
    );
    h.key(Key::ArrowRight);
    assert_eq!(
        member_node(&h).name,
        "Right",
        "arrow keys reach a button member"
    );
    assert!(h.messages().is_empty(), "a button member takes no check");
    h.key(Key::Home);
    assert_eq!(member_node(&h).name, "Left");
    h.key(Key::End);
    assert_eq!(member_node(&h).name, "Right");
    assert_eq!(h.messages(), vec![changed(ControlValue::Indices(vec![0]))]);
}

#[test]
fn a_button_group_size_sets_the_member_height() {
    let small = Harness::new(
        alignment(false)
            .with_size(ControlSize::Sm)
            .hosted("Alignment", labels()),
    );
    let large = Harness::new(
        alignment(false)
            .with_size(ControlSize::Lg)
            .hosted("Alignment", labels()),
    );
    assert_eq!(small.bounds().size.height, 32.0);
    assert_eq!(large.bounds().size.height, 40.0);
}

/// A zoom group: a text part, two outline buttons with a separator
/// between them, and a toggle.
fn zoom() -> ButtonGroup {
    use ButtonGroupMember::{Button, Separator, Text, Toggle};
    ButtonGroup::from_members(
        vec![
            Text,
            Button(CommandId(1), ButtonVariant::Outline),
            Separator,
            Button(CommandId(2), ButtonVariant::Outline),
            Toggle(CommandId(3)),
        ],
        false,
    )
}

#[test]
fn a_hosted_button_group_runs_buttons_and_skips_parts() {
    let labels = ["Zoom", "Out", "", "In", "Fit"].map(String::from).to_vec();
    let mut h = Harness::new(zoom().hosted("Zoom", labels));
    h.focus();
    let semantics = member_node(&h);
    assert_eq!(semantics.name, "Out", "focus starts at the first button");
    assert_eq!(semantics.role, SemanticRole::Button);
    assert_eq!(semantics.state.checked, None);
    assert_eq!(h.semantics().name, "Zoom", "the group name");
    assert_eq!(h.parts().parts()[0].node.name, "Zoom", "the text part");
    // Arrow keys and Tab skip the separator and the text part.
    h.key(Key::ArrowRight);
    assert_eq!(member_node(&h).name, "In");
    h.key(Key::Enter);
    h.key(Key::Tab);
    assert_eq!(member_node(&h).name, "Fit");
    assert_eq!(member_node(&h).role, SemanticRole::Button);
    assert_eq!(member_node(&h).state.checked, Some(false));
    h.key(Key::Space);
    h.key(Key::ArrowRight);
    assert_eq!(member_node(&h).name, "Out", "arrow keys wrap");
    assert!(h.perform(SemanticAction::Activate, None));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::ItemActivated(3),
            changed(ControlValue::Indices(vec![4])),
            ControlMessage::ItemActivated(1),
        ]
    );
    // A click on the text part does nothing.
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 2.0, bounds.center().y));
    assert!(h.messages().is_empty());
    assert!(!set_value(&mut h, ControlValue::Indices(vec![1])));
    assert!(set_value(&mut h, ControlValue::Indices(vec![4])));
    let list = h.paint();
    assert_eq!(texts(&list), vec!["Zoom", "Out", "In", "Fit"]);
    let tokens = choscorui_core::DesignTokens::light();
    assert!(fills(&list, tokens.color(choscorui_core::ColorRole::Input)));
    assert!(fills(&list, tokens.color(choscorui_core::ColorRole::Muted)));
}

/// The baseline origin of each text run of `h`, by text.
fn baselines(h: &Harness) -> Vec<(String, Point)> {
    h.paint()
        .commands()
        .iter()
        .filter_map(|command| match command {
            choscorui_core::DisplayCommand::DrawText { text, baseline, .. } => {
                Some((text.clone(), *baseline))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_hosted_button_group_keeps_parts_at_their_natural_width() {
    let labels = ["Zoom", "Out", "", "In", "Fit"].map(String::from).to_vec();
    let mut h = Harness::new(zoom().hosted("Zoom", labels).with_enabled(1, false));
    let before = baselines(&h);
    let natural = h.bounds();
    h.place(hosted::rect(10.0, 10.0, natural.size.width + 90.0, 36.0));
    let after = baselines(&h);
    // The text part keeps its place, and the three command members share
    // the extra 90 pt: 30 pt each, so the centered label of the last
    // member moves 75 pt.
    assert_eq!(before[0], after[0]);
    assert!(after[1].1.x > before[1].1.x);
    assert!((after[3].1.x - before[3].1.x - 75.0).abs() < 0.5);
    // The disabled button takes no click and no focus.
    h.click(after[1].1);
    assert!(h.messages().is_empty());
    assert_eq!(member_node(&h).name, "In");
    assert!(!member_node(&h).state.disabled);
}

#[test]
fn a_hosted_command_link_activates_like_a_button() {
    let link = CommandLink::new("Install", "Add the app to the dock");
    let mut h = Harness::new(link.hosted(CommandId(3)));
    let semantics = h.semantics();
    assert_eq!(semantics.role, SemanticRole::Button);
    assert_eq!(semantics.name, "Install");
    assert_eq!(semantics.description, "Add the app to the dock");
    h.click(h.bounds().center());
    h.focus();
    h.key(Key::Enter);
    assert!(h.perform(SemanticAction::Activate, None));
    assert_eq!(h.messages(), vec![ControlMessage::Activated; 3]);
    assert_eq!(
        texts(&h.paint()),
        vec!["Install", "Add the app to the dock"]
    );
    // The pressed look comes from the button: Space held down fills the
    // row. Each paint is after the 100 ms fill motion.
    let mut time = 0.0;
    let mut settled = |h: &mut Harness| {
        // The first paint starts the motion, and the second ends it.
        for _ in 0..2 {
            time += 1.0;
            h.set_time(time);
            h.paint();
        }
        format!("{:?}", h.paint())
    };
    let link = CommandLink::new("Install", "Add the app to the dock");
    let mut p = Harness::new(link.hosted(CommandId(3)));
    p.focus();
    let idle = settled(&mut p);
    let space = |h: &mut Harness, event| h.host.dispatch_key(&mut h.tree, event);
    let down = KeyEvent::Down {
        key: Key::Space,
        repeat: false,
    };
    space(&mut p, down);
    assert_ne!(settled(&mut p), idle);
    space(&mut p, KeyEvent::Up { key: Key::Space });
    assert_eq!(settled(&mut p), idle);
    assert_eq!(p.messages(), vec![ControlMessage::Activated]);
    h.set_enabled(false);
    h.click(h.bounds().center());
    assert!(!h.perform(SemanticAction::Activate, None));
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_tool_box_opens_pages() {
    let titles = ["General", "Advanced", "About"].map(String::from).to_vec();
    let mut h = Harness::new(ToolBox::new(titles));
    let bounds = h.bounds();
    let header = bounds.size.height / 3.0;
    h.click(Point::new(
        bounds.center().x,
        bounds.origin.y + header * 1.5,
    ));
    h.focus();
    h.key(Key::ArrowDown);
    assert_eq!(h.semantics().name, "About");
    assert_eq!(h.semantics().state.expanded, Some(false));
    h.key(Key::Enter);
    assert_eq!(h.semantics().role, SemanticRole::Button);
    assert_eq!(h.semantics().state.expanded, Some(true));
    h.key(Key::ArrowUp);
    assert!(h.perform(SemanticAction::Expand, None));
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Index(Some(1))),
            changed(ControlValue::Index(Some(2))),
            changed(ControlValue::Index(Some(1))),
        ]
    );
    assert!(set_value(&mut h, ControlValue::Index(Some(0))));
    assert!(!set_value(&mut h, ControlValue::Index(Some(3))));
    assert_eq!(h.semantics().name, "General");
    assert_eq!(h.semantics().state.expanded, Some(true));
    assert!(h.messages().is_empty());
    h.set_enabled(false);
    h.key(Key::ArrowDown);
    assert!(!h.perform(SemanticAction::Activate, None));
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_numeric_display_shows_a_value_and_takes_no_input() {
    let mut h = Harness::new(NumericDisplay::new(4, NumberBase::Decimal).hosted("Count"));
    assert!(!h.host.get(h.node).unwrap().focusable());
    assert_eq!(h.semantics().role, SemanticRole::StaticText);
    assert_eq!(h.semantics().name, "Count");
    assert!(set_value(&mut h, ControlValue::Number(42.0)));
    assert!(!set_value(&mut h, ControlValue::Number(1.5)));
    assert_eq!(h.semantics().value.as_deref(), Some("42"));
    h.click(h.bounds().center());
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_key_sequence_edit_records_a_sequence() {
    let edit = KeySequenceEdit::new(2)
        .with_name("Shortcut")
        .with_placeholder("Record");
    let mut h = Harness::new(edit);
    assert_eq!(texts(&h.paint()), vec!["Record"]);
    assert_eq!(h.semantics().name, "Shortcut");
    h.focus();
    h.key(Key::Enter);
    assert!(!h.semantics().description.is_empty());
    let command = ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    };
    h.modifiers(command);
    h.key(Key::Character('k'));
    h.modifiers(ShortcutModifiers {
        shift: true,
        ..command
    });
    h.key(Key::Character('s'));
    h.modifiers(ShortcutModifiers::default());
    assert!(h.perform(SemanticAction::Activate, None));
    h.key(Key::F(5));
    h.host.move_focus(&mut h.tree, None);
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Text("⌘K, ⇧⌘S".into())),
            changed(ControlValue::Text("F5".into())),
        ]
    );
    assert!(set_value(&mut h, ControlValue::Text("⌃↩, ⌘,".into())));
    assert!(!set_value(&mut h, ControlValue::Text("bogus".into())));
    assert_eq!(h.semantics().value.as_deref(), Some("⌃↩, ⌘,"));
    assert!(h.messages().is_empty());
    h.set_enabled(false);
    h.focus();
    h.key(Key::Enter);
    assert!(h.semantics().description.is_empty());
    assert!(!h.perform(SemanticAction::Activate, None));
}

/// Counts the beeps of a host.
#[derive(Default)]
struct Beeps(Cell<usize>);

impl Feedback for Beeps {
    fn beep(&self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn a_recording_needs_a_modifier_first_and_commits_on_return_or_a_pause() {
    let mut h = Harness::new(KeySequenceEdit::new(3).with_clear_button(true));
    let beeps = Rc::new(Beeps::default());
    h.host.set_feedback(beeps.clone());
    h.focus();
    h.key(Key::Enter);
    // A prompt shows until the first chord.
    assert_eq!(texts(&h.paint()), vec!["Type a shortcut"]);
    // A plain key, a shifted key, and Return cannot start a chord.
    h.key(Key::Character('a'));
    h.chord(
        ShortcutModifiers {
            shift: true,
            ..ShortcutModifiers::default()
        },
        Key::Character('b'),
    );
    h.key(Key::Enter);
    assert_eq!(beeps.0.get(), 2);
    let option = ShortcutModifiers {
        option: true,
        ..ShortcutModifiers::default()
    };
    h.chord(option, Key::Character('a'));
    // Return commits a shorter sequence.
    h.key(Key::Enter);
    assert_eq!(h.messages(), vec![changed(ControlValue::Text("⌥A".into()))]);
    // A pause commits too, and a second chord needs no modifier.
    h.key(Key::Space);
    h.chord(option, Key::Character('k'));
    h.key(Key::Character('s'));
    assert!(h.host.next_deadline(&h.tree).is_some());
    h.set_time(5.0);
    assert!(h.host.advance(&mut h.tree, 5.0));
    assert_eq!(
        h.messages(),
        vec![changed(ControlValue::Text("⌥K, S".into()))]
    );
    // Tab ends a recording and moves on; it is not a chord.
    h.key(Key::Space);
    h.key(Key::Tab);
    assert!(h.messages().is_empty());
    assert_eq!(h.semantics().value.as_deref(), Some("⌥K, S"));
    // The clear button clears the sequence.
    let bounds = h.bounds();
    h.click(Point::new(bounds.max_x() - 4.0, bounds.center().y));
    assert_eq!(
        h.messages(),
        vec![changed(ControlValue::Text(String::new()))]
    );
}

#[test]
fn a_hosted_size_grip_reports_the_new_size() {
    let grip = SizeGrip::new(Size::new(200.0, 100.0), Size::new(1000.0, 650.0), false);
    let mut h = Harness::new(grip);
    assert!(!h.host.get(h.node).unwrap().focusable());
    assert!(h.semantics().state.hidden);
    let start = h.bounds().center();
    press(&mut h, start);
    h.hover(Point::new(start.x + 50.0, start.y + 80.0));
    release(&mut h, Point::new(start.x + 50.0, start.y + 80.0));
    assert_eq!(
        h.messages(),
        vec![changed(ControlValue::Range(850.0, 650.0))]
    );
    h.set_enabled(false);
    press(&mut h, start);
    h.hover(Point::new(start.x + 10.0, start.y));
    assert!(h.messages().is_empty());
}

#[test]
fn a_size_grip_resizes_a_given_size_and_escape_cancels_the_drag() {
    let grip = SizeGrip::new(Size::new(200.0, 100.0), Size::new(1000.0, 650.0), false);
    let mut h = Harness::new(grip);
    let pane = ControlValue::Range(400.0, 300.0);
    assert!(h.host.set_value(&mut h.tree, h.node, &pane));
    let start = h.bounds().center();
    press(&mut h, start);
    h.hover(Point::new(start.x + 50.0, start.y + 80.0));
    assert_eq!(
        h.messages(),
        vec![changed(ControlValue::Range(450.0, 380.0))]
    );
    // Escape reaches the grip without focus, and puts the size back.
    h.key(Key::Escape);
    assert_eq!(h.messages(), vec![changed(pane)]);
    h.hover(Point::new(start.x + 90.0, start.y));
    assert!(h.messages().is_empty(), "the drag ended");
}

#[test]
fn a_hosted_size_grip_grows_left_in_right_to_left_layouts() {
    let grip = SizeGrip::new(Size::new(200.0, 100.0), Size::new(1000.0, 800.0), false);
    let mut h = Harness::with_direction(grip, LayoutDirection::Rtl);
    let start = h.bounds().center();
    press(&mut h, start);
    h.hover(Point::new(start.x - 40.0, start.y));
    assert_eq!(
        h.messages(),
        vec![changed(ControlValue::Range(840.0, 600.0))]
    );
}

/// A combobox with `count` items named "Item 1" and up.
fn many(count: u64) -> Combobox {
    let mut combo = Combobox::new("Many", false);
    let items = (1..=count)
        .map(|index| ComboItem {
            key: ItemKey(index),
            text: format!("Item {index}"),
        })
        .collect();
    combo.set_items(items);
    combo
}

#[test]
fn a_long_combobox_list_scrolls_past_eight_rows_and_keeps_the_highlight_visible() {
    let mut h = Harness::new(many(20));
    h.focus();
    h.key(Key::Space);
    let popup = h.host.get(h.node).unwrap().overlay().unwrap();
    assert_eq!(popup.size.height, 8.0 * 32.0 + 8.0, "eight rows show");
    h.set_time(1.0);
    let rows = |h: &Harness| texts(&h.paint());
    assert_eq!(rows(&h).first().map(String::as_str), Some("Item 1"));
    for _ in 0..10 {
        h.key(Key::ArrowDown);
    }
    let shown = rows(&h);
    assert_eq!(shown.len(), 8);
    assert_eq!(shown.last().map(String::as_str), Some("Item 11"));
    assert_eq!(
        h.host.get(h.node).unwrap().semantics().value.as_deref(),
        Some(""),
        "highlighting does not choose"
    );
    // A click on the second shown row chooses the item in that row.
    h.click(suggestion(&h, 1.0));
    assert_eq!(h.semantics().value.as_deref(), Some("Item 5"));
}

#[test]
fn the_wheel_scrolls_a_long_combobox_list() {
    let mut h = Harness::new(many(20));
    assert!(h.host.set_open(&mut h.tree, h.node, true));
    h.set_time(1.0);
    let popup = h.host.get(h.node).unwrap().overlay().unwrap();
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Wheel {
            position: popup.center(),
            delta: Point::new(0.0, 3.0),
            unit: choscorui_core::WheelUnit::Lines,
        },
    );
    assert_eq!(texts(&h.paint())[0], "Item 4");
}

#[test]
fn near_the_bottom_edge_the_combobox_list_opens_above() {
    let mut h = Harness::new(fruits(false));
    let size = h.bounds().size;
    h.place(hosted::rect(10.0, 550.0, size.width, size.height));
    assert!(h.host.set_open(&mut h.tree, h.node, true));
    let popup = h.host.get(h.node).unwrap().overlay().unwrap();
    assert_eq!(popup.max_y(), 546.0, "4 pt above the trigger");
}

/// The start of the painted text and the 1 pt underlines of a paint.
fn underlines(h: &Harness) -> (f64, Vec<Rect>) {
    let list = h.paint();
    let mut start = 0.0;
    let mut marks = Vec::new();
    for command in list.commands() {
        match command {
            DisplayCommand::DrawText { baseline, .. } => start = baseline.x,
            DisplayCommand::FillRect { rect, .. } if rect.size.height == 1.0 => marks.push(*rect),
            _ => {}
        }
    }
    (start, marks)
}

#[test]
fn a_mnemonic_button_hides_the_marker_and_underlines_the_mnemonic() {
    let mnemonic = |label| Harness::new(Button::new(label, CommandId(1)).with_mnemonic());
    let h = mnemonic("Save &As");
    assert_eq!(texts(&h.paint()), ["Save As"]);
    assert_eq!(h.semantics().name, "Save As");
    let (start, marks) = underlines(&h);
    assert_eq!(marks.len(), 1, "one underline below the A");
    assert!(marks[0].origin.x > start + 10.0, "after `Save `");
    assert!(marks[0].size.width > 0.0 && marks[0].max_x() < h.bounds().max_x());
    let (start, marks) = underlines(&mnemonic("&Save"));
    assert_eq!(marks.first().map(|mark| mark.origin.x), Some(start));
    // A single `&` before a character that is not a letter stays.
    let h = mnemonic("R&&D &-");
    assert_eq!(texts(&h.paint()), ["R&D &-"]);
    assert!(underlines(&h).1.is_empty());
}
