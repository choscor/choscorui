//! Hover tracking, input-device capabilities, and drag-and-drop contracts.

use crate::{NodeId, Point, Tree};
use std::collections::BTreeSet;

/// Enter/leave notifications produced by [`HoverTracker::update`]. `left` is
/// ordered deepest first and `entered` outermost first, matching the order
/// in which a node's ancestors contain it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HoverChange {
    pub left: Vec<NodeId>,
    pub entered: Vec<NodeId>,
}

impl HoverChange {
    pub fn is_empty(&self) -> bool {
        self.left.is_empty() && self.entered.is_empty()
    }
}

/// Tracks the hovered path. While a node captures the pointer, hover is
/// frozen on the captured path so no sibling receives enter during a drag.
#[derive(Clone, Debug, Default)]
pub struct HoverTracker {
    path: Vec<NodeId>,
}

impl HoverTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hovered nodes from root to deepest.
    pub fn path(&self) -> &[NodeId] {
        &self.path
    }

    pub fn is_hovered(&self, id: NodeId) -> bool {
        self.path.contains(&id)
    }

    pub fn update(&mut self, tree: &Tree, position: Option<Point>) -> HoverChange {
        // Drop detached nodes without reporting leave for them.
        self.path.retain(|id| tree.contains(*id));
        if tree.pointer_capture().is_some() {
            return HoverChange::default();
        }
        let target = position.and_then(|position| tree.hit_test(position));
        let mut next = Vec::new();
        let mut current = target;
        while let Some(id) = current {
            next.push(id);
            current = tree.parent(id);
        }
        next.reverse();
        let shared = self
            .path
            .iter()
            .zip(&next)
            .take_while(|(a, b)| a == b)
            .count();
        let change = HoverChange {
            left: self.path[shared..].iter().rev().copied().collect(),
            entered: next[shared..].to_vec(),
        };
        self.path = next;
        change
    }
}

/// The kind of device that produced pointer input.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PointerKind {
    Mouse,
    Trackpad,
    Touch,
    Pen,
    Eraser,
}

/// Extra per-event data from precision devices. Fields a device cannot
/// report keep their defaults, so controls never require them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerDetails {
    pub kind: PointerKind,
    /// Normalized 0..=1; 1 for devices without pressure.
    pub pressure: f64,
    /// Pen tilt in -1..=1 per axis; zero when unknown.
    pub tilt: Point,
    /// Click count within the system double-click interval.
    pub click_count: u32,
}

impl Default for PointerDetails {
    fn default() -> Self {
        Self {
            kind: PointerKind::Mouse,
            pressure: 1.0,
            tilt: Point::new(0.0, 0.0),
            click_count: 1,
        }
    }
}

/// Trackpad gestures. Controls without gesture support may ignore them;
/// each has a keyboard or pointer equivalent (zoom commands, wheel input).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GestureEvent {
    /// Multiplicative scale change around `position`, e.g. 1.1 zooms in.
    Magnify { position: Point, scale: f64 },
    /// Clockwise rotation in radians.
    Rotate { position: Point, radians: f64 },
    /// Smart-zoom double tap.
    SmartMagnify { position: Point },
    /// Inertial phase ended; momentum scrolling stops.
    ScrollEnded,
}

/// Input support a backend has verified. Applications query it instead of
/// assuming a device exists; unsupported features fall back as documented.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InputCapabilities {
    pub precise_scrolling: bool,
    pub momentum_scrolling: bool,
    pub gestures: bool,
    pub pen_pressure: bool,
    pub touch: bool,
    pub drag_and_drop: BTreeSet<DragFormat>,
    pub ime: bool,
}

/// Payload formats that cross application boundaries.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DragFormat {
    Text,
    /// File URLs (`public.file-url` on macOS).
    Files,
    /// PNG or TIFF image data.
    Image,
    Url,
    /// Application-private data, never offered to other processes.
    Custom(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum DragItem {
    Text(String),
    Files(Vec<std::path::PathBuf>),
    /// Encoded PNG bytes.
    Image(Vec<u8>),
    Url(String),
    Custom {
        format: String,
        bytes: Vec<u8>,
    },
}

impl DragItem {
    pub fn format(&self) -> DragFormat {
        match self {
            Self::Text(_) => DragFormat::Text,
            Self::Files(_) => DragFormat::Files,
            Self::Image(_) => DragFormat::Image,
            Self::Url(_) => DragFormat::Url,
            Self::Custom { format, .. } => DragFormat::Custom(format.clone()),
        }
    }
}

/// Copy, move, and link negotiation. The source offers a set; the target
/// picks one; modifier keys express the user's preference.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DragOperations {
    pub copy: bool,
    pub move_: bool,
    pub link: bool,
}

impl DragOperations {
    pub const NONE: Self = Self {
        copy: false,
        move_: false,
        link: false,
    };
    pub const ALL: Self = Self {
        copy: true,
        move_: true,
        link: true,
    };

    pub fn contains(self, op: DragOperation) -> bool {
        match op {
            DragOperation::None => true,
            DragOperation::Copy => self.copy,
            DragOperation::Move => self.move_,
            DragOperation::Link => self.link,
        }
    }

    pub fn is_empty(self) -> bool {
        !(self.copy || self.move_ || self.link)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DragOperation {
    None,
    Copy,
    Move,
    Link,
}

/// User-preferred operation from modifiers, constrained to `allowed`.
/// macOS: Option = copy, Command+Option = link, default = move within an
/// application or copy across applications.
pub fn negotiate_drag(
    allowed: DragOperations,
    option: bool,
    command: bool,
    same_application: bool,
) -> DragOperation {
    let preferred = match (option, command) {
        (true, true) => DragOperation::Link,
        (true, false) => DragOperation::Copy,
        (false, true) => DragOperation::Move,
        (false, false) if same_application => DragOperation::Move,
        (false, false) => DragOperation::Copy,
    };
    if preferred != DragOperation::None && allowed.contains(preferred) {
        return preferred;
    }
    [
        DragOperation::Move,
        DragOperation::Copy,
        DragOperation::Link,
    ]
    .into_iter()
    .find(|op| allowed.contains(*op))
    .unwrap_or(DragOperation::None)
}

/// Drag events delivered to a drop target. `Leave` also means cancel.
#[derive(Clone, Debug, PartialEq)]
pub enum DragEvent {
    Enter {
        position: Point,
        formats: Vec<DragFormat>,
        allowed: DragOperations,
    },
    Over {
        position: Point,
        allowed: DragOperations,
    },
    Leave,
    Drop {
        position: Point,
        items: Vec<DragItem>,
        operation: DragOperation,
    },
}

/// Drop-target routing: tracks which node accepted the current drag and
/// delivers a final `Leave` if that node is detached mid-drag.
#[derive(Clone, Debug, Default)]
pub struct DropRouter {
    target: Option<NodeId>,
    formats: Vec<DragFormat>,
}

impl DropRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn target(&self) -> Option<NodeId> {
        self.target
    }

    /// Returns (node, event) pairs to deliver for a native drag event.
    pub fn route(&mut self, tree: &Tree, event: DragEvent) -> Vec<(NodeId, DragEvent)> {
        let mut out = Vec::new();
        if self.target.is_some_and(|id| !tree.contains(id)) {
            self.target = None;
        }
        if let DragEvent::Enter { formats, .. } = &event {
            self.formats = formats.clone();
        }
        match event {
            DragEvent::Enter { position, .. } | DragEvent::Over { position, .. } => {
                let hit = tree.hit_test(position);
                if hit != self.target {
                    if let Some(old) = self.target {
                        out.push((old, DragEvent::Leave));
                    }
                    self.target = hit;
                    if let Some(new) = hit {
                        let (DragEvent::Enter { allowed, .. } | DragEvent::Over { allowed, .. }) =
                            event
                        else {
                            unreachable!("matched above");
                        };
                        out.push((
                            new,
                            DragEvent::Enter {
                                position,
                                formats: self.formats.clone(),
                                allowed,
                            },
                        ));
                        return out;
                    }
                }
                if let Some(target) = self.target {
                    out.push((target, event));
                }
            }
            DragEvent::Leave => {
                self.formats.clear();
                if let Some(old) = self.target.take() {
                    out.push((old, DragEvent::Leave));
                }
            }
            DragEvent::Drop { .. } => {
                self.formats.clear();
                if let Some(target) = self.target.take() {
                    out.push((target, event));
                }
            }
        }
        out
    }
}
