//! Reference tables of the buttons, choices, toggles, sliders, labels, and
//! key caps, as `docs/controls.md` pins them.

use choscorui_controls::{
    ButtonVariant, ControlKind, ControlSize, Corners, Cursor, Edges, KbdVariant, ResolvedStyle,
    Ring, Slot, StyleQuery, Styles, ToggleVariant, VisualState,
};
use choscorui_core::{
    BaseColor, Color, ColorRole, ColorScheme, DesignTokens, FontWeight, RadiusScale, ShadowLayers,
    ShadowScale, TextSize,
};

/// The color of a solid background.
fn bg(style: &ResolvedStyle) -> Color {
    style.background.solid_color().expect("a solid background")
}

fn get(tokens: &DesignTokens, query: StyleQuery) -> ResolvedStyle {
    Styles::new().resolve(tokens, query)
}

/// Both schemes of the neutral base and of one tinted base.
fn schemes() -> [DesignTokens; 4] {
    [
        DesignTokens::light(),
        DesignTokens::dark(),
        DesignTokens::new(BaseColor::Zinc, ColorScheme::Light),
        DesignTokens::new(BaseColor::Zinc, ColorScheme::Dark),
    ]
}

fn dark(tokens: &DesignTokens) -> bool {
    tokens.scheme == ColorScheme::Dark
}

/// The light-scheme or the dark-scheme value.
fn pick<T>(tokens: &DesignTokens, light: T, dark_value: T) -> T {
    if dark(tokens) { dark_value } else { light }
}

fn radius(tokens: &DesignTokens, step: RadiusScale) -> Corners {
    Corners::all(tokens.radius(step))
}

fn focus_ring(tokens: &DesignTokens) -> Option<Ring> {
    Some(Ring {
        width: 3.0,
        color: tokens.color(ColorRole::Ring).with_alpha(0.5),
    })
}

fn invalid_ring(tokens: &DesignTokens) -> Option<Ring> {
    let alpha = if dark(tokens) { 0.4 } else { 0.2 };
    Some(Ring {
        width: 3.0,
        color: tokens.color(ColorRole::Destructive).with_alpha(alpha),
    })
}

/// The invalid edge: `destructive` at full strength in both schemes.
fn invalid_border(tokens: &DesignTokens) -> Color {
    tokens.color(ColorRole::Destructive)
}

fn button(variant: ButtonVariant, size: ControlSize, state: VisualState) -> StyleQuery {
    StyleQuery::new(ControlKind::Button, Slot::Root)
        .variant(variant)
        .size(size)
        .state(state)
}

#[test]
fn button_matches_its_reference_table() {
    let tokens = DesignTokens::light();
    let md = tokens.radius(RadiusScale::Md);
    // (size, height, width, inline padding, gap, radius, icon, text)
    let sizes = [
        (
            ControlSize::Xs,
            24.0,
            0.0,
            8.0,
            4.0,
            md.min(8.0),
            12.0,
            TextSize::Xs,
        ),
        (
            ControlSize::Sm,
            32.0,
            0.0,
            10.0,
            4.0,
            md.min(10.0),
            16.0,
            TextSize::Sm,
        ),
        (
            ControlSize::Default,
            36.0,
            0.0,
            10.0,
            6.0,
            md,
            16.0,
            TextSize::Sm,
        ),
        (
            ControlSize::Lg,
            40.0,
            0.0,
            10.0,
            6.0,
            md,
            16.0,
            TextSize::Sm,
        ),
        (
            ControlSize::IconXs,
            24.0,
            24.0,
            0.0,
            0.0,
            md.min(8.0),
            12.0,
            TextSize::Sm,
        ),
        (
            ControlSize::IconSm,
            32.0,
            32.0,
            0.0,
            0.0,
            md.min(10.0),
            16.0,
            TextSize::Sm,
        ),
        (
            ControlSize::Icon,
            36.0,
            36.0,
            0.0,
            0.0,
            md,
            16.0,
            TextSize::Sm,
        ),
        (
            ControlSize::IconLg,
            40.0,
            40.0,
            0.0,
            0.0,
            md,
            16.0,
            TextSize::Sm,
        ),
    ];
    for (size, height, width, inline, gap, corner, icon, text) in sizes {
        let style = get(
            &tokens,
            button(ButtonVariant::Default, size, VisualState::NONE),
        );
        assert_eq!(
            (
                style.min_height,
                style.min_width,
                style.padding,
                style.gap,
                style.radius,
                style.icon_size,
                style.text,
                style.border_width
            ),
            (
                height,
                width,
                Edges::axes(inline, 0.0),
                gap,
                Corners::all(corner),
                icon,
                tokens.text(text, FontWeight::MEDIUM),
                1.0
            ),
            "{}",
            size.name()
        );
    }

    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let a = |role, alpha: f64| tokens.color(role).with_alpha(alpha);
        let clear = Color::TRANSPARENT;
        // (variant, rest fill, rest text, hover fill, hover text)
        let variants = [
            (
                ButtonVariant::Default,
                c(ColorRole::Primary),
                c(ColorRole::PrimaryForeground),
                a(ColorRole::Primary, 0.8),
                c(ColorRole::PrimaryForeground),
            ),
            (
                ButtonVariant::Outline,
                pick(&tokens, c(ColorRole::Background), a(ColorRole::Input, 0.3)),
                c(ColorRole::Foreground),
                pick(&tokens, c(ColorRole::Muted), a(ColorRole::Input, 0.5)),
                c(ColorRole::Foreground),
            ),
            (
                ButtonVariant::Secondary,
                c(ColorRole::Secondary),
                c(ColorRole::SecondaryForeground),
                c(ColorRole::Secondary).mix(c(ColorRole::Foreground), 0.05),
                c(ColorRole::SecondaryForeground),
            ),
            (
                ButtonVariant::Ghost,
                clear,
                c(ColorRole::Foreground),
                pick(&tokens, c(ColorRole::Muted), a(ColorRole::Muted, 0.5)),
                c(ColorRole::Foreground),
            ),
            (
                ButtonVariant::Destructive,
                a(ColorRole::Destructive, pick(&tokens, 0.1, 0.2)),
                c(ColorRole::Destructive),
                a(ColorRole::Destructive, pick(&tokens, 0.2, 0.3)),
                c(ColorRole::Destructive),
            ),
            (
                ButtonVariant::Link,
                clear,
                c(ColorRole::Primary),
                clear,
                c(ColorRole::Primary),
            ),
        ];
        for (variant, fill, text, hover_fill, hover_text) in variants {
            let look = |state| get(&tokens, button(variant, ControlSize::Default, state));
            let name = variant.name();
            let rest = look(VisualState::NONE);
            assert_eq!((bg(&rest), rest.foreground), (fill, text), "{name}");
            assert_eq!((rest.ring, rest.opacity, rest.offset_y), (None, 1.0, 0.0));
            let hover = look(VisualState::HOVERED);
            assert_eq!(
                (bg(&hover), hover.foreground),
                (hover_fill, hover_text),
                "{name} hovered"
            );
            assert_eq!(hover.underline, variant == ButtonVariant::Link, "{name}");

            let pressed = look(VisualState::PRESSED);
            assert_eq!(pressed.offset_y, 1.0, "{name} pressed");

            // A disabled button ignores hover and fades to half.
            let disabled = look(VisualState::HOVERED.with(VisualState::DISABLED));
            assert_eq!((bg(&disabled), disabled.opacity), (fill, 0.5));
            assert_eq!(disabled.cursor, Cursor::Arrow);

            let focused = look(VisualState::FOCUS_VISIBLE);
            if variant == ButtonVariant::Destructive {
                assert_eq!(focused.border, a(ColorRole::Destructive, 0.4));
                assert_eq!(focused.ring, invalid_ring(&tokens));
            } else {
                assert_eq!(focused.border, c(ColorRole::Ring), "{name} focused");
                assert_eq!(focused.ring, focus_ring(&tokens), "{name} focused");
            }

            // A button is not a field: the invalid flag changes nothing.
            assert_eq!(look(VisualState::INVALID), rest, "{name} invalid");
        }

        let outline = get(
            &tokens,
            button(
                ButtonVariant::Outline,
                ControlSize::Default,
                VisualState::NONE,
            ),
        );
        assert_eq!(
            outline.border,
            pick(&tokens, c(ColorRole::Border), c(ColorRole::Input))
        );
        assert_eq!(outline.shadow, tokens.shadow(ShadowScale::Xs));
        // An open outline or ghost button keeps the `muted` fill.
        for variant in [ButtonVariant::Outline, ButtonVariant::Ghost] {
            let open = get(
                &tokens,
                button(variant, ControlSize::Default, VisualState::OPEN),
            );
            assert_eq!(bg(&open), c(ColorRole::Muted), "{}", variant.name());
            let pressed_open = get(
                &tokens,
                button(
                    variant,
                    ControlSize::Default,
                    VisualState::OPEN.with(VisualState::PRESSED),
                ),
            );
            assert_eq!(pressed_open.offset_y, 0.0);
        }
    }
}

#[test]
fn button_group_matches_its_reference_table() {
    for tokens in schemes() {
        let query = |slot| StyleQuery::new(ControlKind::ButtonGroup, slot);
        let root = get(&tokens, query(Slot::Root));
        assert_eq!(
            (root.radius, root.gap),
            (radius(&tokens, RadiusScale::Md), 8.0)
        );

        let text = get(&tokens, query(Slot::Text));
        assert_eq!(
            (bg(&text), text.border, text.border_width),
            (
                tokens.color(ColorRole::Muted),
                // The edge of the outline toggle members it joins.
                tokens.color(ColorRole::Input),
                1.0
            )
        );
        assert_eq!(
            (
                text.radius,
                text.gap,
                text.padding,
                text.min_height,
                text.shadow
            ),
            (
                radius(&tokens, RadiusScale::Md),
                8.0,
                Edges::axes(10.0, 0.0),
                36.0,
                tokens.shadow(ShadowScale::Xs)
            )
        );
        assert_eq!(text.text, tokens.text(TextSize::Sm, FontWeight::MEDIUM));

        let separator = get(&tokens, query(Slot::Separator));
        assert_eq!(
            (bg(&separator), separator.min_width, separator.min_height),
            (tokens.color(ColorRole::Input), 1.0, 1.0)
        );
    }
}

/// The shared look of a checkbox and a radio button; only the radius
/// differs.
fn assert_choice_box(tokens: &DesignTokens, kind: ControlKind, corner: Corners) {
    let query = |state| StyleQuery::new(kind, Slot::Root).state(state);
    let c = |role| tokens.color(role);
    let rest = get(tokens, query(VisualState::NONE));
    let fill = if dark(tokens) {
        c(ColorRole::Input).with_alpha(0.3)
    } else {
        Color::TRANSPARENT
    };
    assert_eq!(
        (rest.min_width, rest.min_height, rest.radius),
        (16.0, 16.0, corner),
        "{}",
        kind.name()
    );
    assert_eq!(
        (rest.border, rest.border_width, bg(&rest), rest.shadow),
        (
            c(ColorRole::Input),
            1.0,
            fill,
            tokens.shadow(ShadowScale::Xs)
        )
    );
    assert_eq!(rest.foreground, c(ColorRole::PrimaryForeground));
    for on in [VisualState::CHECKED, VisualState::INDETERMINATE] {
        let checked = get(tokens, query(on));
        assert_eq!(
            (bg(&checked), checked.border),
            (c(ColorRole::Primary), c(ColorRole::Primary))
        );
    }
    let focused = get(tokens, query(VisualState::FOCUS_VISIBLE));
    assert_eq!(
        (focused.border, focused.ring),
        (c(ColorRole::Ring), focus_ring(tokens))
    );
    // Invalid: a destructive edge, no ring, and no shadow.
    let invalid = get(tokens, query(VisualState::INVALID));
    assert_eq!(
        (invalid.border, invalid.ring, invalid.shadow),
        (invalid_border(tokens), None, ShadowLayers::NONE)
    );
    // A checked box keeps its `primary` fill and gets the destructive edge.
    for on in [VisualState::CHECKED, VisualState::INDETERMINATE] {
        let checked_invalid = get(tokens, query(on.with(VisualState::INVALID)));
        assert_eq!(
            (
                bg(&checked_invalid),
                checked_invalid.border,
                checked_invalid.ring,
                checked_invalid.shadow
            ),
            (
                c(ColorRole::Primary),
                invalid_border(tokens),
                None,
                ShadowLayers::NONE
            )
        );
    }
    // Focus stays visible on an invalid box.
    let focused_invalid = get(
        tokens,
        query(VisualState::FOCUS_VISIBLE.with(VisualState::INVALID)),
    );
    assert_eq!(
        (focused_invalid.border, focused_invalid.ring),
        (invalid_border(tokens), focus_ring(tokens))
    );
    let disabled = get(tokens, query(VisualState::DISABLED));
    assert_eq!(disabled.opacity, 0.5);
}

/// Checks a form label. `invalid` is the foreground of the invalid
/// label: `Destructive` for a choice, and `Foreground` for a label that
/// has no invalid state.
fn assert_label(tokens: &DesignTokens, query: StyleQuery, invalid: ColorRole) {
    let label = get(tokens, query);
    let mut text = tokens.text(TextSize::Sm, FontWeight::MEDIUM);
    text.line_height = text.size;
    assert_eq!(
        (label.foreground, label.text, label.gap, label.opacity),
        (tokens.color(ColorRole::Foreground), text, 8.0, 1.0)
    );
    let disabled = get(tokens, query.state(VisualState::DISABLED));
    assert_eq!(disabled.opacity, 0.5);
    let invalid_label = get(tokens, query.state(VisualState::INVALID));
    assert_eq!(invalid_label.foreground, tokens.color(invalid));
}

#[test]
fn date_picker_trigger_keeps_the_field_invalid_look() {
    for tokens in schemes() {
        let query = |state| StyleQuery::new(ControlKind::DatePicker, Slot::Root).state(state);
        let invalid = get(&tokens, query(VisualState::INVALID));
        assert_eq!(
            (invalid.border, invalid.ring),
            (invalid_border(&tokens), invalid_ring(&tokens))
        );
        let focused = get(
            &tokens,
            query(VisualState::INVALID.with(VisualState::FOCUS_VISIBLE)),
        );
        assert_eq!(
            (focused.border, focused.ring),
            (invalid_border(&tokens), focus_ring(&tokens))
        );
    }
}

#[test]
fn switch_has_no_invalid_look() {
    for tokens in schemes() {
        for slot in [Slot::Root, Slot::Thumb] {
            let query = |state| StyleQuery::new(ControlKind::Switch, slot).state(state);
            assert_eq!(
                get(&tokens, query(VisualState::INVALID)),
                get(&tokens, query(VisualState::NONE)),
                "{}",
                slot.name()
            );
        }
    }
}

#[test]
fn checkbox_matches_its_reference_table() {
    for tokens in schemes() {
        assert_choice_box(&tokens, ControlKind::Checkbox, Corners::all(4.0));
        let icon = get(&tokens, StyleQuery::new(ControlKind::Checkbox, Slot::Root));
        assert_eq!(icon.icon_size, 14.0);
        assert_label(
            &tokens,
            StyleQuery::new(ControlKind::Checkbox, Slot::Label),
            ColorRole::Destructive,
        );
    }
}

#[test]
fn radio_group_matches_its_reference_table() {
    for tokens in schemes() {
        assert_choice_box(
            &tokens,
            ControlKind::RadioGroup,
            radius(&tokens, RadiusScale::Full),
        );
        let query = |slot| StyleQuery::new(ControlKind::RadioGroup, slot);
        let dot = get(&tokens, query(Slot::Indicator));
        assert_eq!(
            (bg(&dot), dot.min_width, dot.min_height, dot.radius),
            (
                tokens.color(ColorRole::PrimaryForeground),
                8.0,
                8.0,
                radius(&tokens, RadiusScale::Full)
            )
        );
        assert_eq!(get(&tokens, query(Slot::Group)).gap, 12.0);
        assert_label(&tokens, query(Slot::Label), ColorRole::Destructive);
    }
}

#[test]
fn switch_matches_its_reference_table() {
    for tokens in schemes() {
        let dark = dark(&tokens);
        let c = |role| tokens.color(role);
        // (size, track width, track height, thumb)
        for (size, width, height, thumb) in [
            (ControlSize::Default, 32.0, 18.4, 16.0),
            (ControlSize::Sm, 24.0, 14.0, 12.0),
        ] {
            let query = |slot, state| {
                StyleQuery::new(ControlKind::Switch, slot)
                    .size(size)
                    .state(state)
            };
            let off = get(&tokens, query(Slot::Root, VisualState::NONE));
            let off_fill = c(ColorRole::Input).with_alpha(if dark { 0.8 } else { 1.0 });
            assert_eq!(
                (off.min_width, off.min_height, bg(&off)),
                (width, height, off_fill),
                "{}",
                size.name()
            );
            assert_eq!(
                (off.radius, off.shadow, off.border_width),
                (
                    radius(&tokens, RadiusScale::Full),
                    tokens.shadow(ShadowScale::Xs),
                    1.0
                )
            );
            let on = get(&tokens, query(Slot::Root, VisualState::CHECKED));
            assert_eq!(bg(&on), c(ColorRole::Primary));
            let focused = get(&tokens, query(Slot::Root, VisualState::FOCUS_VISIBLE));
            assert_eq!(focused.ring, focus_ring(&tokens));
            let disabled = get(&tokens, query(Slot::Root, VisualState::DISABLED));
            assert_eq!(disabled.opacity, 0.5);

            for (state, fill) in [
                (
                    VisualState::NONE,
                    if dark {
                        c(ColorRole::Foreground)
                    } else {
                        c(ColorRole::Background)
                    },
                ),
                (
                    VisualState::CHECKED,
                    if dark {
                        c(ColorRole::PrimaryForeground)
                    } else {
                        c(ColorRole::Background)
                    },
                ),
            ] {
                let knob = get(&tokens, query(Slot::Thumb, state));
                assert_eq!(
                    (knob.min_width, knob.min_height, bg(&knob), knob.radius),
                    (thumb, thumb, fill, radius(&tokens, RadiusScale::Full)),
                    "{} thumb {state}",
                    size.name()
                );
            }
        }
        // A switch has no invalid state, so its label does not change.
        assert_label(
            &tokens,
            StyleQuery::new(ControlKind::Switch, Slot::Label),
            ColorRole::Foreground,
        );
    }
}

/// The toggle look, which the toggle group items share.
fn assert_toggle(tokens: &DesignTokens, kind: ControlKind, slot: Slot) {
    let c = |role| tokens.color(role);
    let query = |variant: ToggleVariant, size, state| {
        StyleQuery::new(kind, slot)
            .variant(variant)
            .size(size)
            .state(state)
    };
    for (size, edge) in [
        (ControlSize::Sm, 32.0),
        (ControlSize::Default, 36.0),
        (ControlSize::Lg, 40.0),
    ] {
        let style = get(
            tokens,
            query(ToggleVariant::Default, size, VisualState::NONE),
        );
        assert_eq!(
            (style.min_height, style.min_width, style.padding, style.gap),
            (edge, edge, Edges::axes(10.0, 0.0), 4.0),
            "{} {}",
            kind.name(),
            size.name()
        );
        assert_eq!(
            (style.radius, style.text, style.border_width),
            (
                radius(tokens, RadiusScale::Md),
                tokens.text(TextSize::Sm, FontWeight::MEDIUM),
                1.0
            )
        );
    }
    for (variant, border, shadow) in [
        (
            ToggleVariant::Default,
            Color::TRANSPARENT,
            ShadowLayers::NONE,
        ),
        (
            ToggleVariant::Outline,
            c(ColorRole::Input),
            tokens.shadow(ShadowScale::Xs),
        ),
    ] {
        let look = |state| get(tokens, query(variant, ControlSize::Default, state));
        let rest = look(VisualState::NONE);
        assert_eq!(
            (bg(&rest), rest.border, rest.shadow),
            (Color::TRANSPARENT, border, shadow),
            "{}",
            variant.name()
        );
        let hover = look(VisualState::HOVERED);
        assert_eq!(
            (bg(&hover), hover.foreground),
            (c(ColorRole::Muted), c(ColorRole::Foreground))
        );
        assert_eq!(bg(&look(VisualState::CHECKED)), c(ColorRole::Muted));
        let focused = look(VisualState::FOCUS_VISIBLE);
        assert_eq!(
            (focused.border, focused.ring),
            (c(ColorRole::Ring), focus_ring(tokens))
        );
        // A toggle is not a field: the invalid flag changes nothing.
        assert_eq!(look(VisualState::INVALID), rest);
        let disabled = look(VisualState::HOVERED.with(VisualState::DISABLED));
        assert_eq!((bg(&disabled), disabled.opacity), (Color::TRANSPARENT, 0.5));
    }
}

#[test]
fn toggle_matches_its_reference_table() {
    for tokens in schemes() {
        assert_toggle(&tokens, ControlKind::Toggle, Slot::Root);
    }
}

#[test]
fn toggle_group_matches_its_reference_table() {
    for tokens in schemes() {
        assert_toggle(&tokens, ControlKind::ToggleGroup, Slot::Item);
        for (variant, shadow) in [
            (ToggleVariant::Default, ShadowLayers::NONE),
            (ToggleVariant::Outline, tokens.shadow(ShadowScale::Xs)),
        ] {
            let group = get(
                &tokens,
                StyleQuery::new(ControlKind::ToggleGroup, Slot::Root).variant(variant),
            );
            assert_eq!(
                (group.radius, group.shadow),
                (radius(&tokens, RadiusScale::Md), shadow),
                "{}",
                variant.name()
            );
        }
    }
}

#[test]
fn slider_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let full = radius(&tokens, RadiusScale::Full);
        let query = |slot, state| StyleQuery::new(ControlKind::Slider, slot).state(state);
        let root = get(&tokens, query(Slot::Root, VisualState::NONE));
        assert_eq!((root.min_height, root.min_width), (16.0, 160.0));

        let track = get(&tokens, query(Slot::Track, VisualState::NONE));
        assert_eq!(
            (bg(&track), track.radius, track.min_height, track.min_width),
            (c(ColorRole::Muted), full, 6.0, 6.0)
        );
        let range = get(&tokens, query(Slot::Range, VisualState::NONE));
        assert_eq!((bg(&range), range.radius), (c(ColorRole::Primary), full));

        let thumb = get(&tokens, query(Slot::Thumb, VisualState::NONE));
        assert_eq!(
            (bg(&thumb), thumb.border, thumb.border_width, thumb.radius),
            (Color::WHITE, c(ColorRole::Primary), 1.0, full)
        );
        assert_eq!(
            (thumb.min_width, thumb.min_height, thumb.shadow, thumb.ring),
            (16.0, 16.0, tokens.shadow(ShadowScale::Sm), None)
        );
        let ring = Some(Ring {
            width: 4.0,
            color: c(ColorRole::Ring).with_alpha(0.5),
        });
        for state in [VisualState::HOVERED, VisualState::FOCUS_VISIBLE] {
            assert_eq!(
                get(&tokens, query(Slot::Thumb, state)).ring,
                ring,
                "{state}"
            );
        }
        let disabled = VisualState::HOVERED.with(VisualState::DISABLED);
        let thumb = get(&tokens, query(Slot::Thumb, disabled));
        assert_eq!((thumb.ring, thumb.opacity), (None, 0.5));
        for slot in [Slot::Track, Slot::Range] {
            assert_eq!(get(&tokens, query(slot, disabled)).opacity, 0.5);
        }
    }
}

#[test]
fn label_matches_its_reference_table() {
    for tokens in schemes() {
        assert_label(
            &tokens,
            StyleQuery::new(ControlKind::Label, Slot::Root),
            ColorRole::Foreground,
        );
    }
}

#[test]
fn kbd_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let inverted_alpha = if dark(&tokens) { 0.1 } else { 0.2 };
        for (variant, fill, text) in [
            (
                KbdVariant::Default,
                c(ColorRole::Muted),
                c(ColorRole::MutedForeground),
            ),
            (
                KbdVariant::Inverted,
                c(ColorRole::Background).with_alpha(inverted_alpha),
                c(ColorRole::Background),
            ),
        ] {
            let cap = get(
                &tokens,
                StyleQuery::new(ControlKind::Kbd, Slot::Root).variant(variant),
            );
            assert_eq!(
                (bg(&cap), cap.foreground),
                (fill, text),
                "{}",
                variant.name()
            );
            assert_eq!(
                (
                    cap.min_height,
                    cap.min_width,
                    cap.padding,
                    cap.gap,
                    cap.radius,
                    cap.icon_size
                ),
                (
                    20.0,
                    20.0,
                    Edges::axes(4.0, 0.0),
                    4.0,
                    radius(&tokens, RadiusScale::Sm),
                    12.0
                )
            );
            assert_eq!(cap.text, tokens.text(TextSize::Xs, FontWeight::MEDIUM));
        }
        let group = get(&tokens, StyleQuery::new(ControlKind::Kbd, Slot::Group));
        assert_eq!(group.gap, 4.0);
    }
}
