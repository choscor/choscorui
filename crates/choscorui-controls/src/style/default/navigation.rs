//! Navigation: tabs, breadcrumbs, pagination, the navigation menu, the
//! sidebar, and the window chrome bars.

use super::{Cx, disabled, focus, muted_text, surface, text};
use crate::style::{
    ButtonVariant, ControlKind, ControlSize, Corners, Edges, ResolvedStyle, SidebarItemVariant,
    SidebarVariant, Sides, Slot, TabsVariant,
};
use choscorui_core::{ColorRole, FontWeight, RadiusScale, ShadowScale, TextSize};

pub(super) fn resolve(cx: &Cx) -> ResolvedStyle {
    match cx.q.kind {
        ControlKind::Tabs => tabs(cx),
        ControlKind::Breadcrumb => breadcrumb(cx),
        ControlKind::Pagination => pagination(cx),
        ControlKind::NavigationMenu => navigation_menu(cx),
        ControlKind::Sidebar => sidebar(cx),
        _ => chrome(cx),
    }
}

fn tabs(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    let line = TabsVariant::from_variant(cx.q.variant) == TabsVariant::Line;
    match cx.q.slot {
        Slot::Root => {
            let mut style = cx.base();
            style.gap = cx.sp(2.0);
            style
        }
        Slot::Group => {
            // The tab list.
            let mut style = cx.base();
            style.foreground = cx.c(ColorRole::MutedForeground);
            style.padding = Edges::all(3.0);
            style.min_height = cx.sp(9.0);
            if line {
                style.gap = cx.sp(1.0);
            } else {
                style.background = cx.c(ColorRole::Muted).into();
                style.radius = Corners::all(cx.r(RadiusScale::Lg));
            }
            style
        }
        Slot::Trigger => {
            let mut style = cx.base();
            style.gap = cx.sp(1.5);
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.border_width = cx.t.border_width;
            style.padding = Edges::axes(cx.sp(2.0), cx.sp(1.0));
            style.text = cx.text(TextSize::Sm, FontWeight::MEDIUM);
            style.foreground = cx.pick(
                cx.a(ColorRole::Foreground, 0.6),
                cx.c(ColorRole::MutedForeground),
            );
            if s.interactive_hover() {
                style.foreground = cx.c(ColorRole::Foreground);
            }
            if s.selected() {
                style.foreground = cx.c(ColorRole::Foreground);
                if !line {
                    style.background = cx
                        .pick(cx.c(ColorRole::Background), cx.a(ColorRole::Input, 0.3))
                        .into();
                    if cx.dark() {
                        style.border = cx.c(ColorRole::Input);
                    }
                    style.shadow = cx.shadow(ShadowScale::Sm);
                }
            }
            focus(&mut style, cx);
            disabled(&mut style, cx);
            style
        }
        Slot::Indicator => {
            // The underline of the active tab in the line variant.
            let mut style = cx.base();
            if line && s.selected() {
                style.background = cx.c(ColorRole::Foreground).into();
            }
            style.min_height = 2.0;
            style.min_width = 2.0;
            // The negative padding places the line outside the tab: 5 pt
            // under a horizontal tab, 4 pt beyond a vertical one.
            style.padding = Edges::axes(-4.0, -5.0);
            style
        }
        _ => text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL),
    }
}

fn breadcrumb(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Link => {
            let mut style = muted_text(cx, TextSize::Sm);
            if s.interactive_hover() {
                style.foreground = cx.c(ColorRole::Foreground);
            }
            focus(&mut style, cx);
            style.radius = Corners::all(cx.r(RadiusScale::Sm));
            style.cursor = crate::style::Cursor::Pointer;
            style
        }
        Slot::Item => text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL),
        Slot::Separator => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.icon_size = 14.0;
            style
        }
        Slot::Ellipsis => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.min_width = cx.sp(5.0);
            style.min_height = cx.sp(5.0);
            style.icon_size = 16.0;
            style.radius = Corners::all(cx.r(RadiusScale::Sm));
            if s.interactive_hover() {
                style.foreground = cx.c(ColorRole::Foreground);
            }
            focus(&mut style, cx);
            style
        }
        _ => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.gap = cx.sp(2.5);
            style
        }
    }
}

fn pagination(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Link => {
            let variant = if cx.s().selected() {
                ButtonVariant::Outline
            } else {
                ButtonVariant::Ghost
            };
            super::form::button(cx, variant, ControlSize::Icon)
        }
        Slot::Previous | Slot::Next => {
            let mut style = super::form::button(cx, ButtonVariant::Ghost, ControlSize::Default);
            if cx.q.slot == Slot::Previous {
                style.padding.inline_start = cx.sp(2.0);
            } else {
                style.padding.inline_end = cx.sp(2.0);
            }
            style
        }
        Slot::Ellipsis => {
            let mut style = cx.base();
            style.min_width = cx.sp(9.0);
            style.min_height = cx.sp(9.0);
            style.icon_size = 16.0;
            style
        }
        _ => {
            let mut style = cx.base();
            style.gap = cx.sp(1.0);
            style
        }
    }
}

fn navigation_menu(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Trigger | Slot::Link => {
            let mut style = cx.base();
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.text = cx.text(
                TextSize::Sm,
                if cx.q.slot == Slot::Trigger {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                },
            );
            style.padding = if cx.q.slot == Slot::Trigger {
                Edges::axes(cx.sp(4.0), cx.sp(2.0))
            } else {
                Edges::all(cx.sp(2.0))
            };
            style.gap = cx.sp(1.5);
            style.min_height = cx.sp(9.0);
            if s.open() || s.selected() {
                style.background = cx.a(ColorRole::Muted, 0.5).into();
            }
            if s.interactive_hover() || s.focused() {
                style.background = cx.c(ColorRole::Muted).into();
            }
            if s.focus_visible() {
                style.ring = cx.focus_ring();
            }
            style.motion_ms = cx.motion(150);
            disabled(&mut style, cx);
            style
        }
        Slot::Icon => {
            let mut style = cx.base();
            style.icon_size = 12.0;
            style.motion_ms = cx.motion(300);
            style
        }
        Slot::Content | Slot::Viewport => {
            let mut style = surface(cx, RadiusScale::Lg);
            style.shadow = cx.shadow(ShadowScale::Sm);
            style.padding = Edges {
                inline_end: cx.sp(2.5),
                ..Edges::all(cx.sp(2.0))
            };
            style
        }
        Slot::Title => text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::MEDIUM),
        Slot::Description => muted_text(cx, TextSize::Sm),
        _ => cx.base(),
    }
}

fn sidebar_ring(cx: &Cx, style: &mut ResolvedStyle) {
    if cx.s().focus_visible() {
        style.ring = cx.ring(2.0, cx.c(ColorRole::SidebarRing));
    }
}

fn sidebar(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    let variant = SidebarVariant::from_variant(cx.q.variant);
    let accent = |style: &mut ResolvedStyle| {
        style.background = cx.c(ColorRole::SidebarAccent).into();
        style.foreground = cx.c(ColorRole::SidebarAccentForeground);
    };
    match cx.q.slot {
        Slot::Header | Slot::Footer => {
            let mut style = cx.base();
            style.gap = cx.sp(2.0);
            style.padding = Edges::all(cx.sp(2.0));
            style
        }
        Slot::Content => {
            let mut style = cx.base();
            style.gap = cx.sp(2.0);
            style
        }
        Slot::Separator => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::SidebarBorder).into();
            style.min_height = 1.0;
            style.padding = Edges::axes(cx.sp(2.0), 0.0);
            style
        }
        Slot::Group => {
            let mut style = cx.base();
            style.padding = Edges::all(cx.sp(2.0));
            style.gap = cx.sp(1.0);
            style
        }
        Slot::GroupLabel => {
            let mut style = cx.base();
            style.foreground = cx.a(ColorRole::SidebarForeground, 0.7);
            style.min_height = cx.sp(8.0);
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.padding = Edges::axes(cx.sp(2.0), 0.0);
            style.text = cx.text(TextSize::Xs, FontWeight::MEDIUM);
            style.motion_ms = cx.motion(200);
            sidebar_ring(cx, &mut style);
            style
        }
        Slot::Action => {
            let mut style = cx.base();
            style.foreground = cx.c(ColorRole::SidebarForeground);
            style.min_width = cx.sp(5.0);
            style.min_height = cx.sp(5.0);
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            if s.interactive_hover() {
                accent(&mut style);
            }
            sidebar_ring(cx, &mut style);
            style
        }
        Slot::Item | Slot::Link => {
            let sub = cx.q.slot == Slot::Link;
            let mut style = cx.base();
            style.foreground = cx.c(ColorRole::SidebarForeground);
            style.gap = cx.sp(2.0);
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.padding = if sub {
                Edges::axes(cx.sp(2.0), 0.0)
            } else {
                Edges::all(cx.sp(2.0))
            };
            let (height, size) = match cx.q.size {
                ControlSize::Sm => (7.0, TextSize::Xs),
                ControlSize::Lg => (12.0, TextSize::Sm),
                ControlSize::Icon => (8.0, TextSize::Sm),
                _ if sub => (7.0, TextSize::Sm),
                _ => (8.0, TextSize::Sm),
            };
            style.min_height = cx.sp(height);
            if cx.q.size == ControlSize::Icon {
                style.min_width = cx.sp(8.0);
            }
            style.text = cx.text(size, FontWeight::NORMAL);
            let outline =
                SidebarItemVariant::from_variant(cx.q.variant) == SidebarItemVariant::Outline;
            if outline {
                style.background = cx.c(ColorRole::Background).into();
                style.ring = cx.ring(1.0, cx.c(ColorRole::SidebarBorder));
            }
            if s.interactive_hover() || s.pressed() {
                accent(&mut style);
                if outline {
                    style.ring = cx.ring(1.0, cx.c(ColorRole::SidebarAccent));
                }
            }
            if s.selected() {
                accent(&mut style);
                style.text.weight = FontWeight::MEDIUM;
            }
            sidebar_ring(cx, &mut style);
            disabled(&mut style, cx);
            style
        }
        Slot::Row => {
            let mut style = cx.base();
            style.border = cx.c(ColorRole::SidebarBorder);
            style.border_width = cx.t.border_width;
            style.border_sides = Sides::START;
            style.gap = cx.sp(1.0);
            style.padding = Edges::axes(cx.sp(2.5), cx.sp(0.5));
            style.min_width = cx.sp(3.5);
            style
        }
        Slot::Badge => {
            let mut style = cx.base();
            style.foreground = if s.selected() || s.interactive_hover() {
                cx.c(ColorRole::SidebarAccentForeground)
            } else {
                cx.c(ColorRole::SidebarForeground)
            };
            style.min_height = cx.sp(5.0);
            style.min_width = cx.sp(5.0);
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.padding = Edges::axes(cx.sp(1.0), 0.0);
            style.text = cx.text(TextSize::Xs, FontWeight::MEDIUM);
            style
        }
        Slot::Input => {
            let mut style = super::field_box(cx);
            style.background = cx.c(ColorRole::Background).into();
            style.shadow = choscorui_core::ShadowLayers::NONE;
            style.min_height = cx.sp(8.0);
            super::field_states(&mut style, cx);
            style
        }
        Slot::Handle => {
            // The rail along the inner edge.
            let mut style = cx.base();
            style.min_width = cx.sp(4.0);
            if s.interactive_hover() {
                style.background = cx.c(ColorRole::SidebarBorder).into();
            }
            style
        }
        Slot::Panel => {
            // The main area beside the sidebar.
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Background).into();
            if variant == SidebarVariant::Inset {
                style.radius = Corners::all(cx.r(RadiusScale::Xl));
                style.shadow = cx.shadow(ShadowScale::Sm);
                style.padding = Edges {
                    inline_start: 0.0,
                    ..Edges::all(cx.sp(2.0))
                };
            }
            style
        }
        _ => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Sidebar).into();
            style.foreground = cx.c(ColorRole::SidebarForeground);
            style.min_width = if cx.q.size == ControlSize::Icon {
                48.0
            } else {
                256.0
            };
            style.motion_ms = cx.motion(200);
            match variant {
                SidebarVariant::Floating => {
                    style.ring = cx.ring(1.0, cx.c(ColorRole::SidebarBorder));
                    style.radius = Corners::all(cx.r(RadiusScale::Lg));
                    style.shadow = cx.shadow(ShadowScale::Sm);
                    style.padding = Edges::all(cx.sp(2.0));
                }
                SidebarVariant::Inset => style.padding = Edges::all(cx.sp(2.0)),
                SidebarVariant::Sidebar => {
                    style.border = cx.c(ColorRole::SidebarBorder);
                    style.border_width = cx.t.border_width;
                    style.border_sides = Sides::END;
                }
            }
            style
        }
    }
}

/// Toolbars, status bars, and dock panels, which have no reference
/// counterpart: they take the card and border roles.
fn chrome(cx: &Cx) -> ResolvedStyle {
    match (cx.q.kind, cx.q.slot) {
        (_, Slot::Item | Slot::Action) => {
            let mut style = super::form::button(cx, ButtonVariant::Ghost, ControlSize::IconSm);
            // A checked item takes the on fill of a toggle.
            if cx.s().checked() {
                style.background = cx.c(ColorRole::Muted).into();
                style.foreground = cx.c(ColorRole::Foreground);
            }
            style
        }
        (_, Slot::Separator) => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Border).into();
            style.min_width = 1.0;
            style.min_height = 1.0;
            style.padding = Edges::axes(cx.sp(1.0), cx.sp(1.5));
            style
        }
        (_, Slot::Title) => text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::MEDIUM),
        (ControlKind::StatusBar, _) => {
            let mut style = muted_text(cx, TextSize::Xs);
            style.background = cx.c(ColorRole::Background).into();
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.border_sides = Sides::TOP;
            style.min_height = cx.sp(6.0);
            style.padding = Edges::axes(cx.sp(3.0), 0.0);
            style.gap = cx.sp(3.0);
            style
        }
        (ControlKind::Dock, _) => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Card).into();
            style.foreground = cx.c(ColorRole::CardForeground);
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style
        }
        _ => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Background).into();
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.border_sides = Sides::BOTTOM;
            style.min_height = cx.sp(10.0);
            style.padding = Edges::axes(cx.sp(2.0), cx.sp(1.0));
            style.gap = cx.sp(1.0);
            style
        }
    }
}
