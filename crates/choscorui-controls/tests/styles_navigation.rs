//! Reference tables of the tabs, breadcrumbs, pagination, the navigation
//! menu, the sidebar, and the window chrome bars, as `docs/controls.md`
//! pins them.

use choscorui_controls::{
    ControlKind, ControlSize, Corners, Edges, ResolvedStyle, Ring, SidebarItemVariant,
    SidebarVariant, Sides, Slot, StyleQuery, Styles, TabsVariant, VisualState,
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
fn tabs_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |variant: TabsVariant, slot, state| {
            StyleQuery::new(ControlKind::Tabs, slot)
                .variant(variant)
                .state(state)
        };
        let root = get(
            &tokens,
            query(TabsVariant::Default, Slot::Root, VisualState::NONE),
        );
        assert_eq!(root.gap, 8.0);
        // (variant, list fill, list radius, list gap)
        for (variant, fill, corner, gap) in [
            (
                TabsVariant::Default,
                c(ColorRole::Muted),
                radius(&tokens, RadiusScale::Lg),
                0.0,
            ),
            (TabsVariant::Line, Color::TRANSPARENT, Corners::ZERO, 4.0),
        ] {
            let list = get(&tokens, query(variant, Slot::Group, VisualState::NONE));
            assert_eq!(
                (bg(&list), list.radius, list.gap),
                (fill, corner, gap),
                "{}",
                variant.name()
            );
            assert_eq!(
                (list.padding, list.min_height, list.foreground),
                (Edges::all(3.0), 36.0, c(ColorRole::MutedForeground))
            );

            let tab = get(&tokens, query(variant, Slot::Trigger, VisualState::NONE));
            assert_eq!(
                (tab.padding, tab.radius, tab.gap, tab.border_width),
                (
                    Edges::axes(8.0, 4.0),
                    radius(&tokens, RadiusScale::Md),
                    6.0,
                    1.0
                )
            );
            assert_eq!(tab.text, tokens.text(TextSize::Sm, FontWeight::MEDIUM));
            let resting = pick(
                &tokens,
                c(ColorRole::Foreground).with_alpha(0.6),
                c(ColorRole::MutedForeground),
            );
            assert_eq!(tab.foreground, resting);
            let hover = get(&tokens, query(variant, Slot::Trigger, VisualState::HOVERED));
            assert_eq!(hover.foreground, c(ColorRole::Foreground));
            let focused = get(
                &tokens,
                query(variant, Slot::Trigger, VisualState::FOCUS_VISIBLE),
            );
            assert_eq!(
                (focused.border, focused.ring),
                (c(ColorRole::Ring), focus_ring(&tokens))
            );
            let disabled = get(
                &tokens,
                query(variant, Slot::Trigger, VisualState::DISABLED),
            );
            assert_eq!(disabled.opacity, 0.5);
        }

        let selected = get(
            &tokens,
            query(TabsVariant::Default, Slot::Trigger, VisualState::SELECTED),
        );
        assert_eq!(
            (selected.foreground, selected.shadow),
            (c(ColorRole::Foreground), tokens.shadow(ShadowScale::Sm))
        );
        assert_eq!(
            (bg(&selected), selected.border),
            pick(
                &tokens,
                (c(ColorRole::Background), Color::TRANSPARENT),
                (c(ColorRole::Input).with_alpha(0.3), c(ColorRole::Input))
            )
        );
        let selected = get(
            &tokens,
            query(TabsVariant::Line, Slot::Trigger, VisualState::SELECTED),
        );
        assert_eq!(
            (selected.foreground, bg(&selected), selected.shadow),
            (
                c(ColorRole::Foreground),
                Color::TRANSPARENT,
                ShadowLayers::NONE
            )
        );
        // The line under the active tab, 5 pt outside it.
        for (variant, state, fill) in [
            (
                TabsVariant::Line,
                VisualState::SELECTED,
                c(ColorRole::Foreground),
            ),
            (TabsVariant::Line, VisualState::NONE, Color::TRANSPARENT),
            (
                TabsVariant::Default,
                VisualState::SELECTED,
                Color::TRANSPARENT,
            ),
        ] {
            let line = get(&tokens, query(variant, Slot::Indicator, state));
            assert_eq!(
                (bg(&line), line.min_height, line.padding),
                (fill, 2.0, Edges::axes(-4.0, -5.0)),
                "{} {state}",
                variant.name()
            );
        }
    }
}

#[test]
fn breadcrumb_matches_its_reference_table() {
    for tokens in schemes() {
        let query = |slot| StyleQuery::new(ControlKind::Breadcrumb, slot);
        let trail = get(&tokens, query(Slot::Root));
        text(
            &tokens,
            &trail,
            ColorRole::MutedForeground,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
        assert_eq!(trail.gap, 10.0);

        let link = get(&tokens, query(Slot::Link));
        text(
            &tokens,
            &link,
            ColorRole::MutedForeground,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
        assert_eq!(link.radius, radius(&tokens, RadiusScale::Sm));
        let hover = get(&tokens, query(Slot::Link).state(VisualState::HOVERED));
        assert_eq!(hover.foreground, tokens.color(ColorRole::Foreground));
        let focused = get(&tokens, query(Slot::Link).state(VisualState::FOCUS_VISIBLE));
        assert_eq!(focused.ring, focus_ring(&tokens));

        let page = get(&tokens, query(Slot::Item));
        text(
            &tokens,
            &page,
            ColorRole::Foreground,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
        let separator = get(&tokens, query(Slot::Separator));
        text(
            &tokens,
            &separator,
            ColorRole::MutedForeground,
            TextSize::Sm,
            FontWeight::NORMAL,
        );
        assert_eq!(separator.icon_size, 14.0);
        let ellipsis = get(&tokens, query(Slot::Ellipsis));
        assert_eq!(
            (ellipsis.min_width, ellipsis.min_height, ellipsis.icon_size),
            (20.0, 20.0, 16.0)
        );
    }
}

#[test]
fn pagination_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::Pagination, slot);
        assert_eq!(get(&tokens, query(Slot::Root)).gap, 4.0);

        let page = get(&tokens, query(Slot::Link));
        assert_eq!(
            (page.min_width, page.min_height, bg(&page), page.shadow),
            (36.0, 36.0, Color::TRANSPARENT, ShadowLayers::NONE)
        );
        // The current page is an outline button.
        let current = get(&tokens, query(Slot::Link).state(VisualState::SELECTED));
        assert_eq!(
            (current.border, current.shadow),
            (
                pick(&tokens, c(ColorRole::Border), c(ColorRole::Input)),
                tokens.shadow(ShadowScale::Xs)
            )
        );

        for (slot, padding) in [
            (
                Slot::Previous,
                Edges {
                    inline_start: 8.0,
                    ..Edges::axes(10.0, 0.0)
                },
            ),
            (
                Slot::Next,
                Edges {
                    inline_end: 8.0,
                    ..Edges::axes(10.0, 0.0)
                },
            ),
        ] {
            let button = get(&tokens, query(slot));
            assert_eq!(
                (button.min_height, button.padding, bg(&button)),
                (36.0, padding, Color::TRANSPARENT),
                "{}",
                slot.name()
            );
        }
        let ellipsis = get(&tokens, query(Slot::Ellipsis));
        assert_eq!(
            (ellipsis.min_width, ellipsis.min_height, ellipsis.icon_size),
            (36.0, 36.0, 16.0)
        );
    }
}

#[test]
fn navigation_menu_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::NavigationMenu, slot);
        // (slot, padding, weight)
        for (slot, padding, weight) in [
            (Slot::Trigger, Edges::axes(16.0, 8.0), FontWeight::MEDIUM),
            (Slot::Link, Edges::all(8.0), FontWeight::NORMAL),
        ] {
            let look = |state| get(&tokens, query(slot).state(state));
            let rest = look(VisualState::NONE);
            assert_eq!(
                (rest.padding, rest.text, rest.radius),
                (
                    padding,
                    tokens.text(TextSize::Sm, weight),
                    radius(&tokens, RadiusScale::Md)
                ),
                "{}",
                slot.name()
            );
            assert_eq!(
                (rest.min_height, rest.gap, rest.motion_ms, bg(&rest)),
                (36.0, 6.0, 150, Color::TRANSPARENT)
            );
            let muted = c(ColorRole::Muted);
            for (state, fill) in [
                (VisualState::OPEN, muted.with_alpha(0.5)),
                (VisualState::HOVERED, muted),
                (VisualState::FOCUSED, muted),
            ] {
                assert_eq!(bg(&look(state)), fill, "{} {state}", slot.name());
            }
            assert_eq!(look(VisualState::FOCUS_VISIBLE).ring, focus_ring(&tokens));
            assert_eq!(look(VisualState::DISABLED).opacity, 0.5);
        }
        let chevron = get(&tokens, query(Slot::Icon));
        assert_eq!((chevron.icon_size, chevron.motion_ms), (12.0, 300));

        let panel = get(&tokens, query(Slot::Content));
        assert_eq!(
            (bg(&panel), panel.radius, panel.shadow),
            (
                c(ColorRole::Popover),
                radius(&tokens, RadiusScale::Lg),
                tokens.shadow(ShadowScale::Sm)
            )
        );
        assert_eq!(
            panel.padding,
            Edges {
                inline_end: 10.0,
                ..Edges::all(8.0)
            }
        );
        let title = get(&tokens, query(Slot::Title));
        text(
            &tokens,
            &title,
            ColorRole::Foreground,
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
    }
}

#[test]
fn sidebar_matches_its_reference_table() {
    for tokens in schemes() {
        let c = |role| tokens.color(role);
        let query = |slot| StyleQuery::new(ControlKind::Sidebar, slot);
        let sidebar_ring = Some(Ring {
            width: 2.0,
            color: c(ColorRole::SidebarRing),
        });
        let border = Some(Ring {
            width: 1.0,
            color: c(ColorRole::SidebarBorder),
        });

        for (size, width) in [(ControlSize::Default, 256.0), (ControlSize::Icon, 48.0)] {
            let panel = get(&tokens, query(Slot::Root).size(size));
            assert_eq!(
                (
                    bg(&panel),
                    panel.foreground,
                    panel.min_width,
                    panel.motion_ms
                ),
                (
                    c(ColorRole::Sidebar),
                    c(ColorRole::SidebarForeground),
                    width,
                    200
                ),
                "{}",
                size.name()
            );
        }
        let panel = get(&tokens, query(Slot::Root));
        assert_eq!(
            (panel.border, panel.border_width, panel.border_sides),
            (c(ColorRole::SidebarBorder), 1.0, Sides::END)
        );
        let floating = get(&tokens, query(Slot::Root).variant(SidebarVariant::Floating));
        assert_eq!(
            (
                floating.ring,
                floating.radius,
                floating.shadow,
                floating.padding
            ),
            (
                border,
                radius(&tokens, RadiusScale::Lg),
                tokens.shadow(ShadowScale::Sm),
                Edges::all(8.0)
            )
        );
        let inset = get(&tokens, query(Slot::Root).variant(SidebarVariant::Inset));
        assert_eq!((inset.padding, inset.border_width), (Edges::all(8.0), 0.0));

        let header = get(&tokens, query(Slot::Header));
        assert_eq!((header.padding, header.gap), (Edges::all(8.0), 8.0));
        let group = get(&tokens, query(Slot::Group));
        assert_eq!((group.padding, group.gap), (Edges::all(8.0), 4.0));
        let caption = get(&tokens, query(Slot::GroupLabel));
        assert_eq!(
            (caption.foreground, caption.min_height, caption.text),
            (
                c(ColorRole::SidebarForeground).with_alpha(0.7),
                32.0,
                tokens.text(TextSize::Xs, FontWeight::MEDIUM)
            )
        );
        assert_eq!(
            (caption.radius, caption.padding),
            (radius(&tokens, RadiusScale::Md), Edges::axes(8.0, 0.0))
        );
        let focused = get(
            &tokens,
            query(Slot::GroupLabel).state(VisualState::FOCUS_VISIBLE),
        );
        assert_eq!(focused.ring, sidebar_ring);

        // (size, height, width, text)
        for (size, height, width, size_text) in [
            (ControlSize::Default, 32.0, 0.0, TextSize::Sm),
            (ControlSize::Sm, 28.0, 0.0, TextSize::Xs),
            (ControlSize::Lg, 48.0, 0.0, TextSize::Sm),
            (ControlSize::Icon, 32.0, 32.0, TextSize::Sm),
        ] {
            let item = get(&tokens, query(Slot::Item).size(size));
            assert_eq!(
                (item.min_height, item.min_width, item.text),
                (height, width, tokens.text(size_text, FontWeight::NORMAL)),
                "{}",
                size.name()
            );
            assert_eq!(
                (item.padding, item.radius, item.gap, item.foreground),
                (
                    Edges::all(8.0),
                    radius(&tokens, RadiusScale::Md),
                    8.0,
                    c(ColorRole::SidebarForeground)
                )
            );
        }
        let item = |state| get(&tokens, query(Slot::Item).state(state));
        let accent = (
            c(ColorRole::SidebarAccent),
            c(ColorRole::SidebarAccentForeground),
        );
        for state in [VisualState::HOVERED, VisualState::PRESSED] {
            let lit = item(state);
            assert_eq!((bg(&lit), lit.foreground), accent, "{state}");
        }
        let selected = item(VisualState::SELECTED);
        assert_eq!(
            (bg(&selected), selected.foreground, selected.text.weight),
            (accent.0, accent.1, FontWeight::MEDIUM)
        );
        assert_eq!(item(VisualState::FOCUS_VISIBLE).ring, sidebar_ring);
        assert_eq!(item(VisualState::DISABLED).opacity, 0.5);

        let outline = |state| {
            let query = query(Slot::Item).variant(SidebarItemVariant::Outline);
            get(&tokens, query.state(state))
        };
        let rest = outline(VisualState::NONE);
        assert_eq!((bg(&rest), rest.ring), (c(ColorRole::Background), border));
        let lit = outline(VisualState::HOVERED);
        assert_eq!(
            (bg(&lit), lit.ring),
            (
                accent.0,
                Some(Ring {
                    width: 1.0,
                    color: accent.0
                })
            )
        );

        let menu = get(&tokens, query(Slot::Row));
        assert_eq!(
            (
                menu.border,
                menu.border_width,
                menu.border_sides,
                menu.min_width,
                menu.padding,
                menu.gap
            ),
            (
                c(ColorRole::SidebarBorder),
                1.0,
                Sides::START,
                14.0,
                Edges::axes(10.0, 2.0),
                4.0
            )
        );
        for (size, size_text) in [
            (ControlSize::Default, TextSize::Sm),
            (ControlSize::Sm, TextSize::Xs),
        ] {
            let link = get(&tokens, query(Slot::Link).size(size));
            assert_eq!(
                (link.min_height, link.padding, link.text),
                (
                    28.0,
                    Edges::axes(8.0, 0.0),
                    tokens.text(size_text, FontWeight::NORMAL)
                ),
                "{}",
                size.name()
            );
        }

        let badge = get(&tokens, query(Slot::Badge));
        assert_eq!(
            (badge.foreground, badge.text, badge.min_height),
            (
                c(ColorRole::SidebarForeground),
                tokens.text(TextSize::Xs, FontWeight::MEDIUM),
                20.0
            )
        );
        let lit = get(&tokens, query(Slot::Badge).state(VisualState::SELECTED));
        assert_eq!(lit.foreground, c(ColorRole::SidebarAccentForeground));
    }
}

/// A command of a toolbar: a ghost icon-sm button.
fn assert_ghost_icon(tokens: &DesignTokens, query: StyleQuery) {
    let rest = get(tokens, query);
    assert_eq!(
        (rest.min_width, rest.min_height, bg(&rest)),
        (32.0, 32.0, Color::TRANSPARENT)
    );
    let hover = get(tokens, query.state(VisualState::HOVERED));
    let muted = tokens.color(ColorRole::Muted);
    assert_eq!(bg(&hover), pick(tokens, muted, muted.with_alpha(0.5)));
}

/// The 1 pt rule between the parts of a bar.
fn assert_bar_rule(tokens: &DesignTokens, kind: ControlKind) {
    let rule = get(tokens, StyleQuery::new(kind, Slot::Separator));
    assert_eq!(
        (bg(&rule), rule.min_width, rule.min_height, rule.padding),
        (
            tokens.color(ColorRole::Border),
            1.0,
            1.0,
            Edges::axes(4.0, 6.0)
        ),
        "{}",
        kind.name()
    );
}

#[test]
fn toolbar_matches_its_reference_table() {
    for tokens in schemes() {
        let query = |slot| StyleQuery::new(ControlKind::Toolbar, slot);
        let bar = get(&tokens, query(Slot::Root));
        assert_eq!(
            (bg(&bar), bar.border, bar.border_width, bar.border_sides),
            (
                tokens.color(ColorRole::Background),
                tokens.color(ColorRole::Border),
                1.0,
                Sides::BOTTOM
            )
        );
        assert_eq!(
            (bar.min_height, bar.padding, bar.gap),
            (40.0, Edges::axes(8.0, 4.0), 4.0)
        );
        assert_ghost_icon(&tokens, query(Slot::Item));
        assert_ghost_icon(&tokens, query(Slot::Action));
        assert_bar_rule(&tokens, ControlKind::Toolbar);
    }
}

#[test]
fn status_bar_matches_its_reference_table() {
    for tokens in schemes() {
        let bar = get(&tokens, StyleQuery::new(ControlKind::StatusBar, Slot::Root));
        text(
            &tokens,
            &bar,
            ColorRole::MutedForeground,
            TextSize::Xs,
            FontWeight::NORMAL,
        );
        assert_eq!(
            (bg(&bar), bar.border, bar.border_width, bar.border_sides),
            (
                tokens.color(ColorRole::Background),
                tokens.color(ColorRole::Border),
                1.0,
                Sides::TOP
            )
        );
        assert_eq!(
            (bar.min_height, bar.padding, bar.gap),
            (24.0, Edges::axes(12.0, 0.0), 12.0)
        );
        assert_bar_rule(&tokens, ControlKind::StatusBar);
    }
}

#[test]
fn dock_matches_its_reference_table() {
    for tokens in schemes() {
        let frame = get(&tokens, StyleQuery::new(ControlKind::Dock, Slot::Root));
        assert_eq!(
            (bg(&frame), frame.foreground),
            (
                tokens.color(ColorRole::Card),
                tokens.color(ColorRole::CardForeground)
            )
        );
        assert_eq!(
            (frame.border, frame.border_width, frame.border_sides),
            (tokens.color(ColorRole::Border), 1.0, Sides::ALL)
        );
    }
}
