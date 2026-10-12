//! A chart of app data: area, bar, line, pie, radar, and radial charts,
//! with a tooltip and a legend.

mod plot;

use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::hosted;
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery};
use choscorui_core::{
    ColorRole, DesignTokens, EventPhase, EventReply, Key, KeyEvent, Measured, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, Size,
};
use plot::Plot;

/// The shape of a chart.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ChartKind {
    /// Filled areas under lines, one per series.
    Area,
    /// Grouped bars, one per series in each category.
    #[default]
    Bar,
    /// Lines, one per series.
    Line,
    /// Slices of the first series, one per category.
    Pie,
    /// A polygon per series over one axis per category.
    Radar,
    /// Concentric arcs of the first series, one per category.
    Radial,
}

/// One series of values, one value per category.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartSeries {
    pub label: String,
    pub values: Vec<f64>,
}

impl ChartSeries {
    pub fn new(label: impl Into<String>, values: Vec<f64>) -> Self {
        Self {
            label: label.into(),
            values,
        }
    }
}

/// The chart color for the series or the slice at `index`: `chart-1`
/// through `chart-5`, then again from `chart-1`.
pub(crate) fn chart_color(tokens: &DesignTokens, index: usize) -> choscorui_core::Color {
    const ROLES: [ColorRole; 5] = [
        ColorRole::Chart1,
        ColorRole::Chart2,
        ColorRole::Chart3,
        ColorRole::Chart4,
        ColorRole::Chart5,
    ];
    tokens.color(ROLES[index % ROLES.len()])
}

/// A chart. The app gives the categories and the series. The pointer or
/// the arrow keys pick a category, and the tooltip shows its values. An
/// optional legend names the series, or the categories for pie and
/// radial charts.
pub struct Chart {
    name: String,
    kind: ChartKind,
    categories: Vec<String>,
    series: Vec<ChartSeries>,
    legend: bool,
    donut: bool,
    format: Box<dyn Fn(f64) -> String>,
    /// The picked category, and whether its tooltip shows.
    active: Option<usize>,
    tooltip: bool,
    plot: Plot,
    rtl: bool,
}

impl Chart {
    pub fn new(
        name: impl Into<String>,
        kind: ChartKind,
        categories: Vec<String>,
        series: Vec<ChartSeries>,
    ) -> Self {
        Self {
            name: name.into(),
            kind,
            categories,
            series,
            legend: false,
            donut: false,
            format: Box::new(|value| value.to_string()),
            active: None,
            tooltip: false,
            plot: Plot::default(),
            rtl: false,
        }
    }

    /// Shows the legend under the chart.
    #[must_use]
    pub fn with_legend(mut self) -> Self {
        self.legend = true;
        self
    }

    /// Cuts a hole in a pie chart.
    #[must_use]
    pub fn donut(mut self) -> Self {
        self.donut = true;
        self
    }

    /// Formats values in the tooltip and in semantics.
    #[must_use]
    pub fn with_format(mut self, format: impl Fn(f64) -> String + 'static) -> Self {
        self.format = Box::new(format);
        self
    }

    fn query(slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Chart, slot)
    }

    /// Whether the chart shows one value per category, from the first
    /// series, with a color per category.
    fn per_category(&self) -> bool {
        matches!(self.kind, ChartKind::Pie | ChartKind::Radial)
    }

    /// The value of `series` at `category`, zero when it is missing.
    fn value(&self, series: usize, category: usize) -> f64 {
        self.series
            .get(series)
            .and_then(|series| series.values.get(category))
            .copied()
            .filter(|value| value.is_finite())
            .unwrap_or(0.0)
    }

    /// The tooltip rows of `category`: color index, label, and value.
    fn rows(&self, category: usize) -> Vec<(usize, String, String)> {
        if self.per_category() {
            let label = self.categories.get(category).cloned().unwrap_or_default();
            vec![(category, label, (self.format)(self.value(0, category)))]
        } else {
            self.series
                .iter()
                .enumerate()
                .map(|(index, series)| {
                    (
                        index,
                        series.label.clone(),
                        (self.format)(self.value(index, category)),
                    )
                })
                .collect()
        }
    }

    /// The legend entries: color index and label.
    fn legend_entries(&self) -> Vec<(usize, &str)> {
        if self.per_category() {
            self.categories
                .iter()
                .enumerate()
                .map(|(index, label)| (index, label.as_str()))
                .collect()
        } else {
            self.series
                .iter()
                .enumerate()
                .map(|(index, series)| (index, series.label.as_str()))
                .collect()
        }
    }

    fn pick(&mut self, cx: &mut ControlContext<'_>, active: Option<usize>, tooltip: bool) {
        if active != self.active || tooltip != self.tooltip {
            self.active = active;
            self.tooltip = tooltip && active.is_some();
            cx.request_paint();
            cx.request_semantics();
        }
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        let count = self.categories.len();
        if count == 0 {
            return EventReply::Continue;
        }
        let current = self.active.unwrap_or(0);
        // Cartesian categories follow the reading order; polar ones go
        // clockwise, and the arrow keys step through them in order.
        let mirrored = self.rtl
            && !matches!(
                self.kind,
                ChartKind::Pie | ChartKind::Radar | ChartKind::Radial
            );
        let (next, previous) = if mirrored {
            (Key::ArrowLeft, Key::ArrowRight)
        } else {
            (Key::ArrowRight, Key::ArrowLeft)
        };
        let target = match key {
            Key::Escape if self.tooltip => {
                self.pick(cx, self.active, false);
                return EventReply::Stop;
            }
            Key::Home => 0,
            Key::End => count - 1,
            key if key == next || key == Key::ArrowDown => {
                if self.active.is_some() {
                    (current + 1).min(count - 1)
                } else {
                    0
                }
            }
            key if key == previous || key == Key::ArrowUp => current.saturating_sub(1),
            _ => return EventReply::Continue,
        };
        self.pick(cx, Some(target), true);
        EventReply::Stop
    }

    /// The semantic summary of `category`: its label and its values.
    fn summary(&self, category: usize) -> String {
        let label = self.categories.get(category).cloned().unwrap_or_default();
        if self.per_category() {
            return format!("{label}: {}", (self.format)(self.value(0, category)));
        }
        let values: Vec<String> = self
            .rows(category)
            .into_iter()
            .map(|(_, series, value)| format!("{series} {value}"))
            .collect();
        format!("{label}: {}", values.join(", "))
    }
}

impl CustomControl for Chart {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured {
            size: self.measure_size(cx),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.rtl = cx.rtl();
        self.plot = self.layout(cx, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                let hit = self.category_at(*position);
                if hit.is_some() || !cx.is_focused() {
                    self.pick(cx, hit, true);
                }
                EventReply::Continue
            }
            ControlEvent::Hover(false) if !cx.is_focused() => {
                self.pick(cx, None, false);
                EventReply::Continue
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.key(cx, *key),
            ControlEvent::FocusChanged(false) | ControlEvent::Dismiss => {
                self.pick(cx, self.active, false);
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        self.paint_chart(cx);
    }

    fn overlay(&self) -> Option<Rect> {
        if !self.tooltip {
            return None;
        }
        self.active.map(|category| self.tooltip_rect(category))
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        if let (true, Some(category)) = (self.tooltip, self.active) {
            self.paint_tooltip(cx, category);
        }
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = hosted::node(SemanticRole::Image, &self.name, vec![SemanticAction::Focus]);
        node.value = self.active.map(|category| self.summary(category));
        node.description = self
            .legend_entries()
            .into_iter()
            .map(|(_, label)| label)
            .collect::<Vec<_>>()
            .join(", ");
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if action == SemanticAction::Focus {
            cx.request_focus();
            return true;
        }
        false
    }

    fn focusable(&self) -> bool {
        !self.categories.is_empty()
    }
}

/// The size a chart needs when no width is given.
fn natural_width(cx: &MeasureContext<'_>) -> f64 {
    cx.tokens.spacing * 120.0
}

impl Chart {
    fn measure_size(&self, cx: &MeasureContext<'_>) -> Size {
        let root = cx.style(Self::query(Slot::Root));
        let max = cx.constraints.max.width;
        let width = if max.is_finite() {
            max
        } else {
            natural_width(cx)
        };
        Size::new(
            width,
            root.min_height + self.axis_height(cx) + self.legend_height(cx),
        )
    }
}
