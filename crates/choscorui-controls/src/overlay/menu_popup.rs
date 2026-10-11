//! A hosted dropdown menu: a [`DropdownMenu`] with its own command
//! states, panel placement for each open level, and the reveal
//! animation. `ContextMenu` and `Menubar` share it.

use crate::custom::{ControlContext, ControlMessage, MeasureContext};
use crate::hosted::Reveal;
use crate::menu::{DropdownMenu, MenuCheck, MenuItem, MenuResponse};
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, VisualState};
use crate::{CommandError, CommandId, CommandRegistry, CommandState};
use choscorui_core::{Key, Point, Rect};

/// An open or closed dropdown menu that a hosted control owns.
pub(crate) struct MenuPopup {
    pub menu: DropdownMenu,
    pub commands: CommandRegistry,
    pub reveal: Reveal,
    /// The top-start corner of the root panel, in window coordinates.
    anchor: Point,
    width: f64,
    rtl: bool,
    motion_ms: u32,
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
            anchor: Point::new(0.0, 0.0),
            width: 0.0,
            rtl: false,
            motion_ms: 0,
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
        self.rtl = cx.rtl();
        self.motion_ms = root.motion_ms;
    }

    /// Opens the menu with its root panel's top-start corner at
    /// `anchor`. A keyboard open highlights the first enabled item.
    pub fn open(&mut self, anchor: Point, time: f64, keyboard: bool) {
        self.menu.reset();
        self.anchor = anchor;
        if keyboard {
            self.menu.handle_key(Key::Home, &self.commands);
        }
        self.reveal.set(true, time);
    }

    pub fn close(&mut self, time: f64) {
        self.reveal.set(false, time);
    }

    /// The panel of each open level, root first.
    pub fn panels(&self) -> Vec<Rect> {
        let mut panels = Vec::new();
        let mut origin = self.anchor;
        let path = self.menu.path();
        for level in 0..self.menu.depth() {
            let size = self.menu.level_size(level, self.width);
            let x = if self.rtl {
                origin.x - size.width
            } else {
                origin.x
            };
            let panel = Rect::from_xywh(x, origin.y, size.width, size.height);
            panels.push(panel);
            if let Some(Some(index)) = path.get(level)
                && let Some(row) = self.menu.item_rect(level, panel, *index)
            {
                // A submenu opens beside its row, rows aligned.
                let side = if self.rtl {
                    panel.origin.x
                } else {
                    panel.max_x()
                };
                origin = Point::new(side, row.origin.y - self.menu.inset());
            }
        }
        panels
    }

    /// The rectangle around every open panel.
    pub fn frame(&self) -> Option<Rect> {
        if !self.reveal.open {
            return None;
        }
        self.panels().into_iter().reduce(|a, b| a.union(b))
    }

    /// Hover or a release at `point`. `None` when no panel contains it.
    pub fn pointer(&mut self, point: Point, released: bool) -> Option<MenuResponse> {
        if !self.reveal.open {
            return None;
        }
        let panels = self.panels();
        let level = panels.iter().rposition(|panel| panel.contains(point))?;
        Some(
            self.menu
                .pointer(level, panels[level].origin, point, released, &self.commands),
        )
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

    /// The label of the highlighted item of the deepest open level.
    pub fn highlighted_label(&self) -> Option<&str> {
        let level = self.menu.depth() - 1;
        let index = self.menu.highlighted()?;
        self.menu.level_items(level).get(index)?.label()
    }

    /// Paints every open panel with the reveal animation.
    pub fn paint(&self, cx: &mut PaintContext<'_>) {
        let progress = self.reveal.progress(cx.tokens, self.motion_ms, cx.time);
        if progress <= 0.0 {
            return;
        }
        let panels = self.panels();
        let Some(frame) = panels.iter().copied().reduce(|a, b| a.union(b)) else {
            return;
        };
        let slide = (0.0, -cx.tokens.spacing * 2.0);
        Reveal::paint(cx, frame, progress, slide, |cx| {
            for (level, panel) in panels.iter().enumerate() {
                let mut panel_cx = cx.at(*panel);
                panel_cx.state = VisualState::OPEN;
                self.menu.paint(&mut panel_cx, level, &self.commands);
            }
        });
    }

    pub fn animating(&self, time: f64) -> bool {
        self.reveal.animating(self.motion_ms, time)
    }
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
            MenuItem::Separator => {}
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
    let shortcut = cx.style(query(Slot::Shortcut));
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
                MenuItem::Action {
                    shortcut: Some(text),
                    ..
                } => item.gap + cx.text.width(text, &shortcut.text),
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
