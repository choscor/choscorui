//! A sidebar of grouped navigation items that collapses to icons or off
//! the canvas, and becomes a sheet in a narrow window.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::{self, Reveal};
use crate::style::{
    ButtonVariant, ControlKind, ControlSize, PaintContext, SheetSide, SidebarVariant, Slot,
    StyleQuery, TextAlign, VisualState,
};
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Measured, Point, PointerButton, PointerEvent,
    Rect, SemanticAction, SemanticNode, SemanticRole, Size, Transform,
};

/// Below this window width the sidebar becomes a sheet, in points.
pub const SIDEBAR_MOBILE_BREAKPOINT: f64 = 768.0;

/// The width of the sheet in a narrow window, in points.
const MOBILE_WIDTH: f64 = 288.0;

/// How a sidebar collapses.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SidebarCollapse {
    /// The sidebar slides off; only its toggle stays.
    #[default]
    Offcanvas,
    /// The sidebar narrows to its item icons.
    Icon,
    /// The sidebar does not collapse and has no toggle.
    None,
}

/// One navigation item of a [`Sidebar`].
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarItem {
    pub label: String,
    pub icon: Icon,
    /// Short text at the end of the row, such as a count.
    pub badge: Option<String>,
}

impl SidebarItem {
    pub fn new(label: impl Into<String>, icon: Icon) -> Self {
        Self {
            label: label.into(),
            icon,
            badge: None,
        }
    }

    #[must_use]
    pub fn with_badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Toggle,
    Label(usize),
    /// An item by its index across all groups.
    Item(usize),
}

impl Part {
    fn focusable(self) -> bool {
        !matches!(self, Part::Label(_))
    }
}

/// A sidebar: a toggle and groups of items with icons. The selected item
/// takes the accent look. Command+B, the toggle, `Expand`, and `Collapse`
/// open or collapse it. In a window narrower than
/// [`SIDEBAR_MOBILE_BREAKPOINT`], only the toggle shows, and opening the
/// sidebar shows it as a modal sheet.
pub struct Sidebar {
    name: String,
    toggle: String,
    groups: Vec<(String, Vec<SidebarItem>)>,
    variant: SidebarVariant,
    collapse: SidebarCollapse,
    side: SheetSide,
    expanded: bool,
    selected: Option<usize>,
    /// The focused part, as an index into the active parts.
    focus: usize,
    hovered: Option<usize>,
    pressed: Option<usize>,
    mobile: bool,
    parts: Vec<(Part, Rect)>,
    sheet_parts: Vec<(Part, Rect)>,
    viewport: Rect,
    sheet: Rect,
    reveal: Reveal,
    motion_ms: u32,
    enabled: bool,
}

impl Sidebar {
    /// An expanded sidebar. `toggle` names the toggle button.
    pub fn new(
        name: impl Into<String>,
        toggle: impl Into<String>,
        groups: Vec<(String, Vec<SidebarItem>)>,
    ) -> Self {
        Self {
            name: name.into(),
            toggle: toggle.into(),
            groups,
            variant: SidebarVariant::Sidebar,
            collapse: SidebarCollapse::Offcanvas,
            side: SheetSide::Left,
            expanded: true,
            selected: None,
            focus: 0,
            hovered: None,
            pressed: None,
            mobile: false,
            parts: Vec::new(),
            sheet_parts: Vec::new(),
            viewport: Rect::default(),
            sheet: Rect::default(),
            reveal: Reveal::default(),
            motion_ms: 0,
            enabled: true,
        }
    }

    #[must_use]
    pub fn with_variant(mut self, variant: SidebarVariant) -> Self {
        self.variant = variant;
        self
    }

    #[must_use]
    pub fn with_collapse(mut self, collapse: SidebarCollapse) -> Self {
        self.collapse = collapse;
        self
    }

    /// The window edge the sidebar sits at: `Left` or `Right`.
    #[must_use]
    pub fn with_side(mut self, side: SheetSide) -> Self {
        self.side = if side == SheetSide::Right {
            SheetSide::Right
        } else {
            SheetSide::Left
        };
        self
    }

    #[must_use]
    pub fn with_selected(mut self, item: usize) -> Self {
        self.selected = Some(item);
        self
    }

    pub fn is_expanded(&self) -> bool {
        if self.mobile {
            self.reveal.open
        } else {
            self.expanded || self.collapse == SidebarCollapse::None
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    fn compact(&self) -> bool {
        self.mobile || !self.is_expanded()
    }

    fn size(&self, compact: bool) -> ControlSize {
        if compact {
            ControlSize::Icon
        } else {
            ControlSize::Default
        }
    }

    fn query(&self, slot: Slot, size: ControlSize, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Sidebar, slot)
            .variant(self.variant)
            .size(size)
            .state(state)
    }

    fn toggle_query(state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Button, Slot::Root)
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::IconSm)
            .state(state)
    }

    fn items(&self) -> impl Iterator<Item = (usize, &SidebarItem)> {
        self.groups.iter().flat_map(|(_, items)| items).enumerate()
    }

    fn item(&self, index: usize) -> Option<&SidebarItem> {
        self.items().nth(index).map(|(_, item)| item)
    }

    /// The parts that input and paint use now.
    fn active(&self) -> &[(Part, Rect)] {
        if self.mobile && self.reveal.open {
            &self.sheet_parts
        } else {
            &self.parts
        }
    }

    fn stops(&self) -> Vec<usize> {
        self.active()
            .iter()
            .enumerate()
            .filter(|(_, (part, _))| part.focusable())
            .map(|(index, _)| index)
            .collect()
    }

    fn part_at(&self, point: Point) -> Option<usize> {
        self.active()
            .iter()
            .position(|(part, rect)| part.focusable() && rect.contains(point))
    }

    /// Lays out the toggle, the group labels, and the items in `panel`.
    /// A compact layout shows the toggle and, when collapsing to icons,
    /// the item icons.
    fn layout(&self, cx: &MeasureContext<'_>, panel: Rect, compact: bool) -> Vec<(Part, Rect)> {
        let size = self.size(compact);
        let root = cx.style(self.query(Slot::Root, size, VisualState::NONE));
        let header = cx.style(self.query(Slot::Header, size, VisualState::NONE));
        let group = cx.style(self.query(Slot::Group, size, VisualState::NONE));
        let label = cx.style(self.query(Slot::GroupLabel, size, VisualState::NONE));
        let item = cx.style(self.query(Slot::Item, size, VisualState::NONE));
        let toggle = cx.style(Self::toggle_query(VisualState::NONE));
        let inner = Rect::from_xywh(
            panel.origin.x + root.padding.inline_start,
            panel.origin.y + root.padding.block_start,
            panel.size.width - root.padding.horizontal(),
            panel.size.height - root.padding.vertical(),
        );
        let mut parts = Vec::new();
        let mut y = inner.origin.y;
        if self.collapse != SidebarCollapse::None || self.mobile {
            let side = toggle.min_width.max(toggle.min_height);
            let x = if cx.rtl() {
                inner.max_x() - header.padding.inline_start - side
            } else {
                inner.origin.x + header.padding.inline_start
            };
            parts.push((
                Part::Toggle,
                Rect::from_xywh(x, y + header.padding.block_start, side, side),
            ));
            y += side + header.padding.vertical();
        }
        let icons = !compact || (self.collapse == SidebarCollapse::Icon && !self.mobile);
        if !icons {
            return parts;
        }
        let mut index = 0;
        for (group_index, (title, entries)) in self.groups.iter().enumerate() {
            y += group.padding.block_start;
            let x = inner.origin.x + group.padding.inline_start;
            let width = inner.size.width - group.padding.horizontal();
            if !compact && !title.is_empty() {
                parts.push((
                    Part::Label(group_index),
                    Rect::from_xywh(x, y, width, label.min_height),
                ));
                y += label.min_height;
            }
            for _ in entries {
                let row_width = if compact { item.min_width } else { width };
                parts.push((
                    Part::Item(index),
                    Rect::from_xywh(x, y, row_width, item.min_height),
                ));
                y += item.min_height + group.gap;
                index += 1;
            }
            // The last item has no gap after it.
            let trailing = if entries.is_empty() { 0.0 } else { group.gap };
            y += group.padding.block_end - trailing;
        }
        parts
    }

    fn set_expanded(&mut self, cx: &mut ControlContext<'_>, expanded: bool) {
        if self.mobile {
            if self.reveal.open == expanded {
                return;
            }
            self.reveal.set(expanded, cx.time());
            self.focus = 0;
        } else {
            if self.collapse == SidebarCollapse::None || self.expanded == expanded {
                return;
            }
            self.expanded = expanded;
            self.focus = 0;
            cx.request_layout();
        }
        cx.emit(ControlMessage::OpenChanged(expanded));
        cx.request_paint();
        cx.request_semantics();
    }

    fn activate(&mut self, cx: &mut ControlContext<'_>, part: usize) {
        match self.active().get(part).map(|(part, _)| *part) {
            Some(Part::Toggle) => {
                let expanded = !self.is_expanded();
                self.set_expanded(cx, expanded);
            }
            Some(Part::Item(index)) => {
                if self.selected != Some(index) {
                    self.selected = Some(index);
                    cx.emit(ControlMessage::Changed(ControlValue::Index(Some(index))));
                }
                if self.mobile {
                    self.set_expanded(cx, false);
                }
                cx.request_paint();
                cx.request_semantics();
            }
            _ => {}
        }
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        let modifiers = cx.modifiers();
        let stops = self.stops();
        match key {
            Key::Character('b') if modifiers.command => {
                let expanded = !self.is_expanded();
                self.set_expanded(cx, expanded);
            }
            Key::Escape if self.mobile && self.reveal.open => {
                self.set_expanded(cx, false);
                cx.emit(ControlMessage::Dismissed);
            }
            Key::Enter | Key::Space => {
                if let Some(part) = stops.get(self.focus) {
                    self.activate(cx, *part);
                }
            }
            Key::Tab if self.mobile && self.reveal.open && !stops.is_empty() => {
                // Focus stays in the sheet.
                let count = stops.len();
                self.focus = if modifiers.shift {
                    (self.focus + count - 1) % count
                } else {
                    (self.focus + 1) % count
                };
                cx.request_paint();
                cx.request_semantics();
            }
            key => match hosted::rove(key, self.focus, stops.len(), false, false) {
                Some(next) => {
                    self.focus = next;
                    cx.request_paint();
                    cx.request_semantics();
                }
                None => return EventReply::Continue,
            },
        }
        EventReply::Stop
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, event: &PointerEvent) -> EventReply {
        let position = event.position();
        match *event {
            PointerEvent::Down {
                button: PointerButton::Primary,
                ..
            } => {
                if self.mobile && self.reveal.open && !self.sheet.contains(position) {
                    self.set_expanded(cx, false);
                    cx.emit(ControlMessage::Dismissed);
                    return EventReply::Stop;
                }
                let Some(part) = self.part_at(position) else {
                    return EventReply::Continue;
                };
                self.pressed = Some(part);
                if let Some(stop) = self.stops().iter().position(|stop| *stop == part) {
                    self.focus = stop;
                }
                cx.request_focus();
                cx.capture_pointer(true);
                cx.request_paint();
                EventReply::Stop
            }
            PointerEvent::Up { .. } if self.pressed.is_some() => {
                cx.capture_pointer(false);
                let hit = self.part_at(position);
                if let Some(part) = self.pressed.take().filter(|part| hit == Some(*part)) {
                    self.activate(cx, part);
                }
                cx.request_paint();
                EventReply::Stop
            }
            PointerEvent::Move { .. } => {
                let hovered = self.part_at(position);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_paint();
                }
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint_parts(&self, cx: &mut PaintContext<'_>, parts: &[(Part, Rect)], compact: bool) {
        let host = cx.state;
        let size = self.size(compact);
        let stops: Vec<usize> = parts
            .iter()
            .enumerate()
            .filter(|(_, (part, _))| part.focusable())
            .map(|(index, _)| index)
            .collect();
        let focused = stops.get(self.focus).copied();
        for (index, (part, rect)) in parts.iter().enumerate() {
            let state = VisualState::NONE
                .set(VisualState::HOVERED, self.hovered == Some(index))
                .set(VisualState::PRESSED, self.pressed == Some(index))
                .set(
                    VisualState::FOCUS_VISIBLE,
                    host.focus_visible() && focused == Some(index),
                )
                .set(VisualState::DISABLED, host.disabled());
            match part {
                Part::Toggle => {
                    let style = cx.style(Self::toggle_query(state));
                    cx.slot(*rect, &style, |cx, inner| {
                        cx.icon(inner, Icon::PanelLeft, &style);
                    });
                }
                Part::Label(group) => {
                    let style = cx.style(self.query(Slot::GroupLabel, size, state));
                    let title = &self.groups[*group].0;
                    cx.slot(*rect, &style, |cx, inner| {
                        cx.label(inner, title, &style, TextAlign::Start);
                    });
                }
                Part::Item(item_index) => {
                    let Some(item) = self.item(*item_index) else {
                        continue;
                    };
                    let state =
                        state.set(VisualState::SELECTED, self.selected == Some(*item_index));
                    let style = cx.style(self.query(Slot::Item, size, state));
                    let badge = cx.style(self.query(Slot::Badge, size, state));
                    cx.slot(*rect, &style, |cx, inner| {
                        let icon_size = 16.0;
                        let icon_x = if compact {
                            inner.center().x - icon_size / 2.0
                        } else if cx.rtl {
                            inner.max_x() - icon_size
                        } else {
                            inner.origin.x
                        };
                        let mark =
                            Rect::from_xywh(icon_x, inner.origin.y, icon_size, inner.size.height);
                        cx.icon(mark, item.icon, &style);
                        if compact {
                            return;
                        }
                        let skip = icon_size + style.gap;
                        let text = Rect::from_xywh(
                            if cx.rtl {
                                inner.origin.x
                            } else {
                                inner.origin.x + skip
                            },
                            inner.origin.y,
                            (inner.size.width - skip).max(0.0),
                            inner.size.height,
                        );
                        if let Some(count) = &item.badge {
                            cx.label(text, count, &badge, TextAlign::End);
                        }
                        cx.label(text, &item.label, &style, TextAlign::Start);
                    });
                }
            }
        }
    }

    /// Paints the sidebar box with its border on the inner edge.
    fn paint_root(&self, cx: &mut PaintContext<'_>, rect: Rect, compact: bool) {
        let style = cx.style(self.query(Slot::Root, self.size(compact), VisualState::NONE));
        // The recipe puts the border at the inline end. A right sidebar
        // needs it on the left, which is the inline end right to left.
        let rtl = cx.rtl;
        cx.rtl = self.side == SheetSide::Right;
        cx.slot(rect, &style, |_, _| {});
        cx.rtl = rtl;
    }
}

impl CustomControl for Sidebar {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let compact = self.compact();
        let root = cx.style(self.query(Slot::Root, self.size(compact), VisualState::NONE));
        let toggle = cx.style(Self::toggle_query(VisualState::NONE));
        let header = cx.style(self.query(Slot::Header, ControlSize::Default, VisualState::NONE));
        let width = if self.mobile
            || (!self.is_expanded() && self.collapse == SidebarCollapse::Offcanvas)
        {
            toggle.min_width + header.padding.horizontal()
        } else {
            root.min_width
        };
        let probe = Rect::from_xywh(0.0, 0.0, width, 1e6);
        let height = self
            .layout(cx, probe, compact)
            .iter()
            .map(|(_, rect)| rect.max_y())
            .fold(0.0, f64::max)
            + root.padding.block_end;
        Measured {
            size: Size::new(width, height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.viewport = cx.viewport;
        self.mobile = cx.viewport.size.width < SIDEBAR_MOBILE_BREAKPOINT;
        let compact = self.compact();
        self.parts = self.layout(cx, bounds, compact);
        let root = cx.style(self.query(Slot::Root, ControlSize::Default, VisualState::NONE));
        self.motion_ms = root.motion_ms;
        let width = MOBILE_WIDTH.min(cx.viewport.size.width);
        let x = if self.side == SheetSide::Right {
            cx.viewport.max_x() - width
        } else {
            cx.viewport.origin.x
        };
        self.sheet = Rect::from_xywh(x, cx.viewport.origin.y, width, cx.viewport.size.height);
        self.sheet_parts = self.layout(cx, self.sheet, false);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(pointer) => self.pointer(cx, pointer),
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.key(cx, *key),
            ControlEvent::Hover(false) => {
                self.hovered = None;
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::FocusChanged(_) => {
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let compact = self.compact();
        let offcanvas = !self.is_expanded() && self.collapse == SidebarCollapse::Offcanvas;
        if !self.mobile && !offcanvas {
            self.paint_root(cx, cx.bounds, compact);
        }
        self.paint_parts(cx, &self.parts, compact);
    }

    fn overlay(&self) -> Option<Rect> {
        (self.mobile && self.reveal.open).then_some(self.viewport)
    }

    fn modal(&self) -> bool {
        self.mobile && self.reveal.open
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        if !self.mobile {
            return;
        }
        let progress = self.reveal.progress(cx.tokens, self.motion_ms, cx.time);
        if progress <= 0.0 {
            return;
        }
        let scrim = cx.style(StyleQuery::new(ControlKind::Sheet, Slot::Overlay));
        let _ = cx.list.push_opacity(progress.clamp(0.0, 1.0));
        cx.slot(self.viewport, &scrim, |_, _| {});
        let _ = cx.list.pop_opacity();
        let direction = if self.side == SheetSide::Right {
            1.0
        } else {
            -1.0
        };
        let distance = (1.0 - progress) * self.sheet.size.width * direction;
        let _ = cx.list.push_transform(Transform::translate(distance, 0.0));
        self.paint_root(cx, self.sheet, false);
        self.paint_parts(cx, &self.sheet_parts, false);
        let _ = cx.list.pop_transform();
    }

    fn animating(&self, time: f64) -> bool {
        self.reveal.animating(self.motion_ms, time)
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            let mut actions = vec![SemanticAction::Activate];
            if self.collapse != SidebarCollapse::None || self.mobile {
                actions.push(if self.is_expanded() {
                    SemanticAction::Collapse
                } else {
                    SemanticAction::Expand
                });
            }
            actions.push(SemanticAction::Focus);
            actions
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Group, &self.name, actions);
        node.value = self
            .selected
            .and_then(|index| self.item(index))
            .map(|item| item.label.clone());
        node.description = match self
            .stops()
            .get(self.focus)
            .map(|stop| self.active()[*stop].0)
        {
            Some(Part::Toggle) => self.toggle.clone(),
            Some(Part::Item(index)) => self
                .item(index)
                .map(|item| item.label.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        node.state.expanded = Some(self.is_expanded());
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
            SemanticAction::Activate => match self.stops().get(self.focus) {
                Some(part) => {
                    self.activate(cx, *part);
                    true
                }
                None => false,
            },
            SemanticAction::Expand => {
                self.set_expanded(cx, true);
                true
            }
            SemanticAction::Collapse => {
                self.set_expanded(cx, false);
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
            self.reveal = Reveal::default();
        }
    }
}
