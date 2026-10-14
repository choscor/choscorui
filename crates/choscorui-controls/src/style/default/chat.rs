//! Chat: messages, bubbles, attachments, markers, the message scroller,
//! and questionnaires.

use super::{Cx, choice_invalid, disabled, field_box, field_states, focus, muted_text, text};
use crate::style::{
    AttachmentState, BubbleVariant, ControlKind, ControlSize, Corners, Edges, MarkerVariant,
    ResolvedStyle, Sides, Slot,
};
use choscorui_core::{Color, ColorRole, FontWeight, RadiusScale, ShadowScale, TextSize};

pub(super) fn resolve(cx: &Cx) -> ResolvedStyle {
    match cx.q.kind {
        ControlKind::Message | ControlKind::MessageScroller => message(cx),
        ControlKind::Bubble => bubble(cx),
        ControlKind::Attachment => attachment(cx),
        ControlKind::Marker => marker(cx),
        _ => questionnaire(cx),
    }
}

fn message(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Content => {
            let mut style = cx.base();
            style.gap = cx.sp(2.5);
            style
        }
        Slot::Header | Slot::Footer => {
            let mut style = muted_text(cx, TextSize::Xs);
            style.text.weight = FontWeight::MEDIUM;
            style.padding = Edges::axes(cx.sp(3.0), 0.0);
            style
        }
        Slot::Group => {
            let mut style = cx.base();
            style.gap = cx.sp(2.0);
            style
        }
        Slot::Viewport => {
            let mut style = cx.base();
            style.gap = cx.sp(8.0);
            style
        }
        _ => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL);
            style.gap = cx.sp(2.0);
            style
        }
    }
}

/// `primary` at a new OKLCH lightness and a fraction of its chroma.
fn tint(primary: Color, lightness: f64, chroma: f64) -> Color {
    let (_, a, b) = primary.to_oklab();
    let c = (a * a + b * b).sqrt() * chroma;
    let hue = b.atan2(a).to_degrees();
    Color::from_oklch(lightness, c, hue, 1.0)
}

fn bubble(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    let variant = BubbleVariant::from_variant(cx.q.variant);
    match cx.q.slot {
        Slot::Content => {
            let hover = s.interactive_hover();
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL);
            style.text.line_height = style.text.size * 1.625;
            style.radius = Corners::all(cx.r(RadiusScale::Xl));
            style.border_width = cx.t.border_width;
            style.padding = Edges::axes(cx.sp(3.0), cx.sp(2.0));
            let primary = cx.c(ColorRole::Primary);
            let foreground = cx.c(ColorRole::Foreground);
            match variant {
                BubbleVariant::Default => {
                    style.background = cx
                        .a(ColorRole::Primary, if hover { 0.8 } else { 1.0 })
                        .into();
                    style.foreground = cx.c(ColorRole::PrimaryForeground);
                }
                BubbleVariant::Secondary => {
                    let secondary = cx.c(ColorRole::Secondary);
                    style.background = if hover {
                        secondary.mix(foreground, 0.05)
                    } else {
                        secondary
                    }
                    .into();
                    style.foreground = cx.c(ColorRole::SecondaryForeground);
                }
                BubbleVariant::Muted => {
                    let muted = cx.c(ColorRole::Muted);
                    style.background = if hover {
                        muted.mix(foreground, 0.05)
                    } else {
                        muted
                    }
                    .into();
                }
                BubbleVariant::Tinted => {
                    style.background = match (cx.dark(), hover) {
                        (false, false) => tint(primary, 0.93, 0.4),
                        (false, true) => tint(primary, 0.88, 0.5),
                        (true, false) => tint(primary, 0.3, 0.4),
                        (true, true) => tint(primary, 0.35, 0.5),
                    }
                    .into();
                }
                BubbleVariant::Outline => {
                    style.background = cx.c(ColorRole::Background).into();
                    style.border = cx.c(ColorRole::Border);
                    if hover {
                        style.background = cx
                            .pick(cx.c(ColorRole::Muted), cx.a(ColorRole::Input, 0.3))
                            .into();
                    }
                }
                BubbleVariant::Ghost => {
                    style.radius = Corners::ZERO;
                    style.padding = Edges::ZERO;
                    style.border_width = 0.0;
                    if hover {
                        style.background = cx
                            .pick(cx.c(ColorRole::Muted), cx.a(ColorRole::Muted, 0.5))
                            .into();
                    }
                }
                BubbleVariant::Destructive => {
                    let alpha = match (cx.dark(), hover) {
                        (false, false) => 0.1,
                        (true, true) => 0.3,
                        _ => 0.2,
                    };
                    style.background = cx.a(ColorRole::Destructive, alpha).into();
                    style.foreground = cx.c(ColorRole::Destructive);
                }
            }
            focus(&mut style, cx);
            style
        }
        Slot::Badge => {
            // The reactions pill.
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL);
            style.background = cx.c(ColorRole::Muted).into();
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            style.ring = cx.ring(3.0, cx.c(ColorRole::Card));
            style.gap = cx.sp(1.0);
            style.padding = Edges::axes(cx.sp(1.5), cx.sp(0.5));
            style
        }
        Slot::Group => {
            let mut style = cx.base();
            style.gap = cx.sp(2.0);
            style
        }
        _ => {
            let mut style = cx.base();
            style.gap = cx.sp(1.0);
            style.max_width = if variant == BubbleVariant::Ghost {
                f64::INFINITY
            } else {
                0.8
            };
            style
        }
    }
}

fn attachment(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    let size = cx.q.size;
    let vertical = s.open();
    match cx.q.slot {
        Slot::Media => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Muted).into();
            style.foreground = cx.c(ColorRole::Foreground);
            let (width, radius, icon) = match size {
                ControlSize::Sm => (8.0, RadiusScale::Lg, 16.0),
                ControlSize::Xs => (7.0, RadiusScale::Md, 14.0),
                _ => (10.0, RadiusScale::Lg, 16.0),
            };
            style.min_width = cx.sp(width);
            style.min_height = cx.sp(width);
            style.radius = Corners::all(cx.r(radius));
            style.icon_size = if vertical { 24.0 } else { icon };
            let state = AttachmentState::from_variant(cx.q.variant);
            if cx.s().selected() && !matches!(state, AttachmentState::Idle | AttachmentState::Done)
            {
                style.opacity = 0.6;
            }
            style
        }
        Slot::Title => {
            let size = if size == ControlSize::Default {
                TextSize::Sm
            } else {
                TextSize::Xs
            };
            let mut style = text(cx, ColorRole::Foreground, size, FontWeight::MEDIUM);
            style.text.line_height = style.text.size * 1.25;
            style
        }
        Slot::Description => {
            let mut style = muted_text(cx, TextSize::Xs);
            if AttachmentState::from_variant(cx.q.variant) == AttachmentState::Error {
                style.foreground = cx.c(ColorRole::Destructive);
            }
            style.padding.block_start = cx.sp(0.5);
            style
        }
        Slot::Progress => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Primary).into();
            style.min_height = 2.0;
            style
        }
        Slot::Track => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Muted).into();
            style.min_height = 2.0;
            style
        }
        Slot::Action => {
            let mut style = cx.base();
            style.gap = cx.sp(1.0);
            style
        }
        Slot::Group => {
            let mut style = cx.base();
            style.gap = cx.sp(3.0);
            style.padding = Edges::axes(0.0, cx.sp(1.0));
            style
        }
        _ => {
            let mut style = text(
                cx,
                ColorRole::Foreground,
                if size == ControlSize::Default {
                    TextSize::Sm
                } else {
                    TextSize::Xs
                },
                FontWeight::NORMAL,
            );
            style.radius = Corners::all(cx.r(if size == ControlSize::Xs {
                RadiusScale::Lg
            } else {
                RadiusScale::Xl
            }));
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            let (gap, pad) = match size {
                ControlSize::Sm => (2.5, (2.0, 1.5)),
                ControlSize::Xs => (1.5, (1.5, 1.0)),
                _ => (2.0, (2.5, 2.0)),
            };
            style.gap = cx.sp(gap);
            style.padding = Edges::axes(cx.sp(pad.0), cx.sp(pad.1));
            // Focus goes to the remove button, which shows the one ring.
            style.min_width = if vertical { cx.sp(24.0) } else { cx.sp(40.0) };
            if AttachmentState::from_variant(cx.q.variant) == AttachmentState::Error {
                style.border = cx.c(ColorRole::Destructive);
            }
            disabled(&mut style, cx);
            style
        }
    }
}

fn marker(cx: &Cx) -> ResolvedStyle {
    let variant = MarkerVariant::from_variant(cx.q.variant);
    match cx.q.slot {
        Slot::Icon => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.icon_size = 16.0;
            style
        }
        Slot::Separator => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Border).into();
            style.min_height = 1.0;
            style.padding = Edges::axes(cx.sp(1.0), 0.0);
            style
        }
        _ => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.gap = cx.sp(2.0);
            style.min_height = cx.sp(4.0);
            style.icon_size = 16.0;
            if variant == MarkerVariant::Border {
                style.border = cx.c(ColorRole::Border);
                style.border_width = cx.t.border_width;
                style.border_sides = Sides::BOTTOM;
                style.padding.block_end = cx.sp(2.0);
            }
            if cx.s().interactive_hover() && cx.s().highlighted() {
                style.foreground = cx.c(ColorRole::Foreground);
            }
            style
        }
    }
}

fn questionnaire(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Progress => muted_text(cx, TextSize::Xs),
        Slot::Item => {
            let mut style = cx.base();
            style.gap = cx.sp(5.0);
            style
        }
        Slot::Title => {
            let mut style = text(
                cx,
                ColorRole::Foreground,
                TextSize::Base,
                FontWeight::SEMIBOLD,
            );
            style.padding.block_end = cx.sp(5.0);
            style
        }
        Slot::Description => text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL),
        Slot::Group => {
            let mut style = cx.base();
            style.gap = cx.sp(3.0);
            style
        }
        Slot::Choice => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL);
            style.border = cx.c(ColorRole::Input);
            style.border_width = cx.t.border_width;
            style.background = cx
                .pick(Color::TRANSPARENT, cx.a(ColorRole::Input, 0.2))
                .into();
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.gap = cx.sp(3.0);
            style.padding = Edges::axes(cx.sp(4.0), cx.sp(3.5));
            style.shadow = cx.shadow(ShadowScale::Xs);
            if s.interactive_hover() {
                style.background = cx.a(ColorRole::Muted, 0.5).into();
            }
            if s.checked() {
                style.border = cx.a(ColorRole::Primary, 0.4);
                style.background = cx.c(ColorRole::Muted).into();
            }
            if s.invalid() {
                style.border = cx.c(ColorRole::Destructive);
            }
            focus(&mut style, cx);
            disabled(&mut style, cx);
            style
        }
        Slot::Indicator => {
            let mut style = cx.base();
            style.border = cx.c(ColorRole::Input);
            style.border_width = cx.t.border_width;
            style.background = cx
                .pick(Color::TRANSPARENT, cx.a(ColorRole::Input, 0.3))
                .into();
            style.min_width = cx.sp(4.0);
            style.min_height = cx.sp(4.0);
            style.radius = Corners::all(4.0);
            style.icon_size = 14.0;
            style.foreground = cx.c(ColorRole::PrimaryForeground);
            if s.checked() {
                style.background = cx.c(ColorRole::Primary).into();
                style.border = cx.c(ColorRole::Primary);
            }
            choice_invalid(&mut style, cx);
            style
        }
        Slot::Shortcut => {
            let mut style = cx.base();
            style.border = cx.c(ColorRole::Input);
            style.border_width = cx.t.border_width;
            style.background = cx.c(ColorRole::Background).into();
            style.foreground = cx.c(ColorRole::MutedForeground);
            style.min_width = cx.sp(5.0);
            style.min_height = cx.sp(5.0);
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.text = cx.t.mono(TextSize::Xs, FontWeight::MEDIUM);
            style.text.size = 10.0 * cx.t.text_scale;
            style.text.line_height = style.text.size;
            style.shadow = cx.shadow(ShadowScale::Xs);
            style
        }
        Slot::Input => {
            let mut style = field_box(cx);
            style.cursor = crate::style::Cursor::Text;
            field_states(&mut style, cx);
            style
        }
        Slot::Error => text(cx, ColorRole::Destructive, TextSize::Sm, FontWeight::NORMAL),
        Slot::Action => {
            let mut style = cx.base();
            style.gap = cx.sp(2.0);
            style.min_height = cx.sp(9.0);
            style
        }
        _ => {
            let mut style = cx.base();
            style.gap = cx.sp(6.0);
            style
        }
    }
}
