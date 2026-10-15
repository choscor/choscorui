//! The state that the method handler reads: the application, its
//! windows, and the accessible nodes of each window.

use super::tree::{Node, WindowTree};
use crate::native::{Surface, WindowShared};
use choscorui_core::{NodeId, Point, Rect, SemanticAction};
use std::collections::BTreeMap;
use std::rc::Rc;

/// The prefix of every object path of the bridge.
pub const PATH_PREFIX: &str = "/org/a11y/atspi/accessible/";
/// The application object, which the registry embeds.
pub const ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";
/// The path of a reference to no object.
pub const NULL_PATH: &str = "/org/a11y/atspi/null";
/// The cache object that AT-SPI clients ask first.
pub const CACHE_PATH: &str = "/org/a11y/atspi/cache";

/// An action that an assistive technology asked for on a node. The
/// application routes it into the same control input as a key or a
/// pointer event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessibilityActionRequest {
    pub node: NodeId,
    /// The key of the part of `node` that the action is for, or `None`
    /// for the node itself.
    pub part: Option<u64>,
    pub action: SemanticAction,
    /// The new value for `SemanticAction::SetValue`, as text.
    pub value: Option<String>,
}

/// One object of the bridge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Root,
    Window(u32),
    Node(u32, u64),
    Cache,
}

impl Target {
    /// The object of `path`, if the path has the form of one.
    pub fn parse(path: &str) -> Option<Self> {
        if path == CACHE_PATH {
            return Some(Self::Cache);
        }
        let rest = path.strip_prefix(PATH_PREFIX)?;
        if rest == "root" {
            return Some(Self::Root);
        }
        let rest = rest.strip_prefix('w')?;
        match rest.split_once('n') {
            Some((window, node)) => Some(Self::Node(window.parse().ok()?, node.parse().ok()?)),
            None => Some(Self::Window(rest.parse().ok()?)),
        }
    }

    pub fn path(self) -> String {
        match self {
            Self::Root => ROOT_PATH.to_owned(),
            Self::Window(window) => format!("{PATH_PREFIX}w{window}"),
            Self::Node(window, node) => format!("{PATH_PREFIX}w{window}n{node}"),
            Self::Cache => CACHE_PATH.to_owned(),
        }
    }
}

/// The accessible side of one native window.
pub struct WindowModel {
    pub tree: WindowTree,
    shared: Rc<WindowShared>,
    surface: Rc<dyn Surface>,
    pub actions: Vec<AccessibilityActionRequest>,
    /// The title of the last update, for its change event.
    pub title: String,
}

impl WindowModel {
    pub fn new(shared: Rc<WindowShared>, surface: Rc<dyn Surface>) -> Self {
        let title = shared.title.borrow().clone();
        Self {
            tree: WindowTree::default(),
            shared,
            surface,
            actions: Vec::new(),
            title,
        }
    }

    pub fn current_title(&self) -> String {
        self.shared.title.borrow().clone()
    }

    /// The content of the window in window coordinates.
    pub fn bounds(&self) -> Rect {
        Rect::new(Point::new(0.0, 0.0), self.shared.size.get())
    }

    pub fn active(&self) -> bool {
        self.shared.focused.get() && !self.shared.closed.get()
    }

    pub fn visible(&self) -> bool {
        self.shared.visible.get() && !self.shared.closed.get()
    }

    /// The screen position of the content. Wayland does not tell it, so
    /// it is zero there.
    pub fn origin(&self) -> Point {
        self.surface
            .content_origin()
            .unwrap_or(Point::new(0.0, 0.0))
    }

    /// Queues `action` on `node` when the node declares it. Returns true
    /// when it queued.
    pub fn request(&mut self, node: u64, action: SemanticAction, value: Option<String>) -> bool {
        let Some(node) = self.tree.nodes.get(&node) else {
            return false;
        };
        if !node.entry.node.actions.contains(&action) {
            return false;
        }
        self.actions.push(AccessibilityActionRequest {
            node: node.entry.id,
            part: node.entry.part,
            action,
            value,
        });
        true
    }

    pub fn node(&self, node: u64) -> Option<&Node> {
        self.tree.nodes.get(&node)
    }
}

/// The application and its windows.
pub struct Model {
    pub app_name: String,
    /// The id that the registry gives the application.
    pub app_id: i32,
    /// The registry object that embeds the application: a bus name and a
    /// path.
    pub parent: Option<(String, String)>,
    pub windows: BTreeMap<u32, WindowModel>,
    next_window: u32,
}

impl Model {
    pub fn new(app_name: String) -> Self {
        Self {
            app_name,
            app_id: 0,
            parent: None,
            windows: BTreeMap::new(),
            next_window: 0,
        }
    }

    /// Adds a window and returns its number.
    pub fn add_window(&mut self, window: WindowModel) -> u32 {
        self.next_window += 1;
        self.windows.insert(self.next_window, window);
        self.next_window
    }

    /// The index of window `id` among the children of the application.
    pub fn window_index(&self, id: u32) -> Option<usize> {
        self.windows.keys().position(|&key| key == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_round_trip() {
        for target in [
            Target::Root,
            Target::Window(3),
            Target::Node(3, 41),
            Target::Cache,
        ] {
            assert_eq!(Target::parse(&target.path()), Some(target));
        }
        assert_eq!(Target::parse("/org/a11y/atspi/accessible/wx"), None);
        assert_eq!(Target::parse("/org/example"), None);
    }
}
