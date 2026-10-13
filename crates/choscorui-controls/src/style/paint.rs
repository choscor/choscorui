//! The general slot painter and text helpers that every control uses.

use super::{
    ControlKind, Corners, Cursor, Orientation, ResolvedStyle, Slot, StyleQuery, Styles,
    Transitions, VisualState,
};
use choscorui_core::{
    Color, DesignTokens, DisplayList, FillRule, Icon, Localizer, Paint, Path, Point, Rect, Shadow,
    Size, StrokeStyle, TextRun, TextShaper, TypeSpec,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// Measures one line of text, for layout and alignment.
pub trait TextMeasure {
    /// The advance width of `text` in `spec`.
    fn width(&self, text: &str, spec: &TypeSpec) -> f64;
}

/// A deterministic estimate (0.55 em per character plus tracking) for
/// headless hosts and tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct EstimatedText;

impl TextMeasure for EstimatedText {
    fn width(&self, text: &str, spec: &TypeSpec) -> f64 {
        let count = choscorui_core::grapheme_boundaries(text)
            .len()
            .saturating_sub(1) as f64;
        count * (spec.size * 0.55 + spec.tracking)
    }
}

/// Measures with a real shaper and remembers each width.
pub struct ShapedText<'a> {
    shaper: &'a dyn TextShaper,
    widths: RefCell<HashMap<(String, u64, u16, u64), f64>>,
}

impl<'a> ShapedText<'a> {
    pub fn new(shaper: &'a dyn TextShaper) -> Self {
        Self {
            shaper,
            widths: RefCell::new(HashMap::new()),
        }
    }
}

impl TextMeasure for ShapedText<'_> {
    fn width(&self, text: &str, spec: &TypeSpec) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        let key = (
            text.to_owned(),
            spec.size.to_bits(),
            spec.weight.0,
            spec.tracking.to_bits() ^ u64::from(spec.family.name().len() as u32),
        );
        if let Some(width) = self.widths.borrow().get(&key) {
            return *width;
        }
        let width = self
            .shaper
            .shape(text, &spec.text_style(), None)
            .map_or_else(
                |_| EstimatedText.width(text, spec),
                |layout| layout.size().width,
            );
        let mut widths = self.widths.borrow_mut();
        if widths.len() > 4096 {
            widths.clear();
        }
        widths.insert(key, width);
        width
    }
}

/// Horizontal placement of a text line in its box, in reading order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

fn rounded_path(rect: Rect, corners: &Corners, rtl: bool) -> Path {
    Path::rounded_rect_corners(rect, corners.physical(rtl))
}

/// `paint` placed on `rect`. A solid paint stays the same. A gradient's
/// points go from the unit square of the box onto `rect`, with x from the
/// start side, and a radial radius scales by the longer side.
pub fn paint_in(paint: &Paint, rect: Rect, rtl: bool) -> Paint {
    let place = |point: Point| {
        let x = if rtl {
            rect.max_x() - point.x * rect.size.width
        } else {
            rect.origin.x + point.x * rect.size.width
        };
        Point::new(x, rect.origin.y + point.y * rect.size.height)
    };
    match paint {
        Paint::Solid(_) => paint.clone(),
        Paint::LinearGradient { start, end, stops } => Paint::LinearGradient {
            start: place(*start),
            end: place(*end),
            stops: stops.clone(),
        },
        Paint::RadialGradient {
            center,
            radius,
            stops,
        } => Paint::RadialGradient {
            center: place(*center),
            radius: radius * rect.size.width.max(rect.size.height),
            stops: stops.clone(),
        },
    }
}

/// Fills `rect` with `paint` and the corner radii. A gradient is placed
/// on `rect` as [`paint_in`] describes.
pub fn fill_shape(list: &mut DisplayList, rect: Rect, corners: &Corners, rtl: bool, paint: &Paint) {
    if paint.is_transparent() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    let square = corners.is_uniform() && corners.top_start <= 0.0;
    match paint.solid_color() {
        Some(color) if square => list.fill_rect(rect, color),
        Some(color) if corners.is_uniform() => {
            let _ = list.fill_rounded_rect(rect, corners.top_start, color);
        }
        _ => {
            let path = if square {
                Path::rect(rect)
            } else {
                rounded_path(rect, corners, rtl)
            };
            let _ = list.fill_path(path, paint_in(paint, rect, rtl), FillRule::NonZero);
        }
    }
}

/// Fills the square `rect` with `paint`, for a line or a band of a slot.
/// A solid paint always emits one rectangle fill, even when it is
/// transparent.
pub fn fill_area(list: &mut DisplayList, rect: Rect, rtl: bool, paint: &Paint) {
    match paint.solid_color() {
        Some(color) => list.fill_rect(rect, color),
        None => fill_shape(list, rect, &Corners::ZERO, rtl, paint),
    }
}

/// Paints the box of a slot: drop shadows, ring, backdrop blur,
/// background, inset shadows, then the border inside the box. Shadows,
/// the ring, and the blur follow the radius of each corner.
pub fn paint_box(list: &mut DisplayList, rect: Rect, style: &ResolvedStyle, rtl: bool) {
    let radii = clamp_corners(style.radius).physical(rtl);
    let uniform = style.radius.is_uniform();
    let shadow = |list: &mut DisplayList, shadow: &Shadow| {
        let _ = if uniform {
            list.draw_shadow(rect, radii[0], *shadow)
        } else {
            list.draw_shadow_corners(rect, radii, *shadow)
        };
    };
    for layer in style.shadow.iter().filter(|layer| !layer.inset) {
        shadow(list, layer);
    }
    if let Some(ring) = style.ring
        && ring.width > 0.0
        && ring.color.alpha > 0
    {
        if uniform {
            let _ = list.draw_ring(rect, radii[0], ring.width, ring.color);
        } else {
            let half = ring.width / 2.0;
            let grown = grow(style.radius, half);
            let _ = list.stroke_path(
                rounded_path(rect.inset(-half), &grown, rtl),
                Paint::Solid(ring.color),
                StrokeStyle::new(ring.width),
            );
        }
    }
    if style.backdrop_blur > 0.0 {
        let _ = if uniform {
            list.backdrop_blur(rect, radii[0], style.backdrop_blur)
        } else {
            list.backdrop_blur_corners(rect, radii, style.backdrop_blur)
        };
    }
    fill_shape(list, rect, &style.radius, rtl, &style.background);
    for layer in style.shadow.iter().filter(|layer| layer.inset) {
        shadow(list, layer);
    }
    if style.has_border() {
        paint_border(list, rect, style, rtl);
    }
}

/// The corner radii with negative values raised to zero.
fn clamp_corners(corners: Corners) -> Corners {
    Corners {
        top_start: corners.top_start.max(0.0),
        top_end: corners.top_end.max(0.0),
        bottom_end: corners.bottom_end.max(0.0),
        bottom_start: corners.bottom_start.max(0.0),
    }
}

fn grow(corners: Corners, by: f64) -> Corners {
    let g = |r: f64| if r > 0.0 { r + by } else { 0.0 };
    Corners {
        top_start: g(corners.top_start),
        top_end: g(corners.top_end),
        bottom_end: g(corners.bottom_end),
        bottom_start: g(corners.bottom_start),
    }
}

fn paint_border(list: &mut DisplayList, rect: Rect, style: &ResolvedStyle, rtl: bool) {
    let width = style.border_width;
    let half = width / 2.0;
    if style.border_sides.is_all() {
        let inner = rect.inset(half);
        let corners = grow(style.radius, -half);
        if corners.is_uniform() {
            let _ =
                list.stroke_rounded_rect(inner, corners.top_start.max(0.0), style.border, width);
        } else {
            let _ = list.stroke_path(
                rounded_path(inner, &corners, rtl),
                Paint::Solid(style.border),
                StrokeStyle::new(width),
            );
        }
        return;
    }
    let sides = style.border_sides;
    let (left, right) = if rtl {
        (sides.inline_end, sides.inline_start)
    } else {
        (sides.inline_start, sides.inline_end)
    };
    let (x, y, w, h) = (
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    );
    if sides.block_start {
        list.fill_rect(Rect::from_xywh(x, y, w, width), style.border);
    }
    if sides.block_end {
        list.fill_rect(Rect::from_xywh(x, y + h - width, w, width), style.border);
    }
    if left {
        list.fill_rect(Rect::from_xywh(x, y, width, h), style.border);
    }
    if right {
        list.fill_rect(Rect::from_xywh(x + w - width, y, width, h), style.border);
    }
}

/// The general slot painter. It applies the slot's opacity and pressed
/// offset, paints the box, then calls `content` with the box inside the
/// padding.
pub fn paint_slot(
    list: &mut DisplayList,
    rect: Rect,
    style: &ResolvedStyle,
    rtl: bool,
    content: impl FnOnce(&mut DisplayList, Rect),
) {
    let rect = rect.translate(0.0, style.offset_y);
    let faded = style.opacity < 1.0;
    if faded {
        let _ = list.push_opacity(style.opacity.clamp(0.0, 1.0));
    }
    paint_box(list, rect, style, rtl);
    content(list, content_rect(rect, style, rtl));
    if faded {
        let _ = list.pop_opacity();
    }
}

/// The part of `rect` inside the style's padding.
pub fn content_rect(rect: Rect, style: &ResolvedStyle, rtl: bool) -> Rect {
    let (left, right) = style.padding.left_right(rtl);
    Rect::new(
        Point::new(
            rect.origin.x + left,
            rect.origin.y + style.padding.block_start,
        ),
        Size::new(
            (rect.size.width - left - right).max(0.0),
            (rect.size.height - style.padding.vertical()).max(0.0),
        ),
    )
}

/// The x position of a run of `width` in `rect` for the alignment and
/// direction.
pub fn align_x(rect: Rect, width: f64, align: TextAlign, rtl: bool) -> f64 {
    let start = match (align, rtl) {
        (TextAlign::Center, _) => return rect.origin.x + (rect.size.width - width) / 2.0,
        (TextAlign::Start, false) | (TextAlign::End, true) => true,
        _ => false,
    };
    if start {
        rect.origin.x
    } else {
        rect.max_x() - width
    }
}

/// One line of text and how to place it.
#[derive(Clone, Copy, Debug)]
pub struct TextLine<'a> {
    pub text: &'a str,
    pub spec: TypeSpec,
    pub color: Color,
    pub align: TextAlign,
    pub rtl: bool,
    pub underline: bool,
}

impl<'a> TextLine<'a> {
    /// `text` in the style's type and foreground, at the start.
    pub fn new(text: &'a str, style: &ResolvedStyle, rtl: bool) -> Self {
        Self {
            text,
            spec: style.text,
            color: style.foreground,
            align: TextAlign::Start,
            rtl,
            underline: style.underline,
        }
    }

    #[must_use]
    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    #[must_use]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

/// Draws one line vertically centered in `rect`, with an underline 1 pt
/// below the baseline when asked. Returns the drawn rectangle.
pub fn draw_text(
    list: &mut DisplayList,
    rect: Rect,
    line: &TextLine<'_>,
    measure: &dyn TextMeasure,
) -> Rect {
    let spec = &line.spec;
    if line.text.is_empty() || line.color.alpha == 0 {
        return Rect::new(rect.origin, Size::new(0.0, 0.0));
    }
    let width = measure.width(line.text, spec);
    let x = align_x(rect, width, line.align, line.rtl);
    let top = rect.origin.y + (rect.size.height - spec.line_height) / 2.0;
    let run = TextRun {
        family: spec.family.name(),
        size: spec.size,
        weight: spec.weight,
        tracking: spec.tracking,
    };
    let baseline = top + spec.baseline();
    let _ = list.draw_text_run(Point::new(x, baseline), line.text, &run, line.color);
    if line.underline {
        let y = baseline + (spec.size * 0.12).max(1.0);
        list.fill_rect(Rect::from_xywh(x, y, width, 1.0), line.color);
    }
    Rect::from_xywh(x, top, width, spec.line_height)
}

/// Draws `icon` at `size` centered in `rect`.
pub fn draw_icon(list: &mut DisplayList, rect: Rect, icon: Icon, size: f64, color: Color) {
    if size <= 0.0 || color.alpha == 0 {
        return;
    }
    let center = rect.center();
    let _ = icon.draw(
        list,
        Rect::from_xywh(center.x - size / 2.0, center.y - size / 2.0, size, size),
        color,
    );
}

/// Everything a control needs to paint one node: the display list, the
/// node's bounds, the tokens, the style cache, text measurement, the
/// layout direction, the messages, and the host's view of the node's
/// state.
pub struct PaintContext<'a> {
    pub list: &'a mut DisplayList,
    pub bounds: Rect,
    pub tokens: &'a DesignTokens,
    pub styles: &'a Styles,
    pub text: &'a dyn TextMeasure,
    pub rtl: bool,
    /// Hover, focus, and focus-visible as the host tracks them.
    pub state: VisualState,
    /// Monotonic time in seconds, for running animations.
    pub time: f64,
    /// The messages of the window locale, for the built-in text that a
    /// control paints.
    pub strings: &'a Localizer,
    key: u64,
    /// The host's transition store, when the host eases state changes.
    transitions: Option<&'a Transitions>,
    /// The pointer position and where the cursor of the slot under it
    /// goes, while the node is the hovered one.
    pointer: Option<(Point, &'a Cell<Cursor>)>,
    /// True inside the group opacity of a disabled control.
    dimmed: bool,
}

impl<'a> PaintContext<'a> {
    /// A context that paints into `list` inside `bounds`, left-to-right,
    /// with no visual state, at time zero, with the built-in English
    /// messages.
    pub fn new(
        list: &'a mut DisplayList,
        bounds: Rect,
        tokens: &'a DesignTokens,
        styles: &'a Styles,
        text: &'a dyn TextMeasure,
    ) -> Self {
        Self {
            list,
            bounds,
            tokens,
            styles,
            text,
            rtl: false,
            state: VisualState::NONE,
            time: 0.0,
            strings: Localizer::builtin(),
            key: tokens.fingerprint(),
            transitions: None,
            pointer: None,
            dimmed: false,
        }
    }

    /// Sets the right-to-left layout direction.
    #[must_use]
    pub fn rtl(mut self, rtl: bool) -> Self {
        self.rtl = rtl;
        self
    }

    /// Sets the hover, focus, and disabled flags of the node.
    #[must_use]
    pub fn state(mut self, state: VisualState) -> Self {
        self.state = state;
        self
    }

    /// Sets the time in seconds for running animations.
    #[must_use]
    pub fn time(mut self, time: f64) -> Self {
        self.time = time;
        self
    }

    /// Sets the messages of the window locale.
    #[must_use]
    pub fn strings(mut self, strings: &'a Localizer) -> Self {
        self.strings = strings;
        self
    }

    /// Eases the styles that [`PaintContext::style`] returns through the
    /// host's transition store.
    #[must_use]
    pub(crate) fn transitions(mut self, transitions: &'a Transitions) -> Self {
        self.transitions = Some(transitions);
        self
    }

    /// The same context for another rectangle, such as a child part.
    pub fn at(&mut self, bounds: Rect) -> PaintContext<'_> {
        PaintContext {
            list: self.list,
            bounds,
            tokens: self.tokens,
            styles: self.styles,
            text: self.text,
            rtl: self.rtl,
            state: self.state,
            time: self.time,
            strings: self.strings,
            key: self.key,
            transitions: self.transitions,
            pointer: self.pointer,
            dimmed: self.dimmed,
        }
    }

    /// Records into `sink` the cursor of each painted slot under `point`,
    /// so that the host shows the cursor of the hovered part.
    #[must_use]
    pub(crate) fn pointer(mut self, point: Point, sink: &'a Cell<Cursor>) -> Self {
        self.pointer = Some((point, sink));
        self
    }

    /// Marks the context as inside the group opacity of a disabled
    /// control. Disabled slots then do not fade again.
    #[must_use]
    pub(crate) fn dimmed(mut self, dimmed: bool) -> Self {
        self.dimmed = dimmed;
        self
    }

    /// Shows `cursor` while the pointer is over `rect`, for a part that a
    /// control paints without [`PaintContext::slot`]. A later part over
    /// the same point wins. [`Cursor::Auto`] changes nothing.
    pub fn cursor(&self, rect: Rect, cursor: Cursor) {
        if let Some((point, sink)) = self.pointer
            && cursor != Cursor::Auto
            && rect.contains(point)
        {
            sink.set(cursor);
        }
    }

    /// The resolved style of `query`, from the cache. While the host eases
    /// a state change of the node, the colors, ring, shadows, opacity, and
    /// offset are part of the way from the earlier style.
    pub fn style(&self, query: StyleQuery) -> ResolvedStyle {
        let target = self.styles.resolve_keyed(self.key, self.tokens, query);
        let mut style = match self.transitions {
            Some(transitions) => transitions.apply(&query, target, self.tokens, |earlier| {
                self.styles.resolve_keyed(self.key, self.tokens, earlier)
            }),
            None => target,
        };
        if self.dimmed && query.state.disabled() {
            // The group opacity of the control carries the disabled look.
            style.opacity = 1.0;
        }
        style
    }

    /// Paints a slot box like [`paint_slot`], then calls `content` with
    /// a context for the box inside the padding.
    pub fn slot(
        &mut self,
        rect: Rect,
        style: &ResolvedStyle,
        content: impl FnOnce(&mut PaintContext<'_>, Rect),
    ) {
        let rect = rect.translate(0.0, style.offset_y);
        let faded = style.opacity < 1.0;
        if faded {
            let _ = self.list.push_opacity(style.opacity.clamp(0.0, 1.0));
        }
        paint_box(self.list, rect, style, self.rtl);
        self.cursor(rect, style.cursor);
        let inner = content_rect(rect, style, self.rtl);
        content(&mut self.at(inner), inner);
        if faded {
            let _ = self.list.pop_opacity();
        }
    }

    /// Draws one line of text in the style's type and color.
    pub fn label(
        &mut self,
        rect: Rect,
        text: &str,
        style: &ResolvedStyle,
        align: TextAlign,
    ) -> Rect {
        let line = TextLine::new(text, style, self.rtl).align(align);
        draw_text(self.list, rect, &line, self.text)
    }

    /// Draws an icon centered in `rect` in the style's icon size and color.
    pub fn icon(&mut self, rect: Rect, icon: Icon, style: &ResolvedStyle) {
        draw_icon(self.list, rect, icon, style.icon_size, style.foreground);
    }

    /// The width of `text` in `style`.
    pub fn measure(&self, text: &str, style: &ResolvedStyle) -> f64 {
        self.text.width(text, &style.text)
    }
}

/// Joins segment `index` of `count` into one bar: inner corners turn
/// square, and each segment after the first grows back over the border
/// before it, so neighbors share one line.
pub(crate) fn join_segment(
    style: &mut ResolvedStyle,
    rect: &mut Rect,
    index: usize,
    count: usize,
    orientation: Orientation,
    rtl: bool,
) {
    let last = index + 1 >= count;
    let corners = &mut style.radius;
    match orientation {
        Orientation::Horizontal => {
            if index > 0 {
                (corners.top_start, corners.bottom_start) = (0.0, 0.0);
                rect.size.width += style.border_width;
                if !rtl {
                    rect.origin.x -= style.border_width;
                }
            }
            if !last {
                (corners.top_end, corners.bottom_end) = (0.0, 0.0);
            }
        }
        Orientation::Vertical => {
            if index > 0 {
                (corners.top_start, corners.top_end) = (0.0, 0.0);
                rect.origin.y -= style.border_width;
                rect.size.height += style.border_width;
            }
            if !last {
                (corners.bottom_start, corners.bottom_end) = (0.0, 0.0);
            }
        }
    }
}

/// The content of a button-like slot: an optional leading icon, a label,
/// and an optional trailing icon, with the style's gap.
#[derive(Clone, Copy, Debug, Default)]
pub struct Content<'a> {
    pub leading: Option<Icon>,
    pub label: &'a str,
    pub trailing: Option<Icon>,
}

impl<'a> Content<'a> {
    pub fn label(label: &'a str) -> Self {
        Self {
            label,
            ..Self::default()
        }
    }

    /// The natural width of the content in `style`.
    pub fn width(&self, style: &ResolvedStyle, measure: &dyn TextMeasure) -> f64 {
        let mut parts = Vec::with_capacity(3);
        if self.leading.is_some() {
            parts.push(style.icon_size);
        }
        if !self.label.is_empty() {
            parts.push(measure.width(self.label, &style.text));
        }
        if self.trailing.is_some() {
            parts.push(style.icon_size);
        }
        let gaps = parts.len().saturating_sub(1) as f64 * style.gap;
        parts.iter().sum::<f64>() + gaps
    }

    /// The size of a slot that holds the content: padding, content, and
    /// the style's minimum size.
    pub fn size(&self, style: &ResolvedStyle, measure: &dyn TextMeasure) -> Size {
        let width = self.width(style, measure) + style.padding.horizontal();
        let height = style.text.line_height.max(style.icon_size) + style.padding.vertical();
        Size::new(width.max(style.min_width), height.max(style.min_height))
    }

    /// Draws the content in `rect`, aligned in reading order.
    pub fn draw(
        &self,
        cx: &mut PaintContext<'_>,
        rect: Rect,
        style: &ResolvedStyle,
        align: TextAlign,
    ) {
        let width = self.width(style, cx.text);
        let mut x = align_x(rect, width, align, cx.rtl);
        let mut parts: Vec<(Option<Icon>, &str)> = Vec::with_capacity(3);
        if let Some(icon) = self.leading {
            parts.push((Some(icon), ""));
        }
        if !self.label.is_empty() {
            parts.push((None, self.label));
        }
        if let Some(icon) = self.trailing {
            parts.push((Some(icon), ""));
        }
        if cx.rtl {
            parts.reverse();
        }
        for (icon, text) in parts {
            let part = match icon {
                Some(_) => style.icon_size,
                None => cx.text.width(text, &style.text),
            };
            let slot = Rect::from_xywh(x, rect.origin.y, part, rect.size.height);
            match icon {
                Some(icon) => cx.icon(slot, icon, style),
                None => {
                    let line = TextLine::new(text, style, false);
                    draw_text(cx.list, slot, &line, cx.text);
                }
            }
            x += part + style.gap;
        }
    }
}

/// The size of a checkbox-like choice of `kind`: the `Root` indicator,
/// the `Label` gap, and the label.
pub(crate) fn choice_size(
    kind: ControlKind,
    label: &str,
    styles: &Styles,
    tokens: &DesignTokens,
    text: &dyn TextMeasure,
) -> Size {
    let indicator = styles.resolve(tokens, StyleQuery::new(kind, Slot::Root));
    if label.is_empty() {
        return Size::new(indicator.min_width, indicator.min_height);
    }
    let style = styles.resolve(tokens, StyleQuery::new(kind, Slot::Label));
    Size::new(
        indicator.min_width + style.gap + text.width(label, &style.text),
        indicator.min_height.max(style.text.line_height),
    )
}

/// Paints a checkbox-like choice of `kind`: the `Root` indicator box at
/// the start of the bounds, `mark` inside its padding, then the label
/// after the `Label` gap.
pub(crate) fn paint_choice(
    cx: &mut PaintContext<'_>,
    kind: ControlKind,
    label: &str,
    state: VisualState,
    mark: impl FnOnce(&mut PaintContext<'_>, Rect),
) {
    let query = |slot| StyleQuery::new(kind, slot).state(state);
    let indicator = cx.style(query(Slot::Root));
    let bounds = cx.bounds;
    let size = Size::new(indicator.min_width, indicator.min_height);
    let x = if cx.rtl {
        bounds.max_x() - size.width
    } else {
        bounds.origin.x
    };
    let rect = Rect::new(
        Point::new(
            x,
            bounds.origin.y + (bounds.size.height - size.height) / 2.0,
        ),
        size,
    );
    cx.slot(rect, &indicator, |cx, _| mark(cx, rect));
    if label.is_empty() {
        return;
    }
    let style = cx.style(query(Slot::Label));
    let inset = size.width + style.gap;
    let x = if cx.rtl {
        bounds.origin.x
    } else {
        bounds.origin.x + inset
    };
    let text = Rect::new(
        Point::new(x, bounds.origin.y),
        Size::new((bounds.size.width - inset).max(0.0), bounds.size.height),
    );
    cx.slot(text, &style, |cx, inner| {
        cx.label(inner, label, &style, TextAlign::Start);
    });
}
