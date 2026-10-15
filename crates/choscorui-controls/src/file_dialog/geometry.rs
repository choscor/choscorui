//! Where each part of the dialog sits, from the recipes of the controls
//! that the parts look like.

use super::{Filter, Labels};
use crate::custom::MeasureContext;
use crate::style::{
    ButtonVariant, Content, ControlKind, ControlSize, Slot, StyleQuery, choice_size, content_rect,
};
use choscorui_core::{Point, Rect, Size};

/// The size that the dialog asks for.
pub(super) const NATURAL: Size = Size::new(560.0, 420.0);

/// The rectangles of the parts, in window coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Geometry {
    pub bounds: Rect,
    pub title: Rect,
    pub parent: Rect,
    pub location: Rect,
    pub location_text: Rect,
    /// The frame of the list.
    pub list: Rect,
    /// The rows of the list, inside the frame.
    pub rows: Rect,
    pub row_height: f64,
    pub name_label: Rect,
    pub name: Rect,
    pub name_text: Rect,
    pub filter: Rect,
    pub hidden: Rect,
    pub cancel: Rect,
    pub accept: Rect,
}

pub(super) fn button_query(variant: ButtonVariant) -> StyleQuery {
    StyleQuery::new(ControlKind::Button, Slot::Root).variant(variant)
}

pub(super) fn parent_query() -> StyleQuery {
    StyleQuery::new(ControlKind::Button, Slot::Root)
        .variant(ButtonVariant::Ghost)
        .size(ControlSize::IconSm)
}

impl Geometry {
    /// Places the parts in `bounds`. The layout is left to right, and a
    /// right-to-left layout mirrors it.
    pub fn new(
        cx: &MeasureContext<'_>,
        bounds: Rect,
        labels: &Labels,
        filters: &[Filter],
        save: bool,
    ) -> Self {
        let pad = cx.tokens.space(4.0);
        let gap = cx.tokens.space(3.0);
        let style = |query| cx.style(query);
        let title = style(StyleQuery::new(ControlKind::Dialog, Slot::Title));
        let input = style(StyleQuery::new(ControlKind::Input, Slot::Root));
        let parent = style(parent_query());
        let item = style(StyleQuery::new(ControlKind::ListView, Slot::Item));
        let select = style(StyleQuery::new(ControlKind::NativeSelect, Slot::Root));
        let label = style(StyleQuery::new(ControlKind::Label, Slot::Root));
        let (left, right) = (bounds.min_x() + pad, bounds.max_x() - pad);
        let width = (right - left).max(0.0);
        let mut g = Geometry {
            bounds,
            row_height: item.min_height.max(item.text.line_height),
            ..Geometry::default()
        };
        let mut y = bounds.min_y() + pad;
        g.title = Rect::from_xywh(left, y, width, title.text.line_height);
        y += title.text.line_height + gap;
        // The parent button, then the location field.
        let row = input.min_height;
        g.parent = Rect::from_xywh(
            left,
            y + (row - parent.min_height) / 2.0,
            parent.min_width,
            parent.min_height,
        );
        let field_x = g.parent.max_x() + cx.tokens.space(2.0);
        g.location = Rect::from_xywh(field_x, y, (right - field_x).max(0.0), row);
        y += row + gap;
        // The rows at the bottom, from the last one up.
        let mut bottom = bounds.max_y() - pad;
        let buttons = cx.style(button_query(ButtonVariant::Default));
        let cancel = Content::label(&labels.cancel).size(&buttons, cx.text);
        let accept = Content::label(&labels.accept).size(&buttons, cx.text);
        let top = bottom - buttons.min_height;
        g.accept = Rect::from_xywh(right - accept.width, top, accept.width, buttons.min_height);
        let cancel_x = g.accept.min_x() - cx.tokens.space(2.0) - cancel.width;
        g.cancel = Rect::from_xywh(cancel_x, top, cancel.width, buttons.min_height);
        bottom = top - gap;
        let top = bottom - select.min_height;
        let longest = filters
            .iter()
            .map(|filter| cx.text.width(&filter.label, &select.text))
            .fold(0.0, f64::max);
        let filter_width = (longest + select.padding.horizontal()).min(width / 2.0);
        g.filter = Rect::from_xywh(left, top, filter_width, select.min_height);
        let hidden = choice_size(
            ControlKind::Checkbox,
            &labels.hidden,
            cx.styles,
            cx.tokens,
            cx.text,
        );
        let hidden_x = g.filter.max_x() + gap;
        g.hidden = Rect::from_xywh(
            hidden_x,
            top,
            hidden.width.min((right - hidden_x).max(0.0)),
            select.min_height,
        );
        bottom = top - gap;
        if save {
            let top = bottom - input.min_height;
            let label_width = cx.text.width(&labels.name, &label.text);
            g.name_label = Rect::from_xywh(left, top, label_width, input.min_height);
            let name_x = g.name_label.max_x() + gap;
            g.name = Rect::from_xywh(name_x, top, (right - name_x).max(0.0), input.min_height);
            bottom = top - gap;
        }
        g.list = Rect::from_xywh(left, y, width, (bottom - y).max(0.0));
        g.rows = g.list.inset(cx.tokens.space(1.0));
        if cx.rtl() {
            g.mirror();
        }
        g.location_text = content_rect(g.location, &input, cx.rtl());
        g.name_text = content_rect(g.name, &input, cx.rtl());
        g
    }

    fn mirror(&mut self) {
        let bounds = self.bounds;
        for rect in [
            &mut self.title,
            &mut self.parent,
            &mut self.location,
            &mut self.list,
            &mut self.rows,
            &mut self.name_label,
            &mut self.name,
            &mut self.filter,
            &mut self.hidden,
            &mut self.cancel,
            &mut self.accept,
        ] {
            *rect = rect.mirrored_in(bounds);
        }
    }

    /// The rectangle of row `index` with the list scrolled by `scroll`.
    pub fn row(&self, index: usize, scroll: f64) -> Rect {
        Rect::from_xywh(
            self.rows.origin.x,
            self.rows.origin.y + index as f64 * self.row_height - scroll,
            self.rows.size.width,
            self.row_height,
        )
    }

    /// The rows in view among `count` rows, found without a search.
    pub fn visible(&self, count: usize, scroll: f64) -> std::ops::Range<usize> {
        if self.row_height <= 0.0 {
            return 0..0;
        }
        let first = (scroll / self.row_height).floor().max(0.0) as usize;
        let last = ((scroll + self.rows.size.height) / self.row_height).ceil() as usize;
        first.min(count)..last.min(count)
    }

    /// The row under `point`, among `count` rows.
    pub fn row_at(&self, point: Point, count: usize, scroll: f64) -> Option<usize> {
        if !self.rows.contains(point) || self.row_height <= 0.0 {
            return None;
        }
        let index = ((point.y - self.rows.origin.y + scroll) / self.row_height).floor() as usize;
        (index < count).then_some(index)
    }

    /// The number of rows that fit in the list, at least one.
    pub fn page(&self) -> usize {
        if self.row_height <= 0.0 {
            return 1;
        }
        ((self.rows.size.height / self.row_height).floor() as usize).max(1)
    }

    /// The largest scroll offset for `count` rows.
    pub fn max_scroll(&self, count: usize) -> f64 {
        (count as f64 * self.row_height - self.rows.size.height).max(0.0)
    }
}
