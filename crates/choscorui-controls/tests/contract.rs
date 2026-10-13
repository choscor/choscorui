//! Contracts that every hosted control keeps. A recipe changes only how a
//! control paints: under another recipe, the same keys bring the same
//! messages, focus, and semantics. Reduced motion finishes every
//! animation at once.
//!
//! The controls are the main catalog and the built-in controls. A guard
//! test checks that they paint every `ControlKind`, except the kinds in
//! `EXEMPT`.

mod catalog;
mod hosted;

use catalog::{AnyFactory, every_control};
use choscorui_controls::{
    ControlKind, ControlMessage, Corners, DefaultRecipe, Recipe, ResolvedStyle, StyleQuery, Styles,
};
use choscorui_core::{
    Color, DesignTokens, Key, KeyEvent, LayoutDirection, RecipeSet, SemanticNode, ShadowLayers,
    ThemeResolver,
};
use hosted::Harness;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

/// The color of a solid background.
fn bg(style: &ResolvedStyle) -> Color {
    style.background.solid_color().expect("a solid background")
}

/// The default look with other geometry and colors: square corners, more
/// padding and gap, taller boxes, and no shadows.
struct Reshaped;

impl Recipe for Reshaped {
    fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
        let mut style = DefaultRecipe.resolve(query, tokens)?;
        style.radius = Corners::ZERO;
        style.shadow = ShadowLayers::NONE;
        style.padding.inline_start += 3.0;
        style.padding.inline_end += 3.0;
        style.padding.block_start += 2.0;
        style.padding.block_end += 2.0;
        style.gap += 4.0;
        style.min_height += 6.0;
        if bg(&style).alpha > 0 {
            style.background = Color::rgba8(40, 140, 230, 255).into();
        }
        Some(style)
    }
}

fn reshaped() -> Styles {
    let mut styles = Styles::new();
    for kind in ControlKind::ALL {
        styles.set_recipe(RecipeSet::DEFAULT, *kind, Rc::new(Reshaped));
    }
    styles
}

/// The keys that each control receives, in order.
const SCRIPT: [Key; 9] = [
    Key::ArrowDown,
    Key::ArrowRight,
    Key::Space,
    Key::Tab,
    Key::Enter,
    Key::ArrowUp,
    Key::Escape,
    Key::End,
    Key::Tab,
];

/// What a control reports after each step of the script.
#[derive(Debug, PartialEq)]
struct Step {
    key: Key,
    handled: bool,
    messages: Vec<ControlMessage>,
    focused: bool,
    semantics: SemanticNode,
    animating: bool,
}

/// Mounts the control with `styles` and `tokens`, focuses it, and plays
/// the script. The control paints after each step.
fn play(factory: &AnyFactory, styles: Styles, tokens: DesignTokens) -> (Harness, Vec<Step>) {
    let themes = ThemeResolver::new(tokens);
    let mut h = Harness::mount_with(factory, themes, styles, LayoutDirection::Ltr);
    let _ = h.host.advance(&mut h.tree, 0.0);
    let bounds = h.bounds();
    h.place(bounds);
    let _ = h.paint();
    let mut steps = Vec::new();
    if h.host.get(h.node).unwrap().focusable() {
        h.focus();
    }
    for key in SCRIPT {
        let handled = h
            .host
            .dispatch_key(&mut h.tree, KeyEvent::Down { key, repeat: false });
        let _ = h.host.dispatch_key(&mut h.tree, KeyEvent::Up { key });
        let bounds = h.bounds();
        h.place(bounds);
        let _ = h.paint();
        steps.push(Step {
            key,
            handled,
            messages: h.messages(),
            focused: h.tree.focus() == Some(h.node),
            semantics: h.semantics(),
            animating: h.host.animating(&h.tree, h.time),
        });
    }
    (h, steps)
}

#[test]
fn another_recipe_keeps_messages_focus_and_semantics() {
    for (name, factory) in every_control() {
        let (_, default) = play(&factory, Styles::new(), DesignTokens::light());
        let (_, swapped) = play(&factory, reshaped(), DesignTokens::light());
        assert_eq!(default.len(), swapped.len(), "{name}");
        for (expected, actual) in default.iter().zip(&swapped) {
            assert_eq!(expected, actual, "{name}: {:?}", expected.key);
        }
    }
}

#[test]
fn reduced_motion_finishes_every_animation_at_once() {
    let mut tokens = DesignTokens::light();
    tokens.motion.reduced = true;
    for (name, factory) in every_control() {
        let (h, steps) = play(&factory, Styles::new(), tokens);
        assert!(
            !h.host.animating(&h.tree, h.time),
            "{name} animates at rest"
        );
        for step in &steps {
            assert!(!step.animating, "{name} animates after {:?}", step.key);
        }
    }
}

/// Kinds that no hosted control paints, with the reason.
const EXEMPT: &[(ControlKind, &str)] = &[
    (
        ControlKind::Tooltip,
        "the TooltipController overlay paints it, not a hosted control; popup.rs tests it",
    ),
    (
        ControlKind::Typography,
        "no built-in control asks for it; text views take TextPreset specs from the tokens",
    ),
];

/// The default look that records each kind that a paint asks for.
struct Recording(Rc<RefCell<BTreeSet<ControlKind>>>);

impl Recipe for Recording {
    fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
        self.0.borrow_mut().insert(query.kind);
        DefaultRecipe.resolve(query, tokens)
    }
}

#[test]
fn the_contract_covers_every_control_kind() {
    let seen = Rc::new(RefCell::new(BTreeSet::new()));
    for (_, factory) in every_control() {
        let mut styles = Styles::new();
        for kind in ControlKind::ALL {
            styles.set_recipe(RecipeSet::DEFAULT, *kind, Rc::new(Recording(seen.clone())));
        }
        let _ = play(&factory, styles, DesignTokens::light());
    }
    let seen = seen.borrow();
    let exempt: BTreeSet<ControlKind> = EXEMPT.iter().map(|(kind, _)| *kind).collect();
    let missing: Vec<&str> = ControlKind::ALL
        .iter()
        .filter(|kind| !seen.contains(kind) && !exempt.contains(kind))
        .map(|kind| kind.name())
        .collect();
    assert!(missing.is_empty(), "no contract control paints {missing:?}");
    let stale: Vec<&str> = exempt
        .iter()
        .filter(|kind| seen.contains(kind))
        .map(|kind| kind.name())
        .collect();
    assert!(
        stale.is_empty(),
        "exempt kinds that a control paints: {stale:?}"
    );
}
