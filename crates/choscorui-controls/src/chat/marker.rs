//! Chat markers: a short muted line, such as a date or a status, between
//! messages.

use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::hosted;
use crate::style::fill_area;
use crate::style::{ControlKind, MarkerVariant, PaintContext, Slot, StyleQuery, TextAlign};
use choscorui_core::{EventReply, Icon, Measured, Rect, SemanticNode, SemanticRole, Size};

/// A marker between messages. The default and `Border` markers show an
/// optional icon and the label at the start; `Border` adds a line below.
/// The `Separator` marker centers the label between two lines. It takes
/// no focus.
pub struct Marker {
    label: String,
    icon: Option<Icon>,
    variant: MarkerVariant,
}

impl Marker {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            variant: MarkerVariant::Default,
        }
    }

    #[must_use]
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    #[must_use]
    pub fn with_variant(mut self, variant: MarkerVariant) -> Self {
        self.variant = variant;
        self
    }

    fn query(&self, slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Marker, slot).variant(self.variant)
    }
}

impl CustomControl for Marker {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(self.query(Slot::Root));
        let max = cx.constraints.max.width;
        // An unbounded marker takes the width of a medium container.
        let width = if max.is_finite() {
            max
        } else {
            cx.tokens.spacing * 120.0
        };
        let height = root.min_height.max(root.text.line_height) + root.padding.vertical();
        Measured {
            size: Size::new(width, height),
            baseline: None,
        }
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let root = cx.style(self.query(Slot::Root));
        let icon_style = cx.style(self.query(Slot::Icon));
        let bounds = cx.bounds;
        cx.slot(bounds, &root, |cx, inner| {
            let label = cx.measure(&self.label, &root).ceil();
            let icon = if self.icon.is_some() {
                icon_style.icon_size + root.gap
            } else {
                0.0
            };
            let content = icon + label;
            let start = if self.variant == MarkerVariant::Separator {
                inner.center().x - content / 2.0
            } else if cx.rtl {
                inner.max_x() - content
            } else {
                inner.origin.x
            };
            // The icon leads in reading order.
            let (icon_x, label_x) = if cx.rtl {
                (start + label + root.gap, start)
            } else {
                (start, start + icon)
            };
            if let Some(mark) = self.icon {
                let side = icon_style.icon_size;
                let rect = Rect::from_xywh(icon_x, inner.center().y - side / 2.0, side, side);
                cx.icon(rect, mark, &icon_style);
            }
            let text = Rect::from_xywh(label_x, inner.origin.y, label, inner.size.height);
            cx.label(text, &self.label, &root, TextAlign::Start);
            if self.variant == MarkerVariant::Separator {
                let line = cx.style(self.query(Slot::Separator));
                let y = inner.center().y - line.min_height / 2.0;
                // Without a label or an icon, one unbroken line shows.
                let gap = if content > 0.0 {
                    line.padding.inline_start + root.gap
                } else {
                    0.0
                };
                let left = (start - gap - inner.origin.x).max(0.0);
                let right_x = start + content + gap;
                let right = (inner.max_x() - right_x).max(0.0);
                let left = Rect::from_xywh(inner.origin.x, y, left, line.min_height);
                fill_area(cx.list, left, cx.rtl, &line.background);
                let right = Rect::from_xywh(right_x, y, right, line.min_height);
                fill_area(cx.list, right, cx.rtl, &line.background);
            }
        });
    }

    fn semantics(&self) -> SemanticNode {
        hosted::node(SemanticRole::StaticText, &self.label, Vec::new())
    }

    fn focusable(&self) -> bool {
        false
    }
}
