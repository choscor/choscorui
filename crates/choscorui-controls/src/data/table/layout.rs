//! Data table geometry, hit testing, and painting.

use super::{Column, DataTable, Part, Stop};
use crate::custom::MeasureContext;
use crate::style::{
    ButtonVariant, Content, ControlKind, ControlSize, PaintContext, ResolvedStyle, Slot,
    StyleQuery, TextAlign, TextMeasure, VisualState,
};
use choscorui_core::{Icon, ItemKey, Locale, Point, Rect, Role, Size, SortOrder, Value};

/// Where the parts of the table are.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Geometry {
    pub filter: Rect,
    pub filter_text: Rect,
    pub columns: Rect,
    pub frame: Rect,
    pub header: Rect,
    /// The rows on the page.
    pub rows: Vec<Rect>,
    /// The start and the width of each visible column, in its order.
    pub cells: Vec<(Column, f64, f64)>,
    pub status: Rect,
    pub previous: Rect,
    pub next: Rect,
}

/// The metrics that the recipes give.
struct Metrics {
    toolbar: f64,
    header: f64,
    row: f64,
    check: f64,
    footer: f64,
    gap: f64,
}

fn query(slot: Slot) -> StyleQuery {
    StyleQuery::new(ControlKind::Table, slot)
}

fn button_query(variant: ButtonVariant, size: ControlSize, state: VisualState) -> StyleQuery {
    StyleQuery::new(ControlKind::Button, Slot::Root)
        .variant(variant)
        .size(size)
        .state(state)
}

fn button_width(style: &ResolvedStyle, content: Content<'_>, text: &dyn TextMeasure) -> f64 {
    content.size(style, text).width.max(style.min_width)
}

impl DataTable {
    fn metrics(&self, resolve: impl Fn(StyleQuery) -> ResolvedStyle, spacing: f64) -> Metrics {
        let input = resolve(StyleQuery::new(ControlKind::Input, Slot::Root));
        let header = resolve(query(Slot::HeaderCell));
        let cell = resolve(query(Slot::Cell));
        let check = resolve(StyleQuery::new(ControlKind::Checkbox, Slot::Root));
        let footer = resolve(button_query(
            ButtonVariant::Outline,
            ControlSize::Sm,
            VisualState::NONE,
        ));
        let toolbar = if self.filter.is_some() || self.columns_label.is_some() {
            input.min_height
        } else {
            0.0
        };
        Metrics {
            toolbar,
            header: header.min_height,
            row: cell.text.line_height + cell.padding.vertical(),
            check: check.min_width + cell.padding.horizontal(),
            footer: footer.min_height,
            // The space between the toolbar, the table, and the footer.
            gap: spacing * 4.0,
        }
    }

    /// The rows the frame shows: the page rows, or one for the empty text.
    fn shown_rows(&self) -> usize {
        self.page_keys().len().max(1)
    }

    pub(super) fn measure_size(&self, cx: &MeasureContext<'_>) -> Size {
        let m = self.metrics(|q| cx.style(q), cx.tokens.spacing);
        let frame = cx.style(query(Slot::Group));
        let max = cx.constraints.max.width;
        // An unbounded table takes the width of a medium container.
        let width = if max.is_finite() {
            max
        } else {
            cx.tokens.spacing * 160.0
        };
        let toolbar = if m.toolbar > 0.0 {
            m.toolbar + m.gap
        } else {
            0.0
        };
        let table = m.header + m.row * self.shown_rows() as f64 + frame.border_width * 2.0;
        Size::new(width, toolbar + table + m.gap + m.footer)
    }

    pub(super) fn layout(&self, cx: &MeasureContext<'_>, bounds: Rect) -> Geometry {
        let m = self.metrics(|q| cx.style(q), cx.tokens.spacing);
        let frame_style = cx.style(query(Slot::Group));
        let rtl = cx.rtl();
        let width = bounds.size.width;
        // Places a part of `size` at the start or the end of a row at `y`.
        let at = |y: f64, size: Size, end: bool| {
            let x = if end != rtl {
                bounds.max_x() - size.width
            } else {
                bounds.origin.x
            };
            Rect::new(Point::new(x, y), size)
        };
        let mut geometry = Geometry::default();
        let mut y = bounds.origin.y;
        if m.toolbar > 0.0 {
            if let Some(label) = &self.columns_label {
                let style = cx.style(button_query(
                    ButtonVariant::Outline,
                    ControlSize::Default,
                    VisualState::NONE,
                ));
                let content = Content {
                    leading: None,
                    label,
                    trailing: Some(Icon::ChevronDown),
                };
                let size = Size::new(button_width(&style, content, cx.text), m.toolbar);
                geometry.columns = at(y, size, true);
            }
            if self.filter.is_some() {
                let input = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root));
                let room = width - geometry.columns.size.width - m.gap;
                let size = Size::new(room.min(cx.tokens.spacing * 96.0).max(0.0), m.toolbar);
                geometry.filter = at(y, size, false);
                geometry.filter_text = Rect::from_xywh(
                    geometry.filter.origin.x + input.padding.inline_start,
                    geometry.filter.origin.y,
                    (size.width - input.padding.horizontal()).max(1.0),
                    m.toolbar,
                );
            }
            y += m.toolbar + m.gap;
        }
        let border = frame_style.border_width;
        let rows = self.shown_rows();
        let frame_height = m.header + m.row * rows as f64 + border * 2.0;
        geometry.frame = Rect::from_xywh(bounds.origin.x, y, width, frame_height);
        let inner = geometry.frame.inset(border);
        geometry.header =
            Rect::from_xywh(inner.origin.x, inner.origin.y, inner.size.width, m.header);
        geometry.rows = (0..self.page_keys().len())
            .map(|row| {
                Rect::from_xywh(
                    inner.origin.x,
                    geometry.header.max_y() + m.row * row as f64,
                    inner.size.width,
                    m.row,
                )
            })
            .collect();
        let columns = self.columns();
        let data = columns.len().saturating_sub(1).max(1) as f64;
        let share = ((inner.size.width - m.check) / data).max(0.0);
        let mut x = 0.0;
        geometry.cells = columns
            .into_iter()
            .map(|column| {
                let size = if column == Column::Check {
                    m.check
                } else {
                    share
                };
                let start = if rtl {
                    inner.max_x() - x - size
                } else {
                    inner.origin.x + x
                };
                x += size;
                (column, start, size)
            })
            .collect();
        y = geometry.frame.max_y() + m.gap;
        let button = |label: &str, cx: &MeasureContext<'_>| {
            let style = cx.style(button_query(
                ButtonVariant::Outline,
                ControlSize::Sm,
                VisualState::NONE,
            ));
            button_width(&style, Content::label(label), cx.text)
        };
        let next = button(&self.labels[1], cx);
        let previous = button(&self.labels[0], cx);
        geometry.next = at(y, Size::new(next, m.footer), true);
        let gap = cx.tokens.spacing * 2.0;
        let previous_x = if rtl {
            geometry.next.max_x() + gap
        } else {
            geometry.next.origin.x - gap - previous
        };
        geometry.previous = Rect::from_xywh(previous_x, y, previous, m.footer);
        let status_width = (width - next - previous - gap * 2.0).max(0.0);
        geometry.status = at(y, Size::new(status_width, m.footer), false);
        geometry
    }

    /// A numeric column (its first row sorts by a number) aligns to the
    /// end. Other columns align to the start.
    fn align(&self, column: usize) -> TextAlign {
        let numeric = self.order.first().is_some_and(|key| {
            matches!(
                self.model.data(*key, column, Role::Sort),
                Value::Int(_) | Value::Float(_)
            )
        });
        if numeric {
            TextAlign::End
        } else {
            TextAlign::Start
        }
    }

    /// The display text of a cell. A number shows in the locale format:
    /// an `Int` with digit groups, and a `Float` with two fraction digits.
    fn cell_text(&self, key: ItemKey, column: usize, locale: &Locale) -> String {
        match self.model.data(key, column, Role::Display) {
            Value::Int(value) => locale.format_integer(value),
            Value::Float(value) => locale
                .format_decimal(value, 2)
                .unwrap_or_else(|_| value.to_string()),
            value => value.to_display_string(),
        }
    }

    /// The rectangle of the visible column `index` in the row `row`.
    fn cell_rect(&self, index: usize, row: Rect) -> Rect {
        let (_, x, width) = self.geometry.cells[index];
        Rect::from_xywh(x, row.origin.y, width, row.size.height)
    }

    pub(super) fn part_at(&self, point: Point) -> Option<Part> {
        let g = &self.geometry;
        if self.columns_label.is_some() && g.columns.contains(point) {
            return Some(Part::Columns);
        }
        if g.previous.contains(point) {
            return (self.page > 0).then_some(Part::Previous);
        }
        if g.next.contains(point) {
            return (self.page < self.last_page()).then_some(Part::Next);
        }
        let column = g
            .cells
            .iter()
            .find(|(_, x, width)| point.x >= *x && point.x < x + width)
            .map(|(column, _, _)| *column);
        if g.header.contains(point) {
            return column.map(Part::Header);
        }
        let row = g.rows.iter().position(|rect| rect.contains(point))?;
        Some(if column == Some(Column::Check) {
            Part::Check(row)
        } else {
            Part::Row(row)
        })
    }

    fn paint_check(
        &self,
        cx: &mut PaintContext<'_>,
        rect: Rect,
        check: Option<bool>,
        state: VisualState,
    ) {
        let state = state
            .set(VisualState::CHECKED, check == Some(true))
            .set(VisualState::INDETERMINATE, check.is_none());
        let style = cx.style(StyleQuery::new(ControlKind::Checkbox, Slot::Root).state(state));
        let side = style.min_width;
        let box_rect = Rect::from_xywh(
            rect.center().x - side / 2.0,
            rect.center().y - side / 2.0,
            side,
            side,
        );
        let mark = match check {
            None => Some(Icon::Minus),
            Some(true) => Some(Icon::Check),
            Some(false) => None,
        };
        cx.slot(box_rect, &style, |cx, inner| {
            if let Some(mark) = mark {
                cx.icon(inner, mark, &style);
            }
        });
    }

    fn paint_toolbar(&self, cx: &mut PaintContext<'_>, host: VisualState) {
        let g = &self.geometry;
        if let Some((editor, _)) = &self.filter {
            let focused = host.focused() && self.focus == Stop::Filter;
            let state = editor.hosted_state(
                host.set(VisualState::FOCUSED, focused)
                    .set(VisualState::FOCUS_VISIBLE, focused && host.focus_visible()),
            );
            let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root).state(state));
            cx.slot(g.filter, &style, |_, _| {});
            editor.paint_hosted_text(cx, g.filter_text, state);
        }
        if let Some(label) = &self.columns_label {
            let state = self
                .button_state(host, Stop::Columns, Part::Columns, true)
                .set(VisualState::OPEN, self.menu.is_open());
            let style = cx.style(button_query(
                ButtonVariant::Outline,
                ControlSize::Default,
                state,
            ));
            let content = Content {
                leading: None,
                label,
                trailing: Some(Icon::ChevronDown),
            };
            cx.slot(g.columns, &style, |cx, inner| {
                content.draw(cx, inner, &style, TextAlign::Center);
            });
        }
    }

    fn button_state(
        &self,
        host: VisualState,
        stop: Stop,
        part: Part,
        enabled: bool,
    ) -> VisualState {
        VisualState::NONE
            .set(VisualState::DISABLED, host.disabled() || !enabled)
            .set(VisualState::HOVERED, self.hovered == Some(part))
            .set(VisualState::PRESSED, self.pressed == Some(part))
            .set(
                VisualState::FOCUS_VISIBLE,
                host.focus_visible() && self.focus == stop,
            )
    }

    fn paint_header(&self, cx: &mut PaintContext<'_>, host: VisualState) {
        let g = &self.geometry;
        let header = cx.style(query(Slot::Header));
        cx.slot(g.header, &header, |_, _| {});
        for (index, (column, _, _)) in g.cells.iter().enumerate() {
            let rect = self.cell_rect(index, g.header);
            let state = VisualState::NONE
                .set(
                    VisualState::HOVERED,
                    self.hovered == Some(Part::Header(*column)),
                )
                .set(VisualState::DISABLED, host.disabled());
            match column {
                Column::Check => self.paint_check(cx, rect, self.page_check(), state),
                Column::Data(column) => {
                    let style = cx.style(query(Slot::HeaderCell).state(state));
                    let label = self.header(*column);
                    let trailing = match self.proxy.sort() {
                        Some((sorted, SortOrder::Ascending)) if sorted == *column => {
                            Some(Icon::ArrowUp)
                        }
                        Some((sorted, SortOrder::Descending)) if sorted == *column => {
                            Some(Icon::ArrowDown)
                        }
                        // A sortable column that is not sorted shows a
                        // neutral icon.
                        _ => Some(Icon::ChevronsUpDown),
                    };
                    let content = Content {
                        leading: None,
                        label: &label,
                        trailing,
                    };
                    let align = self.align(*column);
                    cx.slot(rect, &style, |cx, inner| {
                        content.draw(cx, inner, &style, align);
                    });
                }
            }
        }
    }

    fn paint_rows(&self, cx: &mut PaintContext<'_>, host: VisualState) {
        let g = &self.geometry;
        let keys = self.page_keys();
        if keys.is_empty() {
            let style = cx.style(query(Slot::Cell));
            let rect = Rect::from_xywh(
                g.header.origin.x,
                g.header.max_y(),
                g.header.size.width,
                g.frame.max_y() - g.header.max_y(),
            );
            cx.label(rect, &self.empty, &style, TextAlign::Center);
            return;
        }
        let last = keys.len() - 1;
        for (index, (key, rect)) in keys.iter().zip(&g.rows).enumerate() {
            let selected = self.selection.is_selected(*key);
            let hovered =
                matches!(self.hovered, Some(Part::Row(row) | Part::Check(row)) if row == index);
            let state = VisualState::NONE
                .set(VisualState::SELECTED, selected)
                .set(VisualState::HOVERED, hovered)
                .set(VisualState::DISABLED, host.disabled());
            let mut row = cx.style(query(Slot::Row).state(state));
            if index == last {
                // The frame border closes the last row.
                row.border_width = 0.0;
            }
            cx.slot(*rect, &row, |_, _| {});
            for (column_index, (column, _, _)) in g.cells.iter().enumerate() {
                let cell = self.cell_rect(column_index, *rect);
                match column {
                    Column::Check => {
                        let check = VisualState::NONE.set(VisualState::DISABLED, host.disabled());
                        self.paint_check(cx, cell, Some(selected), check);
                    }
                    Column::Data(column) => {
                        let style = cx.style(query(Slot::Cell).state(state));
                        let text = self.cell_text(*key, *column, cx.strings.locale());
                        let align = self.align(*column);
                        cx.slot(cell, &style, |cx, inner| {
                            cx.label(inner, &text, &style, align);
                        });
                    }
                }
            }
        }
    }

    fn paint_focus(&self, cx: &mut PaintContext<'_>, host: VisualState) {
        if !(host.focus_visible() && self.focus == super::Stop::Grid) {
            return;
        }
        let g = &self.geometry;
        let row = match self.cell.row {
            None => g.header,
            Some(row) => match g.rows.get(row) {
                Some(rect) => *rect,
                None => return,
            },
        };
        if self.cell.column >= g.cells.len() {
            return;
        }
        let rect = self.cell_rect(self.cell.column, row);
        let style = cx.style(query(Slot::Root).state(VisualState::FOCUS_VISIBLE));
        cx.slot(rect, &style, |_, _| {});
    }

    fn paint_footer(&self, cx: &mut PaintContext<'_>, host: VisualState) {
        let g = &self.geometry;
        if let Some(status) = &self.status {
            let style = cx.style(query(Slot::Caption));
            let text = status(self.selection.len(), self.order.len());
            cx.label(g.status, &text, &style, TextAlign::Start);
        }
        for (stop, part, rect, label, enabled) in [
            (
                Stop::Previous,
                Part::Previous,
                g.previous,
                &self.labels[0],
                self.page > 0,
            ),
            (
                Stop::Next,
                Part::Next,
                g.next,
                &self.labels[1],
                self.page < self.last_page(),
            ),
        ] {
            let state = self.button_state(host, stop, part, enabled);
            let style = cx.style(button_query(ButtonVariant::Outline, ControlSize::Sm, state));
            cx.slot(rect, &style, |cx, inner| {
                cx.label(inner, label, &style, TextAlign::Center);
            });
        }
    }

    pub(super) fn paint_table(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        self.paint_toolbar(cx, host);
        let frame = cx.style(query(Slot::Group));
        cx.slot(self.geometry.frame, &frame, |_, _| {});
        self.paint_header(cx, host);
        self.paint_rows(cx, host);
        self.paint_focus(cx, host);
        self.paint_footer(cx, host);
    }
}
