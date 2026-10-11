//! Data display: tables, cards, badges, avatars, charts, calendars,
//! carousels, and the repainted item views and gauges.

use super::{Cx, disabled, focus, muted_text, text};
use crate::style::{
    BadgeVariant, ButtonVariant, CalendarDay, ControlKind, ControlSize, Corners, Edges,
    ResolvedStyle, Sides, Slot,
};
use choscorui_core::{ColorRole, FontWeight, RadiusScale, ShadowScale, TextSize};

pub(super) fn resolve(cx: &Cx) -> ResolvedStyle {
    match cx.q.kind {
        ControlKind::Table | ControlKind::ListView => table(cx),
        ControlKind::Card => card(cx),
        ControlKind::Badge => badge(cx, BadgeVariant::from_variant(cx.q.variant)),
        ControlKind::Avatar => avatar(cx),
        ControlKind::Chart => chart(cx),
        ControlKind::Calendar => calendar(cx),
        ControlKind::Carousel => carousel(cx),
        _ => gauge(cx),
    }
}

fn table(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Row | Slot::Item => {
            let mut style = cx.base();
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.border_sides = Sides::BOTTOM;
            if cx.q.kind == ControlKind::ListView {
                style.border_sides = Sides::NONE;
                style.radius = Corners::all(cx.r(RadiusScale::Sm));
                style.padding = Edges::axes(cx.sp(2.0), 0.0);
                style.min_height = cx.sp(8.0);
            }
            if s.interactive_hover() {
                style.background = cx.a(ColorRole::Muted, 0.5);
            }
            if s.selected() {
                style.background = cx.c(ColorRole::Muted);
            }
            if s.highlighted() && cx.q.kind == ControlKind::ListView {
                style.background = cx.c(ColorRole::Accent);
                style.foreground = cx.c(ColorRole::AccentForeground);
            }
            style.motion_ms = cx.motion(150);
            style
        }
        Slot::HeaderCell => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::MEDIUM);
            style.min_height = cx.sp(10.0);
            style.padding = Edges::axes(cx.sp(2.0), 0.0);
            if s.interactive_hover() {
                style.background = cx.a(ColorRole::Muted, 0.5);
            }
            style
        }
        Slot::Header => {
            let mut style = cx.base();
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.border_sides = Sides::BOTTOM;
            style
        }
        Slot::Cell => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL);
            style.padding = Edges::all(cx.sp(2.0));
            style
        }
        Slot::Footer => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::MEDIUM);
            style.background = cx.a(ColorRole::Muted, 0.5);
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.border_sides = Sides::TOP;
            style
        }
        Slot::Caption => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.padding.block_start = cx.sp(4.0);
            style
        }
        Slot::Indicator => {
            let mut style = cx.base();
            style.foreground = cx.c(ColorRole::MutedForeground);
            style.icon_size = 16.0;
            style
        }
        _ => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL);
            focus(&mut style, cx);
            if s.focus_visible() {
                style.border = choscorui_core::Color::TRANSPARENT;
            }
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            disabled(&mut style, cx);
            style
        }
    }
}

fn card(cx: &Cx) -> ResolvedStyle {
    let spacing = cx.sp(if cx.q.size == ControlSize::Sm {
        4.0
    } else {
        6.0
    });
    match cx.q.slot {
        Slot::Header => {
            let mut style = cx.base();
            style.gap = cx.sp(1.0);
            style.padding = Edges::axes(spacing, 0.0);
            style
        }
        Slot::Title => {
            let size = if cx.q.size == ControlSize::Sm {
                TextSize::Sm
            } else {
                TextSize::Base
            };
            let mut style = text(cx, ColorRole::CardForeground, size, FontWeight::MEDIUM);
            style.text.line_height = style.text.size * 1.5;
            style
        }
        Slot::Description => muted_text(cx, TextSize::Sm),
        Slot::Content | Slot::Footer | Slot::Action => {
            let mut style = cx.base();
            style.foreground = cx.c(ColorRole::CardForeground);
            style.padding = Edges::axes(spacing, 0.0);
            style.gap = cx.sp(3.0);
            style
        }
        _ => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Card);
            style.foreground = cx.c(ColorRole::CardForeground);
            style.ring = cx.ring(1.0, cx.a(ColorRole::Foreground, 0.1));
            style.radius = Corners::all(cx.r(RadiusScale::Xl));
            style.shadow = cx.shadow(ShadowScale::Xs);
            style.gap = spacing;
            style.padding = Edges::axes(0.0, spacing);
            style
        }
    }
}

/// The badge look; avatars and sidebars reuse it.
pub(crate) fn badge(cx: &Cx, variant: BadgeVariant) -> ResolvedStyle {
    let s = cx.s();
    let hover = s.interactive_hover();
    let mut style = cx.base();
    style.min_height = cx.sp(5.0);
    style.gap = cx.sp(1.0);
    style.radius = Corners::all(cx.r(RadiusScale::Xl4));
    style.border_width = cx.t.border_width;
    style.padding = Edges::axes(cx.sp(2.0), cx.sp(0.5));
    style.text = cx.text(TextSize::Xs, FontWeight::MEDIUM);
    style.icon_size = 12.0;
    match variant {
        BadgeVariant::Default => {
            style.background = cx.a(ColorRole::Primary, if hover { 0.8 } else { 1.0 });
            style.foreground = cx.c(ColorRole::PrimaryForeground);
        }
        BadgeVariant::Secondary => {
            style.background = cx.a(ColorRole::Secondary, if hover { 0.8 } else { 1.0 });
            style.foreground = cx.c(ColorRole::SecondaryForeground);
        }
        BadgeVariant::Outline => {
            style.border = cx.c(ColorRole::Border);
            if hover {
                style.background = cx.c(ColorRole::Muted);
                style.foreground = cx.c(ColorRole::MutedForeground);
            }
        }
        BadgeVariant::Destructive => {
            let alpha = match (cx.dark(), hover) {
                (false, false) => 0.1,
                (true, true) => 0.3,
                _ => 0.2,
            };
            style.background = cx.a(ColorRole::Destructive, alpha);
            style.foreground = cx.c(ColorRole::Destructive);
        }
        BadgeVariant::Ghost => {
            if hover {
                style.background = cx.pick(cx.c(ColorRole::Muted), cx.a(ColorRole::Muted, 0.5));
                style.foreground = cx.c(ColorRole::MutedForeground);
            }
        }
        BadgeVariant::Link => {
            style.foreground = cx.c(ColorRole::Primary);
            style.underline = hover;
        }
    }
    focus(&mut style, cx);
    if variant == BadgeVariant::Destructive && s.focus_visible() {
        style.ring = cx.ring(
            cx.t.ring_width,
            cx.a(ColorRole::Destructive, cx.pick(0.2, 0.4)),
        );
    }
    super::invalid(&mut style, cx);
    style
}

fn avatar(cx: &Cx) -> ResolvedStyle {
    let size = match cx.q.size {
        ControlSize::Sm => 6.0,
        ControlSize::Lg => 10.0,
        _ => 8.0,
    };
    let mut style = cx.base();
    style.radius = Corners::all(cx.r(RadiusScale::Full));
    match cx.q.slot {
        Slot::Fallback | Slot::Group => {
            style.background = cx.c(ColorRole::Muted);
            style.foreground = cx.c(ColorRole::MutedForeground);
            style.min_width = cx.sp(size);
            style.min_height = cx.sp(size);
            style.text = cx.text(
                if cx.q.size == ControlSize::Sm {
                    TextSize::Xs
                } else {
                    TextSize::Sm
                },
                FontWeight::NORMAL,
            );
            style.icon_size = match cx.q.size {
                ControlSize::Sm => 12.0,
                ControlSize::Lg => 20.0,
                _ => 16.0,
            };
        }
        Slot::Badge => {
            style.background = cx.c(ColorRole::Primary);
            style.foreground = cx.c(ColorRole::PrimaryForeground);
            style.ring = cx.ring(2.0, cx.c(ColorRole::Background));
            let badge = match cx.q.size {
                ControlSize::Sm => 2.0,
                ControlSize::Lg => 3.0,
                _ => 2.5,
            };
            style.min_width = cx.sp(badge);
            style.min_height = cx.sp(badge);
            style.icon_size = if cx.q.size == ControlSize::Sm {
                0.0
            } else {
                8.0
            };
        }
        _ => {
            style.min_width = cx.sp(size);
            style.min_height = cx.sp(size);
            style.ring = cx.ring(2.0, cx.c(ColorRole::Background));
            style.gap = -cx.sp(2.0);
        }
    }
    style
}

fn chart(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Content => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Background);
            style.border = cx.a(ColorRole::Border, 0.5);
            style.border_width = cx.t.border_width;
            style.radius = Corners::all(cx.r(RadiusScale::Lg));
            style.padding = Edges::axes(cx.sp(2.5), cx.sp(1.5));
            style.gap = cx.sp(1.5);
            style.text = cx.text(TextSize::Xs, FontWeight::NORMAL);
            style.shadow = cx.shadow(ShadowScale::Xl);
            style
        }
        Slot::Legend | Slot::Label => {
            let mut style = muted_text(cx, TextSize::Xs);
            style.gap = cx.sp(1.5);
            style
        }
        Slot::Value => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Xs, FontWeight::MEDIUM);
            style.text.weight = FontWeight::MEDIUM;
            style
        }
        Slot::Track => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Border);
            style.min_height = 1.0;
            style
        }
        Slot::Indicator => {
            let mut style = cx.base();
            style.radius = Corners::all(2.0);
            style.min_width = cx.sp(2.5);
            style.min_height = cx.sp(2.5);
            style
        }
        _ => {
            let mut style = muted_text(cx, TextSize::Xs);
            style.min_height = 200.0;
            style
        }
    }
}

fn calendar(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    let cell = cx.sp(8.0);
    let cell_radius = cx.r(RadiusScale::Md);
    match cx.q.slot {
        Slot::Day => {
            let mut style = super::form::button(cx, ButtonVariant::Ghost, ControlSize::IconSm);
            style.min_width = cell;
            style.min_height = cell;
            style.radius = Corners::all(cell_radius);
            style.text = cx.text(TextSize::Sm, FontWeight::NORMAL);
            style.text.line_height = style.text.size;
            style.offset_y = 0.0;
            match CalendarDay::from_variant(cx.q.variant) {
                CalendarDay::Today => {
                    style.background = cx.c(ColorRole::Muted);
                    style.foreground = cx.c(ColorRole::Foreground);
                }
                CalendarDay::Outside => style.foreground = cx.c(ColorRole::MutedForeground),
                CalendarDay::RangeMiddle => {
                    style.background = cx.c(ColorRole::Muted);
                    style.foreground = cx.c(ColorRole::Foreground);
                    style.radius = Corners::ZERO;
                }
                CalendarDay::RangeStart => {
                    style.background = cx.c(ColorRole::Primary);
                    style.foreground = cx.c(ColorRole::PrimaryForeground);
                }
                CalendarDay::RangeEnd => {
                    style.background = cx.c(ColorRole::Primary);
                    style.foreground = cx.c(ColorRole::PrimaryForeground);
                }
                CalendarDay::Default => {}
            }
            if s.selected() {
                style.background = cx.c(ColorRole::Primary);
                style.foreground = cx.c(ColorRole::PrimaryForeground);
            }
            if s.focus_visible() {
                style.border = cx.c(ColorRole::Ring);
                style.ring = cx.focus_ring();
            }
            if s.disabled() {
                style.foreground = cx.c(ColorRole::MutedForeground);
                style.opacity = 0.5;
            }
            style
        }
        Slot::Range => {
            // The muted band behind a range endpoint.
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Muted);
            style.radius = Corners::all(cell_radius);
            style
        }
        Slot::Weekday => {
            let mut style = muted_text(cx, TextSize::Xs);
            style.text.size = 12.8 * cx.t.text_scale;
            style.radius = Corners::all(cell_radius);
            style.min_height = cell;
            style
        }
        Slot::Caption => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::MEDIUM);
            style.min_height = cell;
            style.padding = Edges::axes(cell, 0.0);
            style.gap = cx.sp(1.5);
            style
        }
        Slot::Trigger => {
            let mut style = super::field_box(cx);
            style.background = choscorui_core::Color::TRANSPARENT;
            style.radius = Corners::all(cell_radius);
            style.min_height = cell;
            style.padding = Edges {
                inline_start: cx.sp(2.0),
                inline_end: cx.sp(1.0),
                ..Edges::ZERO
            };
            style.text = cx.text(TextSize::Sm, FontWeight::MEDIUM);
            if s.focused() {
                style.border = cx.c(ColorRole::Ring);
                style.ring = cx.focus_ring();
            }
            style
        }
        Slot::Previous | Slot::Next => {
            let mut style = super::form::button(cx, ButtonVariant::Ghost, ControlSize::IconSm);
            style.min_width = cell;
            style.min_height = cell;
            style.padding = Edges::ZERO;
            style
        }
        _ => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Background);
            style.padding = Edges::all(cx.sp(3.0));
            style.gap = cx.sp(4.0);
            style.min_width = cell * 7.0 + cx.sp(6.0);
            style
        }
    }
}

fn carousel(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Previous | Slot::Next => {
            let mut style = super::form::button(cx, ButtonVariant::Outline, ControlSize::IconSm);
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            style
        }
        Slot::Item => {
            let mut style = cx.base();
            style.padding.inline_start = cx.sp(4.0);
            style
        }
        _ => {
            let mut style = cx.base();
            style.gap = cx.sp(4.0);
            style.motion_ms = cx.motion(250);
            style
        }
    }
}

/// The dial, numeric display, and color picker, which have no reference
/// counterpart: they use the input box, muted tracks, and the primary
/// color.
fn gauge(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Track => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Muted);
            style.min_width = cx.sp(1.5);
            style
        }
        Slot::Range | Slot::Indicator => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Primary);
            style.foreground = cx.c(ColorRole::Primary);
            style.min_width = cx.sp(1.5);
            disabled(&mut style, cx);
            style
        }
        Slot::Thumb => {
            let mut style = cx.base();
            style.background = choscorui_core::Color::WHITE;
            style.border = cx.c(ColorRole::Primary);
            style.border_width = cx.t.border_width;
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            style.min_width = cx.sp(4.0);
            style.min_height = cx.sp(4.0);
            style.shadow = cx.shadow(ShadowScale::Sm);
            if s.interactive_hover() || s.focus_visible() {
                style.ring = cx.ring(4.0, cx.t.ring_color(cx.c(ColorRole::Ring)));
            }
            style
        }
        Slot::Value => {
            let mut style = cx.base();
            style.text = cx.t.mono(TextSize::Xl2, FontWeight::MEDIUM);
            style.foreground = cx.c(ColorRole::Foreground);
            style
        }
        Slot::Label => muted_text(cx, TextSize::Xs),
        _ => {
            let mut style = super::field_box(cx);
            if cx.q.kind == ControlKind::Dial {
                style.background = choscorui_core::Color::TRANSPARENT;
                style.border = choscorui_core::Color::TRANSPARENT;
                style.shadow = choscorui_core::ShadowLayers::NONE;
                style.radius = Corners::all(cx.r(RadiusScale::Full));
            } else {
                style.radius = Corners::all(cx.r(RadiusScale::Lg));
                style.padding = Edges::all(cx.sp(3.0));
            }
            super::states(&mut style, cx);
            style
        }
    }
}
