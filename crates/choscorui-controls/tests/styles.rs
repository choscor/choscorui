//! Resolved-style checks over every kind, the style cache, and custom
//! recipes. The per-control reference tables are in `styles_*.rs`.

use choscorui_controls::{
    ButtonVariant, ControlKind, ControlSize, Corners, Recipe, ResolvedStyle, Slot, StyleQuery,
    Styles, VisualState,
};
use choscorui_core::{DesignTokens, RecipeSet};
use std::rc::Rc;

fn button(variant: ButtonVariant, size: ControlSize, state: VisualState) -> StyleQuery {
    StyleQuery::new(ControlKind::Button, Slot::Root)
        .variant(variant)
        .size(size)
        .state(state)
}

#[test]
fn every_kind_has_a_reference_table() {
    // Each kind has a `<kind>_matches_its_reference_table` test in a
    // `styles_<group>.rs` file. A new kind needs a table in
    // `docs/controls.md` and a test before this count changes.
    assert_eq!(ControlKind::ALL.len(), 71);
}

#[test]
fn every_kind_and_slot_resolves_to_finite_values() {
    let styles = Styles::new();
    for tokens in [DesignTokens::light(), DesignTokens::dark()] {
        for kind in ControlKind::ALL {
            for slot in Slot::ALL {
                for state in [
                    VisualState::NONE,
                    VisualState::HOVERED,
                    VisualState::DISABLED,
                ] {
                    let style = styles.resolve(&tokens, StyleQuery::new(*kind, *slot).state(state));
                    let values = [
                        style.border_width,
                        style.min_height,
                        style.min_width,
                        style.gap,
                        style.icon_size,
                        style.opacity,
                        style.text.size,
                    ];
                    assert!(
                        values.iter().all(|value| value.is_finite()),
                        "{} {}",
                        kind.name(),
                        slot.name()
                    );
                    assert!((0.0..=1.0).contains(&style.opacity));
                }
            }
        }
    }
}

#[test]
fn the_cache_resolves_each_query_once() {
    let styles = Styles::new();
    let tokens = DesignTokens::light();
    let query = button(ButtonVariant::Outline, ControlSize::Sm, VisualState::NONE);
    // A thousand nodes in the same state cost one computation.
    for _ in 0..1000 {
        styles.resolve(&tokens, query);
    }
    let stats = styles.stats();
    assert_eq!((stats.misses, stats.hits, stats.entries), (1, 999, 1));
    // A theme change is a new key.
    styles.resolve(&DesignTokens::dark(), query);
    assert_eq!(styles.stats().misses, 2);
}

#[test]
fn reduced_motion_zeroes_every_duration() {
    let styles = Styles::new();
    let mut tokens = DesignTokens::light();
    tokens.motion.reduced = true;
    for kind in ControlKind::ALL {
        for slot in Slot::ALL {
            let style = styles.resolve(&tokens, StyleQuery::new(*kind, *slot));
            assert_eq!(style.motion_ms, 0, "{} {}", kind.name(), slot.name());
        }
    }
}

struct Square;

impl Recipe for Square {
    fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
        (query.slot == Slot::Root).then(|| {
            let mut style = ResolvedStyle::new(tokens);
            style.radius = Corners::ZERO;
            style
        })
    }
}

#[test]
fn custom_recipes_replace_some_slots_in_one_recipe_set() {
    let mut styles = Styles::new();
    let set = RecipeSet(7);
    styles.set_recipe(set, ControlKind::Button, Rc::new(Square));
    let query = button(
        ButtonVariant::Default,
        ControlSize::Default,
        VisualState::NONE,
    );
    let tokens = DesignTokens::light();
    assert_eq!(styles.resolve(&tokens, query).radius, Corners::all(8.0));
    let scoped = DesignTokens {
        recipes: set,
        ..tokens
    };
    assert_eq!(styles.resolve(&scoped, query).radius, Corners::ZERO);
    // Slots the recipe declines fall back to the default.
    let label = query.slot(Slot::Label);
    assert_eq!(
        styles.resolve(&scoped, label),
        styles.resolve(&tokens, label)
    );
    assert!(styles.remove_recipe(set, ControlKind::Button));
    assert_eq!(styles.resolve(&scoped, query).radius, Corners::all(8.0));
}
