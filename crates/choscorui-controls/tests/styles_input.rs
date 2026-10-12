//! Reference tables of the text inputs, selects, the date picker, and the
//! field parts, as `docs/controls.md` pins them.

use choscorui_controls::{
    AddonAlign, ControlKind, ControlSize, Corners, Cursor, Edges, FieldOrientation, ResolvedStyle,
    Ring, Sides, Slot, StyleQuery, Styles, Variant, VisualState,
};
use choscorui_core::{
    Color, ColorRole, ColorScheme, DesignTokens, FontWeight, RadiusScale, ShadowLayers,
    ShadowScale, TextSize,
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

fn invalid_ring(tokens: &DesignTokens) -> Option<Ring> {
    Some(Ring {
        width: 3.0,
        color: tokens
            .color(ColorRole::Destructive)
            .with_alpha(pick(tokens, 0.2, 0.4)),
    })
}

/// Small muted text, as placeholders and captions use.
fn assert_muted(tokens: &DesignTokens, style: &ResolvedStyle, size: TextSize) {
    assert_eq!(
        (style.foreground, style.text),
        (
            tokens.color(ColorRole::MutedForeground),
            tokens.text(size, FontWeight::NORMAL)
        )
    );
}

/// The input box: an `input` edge, a 30% `input` fill in dark, radius md,
/// and the extra-small shadow. The focus, invalid, and disabled looks
/// follow the reference states.
fn assert_input_box(tokens: &DesignTokens, query: StyleQuery) {
    let c = |role| tokens.color(role);
    let rest = get(tokens, query);
    let fill = pick(
        tokens,
        Color::TRANSPARENT,
        c(ColorRole::Input).with_alpha(0.3),
    );
    assert_eq!(
        (rest.border, rest.border_width, bg(&rest)),
        (c(ColorRole::Input), 1.0, fill),
        "{} {}",
        query.kind.name(),
        query.slot.name()
    );
    assert_eq!(
        (rest.radius, rest.shadow, rest.ring),
        (
            radius(tokens, RadiusScale::Md),
            tokens.shadow(ShadowScale::Xs),
            None
        )
    );
    let focused = get(tokens, query.state(VisualState::FOCUS_VISIBLE));
    assert_eq!(
        (focused.border, focused.ring),
        (c(ColorRole::Ring), focus_ring(tokens))
    );
    // Pointer focus shows no ring.
    let pointer = get(tokens, query.state(VisualState::FOCUSED));
    assert_eq!(pointer.ring, None);
    let invalid = get(tokens, query.state(VisualState::INVALID));
    let edge = c(ColorRole::Destructive).with_alpha(pick(tokens, 1.0, 0.5));
    assert_eq!((invalid.border, invalid.ring), (edge, invalid_ring(tokens)));
    let disabled = get(tokens, query.state(VisualState::DISABLED));
    assert_eq!((disabled.opacity, disabled.cursor), (0.5, Cursor::Arrow));
}

#[test]
fn input_matches_its_reference_table() {
    for tokens in schemes() {
        let query = |slot| StyleQuery::new(ControlKind::Input, slot);
        assert_input_box(&tokens, query(Slot::Root));
        let field = get(&tokens, query(Slot::Root));
        assert_eq!(
            (field.min_height, field.padding, field.cursor),
            (36.0, Edges::axes(10.0, 4.0), Cursor::Text)
        );
        assert_muted(
            &tokens,
            &get(&tokens, query(Slot::Placeholder)),
            TextSize::Sm,
        );
        let icon = get(&tokens, query(Slot::Icon));
        assert_muted(&tokens, &icon, TextSize::Sm);
        assert_eq!(icon.icon_size, 16.0);
    }
}

#[test]
fn textarea_matches_its_reference_table() {
    for tokens in schemes() {
        let query = |slot| StyleQuery::new(ControlKind::Textarea, slot);
        assert_input_box(&tokens, query(Slot::Root));
        let field = get(&tokens, query(Slot::Root));
        assert_eq!(
            (field.min_height, field.padding, field.cursor),
            (64.0, Edges::axes(10.0, 8.0), Cursor::Text)
        );
        assert_muted(
            &tokens,
            &get(&tokens, query(Slot::Placeholder)),
            TextSize::Sm,
        );
    }
}

#[test]
fn input_group_matches_its_reference_table() {
    for tokens in schemes() {
        let query = |slot| StyleQuery::new(ControlKind::InputGroup, slot);
        assert_input_box(&tokens, query(Slot::Root));
        let field = get(&tokens, query(Slot::Root));
        assert_eq!((field.min_height, field.padding), (36.0, Edges::ZERO));

        let input = get(&tokens, query(Slot::Input));
        assert_eq!(
            (input.padding, input.cursor),
            (Edges::axes(10.0, 4.0), Cursor::Text)
        );

        // (align, inline start, inline end, block start, block end)
        for (align, start, end, top, bottom) in [
            (AddonAlign::InlineStart, 8.0, 0.0, 6.0, 6.0),
            (AddonAlign::InlineEnd, 0.0, 8.0, 6.0, 6.0),
            (AddonAlign::BlockStart, 10.0, 10.0, 8.0, 6.0),
            (AddonAlign::BlockEnd, 10.0, 10.0, 6.0, 8.0),
        ] {
            let addon = get(&tokens, query(Slot::Addon).variant(align));
            assert_eq!(
                addon.padding,
                Edges {
                    inline_start: start,
                    inline_end: end,
                    block_start: top,
                    block_end: bottom,
                },
                "{}",
                align.name()
            );
            assert_eq!(
                (addon.foreground, addon.text, addon.gap),
                (
                    tokens.color(ColorRole::MutedForeground),
                    tokens.text(TextSize::Sm, FontWeight::MEDIUM),
                    8.0
                )
            );
        }
        let disabled = get(&tokens, query(Slot::Addon).state(VisualState::DISABLED));
        assert_eq!(disabled.opacity, 0.5);

        let text = get(&tokens, query(Slot::Text));
        assert_muted(&tokens, &text, TextSize::Sm);
        assert_eq!(text.gap, 8.0);

        // Button addons are ghost buttons: (size, height, width, radius,
        // inline padding, gap, icon).
        let md = tokens.radius(RadiusScale::Md);
        let small = tokens.radius - 5.0;
        for (size, height, width, corner, inline, gap, icon) in [
            (ControlSize::Xs, 24.0, 0.0, small, 6.0, 4.0, 14.0),
            (ControlSize::IconXs, 24.0, 24.0, small, 0.0, 8.0, 12.0),
            (ControlSize::Sm, 32.0, 0.0, md, 10.0, 8.0, 16.0),
            (ControlSize::IconSm, 32.0, 32.0, md, 0.0, 8.0, 16.0),
        ] {
            let action = get(&tokens, query(Slot::Action).size(size));
            assert_eq!(
                (
                    action.min_height,
                    action.min_width,
                    action.radius,
                    action.padding,
                    action.gap,
                    action.icon_size
                ),
                (
                    height,
                    width,
                    Corners::all(corner),
                    Edges::axes(inline, 0.0),
                    gap,
                    icon
                ),
                "{}",
                size.name()
            );
            assert_eq!(bg(&action), Color::TRANSPARENT);
        }
    }
}

#[test]
fn input_otp_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot, state| StyleQuery::new(ControlKind::InputOtp, slot).state(state);
        let slot = get(&tokens, query(Slot::Item, VisualState::NONE));
        assert_eq!(
            (slot.min_width, slot.min_height, slot.padding, slot.radius),
            (36.0, 36.0, Edges::ZERO, Corners::ZERO)
        );
        assert_eq!(
            (slot.border, slot.border_width, slot.shadow),
            (c(ColorRole::Input), 1.0, tokens.shadow(ShadowScale::Xs))
        );
        // Neighbors share one edge: each slot draws all but its start side.
        assert_eq!(
            slot.border_sides,
            Sides {
                inline_start: false,
                ..Sides::ALL
            }
        );
        let active = get(&tokens, query(Slot::Item, VisualState::SELECTED));
        assert_eq!(
            (active.border, active.ring),
            (c(ColorRole::Ring), focus_ring(&tokens))
        );
        let invalid = get(&tokens, query(Slot::Item, VisualState::INVALID));
        assert_eq!(
            (invalid.border, invalid.ring),
            (c(ColorRole::Destructive), None)
        );
        let invalid_active = get(
            &tokens,
            query(Slot::Item, VisualState::INVALID.with(VisualState::SELECTED)),
        );
        assert_eq!(
            (invalid_active.border, invalid_active.ring),
            (c(ColorRole::Destructive), invalid_ring(&tokens))
        );
        let disabled = get(&tokens, query(Slot::Item, VisualState::DISABLED));
        assert_eq!(disabled.opacity, 0.5);

        let group = get(&tokens, query(Slot::Group, VisualState::NONE));
        assert_eq!(
            (group.radius, group.ring),
            (radius(&tokens, RadiusScale::Md), None)
        );
        let group = get(&tokens, query(Slot::Group, VisualState::INVALID));
        assert_eq!(group.ring, invalid_ring(&tokens));

        let caret = get(&tokens, query(Slot::Indicator, VisualState::NONE));
        assert_eq!(
            (bg(&caret), caret.min_width, caret.min_height),
            (c(ColorRole::Foreground), 1.0, 16.0)
        );
        let separator = get(&tokens, query(Slot::Separator, VisualState::NONE));
        assert_eq!((separator.icon_size, separator.min_width), (16.0, 16.0));
        let root = get(&tokens, query(Slot::Root, VisualState::NONE));
        assert_eq!(root.gap, 8.0);
    }
}

/// The popup list of a select or a combobox, and its rows.
fn assert_list(tokens: &DesignTokens, kind: ControlKind) {
    let c = |role| tokens.color(role);
    let query = |slot, state| StyleQuery::new(kind, slot).state(state);
    let list = get(tokens, query(Slot::Content, VisualState::NONE));
    assert_eq!(
        (bg(&list), list.foreground),
        (c(ColorRole::Popover), c(ColorRole::PopoverForeground)),
        "{}",
        kind.name()
    );
    assert_eq!(
        list.ring,
        Some(Ring {
            width: 1.0,
            color: c(ColorRole::Foreground).with_alpha(0.1),
        })
    );
    assert_eq!(
        (
            list.radius,
            list.shadow,
            list.padding,
            list.min_width,
            list.motion_ms
        ),
        (
            radius(tokens, RadiusScale::Md),
            tokens.shadow(ShadowScale::Md),
            Edges::all(4.0),
            144.0,
            100
        )
    );

    let row = get(tokens, query(Slot::Item, VisualState::NONE));
    assert_eq!(
        (row.min_height, row.radius, row.gap, bg(&row)),
        (
            32.0,
            radius(tokens, RadiusScale::Sm),
            8.0,
            Color::TRANSPARENT
        )
    );
    assert_eq!(
        row.padding,
        Edges {
            inline_start: 8.0,
            inline_end: 32.0,
            block_start: 6.0,
            block_end: 6.0,
        }
    );
    let highlighted = get(tokens, query(Slot::Item, VisualState::HIGHLIGHTED));
    assert_eq!(
        (bg(&highlighted), highlighted.foreground),
        (c(ColorRole::Accent), c(ColorRole::AccentForeground))
    );
    let disabled = get(tokens, query(Slot::Item, VisualState::DISABLED));
    assert_eq!(disabled.opacity, 0.5);

    for (state, color) in [
        (VisualState::NONE, c(ColorRole::PopoverForeground)),
        (VisualState::HIGHLIGHTED, c(ColorRole::AccentForeground)),
    ] {
        let mark = get(tokens, query(Slot::ItemIndicator, state));
        assert_eq!(
            (mark.min_width, mark.icon_size, mark.foreground),
            (16.0, 16.0, color)
        );
    }
    let caption = get(tokens, query(Slot::GroupLabel, VisualState::NONE));
    assert_muted(tokens, &caption, TextSize::Xs);
    assert_eq!(caption.padding, Edges::axes(8.0, 6.0));
    let rule = get(tokens, query(Slot::Separator, VisualState::NONE));
    assert_eq!(
        (bg(&rule), rule.min_height, rule.padding.block_start),
        (c(ColorRole::Border), 1.0, 4.0)
    );
    assert_muted(
        tokens,
        &get(tokens, query(Slot::Placeholder, VisualState::NONE)),
        TextSize::Sm,
    );
    let value = get(tokens, query(Slot::Value, VisualState::NONE));
    assert_eq!(
        (value.foreground, value.text),
        (
            c(ColorRole::Foreground),
            tokens.text(TextSize::Sm, FontWeight::NORMAL)
        )
    );
}

#[test]
fn select_matches_its_reference_table() {
    for tokens in schemes() {
        let query = |slot| StyleQuery::new(ControlKind::Select, slot);
        assert_input_box(&tokens, query(Slot::Trigger));
        for (size, height) in [(ControlSize::Default, 36.0), (ControlSize::Sm, 32.0)] {
            let trigger = get(&tokens, query(Slot::Trigger).size(size));
            assert_eq!(trigger.min_height, height, "{}", size.name());
            assert_eq!(trigger.gap, 6.0);
            assert_eq!(
                trigger.padding,
                Edges {
                    inline_start: 10.0,
                    inline_end: 8.0,
                    block_start: 8.0,
                    block_end: 8.0,
                }
            );
        }
        let hover = get(&tokens, query(Slot::Trigger).state(VisualState::HOVERED));
        let input = tokens.color(ColorRole::Input);
        assert_eq!(
            bg(&hover),
            pick(&tokens, Color::TRANSPARENT, input.with_alpha(0.5))
        );
        assert_list(&tokens, ControlKind::Select);
        assert_eq!(get(&tokens, query(Slot::Content)).min_height, 0.0);
        let button = get(&tokens, query(Slot::Previous));
        assert_eq!(
            (bg(&button), button.min_height, button.padding),
            (
                tokens.color(ColorRole::Popover),
                24.0,
                Edges::axes(0.0, 4.0)
            )
        );
    }
}

#[test]
fn native_select_matches_its_reference_table() {
    for tokens in schemes() {
        let query = |slot| StyleQuery::new(ControlKind::NativeSelect, slot);
        assert_input_box(&tokens, query(Slot::Root));
        for (size, height) in [(ControlSize::Default, 36.0), (ControlSize::Sm, 32.0)] {
            let field = get(&tokens, query(Slot::Root).size(size));
            assert_eq!(field.min_height, height, "{}", size.name());
            assert_eq!(
                field.padding,
                Edges {
                    inline_start: 10.0,
                    inline_end: 32.0,
                    block_start: 4.0,
                    block_end: 4.0,
                }
            );
        }
        let hover = get(&tokens, query(Slot::Root).state(VisualState::HOVERED));
        let input = tokens.color(ColorRole::Input);
        assert_eq!(
            bg(&hover),
            pick(&tokens, Color::TRANSPARENT, input.with_alpha(0.5))
        );
        let chevron = get(&tokens, query(Slot::Icon));
        assert_muted(&tokens, &chevron, TextSize::Sm);
        assert_eq!(chevron.padding.inline_end, 10.0);
    }
}

#[test]
fn combobox_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::Combobox, slot);
        assert_input_box(&tokens, query(Slot::Trigger));
        assert_eq!(get(&tokens, query(Slot::Trigger)).min_height, 36.0);
        assert_list(&tokens, ControlKind::Combobox);
        assert_eq!(get(&tokens, query(Slot::Content)).min_height, 0.0);

        let search = get(&tokens, query(Slot::Input));
        let faint = c(ColorRole::Input).with_alpha(0.3);
        assert_eq!(
            (bg(&search), search.border, search.shadow),
            (faint, faint, ShadowLayers::NONE)
        );
        assert_eq!(
            (search.min_height, search.padding, search.cursor),
            (32.0, Edges::axes(10.0, 0.0), Cursor::Text)
        );

        let chip = get(&tokens, query(Slot::Badge));
        assert_eq!(
            (bg(&chip), chip.radius, chip.min_height),
            (c(ColorRole::Muted), radius(&tokens, RadiusScale::Sm), 22.0)
        );
        assert_eq!(
            (chip.padding, chip.gap, chip.text),
            (
                Edges::axes(6.0, 0.0),
                4.0,
                tokens.text(TextSize::Xs, FontWeight::MEDIUM)
            )
        );
        let close = get(&tokens, query(Slot::Close));
        assert_eq!((close.opacity, close.icon_size), (0.5, 12.0));
        let hover = get(&tokens, query(Slot::Close).state(VisualState::HOVERED));
        assert_eq!(hover.opacity, 1.0);

        let empty = get(&tokens, query(Slot::Description));
        assert_muted(&tokens, &empty, TextSize::Sm);
        assert_eq!(empty.padding, Edges::axes(0.0, 8.0));
    }
}

#[test]
fn date_picker_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::DatePicker, slot);
        let trigger = get(&tokens, query(Slot::Root));
        assert_eq!(
            (trigger.min_height, trigger.gap, trigger.padding),
            (36.0, 8.0, Edges::axes(10.0, 0.0))
        );
        assert_eq!(
            (trigger.border, trigger.shadow, trigger.radius),
            (
                pick(&tokens, c(ColorRole::Border), c(ColorRole::Input)),
                tokens.shadow(ShadowScale::Xs),
                radius(&tokens, RadiusScale::Md)
            )
        );
        assert_eq!(trigger.text, tokens.text(TextSize::Sm, FontWeight::MEDIUM));
        let open = get(&tokens, query(Slot::Root).state(VisualState::OPEN));
        assert_eq!(bg(&open), c(ColorRole::Muted));

        let popover = get(&tokens, query(Slot::Content));
        assert_eq!(
            (bg(&popover), popover.radius),
            (c(ColorRole::Popover), radius(&tokens, RadiusScale::Lg))
        );
        assert_eq!(
            (popover.shadow, popover.padding),
            (tokens.shadow(ShadowScale::Md), Edges::ZERO)
        );
        assert_muted(
            &tokens,
            &get(&tokens, query(Slot::Placeholder)),
            TextSize::Sm,
        );
    }
}

#[test]
fn field_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::Field, slot);
        // (slot, gap): the set, the group, the field, and its content.
        for (slot, gap) in [
            (Slot::Root, 24.0),
            (Slot::Group, 28.0),
            (Slot::Item, 12.0),
            (Slot::Content, 4.0),
        ] {
            assert_eq!(get(&tokens, query(slot)).gap, gap, "{}", slot.name());
        }
        let horizontal = get(
            &tokens,
            query(Slot::Item).variant(FieldOrientation::Horizontal),
        );
        assert_eq!(horizontal.min_height, 16.0);
        let invalid = get(&tokens, query(Slot::Item).state(VisualState::INVALID));
        assert_eq!(invalid.foreground, c(ColorRole::Destructive));
        let disabled = get(&tokens, query(Slot::Item).state(VisualState::DISABLED));
        assert_eq!(disabled.opacity, 0.5);

        for (size, text) in [
            (ControlSize::Default, TextSize::Base),
            (ControlSize::Sm, TextSize::Sm),
        ] {
            let legend = get(&tokens, query(Slot::Legend).size(size));
            assert_eq!(
                (legend.text, legend.padding.block_end),
                (tokens.text(text, FontWeight::MEDIUM), 12.0),
                "{}",
                size.name()
            );
        }

        let medium = tokens.text(TextSize::Sm, FontWeight::MEDIUM);
        for slot in [Slot::Label, Slot::Title] {
            let style = get(&tokens, query(slot));
            assert_eq!(
                (
                    style.foreground,
                    style.text.size,
                    style.text.weight,
                    style.gap
                ),
                (c(ColorRole::Foreground), medium.size, medium.weight, 8.0),
                "{}",
                slot.name()
            );
            assert_eq!(style.text.line_height, medium.size * 1.375);
        }
        // A label that wraps a whole field is a selectable card.
        let card = |state| get(&tokens, query(Slot::Label).variant(Variant(1)).state(state));
        let rest = card(VisualState::NONE);
        assert_eq!(
            (rest.border, rest.radius, rest.padding),
            (
                c(ColorRole::Border),
                radius(&tokens, RadiusScale::Md),
                Edges::all(12.0)
            )
        );
        let muted = c(ColorRole::Muted).with_alpha(0.5);
        assert_eq!(bg(&card(VisualState::HOVERED)), muted);
        let checked = card(VisualState::CHECKED);
        let primary = c(ColorRole::Primary);
        assert_eq!(
            (bg(&checked), checked.border),
            (
                primary.with_alpha(pick(&tokens, 0.05, 0.1)),
                primary.with_alpha(pick(&tokens, 0.3, 0.2))
            )
        );
        assert_eq!(card(VisualState::FOCUS_VISIBLE).ring, focus_ring(&tokens));

        assert_muted(
            &tokens,
            &get(&tokens, query(Slot::Description)),
            TextSize::Sm,
        );
        let error = get(&tokens, query(Slot::Error));
        assert_eq!(
            (error.foreground, error.text),
            (
                c(ColorRole::Destructive),
                tokens.text(TextSize::Sm, FontWeight::NORMAL)
            )
        );
        let separator = get(&tokens, query(Slot::Separator));
        assert_eq!(
            (bg(&separator), separator.min_height),
            (c(ColorRole::Border), 20.0)
        );
        let chip = get(&tokens, query(Slot::Text));
        assert_muted(&tokens, &chip, TextSize::Sm);
        assert_eq!(
            (bg(&chip), chip.padding),
            (c(ColorRole::Background), Edges::axes(8.0, 0.0))
        );
    }
}
