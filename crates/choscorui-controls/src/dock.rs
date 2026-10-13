//! Dockable panels around a central area: tabbed groups in four edge
//! areas, floating panels, hide/show with remembered location, keyboard
//! resizing, and a text form for settings persistence.
//!
//! MDI (child windows inside a main-window workspace) is intentionally not
//! provided. Documents use tabs in the central area or separate native
//! windows, which is what macOS users expect.

use crate::style::fill_area;
use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, TabsVariant, TextAlign, VisualState,
};
use choscorui_core::{
    EventReply, Key, KeyEvent, Measured, Point, PointerButton, PointerEvent, Rect, SemanticAction,
    SemanticNode, SemanticRole, SemanticState, Size,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PanelId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum DockArea {
    Left,
    Right,
    Top,
    Bottom,
}

impl DockArea {
    pub const ALL: [DockArea; 4] = [Self::Left, Self::Right, Self::Top, Self::Bottom];
    fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
    fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|area| area.name() == text)
    }
    fn index(self) -> usize {
        self as usize
    }
    fn horizontal(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockPanel {
    /// Stable persistence key, such as `"files"`.
    pub key: String,
    pub title: String,
    pub closable: bool,
    pub floatable: bool,
}

impl DockPanel {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            closable: true,
            floatable: true,
        }
    }
}

/// Where a panel lives (or lived, while hidden).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PanelLocation {
    /// `group` is an index into the area's tab groups.
    Docked {
        area: DockArea,
        group: usize,
    },
    Floating(Rect),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DockTarget {
    /// Adds a tab to an existing group.
    Tab {
        area: DockArea,
        group: usize,
    },
    /// Starts a new group at `index` in the area (clamped).
    NewGroup {
        area: DockArea,
        index: usize,
    },
    Floating(Rect),
}

#[derive(Clone, Debug, PartialEq)]
struct TabGroup {
    panels: Vec<PanelId>,
    active: usize,
    weight: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct AreaState {
    groups: Vec<TabGroup>,
    extent: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DockError {
    UnknownPanel(PanelId),
    DuplicateKey(String),
    NotClosable(PanelId),
    NotFloatable(PanelId),
    Parse { line: usize, message: String },
}

impl std::fmt::Display for DockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPanel(panel) => write!(f, "unknown dock panel {panel:?}"),
            Self::DuplicateKey(key) => write!(f, "a dock panel already uses the key {key:?}"),
            Self::NotClosable(panel) => write!(f, "dock panel {panel:?} cannot be closed"),
            Self::NotFloatable(panel) => write!(f, "dock panel {panel:?} cannot float"),
            Self::Parse { line, message } => write!(f, "dock layout line {line}: {message}"),
        }
    }
}

impl std::error::Error for DockError {}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupFrame {
    pub area: DockArea,
    pub group: usize,
    pub rect: Rect,
    pub tab_bar: Rect,
    pub content: Rect,
    pub tabs: Vec<PanelId>,
    pub active: PanelId,
}

impl GroupFrame {
    /// The tab at `index` in the tab bar. Tabs share the bar equally, in
    /// reading order.
    pub fn tab_rect(&self, index: usize, rtl: bool) -> Rect {
        let count = self.tabs.len().max(1);
        let width = self.tab_bar.size.width / count as f64;
        let slot = if rtl {
            count.saturating_sub(index + 1)
        } else {
            index
        };
        Rect::new(
            Point::new(
                self.tab_bar.origin.x + slot as f64 * width,
                self.tab_bar.origin.y,
            ),
            Size::new(width, self.tab_bar.size.height),
        )
    }
}

/// The dock part that shows a keyboard focus ring.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DockFocus {
    #[default]
    None,
    Tab(PanelId),
    Splitter(DockArea),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockArrangement {
    pub center: Rect,
    pub groups: Vec<GroupFrame>,
    /// Draggable handle between each non-empty area and the center.
    pub splitters: Vec<(DockArea, Rect)>,
    pub floating: Vec<(PanelId, Rect)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DockRestoreReport {
    /// Keys in the saved layout that no registered panel has.
    pub unknown_keys: Vec<String>,
    /// Registered panels absent from the saved layout; they keep their
    /// registration location (for example, panels added in an update).
    pub unplaced: Vec<PanelId>,
}

const MIN_EXTENT: f64 = 80.0;
const SPLITTER: f64 = 4.0;
const KEY_STEP: f64 = 10.0;

/// Dock layout model; geometry is computed by `arrange`.
#[derive(Clone, Debug, PartialEq)]
pub struct DockLayout {
    panels: BTreeMap<PanelId, DockPanel>,
    areas: [AreaState; 4],
    floating: Vec<(PanelId, Rect)>,
    hidden: BTreeMap<PanelId, PanelLocation>,
    tab_height: f64,
    /// Focus and drag state in a control host.
    hosted: HostState,
}

impl DockLayout {
    pub fn new(tab_height: f64) -> Self {
        let area = AreaState {
            groups: Vec::new(),
            extent: 240.0,
        };
        Self {
            panels: BTreeMap::new(),
            areas: [area.clone(), area.clone(), area.clone(), area],
            floating: Vec::new(),
            hidden: BTreeMap::new(),
            tab_height,
            hosted: HostState::default(),
        }
    }

    /// Registers a panel at its default location.
    pub fn add(
        &mut self,
        id: PanelId,
        panel: DockPanel,
        target: DockTarget,
    ) -> Result<(), DockError> {
        if self
            .panels
            .values()
            .any(|existing| existing.key == panel.key)
            || self.panels.contains_key(&id)
        {
            return Err(DockError::DuplicateKey(panel.key));
        }
        self.panels.insert(id, panel);
        self.place(id, target);
        Ok(())
    }

    pub fn panel(&self, id: PanelId) -> Option<&DockPanel> {
        self.panels.get(&id)
    }
    pub fn panel_by_key(&self, key: &str) -> Option<PanelId> {
        self.panels
            .iter()
            .find(|(_, panel)| panel.key == key)
            .map(|(id, _)| *id)
    }

    pub fn location(&self, id: PanelId) -> Option<PanelLocation> {
        for area in DockArea::ALL {
            for (index, group) in self.areas[area.index()].groups.iter().enumerate() {
                if group.panels.contains(&id) {
                    return Some(PanelLocation::Docked { area, group: index });
                }
            }
        }
        self.floating
            .iter()
            .find(|(panel, _)| *panel == id)
            .map(|(_, rect)| PanelLocation::Floating(*rect))
    }

    pub fn is_visible(&self, id: PanelId) -> bool {
        self.location(id).is_some()
    }

    /// Whether `id` is the selected tab of its group (or floating).
    pub fn is_active(&self, id: PanelId) -> bool {
        match self.location(id) {
            Some(PanelLocation::Docked { area, group }) => {
                let group = &self.areas[area.index()].groups[group];
                group.panels[group.active] == id
            }
            Some(PanelLocation::Floating(_)) => true,
            None => false,
        }
    }

    fn detach(&mut self, id: PanelId) -> Option<PanelLocation> {
        let location = self.location(id)?;
        match location {
            PanelLocation::Docked { area, group } => {
                let groups = &mut self.areas[area.index()].groups;
                let tabs = &mut groups[group];
                let index = tabs.panels.iter().position(|panel| *panel == id)?;
                tabs.panels.remove(index);
                if tabs.panels.is_empty() {
                    groups.remove(group);
                } else if tabs.active >= index && tabs.active > 0 {
                    tabs.active -= 1;
                }
            }
            PanelLocation::Floating(_) => self.floating.retain(|(panel, _)| *panel != id),
        }
        Some(location)
    }

    fn place(&mut self, id: PanelId, target: DockTarget) {
        match target {
            DockTarget::Tab { area, group } => {
                let groups = &mut self.areas[area.index()].groups;
                match groups.get_mut(group) {
                    Some(tabs) => {
                        tabs.panels.push(id);
                        tabs.active = tabs.panels.len() - 1;
                    }
                    None => groups.push(TabGroup {
                        panels: vec![id],
                        active: 0,
                        weight: 1.0,
                    }),
                }
            }
            DockTarget::NewGroup { area, index } => {
                let groups = &mut self.areas[area.index()].groups;
                let index = index.min(groups.len());
                groups.insert(
                    index,
                    TabGroup {
                        panels: vec![id],
                        active: 0,
                        weight: 1.0,
                    },
                );
            }
            DockTarget::Floating(rect) => self.floating.push((id, rect)),
        }
    }

    /// Moves a visible or hidden panel; it becomes visible and active.
    pub fn dock(&mut self, id: PanelId, target: DockTarget) -> Result<(), DockError> {
        let panel = self.panels.get(&id).ok_or(DockError::UnknownPanel(id))?;
        if matches!(target, DockTarget::Floating(_)) && !panel.floatable {
            return Err(DockError::NotFloatable(id));
        }
        // Resolve the target before detaching shifts group indexes.
        let target = match (target, self.location(id)) {
            (
                DockTarget::Tab { area, group },
                Some(PanelLocation::Docked {
                    area: from,
                    group: from_group,
                }),
            ) if area == from
                && self.areas[area.index()].groups[from_group].panels.len() == 1
                && group > from_group =>
            {
                DockTarget::Tab {
                    area,
                    group: group - 1,
                }
            }
            (target, _) => target,
        };
        self.detach(id);
        self.hidden.remove(&id);
        self.place(id, target);
        Ok(())
    }

    /// Hides (closes) a panel, remembering where it was. Returns the panel
    /// that should receive focus next (the newly active tab in the same
    /// group), if any.
    pub fn close(&mut self, id: PanelId) -> Result<Option<PanelId>, DockError> {
        let panel = self.panels.get(&id).ok_or(DockError::UnknownPanel(id))?;
        if !panel.closable {
            return Err(DockError::NotClosable(id));
        }
        let Some(location) = self.detach(id) else {
            return Ok(None);
        };
        self.hidden.insert(id, location);
        Ok(match location {
            PanelLocation::Docked { area, group } => self.areas[area.index()]
                .groups
                .get(group)
                .filter(|tabs| !tabs.panels.contains(&id))
                .map(|tabs| tabs.panels[tabs.active]),
            PanelLocation::Floating(_) => None,
        })
    }

    /// Shows a panel at its remembered location and makes it active.
    pub fn show(&mut self, id: PanelId) -> Result<(), DockError> {
        if !self.panels.contains_key(&id) {
            return Err(DockError::UnknownPanel(id));
        }
        if self.is_visible(id) {
            return self.activate(id);
        }
        let target = match self.hidden.remove(&id) {
            Some(PanelLocation::Docked { area, group }) => DockTarget::Tab { area, group },
            Some(PanelLocation::Floating(rect)) => DockTarget::Floating(rect),
            None => DockTarget::Tab {
                area: DockArea::Right,
                group: usize::MAX,
            },
        };
        self.place(id, target);
        Ok(())
    }

    /// Selects the panel's tab.
    pub fn activate(&mut self, id: PanelId) -> Result<(), DockError> {
        match self.location(id) {
            Some(PanelLocation::Docked { area, group }) => {
                let tabs = &mut self.areas[area.index()].groups[group];
                tabs.active = tabs.panels.iter().position(|panel| *panel == id).unwrap();
                Ok(())
            }
            Some(PanelLocation::Floating(_)) => Ok(()),
            None => Err(DockError::UnknownPanel(id)),
        }
    }

    pub fn set_floating_rect(&mut self, id: PanelId, rect: Rect) -> bool {
        match self.floating.iter_mut().find(|(panel, _)| *panel == id) {
            Some(entry) => {
                entry.1 = rect;
                true
            }
            None => false,
        }
    }

    pub fn extent(&self, area: DockArea) -> f64 {
        self.areas[area.index()].extent
    }

    /// Sets an area's width (left/right) or height (top/bottom). It is
    /// clamped during `arrange` so the center keeps `MIN_EXTENT`.
    pub fn set_extent(&mut self, area: DockArea, extent: f64) {
        if extent.is_finite() {
            self.areas[area.index()].extent = extent.max(MIN_EXTENT);
        }
    }

    /// Keyboard resizing of an area's splitter (arrows by 10 points,
    /// Home = minimum). Returns whether the extent changed.
    pub fn handle_splitter_key(&mut self, area: DockArea, key: Key) -> bool {
        let grow = match (area, key) {
            (DockArea::Left, Key::ArrowRight)
            | (DockArea::Right, Key::ArrowLeft)
            | (DockArea::Top, Key::ArrowDown)
            | (DockArea::Bottom, Key::ArrowUp) => KEY_STEP,
            (DockArea::Left, Key::ArrowLeft)
            | (DockArea::Right, Key::ArrowRight)
            | (DockArea::Top, Key::ArrowUp)
            | (DockArea::Bottom, Key::ArrowDown) => -KEY_STEP,
            (_, Key::Home) => f64::NEG_INFINITY,
            _ => return false,
        };
        let before = self.extent(area);
        let after = (before + grow).max(MIN_EXTENT);
        self.areas[area.index()].extent = after;
        after != before
    }

    /// Relative size of a group within its area (default 1.0).
    pub fn set_group_weight(&mut self, area: DockArea, group: usize, weight: f64) {
        if let Some(tabs) = self.areas[area.index()].groups.get_mut(group)
            && weight.is_finite()
            && weight > 0.0
        {
            tabs.weight = weight;
        }
    }

    /// Computes frames. Top and bottom areas span the full width; left and
    /// right areas sit between them. Extents are clamped so the center
    /// keeps at least `MIN_EXTENT` in each direction.
    pub fn arrange(&self, bounds: Rect) -> DockArrangement {
        let mut center = bounds;
        let mut groups = Vec::new();
        let mut splitters = Vec::new();
        for area in [
            DockArea::Top,
            DockArea::Bottom,
            DockArea::Left,
            DockArea::Right,
        ] {
            let state = &self.areas[area.index()];
            if state.groups.is_empty() {
                continue;
            }
            let available = if area.horizontal() {
                center.size.width
            } else {
                center.size.height
            } - MIN_EXTENT
                - SPLITTER;
            let extent = state.extent.min(available).max(0.0);
            let (area_rect, splitter, rest) = split_edge(center, area, extent);
            center = rest;
            splitters.push((area, splitter));
            let total: f64 = state.groups.iter().map(|group| group.weight).sum();
            let mut offset = 0.0;
            for (index, group) in state.groups.iter().enumerate() {
                let share = group.weight / total;
                // Groups stack across the area's long axis.
                let rect = if area.horizontal() {
                    let height = area_rect.size.height * share;
                    Rect::new(
                        Point::new(area_rect.origin.x, area_rect.origin.y + offset),
                        Size::new(area_rect.size.width, height),
                    )
                } else {
                    let width = area_rect.size.width * share;
                    Rect::new(
                        Point::new(area_rect.origin.x + offset, area_rect.origin.y),
                        Size::new(width, area_rect.size.height),
                    )
                };
                offset += if area.horizontal() {
                    rect.size.height
                } else {
                    rect.size.width
                };
                let tab_height = self.tab_height.min(rect.size.height);
                groups.push(GroupFrame {
                    area,
                    group: index,
                    rect,
                    tab_bar: Rect::new(rect.origin, Size::new(rect.size.width, tab_height)),
                    content: Rect::new(
                        Point::new(rect.origin.x, rect.origin.y + tab_height),
                        Size::new(rect.size.width, rect.size.height - tab_height),
                    ),
                    tabs: group.panels.clone(),
                    active: group.panels[group.active],
                });
            }
        }
        DockArrangement {
            center,
            groups,
            splitters,
            floating: self.floating.clone(),
        }
    }

    /// Paints the groups and splitters of `arrangement`. Each group gets
    /// the `Dock` frame and a line-variant `Tabs` bar; the active tab is
    /// underlined. Splitters use the `Resizable` handle. The host paints
    /// the center and the panel contents.
    pub fn paint(
        &self,
        cx: &mut PaintContext<'_>,
        arrangement: &DockArrangement,
        focus: DockFocus,
    ) {
        let tabs = |slot, state| {
            StyleQuery::new(ControlKind::Tabs, slot)
                .variant(TabsVariant::Line)
                .state(state)
        };
        let border = cx
            .style(StyleQuery::new(ControlKind::Separator, Slot::Root))
            .background;
        let frame = cx.style(StyleQuery::new(ControlKind::Dock, Slot::Root));
        for group in &arrangement.groups {
            cx.slot(group.rect, &frame, |_, _| {});
            let bar = group.tab_bar;
            fill_area(
                cx.list,
                Rect::new(
                    Point::new(bar.origin.x, bar.max_y() - 1.0),
                    Size::new(bar.size.width, 1.0),
                ),
                cx.rtl,
                &border,
            );
            for (index, panel) in group.tabs.iter().enumerate() {
                let rect = group.tab_rect(index, cx.rtl);
                let state = VisualState::NONE
                    .set(VisualState::SELECTED, *panel == group.active)
                    .set(VisualState::FOCUS_VISIBLE, focus == DockFocus::Tab(*panel));
                let trigger = cx.style(tabs(Slot::Trigger, state));
                let title = self.panel(*panel).map_or("", |panel| panel.title.as_str());
                cx.slot(rect, &trigger, |cx, inner| {
                    cx.label(inner, title, &trigger, TextAlign::Center);
                });
                let indicator = cx.style(tabs(Slot::Indicator, state));
                let line = Rect::new(
                    Point::new(rect.origin.x, rect.max_y() - indicator.min_height),
                    Size::new(rect.size.width, indicator.min_height),
                );
                fill_area(cx.list, line, cx.rtl, &indicator.background);
            }
        }
        for (area, rect) in &arrangement.splitters {
            let state = VisualState::NONE.set(
                VisualState::FOCUS_VISIBLE,
                focus == DockFocus::Splitter(*area),
            );
            let line = cx.style(StyleQuery::new(ControlKind::Resizable, Slot::Handle).state(state));
            let center = rect.center();
            let divider = if area.horizontal() {
                Rect::from_xywh(
                    center.x - line.min_width / 2.0,
                    rect.origin.y,
                    line.min_width,
                    rect.size.height,
                )
            } else {
                Rect::from_xywh(
                    rect.origin.x,
                    center.y - line.min_height / 2.0,
                    rect.size.width,
                    line.min_height,
                )
            };
            cx.slot(divider, &line, |_, _| {});
        }
    }

    /// Splitter semantics: an adjustable value in points.
    pub fn splitter_semantics(&self, area: DockArea) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Slider,
            name: format!(
                "{} panel size",
                match area {
                    DockArea::Left => "Left",
                    DockArea::Right => "Right",
                    DockArea::Top => "Top",
                    DockArea::Bottom => "Bottom",
                }
            ),
            description: String::new(),
            value: Some(format!("{}", self.extent(area).round())),
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Increment, SemanticAction::Decrement],
            text: None,
        }
    }

    /// Panel semantics: a tab panel named by its title, with Dismiss when
    /// closable.
    pub fn panel_semantics(&self, id: PanelId) -> Option<SemanticNode> {
        let panel = self.panels.get(&id)?;
        let mut actions = vec![SemanticAction::Focus];
        if panel.closable {
            actions.push(SemanticAction::Dismiss);
        }
        Some(SemanticNode {
            role: SemanticRole::TabPanel,
            name: panel.title.clone(),
            description: match self.location(id) {
                Some(PanelLocation::Floating(_)) => "Floating".into(),
                _ => String::new(),
            },
            value: None,
            state: SemanticState {
                selected: Some(self.is_active(id)),
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions,
            text: None,
        })
    }

    /// Serializes the layout (one line per record) for a `text` setting.
    pub fn save(&self) -> String {
        let mut out = String::from("dock 1\n");
        for area in DockArea::ALL {
            let state = &self.areas[area.index()];
            out.push_str(&format!("area {} {:?}\n", area.name(), state.extent));
            for group in &state.groups {
                out.push_str(&format!("group {:?}", group.weight));
                for (index, panel) in group.panels.iter().enumerate() {
                    out.push(' ');
                    if index == group.active {
                        out.push('*');
                    }
                    out.push_str(&self.panels[panel].key);
                }
                out.push('\n');
            }
        }
        for (panel, rect) in &self.floating {
            out.push_str(&format!(
                "float {} {:?} {:?} {:?} {:?}\n",
                self.panels[panel].key,
                rect.origin.x,
                rect.origin.y,
                rect.size.width,
                rect.size.height
            ));
        }
        for panel in self.hidden.keys() {
            out.push_str(&format!("hidden {}\n", self.panels[panel].key));
        }
        out
    }

    /// Restores a saved layout. Unknown keys are skipped; registered panels
    /// missing from the text keep their current location. On error the
    /// layout is unchanged.
    pub fn restore(&mut self, text: &str) -> Result<DockRestoreReport, DockError> {
        let mut next = self.clone();
        for area in &mut next.areas {
            area.groups.clear();
        }
        next.floating.clear();
        next.hidden.clear();
        let mut report = DockRestoreReport::default();
        let mut placed = std::collections::BTreeSet::new();
        let mut current_area = None;
        let mut lines = text.lines().enumerate();
        match lines.next() {
            Some((_, "dock 1")) => {}
            _ => {
                return Err(DockError::Parse {
                    line: 1,
                    message: "missing `dock 1` header".into(),
                });
            }
        }
        for (index, line) in lines {
            let line_no = index + 1;
            let error = |message: &str| DockError::Parse {
                line: line_no,
                message: message.into(),
            };
            let number = |text: Option<&str>| {
                text.and_then(|t| t.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| error("expected a number"))
            };
            let mut words = line.split_whitespace();
            match words.next() {
                None => {}
                Some("area") => {
                    let area = DockArea::parse(words.next().unwrap_or(""))
                        .ok_or_else(|| error("unknown area"))?;
                    next.areas[area.index()].extent = number(words.next())?.max(MIN_EXTENT);
                    current_area = Some(area);
                }
                Some("group") => {
                    let area = current_area.ok_or_else(|| error("group before area"))?;
                    let weight = number(words.next())?;
                    if weight <= 0.0 {
                        return Err(error("weight must be positive"));
                    }
                    let mut group = TabGroup {
                        panels: Vec::new(),
                        active: 0,
                        weight,
                    };
                    for word in words {
                        if let Some(id) = resolve(&next, &mut placed, word, &mut report) {
                            if word.starts_with('*') {
                                group.active = group.panels.len();
                            }
                            group.panels.push(id);
                        }
                    }
                    if !group.panels.is_empty() {
                        next.areas[area.index()].groups.push(group);
                    }
                }
                Some("float") => {
                    let key = words.next().ok_or_else(|| error("missing panel key"))?;
                    let (x, y, w, h) = (
                        number(words.next())?,
                        number(words.next())?,
                        number(words.next())?,
                        number(words.next())?,
                    );
                    if w <= 0.0 || h <= 0.0 {
                        return Err(error("floating size must be positive"));
                    }
                    if let Some(id) = resolve(&next, &mut placed, key, &mut report) {
                        next.floating
                            .push((id, Rect::new(Point::new(x, y), Size::new(w, h))));
                    }
                }
                Some("hidden") => {
                    let key = words.next().ok_or_else(|| error("missing panel key"))?;
                    if let Some(id) = resolve(&next, &mut placed, key, &mut report) {
                        let location = self.location(id).or_else(|| self.hidden.get(&id).copied());
                        next.hidden.insert(
                            id,
                            location.unwrap_or(PanelLocation::Docked {
                                area: DockArea::Right,
                                group: usize::MAX,
                            }),
                        );
                    }
                }
                Some(_) => return Err(error("unknown record")),
            }
        }
        // Panels the saved layout does not mention keep their location.
        for id in self.panels.keys().copied().collect::<Vec<_>>() {
            if placed.contains(&id) {
                continue;
            }
            report.unplaced.push(id);
            match self.location(id) {
                Some(PanelLocation::Docked { area, group }) => {
                    next.place(id, DockTarget::Tab { area, group })
                }
                Some(PanelLocation::Floating(rect)) => next.place(id, DockTarget::Floating(rect)),
                None => {
                    if let Some(location) = self.hidden.get(&id) {
                        next.hidden.insert(id, *location);
                    }
                }
            }
        }
        next.hosted = self.hosted;
        *self = next;
        Ok(report)
    }
}

fn resolve(
    layout: &DockLayout,
    placed: &mut std::collections::BTreeSet<PanelId>,
    key: &str,
    report: &mut DockRestoreReport,
) -> Option<PanelId> {
    let key = key.trim_start_matches('*');
    match layout.panel_by_key(key) {
        // A panel listed twice keeps its first placement.
        Some(id) if placed.insert(id) => Some(id),
        Some(_) => None,
        None => {
            report.unknown_keys.push(key.to_owned());
            None
        }
    }
}

fn split_edge(rect: Rect, area: DockArea, extent: f64) -> (Rect, Rect, Rect) {
    let Rect { origin, size } = rect;
    match area {
        DockArea::Left => (
            Rect::new(origin, Size::new(extent, size.height)),
            Rect::new(
                Point::new(origin.x + extent, origin.y),
                Size::new(SPLITTER, size.height),
            ),
            Rect::new(
                Point::new(origin.x + extent + SPLITTER, origin.y),
                Size::new(size.width - extent - SPLITTER, size.height),
            ),
        ),
        DockArea::Right => (
            Rect::new(
                Point::new(origin.x + size.width - extent, origin.y),
                Size::new(extent, size.height),
            ),
            Rect::new(
                Point::new(origin.x + size.width - extent - SPLITTER, origin.y),
                Size::new(SPLITTER, size.height),
            ),
            Rect::new(
                origin,
                Size::new(size.width - extent - SPLITTER, size.height),
            ),
        ),
        DockArea::Top => (
            Rect::new(origin, Size::new(size.width, extent)),
            Rect::new(
                Point::new(origin.x, origin.y + extent),
                Size::new(size.width, SPLITTER),
            ),
            Rect::new(
                Point::new(origin.x, origin.y + extent + SPLITTER),
                Size::new(size.width, size.height - extent - SPLITTER),
            ),
        ),
        DockArea::Bottom => (
            Rect::new(
                Point::new(origin.x, origin.y + size.height - extent),
                Size::new(size.width, extent),
            ),
            Rect::new(
                Point::new(origin.x, origin.y + size.height - extent - SPLITTER),
                Size::new(size.width, SPLITTER),
            ),
            Rect::new(
                origin,
                Size::new(size.width, size.height - extent - SPLITTER),
            ),
        ),
    }
}

/// The dock's focus, splitter drag, and geometry in a control host.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct HostState {
    /// The tab stop that has keyboard focus.
    focus: DockFocus,
    /// The splitter being dragged: its area, the pointer position along
    /// the drag axis at the press, and the area's extent then.
    drag: Option<(DockArea, f64, f64)>,
    /// The bounds and the direction from the last arrange.
    bounds: Rect,
    rtl: bool,
    /// The extent of each area before a double click on its splitter
    /// collapsed it, by area index.
    collapsed: [Option<f64>; 4],
}

impl DockLayout {
    /// The tab stops of `arrangement`: one per group, on its active tab,
    /// then one per splitter.
    fn stops(arrangement: &DockArrangement) -> Vec<DockFocus> {
        arrangement
            .groups
            .iter()
            .map(|group| DockFocus::Tab(group.active))
            .chain(
                arrangement
                    .splitters
                    .iter()
                    .map(|(area, _)| DockFocus::Splitter(*area)),
            )
            .collect()
    }

    /// The focused stop in `bounds`, or the first one when the focused
    /// part is gone.
    fn host_focus(&self, bounds: Rect) -> DockFocus {
        let arrangement = self.arrange(bounds);
        let stops = Self::stops(&arrangement);
        let focus = match self.hosted.focus {
            DockFocus::Tab(panel) => arrangement
                .groups
                .iter()
                .find(|group| group.tabs.contains(&panel))
                .map(|group| DockFocus::Tab(group.active)),
            other => Some(other).filter(|focus| stops.contains(focus)),
        };
        focus.or_else(|| stops.first().copied()).unwrap_or_default()
    }

    /// Selects `panel` and reports it when it was not active.
    fn host_activate(&mut self, cx: &mut crate::ControlContext<'_>, panel: PanelId) {
        self.hosted.focus = DockFocus::Tab(panel);
        cx.request_paint();
        cx.request_semantics();
        if !self.is_active(panel) && self.activate(panel).is_ok() {
            cx.request_layout();
            cx.emit(crate::ControlMessage::ItemActivated(panel.0 as usize));
        }
    }

    /// Sets the extent of `area` and reports a change.
    /// Collapses `area` to the minimum extent, or restores the extent that
    /// it had before a collapse.
    fn toggle_collapse(&mut self, cx: &mut crate::ControlContext<'_>, area: DockArea) {
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

    fn host_resize(&mut self, cx: &mut crate::ControlContext<'_>, area: DockArea, extent: f64) {
        let before = self.extent(area);
        self.set_extent(area, extent);
        self.report_extent(cx, area, before);
    }

    fn report_extent(&mut self, cx: &mut crate::ControlContext<'_>, area: DockArea, before: f64) {
        let after = self.extent(area);
        if after != before {
            cx.request_layout();
            cx.request_paint();
            cx.request_semantics();
            cx.emit(crate::ControlMessage::Changed(crate::ControlValue::Number(
                after,
            )));
        }
    }

    fn host_key(&mut self, cx: &mut crate::ControlContext<'_>, key: Key) -> EventReply {
        let arrangement = self.arrange(cx.bounds());
        let focus = self.host_focus(cx.bounds());
        if key == Key::Tab {
            let stops = Self::stops(&arrangement);
            let current = stops.iter().position(|stop| *stop == focus).unwrap_or(0);
            let back = cx.modifiers().shift;
            return match crate::hosted::tab_stop(current, stops.len(), back) {
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

    fn host_pointer(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        event: PointerEvent,
    ) -> EventReply {
        let along = |area: DockArea, point: Point| {
            if area.horizontal() { point.x } else { point.y }
        };
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => {
                let arrangement = self.arrange(cx.bounds());
                let rtl = self.hosted.rtl;
                for group in &arrangement.groups {
                    let tab = (0..group.tabs.len())
                        .find(|index| group.tab_rect(*index, rtl).contains(position));
                    if let Some(index) = tab {
                        self.host_activate(cx, group.tabs[index]);
                        return EventReply::Stop;
                    }
                }
                let splitter = arrangement
                    .splitters
                    .iter()
                    .find(|(_, rect)| rect.contains(position));
                if let Some((area, _)) = splitter
                    && cx.click_count() >= 2
                {
                    self.hosted.focus = DockFocus::Splitter(*area);
                    self.toggle_collapse(cx, *area);
                    return EventReply::Stop;
                }
                if let Some((area, _)) = splitter {
                    self.hosted.focus = DockFocus::Splitter(*area);
                    self.hosted.drag = Some((*area, along(*area, position), self.extent(*area)));
                    cx.capture_pointer(true);
                    cx.request_paint();
                    cx.request_semantics();
                    return EventReply::Stop;
                }
                EventReply::Continue
            }
            PointerEvent::Move { position } => {
                let Some((area, start, extent)) = self.hosted.drag else {
                    return EventReply::Continue;
                };
                let delta = along(area, position) - start;
                let grow = match area {
                    DockArea::Left | DockArea::Top => delta,
                    DockArea::Right | DockArea::Bottom => -delta,
                };
                self.host_resize(cx, area, extent + grow);
                EventReply::Stop
            }
            PointerEvent::Up { .. } if self.hosted.drag.is_some() => {
                self.hosted.drag = None;
                cx.capture_pointer(false);
                cx.request_paint();
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }
}

/// In a control host the dock's node covers its whole area. It paints the
/// group frames, the tab bars, and the splitters, and the view layer
/// places the center and the panel contents in the frames that
/// [`DockLayout::arrange`] returns for the node's bounds.
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

    fn event(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        event: &crate::ControlEvent,
    ) -> EventReply {
        match crate::hosted::route(cx, event) {
            Some(crate::hosted::Routed::Pointer(event)) => self.host_pointer(cx, event),
            Some(crate::hosted::Routed::Key(KeyEvent::Down { key, .. })) => self.host_key(cx, key),
            Some(crate::hosted::Routed::Cancel) => {
                if self.hosted.drag.take().is_some() {
                    cx.capture_pointer(false);
                }
                cx.request_paint();
                EventReply::Continue
            }
            _ => {
                if let crate::ControlEvent::FocusChanged(_) = event {
                    cx.request_paint();
                }
                EventReply::Continue
            }
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let arrangement = self.arrange(cx.bounds);
        let focus = if cx.state.focus_visible() {
            self.host_focus(cx.bounds)
        } else {
            DockFocus::None
        };
        DockLayout::paint(self, cx, &arrangement, focus);
    }

    fn semantics(&self) -> SemanticNode {
        let group = || crate::hosted::node(SemanticRole::Group, "", Vec::new());
        match self.host_focus(self.hosted.bounds) {
            DockFocus::Tab(panel) => self.panel_semantics(panel).unwrap_or_else(group),
            DockFocus::Splitter(area) => {
                let mut node = self.splitter_semantics(area);
                node.actions.push(SemanticAction::Focus);
                node
            }
            DockFocus::None => group(),
        }
    }

    fn perform(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
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
            (SemanticAction::Dismiss, DockFocus::Tab(panel)) => {
                let Ok(next) = self.close(panel) else {
                    return false;
                };
                self.hosted.focus = next.map_or(DockFocus::None, DockFocus::Tab);
                cx.request_layout();
                cx.request_paint();
                cx.request_semantics();
                cx.emit(crate::ControlMessage::Dismissed);
            }
            _ => return false,
        }
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.hosted.drag = None;
        }
    }
}
