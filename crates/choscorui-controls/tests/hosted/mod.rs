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
    Constraints, DesignTokens, DisplayCommand, DisplayList, InputModality, Key, KeyEvent,
    LayoutDirection, NodeId, Point, PointerButton, PointerEvent, Rect, SemanticAction,
    SemanticNode, Size, ThemeResolver, Tree,
};

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
        let mut tree = Tree::new(rect(0.0, 0.0, 800.0, 600.0));
        let mut host = ControlHost::new();
        let root = tree.root();
        let node = host.mount(&mut tree, root, Box::new(control)).unwrap();
        let mut harness = Self {
            tree,
            host,
            themes: ThemeResolver::new(DesignTokens::light()),
            styles: Styles::new(),
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
        let env =
            HostEnv::new(&self.themes, &self.styles, &EstimatedText).direction(self.direction);
        self.host
            .measure(&self.tree, self.node, Constraints::UNBOUNDED, &env)
            .unwrap()
            .size
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
        self.host.dispatch_pointer(
            &mut self.tree,
            PointerEvent::Down {
                position: at,
                button: PointerButton::Primary,
            },
        );
        self.host.dispatch_pointer(
            &mut self.tree,
            PointerEvent::Up {
                position: at,
                button: PointerButton::Primary,
            },
        );
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

    pub fn perform(&mut self, action: SemanticAction, value: Option<&str>) -> bool {
        self.host.perform(&mut self.tree, self.node, action, value)
    }

    pub fn semantics(&self) -> SemanticNode {
        self.host.get(self.node).unwrap().semantics()
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
