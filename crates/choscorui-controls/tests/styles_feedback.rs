//! Reference tables of the alerts, toasts, progress bars, skeletons,
//! spinners, and empty states, as `docs/controls.md` pins them.

use choscorui_controls::{
    AlertVariant, ControlKind, Corners, Edges, EmptyMediaVariant, ResolvedStyle, Ring, Slot,
    StyleQuery, Styles, ToastVariant, VisualState,
};
use choscorui_core::{
    Color, ColorRole, DesignTokens, FontWeight, RadiusScale, ShadowScale, TextSize, Tracking,
};

/// The color of a solid background.
fn bg(style: &ResolvedStyle) -> Color {
    style.background.solid_color().expect("a solid background")
}

fn get(tokens: &DesignTokens, query: StyleQuery) -> ResolvedStyle {
    Styles::new().resolve(tokens, query)
}

fn schemes() -> [DesignTokens; 2] {
    [DesignTokens::light(), DesignTokens::dark()]
}

fn radius(tokens: &DesignTokens, step: RadiusScale) -> Corners {
    Corners::all(tokens.radius(step))
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
fn alert_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        // (variant, text, description)
        for (variant, tone, detail) in [
            (
                AlertVariant::Default,
                ColorRole::CardForeground,
                c(ColorRole::MutedForeground),
            ),
            (
                AlertVariant::Destructive,
                ColorRole::Destructive,
                c(ColorRole::Destructive).with_alpha(0.9),
            ),
        ] {
            let query = |slot| StyleQuery::new(ControlKind::Alert, slot).variant(variant);
            let name = variant.name();
            let alert = get(&tokens, query(Slot::Root));
            assert_eq!(
                (bg(&alert), alert.foreground, alert.border),
                (c(ColorRole::Card), c(tone), c(ColorRole::Border)),
                "{name}"
            );
            assert_eq!(
                (alert.border_width, alert.radius, alert.padding, alert.gap),
                (
                    1.0,
                    radius(&tokens, RadiusScale::Lg),
                    Edges::axes(16.0, 12.0),
                    2.0
                )
            );
            let title = get(&tokens, query(Slot::Title));
            text(&tokens, &title, tone, TextSize::Sm, FontWeight::MEDIUM);
            let description = get(&tokens, query(Slot::Description));
            assert_eq!(
                (description.foreground, description.text),
                (detail, tokens.text(TextSize::Sm, FontWeight::NORMAL)),
                "{name}"
            );
            let icon = get(&tokens, query(Slot::Icon));
            assert_eq!(
                (icon.foreground, icon.icon_size, icon.offset_y),
                (c(tone), 16.0, 2.0)
            );
            let action = get(&tokens, query(Slot::Action));
            assert_eq!(
                action.padding,
                Edges {
                    block_start: 10.0,
                    inline_end: 12.0,
                    ..Edges::ZERO
                }
            );
        }
    }
}

#[test]
fn toast_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::Toast, slot);
        let toast = get(&tokens, query(Slot::Root));
        assert_eq!(
            (bg(&toast), toast.foreground, toast.border),
            (
                c(ColorRole::Popover),
                c(ColorRole::PopoverForeground),
                c(ColorRole::Border)
            )
        );
        assert_eq!(
            (toast.border_width, toast.radius, toast.shadow),
            (
                1.0,
                radius(&tokens, RadiusScale::Xl2),
                tokens.shadow(ShadowScale::Lg)
            )
        );
        assert_eq!(
            (toast.padding, toast.gap, toast.max_width, toast.motion_ms),
            (Edges::all(16.0), 12.0, 384.0, 500)
        );
        // Every variant shares the box; only the icon differs.
        for variant in ToastVariant::ALL {
            assert_eq!(
                get(&tokens, query(Slot::Root).variant(*variant)),
                toast,
                "{}",
                variant.name()
            );
        }
        let focused = get(&tokens, query(Slot::Root).state(VisualState::FOCUS_VISIBLE));
        assert_eq!(
            (focused.border, focused.ring),
            (
                c(ColorRole::Ring),
                Some(Ring {
                    width: 3.0,
                    color: c(ColorRole::Ring).with_alpha(0.5),
                })
            )
        );

        let stack = get(&tokens, query(Slot::Group));
        assert_eq!(
            (stack.padding, stack.gap, stack.max_width, stack.motion_ms),
            (Edges::all(16.0), 12.0, 384.0, 500)
        );
        let title = get(&tokens, query(Slot::Title));
        text(
            &tokens,
            &title,
            ColorRole::PopoverForeground,
            TextSize::Sm,
            FontWeight::MEDIUM,
        );
        let detail = get(&tokens, query(Slot::Description));
        text(
            &tokens,
            &detail,
            ColorRole::MutedForeground,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
        let icon = get(&tokens, query(Slot::Icon));
        assert_eq!(
            (icon.foreground, icon.icon_size),
            (c(ColorRole::PopoverForeground), 16.0)
        );
        let close = get(&tokens, query(Slot::Close));
        assert_eq!(
            (close.foreground, close.icon_size),
            (c(ColorRole::MutedForeground), 16.0)
        );
        let hover = get(&tokens, query(Slot::Close).state(VisualState::HOVERED));
        assert_eq!(hover.foreground, c(ColorRole::Foreground));
    }
}

#[test]
fn progress_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::Progress, slot);
        let track = get(&tokens, query(Slot::Track));
        assert_eq!(
            (bg(&track), track.radius, track.min_height),
            (c(ColorRole::Muted), radius(&tokens, RadiusScale::Full), 6.0)
        );
        for slot in [Slot::Indicator, Slot::Range] {
            let fill = get(&tokens, query(slot));
            assert_eq!(
                (bg(&fill), fill.motion_ms),
                (c(ColorRole::Primary), 150),
                "{}",
                slot.name()
            );
        }
    }
}

#[test]
fn skeleton_matches_its_reference_table() {
    for tokens in schemes() {
        let block = get(&tokens, StyleQuery::new(ControlKind::Skeleton, Slot::Root));
        assert_eq!(
            (bg(&block), block.radius, block.motion_ms),
            (
                tokens.color(ColorRole::Muted),
                radius(&tokens, RadiusScale::Md),
                2000
            )
        );
    }
}

#[test]
fn spinner_matches_its_reference_table() {
    for tokens in schemes() {
        let spinner = get(&tokens, StyleQuery::new(ControlKind::Spinner, Slot::Root));
        assert_eq!((spinner.icon_size, spinner.motion_ms), (16.0, 1000));
    }
}

#[test]
fn empty_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::Empty, slot);
        let lg = radius(&tokens, RadiusScale::Lg);
        let root = get(&tokens, query(Slot::Root));
        assert_eq!((root.padding, root.radius), (Edges::all(48.0), lg));
        let header = get(&tokens, query(Slot::Header));
        assert_eq!((header.gap, header.max_width), (8.0, 384.0));

        let media = get(&tokens, query(Slot::Media));
        assert_eq!(
            (bg(&media), media.padding.block_end),
            (Color::TRANSPARENT, 8.0)
        );
        let icon = get(&tokens, query(Slot::Media).variant(EmptyMediaVariant::Icon));
        assert_eq!(
            (bg(&icon), icon.foreground, icon.radius),
            (c(ColorRole::Muted), c(ColorRole::Foreground), lg)
        );
        assert_eq!(
            (
                icon.min_width,
                icon.min_height,
                icon.icon_size,
                icon.padding.block_end
            ),
            (40.0, 40.0, 24.0, 8.0)
        );

        let title = get(&tokens, query(Slot::Title));
        assert_eq!(
            title.text,
            tokens
                .text(TextSize::Lg, FontWeight::MEDIUM)
                .with_tracking(Tracking::Tight)
        );
        let detail = get(&tokens, query(Slot::Description));
        let mut spec = tokens.text(TextSize::Sm, FontWeight::NORMAL);
        spec.line_height = spec.size * 1.625;
        assert_eq!(
            (detail.foreground, detail.text),
            (c(ColorRole::MutedForeground), spec)
        );
        let content = get(&tokens, query(Slot::Content));
        assert_eq!((content.gap, content.max_width), (16.0, 384.0));
    }
}
