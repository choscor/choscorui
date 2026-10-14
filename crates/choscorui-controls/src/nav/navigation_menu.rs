//! A row of site links whose triggers open panels of described links.

use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::form::measured::{paint_wrapped, wrapped_height};
use crate::hosted::{self, Parts, Reveal};
use crate::popup::{Placement, gap_anchor, place_popup};
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState};
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, LayoutDirection, Measured, Point, PointerButton,
    PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRole, Size,
};

/// How long the pointer rests on a trigger before its panel opens, in
/// seconds. Moving to another trigger while a panel is open switches at
/// once.
pub const NAVIGATION_OPEN_DELAY: f64 = 0.2;

/// How long a panel stays after the pointer leaves the menu, in seconds.
pub const NAVIGATION_CLOSE_DELAY: f64 = 0.3;

/// The width of one link column in a panel, in spacing units.
const COLUMN_UNITS: f64 = 60.0;

/// The gap between a trigger and its panel, in spacing units.
const OFFSET_UNITS: f64 = 1.5;

/// A link in a navigation menu panel.
#[derive(Clone, Debug, PartialEq)]
pub struct NavigationLink {
    pub title: String,
    pub description: String,
}

impl NavigationLink {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Focus {
    Trigger(usize),
    Link(usize),
}

/// A navigation menu: a row of triggers. A trigger with links opens a
/// panel of titled links below the row; a trigger without links is a
/// link itself.
pub struct NavigationMenu {
    name: String,
    items: Vec<(String, Vec<NavigationLink>)>,
    triggers: Vec<Rect>,
    focus: Focus,
    open: Option<usize>,
    /// The panel that shows, also while it fades out.
    shown: usize,
    panel: Rect,
    links: Vec<Rect>,
    reveal: Reveal,
    motion_ms: u32,
    /// Hover and press of the triggers and the links.
    tracked: Parts<Focus>,
    /// The trigger the pointer rests on, and when its panel opens.
    pending: Option<(usize, f64)>,
    close_at: Option<f64>,
    enabled: bool,
    rtl: bool,
    /// The area that the panel fits in.
    room: Rect,
    /// How far the links scroll up in a panel that the window limits.
    scroll: f64,
    /// Link heights and the panel padding, read from the recipe.
    link_heights: Vec<Vec<f64>>,
    padding: crate::style::Edges,
    gap: f64,
    /// The link column width and the panel offset, from the spacing.
    column: f64,
    offset: f64,
    /// The trigger that the pointer rested on when a click or a key
    /// closed its panel. Hover does not open it again until the pointer
    /// leaves it.
    quiet: Option<usize>,
    /// The item of the current page.
    current: Option<usize>,
}

impl NavigationMenu {
    pub fn new(name: impl Into<String>, items: Vec<(String, Vec<NavigationLink>)>) -> Self {
        Self {
            name: name.into(),
            items,
            triggers: Vec::new(),
            focus: Focus::Trigger(0),
            open: None,
            shown: 0,
            panel: Rect::default(),
            links: Vec::new(),
            scroll: 0.0,
            reveal: Reveal::default(),
            motion_ms: 0,
            tracked: Parts::default(),
            pending: None,
            close_at: None,
            enabled: true,
            rtl: false,
            room: Rect::from_xywh(0.0, 0.0, f64::MAX, f64::MAX),
            link_heights: Vec::new(),
            padding: crate::style::Edges::ZERO,
            gap: 0.0,
            column: 0.0,
            offset: 0.0,
            quiet: None,
            current: None,
        }
    }

    /// Marks `item` as the current page: its trigger shows selected.
    #[must_use]
    pub fn with_current(mut self, item: usize) -> Self {
        self.current = (item < self.items.len()).then_some(item);
        self
    }

    /// The trigger whose panel is open.
    pub fn open_item(&self) -> Option<usize> {
        self.open
    }

    fn query(slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::NavigationMenu, slot).state(state)
    }

    fn has_links(&self, item: usize) -> bool {
        self.items
            .get(item)
            .is_some_and(|(_, links)| !links.is_empty())
    }

    /// The index that `ItemActivated` reports: links count in order, and
    /// a trigger without links counts as one link.
    fn flat_index(&self, item: usize, link: Option<usize>) -> usize {
        let before: usize = self.items[..item]
            .iter()
            .map(|(_, links)| links.len().max(1))
            .sum();
        before + link.unwrap_or(0)
    }

    fn columns(count: usize) -> usize {
        if count > 3 { 2 } else { 1 }
    }

    /// Places the panel of `item` below its trigger and its links in
    /// columns.
    fn place_panel(&mut self, item: usize) {
        let Some(trigger) = self.triggers.get(item).copied() else {
            return;
        };
        let heights = &self.link_heights[item];
        let columns = Self::columns(heights.len());
        let rows = heights.len().div_ceil(columns);
        let row_heights: Vec<f64> = (0..rows)
            .map(|row| {
                (0..columns)
                    .filter_map(|column| heights.get(row * columns + column))
                    .fold(0.0, |a: f64, b| a.max(*b))
            })
            .collect();
        let pad = self.padding;
        let column_width = self.column;
        let width =
            columns as f64 * column_width + (columns - 1) as f64 * self.gap + pad.horizontal();
        let height = row_heights.iter().sum::<f64>()
            + rows.saturating_sub(1) as f64 * self.gap
            + pad.vertical();
        let direction = if self.rtl {
            LayoutDirection::Rtl
        } else {
            LayoutDirection::Ltr
        };
        let size = Size::new(width, height);
        // A window that is too short limits the panel, and the links
        // scroll in it.
        self.panel = place_popup(
            gap_anchor(trigger, self.offset, None),
            size,
            self.room,
            Placement::Below,
            direction,
        )
        .rect;
        self.scroll = self
            .scroll
            .clamp(0.0, (height - self.panel.size.height).max(0.0));
        let (x, y) = (self.panel.origin.x, self.panel.origin.y);
        let mut links = Vec::new();
        let mut top = y + pad.block_start - self.scroll;
        for (row, row_height) in row_heights.iter().enumerate() {
            for column in 0..columns {
                if row * columns + column >= heights.len() {
                    break;
                }
                let offset = column as f64 * (column_width + self.gap);
                let left = if self.rtl {
                    self.panel.max_x() - pad.inline_start - offset - column_width
                } else {
                    x + pad.inline_start + offset
                };
                links.push(Rect::from_xywh(left, top, column_width, *row_height));
            }
            top += row_height + self.gap;
        }
        self.links = links;
    }

    /// Shows the panel of `item` at `time` and cancels the hover delays.
    /// Returns false when the item has no links.
    fn show(&mut self, item: usize, time: f64) -> bool {
        self.pending = None;
        self.close_at = None;
        if !self.has_links(item) {
            return false;
        }
        if self.open == Some(item) {
            return true;
        }
        let was_open = self.open.is_some();
        // A link of another panel loses the focus to the trigger.
        if let Focus::Link(_) = self.focus {
            self.focus = Focus::Trigger(item);
        }
        self.open = Some(item);
        self.shown = item;
        self.scroll = 0.0;
        self.place_panel(item);
        if !was_open {
            self.reveal.set(true, time);
        }
        true
    }

    /// Closes the open panel at `time` and cancels the hover delays.
    /// Returns true when a panel was open.
    fn hide(&mut self, time: f64) -> bool {
        self.pending = None;
        self.close_at = None;
        let Some(item) = self.open.take() else {
            return false;
        };
        self.reveal.set(false, time);
        if let Focus::Link(_) = self.focus {
            self.focus = Focus::Trigger(item);
        }
        true
    }

    /// The item that `Expand` opens: the focused trigger, or the open
    /// panel's trigger while a link has focus.
    fn expand_target(&self) -> usize {
        match self.focus {
            Focus::Trigger(item) => item,
            Focus::Link(_) => self.open.unwrap_or(0),
        }
    }

    fn open_at(&mut self, cx: &mut ControlContext<'_>, item: usize) {
        let before = self.open;
        if !self.show(item, cx.time()) || before == self.open {
            return;
        }
        if before.is_none() {
            cx.emit(ControlMessage::OpenChanged(true));
        }
        cx.request_paint();
        cx.request_semantics();
    }

    /// Closes the open panel. Returns true when a panel was open. Hover
    /// does not open the panel again while the pointer stays on its
    /// trigger.
    fn close(&mut self, cx: &mut ControlContext<'_>) -> bool {
        self.quiet = match self.tracked.hovered {
            Some(Focus::Trigger(item)) => Some(item),
            _ => None,
        };
        let closed = self.hide(cx.time());
        if closed {
            cx.emit(ControlMessage::OpenChanged(false));
            cx.request_paint();
            cx.request_semantics();
        }
        closed
    }

    /// Closes the open panel without a choice (Escape, an outside press,
    /// focus loss, or disabling), and reports `Dismissed`.
    fn dismiss(&mut self, cx: &mut ControlContext<'_>) -> EventReply {
        let closed = self.close(cx);
        hosted::dismissed(cx, closed)
    }

    fn activate(&mut self, cx: &mut ControlContext<'_>, focus: Focus) {
        match focus {
            Focus::Trigger(item) if self.has_links(item) => {
                if self.open == Some(item) {
                    self.close(cx);
                } else {
                    self.open_at(cx, item);
                }
            }
            Focus::Trigger(item) => {
                cx.emit(ControlMessage::ItemActivated(self.flat_index(item, None)));
                self.close(cx);
            }
            Focus::Link(link) => {
                if let Some(item) = self.open {
                    cx.emit(ControlMessage::ItemActivated(
                        self.flat_index(item, Some(link)),
                    ));
                    self.close(cx);
                }
            }
        }
    }

    fn hit(&self, point: Point) -> Option<Focus> {
        if self.open.is_some()
            && self.panel.contains(point)
            && let Some(link) = hosted::hit(&self.links, point)
        {
            return Some(Focus::Link(link));
        }
        hosted::hit(&self.triggers, point).map(Focus::Trigger)
    }

    fn set_focus(&mut self, cx: &mut ControlContext<'_>, focus: Focus) {
        self.focus = focus;
        if let (Focus::Link(link), Some(item)) = (focus, self.open)
            && let Some(rect) = self.links.get(link).copied()
        {
            // The focused link scrolls into view.
            let panel = self.panel;
            let shift = if rect.origin.y < panel.origin.y {
                rect.origin.y - panel.origin.y - self.padding.block_start
            } else if rect.max_y() > panel.max_y() {
                rect.max_y() - panel.max_y() + self.padding.block_end
            } else {
                0.0
            };
            if shift != 0.0 {
                self.scroll += shift;
                self.place_panel(item);
            }
        }
        cx.request_paint();
        cx.request_semantics();
    }

    /// Scrolls the open panel under a wheel event. Returns true when the
    /// panel takes the event.
    fn wheel(&mut self, event: &ControlEvent) -> bool {
        let ControlEvent::Pointer(PointerEvent::Wheel {
            position, delta, ..
        }) = event
        else {
            return false;
        };
        let Some(item) = self.open.filter(|_| self.panel.contains(*position)) else {
            return false;
        };
        self.scroll += delta.y;
        self.place_panel(item);
        true
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        let count = self.items.len();
        let links = self.open.map_or(0, |item| self.items[item].1.len());
        match (key, self.focus) {
            (Key::Escape, _) if self.open.is_some() => {
                self.dismiss(cx);
            }
            (Key::Enter | Key::Space, focus) => self.activate(cx, focus),
            (Key::ArrowDown, Focus::Trigger(item)) if self.has_links(item) => {
                self.open_at(cx, item);
                self.set_focus(cx, Focus::Link(0));
            }
            // The links sit in a grid in reading order: ArrowDown and
            // ArrowUp move by a row, and ArrowLeft and ArrowRight in the
            // row.
            (Key::ArrowDown, Focus::Link(link)) => {
                let below = link + Self::columns(links);
                self.set_focus(cx, Focus::Link(if below < links { below } else { link }));
            }
            (Key::ArrowUp, Focus::Link(link)) => {
                let focus = match link.checked_sub(Self::columns(links)) {
                    Some(above) => Focus::Link(above),
                    None => Focus::Trigger(self.open.unwrap_or(0)),
                };
                self.set_focus(cx, focus);
            }
            (Key::ArrowLeft | Key::ArrowRight, Focus::Link(link)) => {
                let columns = Self::columns(links);
                let column = link % columns;
                let next = match key.in_reading_order(self.rtl) {
                    Key::ArrowRight if column + 1 < columns && link + 1 < links => link + 1,
                    Key::ArrowLeft if column > 0 => link - 1,
                    _ => link,
                };
                self.set_focus(cx, Focus::Link(next));
            }
            (Key::Home, Focus::Link(_)) => self.set_focus(cx, Focus::Link(0)),
            (Key::End, Focus::Link(_)) => {
                self.set_focus(cx, Focus::Link(links.saturating_sub(1)));
            }
            (Key::ArrowLeft | Key::ArrowRight | Key::Home | Key::End, Focus::Trigger(item)) => {
                let Some(next) = hosted::rove(key, item, count, true, self.rtl) else {
                    return EventReply::Continue;
                };
                self.close(cx);
                self.set_focus(cx, Focus::Trigger(next));
            }
            (Key::Tab, focus) => {
                // Tab stops: each trigger, then the links of its open panel.
                let back = cx.modifiers().shift;
                let next = match (focus, back) {
                    (Focus::Trigger(item), false) if self.open == Some(item) && links > 0 => {
                        Some(Focus::Link(0))
                    }
                    (Focus::Link(link), false) if link + 1 < links => Some(Focus::Link(link + 1)),
                    (Focus::Link(_), false) => self
                        .open
                        .map(|item| item + 1)
                        .filter(|n| *n < count)
                        .map(Focus::Trigger),
                    (Focus::Trigger(item), false) => {
                        (item + 1 < count).then(|| Focus::Trigger(item + 1))
                    }
                    (Focus::Link(0), true) => self.open.map(Focus::Trigger),
                    (Focus::Link(link), true) => Some(Focus::Link(link - 1)),
                    (Focus::Trigger(item), true) => item.checked_sub(1).map(Focus::Trigger),
                };
                let Some(next) = next else {
                    self.close(cx);
                    return EventReply::Continue;
                };
                if let (Focus::Trigger(item), Some(open)) = (next, self.open)
                    && item != open
                {
                    self.close(cx);
                }
                self.set_focus(cx, next);
            }
            _ => return EventReply::Continue,
        }
        EventReply::Stop
    }

    fn pointer(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &PointerEvent,
        pressed: Option<Focus>,
    ) -> EventReply {
        let hit = self.tracked.hovered;
        match *event {
            PointerEvent::Down {
                button: PointerButton::Primary,
                ..
            } => {
                let Some(focus) = hit else {
                    if !self.panel.contains(event.position()) || self.open.is_none() {
                        return self.dismiss(cx);
                    }
                    return EventReply::Stop;
                };
                self.focus = focus;
                cx.request_focus();
                EventReply::Stop
            }
            PointerEvent::Up { .. } if pressed.is_some() => {
                if let Some(focus) = pressed.filter(|focus| hit == Some(*focus)) {
                    self.activate(cx, focus);
                }
                EventReply::Stop
            }
            PointerEvent::Move { .. } => {
                if self.quiet.is_some() && hit != self.quiet.map(Focus::Trigger) {
                    self.quiet = None;
                }
                if let Some(Focus::Trigger(item)) = hit
                    && self.has_links(item)
                    && self.open != Some(item)
                    && self.quiet != Some(item)
                {
                    if self.open.is_some() {
                        self.open_at(cx, item);
                    } else if self.pending.is_none_or(|(pending, _)| pending != item) {
                        self.pending = Some((item, cx.time() + NAVIGATION_OPEN_DELAY));
                    }
                }
                if !matches!(hit, Some(Focus::Trigger(_))) {
                    self.pending = None;
                }
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    /// The state of the trigger or link `focus`: hovered, pressed, and
    /// focused on its own, and disabled with the host.
    fn part_state(&self, focus: Focus, host: VisualState) -> VisualState {
        let focused = self.focus == focus;
        self.tracked
            .state(focus, host)
            .set(VisualState::FOCUSED, host.focused() && focused)
            .set(VisualState::FOCUS_VISIBLE, host.focus_visible() && focused)
    }

    fn paint_trigger(&self, cx: &mut PaintContext<'_>, item: usize, rect: Rect) {
        let state = self
            .part_state(Focus::Trigger(item), cx.state)
            .set(VisualState::OPEN, self.open == Some(item))
            .set(VisualState::SELECTED, self.current == Some(item));
        let style = cx.style(Self::query(Slot::Trigger, state));
        let icon = cx.style(Self::query(Slot::Icon, state));
        let label = &self.items[item].0;
        let chevron = self.has_links(item).then_some(if self.open == Some(item) {
            Icon::ChevronUp
        } else {
            Icon::ChevronDown
        });
        cx.slot(rect, &style, |cx, inner| {
            let Some(chevron) = chevron else {
                cx.label(inner, label, &style, TextAlign::Center);
                return;
            };
            let skip = style.gap + icon.icon_size;
            let text_width = (inner.size.width - skip).max(0.0);
            let (text_x, icon_x) = if cx.rtl {
                (inner.max_x() - text_width, inner.origin.x)
            } else {
                (inner.origin.x, inner.max_x() - icon.icon_size)
            };
            let text = Rect::from_xywh(text_x, inner.origin.y, text_width, inner.size.height);
            cx.label(text, label, &style, TextAlign::Start);
            let mark = Rect::from_xywh(icon_x, inner.origin.y, icon.icon_size, inner.size.height);
            cx.icon(mark, chevron, &icon);
        });
    }
}

impl CustomControl for NavigationMenu {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let trigger = cx.style(Self::query(Slot::Trigger, VisualState::NONE));
        let icon = cx.style(Self::query(Slot::Icon, VisualState::NONE));
        let width: f64 = self
            .items
            .iter()
            .map(|(label, links)| {
                let chevron = if links.is_empty() {
                    0.0
                } else {
                    trigger.gap + icon.icon_size
                };
                cx.text.width(label, &trigger.text) + trigger.padding.horizontal() + chevron
            })
            .sum();
        let gaps = cx.tokens.spacing * self.items.len().saturating_sub(1) as f64;
        Measured {
            size: Size::new(width + gaps, trigger.min_height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let trigger = cx.style(Self::query(Slot::Trigger, VisualState::NONE));
        let icon = cx.style(Self::query(Slot::Icon, VisualState::NONE));
        let content = cx.style(Self::query(Slot::Content, VisualState::NONE));
        let link = cx.style(Self::query(Slot::Link, VisualState::NONE));
        let title = cx.style(Self::query(Slot::Title, VisualState::NONE));
        let description = cx.style(Self::query(Slot::Description, VisualState::NONE));
        self.rtl = cx.rtl();
        self.room = cx.popup_bounds();
        self.motion_ms = link.motion_ms;
        self.padding = content.padding;
        self.gap = cx.tokens.spacing;
        let spacing = cx.tokens.spacing;
        self.column = spacing * COLUMN_UNITS;
        self.offset = spacing * OFFSET_UNITS;
        let mut x = if self.rtl {
            bounds.max_x()
        } else {
            bounds.origin.x
        };
        self.triggers = self
            .items
            .iter()
            .map(|(label, links)| {
                let chevron = if links.is_empty() {
                    0.0
                } else {
                    trigger.gap + icon.icon_size
                };
                let width =
                    cx.text.width(label, &trigger.text) + trigger.padding.horizontal() + chevron;
                let left = if self.rtl { x - width } else { x };
                x = if self.rtl {
                    x - width - spacing
                } else {
                    x + width + spacing
                };
                Rect::from_xywh(left, bounds.origin.y, width, bounds.size.height)
            })
            .collect();
        // A link is its title, 4 pt, and its description, wrapped.
        let inner = self.column - link.padding.horizontal();
        self.link_heights = self
            .items
            .iter()
            .map(|(_, links)| {
                links
                    .iter()
                    .map(|entry| {
                        let text = wrapped_height(&entry.title, &title.text, inner, cx.text)
                            + spacing
                            + wrapped_height(&entry.description, &description.text, inner, cx.text);
                        text + link.padding.vertical()
                    })
                    .collect()
            })
            .collect();
        if let Some(item) = self.open {
            self.place_panel(item);
        }
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        if self.wheel(event) {
            cx.request_paint();
            return EventReply::Stop;
        }
        let pressed = self.tracked.pressed;
        let mut tracked = self.tracked;
        tracked.track(cx, event, |point| self.hit(point));
        self.tracked = tracked;
        match event {
            ControlEvent::Pointer(pointer) => self.pointer(cx, pointer, pressed),
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.key(cx, *key),
            ControlEvent::Hover(hovered) => {
                if *hovered {
                    self.close_at = None;
                } else {
                    self.pending = None;
                    self.quiet = None;
                    if self.open.is_some() {
                        self.close_at = Some(cx.time() + NAVIGATION_CLOSE_DELAY);
                    }
                }
                EventReply::Continue
            }
            ControlEvent::Timer => {
                if let Some((item, at)) = self.pending
                    && at <= cx.time()
                {
                    self.open_at(cx, item);
                } else if self.close_at.is_some_and(|at| at <= cx.time()) {
                    self.close(cx);
                }
                EventReply::Stop
            }
            ControlEvent::FocusChanged(true) => {
                if cx.modifiers().shift {
                    self.focus = Focus::Trigger(self.items.len().saturating_sub(1));
                } else {
                    self.focus = Focus::Trigger(0);
                }
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::Dismiss | ControlEvent::FocusChanged(false) => self.dismiss(cx),
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        for (item, rect) in self.triggers.iter().enumerate() {
            self.paint_trigger(cx, item, *rect);
        }
    }

    fn overlay(&self) -> Option<Rect> {
        self.open.map(|_| self.panel)
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        let progress = self.reveal.progress(cx.tokens, self.motion_ms, cx.time);
        let item = self.shown;
        if progress <= 0.0 || item >= self.items.len() {
            return;
        }
        let host = cx.state;
        let content = cx.style(Self::query(Slot::Content, VisualState::OPEN));
        let slide = (0.0, -cx.tokens.spacing * 2.0);
        let panel = self.panel;
        let spacing = cx.tokens.spacing;
        Reveal::paint(cx, panel, progress, slide, |cx| {
            cx.slot(panel, &content, |_, _| {});
            cx.list.push_clip(panel);
            for (index, (entry, rect)) in self.items[item].1.iter().zip(&self.links).enumerate() {
                let state = self.part_state(Focus::Link(index), host);
                let style = cx.style(Self::query(Slot::Link, state));
                let title = cx.style(Self::query(Slot::Title, state));
                let description = cx.style(Self::query(Slot::Description, state));
                cx.slot(*rect, &style, |cx, inner| {
                    let title_height =
                        wrapped_height(&entry.title, &title.text, inner.size.width, cx.text);
                    let title_rect = Rect::from_xywh(
                        inner.origin.x,
                        inner.origin.y,
                        inner.size.width,
                        title_height,
                    );
                    paint_wrapped(cx, title_rect, &entry.title, &title, TextAlign::Start);
                    let below = Rect::from_xywh(
                        inner.origin.x,
                        title_rect.max_y() + spacing,
                        inner.size.width,
                        (inner.max_y() - title_rect.max_y() - spacing).max(0.0),
                    );
                    paint_wrapped(
                        cx,
                        below,
                        &entry.description,
                        &description,
                        TextAlign::Start,
                    );
                });
            }
            let _ = cx.list.pop_clip();
        });
    }

    fn animating(&self, time: f64) -> bool {
        self.reveal.animating(self.motion_ms, time)
    }

    fn deadline(&self) -> Option<f64> {
        self.pending.map(|(_, at)| at).or(self.close_at)
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![
                SemanticAction::Activate,
                if self.open.is_some() {
                    SemanticAction::Collapse
                } else {
                    SemanticAction::Expand
                },
                SemanticAction::Focus,
            ]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Group, &self.name, actions);
        match self.focus {
            Focus::Trigger(item) => {
                node.value = self.items.get(item).map(|(label, _)| label.clone());
            }
            Focus::Link(link) => {
                if let Some(entry) = self.open.and_then(|item| self.items[item].1.get(link)) {
                    node.value = Some(entry.title.clone());
                    node.description = entry.description.clone();
                }
            }
        }
        node.state.expanded = Some(self.open.is_some());
        node.state.disabled = !self.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate => {
                self.activate(cx, self.focus);
                true
            }
            SemanticAction::Expand => {
                self.open_at(cx, self.expand_target());
                self.open.is_some()
            }
            SemanticAction::Collapse => {
                self.dismiss(cx);
                true
            }
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.open = None;
            self.reveal = Reveal::default();
            self.pending = None;
            self.close_at = None;
        }
    }

    /// Opens the panel of the focused trigger, as `Expand` does.
    fn set_open(&mut self, open: bool, time: f64) -> bool {
        if !open {
            self.hide(time);
            return (0..self.items.len()).any(|item| self.has_links(item));
        }
        self.enabled && self.show(self.expand_target(), time)
    }
}
