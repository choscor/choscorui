//! The combobox, font chooser, text editor, and basic widgets through the
//! control host: input, assistive actions, values from the application,
//! the disabled state, semantics, and the combobox popup.

mod hosted;

use choscorui_controls::{
    ButtonGroup, ButtonGroupMember, ButtonVariant, ComboItem, Combobox, CommandId, CommandLink,
    ControlMessage, ControlValue, DecimalSpinBox, Dial, FontChooser, IntegerRange, KeySequenceEdit,
    NumberBase, NumericDisplay, ScrollBar, ScrollOrientation, ShortcutModifiers, SizeGrip,
    TextEditor, ToolBox, ToolButton, ToolButtonMenu,
};
use choscorui_core::{
    ItemKey, Key, LayoutDirection, Locale, Point, PointerButton, PointerEvent, SemanticAction,
    SemanticRole, Size,
};
use hosted::{Harness, fills, texts};

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
            ControlMessage::OpenChanged(true),
        ]
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
        vec![changed(ControlValue::Text("Comico".into()))]
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
    assert!(!h.host.get(h.node).unwrap().focusable());
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
    let mut h = Harness::new(paste.hosted());
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 4.0, bounds.center().y));
    h.click(Point::new(bounds.max_x() - 4.0, bounds.center().y));
    h.focus();
    h.key(Key::ArrowDown);
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Command(CommandId(8)),
            ControlMessage::OpenChanged(true),
            ControlMessage::OpenChanged(false),
        ]
    );
    assert!(h.host.set_open(&mut h.tree, h.node, true));
    assert!(h.messages().is_empty());
    assert!(!set_value(&mut h, ControlValue::Bool(true)));
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
    let mut h = Harness::new(alignment(true).hosted(labels()));
    let center = member(&h, 1);
    h.click(center);
    assert_eq!(h.semantics().name, "Center");
    assert_eq!(h.semantics().state.checked, Some(true));
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
    assert_eq!(h.semantics().state.checked, Some(false));
    assert!(h.messages().is_empty());
    h.set_enabled(false);
    h.click(center);
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_button_group_mirrors_and_toggles_members() {
    let mut h = Harness::with_direction(alignment(false).hosted(labels()), LayoutDirection::Rtl);
    // The first member sits at the right of a right-to-left row.
    let right = member(&h, 2);
    h.click(right);
    assert_eq!(h.semantics().name, "Left");
    h.focus();
    h.key(Key::ArrowLeft);
    h.key(Key::Space);
    assert_eq!(h.semantics().name, "Center");
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
    let mut h = Harness::new(alignment(true).hosted(labels()).with_enabled(1, false));
    assert!(set_value(&mut h, ControlValue::Indices(vec![0])));
    h.click(member(&h, 1));
    assert_eq!(h.semantics().name, "Center");
    assert!(h.semantics().state.disabled);
    assert!(h.messages().is_empty());
    h.focus();
    h.key(Key::ArrowLeft);
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().name, "Left");
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Indices(vec![2])),
            changed(ControlValue::Indices(vec![0])),
        ]
    );
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
    let mut h = Harness::new(zoom().hosted(labels));
    h.focus();
    let semantics = h.semantics();
    assert_eq!(semantics.name, "Out", "focus starts at the first button");
    assert_eq!(semantics.role, SemanticRole::Button);
    assert_eq!(semantics.state.checked, None);
    assert_eq!(semantics.description, "Zoom");
    // Arrow keys and Tab skip the separator and the text part.
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().name, "In");
    h.key(Key::Enter);
    h.key(Key::Tab);
    assert_eq!(h.semantics().name, "Fit");
    assert_eq!(h.semantics().role, SemanticRole::Checkbox);
    h.key(Key::Space);
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().name, "Out", "arrow keys wrap");
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
    let mut h = Harness::new(zoom().hosted(labels).with_enabled(1, false));
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
    // The disabled button takes no click.
    h.click(after[1].1);
    assert!(h.messages().is_empty());
    assert_eq!(h.semantics().name, "Out");
    assert!(h.semantics().state.disabled);
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
    h.key(Key::Character('a'));
    h.host.move_focus(&mut h.tree, None);
    assert_eq!(
        h.messages(),
        vec![
            changed(ControlValue::Text("⌘K, ⇧⌘S".into())),
            changed(ControlValue::Text("A".into())),
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
