//! Window shell pieces: toolbars, the status bar, and main-window
//! composition.

use crate::page::{Align, Column, Env, Page, Push, node, pressed, sem};
use choscorui::controls::{
    Button, ButtonVariant, CommandId, CommandRegistry, CommandState, InputText, ShortcutModifiers,
    StatusBar, Toolbar, ToolbarArea, ToolbarItem, ToolbarItemKind, ToolbarLayout, VisualState,
    compose_main_window,
};
use choscorui::core::{
    ColorRole, DisplayList, Key, KeyEvent, NodeId, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, Size, Tree,
};
use std::time::Duration;

const NEW: CommandId = CommandId(1);
const OPEN: CommandId = CommandId(2);
const SAVE: CommandId = CommandId(3);
const UNDO: CommandId = CommandId(4);
const REDO: CommandId = CommandId(5);
const SHARE: CommandId = CommandId(6);

fn editor_commands() -> CommandRegistry {
    let mut commands = CommandRegistry::new();
    for command in [NEW, OPEN, SAVE, UNDO, REDO, SHARE] {
        let _ = commands.insert(command, CommandState::default());
    }
    // Nothing to redo yet.
    let _ = commands.set_enabled(REDO, false);
    commands
}

fn editor_toolbar(search: NodeId) -> Toolbar {
    Toolbar::new(
        "Editor",
        vec![
            ToolbarItem::command("new", "New", NEW),
            ToolbarItem::command("open", "Open", OPEN),
            ToolbarItem::command("save", "Save", SAVE),
            ToolbarItem::separator("sep"),
            ToolbarItem::command("undo", "Undo", UNDO),
            ToolbarItem::command("redo", "Redo", REDO),
            ToolbarItem::flexible_space("space"),
            ToolbarItem::control("search", "Search", search, 150.0),
            ToolbarItem::command("share", "Share", SHARE),
        ],
    )
}

fn command_name(command: CommandId) -> &'static str {
    match command {
        NEW => "New",
        OPEN => "Open",
        SAVE => "Save",
        UNDO => "Undo",
        REDO => "Redo",
        _ => "Share",
    }
}

/// Paints a toolbar, then a placeholder field for each embedded control.
fn paint_toolbar(
    list: &mut DisplayList,
    env: &Env<'_>,
    toolbar: &Toolbar,
    layout: &ToolbarLayout,
    commands: &CommandRegistry,
    focus_visible: bool,
) {
    toolbar.paint(
        &mut env.cx(list, layout.rect),
        layout,
        commands,
        focus_visible,
    );
    for (index, rect) in &layout.items {
        if let ToolbarItemKind::Control { label, .. } = &toolbar.items()[*index].kind {
            let field = rect.inset(2.0);
            InputText::new("")
                .placeholder(&format!("{label}…"))
                .paint(&mut env.cx(list, field), VisualState::NONE);
        }
    }
}

/// A customizable toolbar: commands, a disabled command, an embedded
/// search control, overflow when narrow, and placement on any edge. Arrow
/// keys move focus; Return runs the focused command.
pub struct ToolbarPage {
    bar: NodeId,
    search: NodeId,
    area_buttons: Vec<(Push, ToolbarArea)>,
    narrow: Push,
    toolbar: Toolbar,
    commands: CommandRegistry,
    status: NodeId,
    host: Rect,
    narrow_mode: bool,
    last: String,
}

pub fn toolbar(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let area_buttons = [
        ("Top", ToolbarArea::Top),
        ("Bottom", ToolbarArea::Bottom),
        ("Left", ToolbarArea::Left),
        ("Right", ToolbarArea::Right),
    ]
    .into_iter()
    .map(|(label, area)| (Push::new(tree, parent, label, env), area))
    .collect();
    let narrow = Push::new(tree, parent, "Narrow window", env);
    let bar = node(tree, parent, true);
    let search = node(tree, bar, false);
    let status = node(tree, parent, false);
    Box::new(ToolbarPage {
        bar,
        search,
        area_buttons,
        narrow,
        toolbar: editor_toolbar(search),
        commands: editor_commands(),
        status,
        host: Rect::default(),
        narrow_mode: false,
        last: "Click the toolbar, then use the arrow keys.".into(),
    })
}

impl ToolbarPage {
    fn frames(&self) -> (Rect, Rect) {
        let width = if self.narrow_mode {
            self.host.size.width.min(300.0)
        } else {
            self.host.size.width
        };
        let host = Rect::new(self.host.origin, Size::new(width, self.host.size.height));
        let frames =
            compose_main_window(host, &[(self.toolbar.area(), self.toolbar.thickness)], None);
        (host, frames.toolbars[0])
    }

    fn layout_bar(&self, env: &Env<'_>) -> ToolbarLayout {
        self.toolbar.layout(self.frames().1, env.direction)
    }

    fn place(&self, tree: &mut Tree, env: &Env<'_>) {
        let (_, bar) = self.frames();
        let _ = tree.set_bounds(self.bar, bar);
        let layout = self.layout_bar(env);
        let search = layout.items.iter().find(|(index, _)| {
            matches!(
                self.toolbar.items()[*index].kind,
                ToolbarItemKind::Control { .. }
            )
        });
        let _ = tree.set_visible(self.search, search.is_some());
        let _ = tree.set_bounds(
            self.search,
            search.map(|(_, rect)| *rect).unwrap_or_default(),
        );
    }

    fn run(&mut self, command: CommandId) {
        if command == UNDO {
            let _ = self.commands.set_enabled(REDO, true);
        }
        self.last = format!("Ran “{}”", command_name(command));
    }
}

impl Page for ToolbarPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(30.0, &[80.0, 80.0, 80.0, 80.0, 130.0]);
        for ((push, _), cell) in self.area_buttons.iter().zip(&cells) {
            let _ = tree.set_bounds(push.node, *cell);
        }
        let _ = tree.set_bounds(self.narrow.node, cells[4]);
        let _ = tree.set_bounds(self.status, column.row(22.0));
        let mut host = column.rest();
        host.size.height = host.size.height.min(300.0);
        host.size.width = host.size.width.min(720.0);
        self.host = host;
        self.place(tree, env);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (push, area) in &self.area_buttons {
            let rect = tree.bounds(push.node).unwrap_or_default();
            let variant = if *area == self.toolbar.area() {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Outline
            };
            Button::new(push.label.clone(), CommandId(0))
                .with_variant(variant)
                .paint(&mut env.cx(list, rect), tree, push.node);
        }
        self.narrow.paint(tree, list, env);
        env.caption(
            list,
            tree.bounds(self.status).unwrap_or_default(),
            &self.last,
        );
        let (host, _) = self.frames();
        env.panel(list, host);
        let layout = self.layout_bar(env);
        paint_toolbar(
            list,
            env,
            &self.toolbar,
            &layout,
            &self.commands,
            tree.focus_visible(self.bar),
        );
        env.caption(
            list,
            Rect::new(
                Point::new(host.origin.x + 12.0, host.max_y() - 26.0),
                Size::new(host.size.width - 24.0, 20.0),
            ),
            &format!("Saved as “{}”", self.toolbar.save()),
        );
    }

    fn semantics(&self, tree: &Tree, env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes: Vec<_> = self
            .area_buttons
            .iter()
            .map(|(push, _)| push.semantics(tree))
            .collect();
        nodes.push(self.narrow.semantics(tree));
        let mut bar = self.toolbar.semantics();
        let layout = self.layout_bar(env);
        bar.value =
            self.toolbar
                .focused()
                .and_then(|index| match &self.toolbar.items()[index].kind {
                    ToolbarItemKind::Command { label, .. } => Some(label.clone()),
                    _ => None,
                });
        bar.description = format!(
            "{} items, {} in overflow",
            layout.items.len(),
            layout.overflow.len()
        );
        bar.actions = vec![SemanticAction::Focus];
        nodes.push((self.bar, bar));
        let mut search = sem(SemanticRole::TextField, "Search", Some(String::new()));
        search.state.hidden = tree.visible(self.search) == Some(false);
        nodes.push((self.search, search));
        nodes.push((
            self.status,
            sem(SemanticRole::StaticText, self.last.clone(), None),
        ));
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if let Some(area) = self
            .area_buttons
            .iter_mut()
            .find_map(|(push, area)| push.pointer(tree, event).then_some(*area))
        {
            self.toolbar.move_to(area);
            self.place(tree, env);
            return true;
        }
        if self.narrow.pointer(tree, event) {
            self.narrow_mode = !self.narrow_mode;
            self.narrow.label = if self.narrow_mode {
                "Wide window"
            } else {
                "Narrow window"
            }
            .into();
            self.place(tree, env);
            return true;
        }
        let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = event
        else {
            return false;
        };
        let layout = self.layout_bar(env);
        if !layout.rect.contains(position) {
            return false;
        }
        let _ = tree.set_focus(Some(self.bar));
        if layout
            .overflow_button
            .is_some_and(|chevron| chevron.contains(position))
        {
            let hidden: Vec<String> = self
                .toolbar
                .overflow_menu(&layout, SHARE)
                .iter()
                .filter_map(|item| match item {
                    choscorui::controls::MenuItem::Action { label, .. } => Some(label.clone()),
                    _ => None,
                })
                .collect();
            self.last = format!("Overflow menu: {}", hidden.join(", "));
            return true;
        }
        let hit = layout
            .items
            .iter()
            .find(|(_, rect)| rect.contains(position))
            .map(|(index, _)| *index);
        if let Some(index) = hit
            && let ToolbarItemKind::Command { command, .. } = self.toolbar.items()[index].kind
            && let Ok(Some(command)) = self.commands.activate(command)
        {
            self.run(command);
        }
        true
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if let Some(area) = self
            .area_buttons
            .iter_mut()
            .find_map(|(push, area)| push.key(tree, event).then_some(*area))
        {
            self.toolbar.move_to(area);
            self.place(tree, env);
            return true;
        }
        if self.narrow.key(tree, event) {
            self.narrow_mode = !self.narrow_mode;
            self.place(tree, env);
            return true;
        }
        if tree.focus() != Some(self.bar) {
            return false;
        }
        let Some(key) = pressed(event) else {
            return false;
        };
        if key == Key::Tab {
            return false;
        }
        let layout = self.layout_bar(env);
        if self.toolbar.focused().is_none() && key != Key::Home {
            self.toolbar
                .handle_key(Key::Home, &layout, &self.commands, env.direction);
        }
        if let Some(command) = self
            .toolbar
            .handle_key(key, &layout, &self.commands, env.direction)
        {
            self.run(command);
        }
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
        if let Some(area) = self
            .area_buttons
            .iter_mut()
            .find_map(|(push, area)| push.action(tree, node, action).then_some(*area))
        {
            self.toolbar.move_to(area);
            self.place(tree, env);
            return true;
        }
        if self.narrow.action(tree, node, action) {
            self.narrow_mode = !self.narrow_mode;
            self.place(tree, env);
            return true;
        }
        node == self.bar && action == SemanticAction::Focus && tree.set_focus(Some(node)).is_ok()
    }
}

/// A status bar with permanent sections and a temporary message that
/// expires after three seconds (and is announced to VoiceOver).
pub struct StatusBarPage {
    show: Push,
    words: Push,
    bar: NodeId,
    status: StatusBar,
    count: usize,
    width: f64,
    origin: Point,
}

pub fn status_bar(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let show = Push::new(tree, parent, "Show message", env);
    let words = Push::new(tree, parent, "Add words", env);
    let bar = node(tree, parent, false);
    let mut status = StatusBar::new();
    status.set_default_text("Ready");
    status.set_section("words", "120 words", 110.0);
    status.set_section("encoding", "UTF-8", 70.0);
    status.set_section("position", "Ln 4, Col 12", 110.0);
    Box::new(StatusBarPage {
        show,
        words,
        bar,
        status,
        count: 120,
        width: 0.0,
        origin: Point::new(0.0, 0.0),
    })
}

impl StatusBarPage {
    /// Shows the temporary message at `now`; returns the bar text.
    pub fn show_message(&mut self, now: Duration) -> String {
        self.status
            .show_message("Saved “Notes.txt”", Some(Duration::from_secs(3)), now);
        self.status.text().to_owned()
    }
}

impl Page for StatusBarPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(30.0, &[130.0, 110.0]);
        let _ = tree.set_bounds(self.show.node, cells[0]);
        let _ = tree.set_bounds(self.words.node, cells[1]);
        self.width = bounds.size.width.min(680.0);
        self.origin = Point::new(bounds.origin.x, column.y + 20.0);
        let _ = tree.set_bounds(
            self.bar,
            Rect::new(self.origin, Size::new(self.width, self.status.height)),
        );
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        self.show.paint(tree, list, env);
        self.words.paint(tree, list, env);
        let rect = tree.bounds(self.bar).unwrap_or_default();
        let layout = self.status.layout(rect, env.direction);
        self.status.paint(&mut env.cx(list, rect), &layout);
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        vec![
            self.show.semantics(tree),
            self.words.semantics(tree),
            (self.bar, self.status.semantics()),
        ]
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if self.show.pointer(tree, event) {
            self.show_message(env.now);
            return true;
        }
        if self.words.pointer(tree, event) {
            self.count += 15;
            self.status
                .set_section("words", format!("{} words", self.count), 110.0);
            return true;
        }
        false
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if self.show.key(tree, event) {
            self.show_message(env.now);
            return true;
        }
        if self.words.key(tree, event) {
            self.count += 15;
            self.status
                .set_section("words", format!("{} words", self.count), 110.0);
            return true;
        }
        false
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        env: &Env<'_>,
    ) -> bool {
        if self.show.action(tree, node, action) {
            self.show_message(env.now);
            return true;
        }
        if self.words.action(tree, node, action) {
            self.count += 15;
            self.status
                .set_section("words", format!("{} words", self.count), 110.0);
            return true;
        }
        false
    }

    #[cfg(target_os = "macos")]
    fn tick(&mut self, _tree: &mut Tree, env: &Env<'_>) -> bool {
        let before = self.status.text().to_owned();
        self.status.poll(env.now);
        before != self.status.text()
    }
}

/// A miniature main window: toolbars on chosen edges, an optional status
/// bar, and the content area that remains.
pub struct MainWindowPage {
    toggles: Vec<(Push, &'static str)>,
    window: NodeId,
    top: bool,
    left: bool,
    status: bool,
}

pub fn main_window(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let toggles = [
        ("Top toolbar", "top"),
        ("Side toolbar", "left"),
        ("Status bar", "status"),
    ]
    .into_iter()
    .map(|(label, id)| (Push::new(tree, parent, label, env), id))
    .collect();
    let window = node(tree, parent, false);
    Box::new(MainWindowPage {
        toggles,
        window,
        top: true,
        left: true,
        status: true,
    })
}

impl MainWindowPage {
    fn toggle(&mut self, id: &str) {
        match id {
            "top" => self.top = !self.top,
            "left" => self.left = !self.left,
            _ => self.status = !self.status,
        }
    }

    fn on(&self, id: &str) -> bool {
        match id {
            "top" => self.top,
            "left" => self.left,
            _ => self.status,
        }
    }
}

impl Page for MainWindowPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(30.0, &[120.0, 120.0, 110.0]);
        for ((push, _), cell) in self.toggles.iter().zip(cells) {
            let _ = tree.set_bounds(push.node, cell);
        }
        let mut rect = column.rest();
        rect.size.width = rect.size.width.min(640.0);
        rect.size.height = rect.size.height.min(380.0);
        let _ = tree.set_bounds(self.window, rect);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (push, id) in &self.toggles {
            let rect = tree.bounds(push.node).unwrap_or_default();
            let variant = if self.on(id) {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Outline
            };
            Button::new(push.label.clone(), CommandId(0))
                .with_variant(variant)
                .paint(&mut env.cx(list, rect), tree, push.node);
        }
        let window = tree.bounds(self.window).unwrap_or_default();
        env.panel(list, window);
        let side = if env.rtl() {
            ToolbarArea::Right
        } else {
            ToolbarArea::Left
        };
        let mut toolbars = Vec::new();
        if self.top {
            toolbars.push((ToolbarArea::Top, 36.0));
        }
        if self.left {
            toolbars.push((side, 44.0));
        }
        let frames = compose_main_window(window, &toolbars, self.status.then_some(22.0));
        for (rect, (area, _)) in frames.toolbars.iter().zip(&toolbars) {
            let label = if *area == ToolbarArea::Top {
                "Toolbar"
            } else {
                "Tools"
            };
            let mut toolbar = Toolbar::new(label, vec![]);
            toolbar.move_to(*area);
            let layout = toolbar.layout(*rect, env.direction);
            toolbar.paint(
                &mut env.cx(list, *rect),
                &layout,
                &CommandRegistry::new(),
                false,
            );
            env.caption(list, *rect, label);
        }
        if let Some(rect) = frames.status_bar {
            let mut status = StatusBar::new();
            status.set_default_text("Status bar");
            let layout = status.layout(rect, env.direction);
            status.paint(&mut env.cx(list, rect), &layout);
        }
        env.panel(list, frames.content.inset(6.0));
        env.text(
            list,
            frames.content,
            &format!(
                "Content {:.0} × {:.0}",
                frames.content.size.width, frames.content.size.height
            ),
            env.body(),
            env.tokens.color(ColorRole::Foreground).with_alpha(0.7),
            Align::Center,
        );
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes: Vec<_> = self
            .toggles
            .iter()
            .map(|(push, id)| {
                let (node, mut semantic) = push.semantics(tree);
                semantic.state.checked = Some(self.on(id));
                (node, semantic)
            })
            .collect();
        nodes.push((
            self.window,
            sem(SemanticRole::Group, "Window preview", None),
        ));
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        match self
            .toggles
            .iter_mut()
            .find_map(|(push, id)| push.pointer(tree, event).then_some(*id))
        {
            Some(id) => {
                self.toggle(id);
                true
            }
            None => false,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        match self
            .toggles
            .iter_mut()
            .find_map(|(push, id)| push.key(tree, event).then_some(*id))
        {
            Some(id) => {
                self.toggle(id);
                true
            }
            None => false,
        }
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        match self
            .toggles
            .iter_mut()
            .find_map(|(push, id)| push.action(tree, node, action).then_some(*id))
        {
            Some(id) => {
                self.toggle(id);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::tests::Harness;

    fn placement(h: &mut Harness) -> (usize, usize) {
        let env = crate::pages::tests::env(&h.tokens, &h.locale, true, &h.state, &h.cache);
        let layout = h
            .page
            .as_any_mut()
            .downcast_mut::<ToolbarPage>()
            .unwrap()
            .layout_bar(&env);
        (layout.items.len(), layout.overflow.len())
    }

    #[test]
    fn narrow_toolbar_moves_commands_into_overflow() {
        let mut h = Harness::new("toolbar");
        let (placed, overflow) = placement(&mut h);
        let narrow = h.node("Narrow window", None);
        h.act(narrow, SemanticAction::Activate);
        let (narrow_placed, narrow_overflow) = placement(&mut h);
        assert!(narrow_placed < placed, "{placed} -> {narrow_placed}");
        assert!(
            narrow_overflow > overflow,
            "{overflow} -> {narrow_overflow}"
        );
    }

    #[test]
    fn status_bar_message_expires() {
        let mut h = Harness::new("status-bar");
        let page = h.get::<StatusBarPage>();
        let shown = page.show_message(Duration::from_secs(1));
        assert!(shown.contains("Saved"), "{shown}");
        page.status.poll(Duration::from_secs(2));
        assert_eq!(page.status.text(), shown);
        page.status.poll(Duration::from_secs(5));
        assert!(!page.status.text().contains("Saved"));
    }
}
