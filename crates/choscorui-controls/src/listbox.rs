//! A popup list of options for the hosted controls: placement, highlight,
//! type-ahead, scrolling, and paint. `Select` uses it, and other popup
//! lists can share it.

use crate::custom::MeasureContext;
use crate::popup::{Placement, gap_anchor, place_popup};
use crate::style::{
    Content, ControlKind, PaintContext, ResolvedStyle, Slot, StyleQuery, TextAlign, VisualState,
};
use choscorui_core::{Icon, Key, Point, PointerButton, PointerEvent, Rect, Size, WheelUnit};

/// One row of a popup list.
#[derive(Clone, Debug, PartialEq)]
pub enum ListEntry {
    /// A choice the user can highlight and pick.
    Option { label: String, enabled: bool },
    /// A caption above the options that follow it.
    Group(String),
    /// A rule between groups.
    Separator,
}

impl ListEntry {
    pub fn option(label: impl Into<String>) -> Self {
        Self::Option {
            label: label.into(),
            enabled: true,
        }
    }

    /// An option the user cannot pick.
    pub fn disabled(label: impl Into<String>) -> Self {
        Self::Option {
            label: label.into(),
            enabled: false,
        }
    }

    pub fn group(label: impl Into<String>) -> Self {
        Self::Group(label.into())
    }

    /// The option or group caption text, empty for a separator.
    pub fn label(&self) -> &str {
        match self {
            Self::Option { label, .. } | Self::Group(label) => label,
            Self::Separator => "",
        }
    }

    /// True for an option the user can pick.
    pub fn is_enabled_option(&self) -> bool {
        matches!(self, Self::Option { enabled: true, .. })
    }
}

/// How long typed characters build one type-ahead search, in seconds.
const TYPE_AHEAD_SECONDS: f64 = 1.0;

/// The most rows the list shows before it scrolls.
const MAX_ROWS: usize = 8;

/// How long the pointer rests on a scroll button before the list moves
/// one more row, in seconds.
const SCROLL_REPEAT_SECONDS: f64 = 0.05;

/// What a pointer event did to an open list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ListPointer {
    /// The event is not for the list.
    Ignored,
    /// The list used the event; `paint` says that its look changed.
    Used { paint: bool },
    /// A release on the enabled option at this index.
    Chosen(usize),
}

/// The open list's geometry and highlight. Rectangles are in window
/// coordinates with no scroll applied.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ListPopup {
    kind: ControlKind,
    pub entries: Vec<ListEntry>,
    pub highlighted: Option<usize>,
    pub scroll: f64,
    pub frame: Rect,
    viewport: Rect,
    rows: Vec<Rect>,
    /// The height of the scroll-up and scroll-down buttons.
    button_height: f64,
    /// The scroll step of one row or wheel line: the option height.
    line: f64,
    /// The direction of the scroll button under the pointer and the
    /// host time of its next row step.
    auto_scroll: Option<(f64, f64)>,
    typed: String,
    typed_at: f64,
}

impl ListPopup {
    pub fn new(kind: ControlKind, entries: Vec<ListEntry>) -> Self {
        Self {
            kind,
            entries,
            highlighted: None,
            scroll: 0.0,
            frame: Rect::default(),
            viewport: Rect::default(),
            rows: Vec::new(),
            button_height: 0.0,
            line: 0.0,
            auto_scroll: None,
            typed: String::new(),
            typed_at: f64::NEG_INFINITY,
        }
    }

    fn query(&self, slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(self.kind, slot).state(state)
    }

    fn entry_style(
        &self,
        entry: &ListEntry,
        style: impl Fn(StyleQuery) -> ResolvedStyle,
    ) -> ResolvedStyle {
        let slot = match entry {
            ListEntry::Option { .. } => Slot::Item,
            ListEntry::Group(_) => Slot::GroupLabel,
            ListEntry::Separator => Slot::Separator,
        };
        style(self.query(slot, VisualState::NONE))
    }

    fn entry_height(entry: &ListEntry, style: &ResolvedStyle) -> f64 {
        match entry {
            ListEntry::Separator => style.min_height + style.padding.vertical(),
            _ => (style.text.line_height + style.padding.vertical()).max(style.min_height),
        }
    }

    /// The width that fits every row, the content padding, and the
    /// content's minimum width.
    pub fn natural_width(&self, cx: &MeasureContext<'_>) -> f64 {
        let content = cx.style(self.query(Slot::Content, VisualState::NONE));
        let rows = self
            .entries
            .iter()
            .map(|entry| {
                let style = self.entry_style(entry, |query| cx.style(query));
                Content::label(entry.label()).size(&style, cx.text).width
            })
            .fold(0.0, f64::max);
        (rows + content.padding.horizontal()).max(content.min_width)
    }

    /// Places the list `gap` below `anchor`, at least `min_width` wide and
    /// aligned with the anchor's leading edge. It shows at most 8 options.
    /// It flips above the anchor when it does not fit below, and it
    /// shrinks to the window (and scrolls) when it fits on neither side.
    pub fn place(&mut self, cx: &MeasureContext<'_>, anchor: Rect, min_width: f64, gap: f64) {
        let content = cx.style(self.query(Slot::Content, VisualState::NONE));
        self.button_height = cx
            .style(self.query(Slot::Previous, VisualState::NONE))
            .min_height;
        self.line = cx
            .style(self.query(Slot::Item, VisualState::NONE))
            .min_height;
        let heights: Vec<f64> = self
            .entries
            .iter()
            .map(|entry| Self::entry_height(entry, &self.entry_style(entry, |q| cx.style(q))))
            .collect();
        let mut options = 0;
        let mut visible_height = 0.0;
        for (entry, height) in self.entries.iter().zip(&heights) {
            if options < MAX_ROWS {
                visible_height += height;
            }
            if matches!(entry, ListEntry::Option { .. }) {
                options += 1;
            }
        }
        let size = Size::new(
            self.natural_width(cx).max(min_width),
            (visible_height + content.padding.vertical()).max(content.min_height),
        );
        let placed = place_popup(
            gap_anchor(anchor, gap, None),
            size,
            cx.popup_bounds(),
            Placement::Below,
            cx.direction,
        );
        self.frame = placed.rect;
        let inner_x = self.frame.origin.x + content.padding.horizontal() / 2.0;
        let inner_width = (self.frame.size.width - content.padding.horizontal()).max(0.0);
        let top = self.frame.origin.y + content.padding.block_start;
        let mut y = top;
        self.rows = heights
            .iter()
            .map(|height| {
                let row = Rect::from_xywh(inner_x, y, inner_width, *height);
                y += height;
                row
            })
            .collect();
        let room = (self.frame.size.height - content.padding.vertical()).max(0.0);
        self.viewport = Rect::from_xywh(inner_x, top, inner_width, visible_height.min(room));
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    fn content_height(&self) -> f64 {
        match (self.rows.first(), self.rows.last()) {
            (Some(first), Some(last)) => last.max_y() - first.origin.y,
            _ => 0.0,
        }
    }

    pub fn max_scroll(&self) -> f64 {
        (self.content_height() - self.viewport.size.height).max(0.0)
    }

    pub fn scroll_by(&mut self, delta: f64) {
        self.scroll = (self.scroll + delta).clamp(0.0, self.max_scroll());
    }

    /// Stops the scroll that a hovered scroll button drives, for example
    /// when the pointer leaves the list or the list closes.
    pub fn stop_scroll(&mut self) {
        self.auto_scroll = None;
    }

    /// The host time of the next row step while the pointer rests on a
    /// scroll button.
    pub fn deadline(&self) -> Option<f64> {
        self.auto_scroll.map(|(_, at)| at)
    }

    /// Scrolls one row for the hovered scroll button at `time`. The
    /// scroll stops at the end of the list, where the button goes away.
    /// Returns true when the list scrolled.
    pub fn step_scroll(&mut self, time: f64) -> bool {
        let Some((direction, _)) = self.auto_scroll else {
            return false;
        };
        self.scroll_by(direction * self.line);
        let end = if direction < 0.0 {
            self.scroll <= 0.0
        } else {
            self.scroll >= self.max_scroll()
        };
        self.auto_scroll = (!end).then_some((direction, time + SCROLL_REPEAT_SECONDS));
        true
    }

    /// Handles a pointer event at `time` while the list is open: a press
    /// on a scroll button scrolls one row, resting on a scroll button
    /// scrolls on, a move highlights the option under the pointer, the
    /// wheel scrolls, and a primary-button release on an option chooses it.
    pub fn pointer(&mut self, event: &PointerEvent, time: f64) -> ListPointer {
        match *event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } if self.frame.contains(position) => {
                let direction = self.scroll_button(position);
                if let Some(direction) = direction {
                    self.scroll_by(direction * self.line);
                }
                ListPointer::Used {
                    paint: direction.is_some(),
                }
            }
            PointerEvent::Up {
                position,
                button: PointerButton::Primary,
            } => self
                .hit(position)
                .map_or(ListPointer::Ignored, ListPointer::Chosen),
            PointerEvent::Move { position } => {
                let direction = self.scroll_button(position);
                self.auto_scroll = match (direction, self.auto_scroll) {
                    (Some(direction), Some((current, at))) if direction == current => {
                        Some((current, at))
                    }
                    (Some(direction), _) => Some((direction, time + SCROLL_REPEAT_SECONDS)),
                    (None, _) => None,
                };
                match self.hit(position) {
                    Some(index) if self.highlighted != Some(index) => {
                        self.highlighted = Some(index);
                        ListPointer::Used { paint: true }
                    }
                    _ => ListPointer::Ignored,
                }
            }
            PointerEvent::Wheel {
                position,
                delta,
                unit,
            } if self.frame.contains(position) => {
                let step = match unit {
                    WheelUnit::Points => delta.y,
                    WheelUnit::Lines => delta.y * self.line,
                };
                self.scroll_by(step);
                ListPointer::Used { paint: true }
            }
            _ => ListPointer::Ignored,
        }
    }

    /// Scrolls so that the entry at `index` is in view.
    fn scroll_to(&mut self, index: usize) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let top = row.origin.y - self.viewport.origin.y;
        if top < self.scroll {
            self.scroll = top;
        } else if top + row.size.height > self.scroll + self.viewport.size.height {
            self.scroll = top + row.size.height - self.viewport.size.height;
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    /// Highlights the entry at `index` and scrolls it into view.
    pub fn highlight(&mut self, index: Option<usize>) {
        self.highlighted = index;
        if let Some(index) = index {
            self.scroll_to(index);
        }
    }

    /// The enabled options, in order.
    pub fn options(&self) -> impl Iterator<Item = usize> + '_ {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.is_enabled_option())
            .map(|(index, _)| index)
    }

    /// Moves the highlight for ArrowUp, ArrowDown, Home, or End. The
    /// arrows stop at the ends. Returns false for other keys.
    pub fn step(&mut self, key: Key) -> bool {
        let options: Vec<usize> = self.options().collect();
        let (Some(first), Some(last)) = (options.first(), options.last()) else {
            return matches!(key, Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End);
        };
        let current = self
            .highlighted
            .and_then(|index| options.iter().position(|option| *option == index));
        let next = match (key, current) {
            (Key::Home, _) | (Key::ArrowDown, None) => *first,
            (Key::End, _) | (Key::ArrowUp, None) => *last,
            (Key::ArrowDown, Some(at)) => options[(at + 1).min(options.len() - 1)],
            (Key::ArrowUp, Some(at)) => options[at.saturating_sub(1)],
            _ => return false,
        };
        self.highlight(Some(next));
        true
    }

    /// The option that matches the characters typed so far, searching
    /// after the highlight. Repeating one character cycles through the
    /// options that start with it.
    pub fn type_ahead(&mut self, text: &str, time: f64) -> Option<usize> {
        if time - self.typed_at > TYPE_AHEAD_SECONDS {
            self.typed.clear();
        }
        self.typed_at = time;
        self.typed.push_str(&text.to_lowercase());
        let first = self.typed.chars().next()?;
        let repeated = self.typed.chars().all(|c| c == first);
        let search = if repeated {
            first.to_string()
        } else {
            self.typed.clone()
        };
        let options: Vec<usize> = self.options().collect();
        let current = self
            .highlighted
            .and_then(|index| options.iter().position(|option| *option == index));
        // A new search starts after the highlight; a longer one keeps it.
        let start = match current {
            Some(at) if repeated => at + 1,
            Some(at) => at,
            None => 0,
        };
        (0..options.len())
            .map(|offset| options[(start + offset) % options.len()])
            .find(|index| {
                self.entries[*index]
                    .label()
                    .to_lowercase()
                    .starts_with(&search)
            })
    }

    /// The enabled option under `point`, if any.
    pub fn hit(&self, point: Point) -> Option<usize> {
        if !self.viewport.contains(point) || self.scroll_button(point).is_some() {
            return None;
        }
        let point = Point::new(point.x, point.y + self.scroll);
        self.rows
            .iter()
            .position(|row| row.contains(point))
            .filter(|index| self.entries[*index].is_enabled_option())
    }

    /// The scroll-up or scroll-down button under `point`: `-1` or `1`.
    fn scroll_button(&self, point: Point) -> Option<f64> {
        let (up, down) = self.scroll_buttons();
        if up.is_some_and(|rect| rect.contains(point)) {
            Some(-1.0)
        } else if down.is_some_and(|rect| rect.contains(point)) {
            Some(1.0)
        } else {
            None
        }
    }

    fn scroll_buttons(&self) -> (Option<Rect>, Option<Rect>) {
        let height = self.button_height;
        let up = (self.scroll > 0.0).then(|| {
            Rect::from_xywh(
                self.viewport.origin.x,
                self.viewport.origin.y,
                self.viewport.size.width,
                height,
            )
        });
        let down = (self.scroll < self.max_scroll()).then(|| {
            Rect::from_xywh(
                self.viewport.origin.x,
                self.viewport.max_y() - height,
                self.viewport.size.width,
                height,
            )
        });
        (up, down)
    }

    /// Paints the list. `checked` marks options that show `indicator` at
    /// the end of their row.
    pub fn paint(
        &self,
        cx: &mut PaintContext<'_>,
        checked: &dyn Fn(usize) -> bool,
        indicator: Icon,
    ) {
        let content = cx.style(self.query(Slot::Content, VisualState::OPEN));
        let viewport = self.viewport;
        let entries = &self.entries;
        let rows = &self.rows;
        let (kind, highlighted, scroll) = (self.kind, self.highlighted, self.scroll);
        cx.slot(self.frame, &content, |cx, _| {
            cx.list.push_clip(viewport);
            for (index, (entry, row)) in entries.iter().zip(rows).enumerate() {
                let row = row.translate(0.0, -scroll);
                if row.max_y() < viewport.origin.y || row.origin.y > viewport.max_y() {
                    continue;
                }
                match entry {
                    ListEntry::Option { label, enabled } => {
                        let state = VisualState::NONE
                            .set(VisualState::HIGHLIGHTED, highlighted == Some(index))
                            .set(VisualState::CHECKED, checked(index))
                            .set(VisualState::DISABLED, !enabled);
                        let item = cx.style(StyleQuery::new(kind, Slot::Item).state(state));
                        let mark =
                            cx.style(StyleQuery::new(kind, Slot::ItemIndicator).state(state));
                        cx.slot(row, &item, |cx, inner| {
                            cx.label(inner, label, &item, TextAlign::Start);
                        });
                        if checked(index) {
                            // The indicator is centered in the row's end padding.
                            let end = item.padding.inline_end;
                            let x = if cx.rtl {
                                row.origin.x
                            } else {
                                row.max_x() - end
                            };
                            cx.icon(
                                Rect::from_xywh(x, row.origin.y, end, row.size.height),
                                indicator,
                                &mark,
                            );
                        }
                    }
                    ListEntry::Group(label) => {
                        let style = cx.style(StyleQuery::new(kind, Slot::GroupLabel));
                        cx.slot(row, &style, |cx, inner| {
                            cx.label(inner, label, &style, TextAlign::Start);
                        });
                    }
                    ListEntry::Separator => {
                        let style = cx.style(StyleQuery::new(kind, Slot::Separator));
                        let line = Rect::from_xywh(
                            row.origin.x + style.padding.inline_start,
                            row.origin.y + style.padding.block_start,
                            row.size.width - style.padding.horizontal(),
                            style.min_height,
                        );
                        cx.slot(
                            line,
                            &ResolvedStyle {
                                padding: Default::default(),
                                ..style
                            },
                            |_, _| {},
                        );
                    }
                }
            }
            let _ = cx.list.pop_clip();
        });
        let (up, down) = self.scroll_buttons();
        for (rect, slot, icon) in [
            (up, Slot::Previous, Icon::ChevronUp),
            (down, Slot::Next, Icon::ChevronDown),
        ] {
            if let Some(rect) = rect {
                let style = cx.style(self.query(slot, VisualState::NONE));
                cx.slot(rect, &style, |cx, inner| cx.icon(inner, icon, &style));
            }
        }
    }
}
