//! A small harness that mounts controls in a `ControlHost`, lays them
//! out, and drives them with pointer, key, and assistive input.

#![allow(
    dead_code,
    reason = "each test file uses a different part of the harness"
)]

use choscorui_controls::{
    ControlHost, ControlMessage, CustomControl, EstimatedText, HostEnv, ShortcutModifiers, Styles,
};
use choscorui_core::{
    BackendCapabilities, Constraints, DesignTokens, DisplayCommand, DisplayList, InputModality,
    Key, KeyEvent, LayoutDirection, NodeId, Point, PointerButton, PointerDetails, PointerEvent,
    Rect, SemanticAction, SemanticNode, SemanticParts, ServiceBackend, ServiceError, Services,
    Size, Support, ThemeResolver, Tree,
};

use std::cell::RefCell;
use std::rc::Rc;

pub struct Harness {
    pub tree: Tree,
    pub host: ControlHost,
    pub themes: ThemeResolver,
    pub styles: Styles,
    pub direction: LayoutDirection,
    pub node: NodeId,
    /// Host time in seconds for input and paint.
    pub time: f64,
}

pub fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(w, h))
}

impl Harness {
    /// Mounts `control` at its natural size at (10, 10).
    pub fn new(control: impl CustomControl + 'static) -> Self {
        Self::with_direction(control, LayoutDirection::Ltr)
    }

    pub fn with_direction(
        control: impl CustomControl + 'static,
        direction: LayoutDirection,
    ) -> Self {
        Self::mount(
            Box::new(control),
            ThemeResolver::new(DesignTokens::light()),
            Styles::new(),
            direction,
        )
    }

    /// Mounts `control` with `themes` and `styles` at its natural size.
    pub fn mount(
        control: Box<dyn CustomControl>,
        themes: ThemeResolver,
        styles: Styles,
        direction: LayoutDirection,
    ) -> Self {
        Self::mount_with(|_| control, themes, styles, direction)
    }

    /// Mounts the control that `build` makes in the harness tree, so that
    /// it can refer to nodes that it attaches first.
    pub fn mount_with(
        build: impl FnOnce(&mut Tree) -> Box<dyn CustomControl>,
        themes: ThemeResolver,
        styles: Styles,
        direction: LayoutDirection,
    ) -> Self {
        let mut tree = Tree::new(rect(0.0, 0.0, 800.0, 600.0));
        let mut host = ControlHost::new();
        let root = tree.root();
        let control = build(&mut tree);
        let node = host.mount(&mut tree, root, control).unwrap();
        let mut harness = Self {
            tree,
            host,
            themes,
            styles,
            direction,
            node,
            time: 0.0,
        };
        let size = harness.measure();
        harness.place(rect(10.0, 10.0, size.width, size.height));
        harness
    }

    pub fn env(&self) -> HostEnv<'_> {
        HostEnv::new(&self.themes, &self.styles, &EstimatedText)
            .direction(self.direction)
            .time(self.time)
    }

    /// Sets the modifier keys that later events carry.
    pub fn modifiers(&mut self, modifiers: ShortcutModifiers) {
        self.host.set_modifiers(modifiers);
    }

    /// Sets the time that input and paint see.
    pub fn set_time(&mut self, time: f64) {
        self.time = time;
        self.host.set_time(time);
    }

    pub fn measure(&mut self) -> Size {
        self.measure_in(Constraints::UNBOUNDED)
    }

    pub fn measure_in(&mut self, constraints: Constraints) -> Size {
        let env =
            HostEnv::new(&self.themes, &self.styles, &EstimatedText).direction(self.direction);
        self.host
            .measure(&self.tree, self.node, constraints, &env)
            .unwrap()
            .size
    }

    /// The unbounded baseline that the control reports.
    pub fn baseline(&mut self) -> Option<f64> {
        let env =
            HostEnv::new(&self.themes, &self.styles, &EstimatedText).direction(self.direction);
        let unbounded = Constraints::UNBOUNDED;
        self.host
            .measure(&self.tree, self.node, unbounded, &env)
            .unwrap()
            .baseline
    }

    /// Sets the control's bounds and arranges it.
    pub fn place(&mut self, bounds: Rect) {
        self.tree.set_bounds(self.node, bounds).unwrap();
        let env =
            HostEnv::new(&self.themes, &self.styles, &EstimatedText).direction(self.direction);
        self.host.arrange(&self.tree, &env);
    }

    pub fn bounds(&self) -> Rect {
        self.tree.bounds(self.node).unwrap()
    }

    pub fn click(&mut self, at: Point) {
        self.press_count(at, 1);
    }

    /// A press and a release at `at` that count as click `count` of a
    /// multiple click.
    pub fn press_count(&mut self, at: Point, count: u32) {
        let details = PointerDetails {
            click_count: count,
            ..PointerDetails::default()
        };
        self.host.dispatch_pointer_with(
            &mut self.tree,
            PointerEvent::Down {
                position: at,
                button: PointerButton::Primary,
            },
            details,
        );
        self.host.dispatch_pointer_with(
            &mut self.tree,
            PointerEvent::Up {
                position: at,
                button: PointerButton::Primary,
            },
            details,
        );
    }

    /// The presses of a double click at `at`.
    pub fn double_click(&mut self, at: Point) {
        self.press_count(at, 1);
        self.press_count(at, 2);
    }

    /// The presses of a triple click at `at`.
    pub fn triple_click(&mut self, at: Point) {
        self.double_click(at);
        self.press_count(at, 3);
    }

    pub fn hover(&mut self, at: Point) {
        self.host
            .dispatch_pointer(&mut self.tree, PointerEvent::Move { position: at });
    }

    /// Focuses the control as keyboard navigation would.
    pub fn focus(&mut self) {
        self.tree.set_input_modality(InputModality::Keyboard);
        self.host.move_focus(&mut self.tree, Some(self.node));
    }

    pub fn key(&mut self, key: Key) {
        self.host
            .dispatch_key(&mut self.tree, KeyEvent::Down { key, repeat: false });
        self.host.dispatch_key(&mut self.tree, KeyEvent::Up { key });
    }

    pub fn text(&mut self, text: &str) -> bool {
        self.host.dispatch_text(&mut self.tree, text)
    }

    /// Presses `key` with `modifiers`, then releases the modifiers.
    pub fn chord(&mut self, modifiers: ShortcutModifiers, key: Key) {
        self.host.set_modifiers(modifiers);
        self.key(key);
        self.host.set_modifiers(ShortcutModifiers::default());
    }

    /// Gives the host `clipboard`, as a backend with clipboard support
    /// does.
    pub fn clipboard(&mut self, clipboard: &Rc<FakeClipboard>) {
        let backend: Rc<dyn ServiceBackend> = clipboard.clone();
        self.host.set_services(Services::new(backend));
    }

    pub fn perform(&mut self, action: SemanticAction, value: Option<&str>) -> bool {
        self.host.perform(&mut self.tree, self.node, action, value)
    }

    pub fn semantics(&self) -> SemanticNode {
        self.host.get(self.node).unwrap().semantics()
    }

    /// The parts that the control publishes as child nodes.
    pub fn parts(&self) -> SemanticParts {
        self.host.parts(self.node)
    }

    /// The name of the part that has the focus, or an empty string.
    pub fn focused_part(&self) -> String {
        self.focused_node()
            .map(|node| node.name)
            .unwrap_or_default()
    }

    /// The node of the part that has the focus.
    pub fn focused_node(&self) -> Option<SemanticNode> {
        let parts = self.parts();
        parts
            .focused()
            .map(|position| parts.parts()[position].node.clone())
    }

    /// The name of the first selected part, or an empty string.
    pub fn selected_part(&self) -> String {
        self.parts()
            .parts()
            .iter()
            .find(|part| part.node.state.selected == Some(true))
            .map(|part| part.node.name.clone())
            .unwrap_or_default()
    }

    /// Runs `action` on the part with `key`.
    pub fn perform_part(&mut self, key: u64, action: SemanticAction) -> bool {
        self.host
            .perform_part(&mut self.tree, self.node, key, action)
    }

    pub fn messages(&mut self) -> Vec<ControlMessage> {
        self.host
            .take_messages()
            .into_iter()
            .map(|(_, message)| message)
            .collect()
    }

    pub fn paint(&self) -> DisplayList {
        let mut list = DisplayList::new();
        self.host.paint(&self.tree, &mut list, &self.env());
        list
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.host
            .set_enabled(&mut self.tree, self.node, enabled)
            .unwrap();
    }
}

/// A clipboard in memory, as the system clipboard of a backend.
#[derive(Default)]
pub struct FakeClipboard {
    pub text: RefCell<Option<String>>,
}

impl FakeClipboard {
    pub fn with(text: &str) -> Rc<Self> {
        Rc::new(Self {
            text: RefCell::new(Some(text.to_owned())),
        })
    }

    pub fn get(&self) -> Option<String> {
        self.text.borrow().clone()
    }
}

impl ServiceBackend for FakeClipboard {
    fn capabilities(&self) -> BackendCapabilities {
        let mut capabilities = BackendCapabilities::none("test");
        capabilities.clipboard = Support::Supported;
        capabilities
    }

    fn clipboard_text(&self) -> Result<Option<String>, ServiceError> {
        Ok(self.get())
    }

    fn set_clipboard_text(&self, text: &str) -> Result<(), ServiceError> {
        *self.text.borrow_mut() = Some(text.to_owned());
        Ok(())
    }
}

/// The text runs of `list`, in paint order.
pub fn texts(list: &DisplayList) -> Vec<String> {
    list.commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// True when `list` fills a shape in `color`.
pub fn fills(list: &DisplayList, color: choscorui_core::Color) -> bool {
    list.commands().iter().any(|command| match command {
        DisplayCommand::FillRect { color: c, .. }
        | DisplayCommand::FillRoundedRect { color: c, .. } => *c == color,
        DisplayCommand::FillPath {
            paint: choscorui_core::Paint::Solid(c),
            ..
        } => *c == color,
        _ => false,
    })
}

/// The rounded rectangles `list` fills in `color`.
pub fn rounded_fills(list: &DisplayList, color: choscorui_core::Color) -> Vec<Rect> {
    list.commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::FillRoundedRect { rect, color: c, .. } if *c == color => Some(*rect),
            _ => None,
        })
        .collect()
}

/// True when any command of `list` paints in `color`.
pub fn paints(list: &DisplayList, color: choscorui_core::Color) -> bool {
    format!("{:?}", list.commands()).contains(&format!("{color:?}"))
}

/// True when `list` draws `icon` at any place and size: a run of paths
/// that are the paths of the icon after one move and one scale.
pub fn draws_icon(list: &DisplayList, icon: choscorui_core::Icon) -> bool {
    use choscorui_core::{Path, PathElement, Transform};
    fn path(command: &DisplayCommand) -> Option<&Path> {
        match command {
            DisplayCommand::FillPath { path, .. } | DisplayCommand::StrokePath { path, .. } => {
                Some(path)
            }
            _ => None,
        }
    }
    fn near(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }
    fn same(a: &Path, b: &Path) -> bool {
        a.elements().len() == b.elements().len()
            && a.elements()
                .iter()
                .zip(b.elements())
                .all(|pair| match pair {
                    (PathElement::MoveTo(a), PathElement::MoveTo(b))
                    | (PathElement::LineTo(a), PathElement::LineTo(b)) => near(*a, *b),
                    (PathElement::QuadTo(a, c), PathElement::QuadTo(b, d)) => {
                        near(*a, *b) && near(*c, *d)
                    }
                    (PathElement::CubicTo(a, c, e), PathElement::CubicTo(b, d, f)) => {
                        near(*a, *b) && near(*c, *d) && near(*e, *f)
                    }
                    (PathElement::Close, PathElement::Close) => true,
                    _ => false,
                })
    }
    let mut probe = DisplayList::new();
    let unit = Rect::new(Point::new(0.0, 0.0), Size::new(24.0, 24.0));
    if icon
        .draw(&mut probe, unit, choscorui_core::Color::BLACK)
        .is_err()
    {
        return false;
    }
    let shapes: Vec<&Path> = probe.commands().iter().filter_map(path).collect();
    let drawn: Vec<&Path> = list.commands().iter().filter_map(path).collect();
    if shapes.is_empty() {
        return false;
    }
    drawn.windows(shapes.len()).any(|run| {
        let (from, to) = (shapes[0].bounds(), run[0].bounds());
        let scale = if from.size.width > 0.0 {
            to.size.width / from.size.width
        } else {
            to.size.height / from.size.height
        };
        let place = Transform::scale(scale, scale).then(Transform::translate(
            to.origin.x - from.origin.x * scale,
            to.origin.y - from.origin.y * scale,
        ));
        run.iter()
            .zip(&shapes)
            .all(|(drawn, shape)| same(&shape.transformed(place), drawn))
    })
}
