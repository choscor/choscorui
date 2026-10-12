//! State transitions: colors, rings, and shadows ease between two resolved
//! styles over the motion duration, and the Switch thumb slides.

mod hosted;

use choscorui_controls::{
    Button, ButtonVariant, CommandId, ControlKind, Recipe, ResolvedStyle, Slot, StyleQuery, Styles,
    Switch, VisualState,
};
use choscorui_core::{
    Color, ColorRole, DesignTokens, DisplayCommand, DisplayList, GradientStop, LayoutDirection,
    Paint, Point, RecipeSet, ThemeResolver,
};
use hosted::{Harness, rounded_fills};
use std::rc::Rc;

/// The color of a solid background.
fn bg(style: &ResolvedStyle) -> Color {
    style.background.solid_color().expect("a solid background")
}

/// Half of the default 150 ms duration.
const HALF: f64 = 0.075;
const FULL: f64 = 0.15;

fn button() -> Button {
    Button::new("Save", CommandId(1)).with_variant(ButtonVariant::Default)
}

/// The color of the first rounded fill: the button's background.
fn background(list: &DisplayList) -> Color {
    list.commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::FillRoundedRect { color, .. } => Some(*color),
            _ => None,
        })
        .expect("the button fills its background")
}

fn root_style(h: &Harness, state: VisualState) -> ResolvedStyle {
    let tokens = h.themes.application();
    h.styles.resolve(
        &tokens,
        StyleQuery::new(ControlKind::Button, Slot::Root)
            .variant(ButtonVariant::Default)
            .state(state),
    )
}

fn between(value: u8, a: u8, b: u8) -> bool {
    value > a.min(b) && value < a.max(b)
}

/// The pointer moves onto the button at time zero, and the button paints.
fn hovered() -> Harness {
    let mut h = Harness::new(button());
    h.paint();
    let center = h.bounds().center();
    h.hover(center);
    h
}

#[test]
fn hover_blends_at_half_duration_and_completes_at_the_duration() {
    let mut h = hovered();
    let rest = bg(&root_style(&h, VisualState::NONE));
    let hover = bg(&root_style(&h, VisualState::HOVERED));
    assert_ne!(rest.alpha, hover.alpha, "the hover look changes the alpha");
    assert_eq!(background(&h.paint()), rest, "the blend starts at rest");
    assert!(h.host.animating(&h.tree, 0.0));

    h.set_time(HALF);
    let middle = background(&h.paint());
    assert!(
        between(middle.alpha, rest.alpha, hover.alpha),
        "{middle:?} is between {rest:?} and {hover:?}"
    );
    assert!(h.host.animating(&h.tree, HALF));
    assert_eq!(h.host.transition_stats().running, 1);

    h.set_time(FULL);
    assert_eq!(background(&h.paint()), hover);
    assert!(!h.host.animating(&h.tree, FULL));
    assert_eq!(h.host.transition_stats().running, 0);
}

#[test]
fn reduced_motion_changes_the_style_at_once() {
    let mut tokens = DesignTokens::light();
    tokens.motion.reduced = true;
    let mut h = Harness::mount(
        Box::new(button()),
        ThemeResolver::new(tokens),
        Styles::new(),
        LayoutDirection::Ltr,
    );
    h.paint();
    let center = h.bounds().center();
    h.hover(center);
    let hover = bg(&root_style(&h, VisualState::HOVERED));
    assert_eq!(background(&h.paint()), hover);
    assert!(!h.host.animating(&h.tree, 0.0));
    assert_eq!(h.host.transition_stats().running, 0);
}

#[test]
fn no_record_stays_after_the_transition_of_a_cold_node() {
    let mut h = hovered();
    h.paint();
    h.set_time(FULL);
    h.paint();
    // A hot node keeps only its last styles; it runs nothing.
    let stats = h.host.transition_stats();
    assert_eq!((stats.nodes, stats.running), (1, 0));

    // The pointer leaves: the hover look eases out, then the record goes.
    h.hover(Point::new(700.0, 500.0));
    h.paint();
    assert_eq!(h.host.transition_stats().running, 1);
    h.set_time(FULL + HALF);
    h.paint();
    assert_eq!(h.host.transition_stats().nodes, 1);
    assert!(h.host.advance(&mut h.tree, 2.0 * FULL), "the blend ended");
    assert_eq!(h.host.transition_stats().nodes, 0);
    h.set_time(2.0 * FULL);
    let rest = bg(&root_style(&h, VisualState::NONE));
    assert_eq!(background(&h.paint()), rest);
    assert_eq!(h.host.transition_stats().nodes, 0);
}

#[test]
fn a_node_out_of_view_does_not_hold_frames() {
    let mut h = hovered();
    h.paint();
    h.tree.set_visible(h.node, false).unwrap();
    assert!(!h.host.animating(&h.tree, HALF));
    // The end of the blend drops its record without a paint.
    h.host.advance(&mut h.tree, FULL);
    assert_eq!(h.host.transition_stats().running, 0);
}

/// Paints every button background in a fixed color.
struct Flat(Color);

impl Recipe for Flat {
    fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
        (query.slot == Slot::Root).then(|| {
            let mut style = ResolvedStyle::new(tokens);
            style.background = self.0.into();
            style.radius = choscorui_controls::Corners::all(4.0);
            style
        })
    }
}

#[test]
fn a_recipe_swap_during_a_transition_eases_to_the_new_style() {
    let mut h = hovered();
    h.paint();
    h.set_time(HALF);
    let middle = background(&h.paint());
    let flat = Color::rgba8(0, 128, 0, 255);
    h.styles
        .set_recipe(RecipeSet::DEFAULT, ControlKind::Button, Rc::new(Flat(flat)));
    // The blend starts again from the color on screen.
    let restarted = background(&h.paint());
    assert_eq!(restarted, middle);
    h.set_time(HALF + HALF);
    let next = background(&h.paint());
    assert!(between(next.green, middle.green, flat.green), "{next:?}");
    h.set_time(HALF + FULL);
    assert_eq!(background(&h.paint()), flat);
    assert_eq!(h.host.transition_stats().running, 0);
}

/// Paints every button background in a gradient from `0` to `1`.
struct Gradient(Color, Color);

impl Recipe for Gradient {
    fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
        (query.slot == Slot::Root).then(|| {
            let mut style = ResolvedStyle::new(tokens);
            style.background = Paint::LinearGradient {
                start: Point::new(0.0, 0.0),
                end: Point::new(1.0, 0.0),
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: self.0,
                    },
                    GradientStop {
                        offset: 1.0,
                        color: self.1,
                    },
                ],
            };
            style.radius = choscorui_controls::Corners::all(4.0);
            style
        })
    }
}

/// The paint of the first path fill.
fn path_paint(list: &DisplayList) -> Option<Paint> {
    list.commands().iter().find_map(|command| match command {
        DisplayCommand::FillPath { paint, .. } => Some(paint.clone()),
        _ => None,
    })
}

#[test]
fn a_change_to_a_gradient_shows_the_gradient_at_once() {
    let mut h = hovered();
    h.paint();
    h.set_time(HALF);
    let (from, to) = (Color::rgba8(0, 0, 200, 255), Color::rgba8(200, 0, 0, 255));
    h.styles.set_recipe(
        RecipeSet::DEFAULT,
        ControlKind::Button,
        Rc::new(Gradient(from, to)),
    );
    let bounds = h.bounds();
    let Some(Paint::LinearGradient { start, end, stops }) = path_paint(&h.paint()) else {
        panic!("the button fills its background with the gradient");
    };
    // The unit square of the box maps onto the button bounds.
    assert_eq!(start, bounds.origin);
    assert_eq!(end, Point::new(bounds.max_x(), bounds.origin.y));
    assert_eq!((stops[0].color, stops[1].color), (from, to));
}

#[test]
fn the_switch_thumb_slides_over_the_duration() {
    let tokens = DesignTokens::light();
    let thumb_x = |h: &Harness| {
        rounded_fills(&h.paint(), tokens.color(ColorRole::Background))
            .last()
            .expect("the thumb paints")
            .origin
            .x
    };
    let mut h = Harness::new(Switch::new("", false));
    let start = thumb_x(&h);
    let center = h.bounds().center();
    h.click(center);
    assert!(h.host.animating(&h.tree, 0.0));
    assert_eq!(thumb_x(&h), start, "the slide starts at the start");

    h.set_time(HALF);
    let middle = thumb_x(&h);
    h.set_time(FULL);
    let end = thumb_x(&h);
    assert!(end > start, "a checked thumb rests at the end");
    assert!(middle > start && middle < end, "{start} < {middle} < {end}");
    assert!(!h.host.animating(&h.tree, FULL + 0.01));
}
