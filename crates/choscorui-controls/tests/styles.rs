//! Resolved-style tables for the default recipes, the style cache, and
//! custom recipes.

use choscorui_controls::{
    ButtonVariant, ControlKind, ControlSize, Corners, Recipe, ResolvedStyle, Slot, StyleQuery,
    Styles, VisualState,
};
use choscorui_core::{
    BaseColor, ColorRole, ColorScheme, DesignTokens, FontWeight, RecipeSet, ShadowScale,
};
use std::rc::Rc;

fn button(variant: ButtonVariant, size: ControlSize, state: VisualState) -> StyleQuery {
    StyleQuery::new(ControlKind::Button, Slot::Root)
        .variant(variant)
        .size(size)
        .state(state)
}

#[test]
fn button_sizes_match_the_reference() {
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    // (size, height, width, inline padding, gap, radius, icon)
    let table = [
        (ControlSize::Xs, 24.0, 0.0, 8.0, 4.0, 8.0, 12.0),
        (ControlSize::Sm, 32.0, 0.0, 10.0, 4.0, 8.0, 16.0),
        (ControlSize::Default, 36.0, 0.0, 10.0, 6.0, 8.0, 16.0),
        (ControlSize::Lg, 40.0, 0.0, 10.0, 6.0, 8.0, 16.0),
        (ControlSize::IconXs, 24.0, 24.0, 0.0, 0.0, 8.0, 12.0),
        (ControlSize::IconSm, 32.0, 32.0, 0.0, 0.0, 8.0, 16.0),
        (ControlSize::Icon, 36.0, 36.0, 0.0, 0.0, 8.0, 16.0),
        (ControlSize::IconLg, 40.0, 40.0, 0.0, 0.0, 8.0, 16.0),
    ];
    for (size, height, width, inline, gap, radius, icon) in table {
        let style = styles.resolve(
            &tokens,
            button(ButtonVariant::Default, size, VisualState::NONE),
        );
        assert_eq!(
            (
                style.min_height,
                style.min_width,
                style.padding.inline_start,
                style.gap,
                style.radius,
                style.icon_size
            ),
            (height, width, inline, gap, Corners::all(radius), icon),
            "{}",
            size.name()
        );
        assert_eq!(style.text.weight, FontWeight::MEDIUM);
    }
}

#[test]
fn button_variants_follow_the_palette_in_each_scheme() {
    let styles = Styles::new();
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        let tokens = DesignTokens::new(BaseColor::Zinc, scheme);
        let dark = scheme == ColorScheme::Dark;
        let c = |role| tokens.color(role);
        let get =
            |variant, state| styles.resolve(&tokens, button(variant, ControlSize::Default, state));
        let rest = get(ButtonVariant::Default, VisualState::NONE);
        assert_eq!(rest.background, c(ColorRole::Primary));
        assert_eq!(rest.foreground, c(ColorRole::PrimaryForeground));
        let hover = get(ButtonVariant::Default, VisualState::HOVERED);
        assert_eq!(hover.background, c(ColorRole::Primary).with_alpha(0.8));

        let outline = get(ButtonVariant::Outline, VisualState::NONE);
        assert_eq!(outline.shadow, tokens.shadow(ShadowScale::Xs));
        assert_eq!(
            outline.border,
            if dark {
                c(ColorRole::Input)
            } else {
                c(ColorRole::Border)
            }
        );

        let destructive = get(ButtonVariant::Destructive, VisualState::NONE);
        let alpha = if dark { 0.2 } else { 0.1 };
        assert_eq!(
            destructive.background,
            c(ColorRole::Destructive).with_alpha(alpha)
        );
        assert_eq!(destructive.foreground, c(ColorRole::Destructive));

        let link = get(ButtonVariant::Link, VisualState::HOVERED);
        assert!(link.underline);

        // A disabled button ignores hover and fades to half.
        let disabled = get(
            ButtonVariant::Default,
            VisualState::HOVERED.with(VisualState::DISABLED),
        );
        assert_eq!(disabled.background, c(ColorRole::Primary));
        assert_eq!(disabled.opacity, 0.5);

        let pressed = get(ButtonVariant::Ghost, VisualState::PRESSED);
        assert_eq!(pressed.offset_y, 1.0);
    }
}

#[test]
fn focus_ring_and_invalid_ring_follow_the_tokens() {
    let styles = Styles::new();
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        let tokens = DesignTokens::new(BaseColor::Neutral, scheme);
        let input = StyleQuery::new(ControlKind::Input, Slot::Root);
        let rest = styles.resolve(&tokens, input);
        assert_eq!(rest.ring, None);
        assert_eq!(rest.border, tokens.color(ColorRole::Input));
        assert_eq!(rest.min_height, 36.0);

        let focused = styles.resolve(&tokens, input.state(VisualState::FOCUS_VISIBLE));
        let ring = focused.ring.unwrap();
        assert_eq!(ring.width, 3.0);
        assert_eq!(ring.color, tokens.color(ColorRole::Ring).with_alpha(0.5));
        assert_eq!(focused.border, tokens.color(ColorRole::Ring));

        // Pointer focus shows no ring.
        let pointer = styles.resolve(&tokens, input.state(VisualState::FOCUSED));
        assert_eq!(pointer.ring, None);

        let invalid = styles.resolve(&tokens, input.state(VisualState::INVALID));
        let alpha = if scheme == ColorScheme::Dark {
            0.4
        } else {
            0.2
        };
        assert_eq!(
            invalid.ring.unwrap().color,
            tokens.color(ColorRole::Destructive).with_alpha(alpha)
        );
    }
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
