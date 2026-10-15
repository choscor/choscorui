//! The dock in a control host: tab stops, splitter and tab drags with
//! drop targets, floating panels, tab close buttons, and paint.

use super::{
    DockArea, DockArrangement, DockFocus, DockLayout, DockTarget, KEY_STEP, MIN_EXTENT, PanelId,
    PanelLocation, inside,
};
use crate::SplitOrientation;
use crate::hosted::Parts;
use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, TabsVariant, TextAlign, VisualState, fill_area,
};
use crate::{ControlContext, ControlEvent, ControlMessage, ControlValue};
use choscorui_core::{
    EventReply, Icon, Key, KeyEvent, Localizer, Measured, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticParts, SemanticRole, Size,
};

/// The side of the square close button in a tab.
const CLOSE: f64 = 16.0;
/// The inset of a panel text from the edges of the panel content.
const TEXT_INSET: f64 = 8.0;
/// How far the pointer moves before a press on a tab becomes a drag.
const DRAG_SLOP: f64 = 4.0;
/// The depth of the drop zone along each edge of the dock.
const EDGE: f64 = 24.0;
/// The smallest size of a panel that a drag makes float.
const FLOAT_MIN: Size = Size::new(160.0, 120.0);

/// A part of the dock that shows hover and press.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Part {
    Tab(PanelId),
    Close(PanelId),
    Splitter(DockArea),
}

/// A pointer drag in a control host.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// A splitter: its area, the pointer position along the drag axis at
    /// the press, and the area's extent then.
    Splitter(DockArea, f64, f64),
    /// A tab, or the title bar of a floating panel.
    Panel {
        panel: PanelId,
        /// The pointer at the press.
        start: Point,
        /// The offset of the pointer from the origin of the floating frame.
        grab: Point,
        /// The size of the panel when the drop makes it float.
        size: Size,
        /// Whether the pointer moved past the drag slop.
        moved: bool,
        /// The drop target under the pointer, with the rectangle that
        /// shows it.
        target: Option<(DockTarget, Rect)>,
    },
}

/// The dock's focus, drags, and geometry in a control host.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct HostState {
    /// The tab stop that has keyboard focus.
    focus: DockFocus,
    drag: Option<Drag>,
    /// The bounds and the direction from the last arrange.
    pub(super) bounds: Rect,
    rtl: bool,
    /// The extent of each area before a double click on its splitter
    /// collapsed it, by area index.
    collapsed: [Option<f64>; 4],
    /// Hover and press of the tabs, close buttons, and splitters.
    parts: Parts<Part>,
}

/// The title bar of a floating panel at `rect`.
fn title_bar(rect: Rect, tab_height: f64) -> Rect {
    Rect::new(
        rect.origin,
        Size::new(rect.size.width, tab_height.min(rect.size.height)),
    )
}

impl DockLayout {
    /// The close button of the tab of `panel` at `tab`: a square at the
    /// end of the tab, for a closable panel in a tab that has room.
    fn close_rect(&self, tab: Rect, panel: PanelId, rtl: bool) -> Option<Rect> {
        let closable = self.panel(panel).is_some_and(|panel| panel.closable);
        if !closable || tab.size.width < CLOSE * 3.0 || tab.size.height < CLOSE {
            return None;
        }
        let inset = (tab.size.height - CLOSE) / 2.0;
        let x = if rtl {
            tab.origin.x + inset
        } else {
            tab.max_x() - inset - CLOSE
        };
        Some(Rect::new(
            Point::new(x, tab.origin.y + inset),
            Size::new(CLOSE, CLOSE),
        ))
    }

    /// Each tab of `arrangement` with its rectangle: the docked tabs, then
    /// the title bar of each floating panel.
    fn tab_rects(&self, arrangement: &DockArrangement, rtl: bool) -> Vec<(PanelId, Rect)> {
        let docked = arrangement.groups.iter().flat_map(|group| {
            (0..group.tabs.len()).map(move |index| (group.tabs[index], group.tab_rect(index, rtl)))
        });
        let floating = arrangement
            .floating
            .iter()
            .map(|(panel, rect)| (*panel, title_bar(*rect, self.tab_height)));
        docked.chain(floating).collect()
    }

    /// The part under `point`. A floating panel covers the parts below
    /// it, and the last floating panel is on top.
    fn part_at(&self, point: Point) -> Option<Part> {
        let arrangement = self.arrange(self.hosted.bounds);
        let rtl = self.hosted.rtl;
        if let Some((panel, rect)) = arrangement
            .floating
            .iter()
            .rev()
            .find(|(_, rect)| rect.contains(point))
        {
            let bar = title_bar(*rect, self.tab_height);
            return match self.close_rect(bar, *panel, rtl) {
                Some(close) if close.contains(point) => Some(Part::Close(*panel)),
                _ => bar.contains(point).then_some(Part::Tab(*panel)),
            };
        }
        for (panel, tab) in self.tab_rects(&arrangement, rtl) {
            if tab.contains(point) {
                return match self.close_rect(tab, panel, rtl) {
                    Some(close) if close.contains(point) => Some(Part::Close(panel)),
                    _ => Some(Part::Tab(panel)),
                };
            }
        }
        arrangement
            .splitters
            .iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(area, _)| Part::Splitter(*area))
    }

    /// The tab stops of `arrangement` in reading order: the active tab of
    /// each group and each splitter, by their position, then each
    /// floating panel.
    fn stops(arrangement: &DockArrangement, rtl: bool) -> Vec<DockFocus> {
        let mut docked: Vec<(Rect, DockFocus)> = arrangement
            .groups
            .iter()
            .map(|group| (group.tab_bar, DockFocus::Tab(group.active)))
            .chain(
                arrangement
                    .splitters
                    .iter()
                    .map(|(area, rect)| (*rect, DockFocus::Splitter(*area))),
            )
            .collect();
        let line = |rect: &Rect| if rtl { -rect.max_x() } else { rect.origin.x };
        crate::order::sort_by(&mut docked, |(a, _), (b, _)| {
            a.origin
                .y
                .total_cmp(&b.origin.y)
                .then(line(a).total_cmp(&line(b)))
        });
        docked
            .into_iter()
            .map(|(_, stop)| stop)
            .chain(
                arrangement
                    .floating
                    .iter()
                    .map(|(panel, _)| DockFocus::Tab(*panel)),
            )
            .collect()
    }

    /// The focused stop in `bounds`, or the first one when the focused
    /// part is gone.
    fn host_focus(&self, bounds: Rect) -> DockFocus {
        let arrangement = self.arrange(bounds);
        let stops = Self::stops(&arrangement, self.hosted.rtl);
        let focus = match self.hosted.focus {
            DockFocus::Tab(panel) => match self.location(panel) {
                Some(PanelLocation::Floating(_)) => Some(DockFocus::Tab(panel)),
                _ => arrangement
                    .groups
                    .iter()
                    .find(|group| group.tabs.contains(&panel))
                    .map(|group| DockFocus::Tab(group.active)),
            },
            other => Some(other).filter(|focus| stops.contains(focus)),
        };
        focus.or_else(|| stops.first().copied()).unwrap_or_default()
    }

    /// Asks the host to lay out, paint, and publish the dock again.
    fn refresh(cx: &mut ControlContext<'_>) {
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
    }

    /// Selects `panel` and reports it when it was not active.
    fn host_activate(&mut self, cx: &mut ControlContext<'_>, panel: PanelId) {
        self.hosted.focus = DockFocus::Tab(panel);
        cx.request_paint();
        cx.request_semantics();
        if !self.is_active(panel) && self.activate(panel).is_ok() {
            cx.request_layout();
            cx.emit(ControlMessage::ItemActivated(panel.0 as usize));
        }
    }

    /// Closes `panel` when it is closable, moves focus to the tab that
    /// becomes active, and sends `Dismissed`.
    fn host_close(&mut self, cx: &mut ControlContext<'_>, panel: PanelId) -> bool {
        let Ok(next) = self.close(panel) else {
            return false;
        };
        self.hosted.focus = next.map_or(DockFocus::None, DockFocus::Tab);
        self.hosted.parts.clear();
        Self::refresh(cx);
        cx.emit(ControlMessage::Dismissed);
        true
    }

    /// Collapses `area` to the minimum extent, or restores the extent that
    /// it had before a collapse.
    fn toggle_collapse(&mut self, cx: &mut ControlContext<'_>, area: DockArea) {
        let current = self.extent(area);
        let saved = &mut self.hosted.collapsed[area.index()];
        let target = match saved.take() {
            Some(extent) if current <= MIN_EXTENT => extent,
            _ if current > MIN_EXTENT => {
                *saved = Some(current);
                MIN_EXTENT
            }
            _ => return,
        };
        self.host_resize(cx, area, target);
    }

    /// Sets the extent of `area` and reports a change.
    fn host_resize(&mut self, cx: &mut ControlContext<'_>, area: DockArea, extent: f64) {
        let before = self.extent(area);
        self.set_extent(area, extent);
        self.report_extent(cx, area, before);
    }

    /// Sends the extent that the dock shows when it is no longer `before`.
    fn report_extent(&mut self, cx: &mut ControlContext<'_>, area: DockArea, before: f64) {
        if self.extent(area) != before {
            Self::refresh(cx);
            cx.emit(ControlMessage::Changed(ControlValue::Number(
                self.shown_extent(area),
            )));
        }
    }

    /// Ends a drag without a change: a splitter gets its extent from the
    /// press back, and a tab stays where it is.
    fn cancel_drag(&mut self, cx: &mut ControlContext<'_>) -> bool {
        let Some(drag) = self.hosted.drag.take() else {
            return false;
        };
        cx.capture_pointer(false);
        if let Drag::Splitter(area, _, extent) = drag {
            self.host_resize(cx, area, extent);
        }
        cx.request_paint();
        true
    }

    fn host_key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        if key == Key::Escape {
            return if self.cancel_drag(cx) {
                EventReply::Stop
            } else {
                EventReply::Continue
            };
        }
        let arrangement = self.arrange(cx.bounds());
        let focus = self.host_focus(cx.bounds());
        let modifiers = cx.modifiers();
        if key == Key::Character('w') && modifiers.command && !modifiers.option {
            return match focus {
                DockFocus::Tab(panel) if self.host_close(cx, panel) => EventReply::Stop,
                _ => EventReply::Continue,
            };
        }
        if key == Key::Tab {
            let stops = Self::stops(&arrangement, self.hosted.rtl);
            let current = stops.iter().position(|stop| *stop == focus).unwrap_or(0);
            return match crate::hosted::tab_stop(current, stops.len(), modifiers.shift) {
                Some(next) => {
                    self.hosted.focus = stops[next];
                    cx.request_paint();
                    cx.request_semantics();
                    EventReply::Stop
                }
                None => EventReply::Continue,
            };
        }
        match focus {
            DockFocus::Tab(panel) => {
                let Some(group) = arrangement
                    .groups
                    .iter()
                    .find(|group| group.tabs.contains(&panel))
                else {
                    return EventReply::Continue;
                };
                let current = group.tabs.iter().position(|tab| *tab == panel).unwrap_or(0);
                let rtl = self.hosted.rtl;
                match crate::hosted::rove(key, current, group.tabs.len(), true, rtl) {
                    Some(next) => {
                        self.host_activate(cx, group.tabs[next]);
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                }
            }
            DockFocus::Splitter(area) => {
                let before = self.extent(area);
                if self.handle_splitter_key(area, key) {
                    self.report_extent(cx, area, before);
                    EventReply::Stop
                } else {
                    EventReply::Continue
                }
            }
            DockFocus::None => EventReply::Continue,
        }
    }

    /// Where a drop of `panel` at `point` puts it, with the rectangle
    /// that shows the target. An edge zone of the dock starts a new group
    /// in that area, a group adds a tab, and any other place makes the
    /// panel float with its frame at `point - grab`.
    fn drop_target(
        &self,
        panel: PanelId,
        point: Point,
        grab: Point,
        size: Size,
    ) -> Option<(DockTarget, Rect)> {
        let bounds = self.hosted.bounds;
        let arrangement = self.arrange(bounds);
        let edges = [
            (DockArea::Left, point.x - bounds.origin.x),
            (DockArea::Right, bounds.max_x() - point.x),
            (DockArea::Top, point.y - bounds.origin.y),
            (DockArea::Bottom, bounds.max_y() - point.y),
        ];
        let (area, distance) = edges
            .into_iter()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((DockArea::Left, f64::INFINITY));
        if (0.0..EDGE).contains(&distance) {
            let groups = self.areas[area.index()].groups.len();
            let depth = if groups == 0 {
                self.extent(area).min(MIN_EXTENT * 2.0)
            } else {
                self.extents(bounds)[area.index()]
            };
            let band = super::split_edge(bounds, area, depth).0;
            let target = DockTarget::NewGroup {
                area,
                index: groups,
            };
            return Some((target, band));
        }
        if let Some(group) = arrangement
            .groups
            .iter()
            .find(|group| group.rect.contains(point))
        {
            // Dropping the only tab of a group on its own group does nothing.
            let alone = group.tabs == [panel];
            let target = DockTarget::Tab {
                area: group.area,
                group: group.group,
            };
            return (!alone).then_some((target, group.rect));
        }
        if !self.panel(panel).is_some_and(|panel| panel.floatable) {
            return None;
        }
        let relative = Rect::new(
            Point::new(
                point.x - grab.x - bounds.origin.x,
                point.y - grab.y - bounds.origin.y,
            ),
            size,
        );
        let shown = inside(relative, bounds);
        let stored = Rect::new(
            Point::new(
                shown.origin.x - bounds.origin.x,
                shown.origin.y - bounds.origin.y,
            ),
            shown.size,
        );
        Some((DockTarget::Floating(stored), shown))
    }

    /// Starts a drag of `panel` from a press at `position` on a tab at
    /// `tab`, or on the title bar of a floating panel at `frame`.
    fn start_panel_drag(
        &mut self,
        cx: &mut ControlContext<'_>,
        panel: PanelId,
        position: Point,
        frame: Rect,
    ) {
        let bounds = self.hosted.bounds;
        let size = Size::new(
            frame.size.width.max(FLOAT_MIN.width).min(bounds.size.width),
            frame
                .size
                .height
                .max(FLOAT_MIN.height)
                .min(bounds.size.height),
        );
        self.hosted.drag = Some(Drag::Panel {
            panel,
            start: position,
            grab: Point::new(position.x - frame.origin.x, position.y - frame.origin.y),
            size,
            moved: false,
            target: None,
        });
        cx.capture_pointer(true);
    }

    fn host_press(&mut self, cx: &mut ControlContext<'_>, position: Point) -> EventReply {
        let arrangement = self.arrange(cx.bounds());
        let rtl = self.hosted.rtl;
        if let Some(index) = arrangement
            .floating
            .iter()
            .rposition(|(_, rect)| rect.contains(position))
        {
            let (panel, rect) = arrangement.floating[index];
            // A press raises the floating panel above the others.
            if let Some(stored) = self.floating.iter().position(|(id, _)| *id == panel) {
                let entry = self.floating.remove(stored);
                self.floating.push(entry);
            }
            self.host_activate(cx, panel);
            let bar = title_bar(rect, self.tab_height);
            let on_close = self
                .close_rect(bar, panel, rtl)
                .is_some_and(|close| close.contains(position));
            if bar.contains(position) && !on_close {
                self.start_panel_drag(cx, panel, position, rect);
            }
            return EventReply::Stop;
        }
        for group in &arrangement.groups {
            let tab =
                (0..group.tabs.len()).find(|index| group.tab_rect(*index, rtl).contains(position));
            if let Some(index) = tab {
                let (panel, tab) = (group.tabs[index], group.tab_rect(index, rtl));
                let on_close = self
                    .close_rect(tab, panel, rtl)
                    .is_some_and(|close| close.contains(position));
                if !on_close {
                    self.host_activate(cx, panel);
                    self.start_panel_drag(cx, panel, position, group.rect);
                }
                return EventReply::Stop;
            }
        }
        let splitter = arrangement
            .splitters
            .iter()
            .find(|(_, rect)| rect.contains(position));
        let Some((area, _)) = splitter else {
            return EventReply::Continue;
        };
        self.hosted.focus = DockFocus::Splitter(*area);
        if cx.click_count() >= 2 {
            self.toggle_collapse(cx, *area);
            return EventReply::Stop;
        }
        let along = if area.horizontal() {
            position.x
        } else {
            position.y
        };
        self.hosted.drag = Some(Drag::Splitter(*area, along, self.extent(*area)));
        cx.capture_pointer(true);
        cx.request_paint();
        cx.request_semantics();
        EventReply::Stop
    }

    fn host_move(&mut self, cx: &mut ControlContext<'_>, position: Point) -> EventReply {
        match self.hosted.drag {
            Some(Drag::Splitter(area, start, extent)) => {
                let delta = if area.horizontal() {
                    position.x
                } else {
                    position.y
                } - start;
                let grow = match area {
                    DockArea::Left | DockArea::Top => delta,
                    DockArea::Right | DockArea::Bottom => -delta,
                };
                self.host_resize(cx, area, extent + grow);
                EventReply::Stop
            }
            Some(Drag::Panel {
                panel,
                start,
                grab,
                size,
                moved,
                ..
            }) => {
                let far = (position.x - start.x).hypot(position.y - start.y) >= DRAG_SLOP;
                if moved || far {
                    self.hosted.drag = Some(Drag::Panel {
                        panel,
                        start,
                        grab,
                        size,
                        moved: true,
                        target: self.drop_target(panel, position, grab, size),
                    });
                    cx.request_paint();
                }
                EventReply::Stop
            }
            None => EventReply::Continue,
        }
    }

    fn host_release(
        &mut self,
        cx: &mut ControlContext<'_>,
        position: Point,
        pressed: Option<Part>,
    ) -> EventReply {
        if let Some(drag) = self.hosted.drag.take() {
            cx.capture_pointer(false);
            cx.request_paint();
            if let Drag::Panel {
                panel,
                moved: true,
                target: Some((target, _)),
                ..
            } = drag
                && self.dock(panel, target).is_ok()
            {
                self.hosted.focus = DockFocus::Tab(panel);
                Self::refresh(cx);
                cx.emit(ControlMessage::Changed(ControlValue::Text(self.save())));
            }
            return EventReply::Stop;
        }
        match pressed {
            Some(Part::Close(panel)) if self.part_at(position) == Some(Part::Close(panel)) => {
                self.host_close(cx, panel);
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    /// Paints the groups, splitters, and floating panels of `arrangement`.
    /// Each group gets the `Dock` frame and a line-variant `Tabs` bar; the
    /// active tab is underlined, and a closable panel's tab has a close
    /// button. Splitters use the `Resizable` handle. A floating panel gets
    /// the `Dock` `Panel` frame with its title as a tab. While a tab drag
    /// is over a drop target, the `Dock` `Indicator` shows the target.
    /// The host paints the center and the panel contents.
    pub fn paint(
        &self,
        cx: &mut PaintContext<'_>,
        arrangement: &DockArrangement,
        focus: DockFocus,
    ) {
        self.paint_parts(cx, arrangement, focus, &mut |_, _, _| {});
    }

    /// Paints the dock as a control host does, and calls `content` with
    /// the content rectangle of each shown panel: the active tab of each
    /// group, then each floating panel just after its frame. Thus a
    /// floating panel covers the docked content below it.
    pub fn paint_hosted(
        &self,
        cx: &mut PaintContext<'_>,
        content: &mut dyn FnMut(&mut PaintContext<'_>, PanelId, Rect),
    ) {
        let arrangement = self.arrange(cx.bounds);
        let focus = if cx.state.focus_visible() {
            self.host_focus(cx.bounds)
        } else {
            DockFocus::None
        };
        self.paint_parts(cx, &arrangement, focus, content);
    }

    fn paint_parts(
        &self,
        cx: &mut PaintContext<'_>,
        arrangement: &DockArrangement,
        focus: DockFocus,
        content: &mut dyn FnMut(&mut PaintContext<'_>, PanelId, Rect),
    ) {
        let frame = cx.style(StyleQuery::new(ControlKind::Dock, Slot::Root));
        for group in &arrangement.groups {
            cx.slot(group.rect, &frame, |_, _| {});
            self.paint_bar_line(cx, group.tab_bar);
            for (index, panel) in group.tabs.iter().enumerate() {
                let rect = group.tab_rect(index, cx.rtl);
                self.paint_tab(cx, rect, *panel, *panel == group.active, focus);
            }
            content(cx, group.active, group.content);
        }
        for (area, rect) in &arrangement.splitters {
            let part = DockFocus::Splitter(*area);
            let state = self
                .hosted
                .parts
                .state(Part::Splitter(*area), cx.state)
                .set(VisualState::FOCUS_VISIBLE, focus == part);
            let orientation = if area.horizontal() {
                SplitOrientation::Horizontal
            } else {
                SplitOrientation::Vertical
            };
            crate::containers::paint_divider(cx, *rect, orientation, false, state);
        }
        let panel = cx.style(StyleQuery::new(ControlKind::Dock, Slot::Panel));
        for (id, rect) in &arrangement.floating {
            cx.slot(*rect, &panel, |_, _| {});
            let bar = title_bar(*rect, self.tab_height);
            self.paint_bar_line(cx, bar);
            self.paint_tab(cx, bar, *id, true, focus);
            content(cx, *id, self.floating_content(*rect));
        }
        if let Some(Drag::Panel {
            target: Some((_, rect)),
            ..
        }) = self.hosted.drag
        {
            let indicator = cx.style(StyleQuery::new(ControlKind::Dock, Slot::Indicator));
            cx.slot(rect, &indicator, |_, _| {});
        }
    }

    /// Paints the line under a tab bar.
    fn paint_bar_line(&self, cx: &mut PaintContext<'_>, bar: Rect) {
        let border = cx
            .style(StyleQuery::new(ControlKind::Separator, Slot::Root))
            .background;
        let line = Rect::new(
            Point::new(bar.origin.x, bar.max_y() - 1.0),
            Size::new(bar.size.width, 1.0),
        );
        fill_area(cx.list, line, cx.rtl, &border);
    }

    /// Paints the tab of `panel` at `rect`, with its close button.
    fn paint_tab(
        &self,
        cx: &mut PaintContext<'_>,
        rect: Rect,
        panel: PanelId,
        active: bool,
        focus: DockFocus,
    ) {
        let (parts, host) = (self.hosted.parts, cx.state);
        let query = |slot, state| {
            StyleQuery::new(ControlKind::Tabs, slot)
                .variant(TabsVariant::Line)
                .state(state)
        };
        let state = parts
            .state(Part::Tab(panel), host)
            .set(VisualState::SELECTED, active)
            .set(VisualState::FOCUS_VISIBLE, focus == DockFocus::Tab(panel));
        let trigger = cx.style(query(Slot::Trigger, state));
        let title = self.panel(panel).map_or("", |panel| panel.title.as_str());
        let close = self.close_rect(rect, panel, cx.rtl);
        cx.slot(rect, &trigger, |cx, inner| {
            let mut label = inner;
            if close.is_some() {
                // The title keeps clear of the close button.
                let room = CLOSE + (rect.size.height - CLOSE) / 2.0;
                label.size.width = (label.size.width - room).max(0.0);
                if cx.rtl {
                    label.origin.x = inner.max_x() - label.size.width;
                }
            }
            cx.label(label, title, &trigger, TextAlign::Center);
        });
        let indicator = cx.style(query(Slot::Indicator, state));
        let line = Rect::new(
            Point::new(rect.origin.x, rect.max_y() - indicator.min_height),
            Size::new(rect.size.width, indicator.min_height),
        );
        fill_area(cx.list, line, cx.rtl, &indicator.background);
        if let Some(close) = close {
            let state = parts.state(Part::Close(panel), host);
            let style = cx.style(StyleQuery::new(ControlKind::Dock, Slot::Close).state(state));
            cx.slot(close, &style, |cx, inner| cx.icon(inner, Icon::X, &style));
        }
    }
}

/// In a control host the dock's node covers its whole area. It paints the
/// group frames, the tab bars, the splitters, and the floating panel
/// frames, and the view layer places the center and the panel contents in
/// the frames that [`DockLayout::arrange`] returns for the node's bounds.
impl crate::CustomControl for DockLayout {
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> Measured {
        let max = cx.constraints.max;
        let size = Size::new(
            if max.width.is_finite() {
                max.width
            } else {
                cx.tokens.spacing * 120.0
            },
            if max.height.is_finite() {
                max.height
            } else {
                cx.tokens.spacing * 96.0
            },
        );
        Measured::new(cx.constraints.constrain(size))
    }

    fn arrange(&mut self, cx: &crate::MeasureContext<'_>, bounds: Rect) {
        self.hosted.bounds = bounds;
        self.hosted.rtl = cx.rtl();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let mut parts = self.hosted.parts;
        let pressed = parts.pressed;
        parts.track(cx, event, |point| self.part_at(point));
        self.hosted.parts = parts;
        match crate::hosted::route(cx, event) {
            Some(crate::hosted::Routed::Pointer(event)) => match event {
                PointerEvent::Down {
                    position,
                    button: PointerButton::Primary,
                } => self.host_press(cx, position),
                PointerEvent::Move { position } => self.host_move(cx, position),
                PointerEvent::Up { position, .. } => self.host_release(cx, position, pressed),
                _ => EventReply::Continue,
            },
            Some(crate::hosted::Routed::Key(KeyEvent::Down { key, .. })) => self.host_key(cx, key),
            Some(crate::hosted::Routed::Cancel) => {
                self.cancel_drag(cx);
                cx.request_paint();
                EventReply::Continue
            }
            _ => {
                if let ControlEvent::FocusChanged(_) = event {
                    cx.request_paint();
                }
                EventReply::Continue
            }
        }
    }

    /// Paints the dock, the center text in the middle of the center, and
    /// the text of each shown panel at the top of its content, with the
    /// `Card` description look.
    fn paint(&self, cx: &mut PaintContext<'_>) {
        let style = cx.style(StyleQuery::new(ControlKind::Card, Slot::Description));
        if !self.center_text.is_empty() {
            let center = self.arrange(cx.bounds).center;
            cx.label(center, &self.center_text, &style, TextAlign::Center);
        }
        self.paint_hosted(cx, &mut |cx, panel, content| {
            let Some(text) = self.panel(panel).map(|panel| panel.text.as_str()) else {
                return;
            };
            if text.is_empty() {
                return;
            }
            let inner = content.inset(TEXT_INSET);
            let line = Rect::new(
                inner.origin,
                Size::new(inner.size.width, style.text.line_height),
            );
            cx.label(line, text, &style, TextAlign::Start);
        });
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    /// A `Group` with the actions of the focused part.
    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        let mut actions = match self.host_focus(self.hosted.bounds) {
            DockFocus::Tab(panel) => self
                .panel_semantics(panel, strings)
                .map(|node| node.actions)
                .unwrap_or_default(),
            DockFocus::Splitter(area) => self.splitter_semantics(area, strings).actions,
            DockFocus::None => Vec::new(),
        };
        actions.retain(|action| *action != SemanticAction::Focus);
        actions.push(SemanticAction::Focus);
        crate::hosted::node(SemanticRole::Group, "", actions)
    }

    /// A `Tab` part for each tab and floating title bar, selected while its
    /// panel is active, and a `Slider` part for each splitter with its
    /// range. The focused tab or splitter is the focused part.
    fn semantic_parts(&self, strings: &Localizer, parts: &mut SemanticParts) {
        let arrangement = self.arrange(self.hosted.bounds);
        let focus = self.host_focus(self.hosted.bounds);
        for (panel, rect) in self.tab_rects(&arrangement, self.hosted.rtl) {
            let Some(mut node) = self.panel_semantics(panel, strings) else {
                continue;
            };
            node.role = SemanticRole::Tab;
            node.actions[0] = SemanticAction::Activate;
            parts.push(None, SemanticRole::Tab, "", rect).node = node;
            if focus == DockFocus::Tab(panel) {
                parts.set_focused(Some(parts.len() - 1));
            }
        }
        for (area, rect) in arrangement.splitters {
            let node = self.splitter_semantics(area, strings);
            parts.push(None, SemanticRole::Slider, "", rect).node = node;
            if focus == DockFocus::Splitter(area) {
                parts.set_focused(Some(parts.len() - 1));
            }
        }
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match (action, self.host_focus(cx.bounds())) {
            (SemanticAction::Focus, _) => cx.request_focus(),
            (SemanticAction::Increment, DockFocus::Splitter(area)) => {
                self.host_resize(cx, area, self.extent(area) + KEY_STEP);
            }
            (SemanticAction::Decrement, DockFocus::Splitter(area)) => {
                self.host_resize(cx, area, self.extent(area) - KEY_STEP);
            }
            (SemanticAction::Dismiss, DockFocus::Tab(panel)) => return self.host_close(cx, panel),
            _ => return false,
        }
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.hosted.drag = None;
        }
    }

    /// Takes the saved text of an arrangement, as [`DockLayout::save`]
    /// gives it. Text that does not restore keeps the arrangement.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Text(saved) = value else {
            return false;
        };
        *saved == self.save() || self.restore(saved).is_ok()
    }
}
