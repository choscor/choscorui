//! Form field text parts and the field layout: legends, labels, titles,
//! descriptions, errors, and separators, stacked or side by side.

use super::measured::{measured_layout, paint_wrapped};
use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::hosted;
use crate::style::{
    ControlKind, ControlSize, FieldOrientation, PaintContext, ResolvedStyle, Slot, StyleQuery,
    Styles, TextAlign, VisualState,
};
use choscorui_core::{
    ChildMeasure, Constraints, CustomLayout, DesignTokens, EventReply, LayoutDirection, Measured,
    Point, Rect, SemanticNode, SemanticRole, Size,
};

/// Which text part of a field a [`FieldText`] is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldPart {
    /// The caption of a field set.
    Legend,
    /// The label of one field.
    Label,
    /// A title inside a field's content, beside a choice control.
    Title,
    /// Muted help text.
    Description,
    /// A validation message.
    Error,
    /// A rule between field groups, with optional text in its middle.
    Separator,
}

impl FieldPart {
    fn slot(self) -> Slot {
        match self {
            Self::Legend => Slot::Legend,
            Self::Label => Slot::Label,
            Self::Title => Slot::Title,
            Self::Description => Slot::Description,
            Self::Error => Slot::Error,
            Self::Separator => Slot::Separator,
        }
    }
}

/// One text part of a form field. Descriptions and errors wrap to the
/// width they are given. A field text takes no focus.
#[derive(Clone, Debug)]
pub struct FieldText {
    part: FieldPart,
    text: String,
    size: ControlSize,
}

impl FieldText {
    pub fn new(part: FieldPart, text: impl Into<String>) -> Self {
        Self {
            part,
            text: text.into(),
            size: ControlSize::Default,
        }
    }

    /// `Sm` gives a legend the label size.
    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn part(&self) -> FieldPart {
        self.part
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    fn query(&self, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Field, self.part.slot())
            .size(self.size)
            .state(state)
    }
}

impl CustomControl for FieldText {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(self.query(VisualState::NONE));
        if self.part == FieldPart::Separator {
            let width = cx.constraints.max.width;
            let width = if width.is_finite() { width } else { 0.0 };
            return Measured {
                size: Size::new(width, style.min_height),
                baseline: None,
            };
        }
        let natural = cx.text.width(&self.text, &style.text) + style.padding.horizontal();
        let max = cx.constraints.max.width;
        let width = if max.is_finite() {
            natural.min(max)
        } else {
            natural
        };
        let inner = (width - style.padding.horizontal()).max(1.0);
        let lines = measured_layout(&self.text, &style.text, inner, cx.text)
            .lines()
            .len()
            .max(1);
        let height = lines as f64 * style.text.line_height + style.padding.vertical();
        Measured {
            size: Size::new(width, height),
            baseline: None,
        }
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = VisualState::NONE.set(VisualState::DISABLED, cx.state.disabled());
        let style = cx.style(self.query(state));
        let bounds = cx.bounds;
        if self.part == FieldPart::Separator {
            let line = Rect::from_xywh(
                bounds.origin.x,
                bounds.center().y - 0.5,
                bounds.size.width,
                1.0,
            );
            let rule = ResolvedStyle {
                padding: crate::style::Edges::ZERO,
                ..style
            };
            cx.slot(line, &rule, |_, _| {});
            if !self.text.is_empty() {
                let chip = cx.style(StyleQuery::new(ControlKind::Field, Slot::Text));
                let width = cx.measure(&self.text, &chip) + chip.padding.horizontal();
                let rect = Rect::from_xywh(
                    bounds.center().x - width / 2.0,
                    bounds.origin.y,
                    width,
                    bounds.size.height,
                );
                let text = self.text.as_str();
                cx.slot(rect, &chip, |cx, inner| {
                    cx.label(inner, text, &chip, TextAlign::Center);
                });
            }
            return;
        }
        let text = self.text.as_str();
        cx.slot(bounds, &style, |cx, inner| {
            paint_wrapped(cx, inner, text, &style, TextAlign::Start);
        });
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = hosted::node(SemanticRole::StaticText, &self.text, Vec::new());
        node.state.invalid = self.part == FieldPart::Error;
        node.state.hidden = self.part == FieldPart::Separator && self.text.is_empty();
        node
    }

    fn focusable(&self) -> bool {
        false
    }
}

/// How far apart a field group, a field set, or a field's content places
/// its items.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldGap {
    /// Fields inside a field group: 28 pt.
    Group,
    /// The legend, description, and group of a field set: 24 pt.
    Set,
    /// The text parts of a field's content, and the parts beside the
    /// first part of a horizontal field: 4 pt.
    Content,
}

impl FieldGap {
    /// The gap in points, from the `Field` recipe.
    pub fn resolve(self, styles: &Styles, tokens: &DesignTokens) -> f64 {
        let slot = match self {
            Self::Group => Slot::Group,
            Self::Set => Slot::Root,
            Self::Content => Slot::Content,
        };
        styles
            .resolve(tokens, StyleQuery::new(ControlKind::Field, slot))
            .gap
    }
}

/// The layout of one form field: its parts in order, stacked or side by
/// side.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Field {
    pub orientation: FieldOrientation,
}

/// The narrowest width at which a responsive field goes side by side.
const RESPONSIVE_WIDTH: f64 = 448.0;

impl Field {
    pub fn new(orientation: FieldOrientation) -> Self {
        Self { orientation }
    }

    /// True when the parts go side by side at `width`.
    pub fn is_horizontal(&self, width: f64) -> bool {
        match self.orientation {
            FieldOrientation::Vertical => false,
            FieldOrientation::Horizontal => true,
            FieldOrientation::Responsive => width >= RESPONSIVE_WIDTH,
        }
    }

    /// The field layout with the gaps of the `Field` recipe. It is a
    /// [`CustomLayout`] for a layout node whose children are the parts.
    pub fn layout(&self, styles: &Styles, tokens: &DesignTokens) -> FieldLayout {
        let gap = |query: StyleQuery| styles.resolve(tokens, query).gap;
        FieldLayout {
            field: *self,
            item_gap: gap(StyleQuery::new(ControlKind::Field, Slot::Item).variant(self.orientation)),
            content_gap: FieldGap::Content.resolve(styles, tokens),
        }
    }
}

/// A [`Field`] with its gaps resolved. Stacked parts fill the width, with
/// the field gap between them. Side by side, the first part sits at the
/// leading edge, and the others stack beside it with the content gap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldLayout {
    field: Field,
    item_gap: f64,
    content_gap: f64,
}

impl FieldLayout {
    /// Places parts of `sizes` in a field `width` wide, from the origin.
    /// Rectangles come back in the order of the sizes. An infinite width
    /// gives each part its own width. `rtl` mirrors side-by-side parts.
    pub fn place(&self, width: f64, rtl: bool, sizes: &[Size]) -> Vec<Rect> {
        let widest = |sizes: &[Size]| sizes.iter().map(|size| size.width).fold(0.0, f64::max);
        if !self.field.is_horizontal(width) || sizes.len() < 2 {
            return stack(Point::new(0.0, 0.0), self.item_gap, sizes, |size| {
                let fill = if width.is_finite() { width } else { size.width };
                Size::new(fill, size.height)
            });
        }
        let lead = sizes[0];
        let rest = &sizes[1..];
        let (width, column) = if width.is_finite() {
            (width, (width - lead.width - self.item_gap).max(0.0))
        } else {
            let column = widest(rest);
            (lead.width + self.item_gap + column, column)
        };
        let rest_height: f64 = rest.iter().map(|size| size.height).sum::<f64>()
            + self.content_gap * rest.len().saturating_sub(1) as f64;
        let height = lead.height.max(rest_height);
        let mirror = |rect: Rect| {
            if rtl {
                Rect::from_xywh(
                    width - rect.max_x(),
                    rect.origin.y,
                    rect.size.width,
                    rect.size.height,
                )
            } else {
                rect
            }
        };
        // One line of content centers on the lead; more align to the top.
        let lead_y = if rest.len() == 1 {
            (height - lead.height) / 2.0
        } else {
            0.0
        };
        let mut rects = vec![mirror(Rect::new(Point::new(0.0, lead_y), lead))];
        let start = Point::new(lead.width + self.item_gap, 0.0);
        rects.extend(
            stack(start, self.content_gap, rest, |size| {
                Size::new(column, size.height)
            })
            .into_iter()
            .map(mirror),
        );
        rects
    }
}

impl CustomLayout for FieldLayout {
    /// Measures the parts at the width that each gets, and places them
    /// with [`FieldLayout::place`]. A finite width fills it.
    fn layout(
        &self,
        space: Constraints,
        direction: LayoutDirection,
        children: &mut dyn ChildMeasure,
    ) -> (Size, Vec<Rect>) {
        let width = space.max.width;
        let count = children.len();
        let loose = |width: f64| Constraints::loose(Size::new(width, f64::INFINITY));
        let mut sizes = Vec::with_capacity(count);
        if self.field.is_horizontal(width) && count >= 2 {
            let lead = children.measure(0, loose(width)).size;
            let column = (width - lead.width - self.item_gap).max(0.0);
            sizes.push(lead);
            sizes.extend((1..count).map(|index| children.measure(index, loose(column)).size));
        } else {
            sizes.extend((0..count).map(|index| children.measure(index, loose(width)).size));
        }
        let rects = self.place(width, direction == LayoutDirection::Rtl, &sizes);
        let extent = |edge: fn(Rect) -> f64| rects.iter().copied().map(edge).fold(0.0, f64::max);
        let used = if width.is_finite() {
            width
        } else {
            extent(Rect::max_x)
        };
        (Size::new(used, extent(Rect::max_y)), rects)
    }
}

/// Places `sizes` top to bottom from `origin` with `gap` between them.
fn stack(origin: Point, gap: f64, sizes: &[Size], fit: impl Fn(Size) -> Size) -> Vec<Rect> {
    let mut y = origin.y;
    sizes
        .iter()
        .map(|size| {
            let rect = Rect::new(Point::new(origin.x, y), fit(*size));
            y += size.height + gap;
            rect
        })
        .collect()
}
