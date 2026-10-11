//! Popups: a command-backed popup menu with a submenu, and hover tooltips.

use crate::page::{Align, Column, Env, Page, Push, node, pressed, sem};
use choscorui::controls::{
    CommandId, CommandRegistry, CommandState, DropdownMenu, MenuCheck, MenuItem, MenuResponse,
    Placement, PopupPress, PopupStack, ShortcutModifiers, TooltipController, place_popup,
};
use choscorui::core::{
    ColorRole, DisplayList, Key, KeyEvent, NodeId, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, Size, Tree,
};

const CUT: CommandId = CommandId(1);
const COPY: CommandId = CommandId(2);
const PASTE: CommandId = CommandId(3);
const WRAP: CommandId = CommandId(4);
const EMAIL: CommandId = CommandId(5);
const MESSAGES: CommandId = CommandId(6);
const MENU_WIDTH: f64 = 200.0;

fn menu_items() -> Vec<MenuItem> {
    let with_shortcut = |label: &str, command, shortcut: &str| {
        MenuItem::action(label, command).with_shortcut(shortcut)
    };
    vec![
        with_shortcut("Cut", CUT, "⌘X"),
        with_shortcut("Copy", COPY, "⌘C"),
        with_shortcut("Paste (nothing copied)", PASTE, "⌘V"),
        MenuItem::Separator,
        MenuItem::action("Wrap long lines", WRAP).with_check(MenuCheck::Checkbox),
        MenuItem::submenu(
            "Share",
            vec![
                MenuItem::action("Email", EMAIL),
                MenuItem::action("Messages", MESSAGES),
            ],
        ),
    ]
}

/// Labelled (non-separator) items per level.
const LEVEL_ITEMS: [usize; 2] = [5, 2];
/// The rows of the separator and of the Share submenu in the root level.
const SEPARATOR_ROW: usize = 3;
const SHARE_ROW: usize = 5;

/// A button that opens a popup menu; Paste is disabled and Wrap toggles
/// a check mark. Arrow keys, Return, and Escape work inside the menu.
pub struct DropdownMenuPage {
    button: Push,
    status: NodeId,
    levels: Vec<(NodeId, Vec<NodeId>)>,
    menu: DropdownMenu,
    stack: PopupStack,
    commands: CommandRegistry,
    rects: Vec<Rect>,
    bounds: Rect,
    last: String,
}

pub fn dropdown_menu(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let button = Push::new(tree, parent, "Edit ▾", env);
    let status = node(tree, parent, false);
    let levels = LEVEL_ITEMS
        .iter()
        .map(|count| {
            let container = node(tree, parent, true);
            let _ = tree.set_visible(container, false);
            let items = (0..*count).map(|_| node(tree, container, false)).collect();
            (container, items)
        })
        .collect();
    let mut commands = CommandRegistry::new();
    for command in [CUT, COPY, PASTE, WRAP, EMAIL, MESSAGES] {
        let _ = commands.insert(command, CommandState::default());
    }
    let _ = commands.set_enabled(PASTE, false);
    let _ = commands.set_checked(WRAP, true);
    Box::new(DropdownMenuPage {
        button,
        status,
        levels,
        menu: DropdownMenu::new(menu_items()),
        stack: PopupStack::new(),
        commands,
        rects: Vec::new(),
        bounds: Rect::default(),
        last: "Click the button or press Space on it.".into(),
    })
}

impl DropdownMenuPage {
    /// Opens the menu under the button.
    pub fn open(&mut self, tree: &mut Tree, env: &Env<'_>) {
        if self.stack.is_open() {
            return;
        }
        self.menu = DropdownMenu::new(menu_items());
        let anchor = tree.bounds(self.button.node).unwrap_or_default();
        let rect = place_popup(
            anchor,
            self.menu.level_size(0, MENU_WIDTH),
            self.bounds,
            Placement::Below,
            env.direction,
        )
        .rect;
        if self.stack.open(tree, self.button.node, rect).is_ok() {
            let _ = tree.set_focus(Some(self.levels[0].0));
        }
        self.place(tree, env);
    }

    fn close(&mut self, tree: &mut Tree) {
        self.stack.close_all(tree);
        self.place_hidden(tree);
    }

    fn place_hidden(&mut self, tree: &mut Tree) {
        self.rects.clear();
        for (container, _) in &self.levels {
            let _ = tree.set_visible(*container, false);
        }
    }

    /// Positions each open level next to its parent row.
    fn place(&mut self, tree: &mut Tree, env: &Env<'_>) {
        let Some(top) = self.stack.top() else {
            self.place_hidden(tree);
            return;
        };
        let root = self.stack.rect(top).unwrap_or_default();
        self.rects = vec![root];
        if self.menu.depth() > 1 {
            // The submenu opens beside the Share row (the last row).
            let size = self.menu.level_size(1, MENU_WIDTH);
            let share = self.menu.item_rect(0, root, SHARE_ROW).unwrap_or(root);
            self.rects
                .push(place_popup(share, size, self.bounds, Placement::After, env.direction).rect);
        }
        for (level, (container, items)) in self.levels.iter().enumerate() {
            let rect = self.rects.get(level).copied();
            let _ = tree.set_visible(*container, rect.is_some());
            let Some(rect) = rect else { continue };
            let _ = tree.set_bounds(*container, rect);
            // Separators have no node, so skip their rows.
            let rows = (0..).filter(|&row| !(level == 0 && row == SEPARATOR_ROW));
            for (id, row) in items.iter().zip(rows) {
                if let Some(bounds) = self.menu.item_rect(level, rect, row) {
                    let _ = tree.set_bounds(*id, bounds);
                }
            }
        }
        let depth = self.menu.depth().min(self.levels.len());
        let _ = tree.set_focus(Some(self.levels[depth - 1].0));
    }

    fn respond(&mut self, tree: &mut Tree, response: MenuResponse, env: &Env<'_>) -> bool {
        match response {
            MenuResponse::Activated(command) => {
                self.run(command);
                self.close(tree);
                true
            }
            MenuResponse::Closed => {
                if self.menu.depth() == 0 || self.rects.len() <= 1 {
                    self.close(tree);
                } else {
                    self.place(tree, env);
                }
                true
            }
            MenuResponse::None => {
                self.place(tree, env);
                true
            }
        }
    }

    fn run(&mut self, command: CommandId) {
        if command == WRAP {
            let checked = self.commands.state(WRAP).is_ok_and(|state| state.checked);
            let _ = self.commands.set_checked(WRAP, !checked);
        }
        let name = match command {
            CUT => "Cut",
            COPY => "Copy",
            PASTE => "Paste",
            WRAP => "Wrap long lines",
            EMAIL => "Share › Email",
            _ => "Share › Messages",
        };
        self.last = format!("Chose “{name}”");
    }

    /// Presses keys in the open menu, for scripted checks.
    pub fn key_in_menu(&mut self, tree: &mut Tree, key: Key, env: &Env<'_>) -> String {
        let response = self.menu.handle_key(key, &self.commands);
        self.respond(tree, response, env);
        self.last.clone()
    }
}

impl Page for DropdownMenuPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        self.bounds = bounds;
        self.menu.set_metrics(env.styles(), env.tokens);
        let mut column = Column::new(bounds, env);
        let cell = column.cells(30.0, &[110.0])[0];
        let _ = tree.set_bounds(self.button.node, cell);
        let status = column.row(24.0);
        let _ = tree.set_bounds(
            self.status,
            Rect::new(
                Point::new(status.origin.x, bounds.max_y() - 24.0),
                status.size,
            ),
        );
        if self.stack.is_open() {
            if let Some(top) = self.stack.top() {
                let rect = place_popup(
                    cell,
                    self.menu.level_size(0, MENU_WIDTH),
                    bounds,
                    Placement::Below,
                    env.direction,
                )
                .rect;
                self.stack.reposition(top, rect);
            }
            self.place(tree, env);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        self.button.paint(tree, list, env);
        env.caption(
            list,
            tree.bounds(self.status).unwrap_or_default(),
            &self.last,
        );
        for (level, rect) in self.rects.iter().enumerate() {
            self.menu
                .paint(&mut env.cx(list, *rect), level, &self.commands);
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let (id, mut button) = self.button.semantics(tree);
        button.state.expanded = Some(self.stack.is_open());
        let mut nodes = vec![
            (id, button),
            (
                self.status,
                sem(SemanticRole::StaticText, self.last.clone(), None),
            ),
        ];
        for (level, (container, items)) in self.levels.iter().enumerate() {
            let open = level < self.rects.len();
            let mut menu = sem(
                SemanticRole::Menu,
                if level == 0 { "Edit" } else { "Share" },
                None,
            );
            menu.state.hidden = !open;
            nodes.push((*container, menu));
            let semantics = if open {
                self.menu.semantics(level, &self.commands)
            } else {
                Vec::new()
            };
            for (index, id) in items.iter().enumerate() {
                let mut item = semantics
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| sem(SemanticRole::MenuItem, "Closed menu item", None));
                item.state.hidden = !open;
                nodes.push((*id, item));
            }
        }
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if !self.stack.is_open() {
            if self.button.pointer(tree, event) {
                self.open(tree, env);
                return true;
            }
            return false;
        }
        let point = event.position();
        let level = self.rects.iter().rposition(|rect| rect.contains(point));
        match event {
            PointerEvent::Down {
                button: PointerButton::Primary,
                ..
            } => match self.stack.press(tree, point) {
                PopupPress::Inside(_) => {
                    let level = level.unwrap_or(0);
                    let response = self.menu.pointer(
                        level,
                        self.rects[level].origin,
                        point,
                        false,
                        &self.commands,
                    );
                    self.respond(tree, response, env)
                }
                PopupPress::Dismissed(_) | PopupPress::None => {
                    self.place_hidden(tree);
                    true
                }
            },
            PointerEvent::Move { .. } | PointerEvent::Up { .. } => match level {
                Some(level) => {
                    let released = matches!(event, PointerEvent::Up { .. });
                    let response = self.menu.pointer(
                        level,
                        self.rects[level].origin,
                        point,
                        released,
                        &self.commands,
                    );
                    self.respond(tree, response, env)
                }
                None => false,
            },
            _ => false,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if !self.stack.is_open() {
            if self.button.key(tree, event) {
                self.open(tree, env);
                return true;
            }
            return false;
        }
        let Some(key) = pressed(event) else {
            return true;
        };
        // Left and right follow reading order for submenus.
        let key = match (key, env.rtl()) {
            (Key::ArrowLeft, true) => Key::ArrowRight,
            (Key::ArrowRight, true) => Key::ArrowLeft,
            (key, _) => key,
        };
        self.key_in_menu(tree, key, env);
        true
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        env: &Env<'_>,
    ) -> bool {
        if !self.stack.is_open() && self.button.action(tree, node, action) {
            self.open(tree, env);
            return true;
        }
        for (level, (_, items)) in self.levels.iter().enumerate() {
            if !items.contains(&node) {
                continue;
            }
            if action != SemanticAction::Activate || level >= self.rects.len() {
                return false;
            }
            let rect = tree.bounds(node).unwrap_or_default();
            let center = rect.center();
            let response = self.menu.pointer(
                level,
                self.rects[level].origin,
                center,
                true,
                &self.commands,
            );
            return self.respond(tree, response, env);
        }
        false
    }
}

const TIPS: [(&str, &str); 3] = [
    ("Bold", "Make the selection bold (⌘B)"),
    ("Italic", "Make the selection italic (⌘I)"),
    ("Underline", "Underline the selection (⌘U)"),
];

/// Buttons with hover tooltips: a delay before the first, a grace period
/// between neighbours, and hiding on press.
pub struct TooltipPage {
    pushes: Vec<Push>,
    tip: NodeId,
    tooltips: TooltipController,
    bounds: Rect,
}

pub fn tooltip(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let pushes = TIPS
        .iter()
        .map(|(label, _)| Push::new(tree, parent, *label, env))
        .collect();
    let tip = node(tree, parent, false);
    Box::new(TooltipPage {
        pushes,
        tip,
        tooltips: TooltipController::new(),
        bounds: Rect::default(),
    })
}

impl TooltipPage {
    /// Hovers button `index` at time `now`, for scripted checks.
    pub fn hover(&mut self, tree: &Tree, index: usize, now: std::time::Duration) {
        let anchor = tree.bounds(self.pushes[index].node).unwrap_or_default();
        self.tooltips
            .hover(self.pushes[index].node, TIPS[index].1, anchor, now);
    }

    fn tip_rect(&self, env: &Env<'_>) -> Option<Rect> {
        let tip = self.tooltips.visible()?;
        let size = Size::new(env.measure(&tip.text, env.small()) + 16.0, 24.0);
        Some(
            place_popup(
                tip.anchor,
                size,
                self.bounds,
                Placement::Below,
                env.direction,
            )
            .rect,
        )
    }
}

impl Page for TooltipPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        self.bounds = bounds;
        let mut column = Column::new(bounds, env);
        column.space(30.0);
        let cells = column.cells(30.0, &[100.0, 100.0, 100.0]);
        for (push, cell) in self.pushes.iter().zip(cells) {
            let _ = tree.set_bounds(push.node, cell);
        }
        let rect = self.tip_rect(env).unwrap_or_default();
        let _ = tree.set_bounds(self.tip, rect);
        let _ = tree.set_visible(self.tip, self.tooltips.visible().is_some());
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for push in &self.pushes {
            push.paint(tree, list, env);
        }
        env.caption(
            list,
            Rect::new(self.bounds.origin, Size::new(self.bounds.size.width, 20.0)),
            "Hover a button and wait; move to the next one to see the grace period.",
        );
        if let (Some(tip), Some(rect)) = (self.tooltips.visible(), self.tip_rect(env)) {
            let _ = list.fill_rounded_rect(rect, 4.0, env.tokens.color(ColorRole::Foreground));
            env.text(
                list,
                rect,
                &tip.text,
                env.small(),
                env.tokens.color(ColorRole::Background),
                Align::Center,
            );
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes: Vec<_> = self
            .pushes
            .iter()
            .zip(TIPS)
            .map(|(push, (_, tip))| {
                let (id, mut node) = push.semantics(tree);
                node.description = tip.into();
                (id, node)
            })
            .collect();
        let mut tip = self
            .tooltips
            .semantics()
            .unwrap_or_else(|| sem(SemanticRole::Tooltip, "No tooltip", None));
        tip.state.hidden = self.tooltips.visible().is_none();
        nodes.push((self.tip, tip));
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        let mut changed = false;
        for push in &mut self.pushes {
            changed |= push.pointer(tree, event);
        }
        let before = self.tooltips.visible().is_some();
        match event {
            PointerEvent::Move { position } => {
                match self
                    .pushes
                    .iter()
                    .position(|push| tree.bounds(push.node).is_some_and(|b| b.contains(position)))
                {
                    Some(index) => self.hover(tree, index, env.now),
                    None => self.tooltips.leave(env.now),
                }
            }
            PointerEvent::Down { .. } | PointerEvent::Wheel { .. } => {
                self.tooltips.interrupt(env.now);
            }
            PointerEvent::Up { .. } => {}
        }
        let _ = tree.set_visible(self.tip, self.tooltips.visible().is_some());
        changed || before != self.tooltips.visible().is_some()
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        self.tooltips.interrupt(env.now);
        let _ = tree.set_visible(self.tip, false);
        let mut changed = false;
        for push in &mut self.pushes {
            changed |= push.key(tree, event);
        }
        changed
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        self.pushes
            .iter_mut()
            .any(|push| push.action(tree, node, action))
    }

    #[cfg(target_os = "macos")]
    fn tick(&mut self, env: &Env<'_>) -> bool {
        let before = self.tooltips.visible().is_some();
        self.tooltips.poll(env.now);
        before != self.tooltips.visible().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::tests::Harness;
    use std::time::Duration;

    #[test]
    fn dropdown_menu_opens_from_its_button_and_escape_closes_it() {
        let mut h = Harness::new("dropdown-menu");
        let button = h.node("Edit ▾", None);
        h.act(button, SemanticAction::Activate);
        assert!(h.get::<DropdownMenuPage>().stack.is_open());
        h.press(Key::Escape);
        assert!(!h.get::<DropdownMenuPage>().stack.is_open());
    }

    #[test]
    fn tooltip_appears_only_after_the_hover_delay() {
        let mut h = Harness::new("tooltip");
        let Harness { tree, page, .. } = &mut h;
        let page = page.as_any_mut().downcast_mut::<TooltipPage>().unwrap();
        page.hover(tree, 0, Duration::ZERO);
        page.tooltips.poll(Duration::from_millis(50));
        assert!(page.tooltips.visible().is_none());
        page.tooltips.poll(Duration::from_secs(3));
        assert!(page.tooltips.visible().is_some());
    }
}
