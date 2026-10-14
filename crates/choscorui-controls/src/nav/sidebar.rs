//! A sidebar of grouped navigation items that collapses to icons or off
//! the canvas, and becomes a sheet in a narrow window.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::{self, Parts, Reveal};
use crate::style::{
    ButtonVariant, ControlKind, ControlSize, PaintContext, SheetSide, SidebarItemVariant,
    SidebarVariant, Slot, StyleQuery, TextAlign, Variant, VisualState,
};
use crate::{Shortcut, ShortcutModifiers};
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Localizer, Measured, Point, PointerButton,
    PointerEvent, Rect, SemanticAction, SemanticNode, SemanticParts, SemanticRole, Size, Transform,
};

/// Below this window width the sidebar becomes a sheet, in points.
pub const SIDEBAR_MOBILE_BREAKPOINT: f64 = 768.0;

/// Command+B, which opens and collapses the sidebar from anywhere in
/// its window.
const TOGGLE_SHORTCUT: [Shortcut; 1] = [Shortcut {
    key: Key::Character('b'),
    modifiers: ShortcutModifiers {
        shift: false,
        control: false,
        option: false,
        command: true,
    },
}];

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
    /// The look of the row.
    pub variant: SidebarItemVariant,
    /// The items of the sub-menu under the row. The children of a
    /// sub-item do not show.
    pub children: Vec<SidebarItem>,
}

impl SidebarItem {
    pub fn new(label: impl Into<String>, icon: Icon) -> Self {
        Self {
            label: label.into(),
            icon,
            badge: None,
            variant: SidebarItemVariant::Default,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    #[must_use]
    pub fn with_variant(mut self, variant: SidebarItemVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Gives the row a sub-menu with `children`.
    #[must_use]
    pub fn with_children(mut self, children: Vec<SidebarItem>) -> Self {
        self.children = children;
        self
    }
}

/// An item or a sub-item, in the order that items count.
#[derive(Clone, Copy)]
struct Entry<'a> {
    item: &'a SidebarItem,
    /// The index of the item whose sub-menu holds this sub-item.
    parent: Option<usize>,
}

impl Entry<'_> {
    /// True for an item with a sub-menu.
    fn has_menu(self) -> bool {
        self.parent.is_none() && !self.item.children.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Toggle,
    Label(usize),
    /// An item or a sub-item by its index across all groups.
    Item(usize),
    /// The sub-menu box of the item with this index.
    Menu(usize),
}

impl Part {
    fn focusable(self) -> bool {
        matches!(self, Part::Toggle | Part::Item(_))
    }
}

/// A sidebar: a toggle and groups of items with icons. The selected item
/// takes the accent look. An item with children opens a sub-menu. Command+B, the toggle, `Expand`, and `Collapse`
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
    menu_size: ControlSize,
    /// The items whose sub-menus are open.
    open: Vec<usize>,
    selected: Option<usize>,
    /// The focused part, as an index into the active parts.
    focus: usize,
    /// Hover and press of the rows and the toggle.
    tracked: Parts,
    mobile: bool,
    parts: Vec<(Part, Rect)>,
    sheet_parts: Vec<(Part, Rect)>,
    viewport: Rect,
    sheet: Rect,
    reveal: Reveal,
    motion_ms: u32,
    /// The part that takes focus after the next arrange.
    keep: Option<Part>,
    /// The hovered item on the icon rail, the time its label shows, and
    /// whether it shows.
    tip: Option<(usize, f64, bool)>,
    /// The label bubble of each rail item, by part index.
    tips: Vec<Rect>,
    rtl: bool,
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
            menu_size: ControlSize::Default,
            open: Vec::new(),
            selected: None,
            focus: 0,
            tracked: Parts::default(),
            mobile: false,
            parts: Vec::new(),
            sheet_parts: Vec::new(),
            viewport: Rect::default(),
            sheet: Rect::default(),
            reveal: Reveal::default(),
            motion_ms: 0,
            keep: None,
            tip: None,
            tips: Vec::new(),
            rtl: false,
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

    /// The size of the item rows: `Sm`, `Default`, or `Lg`. Other sizes
    /// take `Default`.
    #[must_use]
    pub fn with_menu_size(mut self, size: ControlSize) -> Self {
        self.menu_size = match size {
            ControlSize::Sm | ControlSize::Lg => size,
            _ => ControlSize::Default,
        };
        self
    }

    /// Selects `item` and opens the sub-menu that holds it. An index past
    /// the items, or of an item with a sub-menu, selects nothing.
    #[must_use]
    pub fn with_selected(mut self, item: usize) -> Self {
        let fits = self
            .entries()
            .get(item)
            .is_some_and(|entry| !entry.has_menu());
        self.selected = fits.then_some(item);
        if fits {
            self.open_parent(item);
        }
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
            self.menu_size
        }
    }

    /// The size of the sub-item rows.
    fn link_size(&self) -> ControlSize {
        if self.menu_size == ControlSize::Sm {
            ControlSize::Sm
        } else {
            ControlSize::Default
        }
    }

    /// A query of a panel slot, which takes the sidebar variant.
    fn query(&self, slot: Slot, size: ControlSize, state: VisualState) -> StyleQuery {
        Self::item_query(slot, self.variant.into(), size, state)
    }

    /// A query of a row slot. Rows take the variant of their item, not
    /// the sidebar variant.
    fn item_query(
        slot: Slot,
        variant: Variant,
        size: ControlSize,
        state: VisualState,
    ) -> StyleQuery {
        StyleQuery::new(ControlKind::Sidebar, slot)
            .variant(variant)
            .size(size)
            .state(state)
    }

    fn toggle_query(state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Button, Slot::Root)
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::IconSm)
            .state(state)
    }

    /// The items, each followed by its sub-items.
    fn entries(&self) -> Vec<Entry<'_>> {
        let mut entries = Vec::new();
        for item in self.groups.iter().flat_map(|(_, items)| items) {
            let parent = entries.len();
            entries.push(Entry { item, parent: None });
            entries.extend(item.children.iter().map(|child| Entry {
                item: child,
                parent: Some(parent),
            }));
        }
        entries
    }

    fn item(&self, index: usize) -> Option<&SidebarItem> {
        self.entries().get(index).map(|entry| entry.item)
    }

    /// Opens the sub-menu that holds `item`, if any.
    fn open_parent(&mut self, item: usize) {
        let parent = self.entries().get(item).and_then(|entry| entry.parent);
        if let Some(parent) = parent.filter(|parent| !self.open.contains(parent)) {
            self.open.push(parent);
        }
    }

    /// The focused part.
    fn focused(&self) -> Option<Part> {
        let stops = self.stops();
        stops.get(self.focus).map(|stop| self.active()[*stop].0)
    }

    fn focus_part(&mut self, part: Part) {
        let active = self.active();
        let stop = self.stops().iter().position(|stop| active[*stop].0 == part);
        if let Some(stop) = stop {
            self.focus = stop;
        }
    }

    /// The focused item with a sub-menu, while the item labels show.
    /// `Expand` and `Collapse` act on its sub-menu.
    fn menu_target(&self) -> Option<usize> {
        let Some(Part::Item(index)) = self.focused() else {
            return None;
        };
        let entries = self.entries();
        let has_menu = entries.get(index).is_some_and(|entry| entry.has_menu());
        (self.is_expanded() && has_menu).then_some(index)
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
        // Row sizes do not depend on the item variant.
        let row = |slot, size| Self::item_query(slot, Variant::default(), size, VisualState::NONE);
        let item = cx.style(row(Slot::Item, size));
        let menu = cx.style(self.query(Slot::Row, self.link_size(), VisualState::NONE));
        let link = cx.style(row(Slot::Link, self.link_size()));
        let toggle = cx.style(Self::toggle_query(VisualState::NONE));
        let inner = Rect::from_xywh(
            panel.origin.x + root.padding.inline_start,
            panel.origin.y + root.padding.block_start,
            panel.size.width - root.padding.horizontal(),
            panel.size.height - root.padding.vertical(),
        );
        let mut parts = Vec::new();
        let mut y = inner.origin.y;
        if self.collapsible() {
            let side = toggle.min_width.max(toggle.min_height);
            let x = if compact {
                panel.center().x - side / 2.0
            } else if cx.rtl() {
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
            for entry in entries {
                // The icon rail centers its rows.
                let (row_x, row_width) = if compact {
                    (panel.center().x - item.min_width / 2.0, item.min_width)
                } else {
                    (x, width)
                };
                parts.push((
                    Part::Item(index),
                    Rect::from_xywh(row_x, y, row_width, item.min_height),
                ));
                y += item.min_height + group.gap;
                let parent = index;
                index += 1;
                if compact || entry.children.is_empty() || !self.open.contains(&parent) {
                    index += entry.children.len();
                    continue;
                }
                // The sub-menu box: inset by the row's minimum width at
                // the start, with a border on that edge.
                let margin = menu.min_width;
                let box_x = if cx.rtl() { x } else { x + margin };
                let box_width = (width - margin).max(0.0);
                let slot = parts.len();
                parts.push((Part::Menu(parent), Rect::default()));
                let top = y;
                y += menu.padding.block_start;
                for _ in &entry.children {
                    parts.push((
                        Part::Item(index),
                        Rect::from_xywh(
                            box_x + menu.padding.inline_start,
                            y,
                            (box_width - menu.padding.horizontal()).max(0.0),
                            link.min_height,
                        ),
                    ));
                    y += link.min_height + menu.gap;
                    index += 1;
                }
                y += menu.padding.block_end - menu.gap;
                parts[slot].1 = Rect::from_xywh(box_x, top, box_width, y - top);
                y += group.gap;
            }
            // The last item has no gap after it.
            let trailing = if entries.is_empty() { 0.0 } else { group.gap };
            y += group.padding.block_end - trailing;
        }
        parts
    }

    /// True when the sidebar can open and collapse: in a narrow window,
    /// or with a collapse style.
    fn collapsible(&self) -> bool {
        self.mobile || self.collapse != SidebarCollapse::None
    }

    /// Opens or collapses the sidebar at `time`. Returns true when it
    /// changed.
    fn apply_expanded(&mut self, expanded: bool, time: f64) -> bool {
        if self.mobile {
            if self.reveal.open == expanded {
                return false;
            }
            self.reveal.set(expanded, time);
        } else {
            if !self.collapsible() || self.expanded == expanded {
                return false;
            }
            self.expanded = expanded;
        }
        self.focus = 0;
        true
    }

    fn set_expanded(&mut self, cx: &mut ControlContext<'_>, expanded: bool) {
        if !self.apply_expanded(expanded, cx.time()) {
            return;
        }
        if !self.mobile {
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
            Some(Part::Item(index)) if self.entries().get(index).is_some_and(|e| e.has_menu()) => {
                if self.is_expanded() {
                    let open = !self.open.contains(&index);
                    self.set_menu_open(cx, index, open);
                } else {
                    // On the icon rail: expand the sidebar with the
                    // sub-menu open, and keep focus on the item.
                    if !self.open.contains(&index) {
                        self.open.push(index);
                    }
                    self.set_expanded(cx, true);
                    self.keep = Some(Part::Item(index));
                }
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

    /// Opens or closes the sub-menu of `item`.
    fn set_menu_open(&mut self, cx: &mut ControlContext<'_>, item: usize, open: bool) {
        if self.open.contains(&item) == open {
            return;
        }
        if open {
            self.open.push(item);
        } else {
            self.open.retain(|other| *other != item);
        }
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
    }

    /// ArrowRight, or ArrowLeft right to left, with `forward`: opens the
    /// focused sub-menu, or goes into it. The other arrow closes it, or
    /// goes from a sub-item to its item.
    fn menu_key(&mut self, cx: &mut ControlContext<'_>, forward: bool) -> EventReply {
        let Some(Part::Item(index)) = self.focused() else {
            return EventReply::Continue;
        };
        let Some(entry) = self.entries().get(index).copied() else {
            return EventReply::Continue;
        };
        let open = self.open.contains(&index);
        match (entry.parent, forward) {
            (Some(parent), false) => self.focus_part(Part::Item(parent)),
            (None, true) if entry.has_menu() && open => self.focus_part(Part::Item(index + 1)),
            (None, true) if entry.has_menu() => self.set_menu_open(cx, index, true),
            (None, false) if open => self.set_menu_open(cx, index, false),
            _ => return EventReply::Continue,
        }
        cx.request_paint();
        cx.request_semantics();
        EventReply::Stop
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        let modifiers = cx.modifiers();
        let stops = self.stops();
        match key {
            Key::Character('b') if modifiers.command => {
                if !self.collapsible() {
                    return EventReply::Continue;
                }
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
            Key::ArrowRight | Key::ArrowLeft if self.is_expanded() => {
                let forward = (key == Key::ArrowRight) != self.rtl;
                return self.menu_key(cx, forward);
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

    fn pointer(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &PointerEvent,
        pressed: Option<usize>,
    ) -> EventReply {
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
                let Some(part) = self.tracked.pressed else {
                    return EventReply::Continue;
                };
                if let Some(stop) = self.stops().iter().position(|stop| *stop == part) {
                    self.focus = stop;
                }
                cx.request_focus();
                EventReply::Stop
            }
            PointerEvent::Up { .. } if pressed.is_some() => {
                if let Some(part) = pressed.filter(|part| self.tracked.hovered == Some(*part)) {
                    self.activate(cx, part);
                }
                EventReply::Stop
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
        let entries = self.entries();
        for (index, (part, rect)) in parts.iter().enumerate() {
            let state = self.tracked.state(index, host).set(
                VisualState::FOCUS_VISIBLE,
                host.focus_visible() && focused == Some(index),
            );
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
                Part::Menu(_) => {
                    let style = cx.style(self.query(Slot::Row, self.link_size(), state));
                    cx.slot(*rect, &style, |_, _| {});
                }
                Part::Item(item_index) => {
                    let Some(entry) = entries.get(*item_index).copied() else {
                        continue;
                    };
                    let item = entry.item;
                    let (slot, size) = if entry.parent.is_some() {
                        (Slot::Link, self.link_size())
                    } else {
                        (Slot::Item, size)
                    };
                    let state = state.set(VisualState::SELECTED, self.marked(*item_index, compact));
                    let variant = item.variant.into();
                    let style = cx.style(Self::item_query(slot, variant, size, state));
                    let badge = cx.style(Self::item_query(Slot::Badge, variant, size, state));
                    let chevron = (entry.has_menu() && !compact).then(|| {
                        if self.open.contains(item_index) {
                            Icon::ChevronDown
                        } else if cx.rtl {
                            Icon::ChevronLeft
                        } else {
                            Icon::ChevronRight
                        }
                    });
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
                        let end = if let Some(chevron) = chevron {
                            let x = if cx.rtl {
                                inner.origin.x
                            } else {
                                inner.max_x() - icon_size
                            };
                            let mark =
                                Rect::from_xywh(x, inner.origin.y, icon_size, inner.size.height);
                            cx.icon(mark, chevron, &style);
                            icon_size + style.gap
                        } else {
                            0.0
                        };
                        let text = Rect::from_xywh(
                            if cx.rtl {
                                inner.origin.x + end
                            } else {
                                inner.origin.x + skip
                            },
                            inner.origin.y,
                            (inner.size.width - skip - end).max(0.0),
                            inner.size.height,
                        );
                        let count = item.badge.as_deref().unwrap_or_default();
                        cx.label_and_trailing(text, &item.label, &style, count, &badge);
                    });
                }
            }
        }
    }

    /// True when the row of `item` takes the selected look: the selected
    /// item, or the item whose hidden sub-menu holds it.
    fn marked(&self, item: usize, compact: bool) -> bool {
        let Some(selected) = self.selected else {
            return false;
        };
        let parent = self.entries().get(selected).and_then(|entry| entry.parent);
        selected == item || (parent == Some(item) && (compact || !self.open.contains(&item)))
    }

    /// True while the sidebar shows as an icon rail.
    fn rail(&self) -> bool {
        !self.mobile && !self.is_expanded() && self.collapse == SidebarCollapse::Icon
    }

    /// The label bubble beside each item on the icon rail, by part index.
    fn tip_rects(&self, cx: &MeasureContext<'_>) -> Vec<Rect> {
        if !self.rail() {
            return Vec::new();
        }
        let style = cx.style(StyleQuery::new(ControlKind::Tooltip, Slot::Root));
        let gap = cx
            .style(StyleQuery::new(ControlKind::Tooltip, Slot::Indicator))
            .min_height;
        let entries = self.entries();
        // The bubble sits toward the content, away from the window edge.
        let before = (self.side == SheetSide::Right) != self.rtl;
        self.parts
            .iter()
            .map(|(part, row)| {
                let Part::Item(index) = part else {
                    return Rect::default();
                };
                let label = entries.get(*index).map_or("", |entry| &entry.item.label);
                let width = (cx.text.width(label, &style.text) + style.padding.horizontal())
                    .min(style.max_width);
                let height =
                    (style.text.line_height + style.padding.vertical()).max(style.min_height);
                let x = if before {
                    row.origin.x - gap - width
                } else {
                    row.max_x() + gap
                };
                Rect::from_xywh(x, row.center().y - height / 2.0, width, height)
            })
            .collect()
    }

    /// Follows the hovered rail item: its label shows after the tooltip
    /// delay, and hides when the pointer leaves it.
    fn follow_tip(&mut self, cx: &mut ControlContext<'_>) {
        let hovered = self.tracked.hovered.filter(|part| {
            self.rail() && matches!(self.parts.get(*part), Some((Part::Item(_), _)))
        });
        match (hovered, self.tip) {
            (Some(part), Some((tip, ..))) if part == tip => {}
            (Some(part), _) => {
                let at = cx.time() + crate::popup::TOOLTIP_DELAY.as_secs_f64();
                self.tip = Some((part, at, false));
            }
            (None, Some(_)) => {
                self.tip = None;
                cx.request_paint();
            }
            (None, None) => {}
        }
    }

    /// The shown label bubble and its text.
    fn shown_tip(&self) -> Option<(Rect, &str)> {
        let (part, _, true) = self.tip? else {
            return None;
        };
        let Part::Item(index) = self.parts.get(part)?.0 else {
            return None;
        };
        let label = &self.entries().get(index)?.item.label;
        Some((*self.tips.get(part)?, label.as_str()))
    }

    /// Paints the sidebar box with its border on the inner edge.
    fn paint_root(&self, cx: &mut PaintContext<'_>, rect: Rect, compact: bool) {
        let style = cx.style(self.query(Slot::Root, self.size(compact), VisualState::NONE));
        // The recipe puts the border at the inline end, which is the
        // inner edge of a left sidebar. A right sidebar mirrors it.
        cx.edge_panel(rect, &style, self.side == SheetSide::Right);
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
        } else if compact {
            // A floating or inset rail keeps room for its padding.
            root.min_width + root.padding.horizontal()
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
        self.rtl = cx.rtl();
        self.mobile = cx.viewport.size.width < SIDEBAR_MOBILE_BREAKPOINT;
        let compact = self.compact();
        self.parts = self.layout(cx, bounds, compact);
        if let Some(part) = self.keep.take() {
            self.focus_part(part);
        }
        self.tips = self.tip_rects(cx);
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
        let pressed = self.tracked.pressed;
        let mut tracked = self.tracked;
        tracked.track(cx, event, |point| self.part_at(point));
        self.tracked = tracked;
        self.follow_tip(cx);
        match event {
            ControlEvent::Timer => {
                if let Some((_, at, shown)) = &mut self.tip
                    && !*shown
                    && cx.time() >= *at
                {
                    *shown = true;
                    cx.request_paint();
                }
                EventReply::Continue
            }
            ControlEvent::Pointer(PointerEvent::Down { .. }) | ControlEvent::Key(_)
                if self.tip.is_some_and(|(_, _, shown)| shown) =>
            {
                // A press or a key hides the label until the pointer
                // moves to another item.
                self.tip = self.tip.map(|(part, _, _)| (part, f64::INFINITY, false));
                cx.request_paint();
                match event {
                    ControlEvent::Pointer(pointer) => self.pointer(cx, pointer, pressed),
                    ControlEvent::Key(KeyEvent::Down { key, .. }) => self.key(cx, *key),
                    _ => EventReply::Continue,
                }
            }
            ControlEvent::Pointer(pointer) => self.pointer(cx, pointer, pressed),
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.key(cx, *key),
            ControlEvent::Shortcut(_) if self.collapse != SidebarCollapse::None || self.mobile => {
                let expanded = !self.is_expanded();
                self.set_expanded(cx, expanded);
                EventReply::Stop
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
        (self.mobile && self.reveal.open)
            .then_some(self.viewport)
            .or_else(|| self.shown_tip().map(|(rect, _)| rect))
    }

    fn deadline(&self) -> Option<f64> {
        self.tip
            .filter(|(_, at, shown)| !shown && at.is_finite())
            .map(|(_, at, _)| at)
    }

    fn modal(&self) -> bool {
        self.mobile && self.reveal.open
    }

    fn window_shortcuts(&self) -> &[Shortcut] {
        &TOGGLE_SHORTCUT
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        if let Some((rect, label)) = self.shown_tip() {
            let style = cx.style(StyleQuery::new(ControlKind::Tooltip, Slot::Root));
            cx.slot(rect, &style, |cx, inner| {
                cx.label(inner, label, &style, TextAlign::Center);
            });
        }
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
        let menu = self.menu_target();
        let expanded = match menu {
            Some(item) => self.open.contains(&item),
            None => self.is_expanded(),
        };
        let actions = if self.enabled {
            let mut actions = vec![SemanticAction::Activate];
            if self.collapsible() || menu.is_some() {
                actions.push(if expanded {
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
        node.state.expanded = Some(expanded);
        node.state.disabled = !self.enabled;
        node
    }

    /// A `Button` part for the toggle, a `StaticText` part for each group
    /// label, and a `Link` part for each item, the selected one selected.
    /// An item with a sub-menu is a `Button`, expanded while open, with
    /// its sub-items under it. The focused row is the focused part.
    fn semantic_parts(&self, _strings: &Localizer, parts: &mut SemanticParts) {
        let entries = self.entries();
        let focused = self.focused();
        // The part position of each item, for its sub-items.
        let mut positions = vec![None; entries.len()];
        for (part, rect) in self.active() {
            let (role, name, parent) = match *part {
                Part::Toggle => (SemanticRole::Button, self.toggle.as_str(), None),
                Part::Label(group) => match self.groups.get(group) {
                    Some((label, _)) => (SemanticRole::StaticText, label.as_str(), None),
                    None => continue,
                },
                Part::Item(index) => match entries.get(index) {
                    Some(entry) if entry.has_menu() => {
                        (SemanticRole::Button, entry.item.label.as_str(), None)
                    }
                    Some(entry) => {
                        let parent = entry.parent.and_then(|parent| positions[parent]);
                        (SemanticRole::Link, entry.item.label.as_str(), parent)
                    }
                    None => continue,
                },
                Part::Menu(_) => continue,
            };
            let pressable = role != SemanticRole::StaticText;
            let node = &mut hosted::pressable(parts, parent, role, name, *rect, pressable).node;
            node.state.disabled = false;
            match *part {
                Part::Toggle => node.state.expanded = Some(self.is_expanded()),
                Part::Item(index) if role == SemanticRole::Button => {
                    node.state.expanded = Some(self.open.contains(&index));
                }
                Part::Item(index) => {
                    node.state.selected = Some(self.selected == Some(index));
                    node.value = entries[index].item.badge.clone();
                }
                _ => {}
            }
            if let Part::Item(index) = *part {
                positions[index] = Some(parts.len() - 1);
            }
            if focused == Some(*part) {
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
        match action {
            SemanticAction::Activate => match self.stops().get(self.focus) {
                Some(part) => {
                    self.activate(cx, *part);
                    true
                }
                None => false,
            },
            SemanticAction::Expand | SemanticAction::Collapse => {
                let open = action == SemanticAction::Expand;
                match self.menu_target() {
                    Some(item) => self.set_menu_open(cx, item, open),
                    None => self.set_expanded(cx, open),
                }
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

    /// Takes `Index`, the selected item, or no item, and opens the
    /// sub-menu that holds the item. Rejects an index past the items or
    /// of an item with a sub-menu.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Index(index) = *value else {
            return false;
        };
        let entries = self.entries();
        let fits =
            index.is_none_or(|index| entries.get(index).is_some_and(|entry| !entry.has_menu()));
        if fits {
            self.selected = index;
            if let Some(index) = index {
                self.open_parent(index);
            }
        }
        fits
    }

    /// Opens or collapses the sidebar, or the sheet in a narrow window.
    fn set_open(&mut self, open: bool, time: f64) -> bool {
        if !self.collapsible() || (open && self.mobile && !self.enabled) {
            return false;
        }
        self.apply_expanded(open, time);
        true
    }
}
