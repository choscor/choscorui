//! Reference tables of the dialogs, sheets, drawers, popovers, tooltips,
//! menus, and the command palette, as `docs/controls.md` pins them.

use choscorui_controls::{
    ControlKind, ControlSize, Corners, Cursor, Edges, MenuItemVariant, ResolvedStyle, Ring,
    SheetSide, Sides, Slot, StyleQuery, Styles, VisualState,
};
use choscorui_core::{
    Color, ColorRole, ColorScheme, DesignTokens, FontWeight, RadiusScale, ShadowLayers,
    ShadowScale, TextSize, Tracking,
};

fn get(tokens: &DesignTokens, query: StyleQuery) -> ResolvedStyle {
    Styles::new().resolve(tokens, query)
}

fn schemes() -> [DesignTokens; 2] {
    [DesignTokens::light(), DesignTokens::dark()]
}

fn radius(tokens: &DesignTokens, step: RadiusScale) -> Corners {
    Corners::all(tokens.radius(step))
}

/// The 1 pt hairline ring of floating surfaces.
fn hairline(tokens: &DesignTokens) -> Option<Ring> {
    Some(Ring {
        width: 1.0,
        color: tokens.color(ColorRole::Foreground).with_alpha(0.1),
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

/// The popover surface: fill, hairline ring, and the open motion.
fn assert_surface(tokens: &DesignTokens, style: &ResolvedStyle, step: RadiusScale) {
    assert_eq!(
        (style.background, style.foreground),
        (
            tokens.color(ColorRole::Popover),
            tokens.color(ColorRole::PopoverForeground)
        )
    );
    assert_eq!(
        (style.ring, style.radius, style.motion_ms),
        (hairline(tokens), radius(tokens, step), 100)
    );
}

fn assert_scrim(style: &ResolvedStyle, motion: u32) {
    assert_eq!(
        (style.background, style.backdrop_blur, style.motion_ms),
        (Color::BLACK.with_alpha(0.1), 4.0, motion)
    );
}

/// The close button: a ghost icon-sm button 16 pt inside the corner.
fn assert_close(tokens: &DesignTokens, kind: ControlKind) {
    let close = get(tokens, StyleQuery::new(kind, Slot::Close));
    assert_eq!(
        (close.min_width, close.min_height, close.padding),
        (32.0, 32.0, Edges::all(16.0)),
        "{}",
        kind.name()
    );
    let hover = get(
        tokens,
        StyleQuery::new(kind, Slot::Close).state(VisualState::HOVERED),
    );
    let muted = tokens.color(ColorRole::Muted);
    let fill = if tokens.scheme == ColorScheme::Dark {
        muted.with_alpha(0.5)
    } else {
        muted
    };
    assert_eq!(hover.background, fill);
}

#[test]
fn alert_dialog_matches_its_reference_table() {
    for tokens in schemes() {
        let query = |slot| StyleQuery::new(ControlKind::AlertDialog, slot);
        let panel = get(&tokens, query(Slot::Root));
        assert_surface(&tokens, &panel, RadiusScale::Xl);
        assert_eq!(
            (panel.shadow, panel.padding, panel.gap),
            (ShadowLayers::NONE, Edges::all(24.0), 24.0)
        );
        for (size, width) in [(ControlSize::Default, 512.0), (ControlSize::Sm, 320.0)] {
            let panel = get(&tokens, query(Slot::Root).size(size));
            assert_eq!(panel.max_width, width, "{}", size.name());
        }
        assert_scrim(&get(&tokens, query(Slot::Overlay)), 100);
        let title = get(&tokens, query(Slot::Title));
        text(
            &tokens,
            &title,
            ColorRole::Foreground,
            TextSize::Lg,
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
        assert_eq!(get(&tokens, query(Slot::Header)).gap, 6.0);
        assert_eq!(get(&tokens, query(Slot::Footer)).gap, 8.0);
        let media = get(&tokens, query(Slot::Media));
        assert_eq!(
            (media.background, media.radius),
            (
                tokens.color(ColorRole::Muted),
                radius(&tokens, RadiusScale::Md)
            )
        );
        assert_eq!(
            (
                media.min_width,
                media.min_height,
                media.padding.block_end,
                media.icon_size
            ),
            (64.0, 64.0, 8.0, 32.0)
        );
    }
}

#[test]
fn dialog_matches_its_reference_table() {
    for tokens in schemes() {
        let query = |slot| StyleQuery::new(ControlKind::Dialog, slot);
        let panel = get(&tokens, query(Slot::Root));
        assert_surface(&tokens, &panel, RadiusScale::Xl);
        assert_eq!(
            (panel.shadow, panel.padding, panel.gap, panel.max_width),
            (ShadowLayers::NONE, Edges::all(24.0), 24.0, 448.0)
        );
        assert_scrim(&get(&tokens, query(Slot::Overlay)), 100);
        let title = get(&tokens, query(Slot::Title));
        let mut spec = tokens.text(TextSize::Base, FontWeight::MEDIUM);
        spec.line_height = spec.size;
        assert_eq!(title.text, spec);
        let detail = get(&tokens, query(Slot::Description));
        text(
            &tokens,
            &detail,
            ColorRole::MutedForeground,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
        assert_eq!(get(&tokens, query(Slot::Header)).gap, 8.0);
        assert_eq!(get(&tokens, query(Slot::Footer)).gap, 8.0);
        assert_close(&tokens, ControlKind::Dialog);
    }
}

/// The side, the border side, and the rounded inner corners of each edge.
fn sides(tokens: &DesignTokens) -> [(SheetSide, Sides, Corners); 4] {
    let xl = radius(tokens, RadiusScale::Xl);
    [
        (SheetSide::Right, Sides::START, xl.square_end()),
        (SheetSide::Left, Sides::END, xl.square_start()),
        (SheetSide::Top, Sides::BOTTOM, xl.square_top()),
        (SheetSide::Bottom, Sides::TOP, xl.square_bottom()),
    ]
}

/// The parts that a sheet and a drawer share.
fn assert_edge_panel(tokens: &DesignTokens, kind: ControlKind) {
    let query = |slot| StyleQuery::new(kind, slot);
    let panel = get(tokens, query(Slot::Root));
    assert_eq!(
        (
            panel.background,
            panel.border,
            panel.border_width,
            panel.gap
        ),
        (
            tokens.color(ColorRole::Popover),
            tokens.color(ColorRole::Border),
            1.0,
            16.0
        ),
        "{}",
        kind.name()
    );
    let header = get(tokens, query(Slot::Header));
    assert_eq!((header.padding, header.gap), (Edges::all(16.0), 6.0));
    let footer = get(tokens, query(Slot::Footer));
    assert_eq!((footer.padding, footer.gap), (Edges::all(16.0), 8.0));
    let title = get(tokens, query(Slot::Title));
    text(
        tokens,
        &title,
        ColorRole::Foreground,
        TextSize::Base,
        FontWeight::MEDIUM,
    );
    let detail = get(tokens, query(Slot::Description));
    text(
        tokens,
        &detail,
        ColorRole::MutedForeground,
        TextSize::Sm,
        FontWeight::NORMAL,
    );
    assert_close(tokens, kind);
}

#[test]
fn sheet_matches_its_reference_table() {
    for tokens in schemes() {
        assert_edge_panel(&tokens, ControlKind::Sheet);
        for (side, border, _) in sides(&tokens) {
            let panel = get(
                &tokens,
                StyleQuery::new(ControlKind::Sheet, Slot::Root).variant(side),
            );
            let width = match side {
                SheetSide::Left | SheetSide::Right => 384.0,
                SheetSide::Top | SheetSide::Bottom => f64::INFINITY,
            };
            assert_eq!(
                (panel.border_sides, panel.radius, panel.max_width),
                (border, Corners::ZERO, width),
                "{}",
                side.name()
            );
            assert_eq!(
                (panel.shadow, panel.motion_ms),
                (tokens.shadow(ShadowScale::Lg), 200)
            );
        }
        let scrim = get(&tokens, StyleQuery::new(ControlKind::Sheet, Slot::Overlay));
        assert_scrim(&scrim, 150);
    }
}

#[test]
fn drawer_matches_its_reference_table() {
    for tokens in schemes() {
        assert_edge_panel(&tokens, ControlKind::Drawer);
        for (side, border, corners) in sides(&tokens) {
            let panel = get(
                &tokens,
                StyleQuery::new(ControlKind::Drawer, Slot::Root).variant(side),
            );
            assert_eq!(
                (
                    panel.border_sides,
                    panel.radius,
                    panel.shadow,
                    panel.motion_ms
                ),
                (border, corners, ShadowLayers::NONE, 450),
                "{}",
                side.name()
            );
        }
        let query = |slot| StyleQuery::new(ControlKind::Drawer, slot);
        assert_scrim(&get(&tokens, query(Slot::Overlay)), 450);
        let handle = get(&tokens, query(Slot::Handle));
        assert_eq!(
            (handle.background, handle.radius),
            (
                tokens.color(ColorRole::Muted),
                radius(&tokens, RadiusScale::Full)
            )
        );
        assert_eq!(
            (
                handle.min_width,
                handle.min_height,
                handle.padding.block_start
            ),
            (100.0, 6.0, 16.0)
        );
    }
}

/// A popover or a hover card panel and its text.
fn assert_floating_panel(tokens: &DesignTokens, kind: ControlKind, step: RadiusScale, width: f64) {
    let query = |slot| StyleQuery::new(kind, slot);
    let panel = get(tokens, query(Slot::Root));
    assert_surface(tokens, &panel, step);
    assert_eq!(
        (
            panel.shadow,
            panel.padding,
            panel.gap,
            panel.min_width,
            panel.max_width
        ),
        (
            tokens.shadow(ShadowScale::Md),
            Edges::all(16.0),
            16.0,
            width,
            width
        ),
        "{}",
        kind.name()
    );
    assert_eq!(get(tokens, query(Slot::Header)).gap, 4.0);
    let title = get(tokens, query(Slot::Title));
    text(
        tokens,
        &title,
        ColorRole::Foreground,
        TextSize::Sm,
        FontWeight::MEDIUM,
    );
    let detail = get(tokens, query(Slot::Description));
    text(
        tokens,
        &detail,
        ColorRole::MutedForeground,
        TextSize::Sm,
        FontWeight::NORMAL,
    );
}

#[test]
fn popover_matches_its_reference_table() {
    for tokens in schemes() {
        assert_floating_panel(&tokens, ControlKind::Popover, RadiusScale::Md, 288.0);
        let content = StyleQuery::new(ControlKind::Popover, Slot::Content);
        assert_eq!(
            get(&tokens, content),
            get(&tokens, StyleQuery::new(ControlKind::Popover, Slot::Root))
        );
    }
}

#[test]
fn hover_card_matches_its_reference_table() {
    for tokens in schemes() {
        assert_floating_panel(&tokens, ControlKind::HoverCard, RadiusScale::Lg, 256.0);
    }
}

#[test]
fn tooltip_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::Tooltip, slot);
        let bubble = get(&tokens, query(Slot::Root));
        assert_eq!(
            (bubble.background, bubble.foreground, bubble.radius),
            (
                c(ColorRole::Foreground),
                c(ColorRole::Background),
                radius(&tokens, RadiusScale::Md)
            )
        );
        assert_eq!(
            (
                bubble.padding,
                bubble.gap,
                bubble.max_width,
                bubble.motion_ms
            ),
            (Edges::axes(12.0, 6.0), 6.0, 320.0, 100)
        );
        assert_eq!(bubble.text, tokens.text(TextSize::Xs, FontWeight::NORMAL));
        let arrow = get(&tokens, query(Slot::Indicator));
        assert_eq!(
            (
                arrow.background,
                arrow.min_width,
                arrow.min_height,
                arrow.radius
            ),
            (c(ColorRole::Foreground), 10.0, 10.0, Corners::all(2.0))
        );
    }
}

/// The rows, marks, captions, rules, and shortcuts of a menu.
fn assert_menu_rows(tokens: &DesignTokens, kind: ControlKind) {
    let c = |role| tokens.color(role);
    let query = |slot, state| StyleQuery::new(kind, slot).state(state);
    let row = get(tokens, query(Slot::Item, VisualState::NONE));
    assert_eq!(
        (row.min_height, row.padding, row.radius, row.gap),
        (
            32.0,
            Edges::axes(8.0, 6.0),
            radius(tokens, RadiusScale::Sm),
            8.0
        ),
        "{}",
        kind.name()
    );
    let highlighted = get(tokens, query(Slot::Item, VisualState::HIGHLIGHTED));
    assert_eq!(
        (highlighted.background, highlighted.foreground),
        (c(ColorRole::Accent), c(ColorRole::AccentForeground))
    );
    let destructive = |state| {
        get(
            tokens,
            query(Slot::Item, state).variant(MenuItemVariant::Destructive),
        )
    };
    let rest = destructive(VisualState::NONE);
    assert_eq!(
        (rest.foreground, rest.background),
        (c(ColorRole::Destructive), Color::TRANSPARENT)
    );
    let alpha = if tokens.scheme == ColorScheme::Dark {
        0.2
    } else {
        0.1
    };
    assert_eq!(
        destructive(VisualState::HIGHLIGHTED).background,
        c(ColorRole::Destructive).with_alpha(alpha)
    );
    assert_eq!(
        get(tokens, query(Slot::Item, VisualState::DISABLED)).opacity,
        0.5
    );
    // A submenu row; in a menu bar, `Trigger` is a bar title instead.
    if kind == ControlKind::Menu {
        let open = get(tokens, query(Slot::Trigger, VisualState::OPEN));
        assert_eq!(
            (open.background, open.foreground),
            (c(ColorRole::Accent), c(ColorRole::AccentForeground))
        );
    }
    let mark = get(tokens, query(Slot::ItemIndicator, VisualState::NONE));
    assert_eq!((mark.min_width, mark.icon_size), (16.0, 16.0));
    let rule = get(tokens, query(Slot::Separator, VisualState::NONE));
    assert_eq!(
        (rule.background, rule.min_height, rule.padding),
        (c(ColorRole::Border), 1.0, Edges::axes(-4.0, 4.0))
    );
    let widest = tokens
        .text(TextSize::Xs, FontWeight::NORMAL)
        .with_tracking(Tracking::Widest);
    let shortcut = get(tokens, query(Slot::Shortcut, VisualState::NONE));
    assert_eq!(
        (shortcut.foreground, shortcut.text),
        (c(ColorRole::MutedForeground), widest)
    );
    let lit = get(tokens, query(Slot::Shortcut, VisualState::HIGHLIGHTED));
    assert_eq!(lit.foreground, c(ColorRole::AccentForeground));
}

#[test]
fn menu_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::Menu, slot);
        let panel = get(&tokens, query(Slot::Root));
        assert_surface(&tokens, &panel, RadiusScale::Md);
        assert_eq!(
            (panel.shadow, panel.padding, panel.min_width),
            (tokens.shadow(ShadowScale::Md), Edges::all(4.0), 128.0)
        );
        assert_menu_rows(&tokens, ControlKind::Menu);
        let caption = get(&tokens, query(Slot::GroupLabel));
        text(
            &tokens,
            &caption,
            ColorRole::MutedForeground,
            TextSize::Xs,
            FontWeight::MEDIUM,
        );
        assert_eq!(caption.padding, Edges::axes(8.0, 6.0));

        // The area that opens a context menu.
        let area = get(&tokens, query(Slot::Placeholder));
        assert_eq!(
            (
                area.min_width,
                area.min_height,
                area.border,
                area.border_width
            ),
            (300.0, 150.0, c(ColorRole::Border), 1.0)
        );
        text(
            &tokens,
            &area,
            ColorRole::MutedForeground,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
        assert_eq!(area.radius, radius(&tokens, RadiusScale::Md));
    }
}

#[test]
fn menubar_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot, state| StyleQuery::new(ControlKind::Menubar, slot).state(state);
        let bar = get(&tokens, query(Slot::Root, VisualState::NONE));
        assert_eq!(
            (bar.min_height, bar.padding, bar.gap, bar.radius),
            (36.0, Edges::all(4.0), 4.0, radius(&tokens, RadiusScale::Md))
        );
        assert_eq!(
            (bar.background, bar.border, bar.border_width, bar.shadow),
            (
                c(ColorRole::Background),
                c(ColorRole::Border),
                1.0,
                tokens.shadow(ShadowScale::Xs)
            )
        );
        let title = get(&tokens, query(Slot::Trigger, VisualState::NONE));
        assert_eq!(
            (title.padding, title.radius, title.background),
            (
                Edges::axes(8.0, 4.0),
                radius(&tokens, RadiusScale::Sm),
                Color::TRANSPARENT
            )
        );
        assert_eq!(title.text, tokens.text(TextSize::Sm, FontWeight::MEDIUM));
        for state in [
            VisualState::HOVERED,
            VisualState::OPEN,
            VisualState::HIGHLIGHTED,
        ] {
            let lit = get(&tokens, query(Slot::Trigger, state));
            assert_eq!(lit.background, c(ColorRole::Muted), "{state}");
        }
        let disabled = get(&tokens, query(Slot::Trigger, VisualState::DISABLED));
        assert_eq!(disabled.opacity, 0.5);

        let panel = get(&tokens, query(Slot::Content, VisualState::NONE));
        assert_surface(&tokens, &panel, RadiusScale::Md);
        assert_eq!((panel.padding, panel.min_width), (Edges::all(4.0), 144.0));
        assert_menu_rows(&tokens, ControlKind::Menubar);
        let caption = get(&tokens, query(Slot::GroupLabel, VisualState::NONE));
        text(
            &tokens,
            &caption,
            ColorRole::Foreground,
            TextSize::Sm,
            FontWeight::MEDIUM,
        );
    }
}

#[test]
fn command_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::Command, slot);
        let palette = get(&tokens, query(Slot::Root));
        assert_eq!(
            (palette.background, palette.foreground),
            (c(ColorRole::Popover), c(ColorRole::PopoverForeground))
        );
        assert_eq!(
            (palette.radius, palette.padding, palette.ring),
            (radius(&tokens, RadiusScale::Xl), Edges::all(4.0), None)
        );
        // The dialog form: a hairline ring and at most 448 pt.
        let dialog = get(&tokens, query(Slot::Root).size(ControlSize::Lg));
        assert_eq!((dialog.ring, dialog.max_width), (hairline(&tokens), 448.0));
        assert_scrim(&get(&tokens, query(Slot::Overlay)), 100);

        let input = get(&tokens, query(Slot::Input));
        let faint = c(ColorRole::Input).with_alpha(0.3);
        assert_eq!(
            (input.background, input.border, input.border_width),
            (faint, faint, 1.0)
        );
        assert_eq!(
            (input.radius, input.min_height, input.padding, input.gap),
            (
                radius(&tokens, RadiusScale::Lg),
                32.0,
                Edges::axes(8.0, 0.0),
                8.0
            )
        );
        assert_eq!(input.cursor, Cursor::Text);
        let icon = get(&tokens, query(Slot::Icon));
        assert_eq!((icon.icon_size, icon.opacity), (16.0, 0.5));
        assert_eq!(get(&tokens, query(Slot::Group)).padding, Edges::all(4.0));
        let caption = get(&tokens, query(Slot::GroupLabel));
        text(
            &tokens,
            &caption,
            ColorRole::MutedForeground,
            TextSize::Xs,
            FontWeight::MEDIUM,
        );

        let row = get(&tokens, query(Slot::Item));
        assert_eq!(
            (row.min_height, row.radius),
            (32.0, radius(&tokens, RadiusScale::Sm))
        );
        for state in [VisualState::HIGHLIGHTED, VisualState::SELECTED] {
            let lit = get(&tokens, query(Slot::Item).state(state));
            assert_eq!(
                (lit.background, lit.foreground),
                (c(ColorRole::Muted), c(ColorRole::Foreground)),
                "{state}"
            );
        }
        let dialog_row = get(&tokens, query(Slot::Item).size(ControlSize::Lg));
        assert_eq!(dialog_row.radius, radius(&tokens, RadiusScale::Lg));

        let shortcut = get(&tokens, query(Slot::Shortcut));
        assert_eq!(
            shortcut.text,
            tokens
                .text(TextSize::Xs, FontWeight::NORMAL)
                .with_tracking(Tracking::Widest)
        );
        assert_eq!(shortcut.foreground, c(ColorRole::MutedForeground));
        let lit = get(
            &tokens,
            query(Slot::Shortcut).state(VisualState::HIGHLIGHTED),
        );
        assert_eq!(lit.foreground, c(ColorRole::Foreground));
        let rule = get(&tokens, query(Slot::Separator));
        assert_eq!(
            (rule.background, rule.min_height),
            (c(ColorRole::Border), 1.0)
        );
        let empty = get(&tokens, query(Slot::Description));
        text(
            &tokens,
            &empty,
            ColorRole::Foreground,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
        assert_eq!(empty.padding, Edges::axes(0.0, 24.0));
    }
}
