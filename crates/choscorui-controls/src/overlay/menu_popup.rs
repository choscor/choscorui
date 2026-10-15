//! A hosted dropdown menu: a [`DropdownMenu`] with its own command
//! states, panel placement for each open level, and the reveal
//! animation. `ContextMenu` and `Menubar` share it.

use crate::custom::{ControlContext, ControlMessage, MeasureContext};
use crate::hosted::Reveal;
use crate::menu::{DropdownMenu, MenuCheck, MenuItem, MenuResponse};
use crate::popup::{Placement, place_popup};
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, VisualState};
use crate::{CommandError, CommandId, CommandRegistry, CommandState};
use choscorui_core::{
    Key, LayoutDirection, Point, PointerEvent, Rect, SemanticParts, SemanticRole, WheelUnit,
};

/// An open or closed dropdown menu that a hosted control owns.
pub(crate) struct MenuPopup {
    pub menu: DropdownMenu,
    pub commands: CommandRegistry,
    pub reveal: Reveal,
    /// What the root panel opens next to, in window coordinates. A menu
    /// in place puts its root panel at the top-start corner.
    anchor: Rect,
    /// True when the root panel stays in place, in the control's bounds.
    in_place: bool,
    /// The side of the anchor that the root panel prefers.
    side: Placement,
    /// The area that the panels fit in, from `popup_bounds`.
    bounds: Rect,
    width: f64,
    direction: LayoutDirection,
    motion_ms: u32,
    /// How far the rows of each open level scroll, root first. A level
    /// scrolls only when the window limits its panel.
    scroll: Vec<f64>,
    /// The last pointer position over a panel, for the path toward an
    /// open submenu.
    last: Option<Point>,
}

/// Where one open level is: the panel on screen, and the panel of all of
/// its rows, moved up by the scroll offset.
#[derive(Clone, Copy, Debug)]
struct LevelPanel {
    panel: Rect,
    rows: Rect,
}

impl MenuPopup {
    /// A closed menu. Every action command starts enabled and unchecked.
    pub fn new(items: Vec<MenuItem>) -> Self {
        let mut commands = CommandRegistry::new();
        register(&items, &mut commands);
        Self {
            menu: DropdownMenu::new(items),
            commands,
            reveal: Reveal::default(),
            anchor: Rect::default(),
            in_place: false,
            side: Placement::Below,
            bounds: Rect::from_xywh(0.0, 0.0, f64::MAX, f64::MAX),
            width: 0.0,
            direction: LayoutDirection::Ltr,
            motion_ms: 0,
            scroll: Vec::new(),
            last: None,
        }
    }

    /// A menu whose root panel stays at the top-start corner of its
    /// anchor, as in a [`crate::HostedDropdownMenu`]. Only its submenus
    /// are placed as popups.
    pub fn in_place(items: Vec<MenuItem>) -> Self {
        Self {
            in_place: true,
            ..Self::new(items)
        }
    }

    pub fn is_open(&self) -> bool {
        self.reveal.open
    }

    /// Sets the state of `command`.
    pub fn set_command(&mut self, command: CommandId, state: CommandState) -> CommandResult {
        self.commands.set_enabled(command, state.enabled)?;
        self.commands.set_checked(command, state.checked)
    }

    /// Reads the row metrics, the panel width, and the direction. Every
    /// level is as wide as the widest row, and at least `min_width`.
    pub fn arrange(&mut self, cx: &MeasureContext<'_>, min_width: f64) {
        self.menu.set_metrics(cx.styles, cx.tokens);
        let root = cx.style(StyleQuery::new(ControlKind::Menu, Slot::Root));
        let rows = widest(self.menu.items(), cx);
        self.width = (rows + root.padding.horizontal()).max(min_width);
        self.direction = cx.direction;
        self.menu.set_direction(cx.direction);
        self.menu.set_key_convention(cx.key_convention);
        self.bounds = cx.popup_bounds();
        self.motion_ms = root.motion_ms;
    }

    /// Opens the menu below `anchor`: a trigger grown by its gap, or an
    /// empty rectangle at the pointer. `place_popup` keeps every panel in
    /// the window. A keyboard open highlights the first enabled item.
    pub fn open(&mut self, anchor: Rect, time: f64, keyboard: bool) {
        self.open_beside(anchor, Placement::Below, time, keyboard);
    }

    /// Opens the menu on the `side` of `anchor` that it prefers, as
    /// [`MenuPopup::open`] does below it.
    pub fn open_beside(&mut self, anchor: Rect, side: Placement, time: f64, keyboard: bool) {
        self.side = side;
        self.menu.reset();
        self.scroll.clear();
        self.anchor = anchor;
        if keyboard {
            self.key(Key::Home, f64::NAN);
        }
        self.reveal.set(true, time);
    }

    pub fn close(&mut self, time: f64) {
        self.reveal.set(false, time);
    }

    /// The panel of each open level, root first. The root opens below
    /// its anchor, and a submenu beside its row with the rows aligned.
    /// Each panel flips and moves to stay inside the bounds. A panel
    /// taller than the bounds gets their height, and its rows scroll.
    pub fn panels(&self) -> Vec<Rect> {
        self.levels().into_iter().map(|level| level.panel).collect()
    }

    fn levels(&self) -> Vec<LevelPanel> {
        let mut levels = Vec::new();
        let mut anchor = self.anchor;
        let mut preferred = self.side;
        let path = self.menu.path();
        for level in 0..self.menu.depth() {
            let size = self.menu.level_size(level, self.width);
            let panel = if level == 0 && self.in_place {
                let x = if self.direction == LayoutDirection::Rtl {
                    anchor.max_x() - size.width
                } else {
                    anchor.origin.x
                };
                Rect::from_xywh(x, anchor.origin.y, size.width, size.height)
            } else {
                place_popup(anchor, size, self.bounds, preferred, self.direction).rect
            };
            let room = (size.height - panel.size.height).max(0.0);
            let offset = self
                .scroll
                .get(level)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, room);
            let rows = Rect::from_xywh(
                panel.origin.x,
                panel.origin.y - offset,
                panel.size.width,
                size.height,
            );
            levels.push(LevelPanel { panel, rows });
            if let Some(Some(index)) = path.get(level)
                && let Some(row) = self.menu.item_rect(level, rows, *index)
            {
                anchor = Rect::from_xywh(
                    panel.origin.x,
                    row.origin.y - self.menu.inset(),
                    panel.size.width,
                    row.size.height,
                );
                preferred = Placement::After;
            }
        }
        levels
    }

    /// A key for the open menu at host `time`. The highlighted row of
    /// each level scrolls into view.
    pub fn key(&mut self, key: Key, time: f64) -> MenuResponse {
        let response = self.menu.handle_key_at(key, &self.commands, time);
        self.reveal_highlight();
        response
    }

    /// Scrolls each open level so that its highlighted row shows inside
    /// the panel padding.
    fn reveal_highlight(&mut self) {
        let depth = self.menu.depth();
        self.scroll.resize(depth, 0.0);
        let inset = self.menu.inset();
        for level in 0..depth {
            let levels = self.levels();
            let LevelPanel { panel, rows } = levels[level];
            let offset = panel.origin.y - rows.origin.y;
            let Some(Some(index)) = self.menu.path().get(level).copied() else {
                self.scroll[level] = offset;
                continue;
            };
            let Some(row) = self.menu.item_rect(level, rows, index) else {
                continue;
            };
            let (top, bottom) = (panel.origin.y + inset, panel.max_y() - inset);
            self.scroll[level] = if row.origin.y < top {
                offset - (top - row.origin.y)
            } else if row.max_y() > bottom {
                offset + (row.max_y() - bottom)
            } else {
                offset
            };
        }
    }

    /// Scrolls the rows of the panel under a wheel event. Returns true
    /// when the event is over an open panel, so the menu takes it.
    pub fn wheel(&mut self, event: &PointerEvent) -> bool {
        let PointerEvent::Wheel {
            position,
            delta,
            unit,
        } = *event
        else {
            return false;
        };
        if !self.reveal.open {
            return false;
        }
        let levels = self.levels();
        let Some(level) = levels
            .iter()
            .rposition(|level| level.panel.contains(position))
        else {
            return false;
        };
        let LevelPanel { panel, rows } = levels[level];
        let step = match unit {
            WheelUnit::Points => delta.y,
            WheelUnit::Lines => delta.y * self.menu.row_height(),
        };
        let room = (rows.size.height - panel.size.height).max(0.0);
        let offset = panel.origin.y - rows.origin.y;
        self.scroll.resize(self.menu.depth(), 0.0);
        self.scroll[level] = (offset + step).clamp(0.0, room);
        true
    }

    /// The rectangle around every open panel.
    pub fn frame(&self) -> Option<Rect> {
        if !self.reveal.open {
            return None;
        }
        self.panels().into_iter().reduce(|a, b| a.union(b))
    }

    /// Hover or a release at `point`. `None` when no panel contains it;
    /// a hover there clears the highlight of the deepest level. While the
    /// pointer moves toward an open submenu, the rows that it crosses do
    /// not take the highlight.
    pub fn pointer(&mut self, point: Point, released: bool) -> Option<MenuResponse> {
        if !self.reveal.open {
            return None;
        }
        let levels = self.levels();
        let Some(level) = levels.iter().rposition(|level| level.panel.contains(point)) else {
            self.last = None;
            if !released {
                self.menu.leave();
            }
            return None;
        };
        let last = self.last.replace(point);
        if let (false, Some(last), Some(submenu)) = (released, last, levels.get(level + 1))
            && toward(last, point, submenu.panel)
        {
            return Some(MenuResponse::None);
        }
        let response = self.menu.pointer(
            level,
            levels[level].rows.origin,
            point,
            released,
            &self.commands,
        );
        // A submenu that opens starts at its first row.
        self.scroll.truncate(level + 1);
        Some(response)
    }

    /// Acts on a menu response: an activated command updates its check,
    /// reports `Command`, and closes the menu. A closed root reports
    /// `Dismissed`. Returns true when the menu closed.
    pub fn finish(&mut self, cx: &mut ControlContext<'_>, response: MenuResponse) -> bool {
        match response {
            MenuResponse::None => false,
            MenuResponse::Activated(command) => {
                self.toggle_check(command);
                cx.emit(ControlMessage::Command(command));
                self.close(cx.time());
                true
            }
            MenuResponse::Closed => {
                self.close(cx.time());
                cx.emit(ControlMessage::Dismissed);
                true
            }
        }
    }

    /// A checkbox action flips its check. A radio action checks itself
    /// and clears the other radio actions between the same separators.
    fn toggle_check(&mut self, command: CommandId) {
        let Some((items, index)) = find(self.menu.items(), command) else {
            return;
        };
        match items[index].check() {
            MenuCheck::None => {}
            MenuCheck::Checkbox => {
                let checked = self.commands.state(command).is_ok_and(|s| s.checked);
                let _ = self.commands.set_checked(command, !checked);
            }
            MenuCheck::Radio => {
                let start = items[..index]
                    .iter()
                    .rposition(|item| *item == MenuItem::Separator)
                    .map_or(0, |at| at + 1);
                let end = items[index..]
                    .iter()
                    .position(|item| *item == MenuItem::Separator)
                    .map_or(items.len(), |at| index + at);
                let group: Vec<CommandId> = items[start..end]
                    .iter()
                    .filter_map(|item| match item {
                        MenuItem::Action {
                            command,
                            check: MenuCheck::Radio,
                            ..
                        } => Some(*command),
                        _ => None,
                    })
                    .collect();
                for other in group {
                    let _ = self.commands.set_checked(other, other == command);
                }
            }
        }
    }

    /// Adds each open level as a `Menu` part, under `parent` for the root
    /// and under the submenu item that opened it for the others, with its
    /// items. A menu in place puts its root items under `parent`. The
    /// highlighted item of the deepest level gets the focus.
    pub fn semantic_parts(&self, parent: Option<usize>, parts: &mut SemanticParts) {
        if !self.is_open() && !self.in_place {
            return;
        }
        let mut parent = parent;
        let mut focused = None;
        for (level, LevelPanel { panel, rows }) in self.levels().into_iter().enumerate() {
            let menu = if level == 0 && self.in_place {
                parent
            } else {
                parts.push(parent, SemanticRole::Menu, "", panel);
                Some(parts.len() - 1)
            };
            focused = self
                .menu
                .semantic_parts(level, &self.commands, rows, menu, parts);
            parent = focused;
        }
        if focused.is_some() {
            parts.set_focused(focused);
        }
    }

    /// Paints every open panel with the reveal animation.
    pub fn paint(&self, cx: &mut PaintContext<'_>) {
        let progress = self.reveal.progress(cx.tokens, self.motion_ms, cx.time);
        if progress <= 0.0 {
            return;
        }
        let levels = self.levels();
        let Some(frame) = levels
            .iter()
            .map(|level| level.panel)
            .reduce(|a, b| a.union(b))
        else {
            return;
        };
        let slide = (0.0, -cx.tokens.spacing * 2.0);
        Reveal::paint(cx, frame, progress, slide, |cx| {
            for (level, LevelPanel { panel, rows }) in levels.iter().enumerate() {
                let mut panel_cx = cx.at(*panel);
                panel_cx.state = VisualState::OPEN;
                self.menu
                    .paint_scrolled(&mut panel_cx, level, &self.commands, *rows);
            }
        });
    }

    pub fn animating(&self, time: f64) -> bool {
        self.reveal.animating(self.motion_ms, time)
    }
}

/// Whether the pointer that moved from `from` to `to` heads into
/// `panel`: `to` is inside the triangle from `from` to the near edge of
/// the panel.
fn toward(from: Point, to: Point, panel: Rect) -> bool {
    let x = if panel.origin.x >= from.x {
        panel.origin.x
    } else {
        panel.max_x()
    };
    let corners = [
        from,
        Point::new(x, panel.origin.y),
        Point::new(x, panel.max_y()),
    ];
    let side = |a: Point, b: Point| (b.x - a.x) * (to.y - a.y) - (b.y - a.y) * (to.x - a.x);
    let sides = [
        side(corners[0], corners[1]),
        side(corners[1], corners[2]),
        side(corners[2], corners[0]),
    ];
    to != from && (sides.iter().all(|s| *s >= 0.0) || sides.iter().all(|s| *s <= 0.0))
}

/// The result of changing a command state.
pub(crate) type CommandResult = Result<(), CommandError>;

fn register(items: &[MenuItem], commands: &mut CommandRegistry) {
    for item in items {
        match item {
            MenuItem::Action { command, .. } => {
                // A command listed twice keeps one state.
                let _ = commands.insert(*command, CommandState::default());
            }
            MenuItem::Submenu { items, .. } => register(items, commands),
            MenuItem::Separator | MenuItem::Label(_) => {}
        }
    }
}

/// The level that holds the action for `command`, and its index.
fn find(items: &[MenuItem], command: CommandId) -> Option<(&[MenuItem], usize)> {
    for (index, item) in items.iter().enumerate() {
        match item {
            MenuItem::Action { command: id, .. } if *id == command => return Some((items, index)),
            MenuItem::Submenu { items: inner, .. } => {
                if let Some(found) = find(inner, command) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// The widest row content of `items` and of every submenu level, with
/// the row padding.
fn widest(items: &[MenuItem], cx: &MeasureContext<'_>) -> f64 {
    let query = |slot| StyleQuery::new(ControlKind::Menu, slot);
    let item = cx.style(query(Slot::Item));
    let shortcut_style = cx.style(query(Slot::Shortcut));
    let indicator = cx.style(query(Slot::ItemIndicator));
    let indent = if items.iter().any(|item| item.check() != MenuCheck::None) {
        indicator.min_width + item.gap
    } else {
        0.0
    };
    items
        .iter()
        .map(|entry| {
            let label = entry
                .label()
                .map_or(0.0, |label| cx.text.width(label, &item.text));
            let end = match entry {
                MenuItem::Action { shortcut, icon, .. } => {
                    let keys = shortcut.as_ref().map_or(0.0, |text| {
                        let text = crate::widgets::display_shortcut(text, cx.key_convention);
                        item.gap + cx.text.width(&text, &shortcut_style.text)
                    });
                    keys + icon.map_or(0.0, |_| item.icon_size + item.gap)
                }
                MenuItem::Submenu { items, .. } => {
                    // The submenu panels share the width.
                    return widest(items, cx).max(
                        item.padding.horizontal() + indent + label + item.gap + item.icon_size,
                    );
                }
                _ => 0.0,
            };
            item.padding.horizontal() + indent + label + end
        })
        .fold(0.0, f64::max)
}
