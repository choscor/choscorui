//! Chart geometry, hit testing, and painting.

use super::{Chart, ChartKind, chart_color};
use crate::custom::MeasureContext;
use crate::style::{PaintContext, ResolvedStyle, Slot, TextAlign, fill_area, paint_in};
use choscorui_core::{Color, FillRule, LineJoin, Paint, Path, Point, Rect, Size, StrokeStyle};
use std::f64::consts::TAU;

/// The number of steps between grid lines on the value axis.
const TICKS: usize = 4;
/// The largest angle, in radians, between two points of an arc.
const ARC_STEP: f64 = TAU / 120.0;
/// How far an active pie slice grows, in points.
const SLICE_GROWTH: f64 = 8.0;

/// Where the parts of the chart are.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Plot {
    /// The area the marks fill.
    area: Rect,
    /// The category labels under a cartesian chart.
    axis: Rect,
    legend: Rect,
    /// The value at the top of the value axis.
    top: f64,
    center: Point,
    radius: f64,
    inner: f64,
    /// The tooltip size for each category.
    tooltips: Vec<Size>,
    viewport: Rect,
}

impl Plot {
    /// The square around the circle of a radial or radar chart.
    fn disc(&self) -> Rect {
        Rect::from_xywh(
            self.center.x - self.radius,
            self.center.y - self.radius,
            self.radius * 2.0,
            self.radius * 2.0,
        )
    }
}

/// The point at `radius` from `center` at `angle` radians clockwise from
/// the top.
fn polar(center: Point, radius: f64, angle: f64) -> Point {
    Point::new(
        center.x + radius * angle.sin(),
        center.y - radius * angle.cos(),
    )
}

/// The points of an arc from `start` to `end` radians.
fn arc(center: Point, radius: f64, start: f64, end: f64) -> Vec<Point> {
    let steps = ((end - start).abs() / ARC_STEP).ceil().max(1.0) as usize;
    (0..=steps)
        .map(|step| {
            polar(
                center,
                radius,
                start + (end - start) * step as f64 / steps as f64,
            )
        })
        .collect()
}

/// A ring segment between `inner` and `outer` from `start` to `end`. An
/// inner radius of zero makes a pie slice.
fn segment(center: Point, inner: f64, outer: f64, start: f64, end: f64) -> Path {
    let mut points = arc(center, outer, start, end);
    if inner > 0.0 {
        points.extend(arc(center, inner, end, start));
    } else {
        points.push(center);
    }
    Path::polygon(&points)
}

/// The smallest value of the form 1, 2, 2.5, or 5 times a power of ten
/// that is at least `value`.
fn nice(value: f64) -> f64 {
    if value <= 0.0 || !value.is_finite() {
        return 1.0;
    }
    let magnitude = 10f64.powf(value.log10().floor());
    [1.0, 2.0, 2.5, 5.0, 10.0]
        .into_iter()
        .map(|step| step * magnitude)
        .find(|candidate| *candidate >= value)
        .unwrap_or(10.0 * magnitude)
}

/// The angle of `point` around `center`, clockwise from the top, in
/// 0..TAU.
fn angle_of(center: Point, point: Point) -> f64 {
    let angle = (point.x - center.x).atan2(center.y - point.y);
    if angle < 0.0 { angle + TAU } else { angle }
}

impl Chart {
    fn cartesian(&self) -> bool {
        matches!(
            self.kind,
            ChartKind::Area | ChartKind::Bar | ChartKind::Line
        )
    }

    pub(super) fn axis_height(&self, cx: &MeasureContext<'_>) -> f64 {
        if !self.cartesian() {
            return 0.0;
        }
        let label = cx.style(Self::query(Slot::Label));
        label.text.line_height + label.gap
    }

    pub(super) fn legend_height(&self, cx: &MeasureContext<'_>) -> f64 {
        if !self.legend {
            return 0.0;
        }
        let legend = cx.style(Self::query(Slot::Legend));
        legend.text.line_height + legend.gap * 2.0
    }

    fn max_value(&self) -> f64 {
        let series = if self.per_category() {
            1
        } else {
            self.series.len()
        };
        (0..series)
            .flat_map(|series| (0..self.categories.len()).map(move |category| (series, category)))
            .map(|(series, category)| self.value(series, category))
            .fold(0.0, f64::max)
    }

    pub(super) fn layout(&self, cx: &MeasureContext<'_>, bounds: Rect) -> Plot {
        let axis = self.axis_height(cx);
        let legend = self.legend_height(cx);
        let area = Rect::from_xywh(
            bounds.origin.x,
            bounds.origin.y,
            bounds.size.width,
            (bounds.size.height - axis - legend).max(0.0),
        );
        let label = cx.style(Self::query(Slot::Label));
        let room = if self.kind == ChartKind::Radar {
            // Category labels sit around the radar.
            label.text.line_height + label.gap
        } else if self.kind == ChartKind::Pie {
            SLICE_GROWTH
        } else {
            0.0
        };
        let radius = (area.size.width.min(area.size.height) / 2.0 - room).max(0.0);
        let inner = match self.kind {
            ChartKind::Pie if self.donut => radius * 0.6,
            ChartKind::Radial => radius * 0.3,
            _ => 0.0,
        };
        let top = if self.kind == ChartKind::Pie {
            1.0
        } else {
            nice(self.max_value())
        };
        let tooltips = (0..self.categories.len())
            .map(|category| self.tooltip_size(cx, category))
            .collect();
        Plot {
            area,
            axis: Rect::from_xywh(bounds.origin.x, area.max_y(), bounds.size.width, axis),
            legend: Rect::from_xywh(
                bounds.origin.x,
                area.max_y() + axis,
                bounds.size.width,
                legend,
            ),
            top,
            center: area.center(),
            radius,
            inner,
            tooltips,
            viewport: cx.viewport,
        }
    }

    /// The width of one category band of a cartesian chart.
    fn band(&self) -> f64 {
        self.plot.area.size.width / self.categories.len().max(1) as f64
    }

    /// The center of category `index` along the category axis.
    fn x(&self, index: usize) -> f64 {
        let offset = self.band() * (index as f64 + 0.5);
        if self.rtl {
            self.plot.area.max_x() - offset
        } else {
            self.plot.area.origin.x + offset
        }
    }

    /// The position of `value` along the value axis.
    fn y(&self, value: f64) -> f64 {
        let area = self.plot.area;
        area.max_y() - (value / self.plot.top).clamp(0.0, 1.0) * area.size.height
    }

    fn total(&self) -> f64 {
        (0..self.categories.len())
            .map(|category| self.value(0, category).max(0.0))
            .sum()
    }

    /// The start and the end angle of each pie slice.
    fn slices(&self) -> Vec<(f64, f64)> {
        let total = self.total();
        let mut start = 0.0;
        (0..self.categories.len())
            .map(|category| {
                let sweep = if total > 0.0 {
                    self.value(0, category).max(0.0) / total * TAU
                } else {
                    0.0
                };
                let slice = (start, start + sweep);
                start += sweep;
                slice
            })
            .collect()
    }

    /// The inner and the outer radius of the radial ring `index`.
    fn ring(&self, index: usize) -> (f64, f64) {
        let count = self.categories.len().max(1) as f64;
        let width = (self.plot.radius - self.plot.inner) / count;
        let inner = self.plot.inner + width * index as f64;
        // A quarter of each ring is the gap to the next one.
        (inner, inner + width * 0.75)
    }

    /// The angle of radar axis `index`.
    fn spoke(&self, index: usize) -> f64 {
        TAU * index as f64 / self.categories.len().max(1) as f64
    }

    pub(super) fn category_at(&self, point: Point) -> Option<usize> {
        let count = self.categories.len();
        if count == 0 {
            return None;
        }
        let plot = &self.plot;
        let distance = {
            let dx = point.x - plot.center.x;
            let dy = point.y - plot.center.y;
            (dx * dx + dy * dy).sqrt()
        };
        match self.kind {
            ChartKind::Area | ChartKind::Bar | ChartKind::Line => {
                if !plot.area.contains(point) {
                    return None;
                }
                let offset = if self.rtl {
                    plot.area.max_x() - point.x
                } else {
                    point.x - plot.area.origin.x
                };
                Some(((offset / self.band()).floor() as usize).min(count - 1))
            }
            ChartKind::Pie => {
                if distance < plot.inner || distance > plot.radius + SLICE_GROWTH {
                    return None;
                }
                let angle = angle_of(plot.center, point);
                self.slices()
                    .iter()
                    .position(|(start, end)| angle >= *start && angle < *end)
            }
            ChartKind::Radar => {
                if distance > plot.radius {
                    return None;
                }
                let step = TAU / count as f64;
                let angle = angle_of(plot.center, point);
                Some(((angle / step).round() as usize) % count)
            }
            ChartKind::Radial => (0..count).find(|index| {
                let (inner, outer) = self.ring(*index);
                distance >= inner && distance <= outer
            }),
        }
    }

    fn tooltip_size(&self, cx: &MeasureContext<'_>, category: usize) -> Size {
        let content = cx.style(Self::query(Slot::Content));
        let label = cx.style(Self::query(Slot::Label));
        let value = cx.style(Self::query(Slot::Value));
        let indicator = cx.style(Self::query(Slot::Indicator));
        let rows = self.rows(category);
        let line = content.text.line_height;
        let row_width = rows
            .iter()
            .map(|(_, name, amount)| {
                indicator.min_width
                    + content.gap
                    + cx.text.width(name, &label.text)
                    + content.gap * 2.0
                    + cx.text.width(amount, &value.text)
            })
            .fold(0.0, f64::max);
        let (title_width, title_lines) = if self.per_category() {
            (0.0, 0)
        } else {
            let title = self.categories.get(category).map_or("", String::as_str);
            (cx.text.width(title, &value.text), 1)
        };
        let lines = rows.len() + title_lines;
        Size::new(
            row_width.max(title_width) + content.padding.horizontal(),
            line * lines as f64
                + content.gap * lines.saturating_sub(1) as f64
                + content.padding.vertical(),
        )
    }

    /// The point the tooltip of `category` sits beside.
    fn anchor(&self, category: usize) -> Point {
        let plot = &self.plot;
        match self.kind {
            ChartKind::Area | ChartKind::Bar | ChartKind::Line => {
                Point::new(self.x(category), plot.area.origin.y)
            }
            ChartKind::Pie => {
                let (start, end) = self.slices()[category];
                polar(plot.center, plot.radius, (start + end) / 2.0)
            }
            ChartKind::Radar => polar(plot.center, plot.radius, self.spoke(category)),
            ChartKind::Radial => {
                let (_, outer) = self.ring(category);
                polar(plot.center, outer, 0.0)
            }
        }
    }

    pub(super) fn tooltip_rect(&self, category: usize) -> Rect {
        let size = self
            .plot
            .tooltips
            .get(category)
            .copied()
            .unwrap_or_default();
        let anchor = self.anchor(category);
        // The tooltip sits 10 pt past the anchor in reading order, and on
        // the other side when it would leave the window.
        let gap = 10.0;
        let viewport = self.plot.viewport;
        let after = if self.rtl {
            anchor.x - gap - size.width
        } else {
            anchor.x + gap
        };
        let before = if self.rtl {
            anchor.x + gap
        } else {
            anchor.x - gap - size.width
        };
        let fits = |x: f64| x >= viewport.origin.x && x + size.width <= viewport.max_x();
        let x = if fits(after) || !fits(before) {
            after
        } else {
            before
        };
        let y = anchor
            .y
            .min(viewport.max_y() - size.height)
            .max(viewport.origin.y);
        Rect::new(Point::new(x, y), size)
    }

    fn grid(&self, cx: &mut PaintContext<'_>, track: &ResolvedStyle) {
        let area = self.plot.area;
        let thickness = track.min_height;
        for tick in 0..=TICKS {
            let y = area.max_y() - area.size.height * tick as f64 / TICKS as f64;
            let line = Rect::from_xywh(
                area.origin.x,
                y - thickness / 2.0,
                area.size.width,
                thickness,
            );
            fill_area(cx.list, line, cx.rtl, &track.background);
        }
    }

    fn paint_cartesian(&self, cx: &mut PaintContext<'_>) {
        let track = cx.style(Self::query(Slot::Track));
        let mark = cx.style(Self::query(Slot::Item));
        let label = cx.style(Self::query(Slot::Label));
        let area = self.plot.area;
        let band = self.band();
        if self.kind == ChartKind::Bar
            && let Some(active) = self.active
        {
            // The cursor band behind the active bars.
            let cursor = Rect::from_xywh(
                self.x(active) - band / 2.0,
                area.origin.y,
                band,
                area.size.height,
            );
            let muted = cx.tokens.color(choscorui_core::ColorRole::Muted);
            cx.list.fill_rect(cursor, muted);
        }
        self.grid(cx, &track);
        let count = self.categories.len();
        let series = self.series.len();
        for (index, _) in self.series.iter().enumerate() {
            let color = chart_color(cx.tokens, index);
            let points: Vec<Point> = (0..count)
                .map(|category| Point::new(self.x(category), self.y(self.value(index, category))))
                .collect();
            match self.kind {
                ChartKind::Bar => {
                    // Bars fill 80% of the band, 4 pt apart.
                    let gap = 4.0;
                    let group = band * 0.8;
                    let width = ((group - gap * (series - 1) as f64) / series as f64).max(1.0);
                    let radius = mark.radius.top_start;
                    for point in &points {
                        let offset = -group / 2.0 + (width + gap) * index as f64;
                        let x = if self.rtl {
                            point.x - offset - width
                        } else {
                            point.x + offset
                        };
                        let rect = Rect::from_xywh(x, point.y, width, area.max_y() - point.y);
                        if rect.size.height <= 0.0 {
                            continue;
                        }
                        let path = Path::rounded_rect_corners(rect, [radius, radius, 0.0, 0.0]);
                        let _ = cx
                            .list
                            .fill_path(path, Paint::Solid(color), FillRule::NonZero);
                    }
                }
                ChartKind::Area | ChartKind::Line => {
                    if points.is_empty() {
                        continue;
                    }
                    if self.kind == ChartKind::Area {
                        let mut outline = points.clone();
                        outline.push(Point::new(points[points.len() - 1].x, area.max_y()));
                        outline.push(Point::new(points[0].x, area.max_y()));
                        let fill = color.with_alpha(mark.opacity);
                        let _ = cx.list.fill_path(
                            Path::polygon(&outline),
                            Paint::Solid(fill),
                            FillRule::NonZero,
                        );
                    }
                    let mut line = Path::new();
                    line.move_to(points[0]);
                    for point in &points[1..] {
                        line.line_to(*point);
                    }
                    let stroke = StrokeStyle {
                        join: LineJoin::Round,
                        ..StrokeStyle::new(mark.border_width)
                    };
                    let _ = cx.list.stroke_path(line, Paint::Solid(color), stroke);
                }
                _ => {}
            }
        }
        if self.kind != ChartKind::Bar
            && let Some(active) = self.active
        {
            // A cursor line and a dot on each series at the category.
            let x = self.x(active);
            let thickness = track.min_height;
            let cursor = Rect::from_xywh(
                x - thickness / 2.0,
                area.origin.y,
                thickness,
                area.size.height,
            );
            fill_area(cx.list, cursor, cx.rtl, &track.background);
            for index in 0..series {
                let center = Point::new(x, self.y(self.value(index, active)));
                self.dot(
                    cx,
                    center,
                    mark.border_width * 2.0,
                    chart_color(cx.tokens, index),
                );
            }
        }
        for (category, text) in self.categories.iter().enumerate() {
            let rect = Rect::from_xywh(
                self.x(category) - band / 2.0,
                self.plot.axis.origin.y + label.gap,
                band,
                label.text.line_height,
            );
            cx.label(rect, text, &label, TextAlign::Center);
        }
    }

    fn dot(&self, cx: &mut PaintContext<'_>, center: Point, radius: f64, color: Color) {
        let rect = Rect::from_xywh(
            center.x - radius,
            center.y - radius,
            radius * 2.0,
            radius * 2.0,
        );
        let _ = cx
            .list
            .fill_path(Path::ellipse(rect), Paint::Solid(color), FillRule::NonZero);
    }

    fn paint_pie(&self, cx: &mut PaintContext<'_>) {
        let plot = &self.plot;
        let background = cx.tokens.color(choscorui_core::ColorRole::Background);
        for (category, (start, end)) in self.slices().into_iter().enumerate() {
            if end <= start {
                continue;
            }
            let outer = if self.active == Some(category) {
                plot.radius + SLICE_GROWTH
            } else {
                plot.radius
            };
            let path = segment(plot.center, plot.inner, outer, start, end);
            let color = chart_color(cx.tokens, category);
            let _ = cx
                .list
                .fill_path(path.clone(), Paint::Solid(color), FillRule::NonZero);
            // A line in the background color between slices.
            let _ = cx
                .list
                .stroke_path(path, Paint::Solid(background), StrokeStyle::new(1.0));
        }
    }

    fn paint_radar(&self, cx: &mut PaintContext<'_>) {
        let track = cx.style(Self::query(Slot::Track));
        let mark = cx.style(Self::query(Slot::Item));
        let label = cx.style(Self::query(Slot::Label));
        let plot = &self.plot;
        let count = self.categories.len();
        if count < 3 {
            return;
        }
        let grid = paint_in(&track.background, plot.disc(), cx.rtl);
        for level in 1..=TICKS {
            let radius = plot.radius * level as f64 / TICKS as f64;
            let points: Vec<Point> = (0..count)
                .map(|index| polar(plot.center, radius, self.spoke(index)))
                .collect();
            let _ = cx.list.stroke_path(
                Path::polygon(&points),
                grid.clone(),
                StrokeStyle::new(track.min_height),
            );
        }
        for index in 0..count {
            let end = polar(plot.center, plot.radius, self.spoke(index));
            let _ = cx.list.stroke_path(
                Path::line(plot.center, end),
                grid.clone(),
                StrokeStyle::new(track.min_height),
            );
        }
        for series in 0..self.series.len() {
            let color = chart_color(cx.tokens, series);
            let points: Vec<Point> = (0..count)
                .map(|index| {
                    let radius =
                        plot.radius * (self.value(series, index) / plot.top).clamp(0.0, 1.0);
                    polar(plot.center, radius, self.spoke(index))
                })
                .collect();
            let shape = Path::polygon(&points);
            let _ = cx.list.fill_path(
                shape.clone(),
                Paint::Solid(color.with_alpha(mark.opacity)),
                FillRule::NonZero,
            );
            let stroke = StrokeStyle {
                join: LineJoin::Round,
                ..StrokeStyle::new(mark.border_width)
            };
            let _ = cx.list.stroke_path(shape, Paint::Solid(color), stroke);
            if let Some(active) = self.active {
                self.dot(cx, points[active], mark.border_width * 2.0, color);
            }
        }
        for (index, text) in self.categories.iter().enumerate() {
            let width = cx.measure(text, &label);
            let at = polar(
                plot.center,
                plot.radius + label.gap + label.text.line_height / 2.0,
                self.spoke(index),
            );
            let rect = Rect::from_xywh(
                at.x - width / 2.0,
                at.y - label.text.line_height / 2.0,
                width,
                label.text.line_height,
            );
            cx.label(rect, text, &label, TextAlign::Center);
        }
    }

    fn paint_radial(&self, cx: &mut PaintContext<'_>) {
        let track = cx.style(Self::query(Slot::Track));
        let plot = &self.plot;
        for category in 0..self.categories.len() {
            let (inner, outer) = self.ring(category);
            let ring = segment(plot.center, inner, outer, 0.0, TAU);
            let paint = paint_in(&track.background, plot.disc(), cx.rtl);
            let _ = cx.list.fill_path(ring, paint, FillRule::NonZero);
            let sweep = (self.value(0, category) / plot.top).clamp(0.0, 1.0) * TAU;
            if sweep <= 0.0 {
                continue;
            }
            let color = chart_color(cx.tokens, category);
            let arc = segment(plot.center, inner, outer, 0.0, sweep);
            let _ = cx
                .list
                .fill_path(arc, Paint::Solid(color), FillRule::NonZero);
        }
    }

    fn paint_legend(&self, cx: &mut PaintContext<'_>) {
        if !self.legend {
            return;
        }
        let legend = cx.style(Self::query(Slot::Legend));
        let indicator = cx.style(Self::query(Slot::Indicator));
        let entries = self.legend_entries();
        let widths: Vec<f64> = entries
            .iter()
            .map(|(_, label)| indicator.min_width + legend.gap + cx.measure(label, &legend))
            .collect();
        // Entries sit two gaps apart, centered.
        let spacing = legend.gap * 2.0;
        let total = widths.iter().sum::<f64>() + spacing * widths.len().saturating_sub(1) as f64;
        let rect = self.plot.legend;
        let y = rect.origin.y + legend.gap * 2.0;
        let mut x = if self.rtl {
            rect.center().x + total / 2.0
        } else {
            rect.center().x - total / 2.0
        };
        for ((color, label), width) in entries.into_iter().zip(widths) {
            let start = if self.rtl { x - width } else { x };
            let line = legend.text.line_height;
            let swatch_x = if self.rtl {
                start + width - indicator.min_width
            } else {
                start
            };
            let swatch = Rect::from_xywh(
                swatch_x,
                y + (line - indicator.min_height) / 2.0,
                indicator.min_width,
                indicator.min_height,
            );
            self.swatch(cx, swatch, &indicator, chart_color(cx.tokens, color));
            let text_x = if self.rtl {
                start
            } else {
                start + indicator.min_width + legend.gap
            };
            let text = Rect::from_xywh(text_x, y, width - indicator.min_width - legend.gap, line);
            cx.label(text, label, &legend, TextAlign::Start);
            x += if self.rtl {
                -(width + spacing)
            } else {
                width + spacing
            };
        }
    }

    fn swatch(&self, cx: &mut PaintContext<'_>, rect: Rect, style: &ResolvedStyle, color: Color) {
        let mut style = style.clone();
        style.background = color.into();
        cx.slot(rect, &style, |_, _| {});
    }

    pub(super) fn paint_chart(&self, cx: &mut PaintContext<'_>) {
        match self.kind {
            ChartKind::Area | ChartKind::Bar | ChartKind::Line => self.paint_cartesian(cx),
            ChartKind::Pie => self.paint_pie(cx),
            ChartKind::Radar => self.paint_radar(cx),
            ChartKind::Radial => self.paint_radial(cx),
        }
        self.paint_legend(cx);
    }

    pub(super) fn paint_tooltip(&self, cx: &mut PaintContext<'_>, category: usize) {
        let content = cx.style(Self::query(Slot::Content));
        let label = cx.style(Self::query(Slot::Label));
        let value = cx.style(Self::query(Slot::Value));
        let indicator = cx.style(Self::query(Slot::Indicator));
        let rect = self.tooltip_rect(category);
        let rows = self.rows(category);
        let title = (!self.per_category())
            .then(|| self.categories.get(category).cloned())
            .flatten();
        cx.slot(rect, &content, |cx, inner| {
            let line = content.text.line_height;
            let mut y = inner.origin.y;
            if let Some(title) = &title {
                let row = Rect::from_xywh(inner.origin.x, y, inner.size.width, line);
                cx.label(row, title, &value, TextAlign::Start);
                y += line + content.gap;
            }
            for (color, name, amount) in &rows {
                let row = Rect::from_xywh(inner.origin.x, y, inner.size.width, line);
                let swatch_x = if cx.rtl {
                    row.max_x() - indicator.min_width
                } else {
                    row.origin.x
                };
                let swatch = Rect::from_xywh(
                    swatch_x,
                    y + (line - indicator.min_height) / 2.0,
                    indicator.min_width,
                    indicator.min_height,
                );
                self.swatch(cx, swatch, &indicator, chart_color(cx.tokens, *color));
                let offset = indicator.min_width + content.gap;
                let name_rect = Rect::from_xywh(
                    if cx.rtl {
                        row.origin.x
                    } else {
                        row.origin.x + offset
                    },
                    y,
                    row.size.width - offset,
                    line,
                );
                cx.label(name_rect, name, &label, TextAlign::Start);
                cx.label(name_rect, amount, &value, TextAlign::End);
                y += line + content.gap;
            }
        });
    }
}
