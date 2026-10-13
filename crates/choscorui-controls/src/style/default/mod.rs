//! The default recipe set. Each group module holds the values of its
//! controls, copied from the reference style. Values are in points; one
//! spacing unit is the `spacing` token.

mod chat;
mod data;
mod feedback;
mod form;
mod layout;
mod navigation;
mod overlay;

use super::{ControlKind, Corners, Edges, ResolvedStyle, Ring, StyleQuery, VisualState};
use choscorui_core::{
    Color, ColorRole, ColorScheme, DesignTokens, FontWeight, RadiusScale, ShadowLayers,
    ShadowScale, TextSize, TypeSpec,
};

/// The default style of `query`.
pub(super) fn resolve(query: &StyleQuery, tokens: &DesignTokens) -> ResolvedStyle {
    use ControlKind::*;
    let cx = Cx {
        t: tokens,
        q: query,
    };
    match query.kind {
        Button | ButtonGroup | Checkbox | Combobox | DatePicker | Input | InputOtp | InputGroup
        | Label | RadioGroup | Select | NativeSelect | Slider | Switch | Textarea | Toggle
        | ToggleGroup | Field | Kbd => form::resolve(&cx),
        AlertDialog | Dialog | Sheet | Drawer | Popover | Tooltip | HoverCard | Menu | Menubar
        | Command => overlay::resolve(&cx),
        Tabs | Breadcrumb | Pagination | NavigationMenu | Sidebar | Toolbar | StatusBar | Dock => {
            navigation::resolve(&cx)
        }
        Table | Card | Badge | Avatar | Chart | Calendar | Carousel | ListView | Dial
        | NumericDisplay | ColorPicker => data::resolve(&cx),
        Alert | Toast | Progress | Skeleton | Spinner | Empty => feedback::resolve(&cx),
        Separator | Resizable | ScrollArea | AspectRatio | Collapsible | Accordion | Item
        | Typography | SizeGrip | Canvas | TextEditor => layout::resolve(&cx),
        Message | Bubble | Attachment | Marker | MessageScroller | Questionnaire => {
            chat::resolve(&cx)
        }
    }
}

/// The query and tokens, with short helpers for the value tables.
#[derive(Clone, Copy)]
pub(super) struct Cx<'a> {
    pub t: &'a DesignTokens,
    pub q: &'a StyleQuery,
}

impl Cx<'_> {
    pub fn dark(&self) -> bool {
        self.t.scheme == ColorScheme::Dark
    }

    pub fn s(&self) -> VisualState {
        self.q.state
    }

    /// A role color.
    pub fn c(&self, role: ColorRole) -> Color {
        self.t.color(role)
    }

    /// A role color at `alpha`, as the reference `role/NN` modifier.
    pub fn a(&self, role: ColorRole, alpha: f64) -> Color {
        self.t.color(role).with_alpha(alpha)
    }

    /// A light-scheme or dark-scheme value.
    pub fn pick<T>(&self, light: T, dark: T) -> T {
        if self.dark() { dark } else { light }
    }

    /// `units` spacing steps.
    pub fn sp(&self, units: f64) -> f64 {
        self.t.space(units)
    }

    pub fn r(&self, step: RadiusScale) -> f64 {
        self.t.radius(step)
    }

    pub fn text(&self, size: TextSize, weight: FontWeight) -> TypeSpec {
        self.t.text(size, weight)
    }

    pub fn shadow(&self, step: ShadowScale) -> ShadowLayers {
        self.t.shadow(step)
    }

    /// A transparent slot in foreground text at the small size.
    pub fn base(&self) -> ResolvedStyle {
        ResolvedStyle::new(self.t)
    }

    /// An animation duration in milliseconds, zero with reduced motion.
    pub fn motion(&self, ms: u32) -> u32 {
        if self.t.motion.reduced { 0 } else { ms }
    }

    pub fn ring(&self, width: f64, color: Color) -> Option<Ring> {
        Some(Ring { width, color })
    }

    /// The soft destructive ring of an invalid field, and of a focused
    /// destructive button or badge.
    pub fn invalid_ring(&self) -> Option<Ring> {
        self.ring(
            self.t.ring_width,
            self.a(ColorRole::Destructive, self.pick(0.2, 0.4)),
        )
    }

    /// The standard focus ring: `ring` at the ring alpha.
    pub fn focus_ring(&self) -> Option<Ring> {
        self.ring(
            self.t.ring_width,
            self.t.ring_color(self.c(ColorRole::Ring)),
        )
    }
}

/// The focus-visible look: the border turns to `ring` and the ring shows.
pub(super) fn focus(style: &mut ResolvedStyle, cx: &Cx) {
    if cx.s().focus_visible() {
        style.border = cx.c(ColorRole::Ring);
        style.ring = cx.focus_ring();
    }
}

/// The invalid look of a text or select field. The border is destructive
/// at full strength. The soft destructive ring shows only while focus does
/// not show, so a focused invalid field keeps the focus ring. Call it after
/// `focus`.
pub(super) fn field_invalid(style: &mut ResolvedStyle, cx: &Cx) {
    if cx.s().invalid() {
        style.border = cx.c(ColorRole::Destructive);
        if !cx.s().focus_visible() {
            style.ring = cx.invalid_ring();
        }
    }
}

/// The invalid look of a checkbox or radio box: a destructive border,
/// also when checked, no shadow, and no ring other than the focus ring.
/// Call it after `focus`.
pub(super) fn choice_invalid(style: &mut ResolvedStyle, cx: &Cx) {
    if cx.s().invalid() {
        style.border = cx.c(ColorRole::Destructive);
        style.shadow = ShadowLayers::NONE;
        if !cx.s().focus_visible() {
            style.ring = None;
        }
    }
}

/// The opacity of a disabled slot, and through the button root, of the
/// group over a disabled control.
pub(super) const DISABLED_OPACITY: f64 = 0.5;

/// The disabled look: half opacity.
pub(super) fn disabled(style: &mut ResolvedStyle, cx: &Cx) {
    if cx.s().disabled() {
        style.opacity = DISABLED_OPACITY;
        style.cursor = super::Cursor::Arrow;
    }
}

/// Focus, invalid, and disabled of a text or select field.
pub(super) fn field_states(style: &mut ResolvedStyle, cx: &Cx) {
    focus(style, cx);
    field_invalid(style, cx);
    disabled(style, cx);
}

/// The box of a text input: an input border, a translucent fill in dark,
/// a medium radius, and the extra-small shadow.
pub(super) fn field_box(cx: &Cx) -> ResolvedStyle {
    let mut style = cx.base();
    style.border = cx.c(ColorRole::Input);
    style.border_width = cx.t.border_width;
    style.background = cx
        .pick(Color::TRANSPARENT, cx.a(ColorRole::Input, 0.3))
        .into();
    style.radius = Corners::all(cx.r(RadiusScale::Md));
    style.shadow = cx.shadow(ShadowScale::Xs);
    style.min_height = cx.sp(9.0);
    style.padding = Edges::axes(cx.sp(2.5), cx.sp(1.0));
    style
}

/// The floating surface of popovers, menus, and lists: the popover color,
/// a 1 pt hairline ring, and the medium shadow.
pub(super) fn surface(cx: &Cx, radius: RadiusScale) -> ResolvedStyle {
    let mut style = cx.base();
    style.background = cx.c(ColorRole::Popover).into();
    style.foreground = cx.c(ColorRole::PopoverForeground);
    style.ring = cx.ring(1.0, cx.a(ColorRole::Foreground, 0.1));
    style.radius = Corners::all(cx.r(radius));
    style.shadow = cx.shadow(ShadowScale::Md);
    style.motion_ms = cx.motion(100);
    style
}

/// A row of a menu or list: highlighted rows take the accent color.
pub(super) fn list_item(cx: &Cx, destructive: bool) -> ResolvedStyle {
    let mut style = cx.base();
    style.radius = Corners::all(cx.r(RadiusScale::Sm));
    style.gap = cx.sp(2.0);
    style.padding = Edges {
        inline_start: cx.sp(2.0),
        inline_end: cx.sp(2.0),
        block_start: cx.sp(1.5),
        block_end: cx.sp(1.5),
    };
    style.min_height = cx.sp(8.0);
    if destructive {
        style.foreground = cx.c(ColorRole::Destructive);
        if cx.s().highlighted() {
            style.background = cx.a(ColorRole::Destructive, cx.pick(0.1, 0.2)).into();
        }
    } else if cx.s().highlighted() {
        style.background = cx.c(ColorRole::Accent).into();
        style.foreground = cx.c(ColorRole::AccentForeground);
    }
    disabled(&mut style, cx);
    style
}

/// A small muted caption above a group of rows.
pub(super) fn group_label(cx: &Cx) -> ResolvedStyle {
    let mut style = cx.base();
    style.foreground = cx.c(ColorRole::MutedForeground);
    style.padding = Edges::axes(cx.sp(2.0), cx.sp(1.5));
    style.text = cx.text(TextSize::Xs, FontWeight::NORMAL);
    style
}

/// A 1 pt rule in the border color with `margin` block insets.
pub(super) fn rule(cx: &Cx, margin: f64) -> ResolvedStyle {
    let mut style = cx.base();
    style.background = cx.c(ColorRole::Border).into();
    style.min_height = 1.0;
    style.min_width = 1.0;
    style.padding = Edges::axes(-cx.sp(1.0), margin);
    style
}

/// Muted secondary text at the small size.
pub(super) fn muted_text(cx: &Cx, size: TextSize) -> ResolvedStyle {
    let mut style = cx.base();
    style.foreground = cx.c(ColorRole::MutedForeground);
    style.text = cx.text(size, FontWeight::NORMAL);
    style
}

/// Text in `role` at `size` and `weight`.
pub(super) fn text(cx: &Cx, role: ColorRole, size: TextSize, weight: FontWeight) -> ResolvedStyle {
    let mut style = cx.base();
    style.foreground = cx.c(role);
    style.text = cx.text(size, weight);
    style
}
