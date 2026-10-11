//! Form field text parts and the field layout: legends, labels, titles,
//! descriptions, errors, and separators, stacked or side by side.

use super::measured::measured_layout;
use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::hosted;
use crate::style::{
    ControlKind, ControlSize, FieldOrientation, PaintContext, ResolvedStyle, Slot, StyleQuery,
    Styles, TextAlign, VisualState,
};
use choscorui_core::{
    DesignTokens, EventReply, Measured, Point, Rect, SemanticNode, SemanticRole, Size,
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
            let layout = measured_layout(text, &style.text, inner.size.width.max(1.0), cx.text);
            for line in layout.lines() {
                let rect = Rect::from_xywh(
                    inner.origin.x,
                    inner.origin.y + line.baseline - style.text.baseline(),
                    inner.size.width,
                    style.text.line_height,
                );
                cx.label(rect, &text[line.bytes.clone()], &style, TextAlign::Start);
            }
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

/// How far apart [`Field::stack`] places its items.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldGap {
    /// Fields inside a field group: 28 pt.
    Group,
    /// The legend, description, and group of a field set: 24 pt.
    Set,
}

/// The layout of one form field: its parts in order, stacked or side by
/// side. Rectangles come back in the order of the sizes given.
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

    fn item(&self, styles: &Styles, tokens: &DesignTokens) -> ResolvedStyle {
        styles.resolve(
            tokens,
            StyleQuery::new(ControlKind::Field, Slot::Item).variant(self.orientation),
        )
    }

    /// True when the parts go side by side at `width`.
    pub fn is_horizontal(&self, width: f64) -> bool {
        match self.orientation {
            FieldOrientation::Vertical => false,
            FieldOrientation::Horizontal => true,
            FieldOrientation::Responsive => width >= RESPONSIVE_WIDTH,
        }
    }

    /// Places `sizes` in `bounds`. Stacked parts fill the width, with the
    /// field gap between them. Side by side, the first part sits at the
    /// leading edge and the others stack beside it with the content gap.
    pub fn layout(
        &self,
        styles: &Styles,
        tokens: &DesignTokens,
        bounds: Rect,
        rtl: bool,
        sizes: &[Size],
    ) -> Vec<Rect> {
        let item = self.item(styles, tokens);
        if !self.is_horizontal(bounds.size.width) || sizes.len() < 2 {
            return stack(bounds.origin, item.gap, sizes, |size| {
                Size::new(bounds.size.width, size.height)
            });
        }
        let content = styles.resolve(tokens, StyleQuery::new(ControlKind::Field, Slot::Content));
        let lead = sizes[0];
        let rest = &sizes[1..];
        let column = (bounds.size.width - lead.width - item.gap).max(0.0);
        let rest_height: f64 = rest.iter().map(|size| size.height).sum::<f64>()
            + content.gap * rest.len().saturating_sub(1) as f64;
        let height = lead.height.max(rest_height);
        let mirror = |rect: Rect| {
            if rtl {
                Rect::from_xywh(
                    bounds.max_x() - (rect.max_x() - bounds.origin.x),
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
            bounds.origin.y + (height - lead.height) / 2.0
        } else {
            bounds.origin.y
        };
        let mut rects = vec![mirror(Rect::new(Point::new(bounds.origin.x, lead_y), lead))];
        let start = Point::new(bounds.origin.x + lead.width + item.gap, bounds.origin.y);
        rects.extend(
            stack(start, content.gap, rest, |size| {
                Size::new(column, size.height)
            })
            .into_iter()
            .map(mirror),
        );
        rects
    }

    /// The height that [`layout`](Self::layout) uses for `sizes` at
    /// `width`.
    pub fn height(
        &self,
        styles: &Styles,
        tokens: &DesignTokens,
        width: f64,
        sizes: &[Size],
    ) -> f64 {
        let bounds = Rect::from_xywh(0.0, 0.0, width, f64::MAX);
        self.layout(styles, tokens, bounds, false, sizes)
            .iter()
            .map(|rect| rect.max_y())
            .fold(0.0, f64::max)
    }

    /// Stacks whole fields or field set parts down `bounds` with the
    /// group or set gap. Each item fills the width.
    pub fn stack(
        styles: &Styles,
        tokens: &DesignTokens,
        bounds: Rect,
        gap: FieldGap,
        heights: &[f64],
    ) -> Vec<Rect> {
        let slot = match gap {
            FieldGap::Group => Slot::Group,
            FieldGap::Set => Slot::Root,
        };
        let gap = styles
            .resolve(tokens, StyleQuery::new(ControlKind::Field, slot))
            .gap;
        let sizes: Vec<Size> = heights
            .iter()
            .map(|height| Size::new(bounds.size.width, *height))
            .collect();
        stack(bounds.origin, gap, &sizes, |size| size)
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
