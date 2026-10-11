//! Form controls: buttons, inputs, choices, toggles, and field layout.

use super::{
    Cx, disabled, field_box, focus, group_label, invalid, list_item, muted_text, rule, states,
    surface, text,
};
use crate::style::{
    AddonAlign, ButtonVariant, ControlKind, ControlSize, Corners, Cursor, Edges, FieldOrientation,
    KbdVariant, ResolvedStyle, Sides, Slot, ToggleVariant,
};
use choscorui_core::{
    Color, ColorRole, FontWeight, RadiusScale, ShadowLayers, ShadowScale, TextSize,
};

pub(super) fn resolve(cx: &Cx) -> ResolvedStyle {
    match cx.q.kind {
        ControlKind::Button => button(cx, ButtonVariant::from_variant(cx.q.variant), cx.q.size),
        ControlKind::ButtonGroup => button_group(cx),
        ControlKind::Checkbox => checkbox(cx),
        ControlKind::RadioGroup => radio(cx),
        ControlKind::Input => input(cx),
        ControlKind::Textarea => textarea(cx),
        ControlKind::Select | ControlKind::Combobox => select(cx),
        ControlKind::NativeSelect => native_select(cx),
        ControlKind::DatePicker => date_picker(cx),
        ControlKind::Slider => slider(cx),
        ControlKind::Switch => switch(cx),
        ControlKind::Toggle | ControlKind::ToggleGroup => toggle(cx),
        ControlKind::Field => field(cx),
        ControlKind::InputGroup => input_group(cx),
        ControlKind::InputOtp => input_otp(cx),
        ControlKind::Kbd => kbd(cx),
        _ => label(cx),
    }
}

/// The button look, shared by every control that shows a button.
pub(crate) fn button(cx: &Cx, variant: ButtonVariant, size: ControlSize) -> ResolvedStyle {
    let s = cx.s();
    let mut style = cx.base();
    style.border_width = cx.t.border_width;
    style.text = cx.text(TextSize::Sm, FontWeight::MEDIUM);
    let md = cx.r(RadiusScale::Md);
    let (height, gap, inline, radius, icon) = match size {
        ControlSize::Xs => (6.0, 1.0, 2.0, md.min(8.0), 12.0),
        ControlSize::Sm => (8.0, 1.0, 2.5, md.min(10.0), 16.0),
        ControlSize::Lg => (10.0, 1.5, 2.5, md, 16.0),
        ControlSize::IconXs => (6.0, 0.0, 0.0, md.min(8.0), 12.0),
        ControlSize::IconSm => (8.0, 0.0, 0.0, md.min(10.0), 16.0),
        ControlSize::Icon => (9.0, 0.0, 0.0, md, 16.0),
        ControlSize::IconLg => (10.0, 0.0, 0.0, md, 16.0),
        ControlSize::Default => (9.0, 1.5, 2.5, md, 16.0),
    };
    style.min_height = cx.sp(height);
    if size.is_icon() {
        style.min_width = cx.sp(height);
    }
    style.gap = cx.sp(gap);
    style.padding = Edges::axes(cx.sp(inline), 0.0);
    style.radius = Corners::all(radius);
    style.icon_size = icon;
    if size == ControlSize::Xs {
        style.text = cx.text(TextSize::Xs, FontWeight::MEDIUM);
    }
    let hover = s.interactive_hover();
    let open = s.open();
    match variant {
        ButtonVariant::Default => {
            style.background = cx.a(ColorRole::Primary, if hover { 0.8 } else { 1.0 });
            style.foreground = cx.c(ColorRole::PrimaryForeground);
        }
        ButtonVariant::Outline => {
            style.border = cx.pick(cx.c(ColorRole::Border), cx.c(ColorRole::Input));
            style.background = cx.pick(cx.c(ColorRole::Background), cx.a(ColorRole::Input, 0.3));
            style.shadow = cx.shadow(ShadowScale::Xs);
            if hover || open {
                style.background = if hover && cx.dark() {
                    cx.a(ColorRole::Input, 0.5)
                } else {
                    cx.c(ColorRole::Muted)
                };
                style.foreground = cx.c(ColorRole::Foreground);
            }
        }
        ButtonVariant::Secondary => {
            style.background = cx.c(ColorRole::Secondary);
            style.foreground = cx.c(ColorRole::SecondaryForeground);
            if hover && !open {
                style.background = cx
                    .c(ColorRole::Secondary)
                    .mix(cx.c(ColorRole::Foreground), 0.05);
            }
        }
        ButtonVariant::Ghost => {
            if hover || open {
                style.background = if hover && cx.dark() {
                    cx.a(ColorRole::Muted, 0.5)
                } else {
                    cx.c(ColorRole::Muted)
                };
                style.foreground = cx.c(ColorRole::Foreground);
            }
        }
        ButtonVariant::Destructive => {
            let alpha = match (cx.dark(), hover) {
                (false, false) => 0.1,
                (false, true) | (true, false) => 0.2,
                (true, true) => 0.3,
            };
            style.background = cx.a(ColorRole::Destructive, alpha);
            style.foreground = cx.c(ColorRole::Destructive);
        }
        ButtonVariant::Link => {
            style.foreground = cx.c(ColorRole::Primary);
            style.underline = hover;
        }
    }
    if s.pressed() && !open {
        style.offset_y = 1.0;
    }
    focus(&mut style, cx);
    if variant == ButtonVariant::Destructive && s.focus_visible() {
        style.border = cx.a(ColorRole::Destructive, 0.4);
        style.ring = cx.ring(
            cx.t.ring_width,
            cx.a(ColorRole::Destructive, cx.pick(0.2, 0.4)),
        );
    }
    invalid(&mut style, cx);
    disabled(&mut style, cx);
    style
}

fn button_group(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Text => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Muted);
            style.border = cx.c(ColorRole::Border);
            style.border_width = cx.t.border_width;
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.gap = cx.sp(2.0);
            style.padding = Edges::axes(cx.sp(2.5), 0.0);
            style.text = cx.text(TextSize::Sm, FontWeight::MEDIUM);
            style.shadow = cx.shadow(ShadowScale::Xs);
            style.min_height = cx.sp(9.0);
            style
        }
        Slot::Separator => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Input);
            style.min_width = 1.0;
            style.min_height = 1.0;
            style
        }
        _ => {
            let mut style = cx.base();
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            style.gap = cx.sp(2.0);
            style
        }
    }
}

/// A checkbox or radio box before its shape.
fn choice_box(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    let mut style = cx.base();
    style.min_width = cx.sp(4.0);
    style.min_height = cx.sp(4.0);
    style.border = cx.c(ColorRole::Input);
    style.border_width = cx.t.border_width;
    style.background = cx.pick(Color::TRANSPARENT, cx.a(ColorRole::Input, 0.3));
    style.shadow = cx.shadow(ShadowScale::Xs);
    style.foreground = cx.c(ColorRole::PrimaryForeground);
    let on = s.checked() || s.indeterminate();
    if on {
        style.background = cx.c(ColorRole::Primary);
        style.border = cx.c(ColorRole::Primary);
    }
    focus(&mut style, cx);
    invalid(&mut style, cx);
    if on && s.invalid() {
        style.border = cx.c(ColorRole::Primary);
    }
    disabled(&mut style, cx);
    style
}

fn checkbox(cx: &Cx) -> ResolvedStyle {
    let mut style = choice_box(cx);
    style.radius = Corners::all(4.0);
    style.icon_size = 14.0;
    if cx.q.slot == Slot::Label {
        return label(cx);
    }
    style
}

fn radio(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Indicator => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::PrimaryForeground);
            style.min_width = cx.sp(2.0);
            style.min_height = cx.sp(2.0);
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            style
        }
        Slot::Label => label(cx),
        Slot::Group => {
            let mut style = cx.base();
            style.gap = cx.sp(3.0);
            style
        }
        _ => {
            let mut style = choice_box(cx);
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            style
        }
    }
}

fn input(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Placeholder => muted_text(cx, TextSize::Sm),
        _ => {
            let mut style = field_box(cx);
            style.cursor = Cursor::Text;
            states(&mut style, cx);
            style
        }
    }
}

fn textarea(cx: &Cx) -> ResolvedStyle {
    let mut style = input(cx);
    if cx.q.slot != Slot::Placeholder {
        style.padding = Edges::axes(cx.sp(2.5), cx.sp(2.0));
        style.min_height = cx.sp(16.0);
    }
    style
}

/// The select trigger, list surface, and rows. The combobox shares them.
fn select(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Trigger | Slot::Root => {
            let mut style = field_box(cx);
            style.gap = cx.sp(1.5);
            style.padding = Edges {
                inline_start: cx.sp(2.5),
                inline_end: cx.sp(2.0),
                block_start: cx.sp(2.0),
                block_end: cx.sp(2.0),
            };
            style.min_height = cx.sp(if cx.q.size == ControlSize::Sm {
                8.0
            } else {
                9.0
            });
            if cx.dark() && s.interactive_hover() {
                style.background = cx.a(ColorRole::Input, 0.5);
            }
            states(&mut style, cx);
            style
        }
        Slot::Placeholder | Slot::Icon => muted_text(cx, TextSize::Sm),
        Slot::Value => text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::NORMAL),
        Slot::Content => {
            let mut style = surface(cx, RadiusScale::Md);
            style.min_width = cx.sp(36.0);
            style.padding = Edges::all(cx.sp(1.0));
            if cx.q.kind == ControlKind::Combobox {
                style.min_height = 0.0;
            }
            style
        }
        Slot::Item => {
            let mut style = list_item(cx, false);
            style.padding.inline_end = cx.sp(8.0);
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
        Slot::GroupLabel => group_label(cx),
        Slot::Separator => rule(cx, cx.sp(1.0)),
        Slot::Previous | Slot::Next => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Popover);
            style.padding = Edges::axes(0.0, cx.sp(1.0));
            style.min_height = cx.sp(6.0);
            style
        }
        Slot::Description => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.padding = Edges::axes(0.0, cx.sp(2.0));
            style
        }
        Slot::Input => {
            let mut style = field_box(cx);
            style.background = cx.a(ColorRole::Input, 0.3);
            style.border = cx.a(ColorRole::Input, 0.3);
            style.shadow = ShadowLayers::NONE;
            style.min_height = cx.sp(8.0);
            style.padding = Edges::axes(cx.sp(2.5), 0.0);
            style.cursor = Cursor::Text;
            style
        }
        Slot::Badge => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Muted);
            style.radius = Corners::all(cx.r(RadiusScale::Sm));
            style.min_height = cx.sp(5.5);
            style.padding = Edges::axes(cx.sp(1.5), 0.0);
            style.gap = cx.sp(1.0);
            style.text = cx.text(TextSize::Xs, FontWeight::MEDIUM);
            style
        }
        Slot::Close => {
            let mut style = cx.base();
            style.opacity = if s.interactive_hover() { 1.0 } else { 0.5 };
            style.icon_size = 12.0;
            style
        }
        _ => cx.base(),
    }
}

fn native_select(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Icon => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.padding = Edges {
                inline_end: cx.sp(2.5),
                ..Edges::ZERO
            };
            style
        }
        _ => {
            let mut style = field_box(cx);
            style.padding = Edges {
                inline_start: cx.sp(2.5),
                inline_end: cx.sp(8.0),
                block_start: cx.sp(1.0),
                block_end: cx.sp(1.0),
            };
            style.min_height = cx.sp(if cx.q.size == ControlSize::Sm {
                8.0
            } else {
                9.0
            });
            if cx.dark() && cx.s().interactive_hover() {
                style.background = cx.a(ColorRole::Input, 0.5);
            }
            states(&mut style, cx);
            style
        }
    }
}

fn date_picker(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Content => {
            let mut style = surface(cx, RadiusScale::Lg);
            style.padding = Edges::ZERO;
            style
        }
        Slot::Placeholder => muted_text(cx, TextSize::Sm),
        _ => {
            let mut style = button(cx, ButtonVariant::Outline, ControlSize::Default);
            style.gap = cx.sp(2.0);
            style
        }
    }
}

fn slider(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Track => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Muted);
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            style.min_height = cx.sp(1.5);
            style.min_width = cx.sp(1.5);
            disabled(&mut style, cx);
            style
        }
        Slot::Range => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Primary);
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            disabled(&mut style, cx);
            style
        }
        Slot::Thumb => {
            let mut style = cx.base();
            style.background = Color::WHITE;
            style.border = cx.c(ColorRole::Primary);
            style.border_width = cx.t.border_width;
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            style.min_width = cx.sp(4.0);
            style.min_height = cx.sp(4.0);
            style.shadow = cx.shadow(ShadowScale::Sm);
            if s.interactive_hover() || s.focus_visible() {
                style.ring = cx.ring(4.0, cx.t.ring_color(cx.c(ColorRole::Ring)));
            }
            disabled(&mut style, cx);
            style
        }
        _ => {
            let mut style = cx.base();
            style.min_height = cx.sp(4.0);
            style.min_width = cx.sp(40.0);
            style
        }
    }
}

fn switch(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    let small = cx.q.size == ControlSize::Sm;
    match cx.q.slot {
        Slot::Thumb => {
            let mut style = cx.base();
            style.background = match (cx.dark(), s.checked()) {
                (false, _) => cx.c(ColorRole::Background),
                (true, true) => cx.c(ColorRole::PrimaryForeground),
                (true, false) => cx.c(ColorRole::Foreground),
            };
            let size = if small { 12.0 } else { 16.0 };
            style.min_width = size;
            style.min_height = size;
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            style
        }
        Slot::Label => label(cx),
        _ => {
            let mut style = cx.base();
            style.background = if s.checked() {
                cx.c(ColorRole::Primary)
            } else {
                cx.a(ColorRole::Input, cx.pick(1.0, 0.8))
            };
            style.border_width = cx.t.border_width;
            style.radius = Corners::all(cx.r(RadiusScale::Full));
            style.shadow = cx.shadow(ShadowScale::Xs);
            (style.min_width, style.min_height) = if small { (24.0, 14.0) } else { (32.0, 18.4) };
            states(&mut style, cx);
            style
        }
    }
}

fn toggle(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    if cx.q.slot == Slot::Group
        || (cx.q.kind == ControlKind::ToggleGroup && cx.q.slot == Slot::Root)
    {
        let mut style = cx.base();
        style.radius = Corners::all(cx.r(RadiusScale::Md));
        if ToggleVariant::from_variant(cx.q.variant) == ToggleVariant::Outline {
            style.shadow = cx.shadow(ShadowScale::Xs);
        }
        return style;
    }
    let variant = ToggleVariant::from_variant(cx.q.variant);
    let mut style = cx.base();
    style.gap = cx.sp(1.0);
    style.radius = Corners::all(cx.r(RadiusScale::Md));
    style.text = cx.text(TextSize::Sm, FontWeight::MEDIUM);
    style.border_width = cx.t.border_width;
    let height = match cx.q.size {
        ControlSize::Sm | ControlSize::Xs | ControlSize::IconSm | ControlSize::IconXs => 8.0,
        ControlSize::Lg | ControlSize::IconLg => 10.0,
        _ => 9.0,
    };
    style.min_height = cx.sp(height);
    style.min_width = cx.sp(height);
    style.padding = Edges::axes(cx.sp(2.5), 0.0);
    if variant == ToggleVariant::Outline {
        style.border = cx.c(ColorRole::Input);
        style.shadow = cx.shadow(ShadowScale::Xs);
    }
    if s.interactive_hover() {
        style.background = cx.c(ColorRole::Muted);
        style.foreground = cx.c(ColorRole::Foreground);
    }
    if s.checked() {
        style.background = cx.c(ColorRole::Muted);
    }
    focus(&mut style, cx);
    if s.invalid() {
        style.ring = cx.ring(
            cx.t.ring_width,
            cx.a(ColorRole::Destructive, cx.pick(0.2, 0.4)),
        );
        style.border = cx.c(ColorRole::Destructive);
    }
    disabled(&mut style, cx);
    style
}

fn field(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Root => {
            let mut style = cx.base();
            style.gap = cx.sp(6.0);
            style
        }
        Slot::Legend => {
            let mut style = text(
                cx,
                ColorRole::Foreground,
                TextSize::Base,
                FontWeight::MEDIUM,
            );
            style.padding.block_end = cx.sp(3.0);
            if cx.q.size == ControlSize::Sm {
                style.text = cx.text(TextSize::Sm, FontWeight::MEDIUM);
            }
            style
        }
        Slot::Group => {
            let mut style = cx.base();
            style.gap = cx.sp(7.0);
            style
        }
        Slot::Item => {
            let mut style = cx.base();
            style.gap = cx.sp(3.0);
            if FieldOrientation::from_variant(cx.q.variant) != FieldOrientation::Vertical {
                style.min_height = cx.sp(4.0);
            }
            if s.invalid() {
                style.foreground = cx.c(ColorRole::Destructive);
            }
            disabled(&mut style, cx);
            style
        }
        Slot::Content => {
            let mut style = cx.base();
            style.gap = cx.sp(1.0);
            style
        }
        Slot::Label => {
            // A label that wraps a whole field becomes a selectable card.
            let mut style = label(cx);
            style.gap = cx.sp(2.0);
            style.text.line_height = style.text.size * 1.375;
            if cx.q.variant.0 == 1 {
                style.border = cx.c(ColorRole::Border);
                style.border_width = cx.t.border_width;
                style.radius = Corners::all(cx.r(RadiusScale::Md));
                style.padding = Edges::all(cx.sp(3.0));
                if s.interactive_hover() {
                    style.background = cx.a(ColorRole::Muted, 0.5);
                }
                if s.checked() {
                    style.background = cx.a(ColorRole::Primary, cx.pick(0.05, 0.1));
                    style.border = cx.a(ColorRole::Primary, cx.pick(0.3, 0.2));
                }
                focus(&mut style, cx);
            }
            style
        }
        Slot::Title => {
            let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::MEDIUM);
            style.gap = cx.sp(2.0);
            style.text.line_height = style.text.size * 1.375;
            disabled(&mut style, cx);
            style
        }
        Slot::Description => muted_text(cx, TextSize::Sm),
        Slot::Separator => {
            let mut style = rule(cx, 0.0);
            style.min_height = cx.sp(5.0);
            style.text = cx.text(TextSize::Sm, FontWeight::NORMAL);
            style
        }
        Slot::Text => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.padding = Edges::axes(cx.sp(2.0), 0.0);
            style.background = cx.c(ColorRole::Background);
            style
        }
        Slot::Error => text(cx, ColorRole::Destructive, TextSize::Sm, FontWeight::NORMAL),
        _ => cx.base(),
    }
}

fn input_group(cx: &Cx) -> ResolvedStyle {
    match cx.q.slot {
        Slot::Addon => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.text.weight = FontWeight::MEDIUM;
            style.gap = cx.sp(2.0);
            style.padding = match AddonAlign::from_variant(cx.q.variant) {
                AddonAlign::InlineStart => Edges {
                    inline_start: cx.sp(2.0),
                    block_start: cx.sp(1.5),
                    block_end: cx.sp(1.5),
                    inline_end: 0.0,
                },
                AddonAlign::InlineEnd => Edges {
                    inline_end: cx.sp(2.0),
                    block_start: cx.sp(1.5),
                    block_end: cx.sp(1.5),
                    inline_start: 0.0,
                },
                AddonAlign::BlockStart => Edges {
                    block_start: cx.sp(2.0),
                    block_end: cx.sp(1.5),
                    ..Edges::axes(cx.sp(2.5), 0.0)
                },
                AddonAlign::BlockEnd => Edges {
                    block_end: cx.sp(2.0),
                    block_start: cx.sp(1.5),
                    ..Edges::axes(cx.sp(2.5), 0.0)
                },
            };
            disabled(&mut style, cx);
            style
        }
        Slot::Action => {
            let mut style = button(cx, ButtonVariant::Ghost, cx.q.size);
            style.gap = cx.sp(2.0);
            let small = (cx.t.radius - 5.0).max(0.0);
            match cx.q.size {
                ControlSize::Xs => {
                    style.radius = Corners::all(small);
                    style.padding = Edges::axes(cx.sp(1.5), 0.0);
                    style.gap = cx.sp(1.0);
                    style.icon_size = 14.0;
                }
                ControlSize::IconXs => {
                    style.radius = Corners::all(small);
                    style.padding = Edges::ZERO;
                }
                ControlSize::IconSm => style.padding = Edges::ZERO,
                _ => {}
            }
            style
        }
        Slot::Text => {
            let mut style = muted_text(cx, TextSize::Sm);
            style.gap = cx.sp(2.0);
            style
        }
        Slot::Input => {
            let mut style = cx.base();
            style.cursor = Cursor::Text;
            style.padding = Edges::axes(cx.sp(2.5), cx.sp(1.0));
            style
        }
        Slot::Placeholder => muted_text(cx, TextSize::Sm),
        _ => {
            let mut style = field_box(cx);
            style.padding = Edges::ZERO;
            states(&mut style, cx);
            style
        }
    }
}

fn input_otp(cx: &Cx) -> ResolvedStyle {
    let s = cx.s();
    match cx.q.slot {
        Slot::Item => {
            let mut style = field_box(cx);
            style.min_width = cx.sp(9.0);
            style.min_height = cx.sp(9.0);
            style.padding = Edges::ZERO;
            style.radius = Corners::ZERO;
            style.border_sides = Sides {
                inline_start: false,
                ..Sides::ALL
            };
            if s.selected() {
                style.border = cx.c(ColorRole::Ring);
                style.ring = cx.focus_ring();
            }
            if s.invalid() {
                style.border = cx.c(ColorRole::Destructive);
                if s.selected() {
                    style.ring = cx.ring(
                        cx.t.ring_width,
                        cx.a(ColorRole::Destructive, cx.pick(0.2, 0.4)),
                    );
                }
            }
            disabled(&mut style, cx);
            style
        }
        Slot::Indicator => {
            let mut style = cx.base();
            style.background = cx.c(ColorRole::Foreground);
            style.min_width = 1.0;
            style.min_height = cx.sp(4.0);
            style
        }
        Slot::Group => {
            let mut style = cx.base();
            style.radius = Corners::all(cx.r(RadiusScale::Md));
            if s.invalid() {
                style.ring = cx.ring(
                    cx.t.ring_width,
                    cx.a(ColorRole::Destructive, cx.pick(0.2, 0.4)),
                );
            }
            style
        }
        Slot::Separator => {
            let mut style = cx.base();
            style.icon_size = 16.0;
            style.min_width = cx.sp(4.0);
            style
        }
        _ => {
            let mut style = cx.base();
            style.gap = cx.sp(2.0);
            disabled(&mut style, cx);
            style
        }
    }
}

fn kbd(cx: &Cx) -> ResolvedStyle {
    if cx.q.slot == Slot::Group {
        let mut style = cx.base();
        style.gap = cx.sp(1.0);
        return style;
    }
    let mut style = cx.base();
    match KbdVariant::from_variant(cx.q.variant) {
        KbdVariant::Default => {
            style.background = cx.c(ColorRole::Muted);
            style.foreground = cx.c(ColorRole::MutedForeground);
        }
        KbdVariant::Inverted => {
            style.background = cx.a(ColorRole::Background, cx.pick(0.2, 0.1));
            style.foreground = cx.c(ColorRole::Background);
        }
    }
    style.min_height = cx.sp(5.0);
    style.min_width = cx.sp(5.0);
    style.gap = cx.sp(1.0);
    style.radius = Corners::all(cx.r(RadiusScale::Sm));
    style.padding = Edges::axes(cx.sp(1.0), 0.0);
    style.text = cx.text(TextSize::Xs, FontWeight::MEDIUM);
    style.icon_size = 12.0;
    style
}

/// A form label: small, medium weight, and tight leading.
pub(crate) fn label(cx: &Cx) -> ResolvedStyle {
    let mut style = text(cx, ColorRole::Foreground, TextSize::Sm, FontWeight::MEDIUM);
    style.text.line_height = style.text.size;
    style.gap = cx.sp(2.0);
    disabled(&mut style, cx);
    style
}
