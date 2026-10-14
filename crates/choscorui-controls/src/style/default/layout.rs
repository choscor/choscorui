//! Layout: separators, resizable panels, scroll areas, collapsibles,
//! accordions, item rows, typography, and the repainted editors.

use super::{Cx, disabled, focus, muted_text, text};
use crate::style::{
    ControlKind, ControlSize, Corners, Cursor, Edges, ItemVariant, ResolvedStyle, Sides, Slot,
};
use choscorui_core::{Color, ColorRole, FontWeight, RadiusScale, TextPreset, TextSize};

pub(super) fn resolve(cx: &Cx) -> ResolvedStyle {
    match cx.q.kind {
        ControlKind::Separator => separator(cx),
        ControlKind::Resizable | ControlKind::SizeGrip => resizable(cx),
        ControlKind::ScrollArea => scroll_area(cx),
        ControlKind::Accordion | ControlKind::Collapsible => accordion(cx),
        ControlKind::Item => item(cx),
        ControlKind::Typography => typography(cx),
        ControlKind::TextEditor => editor(cx),
        ControlKind::Canvas => canvas(cx),
        ControlKind::AspectRatio => aspect_ratio(cx),
        _ => cx.base(),
    }
}

/// The selection outline (`Indicator`) and the rubber band (`Range`).
fn canvas(cx: &Cx) -> ResolvedStyle {
    let mut style = cx.base();
    match cx.q.slot {
        Slot::Indicator => {
            style.border = cx.c(ColorRole::Primary);
            style.border_width = cx.t.border_width;
        }
        Slot::Range => {
            style.background = cx.a(ColorRole::Primary, 0.15).into();
            style.border = cx.c(ColorRole::Primary);
            style.border_width = cx.t.border_width;
        }
        _ => {}
    }
    style
}

/// The placeholder box behind an aspect-ratio image.
fn aspect_ratio(cx: &Cx) -> ResolvedStyle {
    let mut style = cx.base();
    style.background = cx.c(ColorRole::Muted).into();
    style.radius = Corners::all(cx.r(RadiusScale::Lg));
    style
}

fn separator(cx: &Cx) -> ResolvedStyle {
    let mut style = cx.base();
    style.background = cx.c(ColorRole::Border).into();
    style.min_height = 1.0;
    style.min_width = 1.0;
    style
}

fn resizable(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Icon | Slot::Thumb => {
            // The grip on a handle.
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Border).into();
            style.radius = Corners::all(cx.r(RadiusScale::Lg));
            style.min_width = cx.sp(1.0);
            style.min_height = cx.sp(6.0);
            style.foreground = cx.c(ColorRole::MutedForeground);
            style
        }
        Slot::Handle | Slot::Root if cx.q.kind == ControlKind::Resizable => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Border).into();
            style.min_width = 1.0;
            style.min_height = 1.0;
            // The hit area is 4 pt wide around the 1 pt line.
            style.padding = Edges::axes(cx.sp(1.0) / 2.0, 0.0);
            style.cursor = Cursor::ResizeHorizontal;
            if s.focus_visible() {
                style.ring = cx.ring(1.0, cx.c(ColorRole::Ring));
            }
            style
        }
        _ => {
            let mut style = cx.base();
            style.foreground = cx.c(ColorRole::MutedForeground);
            style.min_width = cx.sp(4.0);
            style.min_height = cx.sp(4.0);
            if cx.q.kind == ControlKind::SizeGrip {
                // The grip sits at the bottom end corner of the window.
                style.cursor = Cursor::ResizeDiagonalEnd;
            }
            style
        }
    }
}

fn scroll_area(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Scrollbar => {
            let mut style = cx.base();
            style.min_width = cx.sp(2.5);
            style.min_height = cx.sp(2.5);
            style.padding = Edges::all(1.0);
            style.motion_ms = cx.motion(150);
            style
        }
        Slot::Thumb => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Border).into();
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            if s.interactive_hover() || s.pressed() {
                style.background = cx.c(ColorRole::MutedForeground).with_alpha(0.5).into();
            }
            style
        }
        _ => {
            let mut style = cx.base();
            if s.focus_visible() {
                style.ring = cx.focus_ring();
            }
            style
        }
    }
}

fn accordion(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Item => {
            let mut style = cx.base();
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.border_sides = Sides::BOTTOM;
            style
        }
        Slot::Trigger => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::MEDIUM);
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.border_width = cx.t.border_width;
            style.padding = Edges::axes(0.0, cx.sp(4.0));
            style.underline = s.interactive_hover();
            style.icon_size = 16.0;
            style.gap = cx.sp(4.0);
            focus(&mut style, cx);
            disabled(&mut style, cx);
            style
        }
        Slot::Icon => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.icon_size = 16.0;
            style
        }
        Slot::Content => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL);
            style.padding.block_end = cx.sp(4.0);
            style.motion_ms = cx.motion(200);
            style
        }
        _ => cx.base(),
    }
}

fn item(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    let size = cx.q.size;
    match cx.q.slot {
        Slot::Group => {
            let mut style = cx.base();
            style.gap = match size {
                ControlSize::Sm => cx.sp(2.5),
                ControlSize::Xs => cx.sp(2.0),
                _ => cx.sp(4.0),
            };
            style
        }
        Slot::Separator => {
            let mut style = separator(cx);
            style.padding = Edges::axes(0.0, cx.sp(2.0));
            style
        }
        Slot::Media => {
            let mut style = cx.base();
            style.gap = cx.sp(2.0);
            style.icon_size = 16.0;
            if cx.q.variant.0 == 2 {
                let edge = match size {
                    ControlSize::Sm => 8.0,
                    ControlSize::Xs => 6.0,
                    _ => 10.0,
                };
                style.min_width = cx.sp(edge);
                style.min_height = cx.sp(edge);
                style.radius = Corners::all(cx.r(RadiusScale::Sm));
            }
            style
        }
        Slot::Content => {
            let mut style = cx.base();
            style.gap = if size == ControlSize::Xs {
                0.0
            } else {
                cx.sp(1.0)
            };
            style
        }
        Slot::Title => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::MEDIUM);
            style.text.line_height = style.text.size * 1.375;
            style.gap = cx.sp(2.0);
            style
        }
        Slot::Description => {
            let mut style = muted_text(
                cx,
                if size == ControlSize::Xs {
                    TextSize::Xs
                } else {
                    TextSize::Sm
                },
            );
            style.text.line_height = style.text.size * 1.5;
            style
        }
        Slot::Action | Slot::Header | Slot::Footer => {
            let mut style = cx.base();
            style.gap = cx.sp(2.0);
            style
        }
        _ => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL);
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.border_width = cx.t.border_width;
            match ItemVariant::from_variant(cx.q.variant) {
                ItemVariant::Default => {}
                ItemVariant::Outline => style.border = cx.c(ColorRole::Border),
                ItemVariant::Muted => style.background = cx.a(ColorRole::Muted, 0.5).into(),
            }
            let (gap, inline, block) = match size {
                ControlSize::Sm => (2.5, 3.0, 2.5),
                ControlSize::Xs => (2.0, 2.5, 2.0),
                _ => (3.5, 4.0, 3.5),
            };
            style.gap = cx.sp(gap);
            style.padding = Edges::axes(cx.sp(inline), cx.sp(block));
            if s.highlighted() {
                // A link row or a command link.
                style.cursor = Cursor::Pointer;
                if s.interactive_hover() || s.pressed() && !s.disabled() {
                    style.background = cx.c(ColorRole::Muted).into();
                }
            }
            style.motion_ms = cx.motion(100);
            focus(&mut style, cx);
            disabled(&mut style, cx);
            style
        }
    }
}

fn typography(cx: &Cx) -> ResolvedStyle {
    let preset = TextPreset::ALL
        .get(usize::from(cx.q.variant.0))
        .copied()
        .unwrap_or(TextPreset::Paragraph);
    let mut style = cx.base();
    style.text = cx.t.preset(preset);
    style.foreground = cx.c(preset.color());
    match preset {
        TextPreset::H2 => {
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.border_sides = Sides::BOTTOM;
            style.padding.block_end = cx.sp(2.0);
        }
        TextPreset::Blockquote => {
            style.border = cx.c(ColorRole::Border);
            style.border_width = 2.0;
            style.border_sides = Sides::START;
            style.padding.inline_start = cx.sp(6.0);
        }
        TextPreset::InlineCode => {
            style.background = cx.c(ColorRole::Muted).into();
            style.radius = Corners::all(cx.r(RadiusScale::Sm));
            style.padding = Edges::axes(0.3 * 16.0, 0.2 * 16.0);
        }
        TextPreset::List => {
            style.padding.inline_start = cx.sp(6.0);
            style.gap = cx.sp(2.0);
        }
        _ => {}
    }
    style
}

/// The text editor and code areas: the textarea look.
fn editor(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Placeholder => muted_text(cx, TextSize::Sm),
        Slot::Indicator => {
            // The caret.
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Foreground).into();
            style.min_width = 1.0;
            style
        }
        Slot::Range => {
            // The selection.
            let mut style = cx.base();
            style.background = cx.a(ColorRole::Primary, 0.25).into();
            style
        }
        Slot::Text => {
            let mut style = cx.base();
            style.text = cx.t.mono(TextSize::Sm, FontWeight::NORMAL);
            style
        }
        _ => {
            let mut style = super::field_box(cx);
            style.padding = Edges::axes(cx.sp(2.5), cx.sp(2.0));
            style.cursor = Cursor::Text;
            style.background = cx
                .pick(Color::TRANSPARENT, cx.a(ColorRole::Input, 0.3))
                .into();
            super::field_states(&mut style, cx);
            style
        }
    }
}
