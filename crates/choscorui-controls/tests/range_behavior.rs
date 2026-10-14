//! The step, page, repeat, wrap, and animation behavior of the range
//! controls through the control host: Slider, RangeSlider, SpinBox, Dial,
//! ScrollBar, NumericDisplay, and Progress.

mod hosted;

use choscorui_controls::{
    ControlMessage, ControlValue, Dial, IntegerRange, NumberBase, NumericDisplay, Progress,
    RangeSlider, ScrollBar, ScrollOrientation, Slider, SliderOrientation, SpinBox,
};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, Key, Locale, Point, PointerButton, PointerEvent,
    SemanticAction, SemanticRange, SemanticRole,
};
use hosted::{Harness, texts};

fn number(value: f64) -> ControlMessage {
    ControlMessage::Changed(ControlValue::Number(value))
}

fn down(h: &mut Harness, at: Point) {
    let event = PointerEvent::Down {
        position: at,
        button: PointerButton::Primary,
    };
    h.host.dispatch_pointer(&mut h.tree, event);
}

fn drag(h: &mut Harness, at: Point) {
    h.host
        .dispatch_pointer(&mut h.tree, PointerEvent::Move { position: at });
}

fn up(h: &mut Harness, at: Point) {
    let event = PointerEvent::Up {
        position: at,
        button: PointerButton::Primary,
    };
    h.host.dispatch_pointer(&mut h.tree, event);
}

/// Advances the host time to `time`, which fires the due timers.
fn advance(h: &mut Harness, time: f64) {
    h.set_time(time);
    h.host.advance(&mut h.tree, time);
}

/// The point at `fraction` across the bounds of `h`, at its middle.
fn across(h: &Harness, fraction: f64) -> Point {
    let bounds = h.bounds();
    Point::new(
        bounds.origin.x + bounds.size.width * fraction,
        bounds.center().y,
    )
}

#[test]
fn a_slider_snaps_the_pointer_and_set_value_to_the_step_and_pages() {
    let range = IntegerRange::new(0, 10, 4, 2, 5).unwrap();
    let mut h = Harness::new(Slider::new("Level", range, SliderOrientation::Horizontal));
    // The ends of the bounds are the ends of the range: the thumb travel
    // stays inside the track.
    h.click(across(&h, 0.0));
    h.click(across(&h, 0.999));
    // The middle is 5, which snaps to the step grid.
    h.click(across(&h, 0.5));
    assert!(h.perform(SemanticAction::SetValue, Some("3")));
    h.focus();
    h.key(Key::PageUp);
    h.key(Key::PageDown);
    assert_eq!(
        h.messages(),
        [0.0, 10.0, 6.0, 4.0, 9.0, 4.0].map(number).to_vec()
    );
}

#[test]
fn range_slider_thumbs_that_meet_follow_the_drag_direction() {
    let mut h = Harness::new(RangeSlider::new("Price", 0.0, 100.0, 5.0, 50.0, 50.0));
    let [start, low, middle, high, end] = [0.0, 0.1, 0.5, 0.4, 1.0].map(|f| across(&h, f));
    down(&mut h, middle);
    drag(&mut h, end);
    up(&mut h, end);
    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Range(50.0, 50.0))
    );
    down(&mut h, middle);
    drag(&mut h, start);
    up(&mut h, start);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Changed(ControlValue::Range(50.0, 100.0)),
            ControlMessage::Changed(ControlValue::Range(0.0, 50.0)),
        ]
    );

    // Disabling ends a drag.
    down(&mut h, low);
    h.messages();
    h.set_enabled(false);
    h.set_enabled(true);
    drag(&mut h, high);
    assert!(h.messages().is_empty());
}

#[test]
fn range_slider_rounds_values_to_the_decimals_of_the_step() {
    let mut h = Harness::new(RangeSlider::new("Ratio", 0.0, 1.0, 0.1, 0.0, 1.0));
    h.focus();
    for _ in 0..3 {
        h.key(Key::ArrowRight);
    }
    assert_eq!(h.semantics().value.as_deref(), Some("0.3 – 1"));
}

#[test]
fn a_spin_box_pages_repeats_its_steppers_and_formats_with_the_locale() {
    let range = IntegerRange::new(0, 2000, 1500, 1, 100).unwrap();
    let locale = Locale::parse("en-US").unwrap();
    let mut h = Harness::new(SpinBox::new("Count", range).with_locale(locale));
    assert!(texts(&h.paint()).contains(&"1,500".to_owned()));
    h.focus();
    h.key(Key::PageUp);
    assert!(texts(&h.paint()).contains(&"1,600".to_owned()));
    // A held up stepper repeats after a delay, until the release.
    let bounds = h.bounds();
    let stepper = Point::new(bounds.max_x() - 4.0, bounds.origin.y + 4.0);
    down(&mut h, stepper);
    advance(&mut h, 0.1);
    advance(&mut h, 0.5);
    advance(&mut h, 0.6);
    up(&mut h, stepper);
    advance(&mut h, 2.0);
    assert_eq!(
        h.messages(),
        [1600.0, 1601.0, 1602.0, 1603.0].map(number).to_vec()
    );
    // Left and Right move the caret in the text: typed digits go to the
    // start.
    h.key(Key::ArrowLeft);
    assert!(h.text("1"));
    assert!(texts(&h.paint()).contains(&"11,603".to_owned()));
}

#[test]
fn a_wrapping_dial_has_one_place_per_value_and_wraps_its_pages() {
    let range = IntegerRange::new(0, 359, 350, 1, 15).unwrap();
    let mut dial = Dial::new(range, true);
    assert!(dial.handle_key(Key::PageUp));
    assert_eq!(dial.value(), 5);
    assert!(dial.handle_key(Key::PageDown));
    assert!(dial.handle_key(Key::ArrowDown));
    assert_eq!(dial.value(), 349);
    let last = Dial::new(IntegerRange::new(0, 359, 359, 1, 15).unwrap(), true);
    assert!(last.angle() < 2.0 * std::f64::consts::PI - 0.01);
}

#[test]
fn a_dial_drag_keeps_its_end_snaps_and_hits_only_the_circle() {
    let range = IntegerRange::new(0, 100, 0, 10, 10).unwrap();
    let mut h = Harness::new(Dial::new(range, false).hosted("Gain"));
    let bounds = h.bounds();
    let center = bounds.center();
    let radius = bounds.size.width.min(bounds.size.height) * 0.4;
    // Clockwise degrees from straight up.
    let at = |degrees: f64| {
        let angle = degrees.to_radians();
        Point::new(
            center.x + angle.sin() * radius,
            center.y - angle.cos() * radius,
        )
    };
    // 90 degrees is 83 of 100, which snaps to 80.
    down(&mut h, at(90.0));
    drag(&mut h, at(130.0));
    // Through the gap at the bottom and back in at the minimum end, the
    // drag stays at the maximum.
    drag(&mut h, at(180.0));
    drag(&mut h, at(220.0));
    drag(&mut h, at(240.0));
    up(&mut h, at(240.0));
    // A press in a corner of the bounds is outside the circle.
    down(&mut h, bounds.origin);
    up(&mut h, bounds.origin);
    assert_eq!(h.messages(), [80.0, 100.0].map(number).to_vec());
}

#[test]
fn a_scroll_bar_repeats_track_paging_and_reports_its_limits() {
    let mut bar = ScrollBar::new("Scroll", ScrollOrientation::Vertical);
    bar.set_range(1000.0, 100.0, 200.0);
    let mut h = Harness::new(bar);
    let bounds = h.bounds();
    // A held press at 150 of 200 points pages until the thumb reaches it.
    let press = Point::new(bounds.center().x, bounds.origin.y + 150.0);
    down(&mut h, press);
    for step in 0..20 {
        advance(&mut h, 0.4 + f64::from(step) * 0.06);
    }
    up(&mut h, press);
    let pages: Vec<_> = (1..=7)
        .map(|page| number(f64::from(page) * 100.0))
        .collect();
    assert_eq!(h.messages(), pages);

    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Number(900.0))
    );
    assert!(!h.perform(SemanticAction::Increment, None), "at the end");
    assert!(h.perform(SemanticAction::Decrement, None));
    h.set_enabled(false);
    let semantics = h.semantics();
    assert!(semantics.state.disabled);
    assert!(semantics.actions.is_empty());
}

#[test]
fn a_numeric_display_counter_is_adjustable_and_keeps_i64_values_exact() {
    let display = NumericDisplay::new(20, NumberBase::Decimal)
        .hosted("Counter")
        .counter();
    let mut h = Harness::new(display);
    let exact = (1_i64 << 53) + 1;
    let text = |value: i64| ControlValue::Text(value.to_string());
    assert!(h.host.set_value(&mut h.tree, h.node, &text(exact)));
    let semantics = h.semantics();
    assert_eq!(semantics.role, SemanticRole::SpinButton);
    assert!(semantics.actions.contains(&SemanticAction::Increment));
    assert_eq!(semantics.value, Some(exact.to_string()));
    assert!(h.perform(SemanticAction::Increment, None));
    assert_eq!(h.messages(), [ControlMessage::Changed(text(exact + 1))]);
}

#[test]
fn a_progress_fill_moves_over_the_recipe_motion_and_reports_its_range() {
    let mut h = Harness::new(Progress::new("Upload", 0, 200).unwrap());
    let primary = DesignTokens::light().color(ColorRole::Primary);
    let fill = |h: &Harness| {
        h.paint()
            .commands()
            .iter()
            .find_map(|command| match command {
                DisplayCommand::FillRect { rect, color } if *color == primary => {
                    Some(rect.size.width)
                }
                _ => None,
            })
            .unwrap_or(0.0)
    };
    let width = h.bounds().size.width;
    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Number(200.0))
    );
    assert!(h.host.animating(&h.tree, 0.0));
    assert_eq!(fill(&h), 0.0, "the fill starts where it was");
    h.set_time(0.075);
    let halfway = fill(&h);
    assert!(halfway > 0.0 && halfway < width, "{halfway} of {width}");
    h.set_time(1.0);
    assert_eq!(fill(&h), width);
    assert!(!h.host.animating(&h.tree, 1.0));
    assert_eq!(
        h.semantics().state.range,
        Some(SemanticRange {
            value: 200,
            minimum: 0,
            maximum: 200,
            step: 1,
        })
    );
}
