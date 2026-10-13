//! Reference tables of the separators, resizable handles, scroll areas,
//! collapsibles, accordions, item rows, typography, and the repainted
//! editors, as `docs/controls.md` pins them.

use choscorui_controls::{
    ControlKind, ControlSize, Corners, Cursor, Edges, ItemVariant, ResolvedStyle, Ring, Sides,
    Slot, StyleQuery, Styles, Variant, VisualState,
};
use choscorui_core::{
    Color, ColorRole, ColorScheme, DesignTokens, FontWeight, RadiusScale, ShadowScale, TextPreset,
    TextSize,
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

#[test]
fn separator_matches_its_reference_table() {
    for tokens in schemes() {
        let line = get(&tokens, StyleQuery::new(ControlKind::Separator, Slot::Root));
        assert_eq!(
            (bg(&line), line.min_width, line.min_height),
            (tokens.color(ColorRole::Border), 1.0, 1.0)
        );
    }
}

/// The grip on a resizable handle and on a size grip.
fn assert_grip(tokens: &DesignTokens, kind: ControlKind) {
    for slot in [Slot::Icon, Slot::Thumb] {
        let grip = get(tokens, StyleQuery::new(kind, slot));
        assert_eq!(
            (bg(&grip), grip.radius, grip.min_width, grip.min_height),
            (
                tokens.color(ColorRole::Border),
                radius(tokens, RadiusScale::Lg),
                4.0,
                24.0
            ),
            "{} {}",
            kind.name(),
            slot.name()
        );
    }
}

#[test]
fn resizable_matches_its_reference_table() {
    for tokens in schemes() {
        for slot in [Slot::Root, Slot::Handle] {
            let query = StyleQuery::new(ControlKind::Resizable, slot);
            let handle = get(&tokens, query);
            assert_eq!(
                (bg(&handle), handle.min_width, handle.min_height),
                (tokens.color(ColorRole::Border), 1.0, 1.0),
                "{}",
                slot.name()
            );
            assert_eq!(
                (handle.padding, handle.cursor, handle.ring),
                (Edges::axes(2.0, 0.0), Cursor::ResizeHorizontal, None)
            );
            let focused = get(&tokens, query.state(VisualState::FOCUS_VISIBLE));
            assert_eq!(
                focused.ring,
                Some(Ring {
                    width: 1.0,
                    color: tokens.color(ColorRole::Ring),
                })
            );
        }
        assert_grip(&tokens, ControlKind::Resizable);
    }
}

#[test]
fn size_grip_matches_its_reference_table() {
    for tokens in schemes() {
        let grip = get(&tokens, StyleQuery::new(ControlKind::SizeGrip, Slot::Root));
        assert_eq!(
            (grip.foreground, grip.min_width, grip.min_height),
            (tokens.color(ColorRole::MutedForeground), 16.0, 16.0)
        );
        assert_grip(&tokens, ControlKind::SizeGrip);
    }
}

#[test]
fn scroll_area_matches_its_reference_table() {
    for tokens in schemes() {
        let query = |slot, state| StyleQuery::new(ControlKind::ScrollArea, slot).state(state);
        let area = get(&tokens, query(Slot::Root, VisualState::NONE));
        assert_eq!(area.ring, None);
        let focused = get(&tokens, query(Slot::Root, VisualState::FOCUS_VISIBLE));
        assert_eq!(focused.ring, focus_ring(&tokens));

        let bar = get(&tokens, query(Slot::Scrollbar, VisualState::NONE));
        assert_eq!(
            (bar.min_width, bar.min_height, bar.padding, bar.motion_ms),
            (10.0, 10.0, Edges::all(1.0), 150)
        );
        let thumb = get(&tokens, query(Slot::Thumb, VisualState::NONE));
        assert_eq!(
            (bg(&thumb), thumb.radius),
            (
                tokens.color(ColorRole::Border),
                radius(&tokens, RadiusScale::Full)
            )
        );
        let active = tokens.color(ColorRole::MutedForeground).with_alpha(0.5);
        for state in [VisualState::HOVERED, VisualState::PRESSED] {
            let thumb = get(&tokens, query(Slot::Thumb, state));
            assert_eq!(bg(&thumb), active, "{state}");
        }
    }
}

#[test]
fn aspect_ratio_matches_its_reference_table() {
    for tokens in schemes() {
        let frame = get(
            &tokens,
            StyleQuery::new(ControlKind::AspectRatio, Slot::Root),
        );
        assert_eq!(
            (bg(&frame), frame.radius),
            (
                tokens.color(ColorRole::Muted),
                radius(&tokens, RadiusScale::Lg)
            )
        );
    }
}

/// The disclosure look that an accordion and a collapsible share.
fn assert_disclosure(tokens: &DesignTokens, kind: ControlKind) {
    let query = |slot, state| StyleQuery::new(kind, slot).state(state);
    let item = get(tokens, query(Slot::Item, VisualState::NONE));
    assert_eq!(
        (item.border, item.border_width, item.border_sides),
        (tokens.color(ColorRole::Border), 1.0, Sides::BOTTOM),
        "{}",
        kind.name()
    );
    let header = get(tokens, query(Slot::Trigger, VisualState::NONE));
    text(
        tokens,
        &header,
        ColorRole::Foreground,
        TextSize::Sm,
        FontWeight::MEDIUM,
    );
    assert_eq!(
        (header.padding, header.radius, header.underline),
        (
            Edges::axes(0.0, 16.0),
            radius(tokens, RadiusScale::Md),
            false
        )
    );
    let hover = get(tokens, query(Slot::Trigger, VisualState::HOVERED));
    assert!(hover.underline);
    let focused = get(tokens, query(Slot::Trigger, VisualState::FOCUS_VISIBLE));
    assert_eq!(
        (focused.border, focused.ring),
        (tokens.color(ColorRole::Ring), focus_ring(tokens))
    );
    let disabled = get(tokens, query(Slot::Trigger, VisualState::DISABLED));
    assert_eq!(disabled.opacity, 0.5);
    let chevron = get(tokens, query(Slot::Icon, VisualState::NONE));
    assert_eq!(
        (chevron.foreground, chevron.icon_size),
        (tokens.color(ColorRole::MutedForeground), 16.0)
    );
    let content = get(tokens, query(Slot::Content, VisualState::NONE));
    text(
        tokens,
        &content,
        ColorRole::Foreground,
        TextSize::Sm,
        FontWeight::NORMAL,
    );
    assert_eq!((content.padding.block_end, content.motion_ms), (16.0, 200));
}

#[test]
fn accordion_matches_its_reference_table() {
    for tokens in schemes() {
        assert_disclosure(&tokens, ControlKind::Accordion);
    }
}

#[test]
fn collapsible_matches_its_reference_table() {
    for tokens in schemes() {
        assert_disclosure(&tokens, ControlKind::Collapsible);
    }
}

#[test]
fn item_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        // (size, gap, inline padding, block padding, image, group gap,
        // description text)
        for (size, gap, inline, block, image, group, detail) in [
            (
                ControlSize::Default,
                14.0,
                16.0,
                14.0,
                40.0,
                16.0,
                TextSize::Sm,
            ),
            (ControlSize::Sm, 10.0, 12.0, 10.0, 32.0, 10.0, TextSize::Sm),
            (ControlSize::Xs, 8.0, 10.0, 8.0, 24.0, 8.0, TextSize::Xs),
        ] {
            let query = |slot| StyleQuery::new(ControlKind::Item, slot).size(size);
            let name = size.name();
            let row = get(&tokens, query(Slot::Root));
            assert_eq!(
                (row.gap, row.padding, row.radius, row.border_width),
                (
                    gap,
                    Edges::axes(inline, block),
                    radius(&tokens, RadiusScale::Md),
                    1.0
                ),
                "{name}"
            );
            text(
                &tokens,
                &row,
                ColorRole::Foreground,
                TextSize::Sm,
                FontWeight::NORMAL,
            );
            assert_eq!(get(&tokens, query(Slot::Group)).gap, group, "{name}");
            // The image media variant.
            let media = get(&tokens, query(Slot::Media).variant(Variant(2)));
            assert_eq!(
                (media.min_width, media.min_height, media.radius),
                (image, image, radius(&tokens, RadiusScale::Sm)),
                "{name}"
            );
            let description = get(&tokens, query(Slot::Description));
            let mut spec = tokens.text(detail, FontWeight::NORMAL);
            spec.line_height = spec.size * 1.5;
            assert_eq!(
                (description.foreground, description.text),
                (c(ColorRole::MutedForeground), spec),
                "{name}"
            );
        }
        let query = |slot| StyleQuery::new(ControlKind::Item, slot);
        for (variant, fill, edge) in [
            (ItemVariant::Default, Color::TRANSPARENT, Color::TRANSPARENT),
            (
                ItemVariant::Outline,
                Color::TRANSPARENT,
                c(ColorRole::Border),
            ),
            (
                ItemVariant::Muted,
                c(ColorRole::Muted).with_alpha(0.5),
                Color::TRANSPARENT,
            ),
        ] {
            let row = get(&tokens, query(Slot::Root).variant(variant));
            assert_eq!((bg(&row), row.border), (fill, edge), "{}", variant.name());
        }
        // A link row (highlighted) fills `muted` on hover.
        let link = VisualState::HOVERED.with(VisualState::HIGHLIGHTED);
        assert_eq!(
            bg(&get(&tokens, query(Slot::Root).state(link))),
            c(ColorRole::Muted)
        );
        let plain = get(&tokens, query(Slot::Root).state(VisualState::HOVERED));
        assert_eq!(bg(&plain), Color::TRANSPARENT);
        let focused = get(&tokens, query(Slot::Root).state(VisualState::FOCUS_VISIBLE));
        assert_eq!(focused.ring, focus_ring(&tokens));

        let icon = get(&tokens, query(Slot::Media));
        assert_eq!((icon.icon_size, icon.gap), (16.0, 8.0));
        let title = get(&tokens, query(Slot::Title));
        let mut spec = tokens.text(TextSize::Sm, FontWeight::MEDIUM);
        spec.line_height = spec.size * 1.375;
        assert_eq!((title.text, title.gap), (spec, 8.0));
        let separator = get(&tokens, query(Slot::Separator));
        assert_eq!(
            (bg(&separator), separator.min_height, separator.padding),
            (c(ColorRole::Border), 1.0, Edges::axes(0.0, 8.0))
        );
    }
}

#[test]
fn typography_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        for (index, preset) in TextPreset::ALL.iter().enumerate() {
            let variant = Variant(u8::try_from(index).unwrap());
            let style = get(
                &tokens,
                StyleQuery::new(ControlKind::Typography, Slot::Root).variant(variant),
            );
            assert_eq!(
                (style.text, style.foreground),
                (tokens.preset(*preset), c(preset.color())),
                "{preset:?}"
            );
            // (border, border width, sides, padding, background, gap)
            let expected = match preset {
                TextPreset::H2 => (
                    c(ColorRole::Border),
                    1.0,
                    Sides::BOTTOM,
                    Edges {
                        block_end: 8.0,
                        ..Edges::ZERO
                    },
                    Color::TRANSPARENT,
                    0.0,
                ),
                TextPreset::Blockquote => (
                    c(ColorRole::Border),
                    2.0,
                    Sides::START,
                    Edges {
                        inline_start: 24.0,
                        ..Edges::ZERO
                    },
                    Color::TRANSPARENT,
                    0.0,
                ),
                TextPreset::InlineCode => (
                    Color::TRANSPARENT,
                    0.0,
                    Sides::ALL,
                    Edges::axes(4.8, 3.2),
                    c(ColorRole::Muted),
                    0.0,
                ),
                TextPreset::List => (
                    Color::TRANSPARENT,
                    0.0,
                    Sides::ALL,
                    Edges {
                        inline_start: 24.0,
                        ..Edges::ZERO
                    },
                    Color::TRANSPARENT,
                    8.0,
                ),
                _ => (
                    Color::TRANSPARENT,
                    0.0,
                    Sides::ALL,
                    Edges::ZERO,
                    Color::TRANSPARENT,
                    0.0,
                ),
            };
            assert_eq!(
                (
                    style.border,
                    style.border_width,
                    style.border_sides,
                    style.padding,
                    bg(&style),
                    style.gap
                ),
                expected,
                "{preset:?}"
            );
            let corner = if *preset == TextPreset::InlineCode {
                radius(&tokens, RadiusScale::Sm)
            } else {
                Corners::ZERO
            };
            assert_eq!(style.radius, corner, "{preset:?}");
        }
    }
}

#[test]
fn canvas_matches_its_reference_table() {
    for tokens in schemes() {
        let primary = tokens.color(ColorRole::Primary);
        let query = |slot| StyleQuery::new(ControlKind::Canvas, slot);
        let outline = get(&tokens, query(Slot::Indicator));
        assert_eq!(
            (outline.border, outline.border_width, bg(&outline)),
            (primary, 1.0, Color::TRANSPARENT)
        );
        let band = get(&tokens, query(Slot::Range));
        assert_eq!(
            (bg(&band), band.border, band.border_width),
            (primary.with_alpha(0.15), primary, 1.0)
        );
    }
}

#[test]
fn text_editor_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot, state| StyleQuery::new(ControlKind::TextEditor, slot).state(state);
        let field = get(&tokens, query(Slot::Root, VisualState::NONE));
        let fill = if tokens.scheme == ColorScheme::Dark {
            c(ColorRole::Input).with_alpha(0.3)
        } else {
            Color::TRANSPARENT
        };
        assert_eq!(
            (field.border, field.border_width, bg(&field)),
            (c(ColorRole::Input), 1.0, fill)
        );
        assert_eq!(
            (field.radius, field.shadow, field.padding, field.cursor),
            (
                radius(&tokens, RadiusScale::Md),
                tokens.shadow(ShadowScale::Xs),
                Edges::axes(10.0, 8.0),
                Cursor::Text
            )
        );
        let focused = get(&tokens, query(Slot::Root, VisualState::FOCUS_VISIBLE));
        assert_eq!(
            (focused.border, focused.ring),
            (c(ColorRole::Ring), focus_ring(&tokens))
        );
        let disabled = get(&tokens, query(Slot::Root, VisualState::DISABLED));
        assert_eq!(disabled.opacity, 0.5);

        let placeholder = get(&tokens, query(Slot::Placeholder, VisualState::NONE));
        text(
            &tokens,
            &placeholder,
            ColorRole::MutedForeground,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
        let caret = get(&tokens, query(Slot::Indicator, VisualState::NONE));
        assert_eq!(
            (bg(&caret), caret.min_width),
            (c(ColorRole::Foreground), 1.0)
        );
        let selection = get(&tokens, query(Slot::Range, VisualState::NONE));
        assert_eq!(bg(&selection), c(ColorRole::Primary).with_alpha(0.25));
        let code = get(&tokens, query(Slot::Text, VisualState::NONE));
        assert_eq!(code.text, tokens.mono(TextSize::Sm, FontWeight::NORMAL));
    }
}
