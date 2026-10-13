//! RangeSlider through the control host.

mod hosted;

use choscorui_controls::{
    ControlMessage, ControlValue, Orientation, RangeSlider, ShortcutModifiers,
};
use choscorui_core::{Key, KeyEvent, LayoutDirection, Point, SemanticAction, SemanticRole};
use hosted::Harness;

fn price() -> RangeSlider {
    RangeSlider::new("Price", 0.0, 100.0, 5.0, 25.0, 75.0)
}

#[test]
fn keys_move_the_active_thumb_and_tab_moves_between_thumbs() {
    let mut h = Harness::new(price());
    assert_eq!(h.measure().width, 160.0, "40 spacing units");
    h.focus();
    h.key(Key::ArrowRight);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Range(30.0, 75.0))]
    );
    // Tab stays inside the slider to reach the high thumb.
    assert!(h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false
        }
    ));
    h.key(Key::PageUp);
    h.key(Key::End);
    assert_eq!(h.semantics().value.as_deref(), Some("30 – 100"));
    // Tab from the high thumb leaves the slider.
    assert!(!h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false
        }
    ));
    h.modifiers(ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    });
    assert!(h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false
        }
    ));
    h.modifiers(ShortcutModifiers::default());
    h.key(Key::End);
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("100 – 100"),
        "the low thumb stops at the high thumb"
    );
}

#[test]
fn a_press_moves_the_nearer_thumb_and_drags_it() {
    let mut h = Harness::new(price());
    let bounds = h.bounds();
    // A point on the thumb travel, half a 16 pt thumb inside each end.
    let at = |fraction: f64| {
        Point::new(
            bounds.origin.x + 8.0 + (bounds.size.width - 16.0) * fraction,
            bounds.center().y,
        )
    };
    h.click(at(0.9));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Range(25.0, 90.0))]
    );
    h.click(at(0.1));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Range(10.0, 90.0))]
    );
    assert_eq!(h.semantics().role, SemanticRole::Slider);
}

#[test]
fn rtl_and_vertical_sliders_map_keys_and_points() {
    let mut h = Harness::with_direction(price(), LayoutDirection::Rtl);
    h.focus();
    h.key(Key::ArrowLeft);
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("30 – 75"),
        "left is forward"
    );
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 2.0, bounds.center().y));
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("30 – 100"),
        "the left end is the maximum"
    );

    let mut v = Harness::new(price().with_orientation(Orientation::Vertical));
    assert_eq!(v.measure().height, 160.0);
    v.focus();
    v.key(Key::ArrowUp);
    assert_eq!(v.semantics().value.as_deref(), Some("30 – 75"));
    assert!(v.perform(SemanticAction::Decrement, None));
    assert_eq!(v.semantics().value.as_deref(), Some("25 – 75"));
}
