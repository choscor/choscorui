//! Reference tables of the chat messages, bubbles, attachments, markers,
//! the message scroller, and questionnaires, as `docs/controls.md` pins
//! them.

use choscorui_controls::{
    AttachmentState, BubbleVariant, ControlKind, ControlSize, Corners, Cursor, Edges,
    MarkerVariant, ResolvedStyle, Ring, Sides, Slot, StyleQuery, Styles, VisualState,
};
use choscorui_core::{
    Color, ColorRole, ColorScheme, DesignTokens, FontWeight, RadiusScale, ShadowScale, TextSize,
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

/// The parts that a message and a message scroller share.
fn assert_message_parts(tokens: &DesignTokens, kind: ControlKind) {
    let query = |slot| StyleQuery::new(kind, slot);
    let root = get(tokens, query(Slot::Root));
    text(
        tokens,
        &root,
        ColorRole::Foreground,
        TextSize::Sm,
        FontWeight::NORMAL,
    );
    // (slot, gap): avatar to content, header to bubbles, bubble to bubble,
    // and message to message.
    for (slot, gap) in [
        (Slot::Root, 8.0),
        (Slot::Content, 10.0),
        (Slot::Group, 8.0),
        (Slot::Viewport, 32.0),
    ] {
        assert_eq!(
            get(tokens, query(slot)).gap,
            gap,
            "{} {}",
            kind.name(),
            slot.name()
        );
    }
    assert_eq!(get(tokens, query(Slot::Avatar)).min_width, 32.0);
    for slot in [Slot::Header, Slot::Footer] {
        let line = get(tokens, query(slot));
        text(
            tokens,
            &line,
            ColorRole::MutedForeground,
            TextSize::Xs,
            FontWeight::MEDIUM,
        );
        assert_eq!(line.padding, Edges::axes(12.0, 0.0), "{}", slot.name());
    }
}

#[test]
fn message_matches_its_reference_table() {
    for tokens in schemes() {
        assert_message_parts(&tokens, ControlKind::Message);
    }
}

#[test]
fn message_scroller_matches_its_reference_table() {
    for tokens in schemes() {
        assert_message_parts(&tokens, ControlKind::MessageScroller);
    }
}

/// `primary` at a new OKLCH lightness and a fraction of its chroma.
fn tint(primary: Color, lightness: f64, chroma: f64) -> Color {
    let (_, a, b) = primary.to_oklab();
    let hue = b.atan2(a).to_degrees();
    Color::from_oklch(lightness, a.hypot(b) * chroma, hue, 1.0)
}

#[test]
fn bubble_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let a = |role, alpha: f64| tokens.color(role).with_alpha(alpha);
        let primary = c(ColorRole::Primary);
        let mixed = |role| c(role).mix(c(ColorRole::Foreground), 0.05);
        let muted = c(ColorRole::Muted);
        // (variant, rest fill, text, hover fill)
        let variants = [
            (
                BubbleVariant::Default,
                primary,
                c(ColorRole::PrimaryForeground),
                a(ColorRole::Primary, 0.8),
            ),
            (
                BubbleVariant::Secondary,
                c(ColorRole::Secondary),
                c(ColorRole::SecondaryForeground),
                mixed(ColorRole::Secondary),
            ),
            (
                BubbleVariant::Muted,
                muted,
                c(ColorRole::Foreground),
                mixed(ColorRole::Muted),
            ),
            (
                BubbleVariant::Tinted,
                pick(&tokens, tint(primary, 0.93, 0.4), tint(primary, 0.3, 0.4)),
                c(ColorRole::Foreground),
                pick(&tokens, tint(primary, 0.88, 0.5), tint(primary, 0.35, 0.5)),
            ),
            (
                BubbleVariant::Outline,
                c(ColorRole::Background),
                c(ColorRole::Foreground),
                pick(&tokens, muted, a(ColorRole::Input, 0.3)),
            ),
            (
                BubbleVariant::Ghost,
                Color::TRANSPARENT,
                c(ColorRole::Foreground),
                pick(&tokens, muted, muted.with_alpha(0.5)),
            ),
            (
                BubbleVariant::Destructive,
                a(ColorRole::Destructive, pick(&tokens, 0.1, 0.2)),
                c(ColorRole::Destructive),
                a(ColorRole::Destructive, pick(&tokens, 0.2, 0.3)),
            ),
        ];
        for (variant, fill, ink, hover_fill) in variants {
            let query = |slot, state| {
                StyleQuery::new(ControlKind::Bubble, slot)
                    .variant(variant)
                    .state(state)
            };
            let name = variant.name();
            let bubble = get(&tokens, query(Slot::Content, VisualState::NONE));
            assert_eq!((bg(&bubble), bubble.foreground), (fill, ink), "{name}");
            let hover = get(&tokens, query(Slot::Content, VisualState::HOVERED));
            assert_eq!(bg(&hover), hover_fill, "{name} hovered");
            let focused = get(&tokens, query(Slot::Content, VisualState::FOCUS_VISIBLE));
            assert_eq!(focused.ring, focus_ring(&tokens), "{name} focused");

            let ghost = variant == BubbleVariant::Ghost;
            let (corner, padding, border_width) = if ghost {
                (Corners::ZERO, Edges::ZERO, 0.0)
            } else {
                (
                    radius(&tokens, RadiusScale::Xl),
                    Edges::axes(12.0, 8.0),
                    1.0,
                )
            };
            assert_eq!(
                (bubble.radius, bubble.padding, bubble.border_width),
                (corner, padding, border_width),
                "{name}"
            );
            let edge = if variant == BubbleVariant::Outline {
                c(ColorRole::Border)
            } else {
                Color::TRANSPARENT
            };
            assert_eq!(bubble.border, edge, "{name}");
            assert_eq!(bubble.text.line_height, bubble.text.size * 1.625);

            let root = get(&tokens, query(Slot::Root, VisualState::NONE));
            let width = if ghost { f64::INFINITY } else { 0.8 };
            assert_eq!((root.gap, root.max_width), (4.0, width), "{name}");
        }
        let query = |slot| StyleQuery::new(ControlKind::Bubble, slot);
        let reactions = get(&tokens, query(Slot::Badge));
        assert_eq!(
            (bg(&reactions), reactions.radius, reactions.padding),
            (
                muted,
                radius(&tokens, RadiusScale::Full),
                Edges::axes(6.0, 2.0)
            )
        );
        assert_eq!(
            (reactions.ring, reactions.gap),
            (
                Some(Ring {
                    width: 3.0,
                    color: c(ColorRole::Card),
                }),
                4.0
            )
        );
        assert_eq!(get(&tokens, query(Slot::Group)).gap, 8.0);
    }
}

#[test]
fn attachment_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        // (size, gap, padding, box radius, media, media radius, media icon,
        // name text)
        for (size, gap, padding, corner, media, media_corner, icon, name) in [
            (
                ControlSize::Default,
                8.0,
                Edges::axes(10.0, 8.0),
                RadiusScale::Xl,
                40.0,
                RadiusScale::Lg,
                16.0,
                TextSize::Sm,
            ),
            (
                ControlSize::Sm,
                10.0,
                Edges::axes(8.0, 6.0),
                RadiusScale::Xl,
                32.0,
                RadiusScale::Lg,
                16.0,
                TextSize::Xs,
            ),
            (
                ControlSize::Xs,
                6.0,
                Edges::axes(6.0, 4.0),
                RadiusScale::Lg,
                28.0,
                RadiusScale::Md,
                14.0,
                TextSize::Xs,
            ),
        ] {
            let query = |slot, state| {
                StyleQuery::new(ControlKind::Attachment, slot)
                    .size(size)
                    .state(state)
            };
            let label = size.name();
            let frame = get(&tokens, query(Slot::Root, VisualState::NONE));
            assert_eq!(
                (frame.gap, frame.padding, frame.radius),
                (gap, padding, radius(&tokens, corner)),
                "{label}"
            );
            assert_eq!(
                (frame.border, frame.border_width, frame.min_width),
                (c(ColorRole::Border), 1.0, 160.0)
            );
            let vertical = get(&tokens, query(Slot::Root, VisualState::OPEN));
            assert_eq!(vertical.min_width, 96.0);

            let thumbnail = get(&tokens, query(Slot::Media, VisualState::NONE));
            assert_eq!(
                (
                    thumbnail.min_width,
                    thumbnail.min_height,
                    thumbnail.radius,
                    thumbnail.icon_size
                ),
                (media, media, radius(&tokens, media_corner), icon),
                "{label}"
            );
            assert_eq!(
                (bg(&thumbnail), thumbnail.foreground),
                (c(ColorRole::Muted), c(ColorRole::Foreground))
            );
            let vertical = get(&tokens, query(Slot::Media, VisualState::OPEN));
            assert_eq!(vertical.icon_size, 24.0);

            let title = get(&tokens, query(Slot::Title, VisualState::NONE));
            let mut spec = tokens.text(name, FontWeight::MEDIUM);
            spec.line_height = spec.size * 1.25;
            assert_eq!(title.text, spec, "{label}");
        }

        let query = |state: AttachmentState, slot, visual| {
            StyleQuery::new(ControlKind::Attachment, slot)
                .variant(state)
                .state(visual)
        };
        // An image (`SELECTED`) dims while it uploads or processes.
        for (state, opacity) in [
            (AttachmentState::Idle, 1.0),
            (AttachmentState::Uploading, 0.6),
            (AttachmentState::Processing, 0.6),
            (AttachmentState::Error, 0.6),
            (AttachmentState::Done, 1.0),
        ] {
            let image = get(&tokens, query(state, Slot::Media, VisualState::SELECTED));
            assert_eq!(image.opacity, opacity, "{}", state.name());
            let icon = get(&tokens, query(state, Slot::Media, VisualState::NONE));
            assert_eq!(icon.opacity, 1.0);
        }
        let error = get(
            &tokens,
            query(AttachmentState::Error, Slot::Root, VisualState::NONE),
        );
        assert_eq!(error.border, c(ColorRole::Destructive));
        let detail = get(
            &tokens,
            query(AttachmentState::Idle, Slot::Description, VisualState::NONE),
        );
        text(
            &tokens,
            &detail,
            ColorRole::MutedForeground,
            TextSize::Xs,
            FontWeight::NORMAL,
        );
        assert_eq!(detail.padding.block_start, 2.0);
        let failed = get(
            &tokens,
            query(AttachmentState::Error, Slot::Description, VisualState::NONE),
        );
        assert_eq!(failed.foreground, c(ColorRole::Destructive));
        let bar = get(
            &tokens,
            query(
                AttachmentState::Uploading,
                Slot::Progress,
                VisualState::NONE,
            ),
        );
        assert_eq!((bg(&bar), bar.min_height), (c(ColorRole::Primary), 2.0));
        // The remove button shows the one focus ring, not the box.
        let focus = VisualState::FOCUSED.with(VisualState::FOCUS_VISIBLE);
        let focused = get(&tokens, query(AttachmentState::Idle, Slot::Root, focus));
        assert_eq!(focused.ring, None);
        let disabled = get(
            &tokens,
            query(AttachmentState::Idle, Slot::Root, VisualState::DISABLED),
        );
        assert_eq!(disabled.opacity, 0.5);
    }
}

#[test]
fn marker_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |variant: MarkerVariant, slot| {
            StyleQuery::new(ControlKind::Marker, slot).variant(variant)
        };
        for variant in MarkerVariant::ALL {
            let marker = get(&tokens, query(*variant, Slot::Root));
            text(
                &tokens,
                &marker,
                ColorRole::MutedForeground,
                TextSize::Sm,
                FontWeight::NORMAL,
            );
            assert_eq!(
                (marker.gap, marker.min_height, marker.icon_size),
                (8.0, 16.0, 16.0),
                "{}",
                variant.name()
            );
            let border = *variant == MarkerVariant::Border;
            assert_eq!(
                (marker.border_width, marker.padding.block_end),
                if border { (1.0, 8.0) } else { (0.0, 0.0) },
                "{}",
                variant.name()
            );
        }
        let border = get(&tokens, query(MarkerVariant::Border, Slot::Root));
        assert_eq!(
            (border.border, border.border_sides),
            (c(ColorRole::Border), Sides::BOTTOM)
        );
        let link = get(
            &tokens,
            query(MarkerVariant::Default, Slot::Root)
                .state(VisualState::HOVERED.with(VisualState::HIGHLIGHTED)),
        );
        assert_eq!(link.foreground, c(ColorRole::Foreground));
        let icon = get(&tokens, query(MarkerVariant::Default, Slot::Icon));
        assert_eq!(
            (icon.foreground, icon.icon_size),
            (c(ColorRole::MutedForeground), 16.0)
        );
        let line = get(&tokens, query(MarkerVariant::Separator, Slot::Separator));
        assert_eq!(
            (bg(&line), line.min_height, line.padding),
            (c(ColorRole::Border), 1.0, Edges::axes(4.0, 0.0))
        );
    }
}

#[test]
fn questionnaire_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot, state| StyleQuery::new(ControlKind::Questionnaire, slot).state(state);
        let rest = |slot| get(&tokens, query(slot, VisualState::NONE));
        // (slot, gap): buttons, title, choices, and button row.
        for (slot, gap) in [
            (Slot::Root, 24.0),
            (Slot::Item, 20.0),
            (Slot::Group, 12.0),
            (Slot::Action, 8.0),
        ] {
            assert_eq!(rest(slot).gap, gap, "{}", slot.name());
        }
        assert_eq!(rest(Slot::Action).min_height, 36.0);
        let progress = rest(Slot::Progress);
        text(
            &tokens,
            &progress,
            ColorRole::MutedForeground,
            TextSize::Xs,
            FontWeight::NORMAL,
        );
        let title = rest(Slot::Title);
        text(
            &tokens,
            &title,
            ColorRole::Foreground,
            TextSize::Base,
            FontWeight::SEMIBOLD,
        );
        assert_eq!(title.padding.block_end, 20.0);

        let choice = rest(Slot::Choice);
        text(
            &tokens,
            &choice,
            ColorRole::Foreground,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
        let fill = pick(
            &tokens,
            Color::TRANSPARENT,
            c(ColorRole::Input).with_alpha(0.2),
        );
        assert_eq!(
            (choice.border, choice.border_width, bg(&choice)),
            (c(ColorRole::Input), 1.0, fill)
        );
        assert_eq!(
            (choice.radius, choice.padding, choice.gap, choice.shadow),
            (
                radius(&tokens, RadiusScale::Md),
                Edges::axes(16.0, 14.0),
                12.0,
                tokens.shadow(ShadowScale::Xs)
            )
        );
        let pick_state = |state| get(&tokens, query(Slot::Choice, state));
        assert_eq!(
            bg(&pick_state(VisualState::HOVERED)),
            c(ColorRole::Muted).with_alpha(0.5)
        );
        let checked = pick_state(VisualState::CHECKED);
        assert_eq!(
            (bg(&checked), checked.border),
            (c(ColorRole::Muted), c(ColorRole::Primary).with_alpha(0.4))
        );
        assert_eq!(
            pick_state(VisualState::INVALID).border,
            c(ColorRole::Destructive)
        );
        let focused = pick_state(VisualState::FOCUS_VISIBLE);
        assert_eq!(
            (focused.border, focused.ring),
            (c(ColorRole::Ring), focus_ring(&tokens))
        );
        assert_eq!(pick_state(VisualState::DISABLED).opacity, 0.5);

        let check = rest(Slot::Indicator);
        assert_eq!(
            (
                check.min_width,
                check.min_height,
                check.radius,
                check.icon_size
            ),
            (16.0, 16.0, Corners::all(4.0), 14.0)
        );
        assert_eq!(check.border, c(ColorRole::Input));
        let on = get(&tokens, query(Slot::Indicator, VisualState::CHECKED));
        assert_eq!(
            (bg(&on), on.border),
            (c(ColorRole::Primary), c(ColorRole::Primary))
        );

        let key = rest(Slot::Shortcut);
        assert_eq!(
            (
                key.min_width,
                key.min_height,
                key.text.size,
                key.text.family
            ),
            (20.0, 20.0, 10.0, tokens.font_mono)
        );
        assert_eq!(
            (key.border, bg(&key), key.foreground),
            (
                c(ColorRole::Input),
                c(ColorRole::Background),
                c(ColorRole::MutedForeground)
            )
        );

        let answer = rest(Slot::Input);
        assert_eq!(
            (answer.min_height, answer.border, answer.cursor),
            (36.0, c(ColorRole::Input), Cursor::Text)
        );
        let invalid = get(&tokens, query(Slot::Input, VisualState::INVALID));
        assert_eq!(invalid.border, c(ColorRole::Destructive));
        let error = rest(Slot::Error);
        text(
            &tokens,
            &error,
            ColorRole::Destructive,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
    }
}
