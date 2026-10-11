//! Popup placement, the popup stack (dismissal and focus restoration),
//! tooltips, and custom-drawn popup menus.
//!
//! Dismissal rules follow macOS: Escape closes the topmost popup; a press
//! outside every open popup closes them all and is consumed (it does not
//! reach the control underneath); deactivating the window closes all
//! popups; closing restores keyboard focus to the popup's owner.

use crate::{CommandId, CommandRegistry};
use choscorui_core::{
    Color, DisplayList, Key, LayoutDirection, NodeId, Point, Rect, SemanticAction, SemanticNode,
    SemanticRole, SemanticState, Size, Tree, TreeError, fold_for_search,
};
use std::time::Duration;

/// Preferred side of the anchor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Placement {
    Below,
    Above,
    /// Submenus: beside the anchor, toward the reading direction's end.
    After,
    Before,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupPlacement {
    pub rect: Rect,
    pub placement: Placement,
}

/// Places a popup of `size` next to `anchor` inside `bounds` (the screen's
/// visible frame or the window). The preferred side flips when the popup
/// does not fit there but fits on the other side; the popup is then
/// shifted (never resized) to stay inside `bounds`. In right-to-left
/// layouts start alignment uses the anchor's right edge and `After` means
/// to the left.
pub fn place_popup(
    anchor: Rect,
    size: Size,
    bounds: Rect,
    preferred: Placement,
    direction: LayoutDirection,
) -> PopupPlacement {
    let rtl = direction == LayoutDirection::Rtl;
    let (preferred, opposite) = match (preferred, rtl) {
        (Placement::After, true) => (Placement::Before, Placement::After),
        (Placement::Before, true) => (Placement::After, Placement::Before),
        (Placement::Below, _) => (Placement::Below, Placement::Above),
        (Placement::Above, _) => (Placement::Above, Placement::Below),
        (Placement::After, false) => (Placement::After, Placement::Before),
        (Placement::Before, false) => (Placement::Before, Placement::After),
    };
    let origin_for = |placement: Placement| -> Point {
        let start_x = if rtl {
            anchor.origin.x + anchor.size.width - size.width
        } else {
            anchor.origin.x
        };
        match placement {
            Placement::Below => Point::new(start_x, anchor.origin.y + anchor.size.height),
            Placement::Above => Point::new(start_x, anchor.origin.y - size.height),
            Placement::After => Point::new(anchor.origin.x + anchor.size.width, anchor.origin.y),
            Placement::Before => Point::new(anchor.origin.x - size.width, anchor.origin.y),
        }
    };
    let fits = |placement: Placement| -> bool {
        let origin = origin_for(placement);
        match placement {
            Placement::Below | Placement::Above => {
                origin.y >= bounds.origin.y
                    && origin.y + size.height <= bounds.origin.y + bounds.size.height
            }
            Placement::After | Placement::Before => {
                origin.x >= bounds.origin.x
                    && origin.x + size.width <= bounds.origin.x + bounds.size.width
            }
        }
    };
    let placement = if fits(preferred) || !fits(opposite) {
        preferred
    } else {
        opposite
    };
    let origin = origin_for(placement);
    let clamp = |value: f64, low: f64, extent: f64, high_edge: f64| -> f64 {
        value.min(high_edge - extent).max(low)
    };
    let rect = Rect::new(
        Point::new(
            clamp(
                origin.x,
                bounds.origin.x,
                size.width,
                bounds.origin.x + bounds.size.width,
            ),
            clamp(
                origin.y,
                bounds.origin.y,
                size.height,
                bounds.origin.y + bounds.size.height,
            ),
        ),
        size,
    );
    PopupPlacement { rect, placement }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PopupId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
struct OpenPopup {
    id: PopupId,
    owner: NodeId,
    rect: Rect,
    /// Focus before the popup opened, restored on close.
    restore_focus: Option<NodeId>,
}

/// What a pointer press did to the popup stack.
#[derive(Clone, Debug, PartialEq)]
pub enum PopupPress {
    /// The press landed in this popup; route it there.
    Inside(PopupId),
    /// Popups were closed and the press is consumed.
    Dismissed(Vec<PopupId>),
    /// No popup is open; route normally.
    None,
}

#[derive(Default)]
pub struct PopupStack {
    open: Vec<OpenPopup>,
    next: u64,
}

impl PopupStack {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_open(&self) -> bool {
        !self.open.is_empty()
    }
    pub fn top(&self) -> Option<PopupId> {
        self.open.last().map(|popup| popup.id)
    }
    pub fn rect(&self, id: PopupId) -> Option<Rect> {
        self.open
            .iter()
            .find(|popup| popup.id == id)
            .map(|popup| popup.rect)
    }
    pub fn owner(&self, id: PopupId) -> Option<NodeId> {
        self.open
            .iter()
            .find(|popup| popup.id == id)
            .map(|popup| popup.owner)
    }

    /// Opens a popup owned by `owner` (which must exist).
    pub fn open(&mut self, tree: &Tree, owner: NodeId, rect: Rect) -> Result<PopupId, TreeError> {
        if !tree.contains(owner) {
            return Err(TreeError::MissingNode);
        }
        self.next += 1;
        let id = PopupId(self.next);
        self.open.push(OpenPopup {
            id,
            owner,
            rect,
            restore_focus: tree.focus(),
        });
        Ok(id)
    }

    pub fn reposition(&mut self, id: PopupId, rect: Rect) {
        if let Some(popup) = self.open.iter_mut().find(|popup| popup.id == id) {
            popup.rect = rect;
        }
    }

    /// Closes `id` and every popup opened after it, restoring focus to the
    /// focus that was current when `id` opened (if that node still exists).
    pub fn close(&mut self, tree: &mut Tree, id: PopupId) -> Vec<PopupId> {
        let Some(index) = self.open.iter().position(|popup| popup.id == id) else {
            return Vec::new();
        };
        let closed: Vec<_> = self.open.drain(index..).collect();
        if let Some(focus) = closed[0].restore_focus.filter(|node| tree.contains(*node)) {
            let _ = tree.set_focus(Some(focus));
        }
        closed.iter().rev().map(|popup| popup.id).collect()
    }

    pub fn close_all(&mut self, tree: &mut Tree) -> Vec<PopupId> {
        match self.open.first() {
            Some(first) => {
                let id = first.id;
                self.close(tree, id)
            }
            None => Vec::new(),
        }
    }

    /// Escape closes the topmost popup.
    pub fn escape(&mut self, tree: &mut Tree) -> Option<PopupId> {
        let top = self.top()?;
        self.close(tree, top);
        Some(top)
    }

    pub fn press(&mut self, tree: &mut Tree, point: Point) -> PopupPress {
        if self.open.is_empty() {
            return PopupPress::None;
        }
        if let Some(popup) = self
            .open
            .iter()
            .rev()
            .find(|popup| popup.rect.contains(point))
        {
            return PopupPress::Inside(popup.id);
        }
        PopupPress::Dismissed(self.close_all(tree))
    }

    /// Window lost key status or a popup's owner was removed.
    pub fn window_deactivated(&mut self, tree: &mut Tree) -> Vec<PopupId> {
        self.close_all(tree)
    }

    /// Closes popups whose owner left the tree.
    pub fn prune(&mut self, tree: &mut Tree) -> Vec<PopupId> {
        match self
            .open
            .iter()
            .position(|popup| !tree.contains(popup.owner))
        {
            Some(index) => {
                let id = self.open[index].id;
                self.close(tree, id)
            }
            None => Vec::new(),
        }
    }
}

/// Hover delay before a tooltip shows, and how long a second tooltip can
/// follow without delay while moving between controls.
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(600);
pub const TOOLTIP_GRACE: Duration = Duration::from_millis(400);

#[derive(Clone, Debug, PartialEq)]
pub struct VisibleTooltip {
    pub target: NodeId,
    pub text: String,
    pub anchor: Rect,
}

/// Hover tooltips with show delay, warm-up grace between targets, and
/// hiding on press, key, scroll, or leave.
#[derive(Default)]
pub struct TooltipController {
    hover: Option<(NodeId, String, Rect, Duration)>,
    visible: Option<VisibleTooltip>,
    last_hidden: Option<Duration>,
}

impl TooltipController {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn visible(&self) -> Option<&VisibleTooltip> {
        self.visible.as_ref()
    }

    /// Pointer entered or moved within `target`.
    pub fn hover(&mut self, target: NodeId, text: &str, anchor: Rect, now: Duration) {
        if text.is_empty() {
            self.leave(now);
            return;
        }
        if self
            .hover
            .as_ref()
            .is_some_and(|(current, ..)| *current == target)
        {
            return;
        }
        let warm = self.visible.is_some()
            || self
                .last_hidden
                .is_some_and(|hidden| now.saturating_sub(hidden) <= TOOLTIP_GRACE);
        self.visible = None;
        let start = if warm {
            now.saturating_sub(TOOLTIP_DELAY)
        } else {
            now
        };
        self.hover = Some((target, text.to_owned(), anchor, start));
        self.poll(now);
    }

    /// Pointer left every tooltip target.
    pub fn leave(&mut self, now: Duration) {
        if self.visible.take().is_some() {
            self.last_hidden = Some(now);
        }
        self.hover = None;
    }

    /// Press, key, or scroll: hide and do not reshow until the pointer
    /// moves to another target.
    pub fn interrupt(&mut self, now: Duration) {
        if self.visible.take().is_some() {
            self.last_hidden = Some(now);
        }
        if let Some(hover) = &mut self.hover {
            hover.3 = Duration::MAX;
        }
    }

    /// Shows the pending tooltip once its delay passed. Returns the time
    /// at which to poll again, if one is pending.
    pub fn poll(&mut self, now: Duration) -> Option<Duration> {
        let (target, text, anchor, start) = self.hover.as_ref()?;
        if *start == Duration::MAX || self.visible.is_some() {
            return None;
        }
        let due = start.saturating_add(TOOLTIP_DELAY);
        if now >= due {
            self.visible = Some(VisibleTooltip {
                target: *target,
                text: text.clone(),
                anchor: *anchor,
            });
            None
        } else {
            Some(due)
        }
    }

    pub fn semantics(&self) -> Option<SemanticNode> {
        let visible = self.visible.as_ref()?;
        Some(SemanticNode {
            role: SemanticRole::Tooltip,
            name: visible.text.clone(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: vec![choscorui_core::SemanticRelation::DescribedBy(
                visible.target,
            )],
            actions: Vec::new(),
            text: None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuItem {
    Action {
        label: String,
        command: CommandId,
        /// Display text such as "⌘C"; matching is done by the command
        /// registry.
        shortcut: Option<String>,
    },
    Submenu {
        label: String,
        items: Vec<MenuItem>,
    },
    Separator,
}

impl MenuItem {
    pub fn action(label: impl Into<String>, command: CommandId) -> Self {
        Self::Action {
            label: label.into(),
            command,
            shortcut: None,
        }
    }
    fn label(&self) -> Option<&str> {
        match self {
            Self::Action { label, .. } | Self::Submenu { label, .. } => Some(label),
            Self::Separator => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuResponse {
    None,
    Activated(CommandId),
    /// The menu (or its last submenu level) closed without a choice.
    Closed,
}

/// Custom-drawn popup menu backed by commands: disabled commands are
/// skipped by the keyboard and cannot activate; checked commands show a
/// check mark. Native context menus remain available through the macOS
/// backend.
#[derive(Clone, Debug)]
pub struct PopupMenu {
    items: Vec<MenuItem>,
    /// Highlighted index per open level, root first.
    path: Vec<Option<usize>>,
    row_height: f64,
    separator_height: f64,
}

impl PopupMenu {
    pub fn new(items: Vec<MenuItem>) -> Self {
        Self {
            items,
            path: vec![None],
            row_height: 22.0,
            separator_height: 9.0,
        }
    }

    pub fn depth(&self) -> usize {
        self.path.len()
    }

    fn level_items(&self, level: usize) -> &[MenuItem] {
        let mut items = &self.items[..];
        for index in self.path.iter().take(level) {
            match index.and_then(|index| items.get(index)) {
                Some(MenuItem::Submenu {
                    items: children, ..
                }) => items = children,
                _ => return &[],
            }
        }
        items
    }

    pub fn highlighted(&self) -> Option<usize> {
        *self.path.last().unwrap_or(&None)
    }

    fn enabled(item: &MenuItem, commands: &CommandRegistry) -> bool {
        match item {
            MenuItem::Action { command, .. } => commands
                .state(*command)
                .map(|state| state.enabled)
                .unwrap_or(false),
            MenuItem::Submenu { items, .. } => !items.is_empty(),
            MenuItem::Separator => false,
        }
    }

    fn step(&mut self, commands: &CommandRegistry, forward: bool) {
        let level = self.path.len() - 1;
        let items = self.level_items(level).to_vec();
        if items.is_empty() {
            return;
        }
        let count = items.len();
        let mut index = match (self.path[level], forward) {
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
            (None, true) => 0,
            (None, false) => count - 1,
        };
        for _ in 0..count {
            if Self::enabled(&items[index], commands) {
                self.path[level] = Some(index);
                return;
            }
            index = if forward {
                (index + 1) % count
            } else {
                (index + count - 1) % count
            };
        }
    }

    pub fn handle_key(&mut self, key: Key, commands: &CommandRegistry) -> MenuResponse {
        let level = self.path.len() - 1;
        match key {
            Key::ArrowDown => self.step(commands, true),
            Key::ArrowUp => self.step(commands, false),
            Key::Home => {
                self.path[level] = None;
                self.step(commands, true);
            }
            Key::End => {
                self.path[level] = None;
                self.step(commands, false);
            }
            Key::ArrowRight => self.open_submenu(commands),
            Key::ArrowLeft | Key::Escape => {
                if self.path.len() > 1 {
                    self.path.pop();
                } else {
                    return MenuResponse::Closed;
                }
            }
            Key::Enter | Key::Space => return self.activate_highlighted(commands),
            Key::Character(character) => {
                // Type-select: next enabled item starting with the letter.
                let items = self.level_items(level).to_vec();
                let needle = fold_for_search(&character.to_string());
                let start = self.path[level].map_or(0, |index| index + 1);
                for step in 0..items.len() {
                    let index = (start + step) % items.len();
                    if Self::enabled(&items[index], commands)
                        && items[index]
                            .label()
                            .is_some_and(|label| fold_for_search(label).starts_with(&needle))
                    {
                        self.path[level] = Some(index);
                        break;
                    }
                }
            }
            _ => {}
        }
        MenuResponse::None
    }

    fn open_submenu(&mut self, commands: &CommandRegistry) {
        let level = self.path.len() - 1;
        if let Some(index) = self.path[level]
            && let Some(MenuItem::Submenu { .. }) = self.level_items(level).get(index)
        {
            self.path.push(None);
            self.step(commands, true);
        }
    }

    fn activate_highlighted(&mut self, commands: &CommandRegistry) -> MenuResponse {
        let level = self.path.len() - 1;
        let Some(index) = self.path[level] else {
            return MenuResponse::None;
        };
        match self.level_items(level).get(index).cloned() {
            Some(MenuItem::Action { command, .. })
                if commands.state(command).is_ok_and(|s| s.enabled) =>
            {
                MenuResponse::Activated(command)
            }
            Some(MenuItem::Submenu { .. }) => {
                self.open_submenu(commands);
                MenuResponse::None
            }
            _ => MenuResponse::None,
        }
    }

    /// Size of level `level` for placement.
    pub fn level_size(&self, level: usize, width: f64) -> Size {
        let height = self
            .level_items(level)
            .iter()
            .map(|item| match item {
                MenuItem::Separator => self.separator_height,
                _ => self.row_height,
            })
            .sum::<f64>()
            + 8.0;
        Size::new(width, height)
    }

    fn item_at(&self, level: usize, origin: Point, point: Point) -> Option<usize> {
        let mut y = origin.y + 4.0;
        for (index, item) in self.level_items(level).iter().enumerate() {
            let height = match item {
                MenuItem::Separator => self.separator_height,
                _ => self.row_height,
            };
            if point.y >= y && point.y < y + height {
                return Some(index);
            }
            y += height;
        }
        None
    }

    /// Pointer hover highlights; release over an enabled action activates.
    pub fn pointer(
        &mut self,
        level: usize,
        origin: Point,
        point: Point,
        released: bool,
        commands: &CommandRegistry,
    ) -> MenuResponse {
        if level >= self.path.len() {
            return MenuResponse::None;
        }
        let Some(index) = self.item_at(level, origin, point) else {
            return MenuResponse::None;
        };
        let item = self.level_items(level)[index].clone();
        if !Self::enabled(&item, commands) {
            return MenuResponse::None;
        }
        self.path.truncate(level + 1);
        self.path[level] = Some(index);
        if matches!(item, MenuItem::Submenu { .. }) {
            self.path.push(None);
            return MenuResponse::None;
        }
        if released {
            return self.activate_highlighted(commands);
        }
        MenuResponse::None
    }

    pub fn semantics(&self, level: usize, commands: &CommandRegistry) -> Vec<SemanticNode> {
        self.level_items(level)
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let label = item.label()?;
                let state = match item {
                    MenuItem::Action { command, .. } => commands.state(*command).ok(),
                    _ => None,
                };
                Some(SemanticNode {
                    role: SemanticRole::MenuItem,
                    name: label.to_owned(),
                    description: match item {
                        MenuItem::Action {
                            shortcut: Some(shortcut),
                            ..
                        } => shortcut.clone(),
                        _ => String::new(),
                    },
                    value: None,
                    state: SemanticState {
                        disabled: !Self::enabled(item, commands),
                        checked: state.and_then(|state| state.checked.then_some(true)),
                        selected: Some(self.path.get(level) == Some(&Some(index))),
                        expanded: matches!(item, MenuItem::Submenu { .. }).then(|| {
                            self.path.len() > level + 1 && self.path[level] == Some(index)
                        }),
                        ..SemanticState::default()
                    },
                    relations: Vec::new(),
                    actions: if Self::enabled(item, commands) {
                        vec![SemanticAction::Activate]
                    } else {
                        Vec::new()
                    },
                    text: None,
                })
            })
            .collect()
    }

    pub fn paint(
        &self,
        list: &mut DisplayList,
        level: usize,
        rect: Rect,
        style: &MenuStyle,
        commands: &CommandRegistry,
    ) {
        let _ = list.draw_shadow(
            rect,
            6.0,
            choscorui_core::Shadow {
                offset: Point::new(0.0, 3.0),
                blur: 10.0,
                color: Color::rgba8(0, 0, 0, 70),
                spread: 0.0,
                inset: false,
            },
        );
        let _ = list.fill_rounded_rect(rect, 6.0, style.background);
        let mut y = rect.origin.y + 4.0;
        for (index, item) in self.level_items(level).iter().enumerate() {
            match item {
                MenuItem::Separator => {
                    list.fill_rect(
                        Rect::new(
                            Point::new(rect.origin.x + 8.0, y + self.separator_height / 2.0),
                            Size::new(rect.size.width - 16.0, 1.0),
                        ),
                        style.separator,
                    );
                    y += self.separator_height;
                }
                MenuItem::Action { label, .. } | MenuItem::Submenu { label, .. } => {
                    let row = Rect::new(
                        Point::new(rect.origin.x + 4.0, y),
                        Size::new(rect.size.width - 8.0, self.row_height),
                    );
                    let highlighted = self.path.get(level) == Some(&Some(index));
                    let enabled = Self::enabled(item, commands);
                    if highlighted {
                        let _ = list.fill_rounded_rect(row, 4.0, style.highlight);
                    }
                    let color = if !enabled {
                        style.disabled_text
                    } else if highlighted {
                        style.highlight_text
                    } else {
                        style.text
                    };
                    let baseline = y + self.row_height / 2.0 + style.font_size * 0.35;
                    if let MenuItem::Action {
                        command, shortcut, ..
                    } = item
                    {
                        if commands.state(*command).is_ok_and(|state| state.checked) {
                            let _ = list.draw_text(
                                Point::new(row.origin.x + 6.0, baseline),
                                "✓",
                                &style.font_family,
                                style.font_size,
                                color,
                            );
                        }
                        if let Some(shortcut) = shortcut {
                            let _ = list.draw_text(
                                Point::new(row.origin.x + row.size.width - 48.0, baseline),
                                shortcut,
                                &style.font_family,
                                style.font_size,
                                color,
                            );
                        }
                    } else {
                        let _ = list.draw_text(
                            Point::new(row.origin.x + row.size.width - 14.0, baseline),
                            "›",
                            &style.font_family,
                            style.font_size,
                            color,
                        );
                    }
                    let _ = list.draw_text(
                        Point::new(row.origin.x + 22.0, baseline),
                        label,
                        &style.font_family,
                        style.font_size,
                        color,
                    );
                    y += self.row_height;
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct MenuStyle {
    pub background: Color,
    pub highlight: Color,
    pub text: Color,
    pub highlight_text: Color,
    pub disabled_text: Color,
    pub separator: Color,
    pub font_family: String,
    pub font_size: f64,
}

impl Default for MenuStyle {
    fn default() -> Self {
        Self {
            background: Color::rgba8(248, 248, 250, 255),
            highlight: Color::rgba8(40, 110, 220, 255),
            text: Color::rgba8(25, 25, 30, 255),
            highlight_text: Color::rgba8(255, 255, 255, 255),
            disabled_text: Color::rgba8(160, 160, 165, 255),
            separator: Color::rgba8(215, 215, 220, 255),
            font_family: "Helvetica".into(),
            font_size: 13.0,
        }
    }
}
