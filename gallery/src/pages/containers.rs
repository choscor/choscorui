//! Containers: group boxes, splitters, stacks, scroll areas, tool boxes,
//! docking, size grips, and focus frames.

use crate::page::{Align, Column, Env, Page, PageState, Push, node, pressed, sem, split};
use choscorui::controls::{
    DockArea, DockLayout, DockPanel, DockTarget, FocusFrame, GroupBox, PanelId, ScrollArea,
    ScrollAreaInput, ShortcutModifiers, SizeGrip, SplitOrientation, Splitter, SplitterInput,
    StackPage, StackedContainer, ToolBox,
};
use choscorui::core::{
    ColorRole, DisplayList, Key, KeyEvent, NodeId, Paint, Path, Point, PointerButton, PointerEvent,
    Rect, ScrollViewport, SemanticAction, SemanticNode, SemanticRole, Size, StrokeStyle, Tree,
};

// ---------------------------------------------------------------------------
// Group box

/// Two titled groups of related buttons.
pub struct GroupBoxPage {
    groups: Vec<(NodeId, NodeId, GroupBox, &'static str, Vec<Push>)>,
}

pub fn group_box(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let mut groups = Vec::new();
    for (title, members) in [
        ("Shipping", ["Standard", "Express", "Pick up"]),
        ("Notifications", ["Email", "SMS", "Push"]),
    ] {
        let container = node(tree, parent, false);
        let title_node = node(tree, container, false);
        let pushes: Vec<Push> = members
            .iter()
            .map(|label| Push::new(tree, container, *label, env))
            .collect();
        let group = GroupBox::new(
            title,
            container,
            Some(title_node),
            pushes.iter().map(|push| push.node).collect(),
        )
        .expect("groups have members");
        group.install(tree).expect("members are descendants");
        groups.push((container, title_node, group, title, pushes));
    }
    Box::new(GroupBoxPage { groups })
}

impl Page for GroupBoxPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (container, title, _, _, pushes) in &self.groups {
            let rect = column.cells(110.0, &[400.0])[0];
            let _ = tree.set_bounds(*container, rect);
            let _ = tree.set_bounds(
                *title,
                Rect::new(
                    Point::new(rect.origin.x + 12.0, rect.origin.y),
                    Size::new(rect.size.width - 24.0, 20.0),
                ),
            );
            let row = Rect::new(
                Point::new(rect.origin.x + 16.0, rect.origin.y + 44.0),
                Size::new(rect.size.width - 32.0, 30.0),
            );
            let widths = vec![110.0; pushes.len()];
            for (push, cell) in pushes.iter().zip(split(row, &widths, 8.0, env.rtl())) {
                let _ = tree.set_bounds(push.node, cell);
            }
            column.space(8.0);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (container, title, _, label, pushes) in &self.groups {
            let rect = tree.bounds(*container).unwrap_or_default();
            let frame = Rect::new(
                Point::new(rect.origin.x, rect.origin.y + 10.0),
                Size::new(rect.size.width, rect.size.height - 10.0),
            );
            let _ = list.stroke_rounded_rect(frame, env.tokens.radius.min(8.0), env.mix(0.3), 1.0);
            let title_rect = tree.bounds(*title).unwrap_or_default();
            let width = env.measure(label, env.small()) + 12.0;
            let x = if env.rtl() {
                title_rect.max_x() - width
            } else {
                title_rect.origin.x
            };
            let chip = Rect::new(Point::new(x, title_rect.origin.y), Size::new(width, 20.0));
            list.fill_rect(chip, env.tokens.color(ColorRole::Background));
            env.text(
                list,
                chip,
                label,
                env.small(),
                env.tokens.color(ColorRole::Foreground),
                Align::Center,
            );
            for push in pushes {
                push.paint(tree, list, env);
            }
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = Vec::new();
        for (container, title, group, label, pushes) in &self.groups {
            nodes.push((*container, group.semantics()));
            nodes.push((*title, sem(SemanticRole::StaticText, *label, None)));
            nodes.extend(pushes.iter().map(|push| push.semantics(tree)));
        }
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let mut changed = false;
        for (_, _, _, _, pushes) in &mut self.groups {
            for push in pushes {
                changed |= push.pointer(tree, event);
            }
        }
        changed
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let mut changed = false;
        for (_, _, _, _, pushes) in &mut self.groups {
            for push in pushes {
                changed |= push.key(tree, event);
            }
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
        self.groups
            .iter_mut()
            .flat_map(|(_, _, _, _, pushes)| pushes.iter_mut())
            .any(|push| push.action(tree, node, action))
    }
}

// ---------------------------------------------------------------------------
// Splitter

const HANDLE: f64 = 6.0;

struct Split {
    label: &'static str,
    orientation: SplitOrientation,
    first: NodeId,
    handle: NodeId,
    second: NodeId,
    splitter: Option<Splitter>,
    /// Position as a fraction, kept across relayouts.
    fraction: f64,
    dragging: bool,
}

impl Split {
    fn new(
        tree: &mut Tree,
        parent: NodeId,
        label: &'static str,
        orientation: SplitOrientation,
    ) -> Self {
        Self {
            label,
            orientation,
            first: node(tree, parent, false),
            handle: node(tree, parent, true),
            second: node(tree, parent, false),
            splitter: None,
            fraction: 0.4,
            dragging: false,
        }
    }

    fn primary(&self, bounds: Rect) -> f64 {
        match self.orientation {
            SplitOrientation::Horizontal => bounds.size.width,
            SplitOrientation::Vertical => bounds.size.height,
        }
    }

    fn arrange(&mut self, tree: &mut Tree, bounds: Rect) {
        let primary = self.primary(bounds);
        let minimum = 60.0;
        let position =
            (primary * self.fraction).clamp(minimum, (primary - HANDLE - minimum).max(minimum));
        self.splitter = Splitter::new(
            self.label,
            self.orientation,
            self.first,
            self.handle,
            self.second,
            bounds,
            minimum,
            minimum,
            HANDLE,
            position,
        )
        .ok();
        if let Some(splitter) = &self.splitter {
            let _ = splitter.install(tree);
        }
    }

    fn bounds(&self, tree: &Tree) -> Rect {
        let first = tree.bounds(self.first).unwrap_or_default();
        let second = tree.bounds(self.second).unwrap_or_default();
        Rect::new(
            first.origin,
            Size::new(
                second.max_x() - first.origin.x,
                second.max_y() - first.origin.y,
            ),
        )
    }

    fn moved(&mut self, tree: &Tree) {
        if let Some(splitter) = &self.splitter {
            self.fraction = splitter.position() / self.primary(self.bounds(tree)).max(1.0);
        }
    }

    fn pointer(&mut self, tree: &mut Tree, event: PointerEvent) -> bool {
        let bounds = self.bounds(tree);
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } if tree
                .bounds(self.handle)
                .is_some_and(|b| b.inset(-3.0).contains(position)) =>
            {
                self.dragging = true;
                let _ = tree.set_focus(Some(self.handle));
                let _ = tree.set_pointer_capture(Some(self.handle));
                true
            }
            PointerEvent::Move { position } if self.dragging => {
                let along = match self.orientation {
                    SplitOrientation::Horizontal => position.x - bounds.origin.x,
                    SplitOrientation::Vertical => position.y - bounds.origin.y,
                } - HANDLE / 2.0;
                let primary = self.primary(bounds);
                let target = along.clamp(60.0, (primary - HANDLE - 60.0).max(60.0));
                let changed = self
                    .splitter
                    .as_mut()
                    .is_some_and(|splitter| splitter.set_position(tree, target).unwrap_or(false));
                self.moved(tree);
                changed
            }
            PointerEvent::Up { .. } if self.dragging => {
                self.dragging = false;
                let _ = tree.set_pointer_capture(None);
                true
            }
            _ => false,
        }
    }

    fn input(&mut self, tree: &mut Tree, input: SplitterInput) -> bool {
        let changed = self
            .splitter
            .as_mut()
            .is_some_and(|splitter| matches!(splitter.handle(tree, input), Ok(Some(_))));
        self.moved(tree);
        changed
    }
}

/// A horizontal splitter whose trailing pane holds a vertical one.
pub struct SplitterPage {
    outer: Split,
    inner: Split,
}

pub fn splitter(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let outer = Split::new(tree, parent, "Sidebar width", SplitOrientation::Horizontal);
    let inner = Split::new(tree, parent, "Console height", SplitOrientation::Vertical);
    Box::new(SplitterPage { outer, inner })
}

impl SplitterPage {
    fn splits(&mut self) -> [&mut Split; 2] {
        [&mut self.outer, &mut self.inner]
    }

    fn relayout_inner(&mut self, tree: &mut Tree) {
        let second = tree.bounds(self.outer.second).unwrap_or_default();
        self.inner.arrange(tree, second);
    }
}

impl Page for SplitterPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, _env: &Env<'_>) {
        let rect = Rect::new(
            bounds.origin,
            Size::new(bounds.size.width.min(700.0), bounds.size.height.min(420.0)),
        );
        self.outer.arrange(tree, rect);
        self.relayout_inner(tree);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (id, label) in [
            (self.outer.first, "Sidebar"),
            (self.inner.first, "Editor"),
            (self.inner.second, "Console"),
        ] {
            let rect = tree.bounds(id).unwrap_or_default();
            env.panel(list, rect);
            env.text(
                list,
                rect,
                label,
                env.body(),
                env.tokens.color(ColorRole::Foreground).with_alpha(0.7),
                Align::Center,
            );
        }
        for split in [&self.outer, &self.inner] {
            if let Some(splitter) = &split.splitter {
                env.splitter_style().paint(
                    list,
                    tree.bounds(split.handle).unwrap_or_default(),
                    splitter.visual_state(tree),
                );
            }
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = vec![
            (self.outer.first, sem(SemanticRole::Group, "Sidebar", None)),
            (self.inner.first, sem(SemanticRole::Group, "Editor", None)),
            (self.inner.second, sem(SemanticRole::Group, "Console", None)),
        ];
        for split in [&self.outer, &self.inner] {
            if let Some(splitter) = &split.splitter {
                nodes.push((split.handle, splitter.semantics(tree)));
            }
        }
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if self.outer.pointer(tree, event) {
            self.relayout_inner(tree);
            return true;
        }
        self.inner.pointer(tree, event)
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        // Arrow keys follow reading order on the horizontal splitter.
        let event = match (event, env.rtl()) {
            (
                KeyEvent::Down {
                    key: Key::ArrowLeft,
                    repeat,
                },
                true,
            ) => KeyEvent::Down {
                key: Key::ArrowRight,
                repeat,
            },
            (
                KeyEvent::Down {
                    key: Key::ArrowRight,
                    repeat,
                },
                true,
            ) => KeyEvent::Down {
                key: Key::ArrowLeft,
                repeat,
            },
            (event, _) => event,
        };
        if self.outer.input(tree, SplitterInput::Key(event)) {
            self.relayout_inner(tree);
            return true;
        }
        self.inner.input(tree, SplitterInput::Key(event))
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let Some(index) = self.splits().iter().position(|split| split.handle == node) else {
            return false;
        };
        let changed = self.splits()[index].input(tree, SplitterInput::Semantic(action));
        if index == 0 {
            self.relayout_inner(tree);
        }
        changed || action == SemanticAction::Focus
    }
}

// ---------------------------------------------------------------------------
// Stacked container

const STACK_PAGES: [(&str, &str); 3] = [
    ("General", "Name, language, and startup behavior."),
    ("Network", "Proxies, timeouts, and offline mode."),
    ("Advanced", "Logging and experimental features."),
];

/// Three panels with a selector; only the current one is visible, and
/// each remembers which of its controls had focus.
pub struct StackedPage {
    selectors: Vec<Push>,
    container: NodeId,
    panels: Vec<(NodeId, Push)>,
    stack: StackedContainer,
}

pub fn stacked(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let selectors = STACK_PAGES
        .iter()
        .map(|(title, _)| Push::new(tree, parent, *title, env))
        .collect();
    let container = node(tree, parent, false);
    let panels: Vec<(NodeId, Push)> = STACK_PAGES
        .iter()
        .map(|(title, _)| {
            let panel = node(tree, container, false);
            let push = Push::new(tree, panel, format!("Reset {title}"), env);
            (panel, push)
        })
        .collect();
    let stack = StackedContainer::new(
        "Settings pages",
        container,
        panels
            .iter()
            .map(|(panel, push)| StackPage::new(*panel, Some(push.node)))
            .collect(),
        panels[0].0,
    )
    .expect("stack has pages");
    let _ = stack.install(tree);
    Box::new(StackedPage {
        selectors,
        container,
        panels,
        stack,
    })
}

impl StackedPage {
    fn select(&mut self, tree: &mut Tree, index: usize) -> bool {
        self.stack
            .select(tree, self.panels[index].0)
            .unwrap_or(false)
    }

    fn current(&self) -> usize {
        self.panels
            .iter()
            .position(|(panel, _)| *panel == self.stack.current())
            .unwrap_or(0)
    }
}

impl Page for StackedPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(30.0, &[110.0, 110.0, 110.0]);
        for (push, cell) in self.selectors.iter().zip(cells) {
            let _ = tree.set_bounds(push.node, cell);
        }
        let rect = column.cells(160.0, &[420.0])[0];
        let _ = tree.set_bounds(self.container, rect);
        for (panel, push) in &self.panels {
            let _ = tree.set_bounds(*panel, rect);
            let _ = tree.set_bounds(
                push.node,
                split(
                    Rect::new(
                        Point::new(rect.origin.x + 16.0, rect.max_y() - 46.0),
                        Size::new(rect.size.width - 32.0, 30.0),
                    ),
                    &[160.0],
                    0.0,
                    env.rtl(),
                )[0],
            );
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let current = self.current();
        for (index, push) in self.selectors.iter().enumerate() {
            push.paint(tree, list, env);
            if index == current {
                let rect = tree.bounds(push.node).unwrap_or_default();
                list.fill_rect(
                    Rect::new(
                        Point::new(rect.origin.x + 8.0, rect.max_y() + 2.0),
                        Size::new(rect.size.width - 16.0, 2.0),
                    ),
                    env.tokens.color(ColorRole::Primary),
                );
            }
        }
        let (panel, push) = &self.panels[current];
        let rect = tree.bounds(*panel).unwrap_or_default();
        env.panel(list, rect);
        let (title, body) = STACK_PAGES[current];
        let inner = rect.inset(16.0);
        env.label(
            list,
            Rect::new(inner.origin, Size::new(inner.size.width, 24.0)),
            title,
        );
        env.caption(
            list,
            Rect::new(
                Point::new(inner.origin.x, inner.origin.y + 28.0),
                Size::new(inner.size.width, 20.0),
            ),
            body,
        );
        push.paint(tree, list, env);
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let current = self.current();
        let mut nodes: Vec<_> = self
            .selectors
            .iter()
            .enumerate()
            .map(|(index, push)| {
                let (id, mut node) = push.semantics(tree);
                node.state.selected = Some(index == current);
                (id, node)
            })
            .collect();
        nodes.push((self.container, self.stack.semantics()));
        for (panel, push) in &self.panels {
            if let Some(mut node) = self.stack.page_semantics(*panel) {
                node.state.hidden = tree.visible(*panel) == Some(false);
                if node.name.is_empty() {
                    node.name = STACK_PAGES[self
                        .panels
                        .iter()
                        .position(|(p, _)| p == panel)
                        .unwrap_or(0)]
                    .0
                    .into();
                }
                nodes.push((*panel, node));
            }
            let (id, mut node) = push.semantics(tree);
            node.state.hidden = tree.visible(*panel) == Some(false);
            nodes.push((id, node));
        }
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if let Some(index) = self
            .selectors
            .iter_mut()
            .position(|push| push.pointer(tree, event))
        {
            return self.select(tree, index);
        }
        let current = self.current();
        self.panels[current].1.pointer(tree, event)
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if let Some(index) = self
            .selectors
            .iter_mut()
            .position(|push| push.key(tree, event))
        {
            return self.select(tree, index);
        }
        let current = self.current();
        self.panels[current].1.key(tree, event)
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        if let Some(index) = self
            .selectors
            .iter_mut()
            .position(|push| push.action(tree, node, action))
        {
            return self.select(tree, index);
        }
        self.panels
            .iter_mut()
            .any(|(_, push)| push.action(tree, node, action))
    }
}

// ---------------------------------------------------------------------------
// Scroll area

const TILE: f64 = 90.0;
const TILES: usize = 14;

/// A 14 × 14 grid of tiles in a scroll area with overlay scroll bars.
pub struct ScrollAreaPage {
    area: ScrollArea,
}

pub fn scroll_area(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let viewport = node(tree, parent, true);
    let content = node(tree, viewport, false);
    let side = TILE * TILES as f64;
    let mut area = ScrollArea::new(
        ScrollViewport::new(
            viewport,
            content,
            Size::new(side, side),
            Size::new(40.0, 40.0),
        )
        .expect("valid content size"),
    );
    let _ = area.set_enabled(tree, env.enabled);
    Box::new(ScrollAreaPage { area })
}

impl ScrollAreaPage {
    pub fn offset(&self) -> Point {
        self.area.viewport().offset()
    }
}

impl Page for ScrollAreaPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, _env: &Env<'_>) {
        let rect = Rect::new(
            bounds.origin,
            Size::new(bounds.size.width.min(560.0), bounds.size.height.min(380.0)),
        );
        let _ = tree.set_bounds(self.area.node(), rect);
        let _ = self.area.viewport_mut().arrange(tree, rect);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let viewport = tree.bounds(self.area.node()).unwrap_or_default();
        let content = tree
            .bounds(self.area.viewport().content())
            .unwrap_or_default();
        env.panel(list, viewport);
        list.push_clip(viewport);
        for row in 0..TILES {
            for column in 0..TILES {
                let tile = Rect::new(
                    Point::new(
                        content.origin.x + column as f64 * TILE,
                        content.origin.y + row as f64 * TILE,
                    ),
                    Size::new(TILE, TILE),
                )
                .inset(6.0);
                if tile.max_x() < viewport.origin.x
                    || tile.origin.x > viewport.max_x()
                    || tile.max_y() < viewport.origin.y
                    || tile.origin.y > viewport.max_y()
                {
                    continue;
                }
                let shade = ((row + column) % 5) as f64 * 0.05 + 0.08;
                let _ = list.fill_rounded_rect(tile, 6.0, env.mix(shade));
                env.text(
                    list,
                    tile,
                    &format!("{}{}", (b'A' + row as u8) as char, column + 1),
                    env.small(),
                    env.tokens.color(ColorRole::Foreground),
                    Align::Center,
                );
            }
        }
        let _ = list.pop_clip();
        self.area.paint(tree, list, &env.scroll_style());
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let offset = self.offset();
        let mut node = self.area.semantics(tree, "Tiles");
        if node.value.is_none() {
            node.value = Some(format!("{:.0}, {:.0}", offset.x, offset.y));
        }
        vec![(self.area.node(), node)]
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let inside = tree
            .bounds(self.area.node())
            .is_some_and(|b| b.contains(event.position()));
        if inside && matches!(event, PointerEvent::Down { .. }) {
            let _ = tree.set_focus(Some(self.area.node()));
        }
        self.area
            .handle(tree, ScrollAreaInput::Pointer(event))
            .unwrap_or(false)
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        tree.focus() == Some(self.area.node())
            && pressed(event) != Some(Key::Tab)
            && self
                .area
                .handle(tree, ScrollAreaInput::Key(event))
                .unwrap_or(false)
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        node == self.area.node()
            && self
                .area
                .handle(tree, ScrollAreaInput::Semantic(action))
                .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Tool box

const TOOL_PAGES: [(&str, &str); 3] = [
    (
        "Brushes",
        "Round, flat, and fan brushes with pressure sensitivity.",
    ),
    (
        "Layers",
        "Background, Sketch, and Ink layers; drag to reorder.",
    ),
    (
        "History",
        "The last 50 strokes; choose one to step back to it.",
    ),
];

/// An accordion: Up and Down move between headers, Space opens one.
pub struct ToolBoxPage {
    container: NodeId,
    headers: Vec<NodeId>,
    page: NodeId,
    toolbox: ToolBox,
}

pub fn tool_box(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let container = node(tree, parent, true);
    let headers = TOOL_PAGES
        .iter()
        .map(|_| node(tree, container, false))
        .collect();
    let page = node(tree, container, false);
    Box::new(ToolBoxPage {
        container,
        headers,
        page,
        toolbox: ToolBox::new(
            TOOL_PAGES
                .iter()
                .map(|(title, _)| (*title).to_owned())
                .collect(),
        ),
    })
}

impl ToolBoxPage {
    fn arrange(&self, tree: &mut Tree) {
        let bounds = tree.bounds(self.container).unwrap_or_default();
        let (headers, page) = self.toolbox.layout(bounds, 30.0);
        for (id, rect) in self.headers.iter().zip(headers) {
            let _ = tree.set_bounds(*id, rect);
        }
        let _ = tree.set_bounds(self.page, page);
    }
}

impl Page for ToolBoxPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let rect = split(
            Rect::new(
                bounds.origin,
                Size::new(bounds.size.width, 260.0_f64.min(bounds.size.height)),
            ),
            &[320.0],
            0.0,
            env.rtl(),
        )[0];
        let _ = tree.set_bounds(self.container, rect);
        self.arrange(tree);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let focused = tree.focus() == Some(self.container);
        for (index, id) in self.headers.iter().enumerate() {
            let rect = tree.bounds(*id).unwrap_or_default();
            let open = index == self.toolbox.current();
            list.fill_rect(rect.inset(1.0), env.mix(if open { 0.16 } else { 0.08 }));
            let marker = if open {
                "▾"
            } else if env.rtl() {
                "◂"
            } else {
                "▸"
            };
            env.text(
                list,
                rect.inset(10.0),
                &format!("{marker}  {}", TOOL_PAGES[index].0),
                env.body(),
                env.tokens.color(ColorRole::Foreground),
                Align::Leading,
            );
            if focused && index == self.toolbox.focused() {
                env.focus_ring(list, rect.inset(2.0));
            }
        }
        let page = tree.bounds(self.page).unwrap_or_default();
        env.panel(list, page);
        env.caption(
            list,
            Rect::new(page.origin, Size::new(page.size.width, 40.0)).inset(10.0),
            TOOL_PAGES[self.toolbox.current()].1,
        );
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut container = sem(
            SemanticRole::Group,
            "Tool box",
            Some(TOOL_PAGES[self.toolbox.current()].0.into()),
        );
        container.actions = vec![SemanticAction::Focus];
        let mut nodes = vec![(self.container, container)];
        nodes.extend(
            self.headers
                .iter()
                .copied()
                .zip(self.toolbox.header_semantics()),
        );
        nodes.push((
            self.page,
            sem(
                SemanticRole::StaticText,
                TOOL_PAGES[self.toolbox.current()].1,
                None,
            ),
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
        let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = event
        else {
            return false;
        };
        let Some(index) = self
            .headers
            .iter()
            .position(|id| tree.bounds(*id).is_some_and(|b| b.contains(position)))
        else {
            return false;
        };
        let _ = tree.set_focus(Some(self.container));
        self.toolbox.open(index);
        self.arrange(tree);
        true
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if tree.focus() != Some(self.container) {
            return false;
        }
        let Some(key) = pressed(event) else {
            return false;
        };
        if key == Key::Tab {
            return false;
        }
        self.toolbox.handle_key(key);
        self.arrange(tree);
        true
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        match self.headers.iter().position(|id| *id == node) {
            Some(index) if matches!(action, SemanticAction::Activate | SemanticAction::Expand) => {
                let opened = self.toolbox.open(index);
                self.arrange(tree);
                opened
            }
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Dock

const FILES: PanelId = PanelId(1);
const OUTLINE: PanelId = PanelId(2);
const INSPECTOR: PanelId = PanelId(3);
const CONSOLE: PanelId = PanelId(4);
const PANELS: [PanelId; 4] = [FILES, OUTLINE, INSPECTOR, CONSOLE];
const AREAS: [DockArea; 4] = [
    DockArea::Left,
    DockArea::Right,
    DockArea::Top,
    DockArea::Bottom,
];

fn default_dock() -> DockLayout {
    let mut dock = DockLayout::new(24.0);
    let tab = |area| DockTarget::Tab { area, group: 0 };
    for (id, key, title, target) in [
        (FILES, "files", "Files", tab(DockArea::Left)),
        (OUTLINE, "outline", "Outline", tab(DockArea::Left)),
        (INSPECTOR, "inspector", "Inspector", tab(DockArea::Right)),
        (CONSOLE, "console", "Console", tab(DockArea::Bottom)),
    ] {
        let _ = dock.add(id, DockPanel::new(key, title), target);
    }
    dock.set_extent(DockArea::Left, 180.0);
    dock.set_extent(DockArea::Right, 170.0);
    dock.set_extent(DockArea::Bottom, 110.0);
    dock
}

/// Four dockable panels around an editor. Tabs activate; the buttons move
/// and close panels; splitters resize areas from the keyboard. The
/// arrangement persists.
pub struct DockPage {
    area: NodeId,
    tabs: Vec<(PanelId, NodeId)>,
    splitters: Vec<(DockArea, NodeId)>,
    move_console: Push,
    close_outline: Push,
    reset: Push,
    dock: DockLayout,
    note: Option<String>,
}

pub fn dock(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let move_console = Push::new(tree, parent, "Move Console", env);
    let close_outline = Push::new(tree, parent, "Toggle Outline", env);
    let reset = Push::new(tree, parent, "Reset layout", env);
    let area = node(tree, parent, false);
    let tabs = PANELS
        .iter()
        .map(|id| (*id, node(tree, area, true)))
        .collect();
    let splitters = AREAS.iter().map(|a| (*a, node(tree, area, true))).collect();
    let mut dock = default_dock();
    let mut note = None;
    if !env.state.dock.is_empty() {
        match dock.restore(&env.state.dock) {
            Ok(report) if !report.unknown_keys.is_empty() => {
                note = Some(format!(
                    "Skipped unknown panels: {}",
                    report.unknown_keys.join(", ")
                ));
            }
            Ok(_) => {}
            Err(error) => {
                dock = default_dock();
                note = Some(format!("Saved layout ignored ({error}); using the default"));
            }
        }
    }
    Box::new(DockPage {
        area,
        tabs,
        splitters,
        move_console,
        close_outline,
        reset,
        dock,
        note,
    })
}

impl DockPage {
    /// The saved arrangement, as persisted in settings.
    #[cfg(target_os = "macos")]
    pub fn save(&self) -> String {
        self.dock.save()
    }

    /// Moves the console between the bottom and right areas.
    pub fn move_console(&mut self, tree: &mut Tree) -> bool {
        let to_right = matches!(
            self.dock.location(CONSOLE),
            Some(choscorui::controls::PanelLocation::Docked {
                area: DockArea::Bottom,
                ..
            })
        );
        let area = if to_right {
            DockArea::Right
        } else {
            DockArea::Bottom
        };
        let _ = self.dock.show(CONSOLE);
        let moved = self
            .dock
            .dock(CONSOLE, DockTarget::Tab { area, group: 0 })
            .is_ok();
        self.arrange(tree);
        moved
    }

    fn toggle_outline(&mut self, tree: &mut Tree) -> bool {
        let changed = if self.dock.is_visible(OUTLINE) {
            self.dock.close(OUTLINE).is_ok()
        } else {
            self.dock.show(OUTLINE).is_ok()
        };
        self.arrange(tree);
        changed
    }

    fn arrange(&self, tree: &mut Tree) {
        let bounds = tree.bounds(self.area).unwrap_or_default();
        let arrangement = self.dock.arrange(bounds);
        for (panel, id) in &self.tabs {
            let placed = arrangement.groups.iter().find_map(|group| {
                let index = group.tabs.iter().position(|tab| tab == panel)?;
                let width = group.tab_bar.size.width / group.tabs.len().max(1) as f64;
                Some(Rect::new(
                    Point::new(
                        group.tab_bar.origin.x + index as f64 * width,
                        group.tab_bar.origin.y,
                    ),
                    Size::new(width, group.tab_bar.size.height),
                ))
            });
            let _ = tree.set_visible(*id, placed.is_some());
            let _ = tree.set_bounds(*id, placed.unwrap_or_default());
        }
        for (area, id) in &self.splitters {
            let placed = arrangement
                .splitters
                .iter()
                .find(|(a, _)| a == area)
                .map(|(_, rect)| *rect);
            let _ = tree.set_visible(*id, placed.is_some());
            let _ = tree.set_bounds(*id, placed.unwrap_or_default());
        }
    }

    fn run(&mut self, tree: &mut Tree, button: usize) -> bool {
        self.note = None;
        match button {
            0 => self.move_console(tree),
            1 => self.toggle_outline(tree),
            _ => {
                self.dock = default_dock();
                self.arrange(tree);
                true
            }
        }
    }

    fn buttons(&mut self) -> [&mut Push; 3] {
        [
            &mut self.move_console,
            &mut self.close_outline,
            &mut self.reset,
        ]
    }
}

impl Page for DockPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(30.0, &[120.0, 130.0, 120.0]);
        for (push, cell) in self.buttons().into_iter().zip(cells) {
            let _ = tree.set_bounds(push.node, cell);
        }
        let mut rest = column.rest();
        rest.size.width = rest.size.width.min(760.0);
        rest.size.height = rest.size.height.min(440.0);
        let _ = tree.set_bounds(self.area, rest);
        self.arrange(tree);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        self.move_console.paint(tree, list, env);
        self.close_outline.paint(tree, list, env);
        self.reset.paint(tree, list, env);
        let bounds = tree.bounds(self.area).unwrap_or_default();
        let arrangement = self.dock.arrange(bounds);
        env.panel(list, arrangement.center);
        env.text(
            list,
            arrangement.center,
            "Editor",
            env.body(),
            env.tokens.color(ColorRole::Foreground).with_alpha(0.5),
            Align::Center,
        );
        for group in &arrangement.groups {
            list.fill_rect(group.tab_bar, env.mix(0.1));
            let _ = list.stroke_rect(group.rect, env.mix(0.2), 1.0);
            for (panel, id) in &self.tabs {
                if !group.tabs.contains(panel) {
                    continue;
                }
                let rect = tree.bounds(*id).unwrap_or_default();
                if *panel == group.active {
                    list.fill_rect(rect, env.mix(0.22));
                }
                let title = self
                    .dock
                    .panel(*panel)
                    .map(|p| p.title.as_str())
                    .unwrap_or("");
                env.text(
                    list,
                    rect,
                    title,
                    env.small(),
                    env.tokens.color(ColorRole::Foreground),
                    Align::Center,
                );
                if tree.focus() == Some(*id) {
                    env.focus_ring(list, rect.inset(2.0));
                }
            }
            let title = self
                .dock
                .panel(group.active)
                .map(|p| p.title.as_str())
                .unwrap_or("");
            env.caption(list, group.content.inset(8.0), &format!("{title} panel"));
        }
        for (area, rect) in &arrangement.splitters {
            let focused = self
                .splitters
                .iter()
                .any(|(a, id)| a == area && tree.focus() == Some(*id));
            list.fill_rect(
                *rect,
                if focused {
                    env.tokens.color(ColorRole::Primary)
                } else {
                    env.mix(0.25)
                },
            );
        }
        if let Some(note) = &self.note {
            let below = Rect::new(
                Point::new(bounds.origin.x, bounds.max_y() + 6.0),
                Size::new(bounds.size.width, 18.0),
            );
            env.caption(list, below, note);
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = vec![
            self.move_console.semantics(tree),
            self.close_outline.semantics(tree),
            self.reset.semantics(tree),
            (self.area, sem(SemanticRole::Group, "Dock area", None)),
        ];
        for (panel, id) in &self.tabs {
            let mut node = self
                .dock
                .panel_semantics(*panel)
                .unwrap_or_else(|| sem(SemanticRole::Tab, format!("Panel {}", panel.0), None));
            if node.name.is_empty() {
                node.name = self
                    .dock
                    .panel(*panel)
                    .map(|p| p.title.clone())
                    .unwrap_or_default();
            }
            node.state.hidden = tree.visible(*id) == Some(false);
            nodes.push((*id, node));
        }
        for (area, id) in &self.splitters {
            let mut node = self.dock.splitter_semantics(*area);
            node.state.hidden = tree.visible(*id) == Some(false);
            nodes.push((*id, node));
        }
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let hit = self
            .buttons()
            .into_iter()
            .position(|push| push.pointer(tree, event));
        if let Some(index) = hit {
            return self.run(tree, index);
        }
        let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = event
        else {
            return false;
        };
        let visible = |id: NodeId| {
            tree.visible(id) == Some(true) && tree.bounds(id).is_some_and(|b| b.contains(position))
        };
        if let Some((panel, id)) = self.tabs.iter().copied().find(|(_, id)| visible(*id)) {
            let _ = tree.set_focus(Some(id));
            let activated = self.dock.activate(panel).is_ok();
            self.arrange(tree);
            return activated;
        }
        if let Some((_, id)) = self.splitters.iter().copied().find(|(_, id)| visible(*id)) {
            return tree.set_focus(Some(id)).is_ok();
        }
        false
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let hit = self
            .buttons()
            .into_iter()
            .position(|push| push.key(tree, event));
        if let Some(index) = hit {
            return self.run(tree, index);
        }
        let Some(key) = pressed(event) else {
            return false;
        };
        let focus = tree.focus();
        if let Some((area, _)) = self.splitters.iter().find(|(_, id)| Some(*id) == focus) {
            let changed = self.dock.handle_splitter_key(*area, key);
            self.arrange(tree);
            return changed;
        }
        if let Some((panel, _)) = self.tabs.iter().find(|(_, id)| Some(*id) == focus)
            && matches!(key, Key::Space | Key::Enter)
        {
            let activated = self.dock.activate(*panel).is_ok();
            self.arrange(tree);
            return activated;
        }
        false
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let hit = self
            .buttons()
            .into_iter()
            .position(|push| push.action(tree, node, action));
        if let Some(index) = hit {
            return self.run(tree, index);
        }
        if let Some((panel, _)) = self.tabs.iter().find(|(_, id)| *id == node) {
            let activated =
                action == SemanticAction::Activate && self.dock.activate(*panel).is_ok();
            self.arrange(tree);
            return activated;
        }
        if let Some((area, _)) = self.splitters.iter().find(|(_, id)| *id == node) {
            let key = match action {
                SemanticAction::Increment => Key::ArrowRight,
                SemanticAction::Decrement => Key::ArrowLeft,
                _ => return false,
            };
            let key = match (area, key) {
                (DockArea::Top | DockArea::Bottom, Key::ArrowRight) => Key::ArrowDown,
                (DockArea::Top | DockArea::Bottom, _) => Key::ArrowUp,
                (_, key) => key,
            };
            let changed = self.dock.handle_splitter_key(*area, key);
            self.arrange(tree);
            return changed;
        }
        false
    }

    fn persist(&self, state: &mut PageState) {
        state.dock = self.dock.save();
    }
}

// ---------------------------------------------------------------------------
// Size grip

/// A mock window resized by its grip (bottom-trailing; bottom-left in
/// right-to-left layouts), clamped to a minimum and maximum size.
pub struct SizeGripPage {
    window: NodeId,
    grip_node: NodeId,
    grip: SizeGrip,
    size: Size,
    origin: Point,
}

pub fn size_grip(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let window = node(tree, parent, false);
    let grip_node = node(tree, window, true);
    Box::new(SizeGripPage {
        window,
        grip_node,
        grip: SizeGrip::new(Size::new(220.0, 140.0), Size::new(620.0, 400.0), env.rtl()),
        size: Size::new(360.0, 220.0),
        origin: Point::new(0.0, 0.0),
    })
}

impl SizeGripPage {
    fn place(&self, tree: &mut Tree) {
        // In right-to-left layouts the window's trailing (right) edge
        // stays put and it grows leftward, like a mirrored window.
        let x = if self.grip.rtl {
            self.origin.x + self.grip.maximum.width - self.size.width
        } else {
            self.origin.x
        };
        let window = Rect::new(Point::new(x, self.origin.y), self.size);
        let _ = tree.set_bounds(self.window, window);
        let local = self.grip.rect(self.size, 16.0);
        let _ = tree.set_bounds(
            self.grip_node,
            Rect::new(
                Point::new(
                    window.origin.x + local.origin.x,
                    window.origin.y + local.origin.y,
                ),
                local.size,
            ),
        );
    }

    fn resize(&mut self, tree: &mut Tree, size: Size) -> bool {
        let clamped = Size::new(
            size.width
                .clamp(self.grip.minimum.width, self.grip.maximum.width),
            size.height
                .clamp(self.grip.minimum.height, self.grip.maximum.height),
        );
        let changed = clamped != self.size;
        self.size = clamped;
        self.place(tree);
        changed
    }
}

impl Page for SizeGripPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, _env: &Env<'_>) {
        self.origin = bounds.origin;
        self.place(tree);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let window = tree.bounds(self.window).unwrap_or_default();
        env.panel(list, window);
        let _ = list.stroke_rounded_rect(window, env.tokens.radius.min(8.0), env.mix(0.3), 1.0);
        env.text(
            list,
            window,
            &format!("{:.0} × {:.0}", self.size.width, self.size.height),
            env.body(),
            env.tokens.color(ColorRole::Foreground),
            Align::Center,
        );
        let grip = tree.bounds(self.grip_node).unwrap_or_default();
        for step in 1..=3 {
            let inset = step as f64 * 4.0;
            let (from, to) = if self.grip.rtl {
                (
                    Point::new(grip.origin.x + inset, grip.max_y() - 2.0),
                    Point::new(grip.origin.x + 2.0, grip.max_y() - inset),
                )
            } else {
                (
                    Point::new(grip.max_x() - inset, grip.max_y() - 2.0),
                    Point::new(grip.max_x() - 2.0, grip.max_y() - inset),
                )
            };
            let _ = list.stroke_path(
                Path::line(from, to),
                Paint::Solid(env.mix(0.5)),
                StrokeStyle::new(1.0),
            );
        }
        if tree.focus() == Some(self.grip_node) {
            env.focus_ring(list, grip);
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut grip = sem(
            SemanticRole::Button,
            "Resize",
            Some(format!("{:.0} by {:.0}", self.size.width, self.size.height)),
        );
        grip.description = "Arrow keys resize by 10 points".into();
        grip.actions = vec![
            SemanticAction::Increment,
            SemanticAction::Decrement,
            SemanticAction::Focus,
        ];
        vec![
            (self.window, sem(SemanticRole::Group, "Mock window", None)),
            (self.grip_node, grip),
        ]
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } if tree
                .bounds(self.grip_node)
                .is_some_and(|b| b.contains(position)) =>
            {
                let _ = tree.set_focus(Some(self.grip_node));
                let _ = tree.set_pointer_capture(Some(self.grip_node));
                self.grip.press(position, self.size);
                true
            }
            PointerEvent::Move { position } => match self.grip.drag(position) {
                Some(size) => self.resize(tree, size),
                None => false,
            },
            PointerEvent::Up { .. } => {
                self.grip.release();
                let _ = tree.set_pointer_capture(None);
                false
            }
            _ => false,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if tree.focus() != Some(self.grip_node) {
            return false;
        }
        let grow = if self.grip.rtl { -10.0 } else { 10.0 };
        let (dx, dy) = match pressed(event) {
            Some(Key::ArrowRight) => (grow, 0.0),
            Some(Key::ArrowLeft) => (-grow, 0.0),
            Some(Key::ArrowDown) => (0.0, 10.0),
            Some(Key::ArrowUp) => (0.0, -10.0),
            _ => return false,
        };
        let size = Size::new(self.size.width + dx, self.size.height + dy);
        self.resize(tree, size);
        true
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        if node != self.grip_node {
            return false;
        }
        let delta = match action {
            SemanticAction::Increment => 10.0,
            SemanticAction::Decrement => -10.0,
            SemanticAction::Focus => return tree.set_focus(Some(node)).is_ok(),
            _ => return false,
        };
        let size = Size::new(self.size.width + delta, self.size.height + delta);
        self.resize(tree, size)
    }
}

// ---------------------------------------------------------------------------
// Focus frame

/// Buttons inside a clipping panel. The focus frame draws outside the
/// focused control's bounds, so focus stays visible even at the clip edge.
pub struct FocusFramePage {
    clip: NodeId,
    pushes: Vec<Push>,
}

pub fn focus_frame(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let clip = node(tree, parent, false);
    let pushes = ["First", "Second", "Third"]
        .iter()
        .map(|label| Push::new(tree, clip, *label, env))
        .collect();
    Box::new(FocusFramePage { clip, pushes })
}

impl Page for FocusFramePage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let rect = column.cells(60.0, &[380.0])[0];
        let _ = tree.set_bounds(self.clip, rect);
        // Buttons sit flush with the panel edges, where a ring drawn by
        // the control itself would be clipped.
        let row = Rect::new(rect.origin, Size::new(rect.size.width, rect.size.height));
        for (push, cell) in
            self.pushes
                .iter()
                .zip(split(row, &[120.0, 120.0, 120.0], 10.0, env.rtl()))
        {
            let _ = tree.set_bounds(push.node, cell);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let rect = tree.bounds(self.clip).unwrap_or_default();
        env.panel(list, rect);
        list.push_clip(rect);
        for push in &self.pushes {
            push.paint(tree, list, env);
        }
        let _ = list.pop_clip();
        if self
            .pushes
            .iter()
            .any(|push| tree.focus() == Some(push.node))
        {
            FocusFrame {
                color: env.tokens.color(ColorRole::Primary),
                width: 3.0,
                outset: 4.0,
                radius: env.tokens.radius.min(8.0) + 4.0,
            }
            .paint(list, tree);
        }
        let below = Rect::new(
            Point::new(rect.origin.x, rect.max_y() + 12.0),
            Size::new(rect.size.width + 120.0, 20.0),
        );
        env.caption(
            list,
            below,
            "Press Tab to move focus; the frame is drawn outside the clip.",
        );
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = vec![(self.clip, sem(SemanticRole::Group, "Clipped panel", None))];
        nodes.extend(self.pushes.iter().map(|push| push.semantics(tree)));
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let mut changed = false;
        for push in &mut self.pushes {
            changed |= push.pointer(tree, event);
        }
        changed
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
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
}
