//! Switch, Toggle, ToggleGroup, and Kbd through the control host.

mod hosted;

use choscorui_controls::{
    ControlMessage, ControlSize, ControlValue, Kbd, Switch, SwitchSize, Toggle, ToggleGroup,
    ToggleItem, ToggleVariant,
};
use choscorui_core::{
    ColorRole, DesignTokens, Icon, Key, LayoutDirection, Point, SemanticAction, SemanticRole, Size,
};
use hosted::{Harness, fills, rounded_fills, texts};

#[test]
fn switch_flips_on_click_keys_and_actions_and_slides_its_thumb() {
    let mut h = Harness::new(Switch::new("Airplane mode", false));
    let size = h.measure();
    assert_eq!(size.height, 18.4, "the default track height");
    assert!(size.width > 32.0, "the track plus the label");
    let tokens = DesignTokens::light();
    let input = tokens.color(ColorRole::Input);
    assert!(fills(&h.paint(), input), "an off track fills input");

    let bounds = h.bounds();
    h.click(Point::new(bounds.max_x() - 4.0, bounds.center().y));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Bool(true))],
        "a click on the label flips it"
    );
    let primary = tokens.color(ColorRole::Primary);
    // The track color and the thumb ease over the transition duration.
    h.paint();
    h.set_time(1.0);
    let list = h.paint();
    assert!(fills(&list, primary), "an on track fills primary");
    // The thumb (16 pt, background) sits at the end of the 32 pt track.
    let thumb = rounded_fills(&list, tokens.color(ColorRole::Background));
    assert_eq!(thumb.last().unwrap().max_x(), bounds.origin.x + 32.0 - 1.2);

    h.focus();
    h.key(Key::Space);
    assert_eq!(h.semantics().state.checked, Some(false));
    assert!(h.perform(SemanticAction::Activate, None));
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::Switch);
    assert_eq!(node.description, "", "the role names the switch");
    assert_eq!(node.state.checked, Some(true));
    assert_eq!(texts(&list), ["Airplane mode"]);

    h.messages();
    h.set_enabled(false);
    h.click(Point::new(bounds.origin.x + 4.0, bounds.center().y));
    assert!(h.messages().is_empty(), "a disabled switch ignores clicks");
    assert!(h.semantics().state.disabled);
}

#[test]
fn small_switch_and_rtl_put_the_track_at_the_start() {
    let mut h = Harness::with_direction(
        Switch::new("", true).with_size(SwitchSize::Sm),
        LayoutDirection::Rtl,
    );
    assert_eq!(h.measure(), Size::new(24.0, 14.0));
    let bounds = h.bounds();
    let tokens = DesignTokens::light();
    let list = h.paint();
    // Checked in RTL: the 12 pt thumb sits at the left end.
    let thumb = rounded_fills(&list, tokens.color(ColorRole::Background));
    assert_eq!(thumb.last().unwrap().origin.x, bounds.origin.x + 1.0);
}

#[test]
fn toggle_reports_on_state_and_uses_the_outline_variant() {
    let mut h = Harness::new(
        Toggle::new("Bold", false)
            .with_variant(ToggleVariant::Outline)
            .with_size(ControlSize::Sm),
    );
    assert_eq!(h.measure().height, 32.0);
    let center = h.bounds().center();
    h.click(center);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Bool(true))]
    );
    let muted = DesignTokens::light().color(ColorRole::Muted);
    assert!(fills(&h.paint(), muted), "on fills muted");
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::Button);
    assert_eq!(node.state.checked, Some(true));
    h.focus();
    h.key(Key::Enter);
    assert_eq!(h.semantics().state.checked, Some(false));
}

fn alignment() -> ToggleGroup {
    ToggleGroup::new(
        "Alignment",
        vec![
            ToggleItem::new("Left"),
            ToggleItem::new("Center").disabled(),
            ToggleItem::new("Right"),
        ],
        false,
    )
    .with_variant(ToggleVariant::Outline)
}

#[test]
fn single_toggle_group_keeps_one_item_and_roves_over_enabled_items() {
    let mut h = Harness::new(alignment());
    let bounds = h.bounds();
    // Items share the bar: click the first and the last.
    h.click(Point::new(bounds.origin.x + 4.0, bounds.center().y));
    h.click(Point::new(bounds.max_x() - 4.0, bounds.center().y));
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Changed(ControlValue::Indices(vec![0])),
            ControlMessage::Changed(ControlValue::Indices(vec![2])),
        ]
    );
    assert_eq!(h.semantics().value.as_deref(), Some("Right"));
    assert_eq!(h.semantics().role, SemanticRole::RadioGroup);

    // Focus roves from Right past the disabled Center to Left, and wraps.
    h.focus();
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().description, "Left");
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().description, "Right", "Center is skipped");
    h.key(Key::Space);
    assert_eq!(
        h.semantics().value.as_deref(),
        Some(""),
        "a second flip clears"
    );
    assert!(h.perform(SemanticAction::SetValue, Some("Left")));
    assert_eq!(h.semantics().value.as_deref(), Some("Left"));
    assert!(!h.perform(SemanticAction::SetValue, Some("Nowhere")));
}

#[test]
fn multiple_toggle_group_flips_items_independently_and_mirrors() {
    let group = ToggleGroup::new(
        "Style",
        vec![
            ToggleItem::new("B"),
            ToggleItem::new("I"),
            ToggleItem::new("U"),
        ],
        true,
    )
    .with_spacing(4.0);
    let mut h = Harness::with_direction(group, LayoutDirection::Rtl);
    let bounds = h.bounds();
    // In RTL the first item is at the right edge.
    h.click(Point::new(bounds.max_x() - 4.0, bounds.center().y));
    h.click(Point::new(bounds.origin.x + 4.0, bounds.center().y));
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Changed(ControlValue::Indices(vec![0])),
            ControlMessage::Changed(ControlValue::Indices(vec![0, 2])),
        ]
    );
    assert_eq!(h.semantics().role, SemanticRole::Group);
    h.focus();
    // Focus follows the last flip (U); ArrowLeft moves forward in RTL and
    // wraps.
    h.key(Key::ArrowLeft);
    assert_eq!(h.semantics().description, "B");
    h.key(Key::ArrowLeft);
    assert_eq!(h.semantics().description, "I");
    h.key(Key::End);
    assert_eq!(h.semantics().description, "U");
}

#[test]
fn kbd_paints_one_cap_per_key_and_takes_no_focus() {
    let mut h = Harness::new(Kbd::group(["⌘".to_string(), "K".to_string()]));
    let size = h.measure();
    assert_eq!(size.height, 20.0, "the 20 pt cap height");
    assert_eq!(texts(&h.paint()), ["⌘", "K"]);
    let muted = DesignTokens::light().color(ColorRole::Muted);
    assert_eq!(rounded_fills(&h.paint(), muted).len(), 2);
    assert_eq!(h.host.focus_next(&mut h.tree, false), None);
    assert_eq!(h.semantics().name, "⌘ K");
}

#[test]
fn disabled_switch_and_toggle_refuse_the_focus_action() {
    let mut switch = Harness::new(Switch::new("Wi-Fi", false));
    switch.set_enabled(false);
    assert!(!switch.perform(SemanticAction::Focus, None));
    let mut toggle = Harness::new(Toggle::new("Bold", false));
    toggle.set_enabled(false);
    assert!(!toggle.perform(SemanticAction::Focus, None));
}

#[test]
fn an_icon_only_toggle_paints_no_label_and_keeps_its_name() {
    let mut h = Harness::new(Toggle::new("Bold", true).with_icon(Icon::Bold).icon_only());
    assert!(texts(&h.paint()).is_empty());
    let node = h.semantics();
    assert_eq!(
        (node.role, node.name.as_str()),
        (SemanticRole::Button, "Bold")
    );
    assert_eq!(node.state.checked, Some(true));
    h.messages();
}

#[test]
fn toggle_group_focus_enters_at_the_selected_or_first_enabled_item() {
    let mut items = alignment();
    items.set_selected(&[2]);
    let mut h = Harness::new(items);
    h.focus();
    assert_eq!(h.semantics().description, "Right");
    let mut first = ToggleGroup::new(
        "Alignment",
        vec![ToggleItem::new("Left").disabled(), ToggleItem::new("Right")],
        false,
    );
    first.set_selected(&[]);
    let mut h = Harness::new(first);
    h.focus();
    assert_eq!(h.semantics().description, "Right");
}
