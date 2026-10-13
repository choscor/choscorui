//! Feedback: alerts, toasts, progress, skeletons, spinners, and empty
//! states.

use super::{Cx, focus, muted_text, text};
use crate::style::{
    AlertVariant, ControlKind, Corners, Edges, EmptyMediaVariant, ResolvedStyle, Slot,
};
use choscorui_core::{ColorRole, FontWeight, RadiusScale, ShadowScale, TextSize, Tracking};

pub(super) fn resolve(cx: &Cx) -> ResolvedStyle {
    match cx.q.kind {
        ControlKind::Alert => alert(cx),
        ControlKind::Toast => toast(cx),
        ControlKind::Progress => progress(cx),
        ControlKind::Skeleton => skeleton(cx),
        ControlKind::Spinner => spinner(cx),
        _ => empty(cx),
    }
}

fn alert(cx: &Cx) -> ResolvedStyle {
    let destructive = AlertVariant::from_variant(cx.q.variant) == AlertVariant::Destructive;
    let tone = if destructive {
        ColorRole::Destructive
    } else {
        ColorRole::CardForeground
    };
    match cx.q.slot {
        Slot::Title => text(cx, tone, TextSize::Sm, FontWeight::MEDIUM),
        Slot::Description => {
            let mut style = muted_text(cx, TextSize::Sm);
            if destructive {
                style.foreground = cx.a(ColorRole::Destructive, 0.9);
            }
            style
        }
        Slot::Icon => {
            let mut style = cx.base();
            style.foreground = cx.c(tone);
            style.icon_size = 16.0;
            style.offset_y = cx.sp(0.5);
            style
        }
        Slot::Action => {
            let mut style = cx.base();
            style.padding = Edges {
                block_start: cx.sp(2.5),
                inline_end: cx.sp(3.0),
                ..Edges::ZERO
            };
            style
        }
        _ => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Card).into();
            style.foreground = cx.c(tone);
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.radius = Corners::all(cx.r(RadiusScale::Lg));
            style.padding = Edges::axes(cx.sp(4.0), cx.sp(3.0));
            style.gap = cx.sp(0.5);
            style
        }
    }
}

fn toast(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Title => text(
            cx,
            ColorRole::PopoverForeground,
            TextSize::Sm,
            FontWeight::MEDIUM,
        ),
        Slot::Description => muted_text(cx, TextSize::Sm),
        Slot::Icon => {
            let mut style = cx.base();
            style.foreground = cx.c(ColorRole::PopoverForeground);
            style.icon_size = 16.0;
            style
        }
        Slot::Close => {
            let mut style = muted_text(cx, TextSize::Sm);
            if cx.s().interactive_hover() {
                style.foreground = cx.c(ColorRole::Foreground);
            }
            style.icon_size = 16.0;
            style.radius = Corners::all(cx.r(RadiusScale::Sm));
            focus(&mut style, cx);
            style
        }
        Slot::Group => {
            // The stack: inset from the window corner, a gap and a peek.
            let mut style = cx.base();
            style.padding = Edges::all(cx.sp(4.0));
            style.gap = cx.sp(3.0);
            style.max_width = 384.0;
            style.motion_ms = cx.motion(500);
            style
        }
        _ => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Popover).into();
            style.foreground = cx.c(ColorRole::PopoverForeground);
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.radius = Corners::all(cx.r(RadiusScale::Xl2));
            style.shadow = cx.shadow(ShadowScale::Lg);
            style.padding = Edges::all(cx.sp(4.0));
            style.gap = cx.sp(3.0);
            style.max_width = 384.0;
            style.motion_ms = cx.motion(500);
            focus(&mut style, cx);
            style
        }
    }
}

fn progress(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Indicator | Slot::Range => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Primary).into();
            style.motion_ms = cx.motion(150);
            style
        }
        Slot::Label => text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::MEDIUM),
        Slot::Value => muted_text(cx, TextSize::Sm),
        Slot::Root => {
            let mut style = cx.base();
            style.gap = cx.sp(3.0);
            style
        }
        _ => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Muted).into();
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            style.min_height = cx.sp(1.5);
            style
        }
    }
}

fn skeleton(cx: &Cx) -> ResolvedStyle {
    let mut style = cx.base();
    style.background = cx.c(ColorRole::Muted).into();
    style.radius = Corners::all(cx.r(RadiusScale::Md));
    // The pulse: 2 s per cycle, still with reduced motion.
    style.motion_ms = cx.motion(2000);
    style
}

fn spinner(cx: &Cx) -> ResolvedStyle {
    let mut style = cx.base();
    style.icon_size = 16.0;
    // One turn per second, still with reduced motion.
    style.motion_ms = cx.motion(1000);
    style
}

fn empty(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Header => {
            let mut style = cx.base();
            style.gap = cx.sp(2.0);
            style.max_width = 384.0;
            style
        }
        Slot::Media => {
            let mut style = cx.base();
            style.padding.block_end = cx.sp(2.0);
            if EmptyMediaVariant::from_variant(cx.q.variant) == EmptyMediaVariant::Icon {
                style.background = cx.c(ColorRole::Muted).into();
                style.foreground = cx.c(ColorRole::Foreground);
                style.min_width = cx.sp(10.0);
                style.min_height = cx.sp(10.0);
                style.radius = Corners::all(cx.r(RadiusScale::Lg));
                style.icon_size = 24.0;
            }
            style
        }
        Slot::Title => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Lg, FontWeight::MEDIUM);
            style.text = style.text.with_tracking(Tracking::Tight);
            style
        }
        Slot::Description => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.text.line_height = style.text.size * 1.625;
            style
        }
        Slot::Content => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL);
            style.gap = cx.sp(4.0);
            style.max_width = 384.0;
            style
        }
        _ => {
            let mut style = cx.base();
            style.gap = cx.sp(4.0);
            style.radius = Corners::all(cx.r(RadiusScale::Lg));
            style.padding = Edges::all(cx.sp(12.0));
            style
        }
    }
}
