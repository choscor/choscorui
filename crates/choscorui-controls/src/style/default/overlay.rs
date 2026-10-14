//! Overlays: dialogs, sheets, drawers, popovers, tooltips, menus, and the
//! command palette.

use super::{Cx, disabled, group_label, list_item, muted_text, rule, surface, text};
use crate::style::{
    ControlKind, Corners, Edges, MenuItemVariant, ResolvedStyle, SheetSide, Sides, Slot,
};
use choscorui_core::{Color, ColorRole, FontWeight, RadiusScale, ShadowScale, TextSize, Tracking};

pub(super) fn resolve(cx: &Cx) -> ResolvedStyle {
    match cx.q.kind {
        ControlKind::AlertDialog | ControlKind::Dialog => dialog(cx),
        ControlKind::Sheet | ControlKind::Drawer => sheet(cx),
        ControlKind::Tooltip => tooltip(cx),
        ControlKind::Menu | ControlKind::Menubar => menu(cx),
        ControlKind::Command => command(cx),
        _ => popover(cx),
    }
}

/// The dimmed, blurred layer behind a modal overlay.
fn scrim(cx: &Cx) -> ResolvedStyle {
    let mut style = cx.base();
    style.background = Color::BLACK.with_alpha(0.1).into();
    style.backdrop_blur = 4.0;
    style.motion_ms = cx.motion(100);
    style
}

fn heading(cx: &Cx, size: TextSize) -> ResolvedStyle {
    text(cx, ColorRole::Foreground, size, FontWeight::MEDIUM)
}

fn dialog(cx: &Cx) -> ResolvedStyle {
    let alert = cx.q.kind == ControlKind::AlertDialog;
    match cx.q.slot {
        Slot::Overlay => scrim(cx),
        Slot::Header => {
            let mut style = cx.base();
            style.gap = cx.sp(if alert { 1.5 } else { 2.0 });
            style
        }
        Slot::Footer => {
            let mut style = cx.base();
            style.gap = cx.sp(2.0);
            style
        }
        Slot::Title => {
            let mut style = heading(cx, if alert { TextSize::Lg } else { TextSize::Base });
            if !alert {
                style.text.line_height = style.text.size;
            }
            style
        }
        Slot::Description => muted_text(cx, TextSize::Sm),
        Slot::Media => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Muted).into();
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.min_width = cx.sp(16.0);
            style.min_height = cx.sp(16.0);
            style.padding.block_end = cx.sp(2.0);
            style.icon_size = 32.0;
            style
        }
        Slot::Close => {
            let mut style = super::form::button(
                cx,
                crate::style::ButtonVariant::Ghost,
                crate::style::ControlSize::IconSm,
            );
            style.padding = Edges::all(cx.sp(4.0));
            style
        }
        _ => {
            let mut style = surface(cx, RadiusScale::Xl);
            style.shadow = choscorui_core::ShadowLayers::NONE;
            style.gap = cx.sp(6.0);
            style.padding = Edges::all(cx.sp(6.0));
            style.max_width = if alert {
                if cx.q.size == crate::style::ControlSize::Sm {
                    320.0
                } else {
                    512.0
                }
            } else {
                448.0
            };
            style
        }
    }
}

fn sheet(cx: &Cx) -> ResolvedStyle {
    let drawer = cx.q.kind == ControlKind::Drawer;
    let side = SheetSide::from_variant(cx.q.variant);
    match cx.q.slot {
        Slot::Overlay => {
            let mut style = scrim(cx);
            style.motion_ms = cx.motion(if drawer { 450 } else { 150 });
            style
        }
        Slot::Header => {
            let mut style = cx.base();
            style.gap = cx.sp(1.5);
            style.padding = Edges::all(cx.sp(4.0));
            style
        }
        Slot::Footer => {
            let mut style = cx.base();
            style.gap = cx.sp(2.0);
            style.padding = Edges::all(cx.sp(4.0));
            style
        }
        Slot::Title => heading(cx, TextSize::Base),
        Slot::Description => muted_text(cx, TextSize::Sm),
        Slot::Handle => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Muted).into();
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            style.min_width = 100.0;
            style.min_height = cx.sp(1.5);
            style.padding.block_start = cx.sp(4.0);
            style
        }
        Slot::Close => {
            let mut style = super::form::button(
                cx,
                crate::style::ButtonVariant::Ghost,
                crate::style::ControlSize::IconSm,
            );
            style.padding = Edges::all(cx.sp(4.0));
            style
        }
        _ => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Popover).into();
            style.foreground = cx.c(ColorRole::PopoverForeground);
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.gap = cx.sp(4.0);
            let xl = cx.r(RadiusScale::Xl);
            let (sides, radius) = match side {
                SheetSide::Right => (Sides::START, Corners::all(xl).square_end()),
                SheetSide::Left => (Sides::END, Corners::all(xl).square_start()),
                SheetSide::Top => (Sides::BOTTOM, Corners::all(xl).square_top()),
                SheetSide::Bottom => (Sides::TOP, Corners::all(xl).square_bottom()),
            };
            style.border_sides = sides;
            if drawer {
                style.radius = radius;
                style.motion_ms = cx.motion(450);
            } else {
                style.shadow = cx.shadow(ShadowScale::Lg);
                style.motion_ms = cx.motion(200);
                if matches!(side, SheetSide::Left | SheetSide::Right) {
                    style.max_width = 384.0;
                }
            }
            style
        }
    }
}

fn popover(cx: &Cx) -> ResolvedStyle {
    let hover_card = cx.q.kind == ControlKind::HoverCard;
    match cx.q.slot {
        Slot::Header => {
            let mut style = cx.base();
            style.gap = cx.sp(1.0);
            style
        }
        Slot::Title => heading(cx, TextSize::Sm),
        Slot::Description => muted_text(cx, TextSize::Sm),
        _ => {
            let mut style = surface(
                cx,
                if hover_card {
                    RadiusScale::Lg
                } else {
                    RadiusScale::Md
                },
            );
            style.gap = cx.sp(4.0);
            style.padding = Edges::all(cx.sp(4.0));
            style.min_width = if hover_card { 256.0 } else { 288.0 };
            style.max_width = style.min_width;
            style
        }
    }
}

fn tooltip(cx: &Cx) -> ResolvedStyle {
    let mut style = cx.base();
    style.background = cx.c(ColorRole::Foreground).into();
    style.foreground = cx.c(ColorRole::Background);
    match cx.q.slot {
        Slot::Indicator => {
            style.min_width = cx.sp(2.5);
            style.min_height = cx.sp(2.5);
        }
        _ => {
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.padding = Edges::axes(cx.sp(3.0), cx.sp(1.5));
            style.gap = cx.sp(1.5);
            style.text = cx.text(TextSize::Xs, FontWeight::NORMAL);
            style.max_width = 320.0;
            style.motion_ms = cx.motion(100);
        }
    }
    style
}

fn shortcut(cx: &Cx, highlight: ColorRole) -> ResolvedStyle {
    let mut style = muted_text(cx, TextSize::Xs);
    style.text = style.text.with_tracking(Tracking::Widest);
    if cx.s().highlighted() {
        style.foreground = cx.c(highlight);
    }
    style
}

fn menu(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    let bar = cx.q.kind == ControlKind::Menubar;
    match cx.q.slot {
        Slot::Root if bar => {
            let mut style = cx.base();
            style.min_height = cx.sp(9.0);
            style.gap = cx.sp(1.0);
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.padding = Edges::all(cx.sp(1.0));
            style.shadow = cx.shadow(ShadowScale::Xs);
            style.background = cx.c(ColorRole::Background).into();
            style
        }
        Slot::Trigger if bar => {
            let mut style = cx.base();
            style.radius = Corners::all(cx.r(RadiusScale::Sm));
            style.padding = Edges::axes(cx.sp(2.0), cx.sp(1.0));
            style.text = cx.text(TextSize::Sm, FontWeight::MEDIUM);
            if s.interactive_hover() || s.open() || s.highlighted() {
                style.background = cx.c(ColorRole::Muted).into();
            }
            super::focus(&mut style, cx);
            disabled(&mut style, cx);
            style
        }
        Slot::Item | Slot::Trigger => {
            let destructive =
                MenuItemVariant::from_variant(cx.q.variant) == MenuItemVariant::Destructive;
            let mut style = list_item(cx, destructive);
            if cx.q.slot == Slot::Trigger && s.open() {
                style.background = cx.c(ColorRole::Accent).into();
                style.foreground = cx.c(ColorRole::AccentForeground);
            }
            style
        }
        Slot::ItemIndicator => {
            let mut style = cx.base();
            style.min_width = cx.sp(4.0);
            style.icon_size = 16.0;
            style.foreground = if s.highlighted() {
                cx.c(ColorRole::AccentForeground)
            } else {
                cx.c(ColorRole::PopoverForeground)
            };
            style
        }
        Slot::GroupLabel => {
            let mut style = group_label(cx);
            style.text.weight = FontWeight::MEDIUM;
            if bar {
                style.foreground = cx.c(ColorRole::Foreground);
                style.text = cx.text(TextSize::Sm, FontWeight::MEDIUM);
            }
            style
        }
        Slot::Separator => rule(cx, cx.sp(1.0)),
        Slot::Shortcut => shortcut(cx, ColorRole::AccentForeground),
        // The area that opens a context menu.
        Slot::Placeholder => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.min_width = 300.0;
            style.min_height = 150.0;
            super::focus(&mut style, cx);
            style
        }
        Slot::Panel => {
            let mut style = surface(cx, RadiusScale::Md);
            style.padding = Edges::all(cx.sp(1.0));
            style.shadow = cx.shadow(ShadowScale::Lg);
            style.min_width = cx.sp(24.0);
            style
        }
        _ => {
            let mut style = surface(cx, RadiusScale::Md);
            style.padding = Edges::all(cx.sp(1.0));
            style.min_width = cx.sp(if bar { 36.0 } else { 32.0 });
            style
        }
    }
}

fn command(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Input => {
            let mut style = cx.base();
            style.background = cx.a(ColorRole::Input, 0.3).into();
            style.border = cx.a(ColorRole::Input, 0.3);
            style.border_width = cx.t.border_width;
            style.radius = Corners::all(cx.r(RadiusScale::Lg));
            style.min_height = cx.sp(8.0);
            style.padding = Edges::axes(cx.sp(2.0), 0.0);
            style.gap = cx.sp(2.0);
            style.cursor = crate::style::Cursor::Text;
            style
        }
        Slot::Icon => {
            let mut style = cx.base();
            style.opacity = 0.5;
            style.icon_size = 16.0;
            style
        }
        Slot::Placeholder => muted_text(cx, TextSize::Sm),
        Slot::Item => {
            let mut style = list_item(cx, false);
            if s.highlighted() || s.selected() {
                style.background = cx.c(ColorRole::Muted).into();
                style.foreground = cx.c(ColorRole::Foreground);
            }
            if cx.q.size == crate::style::ControlSize::Lg {
                style.radius = Corners::all(cx.r(RadiusScale::Lg));
            }
            style
        }
        Slot::GroupLabel => {
            let mut style = group_label(cx);
            style.text.weight = FontWeight::MEDIUM;
            style
        }
        Slot::Group => {
            let mut style = cx.base();
            style.padding = Edges::all(cx.sp(1.0));
            style
        }
        Slot::Separator => rule(cx, 0.0),
        Slot::Shortcut => shortcut(cx, ColorRole::Foreground),
        Slot::Description => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL);
            style.padding = Edges::axes(0.0, cx.sp(6.0));
            style
        }
        Slot::Overlay => scrim(cx),
        _ => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Popover).into();
            style.foreground = cx.c(ColorRole::PopoverForeground);
            style.radius = Corners::all(cx.r(RadiusScale::Xl));
            style.padding = Edges::all(cx.sp(1.0));
            if cx.q.size == crate::style::ControlSize::Lg {
                style.ring = cx.ring(1.0, cx.a(ColorRole::Foreground, 0.1));
                style.max_width = 448.0;
            }
            style
        }
    }
}
