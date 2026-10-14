//! Horizontal arrow keys in right-to-left layouts follow the visual
//! direction, through the control host.

mod hosted;

use choscorui_controls::{
    Calendar, CommandId, ControlMessage, ControlValue, CustomControl, DropdownMenu, HeaderColumn,
    HeaderView, IntegerRange, MenuItem, Menubar, Slider, SliderOrientation, Tabs,
};
use choscorui_core::{CivilDate, Key, LayoutDirection, Locale, Point};
use hosted::{Harness, texts};

fn rtl(control: impl CustomControl + 'static) -> Harness {
    Harness::with_direction(control, LayoutDirection::Rtl)
}

fn volume() -> Slider {
    Slider::new(
        "Volume",
        IntegerRange::new(0, 100, 25, 1, 10).unwrap(),
        SliderOrientation::Horizontal,
    )
}

/// The spoken value after `key` on a focused slider.
fn slider_value(mut h: Harness, key: Key) -> Option<String> {
    h.focus();
    h.key(key);
    h.semantics().value
}

#[test]
fn a_right_to_left_slider_steps_up_with_arrow_left() {
    let up = slider_value(Harness::new(volume()), Key::ArrowRight);
    let down = slider_value(Harness::new(volume()), Key::ArrowLeft);
    assert_ne!(up, down);
    assert_eq!(slider_value(rtl(volume()), Key::ArrowLeft), up);
    assert_eq!(slider_value(rtl(volume()), Key::ArrowRight), down);
    assert_eq!(
        slider_value(rtl(volume()), Key::ArrowUp),
        up,
        "vertical keys do not change"
    );
}

fn index(index: usize) -> ControlMessage {
    ControlMessage::Changed(ControlValue::Index(Some(index)))
}

#[test]
fn right_to_left_tabs_move_forward_with_arrow_left() {
    let tabs = Tabs::hosted("Sections", ["Account", "Password", "Billing"], 0).unwrap();
    let mut h = rtl(tabs);
    h.focus();
    h.key(Key::ArrowLeft);
    h.key(Key::ArrowLeft);
    h.key(Key::ArrowRight);
    h.key(Key::ArrowRight);
    h.key(Key::ArrowRight);
    assert_eq!(
        h.messages(),
        vec![index(1), index(2), index(1), index(0), index(2)],
        "the last move wraps"
    );
}

fn date(day: u8) -> CivilDate {
    CivilDate::new(2024, 5, day).unwrap()
}

fn chosen(day: u8) -> ControlMessage {
    ControlMessage::Changed(ControlValue::Date(Some(date(day))))
}

#[test]
fn a_right_to_left_calendar_moves_days_with_the_mirrored_columns() {
    let calendar = Calendar::new(Locale::parse("en-US").unwrap(), date(15)).with_name("Due");
    let mut h = rtl(calendar);
    h.focus();
    h.key(Key::Tab);
    h.key(Key::Tab);
    for (keys, day) in [
        (&[Key::ArrowLeft][..], 16),
        (&[Key::ArrowRight, Key::ArrowRight], 14),
        (&[Key::ArrowDown], 21),
    ] {
        for key in keys {
            h.key(*key);
        }
        h.key(Key::Enter);
        assert_eq!(h.messages(), vec![chosen(day), ControlMessage::Activated]);
    }
}

const COPY: CommandId = CommandId(1);
const SHARE: CommandId = CommandId(2);

fn more() -> MenuItem {
    MenuItem::submenu("More", vec![MenuItem::action("Share", SHARE)])
}

#[test]
fn a_right_to_left_dropdown_menu_opens_a_submenu_with_arrow_left() {
    let menu = DropdownMenu::new(vec![MenuItem::action("Copy", COPY), more()]).hosted("Edit");
    let mut h = rtl(menu);
    h.focus();
    h.key(Key::End);
    h.key(Key::ArrowRight);
    assert!(
        h.host.get(h.node).unwrap().overlay().is_none(),
        "the arrow away from the submenu side does not open it"
    );
    h.key(Key::End);
    h.key(Key::ArrowLeft);
    assert!(h.host.get(h.node).unwrap().overlay().is_some());
    assert!(texts(&h.paint()).contains(&"Share".to_string()));
    h.key(Key::ArrowRight);
    assert!(h.host.get(h.node).unwrap().overlay().is_none());
    h.key(Key::ArrowLeft);
    h.key(Key::Enter);
    let messages = h.messages();
    assert_eq!(messages.last(), Some(&ControlMessage::Command(SHARE)));
}

#[test]
fn a_right_to_left_menubar_opens_and_closes_submenus_toward_the_left() {
    let bar = Menubar::new(
        "Main menu",
        vec![
            (
                "File".to_string(),
                vec![MenuItem::action("Copy", COPY), more()],
            ),
            (
                "Edit".to_string(),
                vec![MenuItem::action("Undo", CommandId(10))],
            ),
        ],
    );
    let mut h = rtl(bar);
    h.focus();
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    assert_eq!(h.focused_part(), "More");
    h.key(Key::ArrowLeft);
    assert_eq!(h.focused_part(), "Share", "the submenu opens");
    assert_eq!(h.selected_part(), "File");
    h.key(Key::ArrowRight);
    assert_eq!(h.focused_part(), "More", "the submenu closes");
    h.key(Key::ArrowRight);
    assert_eq!(
        h.selected_part(),
        "Edit",
        "at the root, ArrowRight goes back in reading order and wraps"
    );
}

#[test]
fn a_right_to_left_header_runs_its_columns_from_the_right() {
    let column = |title: &str| HeaderColumn {
        title: title.into(),
        width: 100.0,
        min_width: 40.0,
    };
    let mut h = rtl(HeaderView::new(vec![column("Name"), column("Size")]));
    let bounds = h.bounds();
    let y = bounds.center().y;
    h.click(Point::new(bounds.max_x() - 50.0, y));
    assert_eq!(h.focused_part(), "Name", "the first column is at the right");
    h.focus();
    h.key(Key::ArrowLeft);
    assert_eq!(h.focused_part(), "Size");
    h.key(Key::ArrowRight);
    assert_eq!(h.focused_part(), "Name");
}
