//! Reference tables of the tables, list views, cards, badges, avatars,
//! charts, calendars, carousels, and gauges, as `docs/controls.md` pins
//! them.

use choscorui_controls::{
    BadgeVariant, CalendarDay, ControlKind, ControlSize, Corners, Edges, ResolvedStyle, Ring,
    Sides, Slot, StyleQuery, Styles, VisualState,
};
use choscorui_core::{
    Color, ColorRole, ColorScheme, DesignTokens, FontWeight, RadiusScale, ShadowLayers,
    ShadowScale, TextSize,
};

fn get(tokens: &DesignTokens, query: StyleQuery) -> ResolvedStyle {
    Styles::new().resolve(tokens, query)
}

fn schemes() -> [DesignTokens; 2] {
    [DesignTokens::light(), DesignTokens::dark()]
}

/// The light-scheme or the dark-scheme value.
fn pick<T>(tokens: &DesignTokens, light: T, dark: T) -> T {
    if tokens.scheme == ColorScheme::Dark {
        dark
    } else {
        light
    }
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
    Some(Ring {
        width: 3.0,
        color: tokens
            .color(ColorRole::Destructive)
            .with_alpha(pick(tokens, 0.2, 0.4)),
    })
}

fn text(
    tokens: &DesignTokens,
    style: &ResolvedStyle,
    role: ColorRole,
    size: TextSize,
    weight: FontWeight,
) {
    assert_eq!(
        (style.foreground, style.text),
        (tokens.color(role), tokens.text(size, weight))
    );
}

#[test]
fn table_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot, state| StyleQuery::new(ControlKind::Table, slot).state(state);
        let row = get(&tokens, query(Slot::Row, VisualState::NONE));
        assert_eq!(
            (
                row.border,
                row.border_width,
                row.border_sides,
                row.motion_ms
            ),
            (c(ColorRole::Border), 1.0, Sides::BOTTOM, 150)
        );
        let muted = c(ColorRole::Muted);
        for (state, fill) in [
            (VisualState::HOVERED, muted.with_alpha(0.5)),
            (VisualState::SELECTED, muted),
            (VisualState::HIGHLIGHTED, Color::TRANSPARENT),
        ] {
            let lit = get(&tokens, query(Slot::Row, state));
            assert_eq!(lit.background, fill, "{state}");
        }
        let header_cell = get(&tokens, query(Slot::HeaderCell, VisualState::NONE));
        text(
            &tokens,
            &header_cell,
            ColorRole::Foreground,
            TextSize::Sm,
            FontWeight::MEDIUM,
        );
        assert_eq!(
            (header_cell.min_height, header_cell.padding),
            (40.0, Edges::axes(8.0, 0.0))
        );
        let hover = get(&tokens, query(Slot::HeaderCell, VisualState::HOVERED));
        assert_eq!(hover.background, muted.with_alpha(0.5));
        let header = get(&tokens, query(Slot::Header, VisualState::NONE));
        assert_eq!(
            (header.border, header.border_sides),
            (c(ColorRole::Border), Sides::BOTTOM)
        );
        let cell = get(&tokens, query(Slot::Cell, VisualState::NONE));
        text(
            &tokens,
            &cell,
            ColorRole::Foreground,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
        assert_eq!(cell.padding, Edges::all(8.0));
        let footer = get(&tokens, query(Slot::Footer, VisualState::NONE));
        text(
            &tokens,
            &footer,
            ColorRole::Foreground,
            TextSize::Sm,
            FontWeight::MEDIUM,
        );
        assert_eq!(
            (footer.background, footer.border_sides),
            (muted.with_alpha(0.5), Sides::TOP)
        );
        let caption = get(&tokens, query(Slot::Caption, VisualState::NONE));
        text(
            &tokens,
            &caption,
            ColorRole::MutedForeground,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
        assert_eq!(caption.padding.block_start, 16.0);
        let frame = get(&tokens, query(Slot::Group, VisualState::NONE));
        assert_eq!(
            (frame.border, frame.border_width, frame.radius),
            (c(ColorRole::Border), 1.0, radius(&tokens, RadiusScale::Md))
        );
        let chevron = get(&tokens, query(Slot::Indicator, VisualState::NONE));
        assert_eq!(
            (chevron.foreground, chevron.icon_size),
            (c(ColorRole::MutedForeground), 16.0)
        );
        // The focused cell rings while focus is visible.
        let focused = get(&tokens, query(Slot::Root, VisualState::FOCUS_VISIBLE));
        assert_eq!(
            (focused.ring, focused.border, focused.radius),
            (
                focus_ring(&tokens),
                Color::TRANSPARENT,
                radius(&tokens, RadiusScale::Md)
            )
        );
    }
}

#[test]
fn list_view_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot, state| StyleQuery::new(ControlKind::ListView, slot).state(state);
        let row = get(&tokens, query(Slot::Row, VisualState::NONE));
        assert_eq!(
            (row.border_sides, row.radius, row.padding, row.min_height),
            (
                Sides::NONE,
                radius(&tokens, RadiusScale::Sm),
                Edges::axes(8.0, 0.0),
                32.0
            )
        );
        let selected = get(&tokens, query(Slot::Row, VisualState::SELECTED));
        assert_eq!(selected.background, c(ColorRole::Muted));
        // Selected in a focused view.
        let lit = get(
            &tokens,
            query(
                Slot::Row,
                VisualState::SELECTED.with(VisualState::HIGHLIGHTED),
            ),
        );
        assert_eq!(
            (lit.background, lit.foreground),
            (c(ColorRole::Accent), c(ColorRole::AccentForeground))
        );
        let cell = get(&tokens, query(Slot::Cell, VisualState::NONE));
        assert_eq!(cell.padding, Edges::all(8.0));
        let chevron = get(&tokens, query(Slot::Indicator, VisualState::NONE));
        assert_eq!(
            (chevron.foreground, chevron.icon_size),
            (c(ColorRole::MutedForeground), 16.0)
        );
        let focused = get(&tokens, query(Slot::Root, VisualState::FOCUS_VISIBLE));
        assert_eq!(focused.ring, focus_ring(&tokens));
    }
}

#[test]
fn card_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        for (size, spacing, title) in [
            (ControlSize::Default, 24.0, TextSize::Base),
            (ControlSize::Sm, 16.0, TextSize::Sm),
        ] {
            let query = |slot| StyleQuery::new(ControlKind::Card, slot).size(size);
            let surface = get(&tokens, query(Slot::Root));
            assert_eq!(
                (surface.background, surface.foreground),
                (c(ColorRole::Card), c(ColorRole::CardForeground))
            );
            assert_eq!(
                surface.ring,
                Some(Ring {
                    width: 1.0,
                    color: c(ColorRole::Foreground).with_alpha(0.1),
                })
            );
            assert_eq!(
                (surface.radius, surface.shadow, surface.gap, surface.padding),
                (
                    radius(&tokens, RadiusScale::Xl),
                    tokens.shadow(ShadowScale::Xs),
                    spacing,
                    Edges::axes(0.0, spacing)
                ),
                "{}",
                size.name()
            );
            let header = get(&tokens, query(Slot::Header));
            assert_eq!(
                (header.gap, header.padding),
                (4.0, Edges::axes(spacing, 0.0))
            );
            let heading = get(&tokens, query(Slot::Title));
            let mut spec = tokens.text(title, FontWeight::MEDIUM);
            spec.line_height = spec.size * 1.5;
            assert_eq!(
                (heading.foreground, heading.text),
                (c(ColorRole::CardForeground), spec)
            );
            let detail = get(&tokens, query(Slot::Description));
            text(
                &tokens,
                &detail,
                ColorRole::MutedForeground,
                TextSize::Sm,
                FontWeight::NORMAL,
            );
            for slot in [Slot::Content, Slot::Footer] {
                let part = get(&tokens, query(slot));
                assert_eq!(
                    (part.foreground, part.padding, part.gap),
                    (
                        c(ColorRole::CardForeground),
                        Edges::axes(spacing, 0.0),
                        12.0
                    ),
                    "{}",
                    slot.name()
                );
            }
        }
    }
}

#[test]
fn badge_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let a = |role, alpha: f64| tokens.color(role).with_alpha(alpha);
        let clear = Color::TRANSPARENT;
        let muted = c(ColorRole::Muted);
        // (variant, rest fill, rest text, hover fill, hover text)
        let variants = [
            (
                BadgeVariant::Default,
                c(ColorRole::Primary),
                c(ColorRole::PrimaryForeground),
                a(ColorRole::Primary, 0.8),
                c(ColorRole::PrimaryForeground),
            ),
            (
                BadgeVariant::Secondary,
                c(ColorRole::Secondary),
                c(ColorRole::SecondaryForeground),
                a(ColorRole::Secondary, 0.8),
                c(ColorRole::SecondaryForeground),
            ),
            (
                BadgeVariant::Destructive,
                a(ColorRole::Destructive, pick(&tokens, 0.1, 0.2)),
                c(ColorRole::Destructive),
                a(ColorRole::Destructive, pick(&tokens, 0.2, 0.3)),
                c(ColorRole::Destructive),
            ),
            (
                BadgeVariant::Outline,
                clear,
                c(ColorRole::Foreground),
                muted,
                c(ColorRole::MutedForeground),
            ),
            (
                BadgeVariant::Ghost,
                clear,
                c(ColorRole::Foreground),
                pick(&tokens, muted, muted.with_alpha(0.5)),
                c(ColorRole::MutedForeground),
            ),
            (
                BadgeVariant::Link,
                clear,
                c(ColorRole::Primary),
                clear,
                c(ColorRole::Primary),
            ),
        ];
        for (variant, fill, ink, hover_fill, hover_ink) in variants {
            let look = |state| {
                get(
                    &tokens,
                    StyleQuery::new(ControlKind::Badge, Slot::Root)
                        .variant(variant)
                        .state(state),
                )
            };
            let name = variant.name();
            let rest = look(VisualState::NONE);
            assert_eq!((rest.background, rest.foreground), (fill, ink), "{name}");
            assert_eq!(
                (
                    rest.min_height,
                    rest.radius,
                    rest.padding,
                    rest.gap,
                    rest.icon_size
                ),
                (
                    20.0,
                    radius(&tokens, RadiusScale::Xl4),
                    Edges::axes(8.0, 2.0),
                    4.0,
                    12.0
                )
            );
            assert_eq!(
                (rest.text, rest.border_width),
                (tokens.text(TextSize::Xs, FontWeight::MEDIUM), 1.0)
            );
            let edge = if variant == BadgeVariant::Outline {
                c(ColorRole::Border)
            } else {
                clear
            };
            assert_eq!(rest.border, edge, "{name}");
            let hover = look(VisualState::HOVERED);
            assert_eq!(
                (hover.background, hover.foreground),
                (hover_fill, hover_ink),
                "{name} hovered"
            );
            assert_eq!(hover.underline, variant == BadgeVariant::Link);
            let focused = look(VisualState::FOCUS_VISIBLE);
            let ring = if variant == BadgeVariant::Destructive {
                invalid_ring(&tokens)
            } else {
                focus_ring(&tokens)
            };
            assert_eq!(
                (focused.border, focused.ring),
                (c(ColorRole::Ring), ring),
                "{name} focused"
            );
            let invalid = look(VisualState::INVALID);
            assert_eq!(invalid.ring, invalid_ring(&tokens), "{name} invalid");
        }
    }
}

#[test]
fn avatar_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let full = radius(&tokens, RadiusScale::Full);
        let ring = Some(Ring {
            width: 2.0,
            color: c(ColorRole::Background),
        });
        // (size, edge, fallback text, fallback icon, badge, badge icon)
        for (size, edge, label, icon, badge, badge_icon) in [
            (ControlSize::Sm, 24.0, TextSize::Xs, 12.0, 8.0, 0.0),
            (ControlSize::Default, 32.0, TextSize::Sm, 16.0, 10.0, 8.0),
            (ControlSize::Lg, 40.0, TextSize::Sm, 20.0, 12.0, 8.0),
        ] {
            let query = |slot| StyleQuery::new(ControlKind::Avatar, slot).size(size);
            let name = size.name();
            let avatar = get(&tokens, query(Slot::Root));
            assert_eq!(
                (
                    avatar.min_width,
                    avatar.min_height,
                    avatar.radius,
                    avatar.ring,
                    avatar.gap
                ),
                (edge, edge, full, ring, -8.0),
                "{name}"
            );
            for slot in [Slot::Fallback, Slot::Group] {
                let fallback = get(&tokens, query(slot));
                text(
                    &tokens,
                    &fallback,
                    ColorRole::MutedForeground,
                    label,
                    FontWeight::NORMAL,
                );
                assert_eq!(
                    (
                        fallback.background,
                        fallback.min_width,
                        fallback.min_height,
                        fallback.icon_size,
                        fallback.radius
                    ),
                    (c(ColorRole::Muted), edge, edge, icon, full),
                    "{name} {}",
                    slot.name()
                );
            }
            let dot = get(&tokens, query(Slot::Badge));
            assert_eq!(
                (dot.background, dot.foreground, dot.ring),
                (c(ColorRole::Primary), c(ColorRole::PrimaryForeground), ring)
            );
            assert_eq!(
                (dot.min_width, dot.min_height, dot.icon_size),
                (badge, badge, badge_icon),
                "{name} badge"
            );
        }
    }
}

#[test]
fn chart_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::Chart, slot);
        let plot = get(&tokens, query(Slot::Root));
        text(
            &tokens,
            &plot,
            ColorRole::MutedForeground,
            TextSize::Xs,
            FontWeight::NORMAL,
        );
        assert_eq!(plot.min_height, 200.0);
        let grid = get(&tokens, query(Slot::Track));
        assert_eq!(
            (grid.background, grid.min_height),
            (c(ColorRole::Border), 1.0)
        );
        let mark = get(&tokens, query(Slot::Item));
        assert_eq!(
            (mark.border_width, mark.radius, mark.opacity),
            (2.0, radius(&tokens, RadiusScale::Sm), 0.4)
        );
        for slot in [Slot::Label, Slot::Legend] {
            let label = get(&tokens, query(slot));
            text(
                &tokens,
                &label,
                ColorRole::MutedForeground,
                TextSize::Xs,
                FontWeight::NORMAL,
            );
            assert_eq!(label.gap, 6.0, "{}", slot.name());
        }
        let tooltip = get(&tokens, query(Slot::Content));
        assert_eq!(
            (tooltip.background, tooltip.border, tooltip.border_width),
            (
                c(ColorRole::Background),
                c(ColorRole::Border).with_alpha(0.5),
                1.0
            )
        );
        assert_eq!(
            (
                tooltip.radius,
                tooltip.padding,
                tooltip.gap,
                tooltip.shadow,
                tooltip.text
            ),
            (
                radius(&tokens, RadiusScale::Lg),
                Edges::axes(10.0, 6.0),
                6.0,
                tokens.shadow(ShadowScale::Xl),
                tokens.text(TextSize::Xs, FontWeight::NORMAL)
            )
        );
        let value = get(&tokens, query(Slot::Value));
        text(
            &tokens,
            &value,
            ColorRole::Foreground,
            TextSize::Xs,
            FontWeight::MEDIUM,
        );
        let swatch = get(&tokens, query(Slot::Indicator));
        assert_eq!(
            (swatch.min_width, swatch.min_height, swatch.radius),
            (10.0, 10.0, Corners::all(2.0))
        );
    }
}

#[test]
fn calendar_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let md = radius(&tokens, RadiusScale::Md);
        let query = |slot| StyleQuery::new(ControlKind::Calendar, slot);
        let grid = get(&tokens, query(Slot::Root));
        assert_eq!(
            (grid.background, grid.padding, grid.gap, grid.min_width),
            (
                c(ColorRole::Background),
                Edges::all(12.0),
                16.0,
                32.0 * 7.0 + 24.0
            )
        );

        let day = |variant: CalendarDay, state| {
            get(&tokens, query(Slot::Day).variant(variant).state(state))
        };
        let rest = day(CalendarDay::Default, VisualState::NONE);
        let mut spec = tokens.text(TextSize::Sm, FontWeight::NORMAL);
        spec.line_height = spec.size;
        assert_eq!(
            (rest.min_width, rest.min_height, rest.radius, rest.text),
            (32.0, 32.0, md, spec)
        );
        assert_eq!(
            (rest.background, rest.foreground),
            (Color::TRANSPARENT, c(ColorRole::Foreground))
        );
        let muted = c(ColorRole::Muted);
        let hover = day(CalendarDay::Default, VisualState::HOVERED);
        assert_eq!(
            hover.background,
            pick(&tokens, muted, muted.with_alpha(0.5))
        );
        let pressed = day(CalendarDay::Default, VisualState::PRESSED);
        assert_eq!(pressed.offset_y, 0.0);
        let primary = (c(ColorRole::Primary), c(ColorRole::PrimaryForeground));
        // (role, fill and text, radius)
        for (variant, look, corner) in [
            (CalendarDay::Today, (muted, c(ColorRole::Foreground)), md),
            (
                CalendarDay::Outside,
                (Color::TRANSPARENT, c(ColorRole::MutedForeground)),
                md,
            ),
            (
                CalendarDay::RangeMiddle,
                (muted, c(ColorRole::Foreground)),
                Corners::ZERO,
            ),
            (CalendarDay::RangeStart, primary, md),
            (CalendarDay::RangeEnd, primary, md),
        ] {
            let style = day(variant, VisualState::NONE);
            assert_eq!(
                ((style.background, style.foreground), style.radius),
                (look, corner),
                "{}",
                variant.name()
            );
        }
        let selected = day(CalendarDay::Today, VisualState::SELECTED);
        assert_eq!((selected.background, selected.foreground), primary);
        let focused = day(CalendarDay::Default, VisualState::FOCUS_VISIBLE);
        assert_eq!(
            (focused.border, focused.ring),
            (c(ColorRole::Ring), focus_ring(&tokens))
        );
        let disabled = day(CalendarDay::Default, VisualState::DISABLED);
        assert_eq!(
            (disabled.foreground, disabled.opacity),
            (c(ColorRole::MutedForeground), 0.5)
        );

        let band = get(&tokens, query(Slot::Range));
        assert_eq!((band.background, band.radius), (muted, md));
        let weekday = get(&tokens, query(Slot::Weekday));
        assert_eq!(
            (weekday.foreground, weekday.text.size, weekday.min_height),
            (c(ColorRole::MutedForeground), 12.8, 32.0)
        );
        let caption = get(&tokens, query(Slot::Caption));
        text(
            &tokens,
            &caption,
            ColorRole::Foreground,
            TextSize::Sm,
            FontWeight::MEDIUM,
        );
        assert_eq!(
            (caption.min_height, caption.padding, caption.gap),
            (32.0, Edges::axes(32.0, 0.0), 6.0)
        );
        // The month and year dropdowns of the caption.
        let dropdown = get(&tokens, query(Slot::Trigger));
        assert_eq!(
            (dropdown.border, dropdown.background, dropdown.radius),
            (c(ColorRole::Input), Color::TRANSPARENT, md)
        );
        assert_eq!(
            (dropdown.min_height, dropdown.text),
            (32.0, tokens.text(TextSize::Sm, FontWeight::MEDIUM))
        );
        assert_eq!(
            dropdown.padding,
            Edges {
                inline_start: 8.0,
                inline_end: 4.0,
                ..Edges::ZERO
            }
        );
        let open = get(&tokens, query(Slot::Trigger).state(VisualState::FOCUSED));
        assert_eq!(
            (open.border, open.ring),
            (c(ColorRole::Ring), focus_ring(&tokens))
        );
        for slot in [Slot::Previous, Slot::Next] {
            let button = get(&tokens, query(slot));
            assert_eq!(
                (button.min_width, button.min_height, button.padding),
                (32.0, 32.0, Edges::ZERO),
                "{}",
                slot.name()
            );
        }
    }
}

#[test]
fn carousel_matches_its_reference_table() {
    for tokens in schemes() {
        let query = |slot| StyleQuery::new(ControlKind::Carousel, slot);
        let root = get(&tokens, query(Slot::Root));
        assert_eq!((root.gap, root.motion_ms), (16.0, 250));
        assert_eq!(get(&tokens, query(Slot::Item)).padding.inline_start, 16.0);
        for slot in [Slot::Previous, Slot::Next] {
            let button = get(&tokens, query(slot));
            assert_eq!(
                (button.min_width, button.min_height, button.radius),
                (32.0, 32.0, radius(&tokens, RadiusScale::Full)),
                "{}",
                slot.name()
            );
            assert_eq!(button.shadow, tokens.shadow(ShadowScale::Xs));
        }
    }
}

/// The track, the value range, and the thumb of a gauge.
fn assert_gauge_parts(tokens: &DesignTokens, kind: ControlKind) {
    let c = |role| tokens.color(role);
    let query = |slot, state| StyleQuery::new(kind, slot).state(state);
    let track = get(tokens, query(Slot::Track, VisualState::NONE));
    assert_eq!(
        (track.background, track.min_width),
        (c(ColorRole::Muted), 6.0),
        "{}",
        kind.name()
    );
    let range = get(tokens, query(Slot::Range, VisualState::NONE));
    assert_eq!(
        (range.background, range.min_width),
        (c(ColorRole::Primary), 6.0)
    );
    let thumb = get(tokens, query(Slot::Thumb, VisualState::NONE));
    assert_eq!(
        (thumb.background, thumb.border, thumb.border_width),
        (Color::WHITE, c(ColorRole::Primary), 1.0)
    );
    assert_eq!(
        (thumb.min_width, thumb.radius, thumb.shadow, thumb.ring),
        (
            16.0,
            radius(tokens, RadiusScale::Full),
            tokens.shadow(ShadowScale::Sm),
            None
        )
    );
    let ring = Some(Ring {
        width: 4.0,
        color: c(ColorRole::Ring).with_alpha(0.5),
    });
    for state in [VisualState::HOVERED, VisualState::FOCUS_VISIBLE] {
        assert_eq!(get(tokens, query(Slot::Thumb, state)).ring, ring, "{state}");
    }
    let value = get(tokens, query(Slot::Value, VisualState::NONE));
    assert_eq!(
        (value.foreground, value.text),
        (
            c(ColorRole::Foreground),
            tokens.mono(TextSize::Xl2, FontWeight::MEDIUM)
        )
    );
}

/// The input box that frames a numeric display or a color picker: radius
/// lg and 12 pt padding.
fn assert_framed_box(tokens: &DesignTokens, kind: ControlKind) {
    let c = |role| tokens.color(role);
    let query = |state| StyleQuery::new(kind, Slot::Root).state(state);
    let frame = get(tokens, query(VisualState::NONE));
    assert_eq!(
        (frame.border, frame.border_width, frame.shadow),
        (c(ColorRole::Input), 1.0, tokens.shadow(ShadowScale::Xs)),
        "{}",
        kind.name()
    );
    assert_eq!(
        (frame.radius, frame.padding),
        (radius(tokens, RadiusScale::Lg), Edges::all(12.0))
    );
    let focused = get(tokens, query(VisualState::FOCUS_VISIBLE));
    assert_eq!(
        (focused.border, focused.ring),
        (c(ColorRole::Ring), focus_ring(tokens))
    );
    let disabled = get(tokens, query(VisualState::DISABLED));
    assert_eq!(disabled.opacity, 0.5);
}

#[test]
fn dial_matches_its_reference_table() {
    for tokens in schemes() {
        assert_gauge_parts(&tokens, ControlKind::Dial);
        let query = |state| StyleQuery::new(ControlKind::Dial, Slot::Root).state(state);
        let face = get(&tokens, query(VisualState::NONE));
        assert_eq!(
            (face.background, face.border, face.shadow, face.radius),
            (
                Color::TRANSPARENT,
                Color::TRANSPARENT,
                ShadowLayers::NONE,
                radius(&tokens, RadiusScale::Full)
            )
        );
        let focused = get(&tokens, query(VisualState::FOCUS_VISIBLE));
        assert_eq!(focused.ring, focus_ring(&tokens));
    }
}

#[test]
fn numeric_display_matches_its_reference_table() {
    for tokens in schemes() {
        assert_framed_box(&tokens, ControlKind::NumericDisplay);
        assert_gauge_parts(&tokens, ControlKind::NumericDisplay);
    }
}

#[test]
fn color_picker_matches_its_reference_table() {
    for tokens in schemes() {
        assert_framed_box(&tokens, ControlKind::ColorPicker);
        assert_gauge_parts(&tokens, ControlKind::ColorPicker);
        let label = get(
            &tokens,
            StyleQuery::new(ControlKind::ColorPicker, Slot::Label),
        );
        text(
            &tokens,
            &label,
            ColorRole::MutedForeground,
            TextSize::Xs,
            FontWeight::NORMAL,
        );
    }
}
